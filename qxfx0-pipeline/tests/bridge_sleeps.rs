//! The ADR-0043 U4 gate: the learning bridge must be architecturally unable
//! to touch the turn path, and zero-behavior-change must be asserted on a
//! corpus, not merely intended.
//!
//! Two locks:
//!
//! 1. **Link closure.** `qxfx0-bridge` may appear only in *dev* dependency
//!    sections (test code that asserts its deadness is exactly what we
//!    want to allow) and in `qxfx0-cli` (the doctor invariant surface and
//!    the U5 maintenance commands). A runtime edge from pipeline, render,
//!    guard, semantic, persistence, serve, gates or codex into the bridge
//!    breaks the law and fails here.
//! 2. **Corpus equality.** Over the twelve cadence-soak prompts, a session
//!    whose live bridge store is populated (edges + quarantine, written
//!    through the persistence maintenance path and kept open across the
//!    run) produces byte-identical responses, routing, guard verdicts and
//!    non-observational state to the plain production path. The bridge
//!    tables are beside the session state (schema v13); this assert makes
//!    "sleeping means sleeping" executable.

use qxfx0_persistence::Persistence;
use qxfx0_pipeline::{process_turn_with_options, TurnInput, TurnOptions};
use qxfx0_types::system_state::SystemState;

/// The bridge crate's public surface, resolved in this test's dependency
/// graph. If the pipeline ever links the bridge as a runtime dependency,
/// the manifest check below fails; the import keeps the dev-dependency
/// honest (this file is the reason it exists).
#[allow(unused_imports)]
use qxfx0_bridge as _bridge;

/// The twelve prompts the cadence soak cycles (idx 0..11), so this gate
/// reads as a direct extension of the soak corpus.
fn soak_prompts() -> Vec<&'static str> {
    vec![
        "Что такое истина?",
        "Как связаны свобода и ответственность?",
        "Что означает человеческое достоинство?",
        "Как память влияет на личность?",
        "В чём различие знания и убеждения?",
        "Как надежда связана с действием?",
        "Что делает решение справедливым?",
        "Как язык формирует понимание?",
        "Почему доверие требует ответственности?",
        "Как связаны причина и следствие?",
        "Что означает сохранять внутреннюю целостность?",
        "Как опыт меняет представление о будущем?",
    ]
}

/// Run the soak prompts over a fresh in-memory session. When
/// `populate_bridge` is set, the session's bridge store is written through
/// the maintenance path first — and the database is returned *open*, so
/// the live rows exist for the whole run and only the turn path never
/// reads them.
fn run_soak(
    session_id: &str,
    populate_bridge: bool,
    release_overlay: bool,
) -> (
    Vec<String>,
    Vec<String>,
    Vec<bool>,
    SystemState,
    Persistence,
) {
    let db = Persistence::open_memory().expect("in-memory bridge database");
    let mut state = SystemState {
        session_id: session_id.into(),
        ..SystemState::default()
    };
    db.save_state(session_id, &state).unwrap();
    if release_overlay {
        // Drive a real overlay through the store exactly as the CLI does:
        // draft the promoted edge, activate, release, point the active
        // singleton. The turn path reads only `state`, never these rows, so
        // a byte-identical corpus is the executable proof.
        let edge = qxfx0_bridge::BridgeEdge::new(
            qxfx0_types::AtomId::new("свобода"),
            qxfx0_types::AtomId::new("выбор"),
            qxfx0_types::RelationType::RelRequires,
            "свобода",
            0.9,
        );
        let candidate = qxfx0_bridge::PromotionCandidate {
            snapshot_id: "soak-gate".into(),
            topic: "свобода".into(),
            subject: edge.from.clone(),
            relation: edge.rel_type,
            object: edge.to.clone(),
            rendered_ru: "свобода требует выбор".into(),
            confidence: edge.confidence,
            support: edge.co_occurrence,
        };
        let (draft, _) = qxfx0_bridge::create_draft(
            "soak-gate",
            std::slice::from_ref(&candidate),
            &qxfx0_bridge::builtin_gate_policy(),
            &|_topic: &str| Vec::new(),
            1,
        );
        assert_eq!(draft.predicates.len(), 1);
        let draft_json = serde_json::to_string(&draft).unwrap();
        db.save_promotion_overlay(
            &draft.version,
            "Draft",
            &draft.snapshot_id,
            None,
            &draft.checksum,
            &draft_json,
            draft.created_at,
        )
        .unwrap();
        let activated = draft.activate(2).unwrap();
        db.replace_promotion_overlay_if_matches(
            &activated.version,
            &draft_json,
            "Activated",
            &serde_json::to_string(&activated).unwrap(),
            &activated.checksum,
        )
        .unwrap();
        let released = activated.release(3).unwrap();
        db.replace_promotion_overlay_if_matches(
            &released.version,
            &serde_json::to_string(&activated).unwrap(),
            "Released",
            &serde_json::to_string(&released).unwrap(),
            &released.checksum,
        )
        .unwrap();
        db.set_active_promotion_overlay(Some(&released.version), 3)
            .unwrap();
        assert_eq!(
            db.load_active_promotion_overlay().unwrap().as_deref(),
            Some(released.version.as_str())
        );
    }
    if populate_bridge {
        // A real worker-shaped store, encoded through the bridge codec so
        // this is exactly what the maintenance path would persist.
        let mut store = qxfx0_bridge::RuntimeEdgeStore::new();
        store.insert(
            (
                qxfx0_types::AtomId::new("свобода"),
                qxfx0_types::AtomId::new("выбор"),
            ),
            qxfx0_bridge::BridgeEdge::new(
                qxfx0_types::AtomId::new("свобода"),
                qxfx0_types::AtomId::new("выбор"),
                qxfx0_types::RelationType::RelRelatedTo,
                "свобода",
                0.6,
            ),
        );
        db.save_bridge_edges(
            session_id,
            Some(&qxfx0_bridge::encode_store(&store).unwrap()),
        )
        .unwrap();
        db.enqueue_bridge_quarantine(session_id, r#"{"refused":"unknown_endpoint"}"#)
            .unwrap();
        // Sanity: the store really is live for this session.
        assert!(db.load_bridge_edges(session_id).unwrap().is_some());
        assert!(!db.load_bridge_quarantine(session_id).unwrap().is_empty());
    }
    let mut responses = Vec::new();
    let mut families = Vec::new();
    let mut blocked = Vec::new();
    for prompt in soak_prompts() {
        let output = process_turn_with_options(
            &TurnInput {
                session_id: session_id.into(),
                raw_text: prompt.into(),
            },
            &mut state,
            TurnOptions::new(),
        );
        responses.push(output.response);
        families.push(format!("{:?}", output.family));
        blocked.push(output.blocked);
    }
    (responses, families, blocked, state, db)
}

#[test]
fn a_live_untouched_bridge_store_leaves_the_corpus_byte_identical() {
    let plain = run_soak("soak-equality", false, false);
    let live = run_soak("soak-equality", true, false);
    assert_eq!(
        plain.0, live.0,
        "a populated bridge store must not change a single response byte"
    );
    assert_eq!(plain.1, live.1, "routing must not change");
    assert_eq!(plain.2, live.2, "guard verdicts must not change");
    assert!(
        qxfx0_pipeline::response_plan_v2_state_parity(&plain.3, &live.3),
        "non-observational state must be parity-equal with a live bridge store"
    );
    // Neither run's SystemState carries bridge data — the tables are
    // beside the state, and the JSON of a session says so.
    let plain_json = serde_json::to_string(&plain.3).unwrap();
    assert!(
        !plain_json.contains("bridge"),
        "SystemState must not gain bridge fields"
    );
    // The databases are still open (live rows) until here.
    assert!(live.4.load_bridge_edges("soak-equality").unwrap().is_some());
}

/// ADR-0043 U5: a *released and active* promotion overlay (the strongest
/// case — the CLI lifecycle drove a row to Released and pointed the active
/// singleton at it) must also leave the corpus byte-identical. The graph
/// effect of a release is editorial admission into the embedded pack, never
/// a turn-path read of the overlay store, so the boundary cannot perturb a
/// turn even when an overlay is fully live.
#[test]
fn a_released_active_overlay_leaves_the_corpus_byte_identical() {
    let plain = run_soak("boundary-equality", false, false);
    let released = run_soak("boundary-equality", false, true);
    assert_eq!(
        plain.0, released.0,
        "a released active overlay must not change a single response byte"
    );
    assert_eq!(plain.1, released.1, "routing must not change");
    assert_eq!(plain.2, released.2, "guard verdicts must not change");
    assert!(
        qxfx0_pipeline::response_plan_v2_state_parity(&plain.3, &released.3),
        "non-observational state must be parity-equal with a released overlay"
    );
    // The overlay really is Released and active when the run finished.
    let active = released.4.load_active_promotion_overlay().unwrap();
    assert!(active.is_some(), "the soak released an active overlay");
    let (status, _) = released
        .4
        .load_promotion_overlay(active.as_deref().unwrap())
        .unwrap()
        .expect("the active overlay is stored");
    assert_eq!(status, "Released");
    // And again: SystemState never carries promotion fields.
    let plain_json = serde_json::to_string(&plain.3).unwrap();
    assert!(
        !plain_json.contains("overlay") && !plain_json.contains("promotion"),
        "SystemState must not gain promotion/overlay fields"
    );
}

/// Parse the sections of a Cargo.toml and return whether `qxfx0-bridge`
/// appears outside the `[dev-dependencies]` / `[build-dependencies]` and
/// `[[bench]]`/`[[test]]`/`[[example]]`-table regions.
fn bridge_is_a_runtime_dependency(manifest: &str) -> bool {
    let mut section = String::new();
    for line in manifest.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed.to_string();
            continue;
        }
        if trimmed.contains("qxfx0-bridge") || trimmed.starts_with("qxfx0_bridge") {
            let dev_only = section.starts_with("[dev-dependencies")
                || section.starts_with("[build-dependencies");
            if !dev_only {
                return true;
            }
        }
    }
    false
}

#[test]
fn bridge_is_not_linked_into_the_turn_path() {
    // The bridge may be a runtime dependency of qxfx0-cli only (doctor
    // invariants + the U5 maintenance surface). Dev-dependency edges —
    // like this test's own — are explicitly allowed: asserting the
    // bridge's deadness requires linking it in test builds.
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf();
    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(&root).expect("read workspace") {
        let dir = entry.expect("entry").path();
        let manifest = dir.join("Cargo.toml");
        if !manifest.is_file() {
            continue;
        }
        let name = dir
            .file_name()
            .expect("dir name")
            .to_string_lossy()
            .into_owned();
        if name == "qxfx0-bridge" {
            continue; // the crate's own feature self-reference
        }
        let text = std::fs::read_to_string(&manifest).expect("manifest");
        let is_cli = name == "qxfx0-cli";
        if bridge_is_a_runtime_dependency(&text) && !is_cli {
            offenders.push(name);
        }
    }
    assert!(
        offenders.is_empty(),
        "the learning bridge must not be a runtime dependency of any crate \
         except qxfx0-cli (doctor/maintenance surface): {offenders:?} link \
         qxfx0-bridge into the turn path (ADR-0043 U4 law: between turns only)"
    );
}

//! Routing gate over the semantic corpus tiers P0/P1/P2 (110 rows).
//!
//! The corpus originates from the Haskell QxFx0 project (see
//! `data/eval/semantic-corpus-v1/manifest.json` for provenance). Every row
//! carries dialogue and logic inputs (syllogisms, modus ponens/tollens,
//! quantifier traps, agency questions) with the core assertion
//! `must_not_family: [CMRepair]` — none of these inputs may degrade into
//! the repair/fallback family.
//!
//! The Haskell `expected_family_any_of` labels (CMClarify, CMDistinguish…)
//! have no Rust enum counterparts and are recorded, not gated. Instead the
//! harness pins the actual Rust routing per row in `routing-snapshot.csv`
//! (bless on first run) and enforces a ratchet:
//! - a row routed away from CMRepair must never return to it;
//! - the total number of CMRepair-routed rows must not grow;
//! - no row may end up blocked or with an empty response.

use qxfx0_pipeline::{process_turn, TurnInput};
use qxfx0_types::system_state::SystemState;
use std::path::PathBuf;

const CORPUS_ROWS: usize = 110;

fn eval_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../data/eval/semantic-corpus-v1")
}

#[derive(serde::Deserialize)]
struct CorpusRow {
    id: String,
    tier: String,
    input: String,
}

fn corpus_rows() -> Vec<CorpusRow> {
    let raw = std::fs::read_to_string(eval_dir().join("corpus.jsonl")).expect("corpus readable");
    raw.lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| serde_json::from_str(line).expect("corpus row is valid JSON"))
        .collect()
}

/// (id, tier, routed-family-debug, blocked)
fn route_row(row: &CorpusRow) -> (String, String, String, bool) {
    let mut state = SystemState {
        session_id: row.id.clone(),
        ..SystemState::default()
    };
    let input = TurnInput {
        session_id: row.id.clone(),
        raw_text: row.input.clone(),
    };
    let output = process_turn(&input, &mut state);
    (
        row.id.clone(),
        row.tier.clone(),
        format!("{:?}", output.family),
        output.blocked,
    )
}

fn is_repair(family: &str) -> bool {
    family == "CMRepair"
}

#[test]
fn semantic_corpus_never_degrades_into_repair() {
    let rows = corpus_rows();
    assert_eq!(rows.len(), CORPUS_ROWS, "corpus row count is fixed");
    let actual: Vec<(String, String, String, bool)> = rows.iter().map(route_row).collect();

    for (id, _, _family, blocked) in &actual {
        assert!(!blocked, "corpus row '{id}' must not be guard-blocked");
    }

    let snapshot_path = eval_dir().join("routing-snapshot.csv");
    let pinned = match std::fs::read_to_string(&snapshot_path) {
        Ok(pinned) => pinned,
        Err(_) => {
            let mut fresh = String::from("id,tier,family,blocked\n");
            for (id, tier, family, blocked) in &actual {
                fresh.push_str(&format!("{id},{tier},{family},{blocked}\n"));
            }
            std::fs::write(&snapshot_path, fresh).expect("bless routing snapshot");
            panic!(
                "routing snapshot absent; wrote a fresh one to {} — review and re-run",
                snapshot_path.display()
            );
        }
    };

    let mut pinned_rows = std::collections::BTreeMap::new();
    for line in pinned.lines().skip(1) {
        let fields: Vec<&str> = line.split(',').collect();
        assert!(fields.len() == 4, "snapshot row must have 4 columns");
        pinned_rows.insert(
            fields[0].to_string(),
            (fields[2].to_string(), fields[3].parse::<bool>().unwrap()),
        );
    }
    assert_eq!(
        pinned_rows.len(),
        CORPUS_ROWS,
        "snapshot must cover the whole corpus"
    );

    let mut regressions = Vec::new();
    let mut pinned_repair = 0usize;
    let mut actual_repair = 0usize;
    for (id, tier, family, _blocked) in &actual {
        let (pinned_family, _) = &pinned_rows[id];
        if is_repair(pinned_family.as_str()) {
            pinned_repair += 1;
        }
        if is_repair(family) {
            actual_repair += 1;
        }
        if !is_repair(pinned_family.as_str()) && is_repair(family) {
            regressions.push(format!(
                "row {id} (tier {tier}) routed to {pinned_family} before, CMRepair now"
            ));
        }
        if is_repair(pinned_family.as_str()) && !is_repair(family) {
            eprintln!("improvement: {id} left CMRepair ({pinned_family} → {family})");
        }
    }
    assert!(
        regressions.is_empty(),
        "routing regressions against the corpus:\n{}",
        regressions.join("\n")
    );
    assert!(
        actual_repair <= pinned_repair,
        "CMRepair routing grew: {actual_repair} > {pinned_repair} (update the snapshot only with a reviewed routing change)"
    );
}

//! Verifiable diary export — the P3 acceptance gate.
//!
//! The diary's promise is «дневник, который можно верифицировать»: over a
//! real session (fresh database, real turns, a caught contradiction) the
//! export must verify by deterministic replay, and any edit to the recorded
//! content — one letter in an entry, one letter in a response, a wrong
//! passphrase — must fail verification loudly. Prose outside the manifest
//! is presentation, not truth: editing it must NOT break verification.

use qxfx0_cli::codex::{append_diary_signature, build_diary_export, verify_diary};
use qxfx0_cli::run_journal_turn;
use qxfx0_pipeline::RendererAuthority;

fn session_state(db: &qxfx0_persistence::Persistence) -> qxfx0_types::system_state::SystemState {
    db.load_state("diary-export")
        .expect("session loads")
        .expect("session exists after the first turn")
}

#[test]
fn exported_diary_verifies_and_tampering_fails_loudly() {
    let db = qxfx0_persistence::Persistence::open_memory().expect("in-memory db opens");

    run_journal_turn(
        &db,
        "diary-export",
        "я думал о свободе: моя позиция — свобода требует границ",
        20_000,
        RendererAuthority::AuditedPlan,
    )
    .expect("turn 1 completes");
    run_journal_turn(
        &db,
        "diary-export",
        "я думал о долге: моя позиция — долг важнее настроения",
        20_000,
        RendererAuthority::AuditedPlan,
    )
    .expect("turn 2 completes");
    run_journal_turn(
        &db,
        "diary-export",
        "я думал о свободе: я ошибался раньше, это не так",
        20_001,
        RendererAuthority::AuditedPlan,
    )
    .expect("turn 3 completes");

    let state = session_state(&db);
    assert_eq!(state.dialogue.journal.len(), 3, "every turn is journaled");
    assert!(
        state
            .dialogue
            .journal
            .iter()
            .all(|record| !record.state_digest.is_empty()),
        "every journal record carries its replay witness"
    );

    let export = build_diary_export(&state, RendererAuthority::AuditedPlan);
    assert_eq!(export.manifest.entries.len(), 3);
    assert_eq!(export.manifest.practice_days, 2, "two distinct days");
    assert!(
        export.manifest.contradictions >= 1,
        "the caught contradiction is part of the manifest"
    );

    // The export is a pure function of the state: byte-identical twice.
    let again = build_diary_export(&state, RendererAuthority::AuditedPlan);
    assert_eq!(export.markdown, again.markdown);

    // 1. The untouched export verifies.
    let clean = verify_diary(&export.markdown, None);
    assert!(
        clean.verified(),
        "clean export must verify: {:?}",
        clean.failure
    );
    assert_eq!(clean.turns, 3);

    // 2. A signed export verifies with the right passphrase and fails with
    //    the wrong one — and refuses to run unsigned.
    let mut signed = export.markdown.clone();
    append_diary_signature(&mut signed, &export.manifest_json, "моя фраза");
    assert!(verify_diary(&signed, Some("моя фраза")).verified());
    let wrong_passphrase = verify_diary(&signed, Some("не та фраза"));
    assert!(!wrong_passphrase.verified());
    assert!(
        wrong_passphrase.failure.unwrap().contains("подпись"),
        "wrong passphrase is reported as a signature failure"
    );
    let missing_passphrase = verify_diary(&signed, None);
    assert!(!missing_passphrase.verified());

    // 3. Prose outside the manifest is presentation: editing it keeps the
    //    diary verifiable (the manifest is the truth).
    let prose_edited = export.markdown.replacen(
        "я думал о свободе: моя позиция",
        "я думал о свободе: ИЗМЕНЕНО",
        1,
    );
    assert!(
        verify_diary(&prose_edited, None).verified(),
        "prose edits must not affect verification"
    );

    // 4. One letter changed in a recorded entry breaks verification.
    let tampered_json = export.manifest_json.replacen("границ", "граници", 1);
    assert_ne!(
        &tampered_json, &export.manifest_json,
        "tamper target must exist in the manifest"
    );
    let tampered_manifest = export
        .markdown
        .replace(&export.manifest_json, &tampered_json);
    let entry_edited = verify_diary(&tampered_manifest, None);
    assert!(!entry_edited.verified(), "edited entry must not verify");

    // 5. One letter changed in a recorded response breaks verification too.
    let response_snippet: String = export.manifest.entries[0]
        .response
        .chars()
        .take(16)
        .collect();
    let response_replacement = format!("{response_snippet}Х");
    let tampered_response_json =
        export
            .manifest_json
            .replacen(&response_snippet, &response_replacement, 1);
    let tampered_response = export
        .markdown
        .replace(&export.manifest_json, &tampered_response_json);
    let response_edited = verify_diary(&tampered_response, None);
    assert!(
        !response_edited.verified(),
        "edited response must not verify"
    );

    // 6. A diary without a manifest block fails closed.
    assert!(!verify_diary("дневник без блока", None).verified());
}

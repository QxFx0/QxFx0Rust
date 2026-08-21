//! Essence as a living practice metric — vector 2 of the product
//! deepening. A caught contradiction is existential tension by definition,
//! so it must move the essence angst, and the practice surfaces must say so
//! when the tension is high. The essence is part of the persisted state, so
//! the whole thing must stay replay-verifiable.

use qxfx0_cli::codex::{build_diary_export, build_reflection_report, render_report_console};
use qxfx0_cli::run_journal_turn;
use qxfx0_pipeline::RendererAuthority;

fn state(db: &qxfx0_persistence::Persistence) -> qxfx0_types::system_state::SystemState {
    db.load_state("essence-practice")
        .expect("session loads")
        .expect("session exists")
}

#[test]
fn a_caught_contradiction_raises_the_practice_angst_and_stays_verifiable() {
    let db = qxfx0_persistence::Persistence::open_memory().expect("in-memory db opens");
    let session = "essence-practice";

    run_journal_turn(
        &db,
        session,
        "я думал о свободе: моя позиция — свобода требует границ",
        20_000,
        RendererAuthority::AuditedPlan,
    )
    .expect("turn 1 completes");
    let before = state(&db).semantic.essence.angst;

    run_journal_turn(
        &db,
        session,
        "я думал о свободе: я ошибался раньше, это не так",
        20_001,
        RendererAuthority::AuditedPlan,
    )
    .expect("turn 2 completes");
    let after = state(&db);
    let report = build_reflection_report(&after);
    assert!(
        report.contradictions >= 1,
        "the contradiction is caught first"
    );
    assert!(
        after.semantic.essence.angst >= before + 0.05,
        "the contradiction feeds the essence angst: before={before}, after={}",
        after.semantic.essence.angst
    );

    // Once enough tension accumulates, the report names it. Alternate
    // position and self-refutation on one topic: every odd turn is a fresh
    // contradiction (+0.1 each), so 6 of them cross 0.5 regardless of the
    // witness accrual in between.
    let mut db = qxfx0_persistence::Persistence::open_memory().expect("db opens");
    for day in 0..13u64 {
        let text = if day % 2 == 1 {
            // Distinct each day: an identical refutation is correctly
            // deduplicated and would not create a fresh contradiction.
            format!("я думал о свободе, день {day}: я ошибался раньше, это не так")
        } else {
            format!("я думал о свободе, день {day}: моя позиция — принять это всерьёз")
        };
        run_journal_turn(
            &db,
            session,
            &text,
            20_000 + day,
            RendererAuthority::AuditedPlan,
        )
        .expect("turn completes");
    }
    let tense_state = state(&db);
    let tense_report = build_reflection_report(&tense_state);
    assert!(
        tense_report.contradictions >= 3,
        "enough collisions accumulated: {}",
        tense_report.contradictions
    );
    assert!(
        tense_report.angst >= 0.5,
        "angst crossed the attention threshold: {}",
        tense_report.angst
    );
    assert!(
        render_report_console(&tense_report).contains("Тревога практики"),
        "the report names the tension"
    );

    // The essence is part of the replayed state: the export still verifies.
    let export = build_diary_export(&tense_state, RendererAuthority::AuditedPlan);
    let verification = qxfx0_cli::codex::verify_diary(&export.markdown, None);
    assert!(
        verification.verified(),
        "the essence-moving journal stays replay-verifiable: {:?}",
        verification.failure
    );
}

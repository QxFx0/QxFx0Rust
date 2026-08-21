//! The turn response engages the practitioner's prior position — vector 1
//! of the product deepening. When a turn lands on a topic that already
//! carries a held position from an earlier turn, the response itself quotes
//! that position and appends the graph's opposing edge. The first visit
//! stays clean (nothing to challenge yet), and the appendix is
//! deterministic: a fresh replay of the same journal reproduces it
//! byte-identically.

use qxfx0_cli::run_journal_turn;
use qxfx0_pipeline::RendererAuthority;

#[test]
fn turn_response_challenges_a_prior_position_on_the_topic() {
    let db = qxfx0_persistence::Persistence::open_memory().expect("in-memory db opens");
    let session = "turn-challenge";

    let first = run_journal_turn(
        &db,
        session,
        "я думал о свободе: моя позиция — свобода требует границ",
        20_000,
        RendererAuthority::AuditedPlan,
    )
    .expect("first turn completes");
    assert!(
        !first.contains("Граф возражает"),
        "the first visit has no prior position to challenge"
    );

    let second = run_journal_turn(
        &db,
        session,
        "я думал о свободе: свобода требует сознательности",
        20_001,
        RendererAuthority::AuditedPlan,
    )
    .expect("second turn completes");
    assert!(
        second.contains("Я помню твою позицию [ход 1]"),
        "the response quotes the prior position: {second}"
    );
    assert!(
        second.contains("Граф возражает: "),
        "the response carries the graph's opposing edge: {second}"
    );
    assert!(
        second
            .trim_end()
            .ends_with("Как это совместить — или одна из них должна уйти?"),
        "the challenge ends with the question to the practitioner"
    );

    // Deterministic: a fresh session replaying the same journal reproduces
    // the challenged response byte-identically.
    let replay_db = qxfx0_persistence::Persistence::open_memory().expect("replay db opens");
    run_journal_turn(
        &replay_db,
        session,
        "я думал о свободе: моя позиция — свобода требует границ",
        20_000,
        RendererAuthority::AuditedPlan,
    )
    .expect("replay turn 1");
    let replayed = run_journal_turn(
        &replay_db,
        session,
        "я думал о свободе: свобода требует сознательности",
        20_001,
        RendererAuthority::AuditedPlan,
    )
    .expect("replay turn 2");
    assert_eq!(
        second, replayed,
        "the challenge appendix is a pure function of (input, state)"
    );

    // A topic without a held position never gets the appendix.
    let untouched = run_journal_turn(
        &db,
        session,
        "я думал о долге: моя позиция — долг важнее настроения",
        20_002,
        RendererAuthority::AuditedPlan,
    )
    .expect("turn on a fresh topic completes");
    assert!(
        !untouched.contains("Я помню твою позицию"),
        "a positionless topic is answered without a challenge: {untouched}"
    );
}

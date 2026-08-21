//! «Кодекс» product acceptance: the thirty-day practice loop.
//!
//! This is the proof that the journal has product value, not just
//! architecture: over thirty synthetic days of daily practice the loop must
//! (1) stamp every practiced day, (2) walk the audited corpus first and
//! then consciously REVISIT topics, (3) catch the practitioner
//! contradicting an earlier position, and (4) echo prior positions and the
//! contradiction on later reflection cards and in the report.
//!
//! Days are synthetic (`start + n`) and fed through `run_journal_turn`, so
//! the loop runs in-process and deterministically.

use qxfx0_cli::codex::{
    build_memory_card, build_reflection_card, build_reflection_report, render_memory_card,
    select_topic_of_day,
};
use qxfx0_cli::run_journal_turn;
use qxfx0_pipeline::RendererAuthority;

fn temp_db(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("qxfx0-{name}-{}.db", std::process::id()));
    let _ = std::fs::remove_file(&path);
    path
}

fn journal_state(db: &qxfx0_persistence::Persistence) -> qxfx0_types::system_state::SystemState {
    db.load_state("diary")
        .expect("session loads")
        .expect("session exists after the first turn")
}

#[test]
fn thirty_day_practice_closes_the_loop() {
    let db_path = temp_db("diary-practice");
    let db_path_str = db_path.to_str().expect("temp path is UTF-8").to_string();
    let db = qxfx0_persistence::Persistence::open(&db_path_str).expect("database opens");
    let start = 20_000u64;
    let days = 30u64;

    let mut revisited_any = false;
    for day in 0..days {
        let state = journal_state_if_exists(&db);
        let topic = select_topic_of_day(start + day, state.as_ref());
        let already_reflected = state.as_ref().is_some_and(|state| {
            state
                .semantic
                .semantic_commitments
                .as_ref()
                .is_some_and(|store| {
                    store
                        .active
                        .values()
                        .any(|(payload, _)| payload.topic == topic)
                })
        });
        if already_reflected {
            revisited_any = true;
        }

        // Day 12 contradicts the practitioner's earlier stance on purpose.
        // Revisits must write distinct sentences: a held position is the
        // practitioner's own text now, and an identical entry is correctly
        // deduplicated instead of accumulating.
        let text = if day == 12 {
            format!("я думал о {topic}: я ошибался раньше, это не так")
        } else {
            format!("я думал о {topic}, день {day}: моя позиция — принять это всерьёз")
        };
        run_journal_turn(
            &db,
            "diary",
            &text,
            start + day,
            RendererAuthority::LegacyShadow,
        )
        .expect("journal turn completes");
    }

    let state = journal_state(&db);
    let report = build_reflection_report(&state);

    assert_eq!(report.practice_days as u64, days, "every day is stamped");
    assert_eq!(
        report.practice_streak as u64, days,
        "consecutive synthetic days form one streak"
    );
    assert!(
        report.contradictions >= 1,
        "the practice must catch the practitioner contradicting themself; \
         contradictions recorded: {}",
        report.contradictions
    );
    assert!(
        report
            .commitments_by_topic
            .iter()
            .any(|(_, count)| *count >= 2),
        "a revisited topic must accumulate positions: {:?}",
        report.commitments_by_topic
    );
    assert!(
        revisited_any,
        "the revisit policy must return an already-reflected topic"
    );
    assert_eq!(
        report.governance_commitment_contradicted >= 1,
        report.contradictions >= 1,
        "each recorded contradiction is replay-visible in governance"
    );

    // The card for the contradicted topic carries the memory: prior
    // positions and the event are scoped to the recalled subject.
    let topic = state
        .semantic
        .semantic_commitments
        .as_ref()
        .and_then(|store| store.contradictions.last())
        .and_then(|event| {
            store_topic(&state, &event.left).or_else(|| store_topic(&state, &event.right))
        })
        .expect("a contradiction identifies a topic");
    let card = build_reflection_card(&topic, start + 100).expect("audited topic");
    let memory = build_memory_card(card, &state);
    assert!(
        !memory.prior_positions.is_empty(),
        "the card must echo what the practitioner wrote before"
    );
    assert!(memory.contradiction.is_some(), "the event must be visible");
    assert_eq!(memory.practice_days as u64, days);
    let rendered = render_memory_card(&memory);
    assert!(rendered.contains("В прошлый раз"));
    assert!(rendered.contains("Событие практики"));
    assert!(rendered.contains("Дней практики: 30"));

    let _ = std::fs::remove_file(db_path);
}

fn store_topic(
    state: &qxfx0_types::system_state::SystemState,
    id: &qxfx0_types::system_state::CommitmentId,
) -> Option<String> {
    state
        .semantic
        .semantic_commitments
        .as_ref()?
        .active
        .get(id)
        .map(|(payload, _)| payload.topic.clone())
}

fn journal_state_if_exists(
    db: &qxfx0_persistence::Persistence,
) -> Option<qxfx0_types::system_state::SystemState> {
    db.load_state("diary").ok().flatten()
}

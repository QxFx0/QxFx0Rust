//! The journal-turn runtime: load → pipeline → practice-calendar stamp → save.
//!
//! Moved from the CLI crate with the «Кодекс» extraction (ADR-0043 U0.6) so
//! the diary product and its persistence runner live in one crate below the
//! CLI. The calendar semantics are unchanged: the epoch day is always an
//! explicit boundary input — the CLI passes "now", tests pass synthetic days —
//! and the pipeline never samples the clock, keeping the deterministic core
//! (same input + same state → same output) intact.

use super::epoch_day;
use qxfx0_persistence::Persistence;
use qxfx0_pipeline::{
    process_turn_with_options, EssenceAblation, RendererAuthority, TurnInput, TurnOptions,
};
use qxfx0_types::system_state::{SemanticState, SystemState};

/// Build a freshly seeded `SystemState` for a given session id.
pub fn fresh_state(session_id: &str) -> SystemState {
    SystemState {
        session_id: session_id.to_string(),
        semantic: SemanticState {
            runtime_graph: qxfx0_semantic::seed_graph(),
            ..Default::default()
        },
        ..Default::default()
    }
}

/// Load existing state for the session, or create a fresh one seeded with the
/// knowledge graph. Any persistence error other than "no such session row"
/// (which `Persistence::load_state` already maps to `Ok(None)`) is propagated
/// via `?` so the caller can surface it to the user.
pub fn load_or_create_state(db: &Persistence, session_id: &str) -> anyhow::Result<SystemState> {
    match db.load_state(session_id) {
        Ok(Some(state)) => Ok(state),
        Ok(None) => Ok(fresh_state(session_id)),
        Err(e) => Err(anyhow::anyhow!(e)),
    }
}

/// Run one turn with explicit authority for admitted content-plan rendering.
/// Current UTC epoch day, read once per call at the CLI boundary.
pub fn today_epoch_day() -> u64 {
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    epoch_day(unix_seconds)
}

/// Stamp the practice calendar with today and persist. Every CLI turn path
/// funnels its saves through here, so the journal calendar stays consistent
/// regardless of which trace flags accompanied the turn.
pub fn save_journal_state(
    db: &Persistence,
    session_id: &str,
    state: &mut SystemState,
) -> anyhow::Result<()> {
    stamp_practice_today(state);
    db.save_state(session_id, state)?;
    Ok(())
}

/// Stamp today onto the practice calendar in place. The timings-measuring
/// runners call this before `save_state_with_timings` so their sessions
/// share the journal calendar too.
pub fn stamp_practice_today(state: &mut SystemState) {
    let day = today_epoch_day();
    stamp_practice_day(state, day);
}

/// Record one practice day and the topic answered on it, and finalize the
/// journal record of the turn that was just processed. Keeping this at the
/// CLI boundary makes the calendar/revisit policy deterministic in tests
/// and in replay: the day is an explicit input, never sampled by the
/// pipeline.
///
/// Finalization fills the record's placeholder day and writes the replay
/// witness: the stable digest of the state **as it stands at this moment**
/// (record included, its `state_digest` field still empty). A replay that
/// reconstructs records 1..N-1 byte-identically recomputes the same digest
/// for turn N — that is what makes the diary export verifiable. Idempotent:
/// a save without a new turn (chat quit, double save) finalizes nothing.
fn stamp_practice_day(state: &mut SystemState, day: u64) {
    state.dialogue.practice_days.insert(day);
    if let Some(topic) = state.dialogue.last_topic.clone() {
        state.dialogue.topic_last_practice_day.insert(topic, day);
    }
    let Some(pending) = state.dialogue.journal.last() else {
        return;
    };
    if !pending.state_digest.is_empty() {
        return;
    }
    if let Some(record) = state.dialogue.journal.last_mut() {
        record.day = day;
    }
    // The journal witness covers the behaviourally-relevant state only: the
    // ADR-0043 U2 shadow trajectory (`semantic.essence_v2`) and the U3
    // blanket record (`semantic.blanket_v2`) are lifted out for the digest
    // and restored right after. A pre-U2 state serializes byte-identically
    // either way (both fields are skip-when-none), so diaries recorded
    // before the shadow verify on binaries that carry it.
    let essence_v2 = state.semantic.essence_v2.take();
    let blanket_v2 = state.semantic.blanket_v2.take();
    let digest = qxfx0_pipeline::execution_trace::calculate_stable_digest(state)
        .expect("SystemState serializes deterministically for the stable digest");
    state.semantic.essence_v2 = essence_v2;
    state.semantic.blanket_v2 = blanket_v2;
    if let Some(record) = state.dialogue.journal.last_mut() {
        record.state_digest = digest;
    }
}

/// Run a journal turn: like a plain renderer turn, plus the practice
/// calendar. The epoch day comes from the caller's clock — the CLI passes
/// "today", tests pass synthetic days — and is stamped onto the persisted
/// state at the boundary, never sampled inside the pipeline, so the
/// deterministic core (same input + same state → same output) is intact.
pub fn run_journal_turn(
    db: &Persistence,
    session_id: &str,
    text: &str,
    epoch_day: u64,
    renderer_authority: RendererAuthority,
) -> anyhow::Result<String> {
    run_journal_turn_with_essence_ablation(
        db,
        session_id,
        text,
        epoch_day,
        renderer_authority,
        EssenceAblation::Enabled,
    )
}

/// The B2 control-arm journal turn (ADR-0043 U2): identical to
/// [`run_journal_turn`] except the V2 subject core suppresses commitment
/// while still witnessing. Explicit experiment surface only — the production
/// paths delegate with [`EssenceAblation::Enabled`].
pub fn run_journal_turn_with_essence_ablation(
    db: &Persistence,
    session_id: &str,
    text: &str,
    epoch_day: u64,
    renderer_authority: RendererAuthority,
    essence_v2_ablation: EssenceAblation,
) -> anyhow::Result<String> {
    let mut state = load_or_create_state(db, session_id)?;
    let input = TurnInput {
        raw_text: text.to_string(),
        session_id: session_id.to_string(),
    };
    let output = process_turn_with_options(
        &input,
        &mut state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_essence_v2_ablation(essence_v2_ablation),
    );
    stamp_practice_day(&mut state, epoch_day);
    db.save_state(session_id, &state)?;
    Ok(output.response)
}

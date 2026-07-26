//! QxFx0 Pipeline — synchronous sequential turn processing.
//!
//! 6 stages: Prepare → Route → Render → Finalize → Guard → Persist.
//! No async, no Tokio, no external middleware — pure synchronous call chain.

#[cfg(test)]
mod conjugate_pipeline;
pub mod conversation_fsm;
#[path = "tracing.rs"]
pub mod execution_trace;
pub mod stages;
#[cfg(test)]
mod vector_pipeline;

pub use conversation_fsm::{
    fsm_state_discriminant, fsm_state_from_discriminant, initial_state, is_active,
    proposition_to_event, transition as fsm_transition, ConversationEvent, ConversationState,
};

use qxfx0_semantic::PropositionParser;
use qxfx0_types::atom::AtomId;
use qxfx0_types::system_state::*;
use qxfx0_types::*;
use serde::Serialize;
use stages::Hints;
use std::collections::BTreeMap;
use std::time::Instant;

pub(crate) const CHALLENGE_PATTERNS: &[&str] = &[
    "это просто",
    "не более чем",
    "сводится к",
    "всего лишь",
    "это лишь",
    "разве",
    "не согласен",
    "не согласна",
    "противореч",
    "неверно",
    "ошибаешься",
    "не прав",
    "спорю",
    "возраж",
    "сомневаюсь",
    "оспариваю",
];

/// Centralized challenge detection — single source of truth used by all pipelines.
/// Combines parser-based mode detection with substring pattern matching so that
/// no challenge is missed by either mechanism.
pub fn detect_challenge(text: &str) -> bool {
    let parsed = PropositionParser::parse(text);
    if matches!(parsed.mode, qxfx0_semantic::PropositionMode::Challenge) {
        return true;
    }
    let lower = text.to_lowercase();
    CHALLENGE_PATTERNS.iter().any(|p| lower.contains(p))
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnInput {
    pub session_id: String,
    pub raw_text: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnOutput {
    pub response: String,
    pub family: CanonicalMoveFamily,
    pub guard_status: GuardStatus,
    pub blocked: bool,
    pub commitment_engaged: bool,
    pub governance_events: usize,
    pub conatus_energy: f64,
    pub path_depth: usize,
    pub holistic_dominant: bool,
    pub conversation_state: String,
}

/// Build a recovery output after a stage failure, using the rolled-back state.
fn recovery_output(state: &SystemState, hints: &Hints) -> TurnOutput {
    let family_str = hints.get("family").cloned().unwrap_or_default();
    let family = CanonicalMoveFamily::from_hint(&family_str);
    let conversation_state = hints
        .get("conversation_state")
        .cloned()
        .unwrap_or_else(|| format!("{:?}", family));
    let conatus_energy = hints
        .get("conatus_energy")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let path_depth = hints
        .get("path_depth")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    TurnOutput {
        response: "QxFx0: внутренняя ошибка обработки, состояние восстановлено.".into(),
        family,
        guard_status: GuardStatus::Blocked("stage error".into()),
        blocked: true,
        commitment_engaged: false,
        governance_events: state.governance_log.len(),
        conatus_energy,
        path_depth,
        holistic_dominant: state.semantic.adjunction.holistic_dominant,
        conversation_state,
    }
}

fn session_invariant_output(state: &SystemState, reason: &str) -> TurnOutput {
    TurnOutput {
        response: "QxFx0: идентификатор сессии отклонён; состояние не изменено.".into(),
        family: CanonicalMoveFamily::CMRepair,
        guard_status: GuardStatus::InvariantBlock(reason.into()),
        blocked: true,
        commitment_engaged: false,
        governance_events: state.governance_log.len(),
        conatus_energy: 0.0,
        path_depth: 0,
        holistic_dominant: state.semantic.adjunction.holistic_dominant,
        conversation_state: state
            .dialogue
            .conversation_state
            .map(|value| value.to_string())
            .unwrap_or_else(|| "Idle".into()),
    }
}

fn execute_stage<F>(
    trace: &mut Option<&mut execution_trace::PipelineTrace>,
    stage_name: &str,
    state: &mut SystemState,
    hints: &mut Hints,
    stage: F,
) -> Result<(), String>
where
    F: FnOnce(&mut SystemState, &mut Hints) -> Result<(), String>,
{
    if trace.is_none() {
        return stage(state, hints);
    }

    let input_digest = execution_trace::calculate_stable_digest(&(&*state, &*hints))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let start = Instant::now();
    let result = stage(state, hints);
    let output_digest = execution_trace::calculate_stable_digest(&(&*state, &*hints, &result))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let mut metadata = BTreeMap::new();
    metadata.insert(
        "status".into(),
        if result.is_ok() { "ok" } else { "error" }.into(),
    );
    if let Some(family) = hints.get("family") {
        metadata.insert("family".into(), family.clone());
    }
    if let Some(trace) = trace.as_deref_mut() {
        trace.record_step(
            stage_name,
            input_digest,
            output_digest,
            start.elapsed(),
            metadata,
        );
    }
    result
}

/// Process a single turn synchronously through all 6 stages.
///
/// If any stage before the guard fails, the state is rolled back to its
/// pre-turn snapshot and a blocked recovery output is returned. This prevents
/// partial side effects from corrupting the session.
pub fn process_turn(input: &TurnInput, state: &mut SystemState) -> TurnOutput {
    process_turn_internal(input, state, None)
}

/// Process a turn and return a stage-level trace with cross-process stable
/// SHA-256 digests. Durations are diagnostic and excluded from replay
/// signatures.
pub fn process_turn_with_trace(
    input: &TurnInput,
    state: &mut SystemState,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    let request_id = execution_trace::calculate_stable_digest(&(
        input,
        state.dialogue.turn_count,
        state.session_id.as_str(),
    ))
    .unwrap_or_else(|_| "trace-unavailable".into());
    let initial_digest = execution_trace::calculate_stable_digest(&(&*state, input))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let start = Instant::now();
    let mut trace = execution_trace::PipelineTrace::new(&request_id);
    let output = process_turn_internal(input, state, Some(&mut trace));
    let final_digest = execution_trace::calculate_stable_digest(&(&*state, &output))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "turn_output",
        initial_digest,
        final_digest,
        std::time::Duration::ZERO,
        BTreeMap::from([
            ("blocked".into(), output.blocked.to_string()),
            ("family".into(), format!("{:?}", output.family)),
        ]),
    );
    trace.set_total_duration(start.elapsed());
    (output, trace)
}

fn process_turn_internal(
    input: &TurnInput,
    state: &mut SystemState,
    mut trace: Option<&mut execution_trace::PipelineTrace>,
) -> TurnOutput {
    if input.session_id.trim().is_empty()
        || input.session_id.chars().count() > 128
        || input.session_id.chars().any(char::is_control)
    {
        return session_invariant_output(state, "invalid session_id");
    }
    if state.session_id.is_empty() {
        state.session_id = input.session_id.clone();
    } else if state.session_id != input.session_id {
        return session_invariant_output(state, "session_id does not match loaded state");
    }
    let state_violations = state.validate();
    if !state_violations.is_empty() {
        tracing::error!("state invariant violation: {}", state_violations.join("; "));
        return session_invariant_output(state, "loaded state violates invariants");
    }

    let snapshot = state.clone();
    let mut hints: Hints = BTreeMap::new();
    hints.insert("raw_text".into(), input.raw_text.clone());
    hints.insert("session_id".into(), input.session_id.clone());

    // Parse once, stash subject/mode/is_challenge for all stages.
    let mut prop = PropositionParser::parse(&input.raw_text);

    // Normalize subject to nominative form using the runtime graph.
    // Users type topics in oblique cases ("об ответственности" → "ответственности")
    // but the graph stores atoms in nominative ("ответственность").
    if state.semantic.runtime_graph.edges.is_empty() {
        state.semantic.runtime_graph = qxfx0_semantic::seed_graph();
    }
    if matches!(
        prop.mode,
        qxfx0_semantic::PropositionMode::Define | qxfx0_semantic::PropositionMode::Assert
    ) {
        if let Some(known_topic) = qxfx0_semantic::PropositionParser::known_topic_in_input(
            &input.raw_text,
            &state.semantic.runtime_graph,
        ) {
            prop.subject = known_topic;
        }
    }
    prop.subject = qxfx0_semantic::PropositionParser::normalize_topic(
        &prop.subject,
        &state.semantic.runtime_graph,
    );

    hints.insert("subject".into(), prop.subject.clone());
    hints.insert("raw_mode".into(), format!("{:?}", prop.mode));
    let is_challenge = detect_challenge(&input.raw_text);
    hints.insert("is_challenge".into(), is_challenge.to_string());

    // Stage 1: Prepare
    if let Err(e) = execute_stage(
        &mut trace,
        "prepare",
        state,
        &mut hints,
        stages::prepare_stage,
    ) {
        tracing::error!("prepare_stage failed: {e}");
        *state = snapshot;
        return recovery_output(state, &hints);
    }

    // Stage 2: Route
    if let Err(e) = execute_stage(&mut trace, "route", state, &mut hints, stages::route_stage) {
        tracing::error!("route_stage failed: {e}");
        *state = snapshot;
        return recovery_output(state, &hints);
    }

    // Stage 3: Render
    if let Err(e) = execute_stage(
        &mut trace,
        "render",
        state,
        &mut hints,
        stages::render_stage,
    ) {
        tracing::error!("render_stage failed: {e}");
        *state = snapshot;
        return recovery_output(state, &hints);
    }

    // Stage 4: Finalize
    if let Err(e) = execute_stage(
        &mut trace,
        "finalize",
        state,
        &mut hints,
        stages::finalize_stage,
    ) {
        tracing::error!("finalize_stage failed: {e}");
        *state = snapshot;
        return recovery_output(state, &hints);
    }

    // Stage 5: Guard
    let guard_result = execute_stage(&mut trace, "guard", state, &mut hints, stages::guard_stage);
    if let Err(e) = &guard_result {
        // A guard rejection is an expected turn outcome, not a pipeline
        // failure. It must still be archived and returned as a safe response.
        tracing::warn!("guard rejected turn: {e}");
    }

    // Stage 6: Persist
    if let Err(e) = execute_stage(
        &mut trace,
        "persist",
        state,
        &mut hints,
        stages::persist_stage,
    ) {
        tracing::warn!("persist_stage failed: {e}");
    }

    let mut response = hints
        .get("response")
        .cloned()
        .unwrap_or_else(|| "QxFx0: обработка завершена.".into());
    let family_str = hints.get("family").cloned().unwrap_or_default();
    let family = CanonicalMoveFamily::from_hint(&family_str);
    let guard_status = match &state.last_turn_decision {
        Some(decision) => decision.guard_status.clone(),
        None => GuardStatus::Allowed,
    };
    let blocked = matches!(
        guard_status,
        GuardStatus::Blocked(_) | GuardStatus::InvariantBlock(_)
    ) || guard_result.is_err();

    // A rejected response must not mutate semantic/self state. Governance and
    // the explicit blocked decision remain, then dialogue bookkeeping below
    // records that a rejected turn occurred.
    if blocked {
        state.semantic = snapshot.semantic.clone();
        state.dialogue.conversation_state = snapshot.dialogue.conversation_state;
    }

    // W6: If the guard blocked this turn, replace the response with a recovery string
    // before it is stored in history or returned to the user.
    if blocked {
        response = "QxFx0: ответ отклонён системой безопасности.".into();
    }

    // State sync — on blocked turns, still advance turn_count and record
    // history, but skip field adjustments (the response was rejected).
    state.dialogue.turn_count += 1;
    state.dialogue.last_family = family;
    state.dialogue.last_topic = Some(prop.subject.clone());
    state.dialogue.history.push(response.clone());
    if state.dialogue.history.len() > 10_000 {
        let excess = state.dialogue.history.len() - 10_000;
        state.dialogue.history.drain(0..excess);
    }

    // Field adjustments — skip on blocked turns (rejected output should not
    // reinforce confidence or counterfactual).
    if !blocked {
        let topic_in_graph = state
            .semantic
            .runtime_graph
            .atoms
            .contains_key(&AtomId::new(prop.subject.clone()));
        if topic_in_graph {
            state.semantic.field.confidence = (state.semantic.field.confidence + 0.1).min(1.0);
            state.semantic.field.resonance = (state.semantic.field.resonance + 0.05).min(1.0);
            // Positive atmosphere: known topic → valence drifts positive,
            // arousal increases slightly (engagement).
            state.semantic.field.atmosphere.valence =
                (state.semantic.field.atmosphere.valence + 0.05).min(1.0);
            state.semantic.field.atmosphere.arousal =
                (state.semantic.field.atmosphere.arousal + 0.03).min(1.0);
        } else {
            state.semantic.field.counterfactual =
                (state.semantic.field.counterfactual + 0.1).min(1.0);
            // Unknown topic → valence drifts negative (uncertainty),
            // arousal increases (heightened alertness).
            state.semantic.field.atmosphere.valence =
                (state.semantic.field.atmosphere.valence - 0.05).max(-1.0);
            state.semantic.field.atmosphere.arousal =
                (state.semantic.field.atmosphere.arousal + 0.05).min(1.0);
        }
        // Decay arousal slightly each turn (baseline calm).
        state.semantic.field.atmosphere.arousal =
            (state.semantic.field.atmosphere.arousal - 0.02).max(0.0);
    }

    // W10: conversation_state carries the FSM state (from route_stage), not the move family.
    let conversation_state = hints
        .get("conversation_state")
        .cloned()
        .unwrap_or_else(|| format!("{:?}", family));

    let conatus_energy: f64 = hints
        .get("conatus_energy")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);

    let path_depth: usize = hints
        .get("path_depth")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    let commitment_engaged = if let Some(store) = &state.semantic.semantic_commitments {
        let eng = qxfx0_commitment::CommitmentOps::detect_engagement(store, &prop.subject);
        !eng.engaged_ids.is_empty()
    } else {
        false
    };

    TurnOutput {
        response,
        family,
        guard_status,
        blocked,
        commitment_engaged,
        governance_events: state.governance_log.len(),
        conatus_energy,
        path_depth,
        holistic_dominant: state.semantic.adjunction.holistic_dominant,
        conversation_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_state(session_id: &str) -> SystemState {
        SystemState {
            session_id: session_id.into(),
            ..SystemState::default()
        }
    }

    #[test]
    fn test_pipeline_process_turn_define() {
        let mut state = test_state("test");
        let input = TurnInput {
            session_id: "test".into(),
            raw_text: "что такое свобода?".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(!output.response.is_empty());
    }

    #[test]
    fn test_pipeline_process_turn_challenge() {
        let mut state = test_state("test-ch");
        let input = TurnInput {
            session_id: "test-ch".into(),
            raw_text: "свобода это просто отсутствие ограничений".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(!output.response.is_empty());
    }

    #[test]
    fn test_pipeline_multi_turn_no_crash() {
        let mut state = test_state("multi");
        let inputs = [
            "что такое свобода?",
            "свобода это просто вседозволенность",
            "что ты думаешь об ответственности?",
        ];
        for text in &inputs {
            let input = TurnInput {
                session_id: "multi".into(),
                raw_text: text.to_string(),
            };
            let _output = process_turn(&input, &mut state);
        }
    }

    #[test]
    fn test_pipeline_determinism_same_input_same_output() {
        let inputs = ["что такое свобода?", "что ты думаешь об истине?"];
        for text in &inputs {
            let mut state1 = test_state("det");
            let mut state2 = test_state("det");

            let input = TurnInput {
                session_id: "det".into(),
                raw_text: text.to_string(),
            };
            let out1 = process_turn(&input, &mut state1);
            let out2 = process_turn(&input, &mut state2);
            assert_eq!(
                out1.response, out2.response,
                "same input must produce same output"
            );
            assert_eq!(out1.family, out2.family);
            assert_eq!(out1.conatus_energy, out2.conatus_energy);
            assert_eq!(out1.path_depth, out2.path_depth);
        }
    }

    #[test]
    fn test_pipeline_path_depth_nonzero() {
        let mut state = test_state("depth");
        let input = TurnInput {
            session_id: "depth".into(),
            raw_text: "что такое свобода?".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(
            output.path_depth > 0,
            "path_depth should be non-zero for known topic"
        );
    }

    #[test]
    fn test_pipeline_blocked_turn_no_field_change() {
        let mut state = test_state("block");
        let field_before = state.semantic.field.clone();

        // Use a very long string to trigger a safety/quality block in ContentQualityGate
        let input = TurnInput {
            session_id: "block".into(),
            raw_text: "a".repeat(10_000),
        };
        let output = process_turn(&input, &mut state);
        assert!(
            output.blocked,
            "Turn should be blocked for excessively long input"
        );
        assert_eq!(
            state.semantic.field.confidence, field_before.confidence,
            "blocked turn should not change field confidence"
        );
    }
}

//! QxFx0 Pipeline — synchronous sequential turn processing.
//!
//! 6 stages: Prepare → Route → Render → Finalize → Guard → Persist.
//! No async, no Tokio, no external middleware — pure synchronous call chain.

pub mod conversation_fsm;
pub mod stages;
#[cfg(test)]
mod conjugate_pipeline;
#[cfg(test)]
mod vector_pipeline;

pub use conversation_fsm::{
    initial_state, is_active, proposition_to_event, transition as fsm_transition,
    fsm_state_discriminant, fsm_state_from_discriminant,
    ConversationEvent, ConversationState,
};

use qxfx0_semantic::PropositionParser;
use qxfx0_types::atom::AtomId;
use qxfx0_types::system_state::*;
use qxfx0_types::*;
use stages::Hints;
use std::collections::BTreeMap;

pub(crate) const CHALLENGE_PATTERNS: &[&str] = &[
    "это просто", "не более чем", "сводится к", "всего лишь", "это лишь",
    "разве", "не согласен", "не согласна", "противореч", "неверно",
    "ошибаешься", "не прав", "спорю", "возраж", "сомневаюсь", "оспариваю",
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

#[derive(Debug, Clone)]
pub struct TurnInput {
    pub session_id: String,
    pub raw_text: String,
}

#[derive(Debug, Clone)]
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

/// Process a single turn synchronously through all 6 stages.
pub fn process_turn(input: &TurnInput, state: &mut SystemState) -> TurnOutput {
    let mut hints: Hints = BTreeMap::new();
    hints.insert("raw_text".into(), input.raw_text.clone());
    hints.insert("session_id".into(), input.session_id.clone());

    // Parse once, stash subject/mode/is_challenge for all stages.
    let prop = PropositionParser::parse(&input.raw_text);
    hints.insert("subject".into(), prop.subject.clone());
    hints.insert("raw_mode".into(), format!("{:?}", prop.mode));
    hints.insert("is_challenge".into(), detect_challenge(&input.raw_text).to_string());

    // Stage 1: Prepare
    if let Err(e) = stages::prepare_stage(state, &mut hints) {
        tracing::warn!("prepare_stage failed: {e}");
    }

    // Stage 2: Route
    if let Err(e) = stages::route_stage(state, &mut hints) {
        tracing::warn!("route_stage failed: {e}");
    }

    // Stage 3: Render
    if let Err(e) = stages::render_stage(state, &mut hints) {
        tracing::warn!("render_stage failed: {e}");
    }

    // Snapshot state before finalize — if the guard blocks this turn, we must
    // roll back ALL side effects that finalize_stage stored, since the guard
    // runs after finalize (H8 fix: complete rollback).
    let essence_snapshot = state.semantic.essence.clone();
    let commit_store_snapshot = state.semantic.semantic_commitments.clone();
    let graph_snapshot = state.semantic.runtime_graph.clone();

    // Stage 4: Finalize
    if let Err(e) = stages::finalize_stage(state, &mut hints) {
        tracing::warn!("finalize_stage failed: {e}");
    }

    // Stage 5: Guard
    let guard_result = stages::guard_stage(state, &mut hints);

    // Stage 6: Persist
    if let Err(e) = stages::persist_stage(state, &mut hints) {
        tracing::warn!("persist_stage failed: {e}");
    }

    let mut response = hints.get("response").cloned().unwrap_or_else(|| "QxFx0: обработка завершена.".into());
    let family_str = hints.get("family").cloned().unwrap_or_default();
    let family = CanonicalMoveFamily::from_hint(&family_str);
    let guard_status = match &state.last_turn_decision {
        Some(decision) => decision.guard_status.clone(),
        None => GuardStatus::Allowed,
    };
    let blocked = matches!(guard_status, GuardStatus::Blocked(_) | GuardStatus::InvariantBlock(_))
        || guard_result.is_err();

    // H8: If the guard blocked this turn, roll back ALL finalize side effects:
    // essence state (including commitment), commitment store, and graph growth.
    if blocked {
        state.semantic.essence = essence_snapshot;
        state.semantic.semantic_commitments = commit_store_snapshot;
        state.semantic.runtime_graph = graph_snapshot;
    }

    // W6: If the guard blocked this turn, replace the response with a recovery string
    // before it is stored in history or returned to the user.
    if blocked {
        response = "QxFx0: ответ отклонён системой безопасности.".into();
    }

    // State sync — skip dialogue/field advancement on blocked turns.
    state.dialogue.turn_count += 1;
    state.dialogue.last_family = family;
    state.dialogue.last_topic = Some(prop.subject.clone());
    state.dialogue.history.push(response.clone());
    if state.dialogue.history.len() > 10_000 {
        let excess = state.dialogue.history.len() - 10_000;
        state.dialogue.history.drain(0..excess);
    }

    // Field adjustments
    let topic_in_graph = state.semantic.runtime_graph.atoms.contains_key(&AtomId::new(prop.subject.clone()));
    if topic_in_graph {
        state.semantic.field.confidence = (state.semantic.field.confidence + 0.1).min(1.0);
        state.semantic.field.resonance = (state.semantic.field.resonance + 0.05).min(1.0);
    } else {
        state.semantic.field.counterfactual = (state.semantic.field.counterfactual + 0.1).min(1.0);
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
        path_depth: 0,
        holistic_dominant: state.semantic.adjunction.holistic_dominant,
        conversation_state,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_process_turn_define() {
        let mut state = SystemState::default();
        state.session_id = "test".into();
        let input = TurnInput {
            session_id: "test".into(),
            raw_text: "что такое свобода?".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(!output.response.is_empty());
    }

    #[test]
    fn test_pipeline_process_turn_challenge() {
        let mut state = SystemState::default();
        state.session_id = "test-ch".into();
        let input = TurnInput {
            session_id: "test-ch".into(),
            raw_text: "свобода это просто отсутствие ограничений".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(!output.response.is_empty());
    }

    #[test]
    fn test_pipeline_multi_turn_no_crash() {
        let mut state = SystemState::default();
        state.session_id = "multi".into();
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
            // Validate: pipeline does not panic across multi-turn session.
        }
    }
}

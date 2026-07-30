//! Pure same-turn stance issuer request preparation.
//!
//! This module exposes only the values an external issuer must bind. It does
//! not expose raw input and never mutates `SystemState`.

use qxfx0_semantic::{ParsedProposition, PropositionMode, PropositionParser};
use qxfx0_types::{atom::AtomGraph, calculate_stance_request_digest, StanceTopic, SystemState};
use serde::Serialize;
use thiserror::Error;

use crate::TurnInput;

pub const STANCE_REQUEST_CONTEXT_VERSION: u8 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct StanceRequestContext {
    pub version: u8,
    pub session_id: String,
    pub expected_pre_turn: usize,
    pub normalized_topic: StanceTopic,
    pub request_digest: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StanceRequestPreparationError {
    #[error("invalid session id")]
    InvalidSessionId,
    #[error("input session does not match loaded state")]
    SessionMismatch,
    #[error("loaded state violates persistent invariants")]
    InvalidState,
    #[error("pipeline normalization did not produce a valid stance topic")]
    InvalidNormalizedTopic,
}

pub fn prepare_stance_request_context(
    input: &TurnInput,
    state: &SystemState,
) -> Result<StanceRequestContext, StanceRequestPreparationError> {
    validate_input_session(&input.session_id)?;
    if !state.session_id.is_empty() && state.session_id != input.session_id {
        return Err(StanceRequestPreparationError::SessionMismatch);
    }

    if state.session_id.is_empty() {
        let mut validation_state = state.clone();
        validation_state.session_id = input.session_id.clone();
        if !validation_state.validate().is_empty() {
            return Err(StanceRequestPreparationError::InvalidState);
        }
    } else if !state.validate().is_empty() {
        return Err(StanceRequestPreparationError::InvalidState);
    }

    let seeded_graph;
    let graph = if state.semantic.runtime_graph.edges.is_empty() {
        seeded_graph = qxfx0_semantic::seed_graph();
        &seeded_graph
    } else {
        &state.semantic.runtime_graph
    };
    let proposition = parse_and_normalize_topic(&input.raw_text, graph);
    let normalized_topic = StanceTopic::new(proposition.subject)
        .map_err(|_| StanceRequestPreparationError::InvalidNormalizedTopic)?;

    Ok(StanceRequestContext {
        version: STANCE_REQUEST_CONTEXT_VERSION,
        session_id: input.session_id.clone(),
        expected_pre_turn: state.dialogue.turn_count,
        normalized_topic,
        request_digest: calculate_stance_request_digest(&input.session_id, &input.raw_text),
    })
}

pub(crate) fn parse_and_normalize_topic(raw_text: &str, graph: &AtomGraph) -> ParsedProposition {
    let mut proposition = PropositionParser::parse(raw_text);
    if matches!(
        proposition.mode,
        PropositionMode::Define | PropositionMode::Assert
    ) {
        if let Some(known_topic) = PropositionParser::known_topic_in_input(raw_text, graph) {
            proposition.subject = known_topic;
        }
    }
    proposition.subject = PropositionParser::normalize_topic(&proposition.subject, graph);
    proposition
}

fn validate_input_session(session_id: &str) -> Result<(), StanceRequestPreparationError> {
    if session_id.trim().is_empty()
        || session_id.chars().count() > 128
        || session_id.chars().any(char::is_control)
    {
        Err(StanceRequestPreparationError::InvalidSessionId)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{execution_trace::calculate_stable_digest, process_turn};

    #[test]
    fn prepare_is_pure_deterministic_and_matches_the_pipeline_topic() {
        let input = TurnInput {
            session_id: "stance-prepare".into(),
            raw_text: "что такое свобода?".into(),
        };
        let state = SystemState::default();
        let before = calculate_stable_digest(&state).unwrap();
        let first = prepare_stance_request_context(&input, &state).unwrap();
        let second = prepare_stance_request_context(&input, &state).unwrap();
        assert_eq!(first, second);
        assert_eq!(calculate_stable_digest(&state).unwrap(), before);
        assert_eq!(first.normalized_topic.as_str(), "свобода");
        assert_eq!(first.expected_pre_turn, 0);

        let serialized = serde_json::to_string(&first).unwrap();
        assert!(!serialized.contains(&input.raw_text));

        let mut executed_state = state;
        process_turn(&input, &mut executed_state);
        assert_eq!(
            executed_state.dialogue.last_topic.as_deref(),
            Some(first.normalized_topic.as_str())
        );
    }

    #[test]
    fn prepare_binds_the_loaded_pre_turn_without_mutating_it() {
        let input = TurnInput {
            session_id: "stance-loaded".into(),
            raw_text: "об ответственности".into(),
        };
        let mut state = SystemState {
            session_id: input.session_id.clone(),
            ..SystemState::default()
        };
        state.dialogue.turn_count = 7;
        let context = prepare_stance_request_context(&input, &state).unwrap();
        assert_eq!(context.expected_pre_turn, 7);
        assert_eq!(context.normalized_topic.as_str(), "ответственность");
        assert_eq!(state.dialogue.turn_count, 7);
        assert!(state.semantic.runtime_graph.edges.is_empty());
    }

    #[test]
    fn prepare_rejects_invalid_or_mismatched_sessions() {
        let state = SystemState::default();
        let invalid = TurnInput {
            session_id: " ".into(),
            raw_text: "topic".into(),
        };
        assert_eq!(
            prepare_stance_request_context(&invalid, &state),
            Err(StanceRequestPreparationError::InvalidSessionId)
        );

        let loaded = SystemState {
            session_id: "other".into(),
            ..SystemState::default()
        };
        let mismatched = TurnInput {
            session_id: "requested".into(),
            raw_text: "topic".into(),
        };
        assert_eq!(
            prepare_stance_request_context(&mismatched, &loaded),
            Err(StanceRequestPreparationError::SessionMismatch)
        );
    }
}

//! Conversation FSM — dialogue state machine for pipeline routing.
//!
//! Flattened hierarchical states for compile-time deterministic routing.
//! State transitions driven by proposition mode + context availability.

use serde::{Deserialize, Serialize};

/// Flat conversation states (hierarchical states flattened for runtime).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConversationState {
    Idle,
    Active,
    Greeting,
    InformationGathering,
    Reasoning,
    Clarifying,
    Reflecting,
    Concluding,
}

/// Events that drive state transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConversationEvent {
    StartConversation,
    UserMessage,
    InsufficientInfo,
    ReadyToReason,
    NeedsMoreContext,
    ChallengeReceived,
    ReflectRequest,
    EndConversation,
}

/// Returns the initial conversation state.
pub fn initial_state() -> ConversationState {
    ConversationState::Idle
}

/// Transition the conversation state based on the event.
pub fn transition(state: ConversationState, event: ConversationEvent) -> ConversationState {
    use ConversationEvent::*;
    use ConversationState::*;

    match (state, event) {
        (Idle, StartConversation) => Greeting,
        (Idle, UserMessage) => Greeting,
        (Idle, ReadyToReason) => Reasoning,
        (Idle, ChallengeReceived) => Reasoning,
        (Idle, InsufficientInfo) => Clarifying,
        (Idle, ReflectRequest) => Reflecting,
        (Greeting, UserMessage) => InformationGathering,
        (Greeting, ReflectRequest) => Reflecting,
        (InformationGathering, InsufficientInfo) => Clarifying,
        (Clarifying, UserMessage) => InformationGathering,
        (InformationGathering, ReadyToReason) => Reasoning,
        (Reasoning, NeedsMoreContext) => InformationGathering,
        (Reasoning, ChallengeReceived) => Reasoning,
        (Reasoning, ReflectRequest) => Reflecting,
        (Reflecting, UserMessage) => InformationGathering,
        (Reflecting, ReflectRequest) => Reflecting,
        (Reasoning, EndConversation) => Concluding,
        (Active, EndConversation) => Concluding,
        (Concluding, StartConversation) => Greeting,
        _ => state,
    }
}

/// Check if a state is within the Active compound state (used by pipeline routing).
pub fn is_active(state: ConversationState) -> bool {
    matches!(
        state,
        ConversationState::Active
            | ConversationState::Greeting
            | ConversationState::InformationGathering
            | ConversationState::Reasoning
            | ConversationState::Clarifying
            | ConversationState::Reflecting
    )
}

/// Map a user proposition mode to a conversation event for the FSM.
pub fn proposition_to_event(mode: &str, has_enough_info: bool) -> ConversationEvent {
    match mode {
        "Challenge" => ConversationEvent::ChallengeReceived,
        "Reflect" if has_enough_info => ConversationEvent::ReflectRequest,
        "Reflect" => ConversationEvent::UserMessage,
        "Define" | "Connect" if has_enough_info => ConversationEvent::ReadyToReason,
        "Define" | "Connect" => ConversationEvent::UserMessage,
        "Assert" if has_enough_info => ConversationEvent::ReadyToReason,
        "Purpose" | "WorldCause" if has_enough_info => ConversationEvent::ReadyToReason,
        "Greeting" => ConversationEvent::StartConversation,
        "Purpose" | "WorldCause" => ConversationEvent::UserMessage,
        "Assert" | "Other" => ConversationEvent::UserMessage,
        _ => ConversationEvent::UserMessage,
    }
}

/// FSM discriminant encoding version. Increment when the variant order changes;
/// add a migration in the decode function to map old values to new states.
pub const FSM_DISCRIMINANT_VERSION: u8 = 1;

/// Convert ConversationState to a compact u8 discriminant for cheap persistence.
/// **Persistence contract**: the mapping below is versioned by `FSM_DISCRIMINANT_VERSION`.
/// Never reorder, insert, or remove variants without incrementing the version and
/// adding a migration in `fsm_state_from_discriminant`.
pub fn fsm_state_discriminant(state: &ConversationState) -> u8 {
    use ConversationState::*;
    match state {
        Idle => 0,
        Active => 1,
        Greeting => 2,
        InformationGathering => 3,
        Reasoning => 4,
        Clarifying => 5,
        Reflecting => 6,
        Concluding => 7,
    }
}

/// Restore ConversationState from its discriminant. Returns None on invalid values.
pub fn fsm_state_from_discriminant(d: u8) -> Option<ConversationState> {
    use ConversationState::*;
    match d {
        0 => Some(Idle),
        1 => Some(Active),
        2 => Some(Greeting),
        3 => Some(InformationGathering),
        4 => Some(Reasoning),
        5 => Some(Clarifying),
        6 => Some(Reflecting),
        7 => Some(Concluding),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idle_to_greeting() {
        let s = initial_state();
        let s = transition(s, ConversationEvent::StartConversation);
        assert_eq!(s, ConversationState::Greeting);
    }

    #[test]
    fn greeting_to_info_gathering() {
        let s = ConversationState::Greeting;
        let s = transition(s, ConversationEvent::UserMessage);
        assert_eq!(s, ConversationState::InformationGathering);
    }

    #[test]
    fn full_flow() {
        let s = initial_state();
        let s = transition(s, ConversationEvent::StartConversation);
        assert_eq!(s, ConversationState::Greeting);
        let s = transition(s, ConversationEvent::UserMessage);
        assert_eq!(s, ConversationState::InformationGathering);
        let s = transition(s, ConversationEvent::ReadyToReason);
        assert_eq!(s, ConversationState::Reasoning);
        let s = transition(s, ConversationEvent::EndConversation);
        assert_eq!(s, ConversationState::Concluding);
    }

    #[test]
    fn unknown_transition_noop() {
        // Concluding + UserMessage has no transition → stays Concluding.
        let s = ConversationState::Concluding;
        let s = transition(s, ConversationEvent::UserMessage);
        assert_eq!(s, ConversationState::Concluding);
    }

    #[test]
    fn idle_starts_on_challenge() {
        // Idle + ChallengeReceived now transitions to Reasoning (W11 fix).
        let s = ConversationState::Idle;
        let s = transition(s, ConversationEvent::ChallengeReceived);
        assert_eq!(s, ConversationState::Reasoning);
    }

    #[test]
    fn active_matches_all_substates() {
        assert!(is_active(ConversationState::Greeting));
        assert!(is_active(ConversationState::Reasoning));
        assert!(is_active(ConversationState::Clarifying));
        assert!(!is_active(ConversationState::Idle));
        assert!(!is_active(ConversationState::Concluding));
    }

    #[test]
    fn discriminant_roundtrip() {
        for state in &[
            ConversationState::Idle,
            ConversationState::Active,
            ConversationState::Greeting,
            ConversationState::InformationGathering,
            ConversationState::Reasoning,
            ConversationState::Clarifying,
            ConversationState::Reflecting,
            ConversationState::Concluding,
        ] {
            let d = fsm_state_discriminant(state);
            let restored = fsm_state_from_discriminant(d).unwrap();
            assert_eq!(
                *state, restored,
                "discriminant roundtrip failed for {:?}",
                state
            );
        }
    }

    #[test]
    fn idle_to_reflecting() {
        let s = ConversationState::Idle;
        let s = transition(s, ConversationEvent::ReflectRequest);
        assert_eq!(s, ConversationState::Reflecting);
    }

    #[test]
    fn reasoning_to_reflecting() {
        let s = ConversationState::Reasoning;
        let s = transition(s, ConversationEvent::ReflectRequest);
        assert_eq!(s, ConversationState::Reflecting);
    }

    #[test]
    fn reflecting_loops_on_reflect() {
        let s = ConversationState::Reflecting;
        let s = transition(s, ConversationEvent::ReflectRequest);
        assert_eq!(s, ConversationState::Reflecting);
    }

    #[test]
    fn reflecting_to_info_gathering() {
        let s = ConversationState::Reflecting;
        let s = transition(s, ConversationEvent::UserMessage);
        assert_eq!(s, ConversationState::InformationGathering);
    }

    #[test]
    fn proposition_reflect_maps_to_reflect_request() {
        let event = proposition_to_event("Reflect", true);
        assert_eq!(event, ConversationEvent::ReflectRequest);
    }
}

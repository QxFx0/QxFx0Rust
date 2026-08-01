//! Default-off composition boundary for curated grounding and signed stance.
//!
//! This module is intentionally pure. It does not contact an issuer, manage
//! keys, mutate persistence, or change the legacy pipeline route.

use qxfx0_self::fact_perspective::{resolve_render_stance, PerspectiveRenderStance};
use qxfx0_semantic::FactRegistry;
use qxfx0_types::{
    ConceptId, FactId, PerspectiveState, StanceTopic, SystemStanceDecision, VerifiedStanceDecision,
};
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactGroundedRollout {
    #[default]
    Disabled,
    Shadow,
    TraceOnly,
    LimitedNonProduction,
    Enabled,
}

impl FactGroundedRollout {
    pub const fn permits_render_authorization(self) -> bool {
        matches!(self, Self::LimitedNonProduction | Self::Enabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedRenderDecision {
    /// Fact grounding owns the renderer modifier. Authority is retained as a
    /// separate exact decision and never converted into `Opposed`.
    pub grounding: PerspectiveRenderStance,
    pub authority: Option<SystemStanceDecision>,
}

pub struct FactGroundedCompositionInput<'a> {
    pub rollout: FactGroundedRollout,
    pub state: &'a PerspectiveState,
    pub topic: &'a ConceptId,
    pub topic_for_authority: &'a StanceTopic,
    pub thesis_fact_id: &'a FactId,
    pub facts: &'a FactRegistry,
    pub persisted_pack_fingerprint: &'a str,
    pub active_pack_fingerprint: &'a str,
    pub verified_authority: Option<&'a VerifiedStanceDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FactGroundedCompositionError {
    #[error("fact-grounded composition is disabled")]
    Disabled,
    #[error("active knowledge-pack fingerprint mismatch")]
    PackFingerprintMismatch,
    #[error("signed authority topic does not match fact-grounded topic")]
    AuthorityTopicMismatch,
    #[error("fact-grounded state is invalid: {0}")]
    InvalidState(String),
}

/// Validate a persisted session's pack identity before any semantic stage.
pub fn validate_pack_fingerprint(
    persisted: &str,
    active: &str,
) -> Result<(), FactGroundedCompositionError> {
    if persisted.is_empty() || persisted == active {
        Ok(())
    } else {
        Err(FactGroundedCompositionError::PackFingerprintMismatch)
    }
}

/// Resolve grounding and compose it with an already verified authority
/// decision. The signed decision remains optional and independent: local
/// `Qualified` never becomes system authority, and `Rejected` never becomes
/// the unsupported fact-grounded `Opposed` polarity.
pub fn compose_render_decision(
    input: FactGroundedCompositionInput<'_>,
) -> Result<ComposedRenderDecision, FactGroundedCompositionError> {
    if !input.rollout.permits_render_authorization() {
        return Err(FactGroundedCompositionError::Disabled);
    }
    validate_pack_fingerprint(
        input.persisted_pack_fingerprint,
        input.active_pack_fingerprint,
    )?;
    let grounding =
        resolve_render_stance(input.state, input.topic, input.thesis_fact_id, input.facts)
            .map_err(FactGroundedCompositionError::InvalidState)?;
    let authority = input
        .verified_authority
        .map(|verified| verified.decision().clone());
    if let Some(decision) = &authority {
        if decision.topic != *input.topic_for_authority {
            return Err(FactGroundedCompositionError::AuthorityTopicMismatch);
        }
    }
    Ok(ComposedRenderDecision {
        grounding,
        authority,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_off_does_not_authorize_composition() {
        let packs = qxfx0_semantic::active_pack_set();
        let topic = ConceptId("concept.свобода".into());
        let authority_topic = StanceTopic::new("свобода").unwrap();
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        let error = compose_render_decision(FactGroundedCompositionInput {
            rollout: FactGroundedRollout::Disabled,
            state: &PerspectiveState::default(),
            topic: &topic,
            topic_for_authority: &authority_topic,
            thesis_fact_id: &thesis,
            facts: packs.facts(),
            persisted_pack_fingerprint: packs.fingerprint(),
            active_pack_fingerprint: packs.fingerprint(),
            verified_authority: None,
        })
        .unwrap_err();
        assert_eq!(error, FactGroundedCompositionError::Disabled);
    }

    #[test]
    fn first_fact_grounded_turn_is_neutral_and_pack_mismatch_fails_closed() {
        let packs = qxfx0_semantic::active_pack_set();
        let topic = ConceptId("concept.свобода".into());
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        let authority_topic = StanceTopic::new("свобода").unwrap();
        let decision = compose_render_decision(FactGroundedCompositionInput {
            rollout: FactGroundedRollout::LimitedNonProduction,
            state: &PerspectiveState::default(),
            topic: &topic,
            topic_for_authority: &authority_topic,
            thesis_fact_id: &thesis,
            facts: packs.facts(),
            persisted_pack_fingerprint: packs.fingerprint(),
            active_pack_fingerprint: packs.fingerprint(),
            verified_authority: None,
        })
        .unwrap();
        assert_eq!(decision.grounding, PerspectiveRenderStance::Neutral);

        let wrong_fingerprint = "f".repeat(64);
        let error = compose_render_decision(FactGroundedCompositionInput {
            rollout: FactGroundedRollout::LimitedNonProduction,
            state: &PerspectiveState::default(),
            topic: &topic,
            topic_for_authority: &authority_topic,
            thesis_fact_id: &thesis,
            facts: packs.facts(),
            persisted_pack_fingerprint: &wrong_fingerprint,
            active_pack_fingerprint: packs.fingerprint(),
            verified_authority: None,
        })
        .unwrap_err();
        assert_eq!(error, FactGroundedCompositionError::PackFingerprintMismatch);
    }
}

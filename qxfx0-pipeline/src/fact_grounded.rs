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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactGroundedRollout {
    Disabled,
    Shadow,
    TraceOnly,
    LimitedNonProduction,
    Enabled,
}

impl Default for FactGroundedRollout {
    fn default() -> Self {
        Self::Disabled
    }
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
    rollout: FactGroundedRollout,
    state: &PerspectiveState,
    topic: &ConceptId,
    topic_for_authority: &StanceTopic,
    thesis_fact_id: &FactId,
    facts: &FactRegistry,
    persisted_pack_fingerprint: &str,
    active_pack_fingerprint: &str,
    verified_authority: Option<&VerifiedStanceDecision>,
) -> Result<ComposedRenderDecision, FactGroundedCompositionError> {
    if !rollout.permits_render_authorization() {
        return Err(FactGroundedCompositionError::Disabled);
    }
    validate_pack_fingerprint(persisted_pack_fingerprint, active_pack_fingerprint)?;
    let grounding = resolve_render_stance(state, topic, thesis_fact_id, facts)
        .map_err(FactGroundedCompositionError::InvalidState)?;
    let authority = verified_authority.map(|verified| verified.decision().clone());
    if let Some(decision) = &authority {
        if decision.topic != *topic_for_authority {
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
        let error = compose_render_decision(
            FactGroundedRollout::Disabled,
            &PerspectiveState::default(),
            &ConceptId("concept.свобода".into()),
            &StanceTopic::new("свобода").unwrap(),
            &FactId::try_new("fact.freedom_choice").unwrap(),
            packs.facts(),
            packs.fingerprint(),
            packs.fingerprint(),
            None,
        )
        .unwrap_err();
        assert_eq!(error, FactGroundedCompositionError::Disabled);
    }

    #[test]
    fn first_fact_grounded_turn_is_neutral_and_pack_mismatch_fails_closed() {
        let packs = qxfx0_semantic::active_pack_set();
        let topic = ConceptId("concept.свобода".into());
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        let decision = compose_render_decision(
            FactGroundedRollout::LimitedNonProduction,
            &PerspectiveState::default(),
            &topic,
            &StanceTopic::new("свобода").unwrap(),
            &thesis,
            packs.facts(),
            packs.fingerprint(),
            packs.fingerprint(),
            None,
        )
        .unwrap();
        assert_eq!(decision.grounding, PerspectiveRenderStance::Neutral);

        let error = compose_render_decision(
            FactGroundedRollout::LimitedNonProduction,
            &PerspectiveState::default(),
            &topic,
            &StanceTopic::new("свобода").unwrap(),
            &thesis,
            packs.facts(),
            &"f".repeat(64),
            packs.fingerprint(),
            None,
        )
        .unwrap_err();
        assert_eq!(error, FactGroundedCompositionError::PackFingerprintMismatch);
    }
}

//! Fact-grounded composition and post-render Perspective integration.
//!
//! Topic identity, knowledge-pack identity, and rendered-plan evidence are
//! represented by closed capabilities so callers cannot assemble unrelated
//! facts, fingerprints, topics, or plans.

use crate::turn_context::{RenderedTurnContext, RendererSource};
use qxfx0_self::fact_perspective::{
    integrate_curated_claims, resolve_render_stance, PerspectiveRenderStance, PerspectiveUpdate,
};
use qxfx0_semantic::{
    ClaimRole, KnowledgePackSet, PlanOutcome, PlanSubject, ReadyResponsePlan, ResolutionOutcome,
};
use qxfx0_types::{
    AtomId, ConceptId, FactId, PerspectiveState, StanceTopic, SystemStanceDecision, SystemState,
    VerifiedStanceDecision,
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

    pub const fn observes(self) -> bool {
        !matches!(self, Self::Disabled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedTopicBinding {
    concept_id: ConceptId,
    stance_topic: StanceTopic,
    plan_subject: AtomId,
    thesis_fact_id: FactId,
}

impl ResolvedTopicBinding {
    pub fn from_plan(
        plan: &ReadyResponsePlan,
        packs: &KnowledgePackSet,
    ) -> Result<Self, FactGroundedCompositionError> {
        plan.validate_with_facts(packs.facts())
            .map_err(FactGroundedCompositionError::InvalidPlan)?;
        let PlanSubject::Topic(plan_subject) = plan.subject() else {
            return Err(FactGroundedCompositionError::NonTopicPlan);
        };
        let resolved = match packs.resolver().resolve(plan_subject.as_str()) {
            ResolutionOutcome::Resolved(resolved) if resolved.atom_id == *plan_subject => resolved,
            ResolutionOutcome::Resolved(_) | ResolutionOutcome::Ambiguous(_) => {
                return Err(FactGroundedCompositionError::PlanSubjectMismatch)
            }
            ResolutionOutcome::Unknown => {
                return Err(FactGroundedCompositionError::UnknownPlanSubject)
            }
        };
        let mut theses = plan
            .claims()
            .iter()
            .filter(|claim| claim.role() == ClaimRole::Thesis);
        let thesis = theses
            .next()
            .ok_or(FactGroundedCompositionError::MissingThesis)?;
        if theses.next().is_some() {
            return Err(FactGroundedCompositionError::MultipleTheses);
        }
        let thesis_fact_id = thesis
            .fact_id()
            .cloned()
            .ok_or(FactGroundedCompositionError::MissingThesis)?;
        let fact = packs
            .facts()
            .select(&thesis_fact_id)
            .map_err(|error| FactGroundedCompositionError::InvalidPlan(error.to_string()))?;
        if fact.subject != resolved.concept_id {
            return Err(FactGroundedCompositionError::ThesisTopicMismatch);
        }
        let stance_topic = StanceTopic::new(plan_subject.as_str())
            .map_err(|error| FactGroundedCompositionError::InvalidPlan(error.to_string()))?;
        Ok(Self {
            concept_id: resolved.concept_id,
            stance_topic,
            plan_subject: plan_subject.clone(),
            thesis_fact_id,
        })
    }

    pub fn concept_id(&self) -> &ConceptId {
        &self.concept_id
    }

    pub fn stance_topic(&self) -> &StanceTopic {
        &self.stance_topic
    }

    pub fn plan_subject(&self) -> &AtomId {
        &self.plan_subject
    }

    pub fn thesis_fact_id(&self) -> &FactId {
        &self.thesis_fact_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenderedPlanReceipt {
    binding: ResolvedTopicBinding,
    claims: Vec<(ClaimRole, FactId)>,
    session_id: String,
    pre_turn_seq: usize,
    pack_fingerprint: String,
    response_digest: String,
}

impl RenderedPlanReceipt {
    pub(crate) fn from_rendered(
        rendered: &RenderedTurnContext,
        state: &SystemState,
        packs: &KnowledgePackSet,
    ) -> Result<Option<Self>, FactGroundedCompositionError> {
        if !matches!(rendered.renderer_source(), RendererSource::AuditedPlan) {
            return Ok(None);
        }
        let PlanOutcome::Ready(plan) = rendered.planned().shadow_plan() else {
            return Err(FactGroundedCompositionError::RenderedPlanMissing);
        };
        let binding = ResolvedTopicBinding::from_plan(plan, packs)?;
        let claims = plan
            .claims()
            .iter()
            .filter(|claim| claim.role() != ClaimRole::DialogueAct)
            .map(|claim| {
                claim
                    .fact_id()
                    .cloned()
                    .map(|fact_id| (claim.role(), fact_id))
                    .ok_or_else(|| {
                        FactGroundedCompositionError::InvalidPlan(format!(
                            "declarative claim '{}' has no FactId",
                            claim.id().as_str()
                        ))
                    })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let response_digest = crate::execution_trace::calculate_stable_digest(&rendered.response())
            .map_err(|error| FactGroundedCompositionError::InvalidPlan(error.to_string()))?;
        Ok(Some(Self {
            binding,
            claims,
            session_id: state.session_id.clone(),
            pre_turn_seq: state.dialogue.turn_count,
            pack_fingerprint: packs.fingerprint().into(),
            response_digest,
        }))
    }

    pub fn binding(&self) -> &ResolvedTopicBinding {
        &self.binding
    }

    pub fn response_digest(&self) -> &str {
        &self.response_digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComposedRenderDecision {
    pub grounding: PerspectiveRenderStance,
    pub authority: Option<SystemStanceDecision>,
}

pub struct FactGroundedCompositionInput<'a> {
    pub rollout: FactGroundedRollout,
    pub state: &'a SystemState,
    pub packs: &'a KnowledgePackSet,
    pub binding: &'a ResolvedTopicBinding,
    pub verified_authority: Option<&'a VerifiedStanceDecision>,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FactGroundedCompositionError {
    #[error("fact-grounded composition is disabled")]
    Disabled,
    #[error("active knowledge-pack fingerprint mismatch")]
    PackFingerprintMismatch,
    #[error("non-empty Perspective has no knowledge-pack fingerprint")]
    MissingPackFingerprint,
    #[error("signed authority topic does not match fact-grounded topic")]
    AuthorityTopicMismatch,
    #[error("ready plan is not topic-backed")]
    NonTopicPlan,
    #[error("ready plan subject is unknown to the active pack set")]
    UnknownPlanSubject,
    #[error("ready plan subject does not match its resolved concept")]
    PlanSubjectMismatch,
    #[error("ready plan has no thesis FactId")]
    MissingThesis,
    #[error("ready plan has more than one thesis")]
    MultipleTheses,
    #[error("thesis FactId belongs to another topic")]
    ThesisTopicMismatch,
    #[error("audited renderer did not retain a ready plan")]
    RenderedPlanMissing,
    #[error("rendered-plan receipt belongs to another session or turn")]
    ReceiptTurnMismatch,
    #[error("rendered-plan receipt belongs to another knowledge-pack set")]
    ReceiptPackMismatch,
    #[error("fact-grounded plan is invalid: {0}")]
    InvalidPlan(String),
    #[error("fact-grounded state is invalid: {0}")]
    InvalidState(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FactGroundedFinalize {
    Skipped(FactGroundedRollout),
    Observed {
        rollout: FactGroundedRollout,
        claim_count: usize,
    },
    Applied(PerspectiveUpdate),
}

pub fn finalize_fact_grounded_state(
    rollout: FactGroundedRollout,
    state: &mut SystemState,
    receipt: &RenderedPlanReceipt,
    packs: &KnowledgePackSet,
) -> Result<FactGroundedFinalize, FactGroundedCompositionError> {
    if !rollout.observes() {
        return Ok(FactGroundedFinalize::Skipped(rollout));
    }
    validate_pack_fingerprint(
        &state.semantic.perspective,
        &state.semantic.pack_set_fingerprint,
        packs,
    )?;
    if receipt.pack_fingerprint != packs.fingerprint() {
        return Err(FactGroundedCompositionError::ReceiptPackMismatch);
    }
    if receipt.session_id != state.session_id || receipt.pre_turn_seq != state.dialogue.turn_count {
        return Err(FactGroundedCompositionError::ReceiptTurnMismatch);
    }
    if !rollout.permits_render_authorization() {
        return Ok(FactGroundedFinalize::Observed {
            rollout,
            claim_count: receipt.claims.len(),
        });
    }
    let turn_seq = state.dialogue.turn_count + 1;
    let (next_perspective, update) = integrate_curated_claims(
        &state.semantic.perspective,
        turn_seq,
        &receipt.claims,
        packs.facts(),
    )
    .map_err(FactGroundedCompositionError::InvalidState)?;
    state.semantic.perspective = next_perspective;
    if state.semantic.pack_set_fingerprint.is_empty() {
        state.semantic.pack_set_fingerprint = packs.fingerprint().into();
    }
    Ok(FactGroundedFinalize::Applied(update))
}

pub fn validate_pack_fingerprint(
    perspective: &PerspectiveState,
    persisted: &str,
    packs: &KnowledgePackSet,
) -> Result<(), FactGroundedCompositionError> {
    if persisted.is_empty() {
        if perspective.opinions.is_empty() && perspective.episodes.is_empty() {
            return Ok(());
        }
        return Err(FactGroundedCompositionError::MissingPackFingerprint);
    }
    if persisted == packs.fingerprint() {
        Ok(())
    } else {
        Err(FactGroundedCompositionError::PackFingerprintMismatch)
    }
}

pub fn compose_render_decision(
    input: FactGroundedCompositionInput<'_>,
) -> Result<ComposedRenderDecision, FactGroundedCompositionError> {
    if !input.rollout.permits_render_authorization() {
        return Err(FactGroundedCompositionError::Disabled);
    }
    validate_pack_fingerprint(
        &input.state.semantic.perspective,
        &input.state.semantic.pack_set_fingerprint,
        input.packs,
    )?;
    let grounding = resolve_render_stance(
        &input.state.semantic.perspective,
        input.binding.concept_id(),
        input.binding.thesis_fact_id(),
        input.packs.facts(),
    )
    .map_err(FactGroundedCompositionError::InvalidState)?;
    let authority = input
        .verified_authority
        .map(|verified| verified.decision().clone());
    if let Some(decision) = &authority {
        if decision.topic != *input.binding.stance_topic() {
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
    use crate::shadow_plan::build_shadow_plan;
    use crate::stages::{plan_shadow_stage, prepare_stage, render_stage, route_stage};
    use crate::turn_context::TurnInputContext;
    use crate::{detect_challenge, RendererAuthority};

    struct AcceptingSignatureVerifier;

    impl qxfx0_types::StanceDecisionSignatureVerifier for AcceptingSignatureVerifier {
        fn verify_signature(
            &self,
            _issuer_id: &str,
            _key_id: &str,
            canonical_payload: &[u8],
            _signature: &[u8; 64],
        ) -> Result<(), qxfx0_types::StanceVerificationError> {
            assert!(!canonical_payload.is_empty());
            Ok(())
        }
    }

    fn rendered_freedom(state: &mut SystemState) -> RenderedTurnContext {
        if state.semantic.runtime_graph.edges.is_empty() {
            state.semantic.runtime_graph = qxfx0_semantic::seed_graph();
        }
        let proposition = crate::stance_request::parse_and_normalize_topic(
            "что такое свобода?",
            &state.semantic.runtime_graph,
        );
        let input = TurnInputContext::new(
            state.session_id.clone(),
            "что такое свобода?".into(),
            proposition,
            detect_challenge("что такое свобода?"),
        );
        let prepared = prepare_stage(state, input).unwrap();
        let routed = route_stage(state, prepared, false).unwrap();
        let plan = build_shadow_plan(&routed).unwrap();
        let planned = crate::turn_context::PlannedTurnContext::new(routed, plan);
        render_stage(state, planned, RendererAuthority::AuditedPlan).unwrap()
    }

    #[test]
    fn binding_derives_one_topic_from_plan_and_pack() {
        let packs = qxfx0_semantic::active_pack_set();
        let mut state = SystemState {
            session_id: "binding".into(),
            ..Default::default()
        };
        let rendered = rendered_freedom(&mut state);
        let receipt = RenderedPlanReceipt::from_rendered(&rendered, &state, packs)
            .unwrap()
            .unwrap();
        assert_eq!(
            receipt.binding().concept_id(),
            &ConceptId("concept.свобода".into())
        );
        assert_eq!(receipt.binding().stance_topic().as_str(), "свобода");
        assert_eq!(receipt.binding().plan_subject().as_str(), "свобода");
    }

    #[test]
    fn non_empty_perspective_requires_exact_pack_identity() {
        let packs = qxfx0_semantic::active_pack_set();
        let claims = vec![(
            ClaimRole::Thesis,
            FactId::try_new("fact.freedom_choice").unwrap(),
        )];
        let (perspective, _) =
            integrate_curated_claims(&PerspectiveState::default(), 1, &claims, packs.facts())
                .unwrap();
        assert_eq!(
            validate_pack_fingerprint(&perspective, "", packs).unwrap_err(),
            FactGroundedCompositionError::MissingPackFingerprint
        );
    }

    #[test]
    fn receipt_is_issued_only_for_successful_audited_plan_render() {
        let packs = qxfx0_semantic::active_pack_set();
        let mut state = SystemState {
            session_id: "receipt".into(),
            ..Default::default()
        };
        let rendered = rendered_freedom(&mut state);
        assert!(RenderedPlanReceipt::from_rendered(&rendered, &state, packs)
            .unwrap()
            .is_some());

        let input = TurnInputContext::new(
            state.session_id.clone(),
            "что такое свобода?".into(),
            crate::stance_request::parse_and_normalize_topic(
                "что такое свобода?",
                &state.semantic.runtime_graph,
            ),
            false,
        );
        let prepared = prepare_stage(&mut state, input).unwrap();
        let routed = route_stage(&mut state, prepared, false).unwrap();
        let planned = plan_shadow_stage(&mut state, routed).unwrap();
        let rendered = render_stage(&mut state, planned, RendererAuthority::LegacyShadow).unwrap();
        assert!(RenderedPlanReceipt::from_rendered(&rendered, &state, packs)
            .unwrap()
            .is_none());
    }

    #[test]
    fn signed_truth_authority_cannot_bind_to_freedom_thesis() {
        let packs = qxfx0_semantic::active_pack_set();
        let mut state = SystemState {
            session_id: "topic-mismatch".into(),
            ..Default::default()
        };
        let rendered = rendered_freedom(&mut state);
        let receipt = RenderedPlanReceipt::from_rendered(&rendered, &state, packs)
            .unwrap()
            .unwrap();
        let request_digest =
            qxfx0_types::calculate_stance_request_digest(&state.session_id, "что такое свобода?");
        let signed = qxfx0_types::SignedStanceDecision {
            attestation: qxfx0_types::StanceDecisionAttestation {
                version: qxfx0_types::STANCE_ATTESTATION_VERSION,
                issuer_id: "test-issuer".into(),
                key_id: "test-key".into(),
                audience: "qxfx0-test".into(),
                session_id: state.session_id.clone(),
                expected_pre_turn: state.dialogue.turn_count,
                topic: StanceTopic::new("истина").unwrap(),
                polarity: qxfx0_types::StancePolarity::Affirmed,
                request_digest,
                decision_id: [7; 16],
                issued_at_unix_seconds: 100,
                expires_at_unix_seconds: 200,
            },
            signature: [1; 64],
        };
        let verified = qxfx0_types::verify_signed_stance_decision(
            &AcceptingSignatureVerifier,
            &signed,
            &qxfx0_types::StanceVerificationContext {
                audience: "qxfx0-test".into(),
                session_id: state.session_id.clone(),
                expected_pre_turn: state.dialogue.turn_count,
                request_digest,
                verification_time_unix_seconds: 150,
                max_validity_seconds: 300,
            },
        )
        .unwrap();

        let error = compose_render_decision(FactGroundedCompositionInput {
            rollout: FactGroundedRollout::LimitedNonProduction,
            state: &state,
            packs,
            binding: receipt.binding(),
            verified_authority: Some(&verified),
        })
        .unwrap_err();
        assert_eq!(error, FactGroundedCompositionError::AuthorityTopicMismatch);
    }

    #[test]
    fn first_turn_is_neutral_then_audited_finalize_qualifies_next_composition() {
        let packs = qxfx0_semantic::active_pack_set();
        let mut state = SystemState {
            session_id: "qualified-second-turn".into(),
            ..Default::default()
        };
        let rendered = rendered_freedom(&mut state);
        let receipt = RenderedPlanReceipt::from_rendered(&rendered, &state, packs)
            .unwrap()
            .unwrap();

        let first = compose_render_decision(FactGroundedCompositionInput {
            rollout: FactGroundedRollout::LimitedNonProduction,
            state: &state,
            packs,
            binding: receipt.binding(),
            verified_authority: None,
        })
        .unwrap();
        assert_eq!(first.grounding, PerspectiveRenderStance::Neutral);

        let finalized = finalize_fact_grounded_state(
            FactGroundedRollout::LimitedNonProduction,
            &mut state,
            &receipt,
            packs,
        )
        .unwrap();
        assert!(matches!(finalized, FactGroundedFinalize::Applied(_)));

        let second = compose_render_decision(FactGroundedCompositionInput {
            rollout: FactGroundedRollout::LimitedNonProduction,
            state: &state,
            packs,
            binding: receipt.binding(),
            verified_authority: None,
        })
        .unwrap();
        assert_eq!(second.grounding, PerspectiveRenderStance::Qualified);
    }

    #[test]
    fn receipt_from_another_session_or_turn_is_rejected() {
        let packs = qxfx0_semantic::active_pack_set();
        let mut source = SystemState {
            session_id: "receipt-source".into(),
            ..Default::default()
        };
        let rendered = rendered_freedom(&mut source);
        let receipt = RenderedPlanReceipt::from_rendered(&rendered, &source, packs)
            .unwrap()
            .unwrap();

        let mut other_session = source.clone();
        other_session.session_id = "receipt-other".into();
        assert_eq!(
            finalize_fact_grounded_state(
                FactGroundedRollout::Enabled,
                &mut other_session,
                &receipt,
                packs,
            )
            .unwrap_err(),
            FactGroundedCompositionError::ReceiptTurnMismatch
        );
        assert!(other_session.semantic.perspective.opinions.is_empty());

        let mut other_turn = source;
        other_turn.dialogue.turn_count += 1;
        assert_eq!(
            finalize_fact_grounded_state(
                FactGroundedRollout::Enabled,
                &mut other_turn,
                &receipt,
                packs,
            )
            .unwrap_err(),
            FactGroundedCompositionError::ReceiptTurnMismatch
        );
        assert!(other_turn.semantic.perspective.opinions.is_empty());
    }
}

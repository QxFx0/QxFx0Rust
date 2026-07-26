//! Content-bearing planner observed in shadow mode by the current renderer.

use crate::turn_context::RoutedTurnContext;
use qxfx0_semantic::{
    argued_topic_registry, ArguedTopic, ClaimEvidence, ClaimId, ClaimRole, Confidence,
    DerivationRule, DerivationStep, DialogueObligation, DialogueSubject, DiscoursePlan,
    DiscourseRelation, ExternalSubject, ExternalSubjectKind, FallbackPlan, FallbackReason,
    FallbackSubject, NonEmptyVec, PlanOutcome, PlanSubject, PlanVersion, PlannedClaim,
    PredicateRef, PropositionMode, ReadyResponsePlan, RecoveryEvidence, RecoveryTrace,
    ResponseGoal, SemanticProposition, SentenceBudget,
};

pub type ShadowPlanOutcome = PlanOutcome<ReadyResponsePlan>;

pub(crate) fn build_shadow_plan(routed: &RoutedTurnContext) -> Result<ShadowPlanOutcome, String> {
    let prepared = routed.prepared();
    build_outcome(
        prepared.input().mode(),
        prepared.input().subject(),
        prepared.has_enough(),
    )
}

fn build_outcome(
    mode: PropositionMode,
    subject: &str,
    has_enough: bool,
) -> Result<ShadowPlanOutcome, String> {
    let goal = goal_for_mode(mode);
    if has_enough {
        let registry = argued_topic_registry().map_err(str::to_owned)?;
        return match registry.get(subject) {
            Some(topic) => Ok(PlanOutcome::Ready(build_argued_plan(goal, topic)?)),
            None => Ok(fallback_plan(
                goal,
                Some(FallbackSubject::KnownTopic(qxfx0_types::AtomId::new(
                    subject,
                ))),
                FallbackReason::NoAdmissiblePredicate,
                RecoveryEvidence::PredicateSelection {
                    admissible_count: 0,
                },
            )),
        };
    }

    let trimmed_subject = subject.trim();
    match mode {
        PropositionMode::Greeting => build_contract_plan(
            goal,
            PlanSubject::Dialogue(DialogueSubject::Contact),
            SemanticProposition::DialogueAct(DialogueSubject::Contact),
            "system.dialogue.contact",
            ClaimEvidence::system_contract(),
            DerivationRule::AppliedDialogueContract,
            Some(DialogueObligation::ContinueContact),
        ),
        PropositionMode::Purpose | PropositionMode::WorldCause if !trimmed_subject.is_empty() => {
            let (kind, predicate_id) = match mode {
                PropositionMode::Purpose => {
                    (ExternalSubjectKind::Entity, "system.external.purpose")
                }
                PropositionMode::WorldCause => (
                    ExternalSubjectKind::Phenomenon,
                    "system.external.world_cause",
                ),
                _ => unreachable!("guarded by the outer match"),
            };
            let external = ExternalSubject::new(kind, subject);
            build_contract_plan(
                goal,
                PlanSubject::External(external.clone()),
                SemanticProposition::ExternalReference(external),
                predicate_id,
                ClaimEvidence::user_input(),
                DerivationRule::GroundedExternalReference,
                None,
            )
        }
        PropositionMode::Define
        | PropositionMode::Assert
        | PropositionMode::Challenge
        | PropositionMode::Connect
        | PropositionMode::Reflect
        | PropositionMode::Purpose
        | PropositionMode::WorldCause => {
            let (fallback_subject, cause, evidence) = if trimmed_subject.is_empty() {
                (
                    None,
                    FallbackReason::NoTopicProvided,
                    RecoveryEvidence::InputSubjectEmpty,
                )
            } else {
                (
                    Some(FallbackSubject::UnresolvedTopic(subject.to_owned())),
                    FallbackReason::UnknownTopic,
                    RecoveryEvidence::TopicLookup {
                        subject: subject.to_owned(),
                        found: false,
                    },
                )
            };
            Ok(fallback_plan(goal, fallback_subject, cause, evidence))
        }
    }
}

fn build_argued_plan(goal: ResponseGoal, topic: &ArguedTopic) -> Result<ReadyResponsePlan, String> {
    let evidence = ClaimEvidence::curated(topic.evidence_record());
    let primary_ref = topic.primary_predicate_ref().clone();

    let thesis_id = ClaimId::try_new(format!("{}.thesis", primary_ref.as_str()))?;
    let thesis_refs = NonEmptyVec::one(primary_ref.clone());
    let thesis = PlannedClaim::new(
        thesis_id.clone(),
        ClaimRole::Thesis,
        topic.primary_proposition().clone(),
        thesis_refs.clone(),
        evidence.clone(),
        Confidence::from_basis_points(9_500)?,
    );
    let mut claims = NonEmptyVec::one(thesis);
    let mut derivation = vec![DerivationStep::new(
        thesis_id.clone(),
        thesis_refs,
        DerivationRule::SelectedAdmittedPredicate,
    )];

    let counter_ref = topic.counterpoint().predicate_ref().clone();
    let counter_id = ClaimId::try_new(format!("{}.counterpoint", primary_ref.as_str()))?;
    let mut counter_refs = NonEmptyVec::one(counter_ref.clone());
    counter_refs.push(primary_ref.clone());
    claims.push(PlannedClaim::new(
        counter_id.clone(),
        ClaimRole::Counterpoint,
        SemanticProposition::Counterpoint {
            statement: counter_ref,
            counters: primary_ref.clone(),
        },
        counter_refs.clone(),
        evidence.clone(),
        Confidence::from_basis_points(9_000)?,
    ));
    derivation.push(DerivationStep::new(
        counter_id,
        counter_refs,
        DerivationRule::AddedCounterpoint,
    ));

    let sentence_budget = if let Some(consequence) = topic.consequence() {
        let consequence_ref = consequence.predicate_ref().clone();
        let consequence_id = ClaimId::try_new(format!("{}.consequence", primary_ref.as_str()))?;
        let mut consequence_refs = NonEmptyVec::one(consequence_ref.clone());
        consequence_refs.push(primary_ref.clone());
        claims.push(PlannedClaim::new(
            consequence_id.clone(),
            ClaimRole::Consequence,
            SemanticProposition::Consequence {
                statement: consequence_ref,
                follows_from: primary_ref,
            },
            consequence_refs.clone(),
            evidence,
            Confidence::from_basis_points(9_000)?,
        ));
        derivation.push(DerivationStep::new(
            consequence_id,
            consequence_refs,
            DerivationRule::AddedConsequence,
        ));
        SentenceBudget::Three
    } else {
        SentenceBudget::Two
    };

    ReadyResponsePlan::new(
        goal,
        PlanSubject::Topic(topic.topic().clone()),
        claims,
        DiscoursePlan::new(DiscourseRelation::Counterpoint, sentence_budget),
        Some(DialogueObligation::CheckAgreement {
            claim_id: thesis_id,
        }),
        derivation,
    )
}

fn build_contract_plan(
    goal: ResponseGoal,
    subject: PlanSubject,
    proposition: SemanticProposition,
    predicate_id: &str,
    evidence: ClaimEvidence,
    rule: DerivationRule,
    obligation: Option<DialogueObligation>,
) -> Result<ShadowPlanOutcome, String> {
    let predicate_ref = PredicateRef::try_new(predicate_id)?;
    let claim_id = ClaimId::try_new(format!("{predicate_id}.claim"))?;
    let predicate_refs = NonEmptyVec::one(predicate_ref);
    let claim = PlannedClaim::new(
        claim_id.clone(),
        ClaimRole::DialogueAct,
        proposition,
        predicate_refs.clone(),
        evidence,
        Confidence::from_basis_points(10_000)?,
    );
    let plan = ReadyResponsePlan::new(
        goal,
        subject,
        NonEmptyVec::one(claim),
        DiscoursePlan::new(DiscourseRelation::None, SentenceBudget::One),
        obligation,
        vec![DerivationStep::new(claim_id, predicate_refs, rule)],
    )?;
    Ok(PlanOutcome::Ready(plan))
}

fn fallback_plan(
    goal: ResponseGoal,
    subject: Option<FallbackSubject>,
    cause: FallbackReason,
    evidence: RecoveryEvidence,
) -> ShadowPlanOutcome {
    PlanOutcome::Fallback(FallbackPlan::new(
        PlanVersion::ContentV1,
        goal,
        subject,
        RecoveryTrace::enabled(cause, evidence),
    ))
}

fn goal_for_mode(mode: PropositionMode) -> ResponseGoal {
    match mode {
        PropositionMode::Define => ResponseGoal::Define,
        PropositionMode::Assert => ResponseGoal::GenerateThesis,
        PropositionMode::Challenge => ResponseGoal::Challenge,
        PropositionMode::Connect => ResponseGoal::Compare,
        PropositionMode::Reflect => ResponseGoal::Reflect,
        PropositionMode::Greeting => ResponseGoal::Contact,
        PropositionMode::Purpose => ResponseGoal::Explain,
        PropositionMode::WorldCause => ResponseGoal::Hypothesize,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn admitted_topic_produces_content_plan_without_surface_text() {
        let outcome = build_outcome(PropositionMode::Define, "свобода", true).unwrap();
        let ready = outcome.ready().expect("admitted topic should be ready");

        assert_eq!(ready.goal(), ResponseGoal::Define);
        assert_eq!(ready.version(), PlanVersion::ContentV1);
        assert_eq!(ready.claims().len(), 3);
        assert!(matches!(ready.subject(), PlanSubject::Topic(_)));
        assert_eq!(
            ready.claims().first().predicate_refs().first().as_str(),
            "freedom_choice"
        );
        let encoded = serde_json::to_string(ready).unwrap();
        assert!(!encoded.contains("не любой выбор свободен"));
    }

    #[test]
    fn unknown_topic_produces_typed_fallback() {
        let outcome = build_outcome(PropositionMode::Define, "кванточайник", false).unwrap();
        let fallback = outcome
            .fallback()
            .expect("unknown topic should produce fallback evidence");

        assert_eq!(fallback.reason(), FallbackReason::UnknownTopic);
        assert_eq!(
            fallback.recovery().strategy(),
            qxfx0_semantic::RecoveryStrategy::AskClarification
        );
    }

    #[test]
    fn recognized_but_unadmitted_topic_has_explicit_fallback() {
        let outcome = build_outcome(PropositionMode::Define, "знание", true).unwrap();
        let fallback = outcome
            .fallback()
            .expect("recognized non-argued topic must not claim content authority");

        assert_eq!(fallback.reason(), FallbackReason::NoAdmissiblePredicate);
        assert!(matches!(
            fallback.subject(),
            Some(FallbackSubject::KnownTopic(_))
        ));
    }

    #[test]
    fn legitimate_topicless_modes_remain_ready() {
        for mode in [
            PropositionMode::Greeting,
            PropositionMode::Purpose,
            PropositionMode::WorldCause,
        ] {
            assert!(
                build_outcome(mode, "внешний предмет", false)
                    .unwrap()
                    .ready()
                    .is_some(),
                "{mode:?} must not be mislabeled as fallback"
            );
        }
    }

    #[test]
    fn all_30_admitted_topics_build_valid_non_empty_plans() {
        let registry = argued_topic_registry().unwrap();

        for topic in registry.topics() {
            let outcome = build_outcome(PropositionMode::Define, topic.topic().as_str(), true)
                .expect("bundled topic must build");
            let plan = outcome.ready().expect("admitted topic must be ready");
            assert!(plan.claims().len() >= 2);
            plan.validate().unwrap();
            let encoded = serde_json::to_string(plan).unwrap();
            assert!(!encoded.contains(topic.thesis().surface()));
            assert!(!encoded.contains(topic.counterpoint().surface()));
            if let Some(consequence) = topic.consequence() {
                assert!(!encoded.contains(consequence.surface()));
            }
        }
    }
}

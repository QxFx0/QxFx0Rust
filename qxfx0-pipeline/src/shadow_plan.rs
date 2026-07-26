//! Pure transitional planner used to observe route-to-plan parity.

use crate::turn_context::RoutedTurnContext;
use qxfx0_semantic::{
    DialogueSubject, ExternalSubject, ExternalSubjectKind, FallbackPlan, FallbackReason,
    FallbackSubject, PlanOutcome, PlanSubject, PlanVersion, PropositionMode, RecoveryEvidence,
    RecoveryTrace, ResponseGoal,
};
use qxfx0_types::{AtomId, CanonicalMoveFamily};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ShadowResponsePlan {
    version: PlanVersion,
    goal: ResponseGoal,
    subject: PlanSubject,
    source_mode: PropositionMode,
    family: CanonicalMoveFamily,
}

impl ShadowResponsePlan {
    fn new(
        goal: ResponseGoal,
        subject: PlanSubject,
        source_mode: PropositionMode,
        family: CanonicalMoveFamily,
    ) -> Self {
        Self {
            version: PlanVersion::ShadowV1,
            goal,
            subject,
            source_mode,
            family,
        }
    }

    pub fn version(&self) -> PlanVersion {
        self.version
    }

    pub fn goal(&self) -> ResponseGoal {
        self.goal
    }

    pub fn subject(&self) -> &PlanSubject {
        &self.subject
    }

    pub fn source_mode(&self) -> PropositionMode {
        self.source_mode
    }

    pub fn family(&self) -> CanonicalMoveFamily {
        self.family
    }
}

pub type ShadowPlanOutcome = PlanOutcome<ShadowResponsePlan>;

pub(crate) fn build_shadow_plan(routed: &RoutedTurnContext) -> ShadowPlanOutcome {
    let prepared = routed.prepared();
    build_outcome(
        prepared.input().mode(),
        routed.family(),
        prepared.input().subject(),
        prepared.has_enough(),
    )
}

fn build_outcome(
    mode: PropositionMode,
    family: CanonicalMoveFamily,
    subject: &str,
    has_enough: bool,
) -> ShadowPlanOutcome {
    let goal = goal_for_mode(mode);
    let ready_subject = if has_enough {
        Some(PlanSubject::Topic(AtomId::new(subject)))
    } else {
        match mode {
            PropositionMode::Greeting => Some(PlanSubject::Dialogue(DialogueSubject::Contact)),
            PropositionMode::Purpose => Some(PlanSubject::External(ExternalSubject::new(
                ExternalSubjectKind::Entity,
                subject,
            ))),
            PropositionMode::WorldCause => Some(PlanSubject::External(ExternalSubject::new(
                ExternalSubjectKind::Phenomenon,
                subject,
            ))),
            PropositionMode::Define
            | PropositionMode::Assert
            | PropositionMode::Challenge
            | PropositionMode::Connect
            | PropositionMode::Reflect => None,
        }
    };

    if let Some(subject) = ready_subject {
        return PlanOutcome::Ready(ShadowResponsePlan::new(goal, subject, mode, family));
    }

    let trimmed_subject = subject.trim();
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
    PlanOutcome::Fallback(FallbackPlan::new(
        PlanVersion::ShadowV1,
        goal,
        fallback_subject,
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
    fn known_topic_produces_ready_shadow_plan() {
        let outcome = build_outcome(
            PropositionMode::Define,
            CanonicalMoveFamily::CMDefine,
            "свобода",
            true,
        );
        let ready = outcome.ready().expect("known topic should be ready");

        assert_eq!(ready.goal(), ResponseGoal::Define);
        assert_eq!(ready.family(), CanonicalMoveFamily::CMDefine);
        assert!(matches!(ready.subject(), PlanSubject::Topic(_)));
    }

    #[test]
    fn unknown_topic_produces_typed_fallback() {
        let outcome = build_outcome(
            PropositionMode::Define,
            CanonicalMoveFamily::CMDefine,
            "кванточайник",
            false,
        );
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
    fn legitimate_topicless_modes_remain_ready() {
        for (mode, family) in [
            (PropositionMode::Greeting, CanonicalMoveFamily::CMContact),
            (PropositionMode::Purpose, CanonicalMoveFamily::CMPurpose),
            (
                PropositionMode::WorldCause,
                CanonicalMoveFamily::CMHypothesis,
            ),
        ] {
            assert!(
                build_outcome(mode, family, "внешний предмет", false)
                    .ready()
                    .is_some(),
                "{mode:?} must not be mislabeled as fallback"
            );
        }
    }
}

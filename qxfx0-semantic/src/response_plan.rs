//! Renderer-independent response outcome and recovery contracts.

use qxfx0_types::AtomId;
use serde::{Deserialize, Serialize};

/// Version of the transitional shadow contract. The full response-plan
/// contract will receive its own explicit version when it replaces this one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanVersion {
    ShadowV1,
}

impl PlanVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ShadowV1 => "shadow_v1",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseGoal {
    GenerateThesis,
    Explain,
    Define,
    Compare,
    Reflect,
    Clarify,
    Repair,
    Challenge,
    Hypothesize,
    Contact,
}

impl ResponseGoal {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GenerateThesis => "generate_thesis",
            Self::Explain => "explain",
            Self::Define => "define",
            Self::Compare => "compare",
            Self::Reflect => "reflect",
            Self::Clarify => "clarify",
            Self::Repair => "repair",
            Self::Challenge => "challenge",
            Self::Hypothesize => "hypothesize",
            Self::Contact => "contact",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogueSubject {
    Contact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExternalSubjectKind {
    Entity,
    Phenomenon,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalSubject {
    kind: ExternalSubjectKind,
    label: String,
}

impl ExternalSubject {
    pub fn new(kind: ExternalSubjectKind, label: impl Into<String>) -> Self {
        Self {
            kind,
            label: label.into(),
        }
    }

    pub fn kind(&self) -> ExternalSubjectKind {
        self.kind
    }

    pub fn label(&self) -> &str {
        &self.label
    }
}

/// A subject admitted for a ready shadow plan. Unresolved topics are excluded
/// and exist only in `FallbackSubject`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanSubject {
    Topic(AtomId),
    Dialogue(DialogueSubject),
    External(ExternalSubject),
}

impl PlanSubject {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::Topic(_) => "topic",
            Self::Dialogue(_) => "dialogue",
            Self::External(_) => "external",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackSubject {
    UnresolvedTopic(String),
    KnownTopic(AtomId),
    Dialogue(DialogueSubject),
}

impl FallbackSubject {
    pub const fn kind(&self) -> &'static str {
        match self {
            Self::UnresolvedTopic(_) => "unresolved_topic",
            Self::KnownTopic(_) => "known_topic",
            Self::Dialogue(_) => "dialogue",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FallbackReason {
    NoTopicProvided,
    UnknownTopic,
    NoAdmissiblePredicate,
    MorphologyFailure,
    QualityRejection,
    SemanticConflict,
    PersistenceRecovery,
}

impl FallbackReason {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::NoTopicProvided => "no_topic_provided",
            Self::UnknownTopic => "unknown_topic",
            Self::NoAdmissiblePredicate => "no_admissible_predicate",
            Self::MorphologyFailure => "morphology_failure",
            Self::QualityRejection => "quality_rejection",
            Self::SemanticConflict => "semantic_conflict",
            Self::PersistenceRecovery => "persistence_recovery",
        }
    }

    pub const fn default_strategy(self) -> RecoveryStrategy {
        match self {
            Self::NoTopicProvided | Self::UnknownTopic => RecoveryStrategy::AskClarification,
            Self::NoAdmissiblePredicate => RecoveryStrategy::BoundedNonArguedResponse,
            Self::MorphologyFailure => RecoveryStrategy::RetryMorphology,
            Self::QualityRejection => RecoveryStrategy::RejectSurface,
            Self::SemanticConflict => RecoveryStrategy::ResolveSemanticConflict,
            Self::PersistenceRecovery => RecoveryStrategy::RestoreSnapshot,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryPolicy {
    Enabled,
    Disabled,
}

impl RecoveryPolicy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Enabled => "enabled",
            Self::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryStrategy {
    AskClarification,
    BoundedNonArguedResponse,
    RetryMorphology,
    RejectSurface,
    ResolveSemanticConflict,
    RestoreSnapshot,
}

impl RecoveryStrategy {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::AskClarification => "ask_clarification",
            Self::BoundedNonArguedResponse => "bounded_non_argued_response",
            Self::RetryMorphology => "retry_morphology",
            Self::RejectSurface => "reject_surface",
            Self::ResolveSemanticConflict => "resolve_semantic_conflict",
            Self::RestoreSnapshot => "restore_snapshot",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityGatePhase {
    Input,
    PostRenderSafety,
    ContentQuality,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryEvidence {
    InputSubjectEmpty,
    TopicLookup {
        subject: String,
        found: bool,
    },
    PredicateSelection {
        admissible_count: usize,
    },
    Morphology {
        token: String,
        detail: String,
    },
    QualityGate {
        phase: QualityGatePhase,
        detail: String,
    },
    SemanticConflict {
        detail: String,
    },
    Persistence {
        operation: String,
        detail: String,
    },
}

/// Non-empty recovery evidence. A recovery plan cannot exist without at least
/// one replay-visible observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryEvidenceSet {
    first: RecoveryEvidence,
    additional: Vec<RecoveryEvidence>,
}

impl RecoveryEvidenceSet {
    pub fn one(first: RecoveryEvidence) -> Self {
        Self {
            first,
            additional: Vec::new(),
        }
    }

    pub fn push(&mut self, evidence: RecoveryEvidence) {
        self.additional.push(evidence);
    }

    pub fn len(&self) -> usize {
        1 + self.additional.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn iter(&self) -> impl Iterator<Item = &RecoveryEvidence> {
        std::iter::once(&self.first).chain(self.additional.iter())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryTrace {
    policy: RecoveryPolicy,
    cause: FallbackReason,
    strategy: RecoveryStrategy,
    evidence: RecoveryEvidenceSet,
}

impl RecoveryTrace {
    pub fn enabled(cause: FallbackReason, evidence: RecoveryEvidence) -> Self {
        Self {
            policy: RecoveryPolicy::Enabled,
            cause,
            strategy: cause.default_strategy(),
            evidence: RecoveryEvidenceSet::one(evidence),
        }
    }

    pub fn policy(&self) -> RecoveryPolicy {
        self.policy
    }

    pub fn cause(&self) -> FallbackReason {
        self.cause
    }

    pub fn strategy(&self) -> RecoveryStrategy {
        self.strategy
    }

    pub fn evidence(&self) -> &RecoveryEvidenceSet {
        &self.evidence
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FallbackPlan {
    version: PlanVersion,
    goal: ResponseGoal,
    subject: Option<FallbackSubject>,
    recovery: RecoveryTrace,
}

impl FallbackPlan {
    pub fn new(
        version: PlanVersion,
        goal: ResponseGoal,
        subject: Option<FallbackSubject>,
        recovery: RecoveryTrace,
    ) -> Self {
        Self {
            version,
            goal,
            subject,
            recovery,
        }
    }

    pub fn version(&self) -> PlanVersion {
        self.version
    }

    pub fn goal(&self) -> ResponseGoal {
        self.goal
    }

    pub fn subject(&self) -> Option<&FallbackSubject> {
        self.subject.as_ref()
    }

    pub fn reason(&self) -> FallbackReason {
        self.recovery.cause()
    }

    pub fn recovery(&self) -> &RecoveryTrace {
        &self.recovery
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlanOutcomeKind {
    Ready,
    Fallback,
}

impl PlanOutcomeKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Fallback => "fallback",
        }
    }
}

/// Sum type that statically prevents a ready plan and fallback plan from
/// coexisting. `P` is transitional in shadow mode and becomes the final
/// `ReadyResponsePlan` in the content-planning PR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanOutcome<P> {
    Ready(P),
    Fallback(FallbackPlan),
}

impl<P> PlanOutcome<P> {
    pub const fn kind(&self) -> PlanOutcomeKind {
        match self {
            Self::Ready(_) => PlanOutcomeKind::Ready,
            Self::Fallback(_) => PlanOutcomeKind::Fallback,
        }
    }

    pub fn ready(&self) -> Option<&P> {
        match self {
            Self::Ready(plan) => Some(plan),
            Self::Fallback(_) => None,
        }
    }

    pub fn fallback(&self) -> Option<&FallbackPlan> {
        match self {
            Self::Ready(_) => None,
            Self::Fallback(plan) => Some(plan),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_reason_is_derived_from_recovery_cause() {
        let recovery = RecoveryTrace::enabled(
            FallbackReason::UnknownTopic,
            RecoveryEvidence::TopicLookup {
                subject: "неизвестное".into(),
                found: false,
            },
        );
        let plan = FallbackPlan::new(
            PlanVersion::ShadowV1,
            ResponseGoal::Define,
            Some(FallbackSubject::UnresolvedTopic("неизвестное".into())),
            recovery,
        );

        assert_eq!(plan.reason(), FallbackReason::UnknownTopic);
        assert_eq!(
            plan.recovery().strategy(),
            RecoveryStrategy::AskClarification
        );
        assert_eq!(plan.recovery().evidence().len(), 1);
    }

    #[test]
    fn recovery_evidence_is_non_empty_and_round_trips() {
        let trace = RecoveryTrace::enabled(
            FallbackReason::QualityRejection,
            RecoveryEvidence::QualityGate {
                phase: QualityGatePhase::ContentQuality,
                detail: "rejected".into(),
            },
        );
        let encoded = serde_json::to_string(&trace).unwrap();
        let decoded: RecoveryTrace = serde_json::from_str(&encoded).unwrap();

        assert_eq!(decoded, trace);
        assert!(!decoded.evidence().is_empty());
    }

    #[test]
    fn every_fallback_reason_has_a_stable_strategy() {
        let reasons = [
            FallbackReason::NoTopicProvided,
            FallbackReason::UnknownTopic,
            FallbackReason::NoAdmissiblePredicate,
            FallbackReason::MorphologyFailure,
            FallbackReason::QualityRejection,
            FallbackReason::SemanticConflict,
            FallbackReason::PersistenceRecovery,
        ];

        for reason in reasons {
            assert!(!reason.as_str().is_empty());
            assert!(!reason.default_strategy().as_str().is_empty());
        }
    }
}

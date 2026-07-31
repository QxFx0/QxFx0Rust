//! Renderer-independent response outcome and recovery contracts.

use qxfx0_types::AtomId;
use serde::{Deserialize, Serialize};

use crate::FactId;

/// Version of the response-plan contract. `ShadowV1` remains for replaying
/// historical fallback traces; `ContentV1` is renderer-authoritative.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanVersion {
    ShadowV1,
    ContentV1,
}

impl PlanVersion {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ShadowV1 => "shadow_v1",
            Self::ContentV1 => "content_v1",
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

/// A subject admitted for a ready response plan. Unresolved topics are excluded
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

/// Stable identifier used by response-plan contracts. Semantic content is
/// carried by identifiers; surface strings remain in audited renderer assets.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct SemanticId(String);

impl SemanticId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("semantic id must not be empty".into())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ClaimId(String);

impl ClaimId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("claim id must not be empty".into())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PredicateRef(String);

impl PredicateRef {
    pub fn try_new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            Err("predicate reference must not be empty".into())
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Structurally non-empty ordered collection used for claims and predicate
/// references. Empty ready plans cannot be represented through constructors.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NonEmptyVec<T> {
    first: T,
    additional: Vec<T>,
}

impl<T> NonEmptyVec<T> {
    pub fn one(first: T) -> Self {
        Self {
            first,
            additional: Vec::new(),
        }
    }

    pub fn push(&mut self, value: T) {
        self.additional.push(value);
    }

    pub fn len(&self) -> usize {
        1 + self.additional.len()
    }

    pub fn is_empty(&self) -> bool {
        false
    }

    pub fn first(&self) -> &T {
        &self.first
    }

    pub fn iter(&self) -> impl Iterator<Item = &T> {
        std::iter::once(&self.first).chain(self.additional.iter())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimRole {
    Thesis,
    Support,
    Counterpoint,
    Consequence,
    DialogueAct,
}

impl ClaimRole {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Thesis => "thesis",
            Self::Support => "support",
            Self::Counterpoint => "counterpoint",
            Self::Consequence => "consequence",
            Self::DialogueAct => "dialogue_act",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceProvenance {
    CuratedReleaseCorpus,
    SystemContract,
    UserInput,
}

impl EvidenceProvenance {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CuratedReleaseCorpus => "curated_release_corpus",
            Self::SystemContract => "system_contract",
            Self::UserInput => "user_input",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceArtifact {
    HaskellReleaseCorpusV1,
    PipelineContractV1,
    CurrentTurn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimEvidence {
    provenance: EvidenceProvenance,
    artifact: EvidenceArtifact,
    record: Option<u16>,
}

impl ClaimEvidence {
    pub fn curated(record: u16) -> Self {
        Self {
            provenance: EvidenceProvenance::CuratedReleaseCorpus,
            artifact: EvidenceArtifact::HaskellReleaseCorpusV1,
            record: Some(record),
        }
    }

    pub fn system_contract() -> Self {
        Self {
            provenance: EvidenceProvenance::SystemContract,
            artifact: EvidenceArtifact::PipelineContractV1,
            record: None,
        }
    }

    pub fn user_input() -> Self {
        Self {
            provenance: EvidenceProvenance::UserInput,
            artifact: EvidenceArtifact::CurrentTurn,
            record: None,
        }
    }

    pub fn provenance(&self) -> EvidenceProvenance {
        self.provenance
    }

    pub fn artifact(&self) -> EvidenceArtifact {
        self.artifact
    }

    pub fn record(&self) -> Option<u16> {
        self.record
    }
}

/// Confidence represented as basis points to exclude NaN and platform-level
/// floating-point drift from replay-visible plans.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Confidence(u16);

impl Confidence {
    pub const MAX_BASIS_POINTS: u16 = 10_000;

    pub fn from_basis_points(value: u16) -> Result<Self, String> {
        if value <= Self::MAX_BASIS_POINTS {
            Ok(Self(value))
        } else {
            Err(format!(
                "confidence {value} exceeds {} basis points",
                Self::MAX_BASIS_POINTS
            ))
        }
    }

    pub fn basis_points(self) -> u16 {
        self.0
    }
}

/// Renderer-independent proposition algebra. Grounded counterpoint and
/// consequence leaves resolve through the audited predicate registry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticProposition {
    CanonicalPredicate {
        subject: SemanticId,
        relation: SemanticId,
        object: SemanticId,
    },
    Counterpoint {
        statement: PredicateRef,
        counters: PredicateRef,
    },
    Consequence {
        statement: PredicateRef,
        follows_from: PredicateRef,
    },
    DialogueAct(DialogueSubject),
    ExternalReference(ExternalSubject),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlannedClaim {
    id: ClaimId,
    role: ClaimRole,
    fact_id: Option<FactId>,
    proposition: SemanticProposition,
    predicate_refs: NonEmptyVec<PredicateRef>,
    evidence: ClaimEvidence,
    confidence: Confidence,
}

impl PlannedClaim {
    pub fn new(
        id: ClaimId,
        role: ClaimRole,
        fact_id: Option<FactId>,
        proposition: SemanticProposition,
        predicate_refs: NonEmptyVec<PredicateRef>,
        evidence: ClaimEvidence,
        confidence: Confidence,
    ) -> Self {
        Self {
            id,
            role,
            fact_id,
            proposition,
            predicate_refs,
            evidence,
            confidence,
        }
    }

    pub fn id(&self) -> &ClaimId {
        &self.id
    }

    pub fn role(&self) -> ClaimRole {
        self.role
    }

    /// Curated factual authority for declarative claims. Dialogue/system acts
    /// intentionally carry no FactId.
    pub fn fact_id(&self) -> Option<&FactId> {
        self.fact_id.as_ref()
    }

    pub fn proposition(&self) -> &SemanticProposition {
        &self.proposition
    }

    pub fn predicate_refs(&self) -> &NonEmptyVec<PredicateRef> {
        &self.predicate_refs
    }

    pub fn evidence(&self) -> &ClaimEvidence {
        &self.evidence
    }

    pub fn confidence(&self) -> Confidence {
        self.confidence
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscourseRelation {
    None,
    Elaboration,
    Contrast,
    Consequence,
    Counterpoint,
}

impl DiscourseRelation {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::Elaboration => "elaboration",
            Self::Contrast => "contrast",
            Self::Consequence => "consequence",
            Self::Counterpoint => "counterpoint",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SentenceBudget {
    One,
    Two,
    Three,
}

impl SentenceBudget {
    pub const fn get(self) -> u8 {
        match self {
            Self::One => 1,
            Self::Two => 2,
            Self::Three => 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoursePlan {
    relation: DiscourseRelation,
    sentence_budget: SentenceBudget,
}

impl DiscoursePlan {
    pub fn new(relation: DiscourseRelation, sentence_budget: SentenceBudget) -> Self {
        Self {
            relation,
            sentence_budget,
        }
    }

    pub fn relation(&self) -> DiscourseRelation {
        self.relation
    }

    pub fn sentence_budget(&self) -> SentenceBudget {
        self.sentence_budget
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DialogueObligation {
    CheckAgreement { claim_id: ClaimId },
    ContinueContact,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DerivationRule {
    SelectedAdmittedPredicate,
    AddedCounterpoint,
    AddedConsequence,
    AppliedDialogueContract,
    GroundedExternalReference,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivationStep {
    claim_id: ClaimId,
    predicate_refs: NonEmptyVec<PredicateRef>,
    rule: DerivationRule,
}

impl DerivationStep {
    pub fn new(
        claim_id: ClaimId,
        predicate_refs: NonEmptyVec<PredicateRef>,
        rule: DerivationRule,
    ) -> Self {
        Self {
            claim_id,
            predicate_refs,
            rule,
        }
    }

    pub fn claim_id(&self) -> &ClaimId {
        &self.claim_id
    }

    pub fn predicate_refs(&self) -> &NonEmptyVec<PredicateRef> {
        &self.predicate_refs
    }

    pub fn rule(&self) -> DerivationRule {
        self.rule
    }
}

/// Content-bearing ready plan. Claims own their propositions, so there is no
/// second proposition table that can drift from the selected claim set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadyResponsePlan {
    version: PlanVersion,
    goal: ResponseGoal,
    subject: PlanSubject,
    claims: NonEmptyVec<PlannedClaim>,
    discourse: DiscoursePlan,
    obligation: Option<DialogueObligation>,
    derivation: Vec<DerivationStep>,
}

impl ReadyResponsePlan {
    pub fn new(
        goal: ResponseGoal,
        subject: PlanSubject,
        claims: NonEmptyVec<PlannedClaim>,
        discourse: DiscoursePlan,
        obligation: Option<DialogueObligation>,
        derivation: Vec<DerivationStep>,
    ) -> Result<Self, String> {
        let plan = Self {
            version: PlanVersion::ContentV1,
            goal,
            subject,
            claims,
            discourse,
            obligation,
            derivation,
        };
        plan.validate()?;
        Ok(plan)
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.claims.len() > 3 {
            return Err("ready response plan exceeds 3 claims".into());
        }
        let mut claim_ids = std::collections::BTreeSet::new();
        for claim in self.claims.iter() {
            if claim.id().as_str().trim().is_empty() {
                return Err("ready response plan contains an empty claim id".into());
            }
            if claim
                .predicate_refs()
                .iter()
                .any(|predicate_ref| predicate_ref.as_str().trim().is_empty())
            {
                return Err(format!(
                    "claim '{}' contains an empty predicate reference",
                    claim.id().as_str()
                ));
            }
            if claim.confidence().basis_points() > Confidence::MAX_BASIS_POINTS {
                return Err(format!(
                    "claim '{}' confidence exceeds the allowed range",
                    claim.id().as_str()
                ));
            }
            if claim.role() != ClaimRole::DialogueAct && claim.fact_id().is_none() {
                return Err(format!(
                    "declarative claim '{}' has no FactId",
                    claim.id().as_str()
                ));
            }
            if claim.role() == ClaimRole::DialogueAct && claim.fact_id().is_some() {
                return Err(format!(
                    "dialogue claim '{}' must not masquerade as a fact",
                    claim.id().as_str()
                ));
            }
            if !claim_ids.insert(claim.id().as_str()) {
                return Err(format!("duplicate claim id '{}'", claim.id().as_str()));
            }
        }
        if self.derivation.len() > 8 {
            return Err("response plan derivation exceeds 8 steps".into());
        }
        if self
            .derivation
            .iter()
            .any(|step| !claim_ids.contains(step.claim_id().as_str()))
        {
            return Err("derivation references a claim outside the plan".into());
        }
        if let Some(DialogueObligation::CheckAgreement { claim_id }) = &self.obligation {
            if !claim_ids.contains(claim_id.as_str()) {
                return Err("dialogue obligation references a claim outside the plan".into());
            }
        }
        Ok(())
    }

    pub fn validate_with_facts(&self, facts: &crate::FactRegistry) -> Result<(), String> {
        self.validate()?;
        for claim in self.claims.iter() {
            if claim.role() == ClaimRole::DialogueAct {
                continue;
            }
            let fact_id = claim
                .fact_id()
                .ok_or_else(|| format!("claim '{}' has no FactId", claim.id().as_str()))?;
            facts.select(fact_id).map_err(|error| error.to_string())?;
        }
        Ok(())
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

    pub fn claims(&self) -> &NonEmptyVec<PlannedClaim> {
        &self.claims
    }

    pub fn discourse(&self) -> &DiscoursePlan {
        &self.discourse
    }

    pub fn obligation(&self) -> Option<&DialogueObligation> {
        self.obligation.as_ref()
    }

    pub fn derivation(&self) -> &[DerivationStep] {
        &self.derivation
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
/// coexisting. `P` permits contract evolution while each concrete ready plan
/// keeps its own construction invariants.
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

    #[test]
    fn ready_plan_rejects_duplicate_claim_identity() {
        let claim_id = ClaimId::try_new("system.contact.claim").unwrap();
        let predicate_ref = PredicateRef::try_new("system.contact").unwrap();
        let claim = PlannedClaim::new(
            claim_id,
            ClaimRole::DialogueAct,
            None,
            SemanticProposition::DialogueAct(DialogueSubject::Contact),
            NonEmptyVec::one(predicate_ref),
            ClaimEvidence::system_contract(),
            Confidence::from_basis_points(10_000).unwrap(),
        );
        let mut claims = NonEmptyVec::one(claim.clone());
        claims.push(claim);

        let result = ReadyResponsePlan::new(
            ResponseGoal::Contact,
            PlanSubject::Dialogue(DialogueSubject::Contact),
            claims,
            DiscoursePlan::new(DiscourseRelation::None, SentenceBudget::One),
            Some(DialogueObligation::ContinueContact),
            Vec::new(),
        );

        assert!(result.is_err());
    }
}

//! ResponsePlan V2 boundaries (ADR-0034).
//!
//! V2 replaces selection-plus-substitution with a chain of typed certificates:
//!
//! ```text
//! CandidateResponsePlan
//! → LeafAdmittedPlan          LeafAdmissionProof
//! → EvidenceCertifiedPlan     EvidenceAuthorityCertificate (as_of)
//! → AssertionAuthorizedPlan   recursive per-constructor policy
//! → RealizablePlan            ResolvedSynTree + completeness certificate
//! → RealizedSurface           execution receipt
//! ```
//!
//! This module implements the candidate stratum and the first three
//! certificates of the chain. Nothing here is wired into the runtime — the V1
//! audited renderer remains authoritative until `doctor --gate
//! response-plan-v2-phase-b` is implemented and flipped.

pub mod admission;
pub mod assertion;
pub mod attempt;
pub mod audited_corpus;
pub mod candidate;
pub mod derivation;
pub mod discourse;
pub mod evidence;
pub mod morphology_depth;
pub mod proposition;
pub mod realization;
pub mod selection;
pub mod snapshot;
pub mod syn_tree;
pub mod valency;

pub use admission::{
    prove_leaf_admission, AdmissionError, LeafAdmissionProof, LeafAdmittedPlan, ADMISSION_DOMAIN,
};
pub use assertion::{
    AssertionAuthorizedPlan, AssertionError, AssertionFailureReason, AssertionPolicy,
    ClaimAuthority, QualifierAdmission, ASSERTION_POLICY_DOMAIN,
};
pub use attempt::{
    attempt_input_digest, enforce_authorized_budget, enforce_budget, enforce_work_budget,
    fallback_action, fallback_action_for_attempt, fallback_action_for_result,
    BoundedRejectedArtifact, BudgetExceeded, BudgetPhase, BudgetRejection, BudgetResource,
    BudgetWorkItem, CertifiedPrefix, FallbackAction, TruncationWitness, V2Attempt, V2BudgetPolicy,
    V2ExecutionResult, V2PreCandidateOutcome, V2Route,
};
pub use audited_corpus::{
    audit_audited_corpus, audited_surface_source_digest, audited_verbatim_surface,
    authority_outcome, build_audited_topic, build_audited_topic_at, execute_audited_topic_at,
    AuditedCorpusError, AuditedCorpusReport, AuditedTopicPlan, AuditedV2Execution,
    AuthoritySurfaceStrategy, V2AuthorityOutcome,
};
pub use candidate::{CandidateInvariantError, CandidateResponsePlan};
pub use derivation::{
    DerivationDag, DerivationDagBuilder, DerivationId, DerivationInvariantError, DerivationNode,
    EvidenceRef, InferenceRuleId, DERIVATION_DOMAIN,
};
pub use discourse::{
    projected_roles, ClaimId, DiscourseInvariantError, DiscourseOccurrenceId, DiscoursePlan,
    DiscourseTree, ProjectedClaim, CLAIM_DOMAIN, DISCOURSE_DOMAIN,
};
pub use evidence::{
    certify_claim, EvidenceAuthorityCertificate, EvidenceCertifiedPlan, EvidenceError,
    EvidenceEvaluationContext, EVIDENCE_DOMAIN,
};
pub use morphology_depth::{
    preposition_allomorphs, verify_round_trip, MorphologyRoundTripError,
    MorphologyRoundTripWitness, PrepositionAllomorphLexicon, RoundTripClass,
};
pub use proposition::{
    PropositionDag, PropositionDagBuilder, PropositionId, PropositionInvariantError,
    PropositionNode, QualifierId, PROPOSITION_DOMAIN,
};
pub use realization::{
    join_realized_clauses, linearize, try_realize, RealizablePlan, RealizedSurface,
    REALIZATION_JOINER_VERSION,
};
pub use selection::{
    select_candidate, BasisPoints, CandidateSelectionSignals, ResponsePlanV2Mode,
    SelectedCandidate, SelectionCandidate, SelectionError, SelectionPolicy, SelectionReceipt,
    SelfSelectionContext, NUMERIC_SEMANTICS_VERSION, RANKING_VERSION,
};
pub use snapshot::{
    inference_rule_set_digest, verify_replay, AuthoritySnapshot, CapturedRealizationNode,
    ExactReplayBundle, PlanningPolicySnapshot, RealizationSnapshot, ReplayInputEnvelope,
    ReplayLevel, ReplayMaterials, ReplayVerification, SelectionPolicySnapshot, SnapshotError,
    TurnContractSnapshot, TurnRecord,
};
pub use syn_tree::{
    by_occurrence, resolve, Clause, NounPhrase, RealizationCompletenessCertificate,
    RealizationError, ResolvedClause, ResolvedSynNode, ResolvedSynTree, SynNode, SynTree,
    VerbPhrase,
};
pub use valency::{
    starts_with_word, valency_lexicon, AgreementFeatures, Complement, HeadKind, ValencyError,
    ValencyFrame, ValencyLexicon,
};

/// Turn-level failure envelope (ADR-0034 §1).
///
/// Only the strata that exist carry a variant. Startup-level faults — a
/// corrupt or drifted pack, an internally inconsistent schema — are not
/// represented here: they fail loud at `doctor`, never as a turn-level typed
/// rejection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum V2Failure {
    #[error("candidate invariant: {0}")]
    Candidate(#[from] CandidateInvariantError),
    #[error("admission: {0}")]
    Admission(#[from] AdmissionError),
    #[error("evidence: {0}")]
    Evidence(#[from] EvidenceError),
    #[error("assertion: {0}")]
    Assertion(#[from] AssertionError),
    #[error("realization: {0}")]
    Realization(#[from] RealizationError),
    #[error("budget: {0}")]
    Budget(#[from] BudgetExceeded),
    #[error("snapshot: {0}")]
    Snapshot(#[from] SnapshotError),
}

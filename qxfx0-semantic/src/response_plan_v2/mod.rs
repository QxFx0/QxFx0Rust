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
//! This module currently implements the first stratum only: the recursive
//! proposition algebra with Merkle identity, and the typed entailment layer
//! over it. Nothing here is wired into the runtime — the V1 audited renderer
//! remains authoritative until `doctor --gate response-plan-v2-phase-b` is
//! implemented and flipped.
//!
//! The discourse tree, the role projection, and the four certificates are
//! deliberately absent rather than stubbed, so the type system does not
//! suggest a boundary that has not been designed in code yet.

pub mod derivation;
pub mod proposition;

pub use derivation::{
    DerivationDag, DerivationDagBuilder, DerivationId, DerivationInvariantError, DerivationNode,
    EvidenceRef, InferenceRuleId, DERIVATION_DOMAIN,
};
pub use proposition::{
    PropositionDag, PropositionDagBuilder, PropositionId, PropositionInvariantError,
    PropositionNode, QualifierId, PROPOSITION_DOMAIN,
};

/// Turn-level failure envelope (ADR-0034 §1).
///
/// Only the strata that exist carry a variant. Startup-level faults — a
/// corrupt or drifted pack, an internally inconsistent schema — are not
/// represented here: they fail loud at `doctor`, never as a turn-level typed
/// rejection.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum V2Failure {
    #[error("candidate proposition invariant: {0}")]
    Proposition(#[from] PropositionInvariantError),
    #[error("candidate derivation invariant: {0}")]
    Derivation(#[from] DerivationInvariantError),
}

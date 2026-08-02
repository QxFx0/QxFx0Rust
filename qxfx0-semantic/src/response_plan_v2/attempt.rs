//! Turn-level V2 attempt envelope and deterministic fallback policy (ADR-0034 §6).

use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};

use super::admission::LeafAdmittedPlan;
use super::assertion::AssertionAuthorizedPlan;
use super::candidate::CandidateResponsePlan;
use super::evidence::EvidenceCertifiedPlan;
use super::{RealizationError, V2Failure};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BudgetPhase {
    Candidate,
    Admission,
    Evidence,
    Assertion,
    Realization,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum BudgetResource {
    Propositions,
    Derivations,
    DiscourseOccurrences,
    FrontierItems,
    Clauses,
    Bytes,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, thiserror::Error)]
#[error("{phase:?} {resource:?} budget exceeded: observed {observed}, limit {limit}")]
pub struct BudgetExceeded {
    pub phase: BudgetPhase,
    pub resource: BudgetResource,
    pub observed: u64,
    pub limit: u64,
}

pub fn enforce_budget(
    phase: BudgetPhase,
    resource: BudgetResource,
    observed: usize,
    limit: usize,
) -> Result<(), BudgetExceeded> {
    if observed > limit {
        Err(BudgetExceeded {
            phase,
            resource,
            observed: observed as u64,
            limit: limit as u64,
        })
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TruncationWitness {
    pub phase: BudgetPhase,
    pub triggered_limit: u64,
    pub planning_policy_digest: String,
    pub attempt_input_digest: String,
    pub visited_digest: String,
    pub pending_frontier_digest: String,
}

impl TruncationWitness {
    pub fn digest(&self) -> String {
        digest(b"qxfx0:truncation-witness:v1", self)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum CertifiedPrefix {
    Candidate(CandidateResponsePlan),
    Admitted(LeafAdmittedPlan),
    EvidenceCertified(EvidenceCertifiedPlan),
    AssertionAuthorized(AssertionAuthorizedPlan),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoundedRejectedArtifact {
    pub prefix: CertifiedPrefix,
    pub prefix_digest: String,
    pub witness: Option<TruncationWitness>,
}

impl BoundedRejectedArtifact {
    pub fn new(prefix: CertifiedPrefix) -> Self {
        let prefix_digest = digest(b"qxfx0:rejected-prefix:v1", &prefix);
        Self {
            prefix,
            prefix_digest,
            witness: None,
        }
    }

    pub fn truncated(prefix: CertifiedPrefix, witness: TruncationWitness) -> Self {
        let prefix_digest = digest(b"qxfx0:rejected-prefix:v1", &prefix);
        Self {
            prefix,
            prefix_digest,
            witness: Some(witness),
        }
    }

    pub fn prefix(&self) -> &CertifiedPrefix {
        &self.prefix
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum V2Route {
    NoCandidate,
    UnsupportedInput,
    V1Only,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2Attempt {
    NotApplicable {
        route: V2Route,
    },
    Rejected {
        artifact: BoundedRejectedArtifact,
        failure: V2Failure,
    },
    Realizable(super::RealizablePlan),
}

impl Serialize for V2Attempt {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::NotApplicable { route } => {
                #[derive(Serialize)]
                struct NotApplicable<'a> {
                    kind: &'static str,
                    route: &'a V2Route,
                }
                NotApplicable {
                    kind: "not_applicable",
                    route,
                }
                .serialize(serializer)
            }
            Self::Rejected { artifact, failure } => {
                #[derive(Serialize)]
                struct Rejected<'a> {
                    kind: &'static str,
                    artifact: &'a BoundedRejectedArtifact,
                    failure: String,
                }
                Rejected {
                    kind: "rejected",
                    artifact,
                    failure: failure.to_string(),
                }
                .serialize(serializer)
            }
            Self::Realizable(plan) => {
                #[derive(Serialize)]
                struct Realizable<'a> {
                    kind: &'static str,
                    plan: &'a super::RealizablePlan,
                }
                Realizable {
                    kind: "realizable",
                    plan,
                }
                .serialize(serializer)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum FallbackAction {
    V2Renderer,
    AuditedV1Renderer,
    TypedNonDeclarative,
    FailLoud,
}

pub fn fallback_action(failure: Option<&V2Failure>) -> FallbackAction {
    match failure {
        None => FallbackAction::V2Renderer,
        Some(V2Failure::Snapshot(_)) => FallbackAction::FailLoud,
        Some(V2Failure::Realization(RealizationError::IncompleteForm { .. })) => {
            FallbackAction::AuditedV1Renderer
        }
        Some(_) => FallbackAction::TypedNonDeclarative,
    }
}

pub fn fallback_action_for_attempt(attempt: &V2Attempt) -> FallbackAction {
    match attempt {
        V2Attempt::NotApplicable { .. } => FallbackAction::TypedNonDeclarative,
        V2Attempt::Rejected { failure, .. } => fallback_action(Some(failure)),
        V2Attempt::Realizable(_) => FallbackAction::V2Renderer,
    }
}

fn digest<T: Serialize>(domain: &[u8], value: &T) -> String {
    let encoded = serde_json::to_vec(value).expect("V2 artifact serializes");
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update((encoded.len() as u64).to_be_bytes());
    hasher.update(encoded);
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan_v2::SnapshotError;

    #[test]
    fn budget_is_inclusive_and_deterministic() {
        assert!(enforce_budget(BudgetPhase::Candidate, BudgetResource::Propositions, 2, 2).is_ok());
        let error =
            enforce_budget(BudgetPhase::Candidate, BudgetResource::Propositions, 3, 2).unwrap_err();
        assert_eq!(error.observed, 3);
        assert_eq!(error.limit, 2);
    }

    #[test]
    fn truncation_witness_is_not_a_semantic_node() {
        let witness = TruncationWitness {
            phase: BudgetPhase::Candidate,
            triggered_limit: 4,
            planning_policy_digest: "policy".into(),
            attempt_input_digest: "input".into(),
            visited_digest: "visited".into(),
            pending_frontier_digest: "pending".into(),
        };
        assert_eq!(witness.digest(), witness.digest());
        let changed = TruncationWitness {
            pending_frontier_digest: "other".into(),
            ..witness.clone()
        };
        assert_ne!(witness.digest(), changed.digest());
    }

    #[test]
    fn fallback_table_is_closed_and_snapshot_is_fail_loud() {
        assert_eq!(fallback_action(None), FallbackAction::V2Renderer);
        assert_eq!(
            fallback_action(Some(&V2Failure::Snapshot(
                SnapshotError::SnapshotUnavailable {
                    level: super::super::snapshot::ReplayLevel::Integrity
                }
            ))),
            FallbackAction::FailLoud
        );
        let attempt = V2Attempt::NotApplicable {
            route: V2Route::V1Only,
        };
        assert_eq!(
            fallback_action_for_attempt(&attempt),
            FallbackAction::TypedNonDeclarative
        );
    }
}

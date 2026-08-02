//! Turn-level V2 attempt envelope and deterministic fallback policy (ADR-0034 §6).

use serde::{Serialize, Serializer};
use sha2::{Digest, Sha256};

use super::admission::LeafAdmittedPlan;
use super::assertion::AssertionAuthorizedPlan;
use super::candidate::CandidateResponsePlan;
use super::evidence::EvidenceCertifiedPlan;
use super::{RealizablePlan, RealizationError, V2Failure};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct V2BudgetPolicy {
    pub propositions: usize,
    pub derivations: usize,
    pub discourse_occurrences: usize,
    pub clauses: usize,
    pub realized_bytes: usize,
}

impl Default for V2BudgetPolicy {
    fn default() -> Self {
        Self {
            propositions: 8,
            derivations: 8,
            discourse_occurrences: 8,
            clauses: 8,
            realized_bytes: 16 * 1024,
        }
    }
}

impl V2BudgetPolicy {
    pub fn digest(&self) -> String {
        digest(b"qxfx0:v2-budget-policy:v1", self)
    }
}

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
    Realizable(Box<RealizablePlan>),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BudgetWorkItem {
    pub resource: BudgetResource,
    pub id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BudgetRejection {
    pub error: BudgetExceeded,
    pub witness: TruncationWitness,
}

pub fn enforce_work_budget(
    phase: BudgetPhase,
    resource: BudgetResource,
    work: &[BudgetWorkItem],
    limit: usize,
    planning_policy_digest: &str,
    attempt_input_digest: &str,
) -> Result<(), Box<BudgetRejection>> {
    enforce_budget(phase, resource, work.len(), limit).map_err(|exceeded| {
        let split = limit.min(work.len());
        let witness = TruncationWitness {
            phase,
            triggered_limit: limit as u64,
            planning_policy_digest: planning_policy_digest.to_string(),
            attempt_input_digest: attempt_input_digest.to_string(),
            visited_digest: digest(b"qxfx0:v2-budget-visited:v1", &&work[..split]),
            pending_frontier_digest: digest(
                b"qxfx0:v2-budget-pending-frontier:v1",
                &&work[split..],
            ),
        };
        Box::new(BudgetRejection {
            error: exceeded,
            witness,
        })
    })
}

pub fn attempt_input_digest<T: Serialize>(input: &T) -> String {
    digest(b"qxfx0:v2-attempt-input:v1", input)
}

pub fn enforce_authorized_budget(
    authorized: &AssertionAuthorizedPlan,
    policy: &V2BudgetPolicy,
    planning_policy_digest: &str,
    attempt_input_digest: &str,
) -> Result<(), Box<BudgetRejection>> {
    let candidate = authorized.certified().candidate();
    let checks = [
        (
            BudgetResource::Propositions,
            candidate
                .propositions()
                .iter()
                .map(|(id, _)| BudgetWorkItem {
                    resource: BudgetResource::Propositions,
                    id: id.as_str().to_string(),
                })
                .collect::<Vec<_>>(),
            policy.propositions,
        ),
        (
            BudgetResource::DiscourseOccurrences,
            candidate
                .projected_claims()
                .into_iter()
                .map(|claim| BudgetWorkItem {
                    resource: BudgetResource::DiscourseOccurrences,
                    id: claim.claim_id.as_str().to_string(),
                })
                .collect::<Vec<_>>(),
            policy.discourse_occurrences,
        ),
    ];
    for (resource, work, limit) in checks {
        enforce_work_budget(
            BudgetPhase::Candidate,
            resource,
            &work,
            limit,
            planning_policy_digest,
            attempt_input_digest,
        )?;
    }
    Ok(())
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

/// Outcome before any candidate has satisfied its invariants. No certified
/// prefix exists at this boundary, so these cases cannot be represented as a
/// normal rejected artifact without fabricating semantic progress.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum V2PreCandidateOutcome {
    NotApplicable { route: V2Route },
    Startup { reason: String },
    Candidate { failure: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2Attempt {
    NotApplicable {
        route: V2Route,
    },
    Rejected {
        artifact: Box<BoundedRejectedArtifact>,
        failure: V2Failure,
    },
    Realizable(Box<super::RealizablePlan>),
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum V2ExecutionResult {
    PreCandidate(V2PreCandidateOutcome),
    Attempt(V2Attempt),
}

impl Serialize for V2ExecutionResult {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            Self::PreCandidate(outcome) => {
                #[derive(Serialize)]
                struct Envelope<'a> {
                    kind: &'static str,
                    outcome: &'a V2PreCandidateOutcome,
                }
                Envelope {
                    kind: "pre_candidate",
                    outcome,
                }
                .serialize(serializer)
            }
            Self::Attempt(attempt) => attempt.serialize(serializer),
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

impl FallbackAction {
    pub const fn is_declarative(self) -> bool {
        matches!(self, Self::V2Renderer | Self::AuditedV1Renderer)
    }
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

pub fn fallback_action_for_result(result: &V2ExecutionResult) -> FallbackAction {
    match result {
        V2ExecutionResult::PreCandidate(V2PreCandidateOutcome::NotApplicable { .. }) => {
            FallbackAction::TypedNonDeclarative
        }
        V2ExecutionResult::PreCandidate(
            V2PreCandidateOutcome::Startup { .. } | V2PreCandidateOutcome::Candidate { .. },
        ) => FallbackAction::FailLoud,
        V2ExecutionResult::Attempt(attempt) => fallback_action_for_attempt(attempt),
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
    fn real_work_budget_splits_visited_and_pending_frontier() {
        let work = (0..3)
            .map(|index| BudgetWorkItem {
                resource: BudgetResource::Propositions,
                id: index.to_string(),
            })
            .collect::<Vec<_>>();
        let rejection = enforce_work_budget(
            BudgetPhase::Candidate,
            BudgetResource::Propositions,
            &work,
            2,
            "policy",
            "input",
        )
        .unwrap_err();
        assert_eq!(rejection.error.observed, 3);
        assert_eq!(rejection.witness.triggered_limit, 2);
        assert_ne!(
            rejection.witness.visited_digest,
            rejection.witness.pending_frontier_digest
        );
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
        assert!(FallbackAction::V2Renderer.is_declarative());
        assert!(!FallbackAction::TypedNonDeclarative.is_declarative());
    }

    #[test]
    fn pre_candidate_startup_is_not_a_rejected_candidate() {
        let result = V2ExecutionResult::PreCandidate(V2PreCandidateOutcome::Startup {
            reason: "pack drift".into(),
        });
        assert_eq!(
            fallback_action_for_result(&result),
            FallbackAction::FailLoud
        );
        assert!(serde_json::to_string(&result)
            .expect("attempt serializes")
            .contains("pre_candidate"));
    }

    #[test]
    fn legacy_graph_is_not_a_v2_fallback_action() {
        let encoded = serde_json::to_string(&FallbackAction::TypedNonDeclarative).unwrap();
        assert!(!encoded.contains("legacy_graph"));
    }
}

//! The candidate plan and its structural certificate (ADR-0034 §1).
//!
//! The three strata are built independently — a proposition DAG, a derivation
//! DAG and a discourse tree — and only [`CandidateResponsePlan::try_new`]
//! joins them. It is the sole constructor, so a raw triple of collections can
//! never reach a later boundary: possessing the type *is* the certificate that
//! the three agree.
//!
//! Agreement is not merely referential integrity. Two further invariants
//! matter:
//!
//! * Everything the discourse states must exist as meaning.
//! * Everything that exists as meaning must be used, either stated by the
//!   discourse or carried by a derivation. Content in the plan that no one can
//!   reach is either an authoring mistake or an attempt to smuggle material
//!   past the later admission boundary, and both should fail here.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use super::derivation::{DerivationDag, DerivationInvariantError};
use super::discourse::{
    projected_roles, ClaimId, DiscourseInvariantError, DiscoursePlan, ProjectedClaim,
};
use super::proposition::{PropositionDag, PropositionId, PropositionInvariantError};
use crate::response_plan::ClaimRole;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CandidateInvariantError {
    #[error("proposition stratum: {0}")]
    Proposition(#[from] PropositionInvariantError),
    #[error("derivation stratum: {0}")]
    Derivation(#[from] DerivationInvariantError),
    #[error("discourse stratum: {0}")]
    Discourse(#[from] DiscourseInvariantError),
    #[error("discourse states proposition '{0}', which the dag does not contain")]
    StatedPropositionMissing(String),
    #[error("proposition '{0}' is in the dag but is neither stated nor derived")]
    UnreachableProposition(String),
}

/// A structurally consistent, not yet admitted, response plan.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CandidateResponsePlan {
    propositions: PropositionDag,
    derivations: DerivationDag,
    discourse: DiscoursePlan,
}

impl CandidateResponsePlan {
    /// Join three independently built strata into one certified candidate.
    pub fn try_new(
        propositions: PropositionDag,
        derivations: DerivationDag,
        discourse: DiscoursePlan,
    ) -> Result<Self, CandidateInvariantError> {
        // Everything spoken must exist as meaning.
        for stated in discourse.stated_propositions() {
            if !propositions.contains(&stated) {
                return Err(CandidateInvariantError::StatedPropositionMissing(
                    stated.as_str().to_string(),
                ));
            }
        }

        // Everything that exists as meaning must be used. Roots are the stated
        // claims plus whatever the derivations touch; from there, reachability
        // runs through the canonical children, so sub-propositions supporting a
        // stated composite are legitimately live.
        let mut frontier: Vec<PropositionId> = discourse.stated_propositions();
        for (_, node) in derivations.iter() {
            frontier.push(node.conclusion().clone());
            frontier.extend(node.premises().iter().cloned());
        }

        let mut reachable: BTreeSet<PropositionId> = BTreeSet::new();
        while let Some(id) = frontier.pop() {
            if !reachable.insert(id.clone()) {
                continue;
            }
            if let Some(node) = propositions.get(&id) {
                frontier.extend(node.canonical_children());
            }
        }

        for (id, _) in propositions.iter() {
            if !reachable.contains(id) {
                return Err(CandidateInvariantError::UnreachableProposition(
                    id.as_str().to_string(),
                ));
            }
        }

        Ok(Self {
            propositions,
            derivations,
            discourse,
        })
    }

    pub fn propositions(&self) -> &PropositionDag {
        &self.propositions
    }

    pub fn derivations(&self) -> &DerivationDag {
        &self.derivations
    }

    pub fn discourse(&self) -> &DiscoursePlan {
        &self.discourse
    }

    /// Claims with their derived addresses and roles.
    pub fn projected_claims(&self) -> Vec<ProjectedClaim> {
        self.discourse.projected_claims()
    }

    /// Roles keyed by derived claim identity. Never persisted.
    pub fn projected_roles(&self) -> BTreeMap<ClaimId, ClaimRole> {
        projected_roles(&self.discourse)
    }

    /// Digest covering all three strata, for stage traces.
    ///
    /// The discourse root is included because two plans can share every
    /// proposition and differ only in how they are arranged — and that
    /// difference is a difference in what is said.
    pub fn candidate_digest(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(b"qxfx0:candidate-plan:v1");
        for part in [
            self.propositions.merkle_root(),
            self.derivations.merkle_root(),
            self.discourse.root_digest(),
        ] {
            hasher.update((part.len() as u64).to_be_bytes());
            hasher.update(part.as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan::{Confidence, NonEmptyVec, SemanticId};
    use crate::response_plan_v2::derivation::{
        DerivationDagBuilder, DerivationNode, EvidenceRef, InferenceRuleId,
    };
    use crate::response_plan_v2::discourse::DiscourseTree;
    use crate::response_plan_v2::proposition::{PropositionDagBuilder, PropositionNode};

    fn predicate(subject: &str) -> PropositionNode {
        PropositionNode::Predicate {
            subject: SemanticId::try_new(subject).expect("semantic id"),
            relation: SemanticId::try_new("предполагает").expect("semantic id"),
            object: SemanticId::try_new("возможность_выбора").expect("semantic id"),
        }
    }

    #[test]
    fn a_stated_plan_without_derivations_is_valid() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let b = props.insert(predicate("истина"));
        let propositions = props.build().expect("dag");

        let discourse = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Thesis(a),
            DiscourseTree::Counterpoint(b),
        ]))
        .expect("discourse");

        let plan = CandidateResponsePlan::try_new(propositions, DerivationDag::empty(), discourse)
            .expect("candidate");
        assert_eq!(plan.projected_roles().len(), 2);
        assert_eq!(plan.propositions().len(), 2);
    }

    /// A composite that is stated keeps its parts live, because they are
    /// reachable through the canonical children.
    #[test]
    fn sub_propositions_of_a_stated_composite_are_reachable() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let b = props.insert(predicate("истина"));
        let conjunction = props.insert(PropositionNode::Conjunction {
            children: vec![a, b],
        });
        let propositions = props.build().expect("dag");

        let discourse = DiscoursePlan::try_new(DiscourseTree::Thesis(conjunction)).expect("plan");
        assert!(
            CandidateResponsePlan::try_new(propositions, DerivationDag::empty(), discourse).is_ok()
        );
    }

    #[test]
    fn stating_an_absent_proposition_is_rejected() {
        let mut props = PropositionDagBuilder::new();
        props.insert(predicate("свобода"));
        let propositions = props.build().expect("dag");

        let stranger = predicate("власть").id();
        let discourse = DiscoursePlan::try_new(DiscourseTree::Thesis(stranger)).expect("plan");

        assert!(matches!(
            CandidateResponsePlan::try_new(propositions, DerivationDag::empty(), discourse),
            Err(CandidateInvariantError::StatedPropositionMissing(_))
        ));
    }

    /// Meaning nobody can reach is either a mistake or an attempt to carry
    /// material past the admission boundary.
    #[test]
    fn unreachable_meaning_is_rejected() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        props.insert(predicate("власть"));
        let propositions = props.build().expect("dag");

        let discourse = DiscoursePlan::try_new(DiscourseTree::Thesis(a)).expect("plan");

        assert!(matches!(
            CandidateResponsePlan::try_new(propositions, DerivationDag::empty(), discourse),
            Err(CandidateInvariantError::UnreachableProposition(_))
        ));
    }

    /// An unstated premise is still live: it justifies a stated conclusion.
    #[test]
    fn a_premise_that_is_never_spoken_stays_reachable() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let c = props.insert(predicate("ответственность"));
        let implication = props.insert(PropositionNode::Conditional {
            antecedent: a.clone(),
            consequent: c.clone(),
        });
        let propositions = props.build().expect("dag");

        let mut premises = NonEmptyVec::one(a);
        premises.push(implication);
        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises,
            c.clone(),
            InferenceRuleId::ConditionalElimination,
            EvidenceRef::try_new("fact:freedom_responsibility").expect("evidence"),
            Confidence::from_basis_points(7_000).expect("confidence"),
        ));
        let derivations = derivations.build(&propositions).expect("derivations");

        // Only the conclusion is spoken; the premises justify it silently.
        let discourse = DiscoursePlan::try_new(DiscourseTree::Thesis(c)).expect("plan");
        assert!(CandidateResponsePlan::try_new(propositions, derivations, discourse).is_ok());
    }

    /// Two plans over identical meaning that arrange it differently must not
    /// share a digest: arrangement is part of what is said.
    #[test]
    fn candidate_digest_covers_arrangement_not_only_meaning() {
        let build = |thesis_first: bool| {
            let mut props = PropositionDagBuilder::new();
            let a = props.insert(predicate("свобода"));
            let b = props.insert(predicate("истина"));
            let propositions = props.build().expect("dag");
            let tree = if thesis_first {
                DiscourseTree::Sequence(vec![
                    DiscourseTree::Thesis(a),
                    DiscourseTree::Counterpoint(b),
                ])
            } else {
                DiscourseTree::Sequence(vec![
                    DiscourseTree::Counterpoint(b),
                    DiscourseTree::Thesis(a),
                ])
            };
            CandidateResponsePlan::try_new(
                propositions,
                DerivationDag::empty(),
                DiscoursePlan::try_new(tree).expect("plan"),
            )
            .expect("candidate")
        };

        let forward = build(true);
        let reversed = build(false);
        assert_eq!(
            forward.propositions().merkle_root(),
            reversed.propositions().merkle_root(),
            "the same meaning is stored either way"
        );
        assert_ne!(
            forward.candidate_digest(),
            reversed.candidate_digest(),
            "but the plans say different things"
        );
    }

    #[test]
    fn candidate_digest_is_stable_across_rebuilds() {
        let build = || {
            let mut props = PropositionDagBuilder::new();
            let a = props.insert(predicate("свобода"));
            let propositions = props.build().expect("dag");
            CandidateResponsePlan::try_new(
                propositions,
                DerivationDag::empty(),
                DiscoursePlan::try_new(DiscourseTree::Thesis(a)).expect("plan"),
            )
            .expect("candidate")
        };
        assert_eq!(build().candidate_digest(), build().candidate_digest());
    }
}

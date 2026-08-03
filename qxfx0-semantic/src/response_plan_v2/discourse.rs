//! Rhetorical structure and the role projection (ADR-0034 §2).
//!
//! The discourse tree is the *single* role namespace. V1 carried the same
//! rhetorical fact in three independent places, so nothing prevented a
//! `ClaimRole::Thesis` from holding a `SemanticProposition::Counterpoint` under
//! `DiscourseRelation::Elaboration`. Here the constructor *is* the role, and
//! [`projected_roles`] derives `ClaimRole` from the tree rather than storing it.
//! A stored role could disagree with its position; a projected one cannot.
//!
//! Two ordering decisions are deliberate and opposite to the proposition DAG:
//!
//! * **Sequence order is meaning.** Thesis-then-counterpoint does not read as
//!   counterpoint-then-thesis, so a sequence is positional and is never sorted.
//!   Permuting branches is expected to change every derived `ClaimId` below
//!   them, because it is a different discourse.
//! * **Sequences do not nest directly.** Binary nesting would make
//!   `Sequence(Sequence(a, b), c)` and `Sequence(a, Sequence(b, c))` distinct
//!   trees for one reading, and the two would derive different `ClaimId`s for
//!   the same claim. An n-ary sequence with a no-adjacent-sequence invariant
//!   removes that ambiguity by construction.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::proposition::PropositionId;
use crate::response_plan::ClaimRole;

pub const CLAIM_DOMAIN: &str = "qxfx0:claim:v1";
pub const DISCOURSE_DOMAIN: &str = "qxfx0:discourse:v1";

/// Derived address of one claim: a proposition *as used at one place* in the
/// discourse.
///
/// Never stored and never part of persisted JSON (ADR-0034 §2). The same
/// proposition appearing as both a thesis and a counterpoint is two claims with
/// one meaning, which is precisely the distinction a stored identifier would
/// blur.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct ClaimId(String);

impl ClaimId {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Address of an occurrence, scoped so it is unique beyond its own plan.
///
/// A bare path such as `[0, 1]` repeats across plans, so it is qualified by the
/// digest of the discourse root it belongs to (ADR-0034 §2).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct DiscourseOccurrenceId {
    discourse_root_digest: String,
    canonical_path: String,
}

impl DiscourseOccurrenceId {
    pub fn discourse_root_digest(&self) -> &str {
        &self.discourse_root_digest
    }

    pub fn canonical_path(&self) -> &str {
        &self.canonical_path
    }
}

/// Rhetorical constructors.
///
/// The set is a bijection with [`ClaimRole`], which keeps the projection total
/// and information-preserving. The Haskell reference additionally had a
/// `Statement` constructor that mapped onto the same role as `Thesis`; it is
/// dropped rather than ported, because two constructors sharing one role would
/// reintroduce exactly the ambiguity this layer exists to remove.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DiscourseTree {
    /// The claim being advanced.
    Thesis(PropositionId),
    /// Grounds offered for a neighbouring thesis.
    Support(PropositionId),
    /// The admitted objection.
    Counterpoint(PropositionId),
    /// What follows if the thesis holds.
    Consequence(PropositionId),
    /// A move addressed to the interlocutor rather than to the subject matter.
    DialogueAct(PropositionId),
    /// Ordered composition. Positional, and never directly nested.
    Sequence(Vec<DiscourseTree>),
}

impl DiscourseTree {
    /// Stable constructor tag; part of every derived digest.
    pub const fn constructor(&self) -> &'static str {
        match self {
            Self::Thesis(_) => "thesis",
            Self::Support(_) => "support",
            Self::Counterpoint(_) => "counterpoint",
            Self::Consequence(_) => "consequence",
            Self::DialogueAct(_) => "dialogue_act",
            Self::Sequence(_) => "sequence",
        }
    }

    /// The role this constructor denotes, or `None` for pure structure.
    pub const fn role(&self) -> Option<ClaimRole> {
        match self {
            Self::Thesis(_) => Some(ClaimRole::Thesis),
            Self::Support(_) => Some(ClaimRole::Support),
            Self::Counterpoint(_) => Some(ClaimRole::Counterpoint),
            Self::Consequence(_) => Some(ClaimRole::Consequence),
            Self::DialogueAct(_) => Some(ClaimRole::DialogueAct),
            Self::Sequence(_) => None,
        }
    }

    /// The proposition carried by a leaf.
    pub const fn proposition(&self) -> Option<&PropositionId> {
        match self {
            Self::Thesis(id)
            | Self::Support(id)
            | Self::Counterpoint(id)
            | Self::Consequence(id)
            | Self::DialogueAct(id) => Some(id),
            Self::Sequence(_) => None,
        }
    }

    /// Digest of the whole tree, used to scope occurrence addresses.
    pub fn root_digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(DISCOURSE_DOMAIN.as_bytes());
        self.absorb_into(&mut hasher);
        format!("{:x}", hasher.finalize())
    }

    fn absorb_into(&self, hasher: &mut Sha256) {
        absorb(hasher, self.constructor().as_bytes());
        match self {
            Self::Sequence(children) => {
                absorb_len(hasher, children.len());
                for child in children {
                    child.absorb_into(hasher);
                }
            }
            leaf => {
                let id = leaf.proposition().expect("non-sequence node is a leaf");
                absorb(hasher, id.as_str().as_bytes());
            }
        }
    }

    fn validate_shape(&self) -> Result<(), DiscourseInvariantError> {
        match self {
            Self::Sequence(children) => {
                if children.len() < 2 {
                    return Err(DiscourseInvariantError::DegenerateSequence(children.len()));
                }
                for child in children {
                    // A directly nested sequence would let one reading be
                    // written as several distinct trees.
                    if matches!(child, Self::Sequence(_)) {
                        return Err(DiscourseInvariantError::NestedSequence);
                    }
                    child.validate_shape()?;
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }

    /// Every proposition the discourse actually states, in reading order.
    pub fn stated_propositions(&self) -> Vec<PropositionId> {
        let mut out = Vec::new();
        self.collect_propositions(&mut out);
        out
    }

    fn collect_propositions(&self, out: &mut Vec<PropositionId>) {
        match self {
            Self::Sequence(children) => {
                for child in children {
                    child.collect_propositions(out);
                }
            }
            leaf => out.push(
                leaf.proposition()
                    .expect("non-sequence node is a leaf")
                    .clone(),
            ),
        }
    }

    /// Number of leaves.
    pub fn claim_count(&self) -> usize {
        match self {
            Self::Sequence(children) => children.iter().map(Self::claim_count).sum(),
            _ => 1,
        }
    }
}

fn absorb_len(hasher: &mut Sha256, len: usize) {
    hasher.update((len as u64).to_be_bytes());
}

fn absorb(hasher: &mut Sha256, bytes: &[u8]) {
    absorb_len(hasher, bytes.len());
    hasher.update(bytes);
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DiscourseInvariantError {
    #[error("sequence needs at least two children, got {0}")]
    DegenerateSequence(usize),
    #[error("a sequence must not contain a sequence directly; flatten it")]
    NestedSequence,
}

/// A leaf together with its derived addresses and role.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectedClaim {
    pub claim_id: ClaimId,
    pub occurrence: DiscourseOccurrenceId,
    pub proposition: PropositionId,
    pub role: ClaimRole,
}

/// Validated discourse structure.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiscoursePlan {
    tree: DiscourseTree,
}

impl DiscoursePlan {
    /// The only way to obtain the type, so an unflattened or degenerate tree
    /// cannot reach the role projection.
    pub fn try_new(tree: DiscourseTree) -> Result<Self, DiscourseInvariantError> {
        tree.validate_shape()?;
        Ok(Self { tree })
    }

    pub fn tree(&self) -> &DiscourseTree {
        &self.tree
    }

    pub fn root_digest(&self) -> String {
        self.tree.root_digest()
    }

    pub fn stated_propositions(&self) -> Vec<PropositionId> {
        self.tree.stated_propositions()
    }

    pub fn claim_count(&self) -> usize {
        self.tree.claim_count()
    }

    /// Derive every claim's address and role from position alone.
    ///
    /// This is the whole point of the layer: the role is a view, so it can
    /// never contradict the structure it came from.
    pub fn projected_claims(&self) -> Vec<ProjectedClaim> {
        let root_digest = self.root_digest();
        let mut out = Vec::new();
        collect_claims(&self.tree, &root_digest, &mut Vec::new(), &mut out);
        out
    }
}

/// `ClaimRole` for every claim, keyed by derived identity (ADR-0034 §2).
pub fn projected_roles(plan: &DiscoursePlan) -> BTreeMap<ClaimId, ClaimRole> {
    plan.projected_claims()
        .into_iter()
        .map(|claim| (claim.claim_id, claim.role))
        .collect()
}

/// Canonical path encoding: `constructor` at the root, `index.constructor` for
/// each descent. Indices are fixed by sequence order, which is itself meaning.
fn encode_path(steps: &[String]) -> String {
    steps.join("/")
}

fn collect_claims(
    node: &DiscourseTree,
    root_digest: &str,
    steps: &mut Vec<String>,
    out: &mut Vec<ProjectedClaim>,
) {
    match node {
        DiscourseTree::Sequence(children) => {
            for (index, child) in children.iter().enumerate() {
                steps.push(format!("{index}.{}", child.constructor()));
                collect_claims(child, root_digest, steps, out);
                steps.pop();
            }
        }
        leaf => {
            if steps.is_empty() {
                steps.push(leaf.constructor().to_string());
            }
            let canonical_path = encode_path(steps);
            let proposition = leaf
                .proposition()
                .expect("non-sequence node is a leaf")
                .clone();
            let role = leaf.role().expect("non-sequence node has a role");

            let mut hasher = Sha256::new();
            hasher.update(CLAIM_DOMAIN.as_bytes());
            absorb(&mut hasher, proposition.as_str().as_bytes());
            absorb(&mut hasher, canonical_path.as_bytes());

            out.push(ProjectedClaim {
                claim_id: ClaimId(format!("{:x}", hasher.finalize())),
                occurrence: DiscourseOccurrenceId {
                    discourse_root_digest: root_digest.to_string(),
                    canonical_path,
                },
                proposition,
                role,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan::SemanticId;
    use crate::response_plan_v2::proposition::{PropositionDagBuilder, PropositionNode};

    fn predicate(subject: &str) -> PropositionNode {
        PropositionNode::Predicate {
            subject: SemanticId::try_new(subject).expect("semantic id"),
            relation: SemanticId::try_new("предполагает").expect("semantic id"),
            object: SemanticId::try_new("возможность_выбора").expect("semantic id"),
        }
    }

    fn two_propositions() -> (PropositionId, PropositionId) {
        let mut builder = PropositionDagBuilder::new();
        (
            builder.insert(predicate("свобода")),
            builder.insert(predicate("истина")),
        )
    }

    #[test]
    fn role_comes_from_position_not_from_a_field() {
        let (a, b) = two_propositions();
        let plan = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Thesis(a.clone()),
            DiscourseTree::Counterpoint(b.clone()),
        ]))
        .expect("plan");

        let claims = plan.projected_claims();
        assert_eq!(claims.len(), 2);
        assert_eq!(claims[0].role, ClaimRole::Thesis);
        assert_eq!(claims[0].proposition, a);
        assert_eq!(claims[1].role, ClaimRole::Counterpoint);
        assert_eq!(claims[1].proposition, b);
    }

    /// The same meaning used twice is two claims. A stored role would have to
    /// pick one, which is how V1 could disagree with itself.
    #[test]
    fn one_proposition_in_two_positions_is_two_claims() {
        let (a, _) = two_propositions();
        let plan = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Thesis(a.clone()),
            DiscourseTree::Counterpoint(a.clone()),
        ]))
        .expect("plan");

        let claims = plan.projected_claims();
        assert_eq!(claims[0].proposition, claims[1].proposition);
        assert_ne!(
            claims[0].claim_id, claims[1].claim_id,
            "same meaning at two positions must be two claim ids"
        );

        let roles = projected_roles(&plan);
        assert_eq!(roles.len(), 2);
        assert!(roles.values().any(|role| *role == ClaimRole::Thesis));
        assert!(roles.values().any(|role| *role == ClaimRole::Counterpoint));
    }

    /// Discourse order is meaning, so permuting branches is a different plan.
    /// This is the opposite of `Conjunction`, which sorts.
    #[test]
    fn sequence_order_changes_identity() {
        let (a, b) = two_propositions();
        let forward = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Thesis(a.clone()),
            DiscourseTree::Counterpoint(b.clone()),
        ]))
        .expect("plan");
        let reversed = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Counterpoint(b),
            DiscourseTree::Thesis(a),
        ]))
        .expect("plan");

        assert_ne!(forward.root_digest(), reversed.root_digest());
        assert_ne!(
            forward.projected_claims()[0].claim_id,
            reversed.projected_claims()[0].claim_id
        );
    }

    /// Constructor is part of the path, so the same proposition read as a
    /// thesis and as a support are distinct claims even at the same index.
    #[test]
    fn constructor_participates_in_the_claim_id() {
        let (a, b) = two_propositions();
        let as_thesis = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Thesis(a.clone()),
            DiscourseTree::Counterpoint(b.clone()),
        ]))
        .expect("plan");
        let as_support = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Support(a),
            DiscourseTree::Counterpoint(b),
        ]))
        .expect("plan");

        assert_ne!(
            as_thesis.projected_claims()[0].claim_id,
            as_support.projected_claims()[0].claim_id
        );
    }

    #[test]
    fn nested_sequences_are_rejected() {
        let (a, b) = two_propositions();
        let nested = DiscourseTree::Sequence(vec![
            DiscourseTree::Sequence(vec![
                DiscourseTree::Thesis(a),
                DiscourseTree::Support(b.clone()),
            ]),
            DiscourseTree::Counterpoint(b),
        ]);
        assert!(matches!(
            DiscoursePlan::try_new(nested),
            Err(DiscourseInvariantError::NestedSequence)
        ));
    }

    #[test]
    fn degenerate_sequence_is_rejected() {
        let (a, _) = two_propositions();
        assert!(matches!(
            DiscoursePlan::try_new(DiscourseTree::Sequence(vec![DiscourseTree::Thesis(a)])),
            Err(DiscourseInvariantError::DegenerateSequence(1))
        ));
    }

    #[test]
    fn a_single_leaf_is_a_valid_plan() {
        let (a, _) = two_propositions();
        let plan = DiscoursePlan::try_new(DiscourseTree::Thesis(a.clone())).expect("plan");
        let claims = plan.projected_claims();
        assert_eq!(claims.len(), 1);
        assert_eq!(claims[0].role, ClaimRole::Thesis);
        assert_eq!(claims[0].occurrence.canonical_path(), "thesis");
        assert_eq!(plan.stated_propositions(), vec![a]);
    }

    /// Occurrence addresses are scoped by the root digest, so an identical path
    /// in another discourse is a different occurrence.
    #[test]
    fn occurrence_is_scoped_by_the_discourse_root() {
        let (a, b) = two_propositions();
        let left = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Thesis(a.clone()),
            DiscourseTree::Counterpoint(b.clone()),
        ]))
        .expect("plan");
        let right = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Thesis(a),
            DiscourseTree::Support(b),
        ]))
        .expect("plan");

        let left_first = &left.projected_claims()[0].occurrence;
        let right_first = &right.projected_claims()[0].occurrence;
        assert_eq!(left_first.canonical_path(), right_first.canonical_path());
        assert_ne!(
            left_first.discourse_root_digest(),
            right_first.discourse_root_digest()
        );
        assert_ne!(left_first, right_first);
    }

    #[test]
    fn every_constructor_maps_to_exactly_one_role() {
        let (a, _) = two_propositions();
        let leaves = [
            DiscourseTree::Thesis(a.clone()),
            DiscourseTree::Support(a.clone()),
            DiscourseTree::Counterpoint(a.clone()),
            DiscourseTree::Consequence(a.clone()),
            DiscourseTree::DialogueAct(a),
        ];
        let roles: std::collections::BTreeSet<&'static str> = leaves
            .iter()
            .filter_map(DiscourseTree::role)
            .map(ClaimRole::as_str)
            .collect();
        assert_eq!(
            roles.len(),
            leaves.len(),
            "constructors must map injectively onto roles"
        );
        assert!(DiscourseTree::Sequence(Vec::new()).role().is_none());
    }

    #[test]
    fn stated_propositions_follow_reading_order() {
        let (a, b) = two_propositions();
        let plan = DiscoursePlan::try_new(DiscourseTree::Sequence(vec![
            DiscourseTree::Counterpoint(b.clone()),
            DiscourseTree::Thesis(a.clone()),
        ]))
        .expect("plan");
        assert_eq!(plan.stated_propositions(), vec![b, a]);
        assert_eq!(plan.claim_count(), 2);
    }
}

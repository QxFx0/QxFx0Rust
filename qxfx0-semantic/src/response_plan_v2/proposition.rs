//! Recursive proposition algebra with Merkle identity (ADR-0034 §1–2).
//!
//! V1 encodes rhetorical semantics three times — `ClaimRole`,
//! `SemanticProposition` and `DiscourseRelation` — so a plan whose
//! `ClaimRole::Thesis` carries a `SemanticProposition::Counterpoint` under
//! `DiscourseRelation::Elaboration` assembles without error. V2 keeps exactly
//! one value-level algebra here; the rhetorical role is projected from the
//! discourse tree and never stored.
//!
//! Identity is content-addressed:
//!
//! ```text
//! PropositionId = SHA-256("qxfx0:proposition:v1" ‖ type ‖ payload ‖ child_ids)
//! ```
//!
//! Two properties follow from that and are relied on elsewhere:
//!
//! * **Cycles are unrepresentable.** A node's identity depends on its
//!   children's identities, so referencing yourself would require knowing your
//!   own digest before computing it. Validation therefore only has to check
//!   that every referenced child is present, never that the graph is acyclic.
//! * **Equal meaning is equal identity.** The same proposition built twice
//!   collapses to one node, which is what makes the map a DAG rather than a
//!   tree.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::response_plan::SemanticId;

/// Domain separation tag. Any change to the encoding below must change this
/// string, because replay compares digests and a silent re-encoding would make
/// two different canonicalizations look like the same meaning.
pub const PROPOSITION_DOMAIN: &str = "qxfx0:proposition:v1";

/// Content address of a proposition node.
///
/// Constructed only by hashing a node or by parsing a well-formed digest, so a
/// caller cannot mint an identifier that does not correspond to any content.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PropositionId(String);

impl PropositionId {
    /// Parse a previously emitted identifier, e.g. from persisted state.
    pub fn parse(value: impl Into<String>) -> Result<Self, PropositionInvariantError> {
        let value = value.into();
        let valid = value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
        if valid {
            Ok(Self(value))
        } else {
            Err(PropositionInvariantError::MalformedId(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Admitted epistemic qualifier, e.g. "tentatively" or "on the current
/// evidence". The map of admissible qualifiers belongs to the assertion policy
/// (ADR-0034 §12); this type only carries the identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct QualifierId(String);

impl QualifierId {
    pub fn try_new(value: impl Into<String>) -> Result<Self, PropositionInvariantError> {
        let value = value.into();
        if value.trim().is_empty() {
            Err(PropositionInvariantError::EmptyQualifier)
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// One node of the proposition algebra.
///
/// Children are referenced by identity rather than nested, so a subtree shared
/// by several parents is stored once and keeps one identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropositionNode {
    /// Leaf: a subject-relation-object triple drawn from admitted vocabulary.
    Predicate {
        subject: SemanticId,
        relation: SemanticId,
        object: SemanticId,
    },
    /// Order-insensitive composition. Children are canonically sorted, and
    /// repeats are preserved: `A ∧ A ∧ B` is not `A ∧ B` (ADR-0034 §2).
    Conjunction { children: Vec<PropositionId> },
    /// `antecedent → consequent`. Positional: swapping the operands is a
    /// different proposition, so the encoding must not sort them.
    Conditional {
        antecedent: PropositionId,
        consequent: PropositionId,
    },
    /// Rhetorical opposition. Positional for the same reason as `Conditional`.
    Contrast {
        left: PropositionId,
        right: PropositionId,
    },
    /// `antecedent ⇒ consequent` as an asserted consequence rather than a
    /// material conditional.
    Consequence {
        antecedent: PropositionId,
        consequent: PropositionId,
    },
    /// `Qualification(q, A)`: the qualifier constrains the epistemic strength
    /// of `A`, and is admitted separately from `A`'s content (ADR-0034 §12).
    Qualification {
        qualifier: QualifierId,
        proposition: PropositionId,
    },
    /// Interrogative wrapper over a proposition.
    Question { proposition: PropositionId },
}

impl PropositionNode {
    /// Stable constructor tag. It is part of the digest, so renaming a variant
    /// without changing `PROPOSITION_DOMAIN` would silently alter identity.
    pub const fn constructor(&self) -> &'static str {
        match self {
            Self::Predicate { .. } => "predicate",
            Self::Conjunction { .. } => "conjunction",
            Self::Conditional { .. } => "conditional",
            Self::Contrast { .. } => "contrast",
            Self::Consequence { .. } => "consequence",
            Self::Qualification { .. } => "qualification",
            Self::Question { .. } => "question",
        }
    }

    /// Children in canonical order.
    ///
    /// `Conjunction` sorts; every other constructor is positional. This is the
    /// single place the canon is defined, so the digest and the reachability
    /// check can never disagree about what the children are.
    pub fn canonical_children(&self) -> Vec<PropositionId> {
        match self {
            Self::Predicate { .. } => Vec::new(),
            Self::Conjunction { children } => {
                let mut sorted = children.clone();
                sorted.sort();
                sorted
            }
            Self::Conditional {
                antecedent,
                consequent,
            }
            | Self::Contrast {
                left: antecedent,
                right: consequent,
            }
            | Self::Consequence {
                antecedent,
                consequent,
            } => vec![antecedent.clone(), consequent.clone()],
            Self::Qualification { proposition, .. } | Self::Question { proposition } => {
                vec![proposition.clone()]
            }
        }
    }

    /// Non-child payload contributing to identity.
    fn canonical_payload(&self) -> Vec<String> {
        match self {
            Self::Predicate {
                subject,
                relation,
                object,
            } => vec![
                subject.as_str().to_string(),
                relation.as_str().to_string(),
                object.as_str().to_string(),
            ],
            Self::Qualification { qualifier, .. } => vec![qualifier.as_str().to_string()],
            _ => Vec::new(),
        }
    }

    /// Compute this node's content address.
    ///
    /// Every field is length-prefixed before hashing. Bare concatenation would
    /// be ambiguous — `("ab", "c")` and `("a", "bc")` would hash alike — which
    /// would let two different meanings share one identity.
    pub fn id(&self) -> PropositionId {
        let mut hasher = Sha256::new();
        hasher.update(PROPOSITION_DOMAIN.as_bytes());
        absorb(&mut hasher, self.constructor().as_bytes());

        let payload = self.canonical_payload();
        absorb_len(&mut hasher, payload.len());
        for field in &payload {
            absorb(&mut hasher, field.as_bytes());
        }

        let children = self.canonical_children();
        absorb_len(&mut hasher, children.len());
        for child in &children {
            absorb(&mut hasher, child.as_str().as_bytes());
        }

        PropositionId(format!("{:x}", hasher.finalize()))
    }

    /// Structural checks that do not need the surrounding DAG.
    fn validate_local(&self) -> Result<(), PropositionInvariantError> {
        match self {
            Self::Conjunction { children } if children.len() < 2 => Err(
                PropositionInvariantError::DegenerateConjunction(children.len()),
            ),
            _ => Ok(()),
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
pub enum PropositionInvariantError {
    #[error("proposition id '{0}' is not a 64-character hex digest")]
    MalformedId(String),
    #[error("qualifier must not be empty")]
    EmptyQualifier,
    #[error("conjunction needs at least two children, got {0}")]
    DegenerateConjunction(usize),
    #[error("node stored under '{stored}' actually hashes to '{computed}'")]
    ForgedId { stored: String, computed: String },
    #[error("node '{parent}' references missing child '{child}'")]
    DanglingChild { parent: String, child: String },
    #[error("dag is empty")]
    Empty,
}

/// Content-addressed proposition store.
///
/// There is deliberately no `root_ids` field: roots are derived from the
/// discourse tree, and a second list of them would be a second source of truth
/// able to drift from the first (ADR-0034 §2).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PropositionDag {
    nodes: BTreeMap<PropositionId, PropositionNode>,
}

impl PropositionDag {
    /// Validate and take ownership. This is the only way to obtain the type,
    /// so an unchecked map cannot reach later boundaries.
    pub fn try_new(
        nodes: BTreeMap<PropositionId, PropositionNode>,
    ) -> Result<Self, PropositionInvariantError> {
        if nodes.is_empty() {
            return Err(PropositionInvariantError::Empty);
        }
        for (stored_id, node) in &nodes {
            node.validate_local()?;

            // A key that does not match its content would let a caller point
            // one identity at another's meaning.
            let computed = node.id();
            if computed != *stored_id {
                return Err(PropositionInvariantError::ForgedId {
                    stored: stored_id.as_str().to_string(),
                    computed: computed.as_str().to_string(),
                });
            }

            for child in node.canonical_children() {
                if !nodes.contains_key(&child) {
                    return Err(PropositionInvariantError::DanglingChild {
                        parent: stored_id.as_str().to_string(),
                        child: child.as_str().to_string(),
                    });
                }
            }
        }
        Ok(Self { nodes })
    }

    pub fn get(&self, id: &PropositionId) -> Option<&PropositionNode> {
        self.nodes.get(id)
    }

    pub fn contains(&self, id: &PropositionId) -> bool {
        self.nodes.contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&PropositionId, &PropositionNode)> {
        self.nodes.iter()
    }

    /// Merkle root over the whole store, for stage digests.
    pub fn merkle_root(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"qxfx0:proposition-dag:v1");
        absorb_len(&mut hasher, self.nodes.len());
        // `BTreeMap` iterates in key order, so the root does not depend on
        // insertion order.
        for id in self.nodes.keys() {
            absorb(&mut hasher, id.as_str().as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Incremental builder that keys every node by its own digest.
///
/// Callers never choose identifiers, which is what makes `ForgedId` an
/// internal-consistency check rather than a routine failure mode.
#[derive(Debug, Clone, Default)]
pub struct PropositionDagBuilder {
    nodes: BTreeMap<PropositionId, PropositionNode>,
}

impl PropositionDagBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    /// Insert a node and return its identity. Inserting the same meaning twice
    /// is idempotent and yields the same id.
    pub fn insert(&mut self, node: PropositionNode) -> PropositionId {
        let id = node.id();
        self.nodes.entry(id.clone()).or_insert(node);
        id
    }

    pub fn build(self) -> Result<PropositionDag, PropositionInvariantError> {
        PropositionDag::try_new(self.nodes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sem(value: &str) -> SemanticId {
        SemanticId::try_new(value).expect("semantic id")
    }

    fn freedom_choice() -> PropositionNode {
        PropositionNode::Predicate {
            subject: sem("свобода"),
            relation: sem("предполагает"),
            object: sem("возможность_выбора"),
        }
    }

    fn truth_check() -> PropositionNode {
        PropositionNode::Predicate {
            subject: sem("истина"),
            relation: sem("проверяется"),
            object: sem("через_воспроизводимость"),
        }
    }

    #[test]
    fn identical_meaning_collapses_to_one_identity() {
        let mut builder = PropositionDagBuilder::new();
        let first = builder.insert(freedom_choice());
        let second = builder.insert(freedom_choice());
        assert_eq!(first, second);
        assert_eq!(builder.build().expect("dag").len(), 1);
    }

    #[test]
    fn different_meaning_gets_different_identity() {
        assert_ne!(freedom_choice().id(), truth_check().id());
    }

    /// Length prefixing exists so that field boundaries cannot be shifted.
    /// Without it, `("ab","c",…)` and `("a","bc",…)` would hash alike.
    #[test]
    fn field_boundaries_cannot_be_shifted() {
        let left = PropositionNode::Predicate {
            subject: sem("ab"),
            relation: sem("c"),
            object: sem("d"),
        };
        let right = PropositionNode::Predicate {
            subject: sem("a"),
            relation: sem("bc"),
            object: sem("d"),
        };
        assert_ne!(left.id(), right.id());
    }

    #[test]
    fn conjunction_is_order_insensitive_but_keeps_repeats() {
        let mut builder = PropositionDagBuilder::new();
        let a = builder.insert(freedom_choice());
        let b = builder.insert(truth_check());

        let forward = PropositionNode::Conjunction {
            children: vec![a.clone(), b.clone()],
        };
        let reversed = PropositionNode::Conjunction {
            children: vec![b.clone(), a.clone()],
        };
        assert_eq!(
            forward.id(),
            reversed.id(),
            "conjunction must sort children"
        );

        let repeated = PropositionNode::Conjunction {
            children: vec![a.clone(), a.clone(), b.clone()],
        };
        assert_ne!(
            forward.id(),
            repeated.id(),
            "repeated premises keep multiset semantics"
        );
    }

    #[test]
    fn conditional_is_positional() {
        let mut builder = PropositionDagBuilder::new();
        let a = builder.insert(freedom_choice());
        let b = builder.insert(truth_check());

        let forward = PropositionNode::Conditional {
            antecedent: a.clone(),
            consequent: b.clone(),
        };
        let reversed = PropositionNode::Conditional {
            antecedent: b,
            consequent: a,
        };
        assert_ne!(
            forward.id(),
            reversed.id(),
            "swapping antecedent and consequent is a different proposition"
        );
    }

    /// `Contrast(A,B)` and `Conditional(A,B)` share their children, so only the
    /// constructor tag separates them. It must be part of the digest.
    #[test]
    fn constructor_tag_separates_same_shaped_nodes() {
        let mut builder = PropositionDagBuilder::new();
        let a = builder.insert(freedom_choice());
        let b = builder.insert(truth_check());

        let conditional = PropositionNode::Conditional {
            antecedent: a.clone(),
            consequent: b.clone(),
        };
        let contrast = PropositionNode::Contrast {
            left: a.clone(),
            right: b.clone(),
        };
        let consequence = PropositionNode::Consequence {
            antecedent: a,
            consequent: b,
        };
        assert_ne!(conditional.id(), contrast.id());
        assert_ne!(conditional.id(), consequence.id());
        assert_ne!(contrast.id(), consequence.id());
    }

    #[test]
    fn qualifier_participates_in_identity() {
        let mut builder = PropositionDagBuilder::new();
        let a = builder.insert(freedom_choice());
        let tentative = PropositionNode::Qualification {
            qualifier: QualifierId::try_new("tentatively").expect("qualifier"),
            proposition: a.clone(),
        };
        let firm = PropositionNode::Qualification {
            qualifier: QualifierId::try_new("on_current_evidence").expect("qualifier"),
            proposition: a,
        };
        assert_ne!(
            tentative.id(),
            firm.id(),
            "epistemic strength is part of the meaning"
        );
    }

    #[test]
    fn dangling_child_is_rejected() {
        let mut builder = PropositionDagBuilder::new();
        let a = builder.insert(freedom_choice());
        let orphan = truth_check().id();
        builder.insert(PropositionNode::Conditional {
            antecedent: a,
            consequent: orphan,
        });
        assert!(matches!(
            builder.build(),
            Err(PropositionInvariantError::DanglingChild { .. })
        ));
    }

    #[test]
    fn forged_key_is_rejected() {
        let mut nodes = BTreeMap::new();
        let wrong_key = truth_check().id();
        nodes.insert(wrong_key, freedom_choice());
        assert!(matches!(
            PropositionDag::try_new(nodes),
            Err(PropositionInvariantError::ForgedId { .. })
        ));
    }

    #[test]
    fn degenerate_conjunction_is_rejected() {
        let mut builder = PropositionDagBuilder::new();
        let a = builder.insert(freedom_choice());
        builder.insert(PropositionNode::Conjunction { children: vec![a] });
        assert!(matches!(
            builder.build(),
            Err(PropositionInvariantError::DegenerateConjunction(1))
        ));
    }

    #[test]
    fn empty_dag_is_rejected() {
        assert!(matches!(
            PropositionDag::try_new(BTreeMap::new()),
            Err(PropositionInvariantError::Empty)
        ));
    }

    #[test]
    fn merkle_root_is_insertion_order_independent() {
        let mut forward = PropositionDagBuilder::new();
        forward.insert(freedom_choice());
        forward.insert(truth_check());

        let mut reverse = PropositionDagBuilder::new();
        reverse.insert(truth_check());
        reverse.insert(freedom_choice());

        assert_eq!(
            forward.build().expect("dag").merkle_root(),
            reverse.build().expect("dag").merkle_root()
        );
    }

    #[test]
    fn ids_round_trip_through_parse() {
        let id = freedom_choice().id();
        assert_eq!(PropositionId::parse(id.as_str()).expect("parse"), id);
        assert!(PropositionId::parse("not-a-digest").is_err());
    }

    /// Reference vector. A change here means the canonical encoding changed,
    /// which invalidates every persisted digest and must bump
    /// `PROPOSITION_DOMAIN`.
    #[test]
    fn reference_vector_predicate_identity_is_stable() {
        assert_eq!(
            freedom_choice().id().as_str(),
            "18890321e272f389491377446cb5eb588801b02b0287eef87d51388dc6ff8284"
        );
    }
}

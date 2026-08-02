//! Typed entailment layer (ADR-0034 §3).
//!
//! Graph connectivity is not implication. Breadth-first reachability over the
//! semantic network produces *candidates*; only a whitelisted rule that
//! actually confirms the step produces a conclusion. That distinction is the
//! reason this layer exists separately from the proposition DAG: `A` and `B`
//! being adjacent in the graph says nothing about whether `A ⇒ B` may be
//! asserted.
//!
//! Confirmation is structural. Each rule checks that the premises and the
//! conclusion really stand in its shape, so a caller cannot label an arbitrary
//! pair with `ConditionalElimination` and have it admitted.
//!
//! Being derivable is still not being assertable — that verdict belongs to the
//! assertion-authority boundary and is deliberately not decided here.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use super::proposition::{PropositionDag, PropositionId, PropositionNode};
use crate::response_plan::{Confidence, NonEmptyVec};

pub const DERIVATION_DOMAIN: &str = "qxfx0:derivation:v1";

/// Content address of a derivation step.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct DerivationId(String);

impl DerivationId {
    pub fn parse(value: impl Into<String>) -> Result<Self, DerivationInvariantError> {
        let value = value.into();
        let valid = value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit());
        if valid {
            Ok(Self(value))
        } else {
            Err(DerivationInvariantError::MalformedId(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Reference to the evidence backing a derivation.
///
/// L1 carries the identity only. Whether that evidence is admitted, and
/// whether it is still active under the current authority snapshot, are two
/// separate certificates at a later boundary (ADR-0034 §4).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct EvidenceRef(String);

impl EvidenceRef {
    pub fn try_new(value: impl Into<String>) -> Result<Self, DerivationInvariantError> {
        let value = value.into();
        if value.trim().is_empty() {
            Err(DerivationInvariantError::EmptyEvidence)
        } else {
            Ok(Self(value))
        }
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The closed whitelist of inference rules.
///
/// A closed enum rather than an open registry: admitting a new form of
/// reasoning is a reviewed change to this contract, not configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InferenceRuleId {
    /// `A, B ⊢ A ∧ B`
    ConjunctionIntroduction,
    /// `A, A → C ⊢ C` (modus ponens)
    ConditionalElimination,
    /// `A, B ⊢ Contrast(A, B)`
    ContrastIntroduction,
    /// `A, B ⊢ Consequence(A, B)`
    ConsequenceIntroduction,
}

impl InferenceRuleId {
    pub const ALL: [Self; 4] = [
        Self::ConjunctionIntroduction,
        Self::ConditionalElimination,
        Self::ContrastIntroduction,
        Self::ConsequenceIntroduction,
    ];

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ConjunctionIntroduction => "conjunction_introduction",
            Self::ConditionalElimination => "conditional_elimination",
            Self::ContrastIntroduction => "contrast_introduction",
            Self::ConsequenceIntroduction => "consequence_introduction",
        }
    }

    /// Confirm that the premises and conclusion really stand in this rule's
    /// shape. Whitelist membership alone admits nothing.
    fn confirms(
        self,
        premises: &NonEmptyVec<PropositionId>,
        conclusion: &PropositionId,
        dag: &PropositionDag,
    ) -> Result<(), DerivationInvariantError> {
        let conclusion_node =
            dag.get(conclusion)
                .ok_or_else(|| DerivationInvariantError::UnknownProposition {
                    role: "conclusion",
                    id: conclusion.as_str().to_string(),
                })?;
        let premise_ids: Vec<PropositionId> = premises.iter().cloned().collect();

        let unmatched = || DerivationInvariantError::RuleNotConfirmed {
            rule: self,
            detail: format!(
                "premises {:?} do not stand in the {} shape for conclusion {}",
                premise_ids
                    .iter()
                    .map(|id| short(id.as_str()))
                    .collect::<Vec<_>>(),
                self.as_str(),
                short(conclusion.as_str())
            ),
        };

        match (self, conclusion_node) {
            (Self::ConjunctionIntroduction, PropositionNode::Conjunction { .. }) => {
                // The conjunction must be exactly of the premises, as a
                // multiset: neither dropping nor inventing a conjunct.
                let mut expected = premise_ids.clone();
                expected.sort();
                if conclusion_node.canonical_children() == expected {
                    Ok(())
                } else {
                    Err(unmatched())
                }
            }
            (Self::ContrastIntroduction, PropositionNode::Contrast { left, right })
            | (
                Self::ConsequenceIntroduction,
                PropositionNode::Consequence {
                    antecedent: left,
                    consequent: right,
                },
            ) => {
                if premise_ids == vec![left.clone(), right.clone()] {
                    Ok(())
                } else {
                    Err(unmatched())
                }
            }
            (Self::ConditionalElimination, _) => {
                // Some premise must be `A → conclusion`, and `A` must itself
                // be among the premises. Without the second half, an unproven
                // antecedent would smuggle its consequent in.
                let discharged = premise_ids.iter().any(|candidate| {
                    matches!(
                        dag.get(candidate),
                        Some(PropositionNode::Conditional {
                            antecedent,
                            consequent,
                        }) if consequent == conclusion && premise_ids.contains(antecedent)
                    )
                });
                if discharged {
                    Ok(())
                } else {
                    Err(unmatched())
                }
            }
            _ => Err(unmatched()),
        }
    }
}

fn short(digest: &str) -> String {
    digest.chars().take(12).collect()
}

/// One confirmed inference step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivationNode {
    premises: NonEmptyVec<PropositionId>,
    conclusion: PropositionId,
    rule: InferenceRuleId,
    evidence: EvidenceRef,
    confidence: Confidence,
}

impl DerivationNode {
    pub fn new(
        premises: NonEmptyVec<PropositionId>,
        conclusion: PropositionId,
        rule: InferenceRuleId,
        evidence: EvidenceRef,
        confidence: Confidence,
    ) -> Self {
        Self {
            premises,
            conclusion,
            rule,
            evidence,
            confidence,
        }
    }

    pub fn premises(&self) -> &NonEmptyVec<PropositionId> {
        &self.premises
    }

    pub fn conclusion(&self) -> &PropositionId {
        &self.conclusion
    }

    pub fn rule(&self) -> InferenceRuleId {
        self.rule
    }

    pub fn evidence(&self) -> &EvidenceRef {
        &self.evidence
    }

    pub fn confidence(&self) -> Confidence {
        self.confidence
    }

    /// Content address. Premises are hashed in their given order because the
    /// order is part of what the rule confirmed.
    pub fn id(&self) -> DerivationId {
        let mut hasher = Sha256::new();
        hasher.update(DERIVATION_DOMAIN.as_bytes());
        absorb(&mut hasher, self.rule.as_str().as_bytes());
        absorb(&mut hasher, self.conclusion.as_str().as_bytes());
        absorb(&mut hasher, self.evidence.as_str().as_bytes());
        hasher.update(self.confidence.basis_points().to_be_bytes());
        let premises: Vec<&PropositionId> = self.premises.iter().collect();
        absorb_len(&mut hasher, premises.len());
        for premise in premises {
            absorb(&mut hasher, premise.as_str().as_bytes());
        }
        DerivationId(format!("{:x}", hasher.finalize()))
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
pub enum DerivationInvariantError {
    #[error("derivation id '{0}' is not a 64-character hex digest")]
    MalformedId(String),
    #[error("evidence reference must not be empty")]
    EmptyEvidence,
    #[error("{role} '{id}' is not present in the proposition dag")]
    UnknownProposition { role: &'static str, id: String },
    #[error("rule {rule:?} does not confirm this step: {detail}")]
    RuleNotConfirmed {
        rule: InferenceRuleId,
        detail: String,
    },
    #[error("conclusion '{0}' is also one of its own premises")]
    CircularJustification(String),
    #[error("node stored under '{stored}' actually hashes to '{computed}'")]
    ForgedId { stored: String, computed: String },
}

/// Confirmed derivation steps over one proposition DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DerivationDag {
    nodes: BTreeMap<DerivationId, DerivationNode>,
}

impl DerivationDag {
    /// Validate every step against the proposition DAG it refers to.
    ///
    /// An empty set is admissible: a plan may state curated content without
    /// deriving anything.
    pub fn try_new(
        nodes: BTreeMap<DerivationId, DerivationNode>,
        propositions: &PropositionDag,
    ) -> Result<Self, DerivationInvariantError> {
        for (stored_id, node) in &nodes {
            let computed = node.id();
            if computed != *stored_id {
                return Err(DerivationInvariantError::ForgedId {
                    stored: stored_id.as_str().to_string(),
                    computed: computed.as_str().to_string(),
                });
            }

            for premise in node.premises.iter() {
                if !propositions.contains(premise) {
                    return Err(DerivationInvariantError::UnknownProposition {
                        role: "premise",
                        id: premise.as_str().to_string(),
                    });
                }
                // Concluding one of your own premises is circular
                // justification, not inference.
                if premise == &node.conclusion {
                    return Err(DerivationInvariantError::CircularJustification(
                        node.conclusion.as_str().to_string(),
                    ));
                }
            }

            node.rule
                .confirms(&node.premises, &node.conclusion, propositions)?;
        }
        Ok(Self { nodes })
    }

    pub fn empty() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    pub fn get(&self, id: &DerivationId) -> Option<&DerivationNode> {
        self.nodes.get(id)
    }

    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&DerivationId, &DerivationNode)> {
        self.nodes.iter()
    }

    pub fn merkle_root(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(b"qxfx0:derivation-dag:v1");
        absorb_len(&mut hasher, self.nodes.len());
        for id in self.nodes.keys() {
            absorb(&mut hasher, id.as_str().as_bytes());
        }
        format!("{:x}", hasher.finalize())
    }
}

/// Builder that keys each step by its own digest.
#[derive(Debug, Clone, Default)]
pub struct DerivationDagBuilder {
    nodes: BTreeMap<DerivationId, DerivationNode>,
}

impl DerivationDagBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, node: DerivationNode) -> DerivationId {
        let id = node.id();
        self.nodes.entry(id.clone()).or_insert(node);
        id
    }

    pub fn build(
        self,
        propositions: &PropositionDag,
    ) -> Result<DerivationDag, DerivationInvariantError> {
        DerivationDag::try_new(self.nodes, propositions)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::response_plan::SemanticId;
    use crate::response_plan_v2::proposition::PropositionDagBuilder;

    fn sem(value: &str) -> SemanticId {
        SemanticId::try_new(value).expect("semantic id")
    }

    fn predicate(subject: &str) -> PropositionNode {
        PropositionNode::Predicate {
            subject: sem(subject),
            relation: sem("предполагает"),
            object: sem("возможность_выбора"),
        }
    }

    fn evidence() -> EvidenceRef {
        EvidenceRef::try_new("fact:freedom_choice").expect("evidence")
    }

    fn confidence() -> Confidence {
        Confidence::from_basis_points(7_500).expect("confidence")
    }

    fn premises(ids: &[PropositionId]) -> NonEmptyVec<PropositionId> {
        let mut iter = ids.iter().cloned();
        let mut out = NonEmptyVec::one(iter.next().expect("at least one premise"));
        for id in iter {
            out.push(id);
        }
        out
    }

    #[test]
    fn conjunction_introduction_is_confirmed_for_its_own_conjuncts() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let b = props.insert(predicate("истина"));
        let conjunction = props.insert(PropositionNode::Conjunction {
            children: vec![a.clone(), b.clone()],
        });
        let dag = props.build().expect("dag");

        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises(&[a, b]),
            conjunction,
            InferenceRuleId::ConjunctionIntroduction,
            evidence(),
            confidence(),
        ));
        assert!(derivations.build(&dag).is_ok());
    }

    /// The conclusion must be exactly the conjunction of the premises. A
    /// conjunct that was never a premise is invention, not inference.
    #[test]
    fn conjunction_introduction_rejects_an_uninvited_conjunct() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let b = props.insert(predicate("истина"));
        let c = props.insert(predicate("память"));
        let conjunction = props.insert(PropositionNode::Conjunction {
            children: vec![a.clone(), b.clone(), c],
        });
        let dag = props.build().expect("dag");

        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises(&[a, b]),
            conjunction,
            InferenceRuleId::ConjunctionIntroduction,
            evidence(),
            confidence(),
        ));
        assert!(matches!(
            derivations.build(&dag),
            Err(DerivationInvariantError::RuleNotConfirmed { .. })
        ));
    }

    #[test]
    fn modus_ponens_is_confirmed_when_the_antecedent_is_discharged() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let c = props.insert(predicate("ответственность"));
        let implication = props.insert(PropositionNode::Conditional {
            antecedent: a.clone(),
            consequent: c.clone(),
        });
        let dag = props.build().expect("dag");

        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises(&[a, implication]),
            c,
            InferenceRuleId::ConditionalElimination,
            evidence(),
            confidence(),
        ));
        assert!(derivations.build(&dag).is_ok());
    }

    /// Holding `A → C` without holding `A` does not license `C`. This is the
    /// case that separates confirmation from whitelist membership.
    #[test]
    fn modus_ponens_rejects_an_undischarged_antecedent() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let c = props.insert(predicate("ответственность"));
        let implication = props.insert(PropositionNode::Conditional {
            antecedent: a,
            consequent: c.clone(),
        });
        let dag = props.build().expect("dag");

        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises(&[implication]),
            c,
            InferenceRuleId::ConditionalElimination,
            evidence(),
            confidence(),
        ));
        assert!(matches!(
            derivations.build(&dag),
            Err(DerivationInvariantError::RuleNotConfirmed { .. })
        ));
    }

    #[test]
    fn concluding_a_premise_is_circular() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let b = props.insert(predicate("истина"));
        props.insert(PropositionNode::Conjunction {
            children: vec![a.clone(), b.clone()],
        });
        let dag = props.build().expect("dag");

        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises(&[a.clone(), b]),
            a,
            InferenceRuleId::ConjunctionIntroduction,
            evidence(),
            confidence(),
        ));
        assert!(matches!(
            derivations.build(&dag),
            Err(DerivationInvariantError::CircularJustification(_))
        ));
    }

    #[test]
    fn premises_outside_the_proposition_dag_are_rejected() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let b = props.insert(predicate("истина"));
        let conjunction = props.insert(PropositionNode::Conjunction {
            children: vec![a.clone(), b.clone()],
        });
        let dag = props.build().expect("dag");

        let stranger = predicate("власть").id();
        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises(&[a, stranger]),
            conjunction,
            InferenceRuleId::ConjunctionIntroduction,
            evidence(),
            confidence(),
        ));
        assert!(matches!(
            derivations.build(&dag),
            Err(DerivationInvariantError::UnknownProposition {
                role: "premise",
                ..
            })
        ));
    }

    /// A rule may not be used to conclude a node of the wrong constructor.
    #[test]
    fn a_rule_cannot_conclude_a_foreign_constructor() {
        let mut props = PropositionDagBuilder::new();
        let a = props.insert(predicate("свобода"));
        let b = props.insert(predicate("истина"));
        let contrast = props.insert(PropositionNode::Contrast {
            left: a.clone(),
            right: b.clone(),
        });
        let dag = props.build().expect("dag");

        let mut derivations = DerivationDagBuilder::new();
        derivations.insert(DerivationNode::new(
            premises(&[a, b]),
            contrast,
            InferenceRuleId::ConjunctionIntroduction,
            evidence(),
            confidence(),
        ));
        assert!(matches!(
            derivations.build(&dag),
            Err(DerivationInvariantError::RuleNotConfirmed { .. })
        ));
    }

    #[test]
    fn an_empty_derivation_set_is_admissible() {
        let mut props = PropositionDagBuilder::new();
        props.insert(predicate("свобода"));
        let dag = props.build().expect("dag");
        assert!(DerivationDag::try_new(BTreeMap::new(), &dag).is_ok());
    }

    #[test]
    fn every_whitelisted_rule_has_a_stable_name() {
        let mut seen = std::collections::BTreeSet::new();
        for rule in InferenceRuleId::ALL {
            assert!(seen.insert(rule.as_str()), "duplicate rule name");
        }
        assert_eq!(seen.len(), InferenceRuleId::ALL.len());
    }
}

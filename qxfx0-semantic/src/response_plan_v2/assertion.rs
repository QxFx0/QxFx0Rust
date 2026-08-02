//! Assertion authority: the recursive per-constructor policy
//! (ADR-0034 §5).
//!
//! `Assertable(p)` and `Derivable(p)` are two different verdicts: being
//! provable does not confer the right to assert the conclusion to a user. This
//! boundary walks the proposition subtree of every stated claim and grants
//! `ClaimAuthority` per the closed policy:
//!
//! ```text
//! Predicate(A)        → curated FactId(A)
//! Conjunction(A, B)   → authority(A) ∧ authority(B) ∧ admitted conjunction rule
//! Conditional(A, B)   → authority(A) ∧ authority(B) ∧ admitted implication proof
//! Qualification(q, A) → authority(A) ∧ admitted_qualification(q) ∧ confidence(A) satisfies q
//! Consequence(A, B)   → authority(B) ∧ admitted derivation(A ⇒ B)
//! Question(A)         → authority(A)   (a wrapper satisfies its claim's binding)
//! ```
//!
//! A leaf predicate is authorized by content: its subject-relation-object
//! triple must be a curated fact. Composite constructors additionally require
//! the confirming whitelisted derivation to be present in this plan —
//! connectivity is not implication (ADR-0034 §3).
//!
//! `ClaimAuthority` is closed to two variants in V1:
//!
//! ```text
//! Curated { fact_id }
//! DerivedCuratedConclusion { conclusion_fact_id, derivation_id }
//! ```
//!
//! `DerivedNovelConclusion` is a future release and is deliberately absent: a
//! derived conclusion whose conclusion is not itself curated fails closed,
//! because the V1 derived stratum explains curated content and does not invent
//! facts (ADR-0034 §5, Consequences).
//!
//! The semantic self-constraints are part of this boundary's vocabulary:
//! `NotAuthorized ≠ False`, `NotAuthorized ≠ Refuted`, `Rejected ≠ Opposed`.

use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

use crate::fact_model::{FactId, FactRegistry, FactStatus};
use crate::response_plan_v2::derivation::{DerivationDag, DerivationId, InferenceRuleId};
use crate::response_plan_v2::discourse::ClaimId;
use crate::response_plan_v2::evidence::EvidenceCertifiedPlan;
use crate::response_plan_v2::proposition::{PropositionId, PropositionNode, QualifierId};

/// Domain separation tag for the policy digest.
pub const ASSERTION_POLICY_DOMAIN: &str = "qxfx0:assertion-policy:v1";

/// One admitted qualifier and the epistemic strength it demands. The map of
/// admissible qualifiers belongs to the assertion policy and therefore to the
/// authority snapshot (ADR-0034 §12).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize)]
pub struct QualifierAdmission {
    pub qualifier: QualifierId,
    /// Minimum confidence of the qualified proposition, in basis points.
    pub required_confidence_bps: u16,
}

/// The assertion policy: which qualifiers are admitted and what they demand.
///
/// V1 admits no qualifiers: the audited corpus asserts plain predicates, and
/// the system must not assert "безусловно A" or "сомнительно A" for a fact
/// whose authority authorized only A's content (ADR-0034 §12).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AssertionPolicy {
    admitted_qualifiers: BTreeSet<QualifierAdmission>,
}

impl AssertionPolicy {
    pub fn v1() -> Self {
        Self::default()
    }

    pub fn with_qualifier(mut self, admission: QualifierAdmission) -> Self {
        self.admitted_qualifiers.insert(admission);
        self
    }

    pub fn qualifier(&self, qualifier: &QualifierId) -> Option<&QualifierAdmission> {
        self.admitted_qualifiers
            .iter()
            .find(|admission| &admission.qualifier == qualifier)
    }

    pub fn len(&self) -> usize {
        self.admitted_qualifiers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.admitted_qualifiers.is_empty()
    }

    /// Canonical digest of the policy; part of the authority snapshot.
    pub fn digest(&self) -> String {
        let mut hasher = Sha256::new();
        hasher.update(ASSERTION_POLICY_DOMAIN.as_bytes());
        for admission in &self.admitted_qualifiers {
            absorb(&mut hasher, admission.qualifier.as_str().as_bytes());
            absorb(
                &mut hasher,
                admission.required_confidence_bps.to_string().as_bytes(),
            );
        }
        format!("{:x}", hasher.finalize())
    }
}

/// How a claim's content is grounded. Closed in V1 to two variants.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ClaimAuthority {
    /// The claim's content is an admitted curated fact.
    Curated { fact_id: FactId },
    /// The claim's content is the confirmed conclusion of a whitelisted
    /// derivation over curated premises, and the conclusion itself is a
    /// curated fact.
    DerivedCuratedConclusion {
        conclusion_fact_id: FactId,
        derivation_id: DerivationId,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssertionFailureReason {
    #[error("claim '{0}' is bound to no fact")]
    UnboundClaim(String),
    #[error("claim '{0}' has no evidence certificate")]
    UncertifiedClaim(String),
    #[error("proposition '{0}' is not authorized")]
    UnauthorizedProposition(String),
    #[error("claim '{0}' requires a confirming {1} derivation, none is present")]
    MissingDerivation(String, String),
    #[error("qualifier '{0}' is not admitted by the assertion policy")]
    UnadmittedQualifier(String),
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AssertionError {
    #[error("assertion not authorized: {reason}")]
    NotAuthorized {
        claim_id: ClaimId,
        reason: AssertionFailureReason,
    },
    #[error("evidence stratum is missing; certify before authorizing")]
    MissingEvidenceStratum,
}

/// A plan whose every stated claim carries a granted authority.
///
/// The authority map is keyed by claim, and the policy digest is carried
/// forward so a later boundary can tell which policy granted it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AssertionAuthorizedPlan {
    certified: EvidenceCertifiedPlan,
    authorities: BTreeMap<ClaimId, ClaimAuthority>,
    policy_digest: String,
}

impl AssertionAuthorizedPlan {
    /// The only constructor: the recursive per-constructor policy over the
    /// stated claims.
    pub fn try_authorize(
        certified: EvidenceCertifiedPlan,
        policy: &AssertionPolicy,
        facts: &FactRegistry,
    ) -> Result<Self, AssertionError> {
        let discourse = certified.candidate().discourse();
        let mut authorities = BTreeMap::new();
        for claim in discourse.projected_claims() {
            let authority = authorize_claim(
                &claim.claim_id,
                &claim.proposition,
                &certified,
                policy,
                facts,
            )
            .map_err(|reason| AssertionError::NotAuthorized {
                claim_id: claim.claim_id.clone(),
                reason,
            })?;
            authorities.insert(claim.claim_id, authority);
        }
        Ok(Self {
            certified,
            authorities,
            policy_digest: policy.digest(),
        })
    }

    pub fn certified(&self) -> &EvidenceCertifiedPlan {
        &self.certified
    }

    pub fn authorities(&self) -> &BTreeMap<ClaimId, ClaimAuthority> {
        &self.authorities
    }

    pub fn policy_digest(&self) -> &str {
        &self.policy_digest
    }

    pub fn authority_for(&self, claim_id: &ClaimId) -> Option<&ClaimAuthority> {
        self.authorities.get(claim_id)
    }
}

/// The recursive policy walk over one stated claim: the binding and the
/// evidence certificate are required at the claim level, then the content
/// itself authorizes per constructor.
fn authorize_claim(
    claim_id: &ClaimId,
    proposition: &PropositionId,
    certified: &EvidenceCertifiedPlan,
    policy: &AssertionPolicy,
    facts: &FactRegistry,
) -> Result<ClaimAuthority, AssertionFailureReason> {
    if !certified.bindings().contains_key(claim_id) {
        return Err(AssertionFailureReason::UnboundClaim(
            claim_id.as_str().to_string(),
        ));
    }
    if certified.certificate_for(claim_id).is_none() {
        return Err(AssertionFailureReason::UncertifiedClaim(
            claim_id.as_str().to_string(),
        ));
    }
    let fact_id = certified
        .bindings()
        .get(claim_id)
        .expect("verified above")
        .clone();
    let node = certified
        .candidate()
        .propositions()
        .get(proposition)
        .ok_or_else(|| {
            AssertionFailureReason::UnauthorizedProposition(proposition.as_str().to_string())
        })?;

    match node {
        PropositionNode::Predicate {
            subject,
            relation,
            object,
        } => {
            // The stated content must match the bound curated fact: a claim
            // cannot state one triple while binding another fact's id.
            let record = facts.get(&fact_id).ok_or_else(|| {
                AssertionFailureReason::UnauthorizedProposition(proposition.as_str().to_string())
            })?;
            if record.status != FactStatus::Curated
                || record.subject.0 != subject.as_str()
                || record.relation != *relation
                || record.object.0 != object.as_str()
            {
                return Err(AssertionFailureReason::UnauthorizedProposition(
                    proposition.as_str().to_string(),
                ));
            }
            Ok(ClaimAuthority::Curated { fact_id })
        }
        PropositionNode::Question { proposition: inner } => {
            // A dialogue act wrapping a declarative claim must satisfy the
            // wrapped claim's authority; the wrap itself changes no authority.
            authorize_node(inner, certified, policy, facts)?;
            Ok(ClaimAuthority::Curated { fact_id })
        }
        PropositionNode::Conjunction { children } => {
            for child in children {
                authorize_node(child, certified, policy, facts)?;
            }
            let derivation_id = require_derivation(
                claim_id,
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ConjunctionIntroduction,
            )?;
            Ok(ClaimAuthority::DerivedCuratedConclusion {
                conclusion_fact_id: fact_id,
                derivation_id,
            })
        }
        PropositionNode::Conditional {
            antecedent,
            consequent,
        } => {
            authorize_node(antecedent, certified, policy, facts)?;
            authorize_node(consequent, certified, policy, facts)?;
            let derivation_id = require_derivation(
                claim_id,
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ConditionalElimination,
            )?;
            Ok(ClaimAuthority::DerivedCuratedConclusion {
                conclusion_fact_id: fact_id,
                derivation_id,
            })
        }
        PropositionNode::Contrast { left, right } => {
            authorize_node(left, certified, policy, facts)?;
            authorize_node(right, certified, policy, facts)?;
            let derivation_id = require_derivation(
                claim_id,
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ContrastIntroduction,
            )?;
            Ok(ClaimAuthority::DerivedCuratedConclusion {
                conclusion_fact_id: fact_id,
                derivation_id,
            })
        }
        PropositionNode::Consequence { consequent, .. } => {
            authorize_node(consequent, certified, policy, facts)?;
            let derivation_id = require_derivation(
                claim_id,
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ConsequenceIntroduction,
            )?;
            Ok(ClaimAuthority::DerivedCuratedConclusion {
                conclusion_fact_id: fact_id,
                derivation_id,
            })
        }
        PropositionNode::Qualification {
            qualifier,
            proposition: inner,
        } => {
            // authority(A) ∧ admitted_qualification(q) ∧ confidence(A)
            // satisfies q. V1 admits no qualifier, so any qualified claim
            // fails closed at the first clause; the numeric check is bound by
            // the policy map and waits for an admitted qualifier to exist.
            policy.qualifier(qualifier).ok_or_else(|| {
                AssertionFailureReason::UnadmittedQualifier(qualifier.as_str().to_string())
            })?;
            authorize_node(inner, certified, policy, facts)?;
            Ok(ClaimAuthority::Curated { fact_id })
        }
    }
}

/// Content-level authority for a proposition, whether or not it is itself a
/// stated claim.
fn authorize_node(
    proposition: &PropositionId,
    certified: &EvidenceCertifiedPlan,
    policy: &AssertionPolicy,
    facts: &FactRegistry,
) -> Result<(), AssertionFailureReason> {
    let propositions = certified.candidate().propositions();
    let node = propositions.get(proposition).ok_or_else(|| {
        AssertionFailureReason::UnauthorizedProposition(proposition.as_str().to_string())
    })?;
    match node {
        PropositionNode::Predicate {
            subject,
            relation,
            object,
        } => {
            // A leaf is authorized by content: its triple must be a curated
            // fact. Content addressing makes the match exact.
            let is_curated = facts.records().any(|record| {
                record.status == FactStatus::Curated
                    && record.subject.0 == subject.as_str()
                    && record.relation == *relation
                    && record.object.0 == object.as_str()
            });
            if is_curated {
                Ok(())
            } else {
                Err(AssertionFailureReason::UnauthorizedProposition(
                    proposition.as_str().to_string(),
                ))
            }
        }
        PropositionNode::Question { proposition: inner } => {
            authorize_node(inner, certified, policy, facts)
        }
        PropositionNode::Conjunction { children } => {
            for child in children {
                authorize_node(child, certified, policy, facts)?;
            }
            require_derivation_for_node(
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ConjunctionIntroduction,
            )
        }
        PropositionNode::Conditional {
            antecedent,
            consequent,
        } => {
            authorize_node(antecedent, certified, policy, facts)?;
            authorize_node(consequent, certified, policy, facts)?;
            require_derivation_for_node(
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ConditionalElimination,
            )
        }
        PropositionNode::Contrast { left, right } => {
            authorize_node(left, certified, policy, facts)?;
            authorize_node(right, certified, policy, facts)?;
            require_derivation_for_node(
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ContrastIntroduction,
            )
        }
        PropositionNode::Consequence { consequent, .. } => {
            authorize_node(consequent, certified, policy, facts)?;
            require_derivation_for_node(
                proposition,
                certified.candidate().derivations(),
                InferenceRuleId::ConsequenceIntroduction,
            )
        }
        PropositionNode::Qualification {
            qualifier,
            proposition: inner,
        } => {
            policy.qualifier(qualifier).ok_or_else(|| {
                AssertionFailureReason::UnadmittedQualifier(qualifier.as_str().to_string())
            })?;
            authorize_node(inner, certified, policy, facts)
        }
    }
}

fn require_derivation_for_node(
    proposition: &PropositionId,
    derivations: &DerivationDag,
    rule: InferenceRuleId,
) -> Result<(), AssertionFailureReason> {
    let found = derivations
        .iter()
        .any(|(_, node)| node.conclusion() == proposition && node.rule() == rule);
    if found {
        Ok(())
    } else {
        Err(AssertionFailureReason::UnauthorizedProposition(
            proposition.as_str().to_string(),
        ))
    }
}

/// The claim's content is only assertable if a whitelisted rule confirms it
/// in this plan. Connectivity is not implication; the derivation must be
/// present (ADR-0034 §3).
fn require_derivation(
    claim_id: &ClaimId,
    conclusion: &PropositionId,
    derivations: &DerivationDag,
    rule: InferenceRuleId,
) -> Result<DerivationId, AssertionFailureReason> {
    derivations
        .iter()
        .find(|(_, node)| node.conclusion() == conclusion && node.rule() == rule)
        .map(|(id, _)| id.clone())
        .ok_or_else(|| {
            AssertionFailureReason::MissingDerivation(
                claim_id.as_str().to_string(),
                rule.as_str().to_string(),
            )
        })
}

fn absorb(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_pack_set;
    use crate::argued_topics::argued_topic_registry;
    use crate::response_plan::{Confidence, NonEmptyVec, SemanticId};
    use crate::response_plan_v2::admission::LeafAdmittedPlan;
    use crate::response_plan_v2::candidate::CandidateResponsePlan;
    use crate::response_plan_v2::derivation::{DerivationDagBuilder, DerivationNode, EvidenceRef};
    use crate::response_plan_v2::discourse::{DiscoursePlan, DiscourseTree};
    use crate::response_plan_v2::evidence::EvidenceEvaluationContext;
    use crate::response_plan_v2::proposition::{PropositionDagBuilder, PropositionNode};

    fn v1_context() -> EvidenceEvaluationContext {
        EvidenceEvaluationContext::new(42, None)
    }

    fn certify(
        candidate: CandidateResponsePlan,
        bindings: BTreeMap<ClaimId, FactId>,
    ) -> EvidenceCertifiedPlan {
        let pack = active_pack_set();
        let argued = argued_topic_registry().unwrap();
        let admitted =
            LeafAdmittedPlan::try_admit(candidate, bindings, pack, argued).expect("admission");
        EvidenceCertifiedPlan::try_certify(
            admitted,
            &v1_context(),
            pack.facts(),
            pack.fingerprint(),
        )
        .expect("certification")
    }

    /// A single-predicate thesis built from the real audited topic's fact,
    /// bound to that fact: the planner binds a claim to a fact, and the claim
    /// states the fact's own triple.
    fn curated_thesis() -> EvidenceCertifiedPlan {
        let argued = argued_topic_registry().unwrap();
        let pack = active_pack_set();
        let topic = argued.get("свобода").expect("topic");
        let record = pack
            .facts()
            .get(topic.thesis().fact_id())
            .expect("audited thesis fact");
        let mut builder = PropositionDagBuilder::new();
        let id = builder.insert(PropositionNode::Predicate {
            subject: SemanticId::try_new(record.subject.0.clone()).expect("subject id"),
            relation: SemanticId::try_new(record.relation.as_str()).expect("relation id"),
            object: SemanticId::try_new(record.object.0.clone()).expect("object id"),
        });
        let propositions = builder.build().expect("dag");
        let candidate = CandidateResponsePlan::try_new(
            propositions,
            DerivationDag::empty(),
            DiscoursePlan::try_new(DiscourseTree::Thesis(id)).expect("plan"),
        )
        .expect("candidate");
        let claim = candidate.projected_claims().remove(0).claim_id;
        let mut bindings = BTreeMap::new();
        bindings.insert(claim, topic.thesis().fact_id().clone());
        certify(candidate, bindings)
    }

    /// The two audited statements of one topic joined into a conjunction, with
    /// a confirming `ConjunctionIntroduction` step over them.
    fn conjunction_plan(
        with_derivation: bool,
    ) -> (EvidenceCertifiedPlan, PropositionId, Vec<PropositionId>) {
        let argued = argued_topic_registry().unwrap();
        let pack = active_pack_set();
        let topic = argued.get("свобода").expect("topic");
        let mut builder = PropositionDagBuilder::new();
        let mut leaves = Vec::new();
        for statement in topic.statements().take(2) {
            let record = pack
                .facts()
                .get(statement.fact_id())
                .expect("audited statement fact");
            let node = PropositionNode::Predicate {
                subject: SemanticId::try_new(record.subject.0.clone()).expect("subject id"),
                relation: SemanticId::try_new(record.relation.as_str()).expect("relation id"),
                object: SemanticId::try_new(record.object.0.clone()).expect("object id"),
            };
            leaves.push(builder.insert(node));
        }
        let conjunction = builder.insert(PropositionNode::Conjunction {
            children: leaves.clone(),
        });
        let propositions = builder.build().expect("dag");

        let mut derivations = DerivationDagBuilder::new();
        if with_derivation {
            let mut premises = NonEmptyVec::one(leaves[0].clone());
            premises.push(leaves[1].clone());
            derivations.insert(DerivationNode::new(
                premises,
                conjunction.clone(),
                InferenceRuleId::ConjunctionIntroduction,
                EvidenceRef::try_new("fact:freedom_choice.counterpoint").expect("evidence"),
                Confidence::from_basis_points(7_000).expect("confidence"),
            ));
        }
        let derivations = derivations.build(&propositions).expect("derivations");

        let candidate = CandidateResponsePlan::try_new(
            propositions,
            derivations,
            DiscoursePlan::try_new(DiscourseTree::Thesis(conjunction.clone())).expect("plan"),
        )
        .expect("candidate");
        let claim = candidate.projected_claims().remove(0).claim_id;
        let mut bindings = BTreeMap::new();
        bindings.insert(
            claim,
            FactId::try_new("fact.freedom_choice.counterpoint").expect("fact id"),
        );
        (certify(candidate, bindings), conjunction, leaves)
    }

    fn authorized(certified: EvidenceCertifiedPlan) -> AssertionAuthorizedPlan {
        let pack = active_pack_set();
        AssertionAuthorizedPlan::try_authorize(certified, &AssertionPolicy::v1(), pack.facts())
            .expect("authorization")
    }

    #[test]
    fn a_curated_predicate_claim_is_authorized_curated() {
        let certified = curated_thesis();
        let claim = certified.candidate().projected_claims().remove(0).claim_id;
        let plan = authorized(certified);
        assert_eq!(
            plan.authority_for(&claim),
            Some(&ClaimAuthority::Curated {
                fact_id: FactId::try_new("fact.freedom_choice").expect("fact id"),
            })
        );
        assert_eq!(plan.policy_digest(), AssertionPolicy::v1().digest());
    }

    #[test]
    fn a_confirmed_conjunction_is_authorized_as_derived_curated() {
        let (certified, conjunction, _) = conjunction_plan(true);
        let claim = certified.candidate().projected_claims().remove(0).claim_id;
        let plan = authorized(certified);
        match plan.authority_for(&claim) {
            Some(ClaimAuthority::DerivedCuratedConclusion {
                conclusion_fact_id,
                derivation_id,
            }) => {
                assert_eq!(
                    conclusion_fact_id.as_str(),
                    "fact.freedom_choice.counterpoint"
                );
                assert_eq!(derivation_id.as_str().len(), 64);
                assert_eq!(conjunction.as_str().len(), 64);
            }
            other => panic!("unexpected authority: {other:?}"),
        }
    }

    /// Connectivity is not implication: without the confirming derivation the
    /// same conjunction fails closed at the claim level.
    #[test]
    fn a_conjunction_without_derivation_fails_closed() {
        let (certified, _, _) = conjunction_plan(false);
        let claim = certified.candidate().projected_claims().remove(0).claim_id;
        let pack = active_pack_set();
        let result =
            AssertionAuthorizedPlan::try_authorize(certified, &AssertionPolicy::v1(), pack.facts());
        assert!(matches!(
            result,
            Err(AssertionError::NotAuthorized {
                claim_id,
                reason: AssertionFailureReason::MissingDerivation(_, rule),
            }) if claim_id == claim && rule == "conjunction_introduction"
        ));
    }

    /// An unadmitted qualifier fails closed even though the underlying content
    /// is curated: V1 authorizes plain predicates only.
    #[test]
    fn an_unadmitted_qualifier_fails_closed() {
        let argued = argued_topic_registry().unwrap();
        let pack = active_pack_set();
        let topic = argued.get("свобода").expect("topic");
        let record = pack
            .facts()
            .get(topic.thesis().fact_id())
            .expect("audited thesis fact");
        let mut builder = PropositionDagBuilder::new();
        let inner = builder.insert(PropositionNode::Predicate {
            subject: SemanticId::try_new(record.subject.0.clone()).expect("subject id"),
            relation: SemanticId::try_new(record.relation.as_str()).expect("relation id"),
            object: SemanticId::try_new(record.object.0.clone()).expect("object id"),
        });
        let qualifier = QualifierId::try_new("tentatively").expect("qualifier");
        let qualified = builder.insert(PropositionNode::Qualification {
            qualifier: qualifier.clone(),
            proposition: inner,
        });
        let propositions = builder.build().expect("dag");
        let candidate = CandidateResponsePlan::try_new(
            propositions,
            DerivationDag::empty(),
            DiscoursePlan::try_new(DiscourseTree::Thesis(qualified)).expect("plan"),
        )
        .expect("candidate");
        let claim = candidate.projected_claims().remove(0).claim_id;
        let mut bindings = BTreeMap::new();
        bindings.insert(claim.clone(), topic.thesis().fact_id().clone());
        let certified = certify(candidate, bindings);

        let result =
            AssertionAuthorizedPlan::try_authorize(certified, &AssertionPolicy::v1(), pack.facts());
        assert!(matches!(
            result,
            Err(AssertionError::NotAuthorized {
                claim_id,
                reason: AssertionFailureReason::UnadmittedQualifier(unadmitted),
            }) if claim_id == claim && unadmitted == "tentatively"
        ));
    }

    /// Content is the authority for a leaf: a binding to a curated fact does
    /// not authorize content whose triple is not itself curated.
    #[test]
    fn a_binding_to_curated_does_not_authorize_foreign_content() {
        let argued = argued_topic_registry().unwrap();
        let pack = active_pack_set();
        let mut builder = PropositionDagBuilder::new();
        let stranger = builder.insert(PropositionNode::Predicate {
            subject: SemanticId::try_new("concept.несуществующий").expect("subject id"),
            relation: SemanticId::try_new("RelPresupposes").expect("relation id"),
            object: SemanticId::try_new("concept.нет_такого").expect("object id"),
        });
        let propositions = builder.build().expect("dag");
        let candidate = CandidateResponsePlan::try_new(
            propositions,
            DerivationDag::empty(),
            DiscoursePlan::try_new(DiscourseTree::Thesis(stranger)).expect("plan"),
        )
        .expect("candidate");
        let claim = candidate.projected_claims().remove(0).claim_id;
        let mut bindings = BTreeMap::new();
        bindings.insert(
            claim,
            FactId::try_new("fact.freedom_choice").expect("fact id"),
        );
        let certified = certify(candidate, bindings);

        let result =
            AssertionAuthorizedPlan::try_authorize(certified, &AssertionPolicy::v1(), pack.facts());
        assert!(matches!(
            result,
            Err(AssertionError::NotAuthorized {
                reason: AssertionFailureReason::UnauthorizedProposition(_),
                ..
            })
        ));
        let _ = argued;
    }

    #[test]
    fn v1_policy_admits_no_qualifiers_and_hashes_deterministically() {
        let v1 = AssertionPolicy::v1();
        assert!(v1.is_empty());
        assert!(v1
            .qualifier(&QualifierId::try_new("tentatively").expect("qualifier"))
            .is_none());
        assert_eq!(v1.digest(), AssertionPolicy::v1().digest());

        let qualified = v1.clone().with_qualifier(QualifierAdmission {
            qualifier: QualifierId::try_new("tentatively").expect("qualifier"),
            required_confidence_bps: 8_000,
        });
        assert_ne!(v1.digest(), qualified.digest());
        assert_eq!(qualified.len(), 1);
        assert_eq!(
            qualified
                .qualifier(&QualifierId::try_new("tentatively").expect("qualifier"))
                .unwrap()
                .required_confidence_bps,
            8_000
        );
    }
}

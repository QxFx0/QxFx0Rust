//! Evidence authority: selectability under a snapshot and an as-of
//! (ADR-0034 §4, §9).
//!
//! Admission proves *membership*: the fact belongs to the profile. Evidence
//! proves *selectability*: the claim is active under the current
//! `AuthoritySnapshot` and `as_of`. The two boundaries are deliberately not
//! merged — a same-`FactId` update of pack conditions changes selectability
//! without changing membership, and a merged certificate could not express
//! that.
//!
//! The certificate references its admission proof by digest rather than
//! copying it (ADR-0034 §4). It carries the authority snapshot digest (the
//! pack set fingerprint) and the temporal context: in V1 `authority_as_of` is
//! `None`, so calendar-relative policies fail closed as unsupported and
//! evidence is evaluated at the turn-relative `logical_turn` (ADR-0034 §9).

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::fact_model::{FactId, FactRegistry, FactRegistryError, FactStatus};
use crate::response_plan_v2::admission::{LeafAdmissionProof, LeafAdmittedPlan, ADMISSION_DOMAIN};
use crate::response_plan_v2::discourse::ClaimId;
use crate::response_plan_v2::proposition::PropositionId;

/// Domain separation tag for the evidence certificate.
pub const EVIDENCE_DOMAIN: &str = "qxfx0:evidence-authority:v1";

/// Temporal context of one turn's evidence evaluation (ADR-0034 §9).
///
/// V1: `authority_as_of` is `None`. Calendar-relative policies are never
/// guessed; a fact with a validity window fails closed as unsupported rather
/// than being certified without a trusted instant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceEvaluationContext {
    /// `(session_id, turn_seq)`-style governance epoch, as a deterministic
    /// sequence number.
    pub logical_turn: u64,
    /// Trusted persisted calendar instant, when the policy is
    /// calendar-relative. Wall clock is read only at the creation of the
    /// input envelope and recorded.
    pub authority_as_of: Option<String>,
}

impl EvidenceEvaluationContext {
    pub const fn new(logical_turn: u64, authority_as_of: Option<String>) -> Self {
        Self {
            logical_turn,
            authority_as_of,
        }
    }
}

/// A claim proven active under one authority snapshot at one temporal point.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceAuthorityCertificate {
    claim_id: ClaimId,
    fact_id: FactId,
    admission_proof_digest: String,
    authority_snapshot_digest: String,
    logical_turn: u64,
    authority_as_of: Option<String>,
    digest: String,
}

impl EvidenceAuthorityCertificate {
    pub fn claim_id(&self) -> &ClaimId {
        &self.claim_id
    }

    pub fn fact_id(&self) -> &FactId {
        &self.fact_id
    }

    /// The admission proof this certificate is built on, by digest. The
    /// content is not copied: the admission boundary stays the only owner.
    pub fn admission_proof_digest(&self) -> &str {
        &self.admission_proof_digest
    }

    pub fn authority_snapshot_digest(&self) -> &str {
        &self.authority_snapshot_digest
    }

    pub fn logical_turn(&self) -> u64 {
        self.logical_turn
    }

    pub fn authority_as_of(&self) -> Option<&str> {
        self.authority_as_of.as_deref()
    }

    /// The certificate itself.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceError {
    #[error("claim '{claim_id}' has no admission proof; admit before certifying")]
    MissingAdmissionProof { claim_id: String },
    #[error("fact '{0}' is unknown to the active pack set")]
    UnknownFact(FactId),
    #[error("fact '{0}' is not curated and cannot be certified")]
    NotCurated(FactId),
    #[error(
        "fact '{0}' has a temporal validity window but no authority_as_of is available; \
         calendar-relative policies fail closed (ADR-0034 §9)"
    )]
    TemporalPolicyUnsupported(FactId),
    #[error("fact '{fact_id}' is outside its validity window as of '{as_of}'")]
    OutsideValidityWindow { fact_id: FactId, as_of: String },
}

/// Certify that one claim's bound fact is selectable under the context.
///
/// The claim's admission proof must be supplied — a certificate never borrows
/// the assertions of a boundary it did not pass (ADR-0034 intro).
pub fn certify_claim(
    claim_id: ClaimId,
    fact_id: FactId,
    admission: &LeafAdmissionProof,
    context: &EvidenceEvaluationContext,
    facts: &FactRegistry,
    authority_snapshot_digest: &str,
) -> Result<EvidenceAuthorityCertificate, EvidenceError> {
    if admission.fact_id() != &fact_id {
        return Err(EvidenceError::MissingAdmissionProof {
            claim_id: claim_id.as_str().to_string(),
        });
    }

    let record = facts
        .get(&fact_id)
        .ok_or_else(|| EvidenceError::UnknownFact(fact_id.clone()))?;
    if record.status != FactStatus::Curated {
        return Err(EvidenceError::NotCurated(fact_id));
    }

    // Selectability. A temporal record needs a trusted instant; V1 carries
    // none, so the policy fails closed instead of certifying blind.
    let failure = |error: FactRegistryError| match error {
        FactRegistryError::OutsideValidityWindow(fact) => EvidenceError::OutsideValidityWindow {
            fact_id: fact,
            as_of: context.authority_as_of.clone().unwrap_or_default(),
        },
        FactRegistryError::NotCurated(fact) => EvidenceError::NotCurated(fact),
        FactRegistryError::UnknownFact(fact) => EvidenceError::UnknownFact(fact),
        FactRegistryError::TemporalValidityRequired(fact) => {
            EvidenceError::TemporalPolicyUnsupported(fact)
        }
        other => EvidenceError::TemporalPolicyUnsupported(match other {
            FactRegistryError::OutsideValidityWindow(fact)
            | FactRegistryError::NotCurated(fact)
            | FactRegistryError::UnknownFact(fact)
            | FactRegistryError::TemporalValidityRequired(fact) => fact,
            _ => fact_id.clone(),
        }),
    };
    match &context.authority_as_of {
        Some(as_of) => {
            facts.select_at(&fact_id, as_of).map_err(failure)?;
        }
        None => {
            if record.valid_from.is_some() || record.valid_to.is_some() {
                return Err(EvidenceError::TemporalPolicyUnsupported(fact_id));
            }
            facts.select(&fact_id).map_err(failure)?;
        }
    }

    let mut hasher = Sha256::new();
    hasher.update(EVIDENCE_DOMAIN.as_bytes());
    absorb(&mut hasher, claim_id.as_str().as_bytes());
    absorb(&mut hasher, fact_id.as_str().as_bytes());
    absorb(&mut hasher, admission.digest().as_bytes());
    absorb(&mut hasher, authority_snapshot_digest.as_bytes());
    absorb(&mut hasher, context.logical_turn.to_string().as_bytes());
    if let Some(as_of) = &context.authority_as_of {
        absorb(&mut hasher, as_of.as_bytes());
    }

    Ok(EvidenceAuthorityCertificate {
        claim_id,
        fact_id,
        admission_proof_digest: admission.digest().to_string(),
        authority_snapshot_digest: authority_snapshot_digest.to_string(),
        logical_turn: context.logical_turn,
        authority_as_of: context.authority_as_of.clone(),
        digest: format!("{:x}", hasher.finalize()),
    })
}

fn absorb(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// A plan whose every stated claim is certified selectable under one
/// snapshot and one temporal context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EvidenceCertifiedPlan {
    admitted: LeafAdmittedPlan,
    certificates: BTreeMap<ClaimId, EvidenceAuthorityCertificate>,
    authority_snapshot_digest: String,
}

impl EvidenceCertifiedPlan {
    /// The only constructor. Every stated claim of the admitted plan must be
    /// certified; a claim the evidence boundary cannot certify rejects the
    /// whole plan.
    pub fn try_certify(
        admitted: LeafAdmittedPlan,
        context: &EvidenceEvaluationContext,
        facts: &FactRegistry,
        authority_snapshot_digest: &str,
    ) -> Result<Self, EvidenceError> {
        let mut certificates = BTreeMap::new();
        for (claim_id, fact_id) in admitted.bindings() {
            let admission = admitted.proof_for(claim_id).ok_or_else(|| {
                EvidenceError::MissingAdmissionProof {
                    claim_id: claim_id.as_str().to_string(),
                }
            })?;
            let certificate = certify_claim(
                claim_id.clone(),
                fact_id.clone(),
                admission,
                context,
                facts,
                authority_snapshot_digest,
            )?;
            certificates.insert(claim_id.clone(), certificate);
        }
        Ok(Self {
            admitted,
            certificates,
            authority_snapshot_digest: authority_snapshot_digest.to_string(),
        })
    }

    pub fn admitted(&self) -> &LeafAdmittedPlan {
        &self.admitted
    }

    pub fn candidate(&self) -> &crate::response_plan_v2::candidate::CandidateResponsePlan {
        self.admitted.candidate()
    }

    pub fn bindings(&self) -> &BTreeMap<ClaimId, FactId> {
        self.admitted.bindings()
    }

    pub fn certificate_for(&self, claim_id: &ClaimId) -> Option<&EvidenceAuthorityCertificate> {
        self.certificates.get(claim_id)
    }

    pub fn certificates(&self) -> &BTreeMap<ClaimId, EvidenceAuthorityCertificate> {
        &self.certificates
    }

    pub fn authority_snapshot_digest(&self) -> &str {
        &self.authority_snapshot_digest
    }

    /// The stated claim (if any) that binds this proposition. Content
    /// addressing makes the match exact: the same meaning is the same id.
    pub fn claim_for_proposition(&self, proposition: &PropositionId) -> Option<&ClaimId> {
        self.candidate()
            .projected_claims()
            .into_iter()
            .find(|claim| &claim.proposition == proposition)
            .and_then(|claim| {
                self.bindings()
                    .get_key_value(&claim.claim_id)
                    .map(|(claim_id, _)| claim_id)
            })
    }
}

/// Domain tag reused so tests can prove a certificate really references the
/// admission digest rather than embedding it.
pub const ADMISSION_DOMAIN_TAG: &str = ADMISSION_DOMAIN;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_pack_set;
    use crate::argued_topics::{argued_topic_registry, ArguedTopic};
    use crate::response_plan::SemanticId;
    use crate::response_plan::SemanticProposition;
    use crate::response_plan_v2::admission::prove_leaf_admission;
    use crate::response_plan_v2::discourse::{DiscoursePlan, DiscourseTree};
    use crate::response_plan_v2::proposition::{PropositionDagBuilder, PropositionNode};

    fn v1_context() -> EvidenceEvaluationContext {
        EvidenceEvaluationContext::new(42, None)
    }

    fn admission_for(fact_id: &FactId) -> LeafAdmissionProof {
        let pack = active_pack_set();
        let argued = argued_topic_registry().unwrap();
        prove_leaf_admission(fact_id, pack, argued).expect("admission")
    }

    /// Derive one claim per audited statement of a topic, in discourse order,
    /// with the identities the runtime would produce. Propositions are built
    /// from the fact records' own triples; the evidence boundary consumes the
    /// resulting claim addresses as opaque identities.
    fn claims_for(topic: &ArguedTopic) -> Vec<(ClaimId, FactId)> {
        let facts = active_pack_set().facts();
        let mut builder = PropositionDagBuilder::new();
        let mut leaves = Vec::new();
        for statement in topic.statements() {
            let record = facts
                .get(statement.fact_id())
                .expect("audited statement fact must exist");
            let node = PropositionNode::Predicate {
                subject: SemanticId::try_new(record.subject.0.clone()).expect("subject id"),
                relation: SemanticId::try_new(record.relation.as_str()).expect("relation id"),
                object: SemanticId::try_new(record.object.0.clone()).expect("object id"),
            };
            leaves.push(builder.insert(node));
        }
        let dag = builder.build().expect("dag");
        assert!(!dag.is_empty());
        let sequence = match leaves.len() {
            2 => vec![
                DiscourseTree::Thesis(leaves[0].clone()),
                DiscourseTree::Counterpoint(leaves[1].clone()),
            ],
            _ => vec![
                DiscourseTree::Thesis(leaves[0].clone()),
                DiscourseTree::Counterpoint(leaves[1].clone()),
                DiscourseTree::Consequence(leaves[2].clone()),
            ],
        };
        let plan = DiscoursePlan::try_new(DiscourseTree::Sequence(sequence)).expect("plan");
        let claims = plan.projected_claims();
        assert_eq!(claims.len(), topic.statement_count());
        claims
            .into_iter()
            .zip(topic.statements())
            .map(|(claim, statement)| (claim.claim_id, statement.fact_id().clone()))
            .collect()
    }

    fn first_topic_claims() -> Vec<(ClaimId, FactId)> {
        let argued = argued_topic_registry().unwrap();
        let topic = argued.get("свобода").expect("topic");
        // Claims are addresses derived from a discourse tree, never parsed or
        // stored; the evidence boundary consumes them as opaque identities.
        let mut builder = PropositionDagBuilder::new();
        let (subject, relation, object) = match topic.primary_proposition() {
            SemanticProposition::CanonicalPredicate {
                subject,
                relation,
                object,
            } => (subject.clone(), relation.clone(), object.clone()),
            other => panic!("unexpected proposition: {other:?}"),
        };
        let id = builder.insert(PropositionNode::Predicate {
            subject,
            relation,
            object,
        });
        let _dag = builder.build().expect("dag");
        let plan = DiscoursePlan::try_new(DiscourseTree::Thesis(id)).expect("plan");
        let claimed = plan.projected_claims();
        assert_eq!(claimed.len(), 1);
        vec![(
            claimed[0].claim_id.clone(),
            topic.thesis().fact_id().clone(),
        )]
    }

    #[test]
    fn a_curated_fact_is_certified_under_v1_context() {
        let pack = active_pack_set();
        let (claim_id, fact_id) = first_topic_claims().remove(0);
        let admission = admission_for(&fact_id);
        let certificate = certify_claim(
            claim_id.clone(),
            fact_id,
            &admission,
            &v1_context(),
            pack.facts(),
            pack.fingerprint(),
        )
        .expect("certificate");
        assert_eq!(certificate.claim_id(), &claim_id);
        assert_eq!(certificate.admission_proof_digest(), admission.digest());
        assert_eq!(certificate.authority_snapshot_digest(), pack.fingerprint());
        assert_eq!(certificate.logical_turn(), 42);
        assert_eq!(certificate.authority_as_of(), None);
        assert_eq!(certificate.digest().len(), 64);
    }

    #[test]
    fn certification_is_deterministic() {
        let pack = active_pack_set();
        let (claim_id, fact_id) = first_topic_claims().remove(0);
        let admission = admission_for(&fact_id);
        let build = || {
            certify_claim(
                claim_id.clone(),
                fact_id.clone(),
                &admission,
                &v1_context(),
                pack.facts(),
                pack.fingerprint(),
            )
            .expect("certificate")
        };
        assert_eq!(build(), build());
    }

    #[test]
    fn an_admission_under_another_fact_is_rejected() {
        let pack = active_pack_set();
        let (claim_id, fact_id) = first_topic_claims().remove(0);
        let admission = admission_for(&fact_id);
        let stranger = FactId::try_new("fact.freedom_choice.counterpoint").expect("fact id");
        let result = certify_claim(
            claim_id,
            stranger,
            &admission,
            &v1_context(),
            pack.facts(),
            pack.fingerprint(),
        );
        assert!(matches!(
            result,
            Err(EvidenceError::MissingAdmissionProof { .. })
        ));
    }

    #[test]
    fn a_temporal_fact_fails_closed_without_as_of() {
        // Build a temporal record through the registry so the evidence
        // boundary sees the window.
        use crate::fact_model::{FactKind, TypedRelationModel};
        use crate::get_resolver;
        use qxfx0_types::ConceptId;

        let temporal = crate::fact_model::FactRecord {
            id: FactId::try_new("fact.temporal.probe").unwrap(),
            subject: ConceptId("concept.свобода".into()),
            relation: SemanticId::try_new("RelPresupposes").unwrap(),
            object: ConceptId("concept.vozmozhnost_vybora".into()),
            kind: FactKind::InterpretiveClaim,
            conditions: Vec::new(),
            confidence_basis_points: 9_000,
            source_pack: "test-pack".into(),
            source_ref: "test:fact".into(),
            valid_from: Some("2026-01-01".into()),
            valid_to: Some("2026-02-01".into()),
            status: crate::fact_model::FactStatus::Curated,
        };
        let registry = FactRegistry::load(
            [temporal.clone()],
            [],
            get_resolver(),
            &TypedRelationModel::default(),
        )
        .expect("registry");

        // Admission cannot run against the profile for a foreign fact, so the
        // temporal policy check is exercised via the shared selection path:
        // without an as-of the registry itself refuses to guess.
        assert!(matches!(
            registry.select(&temporal.id),
            Err(FactRegistryError::TemporalValidityRequired(_))
        ));

        // With a trusted instant the registry itself certifies within its
        // window…
        assert!(registry.select_at(&temporal.id, "2026-01-15").is_ok());
        // …and fails outside it. The evidence boundary surfaces the registry
        // verdict without re-deciding it.
        let outside = registry.select_at(&temporal.id, "2026-02-01");
        assert!(matches!(
            outside,
            Err(FactRegistryError::OutsideValidityWindow(_))
        ));
    }

    #[test]
    fn every_audited_statement_certifies_under_v1() {
        let pack = active_pack_set();
        let argued = argued_topic_registry().unwrap();
        let mut certified = 0usize;
        for topic in argued.topics() {
            let claims = claims_for(topic);
            assert_eq!(claims.len(), topic.statement_count());
            for (claim_id, fact_id) in claims {
                let admission = admission_for(&fact_id);
                certify_claim(
                    claim_id,
                    fact_id,
                    &admission,
                    &v1_context(),
                    pack.facts(),
                    pack.fingerprint(),
                )
                .expect("certificate");
                certified += 1;
            }
        }
        assert_eq!(certified, 69);
    }

    #[test]
    fn admission_domain_tag_is_reused_for_reference() {
        assert_eq!(ADMISSION_DOMAIN_TAG, "qxfx0:leaf-admission:v1");
    }
}

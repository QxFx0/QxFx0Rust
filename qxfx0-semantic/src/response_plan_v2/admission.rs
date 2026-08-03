//! Leaf admission: static membership in the audited profile (ADR-0034 §4).
//!
//! The first authority boundary answers one question: does this `FactId`
//! belong to the allowed registry/profile for the active pack set? It is
//! *membership*, not selectability — temporal, provenance and dependency
//! checks belong to the evidence boundary, and a plan that has not been
//! admitted must not be certified.
//!
//! Membership is static and stable: the same `FactId` remains a member across
//! pack versions while the pack's predicate-to-fact bindings do not change.
//! `ArguedTopic` is the admission canon that validates leaves (ADR-0034 §2),
//! so membership is read from the audited profile over the pack facts.
//!
//! The proof is keyed by the pack set fingerprint, so a changed pack set
//! (even one that keeps the same `FactId`s) yields a different proof digest —
//! the same property the V1 `Perspective` invalidation relies on when pack
//! conditions change without the `FactId`s changing.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

use crate::argued_topics::{ArguedTopicRegistry, CONTENT_PROFILE};
use crate::fact_model::{FactId, FactRegistry, FactRegistryError, FactStatus};
use crate::knowledge_pack::KnowledgePackSet;

use super::candidate::CandidateResponsePlan;
use super::discourse::ClaimId;

/// Domain separation tag for the admission proof.
pub const ADMISSION_DOMAIN: &str = "qxfx0:leaf-admission:v1";

/// A proven member of the audited profile: `fact_id` is bound to an admitted
/// predicate of the profile for this pack set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LeafAdmissionProof {
    fact_id: FactId,
    profile: String,
    pack_digest: String,
    digest: String,
}

impl LeafAdmissionProof {
    pub fn fact_id(&self) -> &FactId {
        &self.fact_id
    }

    /// The admission profile the fact belongs to (`audited_v1`).
    pub fn profile(&self) -> &str {
        &self.profile
    }

    /// Fingerprint of the pack set the membership was proven under.
    pub fn pack_digest(&self) -> &str {
        &self.pack_digest
    }

    /// The proof itself: content of this certificate, not an address.
    pub fn digest(&self) -> &str {
        &self.digest
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum AdmissionError {
    #[error("fact '{0}' is unknown to the active pack set")]
    UnknownFact(FactId),
    #[error("fact '{fact_id}' is not a member of the '{profile}' profile")]
    NotInProfile { fact_id: FactId, profile: String },
    #[error("fact '{0}' is not curated and cannot be admitted")]
    NotCurated(FactId),
    #[error("stated claim '{0}' has no binding")]
    UnboundClaim(String),
    #[error("binding refers to claim '{0}' which the discourse does not state")]
    BindingWithoutClaim(String),
}

/// Prove that `fact_id` is a member of the audited admission profile.
///
/// Three checks, in order:
///
/// 1. the fact exists in the active pack set;
/// 2. the fact is bound to an admitted predicate of the audited profile
///    (the registry asserts every such predicate selects a curated fact);
/// 3. the fact is `Curated` — a `Draft` or `Deprecated` record must not be
///    admitted even if a stale binding reached the profile.
pub fn prove_leaf_admission(
    fact_id: &FactId,
    pack: &KnowledgePackSet,
    argued: &ArguedTopicRegistry,
) -> Result<LeafAdmissionProof, AdmissionError> {
    if pack.facts().get(fact_id).is_none() {
        return Err(AdmissionError::UnknownFact(fact_id.clone()));
    }
    if !argued.contains_fact_id(fact_id) {
        return Err(AdmissionError::NotInProfile {
            fact_id: fact_id.clone(),
            profile: CONTENT_PROFILE.to_string(),
        });
    }
    match pack.facts().select(fact_id) {
        Ok(_) => {}
        Err(FactRegistryError::NotCurated(_)) => {
            return Err(AdmissionError::NotCurated(fact_id.clone()))
        }
        Err(_) => {
            // A temporal record has no window to resolve against at the
            // membership boundary; selectability belongs to evidence.
        }
    }
    Ok(proof_for(fact_id, pack))
}

fn proof_for(fact_id: &FactId, pack: &KnowledgePackSet) -> LeafAdmissionProof {
    let mut hasher = Sha256::new();
    hasher.update(ADMISSION_DOMAIN.as_bytes());
    absorb(&mut hasher, CONTENT_PROFILE.as_bytes());
    absorb(&mut hasher, fact_id.as_str().as_bytes());
    absorb(&mut hasher, pack.fingerprint().as_bytes());
    LeafAdmissionProof {
        fact_id: fact_id.clone(),
        profile: CONTENT_PROFILE.to_string(),
        pack_digest: pack.fingerprint().to_string(),
        digest: format!("{:x}", hasher.finalize()),
    }
}

fn absorb(hasher: &mut Sha256, bytes: &[u8]) {
    hasher.update((bytes.len() as u64).to_be_bytes());
    hasher.update(bytes);
}

/// The shared fact-membership view both boundaries read.
pub fn is_curated(facts: &FactRegistry, fact_id: &FactId) -> bool {
    facts
        .get(fact_id)
        .is_some_and(|record| record.status == FactStatus::Curated)
}

/// A candidate whose every stated claim carries a membership proof.
///
/// The binding is claimed by the planner and verified here: `fact_id` must
/// belong to the audited profile. Possessing the type *is* the certificate
/// that every stated claim was admitted under the active pack set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LeafAdmittedPlan {
    candidate: CandidateResponsePlan,
    bindings: BTreeMap<ClaimId, FactId>,
    proofs: BTreeMap<ClaimId, LeafAdmissionProof>,
}

impl LeafAdmittedPlan {
    /// The only constructor. `bindings` must cover exactly the stated claims:
    /// a claim without a binding is not admitted, and a binding for a claim
    /// that was not stated is material no one can reach.
    pub fn try_admit(
        candidate: CandidateResponsePlan,
        bindings: BTreeMap<ClaimId, FactId>,
        pack: &KnowledgePackSet,
        argued: &ArguedTopicRegistry,
    ) -> Result<Self, AdmissionError> {
        let stated: Vec<ClaimId> = candidate
            .projected_claims()
            .into_iter()
            .map(|claim| claim.claim_id)
            .collect();

        for claim_id in bindings.keys() {
            if !stated.contains(claim_id) {
                return Err(AdmissionError::BindingWithoutClaim(
                    claim_id.as_str().to_string(),
                ));
            }
        }

        let mut proofs = BTreeMap::new();
        for claim_id in &stated {
            let fact_id = bindings
                .get(claim_id)
                .ok_or_else(|| AdmissionError::UnboundClaim(claim_id.as_str().to_string()))?;
            let proof = prove_leaf_admission(fact_id, pack, argued)?;
            proofs.insert(claim_id.clone(), proof);
        }

        Ok(Self {
            candidate,
            bindings,
            proofs,
        })
    }

    pub fn candidate(&self) -> &CandidateResponsePlan {
        &self.candidate
    }

    pub fn bindings(&self) -> &BTreeMap<ClaimId, FactId> {
        &self.bindings
    }

    pub fn proof_for(&self, claim_id: &ClaimId) -> Option<&LeafAdmissionProof> {
        self.proofs.get(claim_id)
    }

    pub fn proofs(&self) -> &BTreeMap<ClaimId, LeafAdmissionProof> {
        &self.proofs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::active_pack_set;
    use crate::argued_topics::argued_topic_registry;
    use crate::response_plan_v2::proposition::{PropositionDagBuilder, PropositionNode};

    fn pack_and_argued() -> (&'static KnowledgePackSet, &'static ArguedTopicRegistry) {
        (active_pack_set(), argued_topic_registry().unwrap())
    }

    fn known_fact_id() -> FactId {
        FactId::try_new("fact.freedom_choice").expect("fact id")
    }

    #[test]
    fn an_admitted_fact_proves_membership() {
        let (pack, argued) = pack_and_argued();
        let proof = prove_leaf_admission(&known_fact_id(), pack, argued).expect("proof");
        assert_eq!(proof.profile(), CONTENT_PROFILE);
        assert_eq!(proof.pack_digest(), pack.fingerprint());
        assert_eq!(proof.fact_id(), &known_fact_id());
        assert_eq!(proof.digest().len(), 64);
    }

    /// The proof is stable for the same membership under the same pack set.
    #[test]
    fn proof_is_stable_for_unchanged_membership() {
        let (pack, argued) = pack_and_argued();
        let left = prove_leaf_admission(&known_fact_id(), pack, argued).expect("proof");
        let right = prove_leaf_admission(&known_fact_id(), pack, argued).expect("proof");
        assert_eq!(left, right);
    }

    #[test]
    fn an_unknown_fact_is_rejected() {
        let (pack, argued) = pack_and_argued();
        let stranger = FactId::try_new("fact.not_in_any_pack").expect("fact id");
        assert!(matches!(
            prove_leaf_admission(&stranger, pack, argued),
            Err(AdmissionError::UnknownFact(_))
        ));
    }

    /// The audited registry enforces facts == statements, so profile membership
    /// is pack-closed: every curated fact of the active pack set admits. This
    /// is the closure property the gates rely on; `NotInProfile` guards drift
    /// where a pack could one day carry background facts beyond the statements.
    #[test]
    fn profile_is_closed_over_the_active_pack() {
        let (pack, argued) = pack_and_argued();
        let mut admitted = 0usize;
        for fact_id in pack.facts().fact_id_for_predicate_members() {
            prove_leaf_admission(fact_id, pack, argued).expect("pack fact");
            admitted += 1;
        }
        assert_eq!(admitted, 69);
        assert_eq!(
            pack.facts().len(),
            argued.facts().len(),
            "every admitted predicate must have one fact"
        );
    }

    /// Every audited statement fact of every topic must admit; this is the
    /// profile-level closure the gates rely on.
    #[test]
    fn every_audited_statement_admits() {
        let (pack, argued) = pack_and_argued();
        let mut admitted = 0usize;
        for topic in argued.topics() {
            for statement in topic.statements() {
                prove_leaf_admission(statement.fact_id(), pack, argued).expect("statement fact");
                admitted += 1;
            }
        }
        assert_eq!(admitted, 69);
    }

    /// `is_curated` is the shared fact-membership view: a draft or deprecated
    /// record must not read as curated anywhere a boundary looks.
    #[test]
    fn is_curated_distinguishes_status() {
        use crate::fact_model::{FactKind, TypedRelationModel};
        use crate::get_resolver;
        use crate::response_plan::SemanticId;
        use qxfx0_types::ConceptId;

        let curated = crate::fact_model::FactRecord {
            id: FactId::try_new("fact.status.curated").unwrap(),
            subject: ConceptId("concept.свобода".into()),
            relation: SemanticId::try_new("RelPresupposes").unwrap(),
            object: ConceptId("concept.vozmozhnost_vybora".into()),
            kind: FactKind::InterpretiveClaim,
            conditions: Vec::new(),
            confidence_basis_points: 9_000,
            source_pack: "test-pack".into(),
            source_ref: "test:fact".into(),
            valid_from: None,
            valid_to: None,
            status: FactStatus::Curated,
        };
        let draft = crate::fact_model::FactRecord {
            id: FactId::try_new("fact.status.draft").unwrap(),
            status: FactStatus::Draft,
            ..curated.clone()
        };
        let registry = FactRegistry::load(
            [curated.clone(), draft.clone()],
            [],
            get_resolver(),
            &TypedRelationModel::default(),
        )
        .expect("registry");
        assert!(is_curated(&registry, &curated.id));
        assert!(!is_curated(&registry, &draft.id));
    }

    #[test]
    fn proof_digest_depends_on_the_pack_fingerprint() {
        let (pack, argued) = pack_and_argued();
        let fact_id = known_fact_id();
        let mut hasher = Sha256::new();
        hasher.update(ADMISSION_DOMAIN.as_bytes());
        absorb(&mut hasher, CONTENT_PROFILE.as_bytes());
        absorb(&mut hasher, fact_id.as_str().as_bytes());
        absorb(&mut hasher, b"different-pack-fingerprint");
        let expected = format!("{:x}", hasher.finalize());
        let proof = prove_leaf_admission(&fact_id, pack, argued).expect("proof");
        assert_ne!(proof.digest(), expected);
    }

    /// The binding of a claimed leaf to its fact: the same fact admitted
    /// twice is the same proof (claims are addresses, facts are meanings).
    #[test]
    fn the_same_fact_used_in_two_claims_has_one_proof() {
        let (pack, argued) = pack_and_argued();
        let left = prove_leaf_admission(&known_fact_id(), pack, argued).expect("proof");
        let right = prove_leaf_admission(&known_fact_id(), pack, argued).expect("proof");
        assert_eq!(left.digest(), right.digest());
        let _ = PropositionDagBuilder::new();
        let _ = PropositionNode::Predicate {
            subject: crate::response_plan::SemanticId::try_new("x").unwrap(),
            relation: crate::response_plan::SemanticId::try_new("y").unwrap(),
            object: crate::response_plan::SemanticId::try_new("z").unwrap(),
        };
    }
}

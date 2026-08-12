//! Immutable P3 evidence registry and deterministic aggregation policy v1.
use qxfx0_types::{
    sorted_evidence_set_digest, AssessmentId, BasisPoints, ConfidenceAssessment, EvidenceId,
    EvidenceRecord, ThesisDigest, ThesisEvidenceLink, ThesisEvidenceRole, ThesisId, TrustClass,
};
use std::collections::{BTreeMap, BTreeSet};

pub const EVIDENCE_POLICY_V1: u16 = 1;
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceRegistryError {
    #[error("unsupported evidence policy version {0}")]
    UnsupportedPolicy(u16),
    #[error("duplicate or colliding evidence id {0}")]
    DuplicateEvidence(EvidenceId),
    #[error("duplicate thesis/evidence/role link")]
    DuplicateLink,
    #[error("dangling evidence reference {0}")]
    DanglingEvidence(EvidenceId),
    #[error("dangling thesis reference {0}")]
    DanglingThesis(ThesisId),
    #[error("evidence digest mismatch for {0}")]
    DigestMismatch(EvidenceId),
    #[error("assessment mismatch for {0}")]
    AssessmentMismatch(AssessmentId),
    #[error("integer confidence aggregation overflow")]
    Overflow,
    #[error("evidence validation failed: {0}")]
    Type(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceRegistry {
    records: BTreeMap<EvidenceId, EvidenceRecord>,
    links: BTreeSet<ThesisEvidenceLink>,
}
impl EvidenceRegistry {
    pub fn load<I, J>(
        records: I,
        links: J,
        known_theses: &BTreeMap<ThesisId, ThesisDigest>,
    ) -> Result<Self, EvidenceRegistryError>
    where
        I: IntoIterator<Item = (EvidenceRecord, qxfx0_types::EvidenceDigest)>,
        J: IntoIterator<Item = ThesisEvidenceLink>,
    {
        let mut map = BTreeMap::new();
        for (record, declared) in records {
            let actual = record
                .canonical_digest()
                .map_err(|e| EvidenceRegistryError::Type(e.to_string()))?;
            if actual != declared {
                return Err(EvidenceRegistryError::DigestMismatch(record.id));
            }
            let id = record.id.clone();
            if map.insert(id.clone(), record).is_some() {
                return Err(EvidenceRegistryError::DuplicateEvidence(id));
            }
        }
        let mut set = BTreeSet::new();
        for link in links {
            if !known_theses.contains_key(&link.thesis_id) {
                return Err(EvidenceRegistryError::DanglingThesis(link.thesis_id));
            }
            if !map.contains_key(&link.evidence_id) {
                return Err(EvidenceRegistryError::DanglingEvidence(link.evidence_id));
            }
            if !set.insert(link) {
                return Err(EvidenceRegistryError::DuplicateLink);
            }
        }
        Ok(Self {
            records: map,
            links: set,
        })
    }
    pub fn records(&self) -> &BTreeMap<EvidenceId, EvidenceRecord> {
        &self.records
    }
    pub fn links(&self) -> &BTreeSet<ThesisEvidenceLink> {
        &self.links
    }
    pub fn assess(
        &self,
        id: AssessmentId,
        thesis_id: &ThesisId,
        thesis_digest: ThesisDigest,
        policy: u16,
    ) -> Result<ConfidenceAssessment, EvidenceRegistryError> {
        if policy != EVIDENCE_POLICY_V1 {
            return Err(EvidenceRegistryError::UnsupportedPolicy(policy));
        }
        let linked = self
            .links
            .iter()
            .filter(|l| &l.thesis_id == thesis_id)
            .collect::<Vec<_>>();
        let digests = linked
            .iter()
            .map(|l| {
                self.records[&l.evidence_id]
                    .canonical_digest()
                    .expect("validated immutable evidence")
            })
            .collect::<Vec<_>>();
        let set_digest = sorted_evidence_set_digest(digests)
            .map_err(|e| EvidenceRegistryError::Type(e.to_string()))?;
        let mut support: u32 = 0;
        let mut challenge: u32 = 0;
        let mut count: u32 = 0;
        for link in linked {
            let r = &self.records[&link.evidence_id];
            if !matches!(
                r.trust_class,
                TrustClass::CuratedEmbedded | TrustClass::VerifiedSignedExternal
            ) {
                continue;
            }
            count = count
                .checked_add(1)
                .ok_or(EvidenceRegistryError::Overflow)?;
            match link.role {
                ThesisEvidenceRole::Supports => {
                    support = support
                        .checked_add(r.strength_basis_points.get() as u32)
                        .ok_or(EvidenceRegistryError::Overflow)?
                }
                ThesisEvidenceRole::Challenges => {
                    challenge = challenge
                        .checked_add(r.strength_basis_points.get() as u32)
                        .ok_or(EvidenceRegistryError::Overflow)?
                }
                ThesisEvidenceRole::Context => {}
            }
        }
        let total = support
            .checked_add(challenge)
            .ok_or(EvidenceRegistryError::Overflow)?;
        let bp = if total == 0 {
            0
        } else {
            support
                .checked_mul(10_000)
                .ok_or(EvidenceRegistryError::Overflow)?
                / total
        };
        Ok(ConfidenceAssessment {
            id,
            thesis_id: thesis_id.clone(),
            thesis_digest,
            policy_version: policy,
            evidence_set_digest: set_digest,
            confidence_basis_points: BasisPoints::try_new(bp as u16)
                .map_err(|e| EvidenceRegistryError::Type(e.to_string()))?,
            authority_evidence_count: count,
        })
    }
    pub fn verify_assessment(
        &self,
        expected: &ConfidenceAssessment,
    ) -> Result<(), EvidenceRegistryError> {
        let actual = self.assess(
            expected.id.clone(),
            &expected.thesis_id,
            expected.thesis_digest,
            expected.policy_version,
        )?;
        if &actual == expected {
            Ok(())
        } else {
            Err(EvidenceRegistryError::AssessmentMismatch(
                expected.id.clone(),
            ))
        }
    }
}

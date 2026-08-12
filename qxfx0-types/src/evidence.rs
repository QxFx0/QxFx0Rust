//! P3 typed evidence, provenance, and integer-only confidence primitives.

use crate::{ThesisDigest, ThesisId};
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, fmt};

pub const EVIDENCE_CANONICAL_VERSION: u8 = 1;
pub const MAX_EVIDENCE_ID_BYTES: usize = 256;
pub const MAX_SOURCE_ID_BYTES: usize = 256;
pub const MAX_ASSESSMENT_ID_BYTES: usize = 256;
pub const MAX_SOURCE_REVISION_BYTES: usize = 512;
pub const MAX_EVIDENCE_SET: usize = 4096;
const RECORD_DOMAIN: &[u8] = b"qxfx0:evidence-record:v1";
const LINK_DOMAIN: &[u8] = b"qxfx0:thesis-evidence-link:v1";
const SET_DOMAIN: &[u8] = b"qxfx0:evidence-set:v1";
const ASSESSMENT_DOMAIN: &[u8] = b"qxfx0:confidence-assessment:v1";

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceTypeError {
    #[error("{0} must be non-empty and at most {1} UTF-8 bytes")]
    InvalidText(&'static str, usize),
    #[error("basis points must be in 0..=10000")]
    InvalidBasisPoints,
    #[error("digest must be 64 lowercase hexadecimal characters")]
    InvalidDigest,
    #[error("evidence set exceeds its bound")]
    EvidenceSetTooLarge,
    #[error("duplicate evidence digest in set")]
    DuplicateEvidenceDigest,
    #[error("canonical field length exceeds u32")]
    CanonicalLengthOverflow,
}

macro_rules! bounded_id {
    ($name:ident, $label:literal, $max:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
        #[serde(transparent)]
        pub struct $name(String);
        impl $name {
            pub fn try_new(value: impl Into<String>) -> Result<Self, EvidenceTypeError> {
                let value = value.into();
                validate_text($label, &value, $max)?;
                Ok(Self(value))
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }
        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
                Self::try_new(String::deserialize(d)?).map_err(de::Error::custom)
            }
        }
    };
}
bounded_id!(EvidenceId, "evidence id", MAX_EVIDENCE_ID_BYTES);
bounded_id!(SourceId, "source id", MAX_SOURCE_ID_BYTES);
bounded_id!(AssessmentId, "assessment id", MAX_ASSESSMENT_ID_BYTES);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(transparent)]
pub struct BasisPoints(u16);
impl BasisPoints {
    pub fn try_new(value: u16) -> Result<Self, EvidenceTypeError> {
        if value <= 10_000 {
            Ok(Self(value))
        } else {
            Err(EvidenceTypeError::InvalidBasisPoints)
        }
    }
    pub const fn get(self) -> u16 {
        self.0
    }
}
impl<'de> Deserialize<'de> for BasisPoints {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::try_new(u16::deserialize(d)?).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceKind {
    PrimarySource,
    CuratedReference,
    SignedAttestation,
    Observation,
    DerivedAnalysis,
}
impl EvidenceKind {
    const fn tag(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustClass {
    CuratedEmbedded,
    VerifiedSignedExternal,
    UserObservation,
    GeneratedObservation,
    Quarantine,
}
impl TrustClass {
    const fn tag(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThesisEvidenceRole {
    Supports,
    Challenges,
    Context,
}
impl ThesisEvidenceRole {
    const fn tag(self) -> u8 {
        self as u8
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EvidenceDigest([u8; 32]);
impl EvidenceDigest {
    pub fn from_hex(value: &str) -> Result<Self, EvidenceTypeError> {
        if value.len() != 64
            || !value
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(EvidenceTypeError::InvalidDigest);
        }
        let mut out = [0; 32];
        for (i, slot) in out.iter_mut().enumerate() {
            *slot = u8::from_str_radix(&value[i * 2..i * 2 + 2], 16)
                .map_err(|_| EvidenceTypeError::InvalidDigest)?;
        }
        Ok(Self(out))
    }
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
    pub fn to_hex(self) -> String {
        self.0.iter().map(|b| format!("{b:02x}")).collect()
    }
}
impl fmt::Display for EvidenceDigest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_hex())
    }
}
impl Serialize for EvidenceDigest {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_hex())
    }
}
impl<'de> Deserialize<'de> for EvidenceDigest {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        Self::from_hex(&String::deserialize(d)?).map_err(de::Error::custom)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRecord {
    pub id: EvidenceId,
    pub source_id: SourceId,
    pub kind: EvidenceKind,
    pub trust_class: TrustClass,
    pub source_revision: String,
    pub content_digest: EvidenceDigest,
    pub strength_basis_points: BasisPoints,
}
impl EvidenceRecord {
    pub fn validate(&self) -> Result<(), EvidenceTypeError> {
        validate_text(
            "source revision",
            &self.source_revision,
            MAX_SOURCE_REVISION_BYTES,
        )
    }
    pub fn canonical_digest(&self) -> Result<EvidenceDigest, EvidenceTypeError> {
        self.validate()?;
        let mut out = Vec::new();
        put(&mut out, RECORD_DOMAIN)?;
        out.push(EVIDENCE_CANONICAL_VERSION);
        put(&mut out, self.id.as_str().as_bytes())?;
        put(&mut out, self.source_id.as_str().as_bytes())?;
        out.push(self.kind.tag());
        out.push(self.trust_class.tag());
        put(&mut out, self.source_revision.as_bytes())?;
        out.extend_from_slice(self.content_digest.as_bytes());
        out.extend_from_slice(&self.strength_basis_points.get().to_be_bytes());
        Ok(EvidenceDigest::from_bytes(Sha256::digest(out).into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThesisEvidenceLink {
    pub thesis_id: ThesisId,
    pub evidence_id: EvidenceId,
    pub role: ThesisEvidenceRole,
}
impl ThesisEvidenceLink {
    pub fn canonical_digest(&self) -> Result<EvidenceDigest, EvidenceTypeError> {
        let mut out = Vec::new();
        put(&mut out, LINK_DOMAIN)?;
        out.push(1);
        put(&mut out, self.thesis_id.as_str().as_bytes())?;
        put(&mut out, self.evidence_id.as_str().as_bytes())?;
        out.push(self.role.tag());
        Ok(EvidenceDigest::from_bytes(Sha256::digest(out).into()))
    }
}

pub fn sorted_evidence_set_digest<I>(digests: I) -> Result<EvidenceDigest, EvidenceTypeError>
where
    I: IntoIterator<Item = EvidenceDigest>,
{
    let input: Vec<_> = digests.into_iter().collect();
    if input.len() > MAX_EVIDENCE_SET {
        return Err(EvidenceTypeError::EvidenceSetTooLarge);
    }
    let values: BTreeSet<_> = input.iter().copied().collect();
    if values.len() != input.len() {
        return Err(EvidenceTypeError::DuplicateEvidenceDigest);
    }
    let mut out = Vec::new();
    put(&mut out, SET_DOMAIN)?;
    out.push(1);
    out.extend_from_slice(&(values.len() as u32).to_be_bytes());
    for value in values {
        out.extend_from_slice(value.as_bytes());
    }
    Ok(EvidenceDigest::from_bytes(Sha256::digest(out).into()))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConfidenceAssessment {
    pub id: AssessmentId,
    pub thesis_id: ThesisId,
    pub thesis_digest: ThesisDigest,
    pub policy_version: u16,
    pub evidence_set_digest: EvidenceDigest,
    pub confidence_basis_points: BasisPoints,
    pub authority_evidence_count: u32,
}
impl ConfidenceAssessment {
    pub fn canonical_digest(&self) -> Result<EvidenceDigest, EvidenceTypeError> {
        let mut out = Vec::new();
        put(&mut out, ASSESSMENT_DOMAIN)?;
        out.push(1);
        put(&mut out, self.id.as_str().as_bytes())?;
        put(&mut out, self.thesis_id.as_str().as_bytes())?;
        out.extend_from_slice(self.thesis_digest.as_bytes());
        out.extend_from_slice(&self.policy_version.to_be_bytes());
        out.extend_from_slice(self.evidence_set_digest.as_bytes());
        out.extend_from_slice(&self.confidence_basis_points.get().to_be_bytes());
        out.extend_from_slice(&self.authority_evidence_count.to_be_bytes());
        Ok(EvidenceDigest::from_bytes(Sha256::digest(out).into()))
    }
}

fn validate_text(label: &'static str, value: &str, max: usize) -> Result<(), EvidenceTypeError> {
    if value.trim().is_empty() || value.len() > max {
        Err(EvidenceTypeError::InvalidText(label, max))
    } else {
        Ok(())
    }
}
fn put(out: &mut Vec<u8>, value: &[u8]) -> Result<(), EvidenceTypeError> {
    let len = u32::try_from(value.len()).map_err(|_| EvidenceTypeError::CanonicalLengthOverflow)?;
    out.extend_from_slice(&len.to_be_bytes());
    out.extend_from_slice(value);
    Ok(())
}

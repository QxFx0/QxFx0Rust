//! Private, deterministic observation evidence for catalog-bound theses.
//!
//! A receipt is never an authority decision, a persistence command, or a user
//! profile record. It deliberately excludes session identifiers, input text,
//! response text, and wall-clock data.

use crate::{FactId, ThesisDigest, ThesisId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;

pub const THESIS_OBSERVATION_VERSION: u8 = 1;
const DOMAIN: &[u8] = b"qxfx0.thesis-observation.v1\0";
const TURN_BINDING_DOMAIN: &[u8] = b"qxfx0.thesis-observation-turn.v1\0";

/// Create an opaque, domain-separated binding to one input at one logical
/// turn. The input and session identifier are deliberately never serialized
/// into an observation receipt.
pub fn calculate_thesis_observation_turn_binding(
    session_id: &str,
    pre_turn_index: u64,
    raw_input: &str,
) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(TURN_BINDING_DOMAIN);
    push(&mut hasher, session_id.as_bytes());
    hasher.update(pre_turn_index.to_be_bytes());
    push(&mut hasher, raw_input.as_bytes());
    hasher.finalize().into()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[repr(u8)]
pub enum ThesisObservationOutcome {
    Observed = 0,
    NoAuditedPlan = 1,
    GuardBlocked = 2,
    V2AuthorityIsolated = 3,
    ValidationRejected = 4,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThesisObservationReceipt {
    version: u8,
    outcome: ThesisObservationOutcome,
    /// Pre-finalize turn ordinal; it binds evidence to ordering without
    /// exporting a session identifier or wall-clock timestamp.
    pre_turn_index: u64,
    /// Opaque binding to session, turn, and raw input. It cannot be used as a
    /// source of authority and does not export those private values.
    turn_binding: [u8; 32],
    #[serde(skip_serializing_if = "Option::is_none")]
    thesis_id: Option<ThesisId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thesis_digest: Option<ThesisDigest>,
    #[serde(skip_serializing_if = "Option::is_none")]
    fact_id: Option<FactId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pack_fingerprint: Option<String>,
    digest: [u8; 32],
}

impl ThesisObservationReceipt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        outcome: ThesisObservationOutcome,
        pre_turn_index: u64,
        turn_binding: [u8; 32],
        thesis_id: Option<ThesisId>,
        thesis_digest: Option<ThesisDigest>,
        fact_id: Option<FactId>,
        pack_fingerprint: Option<String>,
    ) -> Result<Self, ThesisObservationValidationError> {
        let mut receipt = Self {
            version: THESIS_OBSERVATION_VERSION,
            outcome,
            pre_turn_index,
            turn_binding,
            thesis_id,
            thesis_digest,
            fact_id,
            pack_fingerprint,
            digest: [0; 32],
        };
        receipt.validate_structure()?;
        receipt.digest = receipt.calculate_digest();
        Ok(receipt)
    }

    pub fn validate(&self) -> Result<(), ThesisObservationValidationError> {
        self.validate_structure()?;
        if self.digest != self.calculate_digest() {
            return Err(ThesisObservationValidationError::DigestMismatch);
        }
        Ok(())
    }

    pub const fn outcome(&self) -> ThesisObservationOutcome {
        self.outcome
    }
    pub const fn pre_turn_index(&self) -> u64 {
        self.pre_turn_index
    }
    pub const fn turn_binding(&self) -> &[u8; 32] {
        &self.turn_binding
    }
    pub fn thesis_id(&self) -> Option<&ThesisId> {
        self.thesis_id.as_ref()
    }
    pub fn thesis_digest(&self) -> Option<ThesisDigest> {
        self.thesis_digest
    }
    pub fn fact_id(&self) -> Option<&FactId> {
        self.fact_id.as_ref()
    }
    pub fn pack_fingerprint(&self) -> Option<&str> {
        self.pack_fingerprint.as_deref()
    }
    pub const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }

    fn validate_structure(&self) -> Result<(), ThesisObservationValidationError> {
        if self.version != THESIS_OBSERVATION_VERSION {
            return Err(ThesisObservationValidationError::UnsupportedVersion(
                self.version,
            ));
        }
        let complete = self.thesis_id.is_some()
            && self.thesis_digest.is_some()
            && self.fact_id.is_some()
            && self.pack_fingerprint.is_some();
        if self.outcome == ThesisObservationOutcome::Observed {
            if !complete {
                return Err(ThesisObservationValidationError::MissingBinding);
            }
            let fingerprint = self.pack_fingerprint.as_deref().expect("complete binding");
            if fingerprint.len() != 64
                || !fingerprint
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
            {
                return Err(ThesisObservationValidationError::InvalidPackFingerprint);
            }
        } else if self.thesis_id.is_some()
            || self.thesis_digest.is_some()
            || self.fact_id.is_some()
            || self.pack_fingerprint.is_some()
        {
            return Err(ThesisObservationValidationError::UnexpectedBinding);
        }
        Ok(())
    }

    fn calculate_digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        hasher.update(DOMAIN);
        hasher.update([self.version, self.outcome as u8]);
        hasher.update(self.pre_turn_index.to_be_bytes());
        hasher.update(self.turn_binding);
        match (
            &self.thesis_id,
            self.thesis_digest,
            &self.fact_id,
            &self.pack_fingerprint,
        ) {
            (Some(id), Some(digest), Some(fact), Some(fingerprint)) => {
                hasher.update([1]);
                push(&mut hasher, id.as_str().as_bytes());
                hasher.update(digest.as_bytes());
                push(&mut hasher, fact.as_str().as_bytes());
                push(&mut hasher, fingerprint.as_bytes());
            }
            _ => hasher.update([0]),
        }
        hasher.finalize().into()
    }
}

fn push(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ThesisObservationValidationError {
    #[error("unsupported thesis observation version {0}")]
    UnsupportedVersion(u8),
    #[error("observed thesis receipt requires every catalog binding")]
    MissingBinding,
    #[error("non-observed thesis receipt must not carry catalog bindings")]
    UnexpectedBinding,
    #[error("pack fingerprint must be a 64-character hexadecimal digest")]
    InvalidPackFingerprint,
    #[error("thesis observation digest does not match payload")]
    DigestMismatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn receipt_is_deterministic_private_and_tamper_evident() {
        let first = ThesisObservationReceipt::new(
            ThesisObservationOutcome::Observed,
            3,
            calculate_thesis_observation_turn_binding("session", 3, "private input"),
            Some(ThesisId::try_new("thesis.freedom").unwrap()),
            Some(ThesisDigest::from_bytes([7; 32])),
            Some(FactId::try_new("fact.freedom").unwrap()),
            Some("a".repeat(64)),
        )
        .unwrap();
        let second = ThesisObservationReceipt::new(
            ThesisObservationOutcome::Observed,
            3,
            calculate_thesis_observation_turn_binding("session", 3, "private input"),
            Some(ThesisId::try_new("thesis.freedom").unwrap()),
            Some(ThesisDigest::from_bytes([7; 32])),
            Some(FactId::try_new("fact.freedom").unwrap()),
            Some("a".repeat(64)),
        )
        .unwrap();
        assert_eq!(first.digest(), second.digest());
        let encoded = serde_json::to_string(&first).unwrap();
        assert!(!encoded.contains("session"));
        assert!(!encoded.contains("response"));
        let mut value = serde_json::to_value(first).unwrap();
        value["pre_turn_index"] = serde_json::json!(4);
        let changed: ThesisObservationReceipt = serde_json::from_value(value).unwrap();
        assert_eq!(
            changed.validate(),
            Err(ThesisObservationValidationError::DigestMismatch)
        );
    }
}

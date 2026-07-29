//! Signed, transport-independent authority attestations for system stances.
//!
//! This module deliberately contains no network client, clock, persistence, or
//! key-refresh logic. An integrating service supplies a signed attestation,
//! public-key configuration, and an explicit verification time. The pipeline
//! can then accept only a verified decision without deriving its polarity from
//! user text, history, or a guard result.

use crate::stance::{StancePolarity, StanceTopic, SystemStanceDecision};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use thiserror::Error;

pub const STANCE_ATTESTATION_VERSION: u8 = 1;
const ATTESTATION_DOMAIN: &[u8] = b"qxfx0.stance-attestation.v1\0";
const REQUEST_DIGEST_DOMAIN: &[u8] = b"qxfx0.stance-request-digest.v1\0";
const MAX_TEXT_FIELD_BYTES: usize = 128;
const MAX_SESSION_ID_BYTES: usize = 256;

/// Digest of the exact request bound into a signed attestation.
///
/// The byte layout is domain-separated and length-prefixed; it does not rely
/// on JSON serialization or separator characters in user input.
pub fn calculate_stance_request_digest(session_id: &str, raw_text: &str) -> [u8; 32] {
    let mut digest = Sha256::new();
    digest.update(REQUEST_DIGEST_DOMAIN);
    push_bytes(&mut digest, session_id.as_bytes());
    push_bytes(&mut digest, raw_text.as_bytes());
    digest.finalize().into()
}

/// Payload signed by an external, authorized stance issuer.
///
/// `signature` is intentionally held by [`SignedStanceDecision`], so the
/// canonical payload always has one unambiguous representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StanceDecisionAttestation {
    pub version: u8,
    pub issuer_id: String,
    pub key_id: String,
    pub audience: String,
    pub session_id: String,
    /// Dialogue turn count before this turn is processed.
    pub expected_pre_turn: usize,
    pub topic: StanceTopic,
    pub polarity: StancePolarity,
    pub request_digest: [u8; 32],
    /// Opaque issuer-generated identifier, unique for this attestation.
    pub decision_id: [u8; 16],
    pub issued_at_unix_seconds: i64,
    pub expires_at_unix_seconds: i64,
}

impl StanceDecisionAttestation {
    /// Canonical bytes covered by Ed25519. This is a versioned binary
    /// contract, not a serialized Rust or JSON representation.
    pub fn canonical_bytes(&self) -> Result<Vec<u8>, StanceVerificationError> {
        self.validate()?;
        let mut bytes = Vec::with_capacity(512);
        bytes.extend_from_slice(ATTESTATION_DOMAIN);
        bytes.push(self.version);
        push_vec(&mut bytes, self.issuer_id.as_bytes());
        push_vec(&mut bytes, self.key_id.as_bytes());
        push_vec(&mut bytes, self.audience.as_bytes());
        push_vec(&mut bytes, self.session_id.as_bytes());
        bytes.extend_from_slice(&(self.expected_pre_turn as u64).to_be_bytes());
        push_vec(&mut bytes, self.topic.as_str().as_bytes());
        bytes.push(match self.polarity {
            StancePolarity::Affirmed => 1,
            StancePolarity::Rejected => 2,
        });
        bytes.extend_from_slice(&self.request_digest);
        bytes.extend_from_slice(&self.decision_id);
        bytes.extend_from_slice(&self.issued_at_unix_seconds.to_be_bytes());
        bytes.extend_from_slice(&self.expires_at_unix_seconds.to_be_bytes());
        Ok(bytes)
    }

    fn validate(&self) -> Result<(), StanceVerificationError> {
        if self.version != STANCE_ATTESTATION_VERSION {
            return Err(StanceVerificationError::UnsupportedVersion(self.version));
        }
        validate_text("issuer_id", &self.issuer_id, MAX_TEXT_FIELD_BYTES)?;
        validate_text("key_id", &self.key_id, MAX_TEXT_FIELD_BYTES)?;
        validate_text("audience", &self.audience, MAX_TEXT_FIELD_BYTES)?;
        validate_text("session_id", &self.session_id, MAX_SESSION_ID_BYTES)?;
        if self.issued_at_unix_seconds < 0
            || self.expires_at_unix_seconds < self.issued_at_unix_seconds
        {
            return Err(StanceVerificationError::InvalidValidityWindow);
        }
        Ok(())
    }
}

/// An attestation and its detached Ed25519 signature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedStanceDecision {
    pub attestation: StanceDecisionAttestation,
    /// Fixed-width detached Ed25519 signature. A fixed array rejects an
    /// unbounded signature allocation at the deserialization boundary.
    #[serde(with = "signature_serde")]
    pub signature: [u8; 64],
}

/// Turn values that a signed attestation must bind exactly.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanceVerificationContext {
    pub audience: String,
    pub session_id: String,
    pub expected_pre_turn: usize,
    pub request_digest: [u8; 32],
    /// Explicitly injected to keep verification deterministic in replay.
    pub verification_time_unix_seconds: i64,
    /// Maximum accepted attestation lifetime. The integrating service owns
    /// policy selection, while verification enforces the bound deterministically.
    pub max_validity_seconds: i64,
}

/// Runtime-supplied verification policy. Transport and key management stay
/// outside the core; this value makes time and lifetime policy explicit at the
/// signed-decision boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StanceAuthorityVerificationPolicy {
    pub audience: String,
    pub verification_time_unix_seconds: i64,
    pub max_validity_seconds: i64,
}

/// A decision accepted by a signature verifier and bound to the current turn.
/// It has no persistence or routing effect until a caller explicitly records
/// it through the existing stance-provenance API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedStanceDecision {
    decision: SystemStanceDecision,
    decision_id: [u8; 16],
    issuer_id: String,
}

impl VerifiedStanceDecision {
    pub fn decision(&self) -> &SystemStanceDecision {
        &self.decision
    }

    pub fn into_decision(self) -> SystemStanceDecision {
        self.decision
    }

    pub fn decision_id(&self) -> [u8; 16] {
        self.decision_id
    }

    pub fn issuer_id(&self) -> &str {
        &self.issuer_id
    }
}

/// Pluggable signature primitive. Implementations validate only the signature
/// for the supplied canonical payload; this module always performs the
/// session, turn, digest, audience, and expiry bindings itself.
pub trait StanceDecisionSignatureVerifier {
    fn verify_signature(
        &self,
        issuer_id: &str,
        key_id: &str,
        canonical_payload: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), StanceVerificationError>;
}

/// Verifies a signed attestation and returns a capability-like decision for
/// the current turn. Any failed check returns a typed rejection and must leave
/// the ordinary pipeline path unchanged.
pub fn verify_signed_stance_decision(
    verifier: &impl StanceDecisionSignatureVerifier,
    signed: &SignedStanceDecision,
    context: &StanceVerificationContext,
) -> Result<VerifiedStanceDecision, StanceVerificationError> {
    signed.attestation.validate()?;
    if signed.attestation.audience != context.audience {
        return Err(StanceVerificationError::AudienceMismatch);
    }
    if signed.attestation.session_id != context.session_id {
        return Err(StanceVerificationError::SessionMismatch);
    }
    if signed.attestation.expected_pre_turn != context.expected_pre_turn {
        return Err(StanceVerificationError::TurnMismatch);
    }
    if signed.attestation.request_digest != context.request_digest {
        return Err(StanceVerificationError::RequestDigestMismatch);
    }
    if context.verification_time_unix_seconds < signed.attestation.issued_at_unix_seconds {
        return Err(StanceVerificationError::NotYetValid);
    }
    if context.verification_time_unix_seconds > signed.attestation.expires_at_unix_seconds {
        return Err(StanceVerificationError::Expired);
    }
    if context.max_validity_seconds < 0 {
        return Err(StanceVerificationError::InvalidMaximumValidity);
    }
    if signed.attestation.expires_at_unix_seconds - signed.attestation.issued_at_unix_seconds
        > context.max_validity_seconds
    {
        return Err(StanceVerificationError::ValidityWindowTooLong);
    }
    let canonical = signed.attestation.canonical_bytes()?;
    verifier.verify_signature(
        &signed.attestation.issuer_id,
        &signed.attestation.key_id,
        &canonical,
        &signed.signature,
    )?;
    Ok(VerifiedStanceDecision {
        decision: SystemStanceDecision {
            topic: signed.attestation.topic.clone(),
            polarity: signed.attestation.polarity,
        },
        decision_id: signed.attestation.decision_id,
        issuer_id: signed.attestation.issuer_id.clone(),
    })
}

/// Static Ed25519 public-key verifier. Key provisioning and rotation remain
/// outside this crate and can replace this verifier through the trait above.
#[derive(Debug, Clone, Default)]
pub struct Ed25519StanceDecisionVerifier {
    public_keys: BTreeMap<(String, String), [u8; 32]>,
}

impl Ed25519StanceDecisionVerifier {
    pub fn new(keys: impl IntoIterator<Item = ((String, String), [u8; 32])>) -> Self {
        Self {
            public_keys: keys.into_iter().collect(),
        }
    }
}

impl StanceDecisionSignatureVerifier for Ed25519StanceDecisionVerifier {
    fn verify_signature(
        &self,
        issuer_id: &str,
        key_id: &str,
        canonical_payload: &[u8],
        signature: &[u8; 64],
    ) -> Result<(), StanceVerificationError> {
        let key = self
            .public_keys
            .get(&(issuer_id.to_owned(), key_id.to_owned()))
            .ok_or_else(|| StanceVerificationError::UnknownIssuerKey {
                issuer_id: issuer_id.to_owned(),
                key_id: key_id.to_owned(),
            })?;
        let verifying_key =
            VerifyingKey::from_bytes(key).map_err(|_| StanceVerificationError::InvalidPublicKey)?;
        let signature = Signature::from_bytes(signature);
        verifying_key
            .verify(canonical_payload, &signature)
            .map_err(|_| StanceVerificationError::InvalidSignature)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum StanceVerificationError {
    #[error("unsupported stance attestation version {0}")]
    UnsupportedVersion(u8),
    #[error("{field} is empty, too long, or contains a control character")]
    InvalidTextField { field: &'static str },
    #[error("attestation issued_at/expires_at window is invalid")]
    InvalidValidityWindow,
    #[error("attestation audience does not match this runtime")]
    AudienceMismatch,
    #[error("attestation session does not match this turn")]
    SessionMismatch,
    #[error("attestation expected turn does not match this turn")]
    TurnMismatch,
    #[error("attestation request digest does not match this request")]
    RequestDigestMismatch,
    #[error("attestation is not yet valid")]
    NotYetValid,
    #[error("attestation has expired")]
    Expired,
    #[error("attestation validity window exceeds the configured limit")]
    ValidityWindowTooLong,
    #[error("maximum attestation validity must not be negative")]
    InvalidMaximumValidity,
    #[error("unknown issuer/key identifier: {issuer_id}/{key_id}")]
    UnknownIssuerKey { issuer_id: String, key_id: String },
    #[error("invalid Ed25519 public key")]
    InvalidPublicKey,
    #[error("invalid Ed25519 signature")]
    InvalidSignature,
}

fn validate_text(
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<(), StanceVerificationError> {
    if value.trim().is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(StanceVerificationError::InvalidTextField { field });
    }
    Ok(())
}

fn push_vec(output: &mut Vec<u8>, value: &[u8]) {
    output.extend_from_slice(&(value.len() as u32).to_be_bytes());
    output.extend_from_slice(value);
}

fn push_bytes(digest: &mut Sha256, value: &[u8]) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value);
}

mod signature_serde {
    use serde::de::{Error, SeqAccess, Visitor};
    use serde::ser::SerializeTuple;
    use serde::{Deserializer, Serializer};
    use std::fmt;

    pub fn serialize<S>(value: &[u8; 64], serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let mut tuple = serializer.serialize_tuple(64)?;
        for byte in value {
            tuple.serialize_element(byte)?;
        }
        tuple.end()
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 64], D::Error>
    where
        D: Deserializer<'de>,
    {
        struct SignatureVisitor;

        impl<'de> Visitor<'de> for SignatureVisitor {
            type Value = [u8; 64];

            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("an exactly 64-byte Ed25519 signature")
            }

            fn visit_seq<A>(self, mut sequence: A) -> Result<Self::Value, A::Error>
            where
                A: SeqAccess<'de>,
            {
                let mut signature = [0; 64];
                for byte in &mut signature {
                    *byte = sequence
                        .next_element()?
                        .ok_or_else(|| A::Error::invalid_length(64, &self))?;
                }
                if sequence.next_element::<u8>()?.is_some() {
                    return Err(A::Error::invalid_length(65, &self));
                }
                Ok(signature)
            }
        }

        deserializer.deserialize_tuple(64, SignatureVisitor)
    }
}

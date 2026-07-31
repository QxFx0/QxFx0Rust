//! Pure eligibility contract for a possible narrow temporal recovery.
//!
//! Producing an [`TemporalRecoveryEligibility::Eligible`] value does not apply
//! a strategy or mutate runtime state. No pipeline integration consumes this
//! contract yet.

use serde::{Deserialize, Serialize};

use crate::anomaly::{
    AnomalyKind, AnomalyRecoveryDecision, AnomalyRecoveryResult, AnomalyRecoveryStrategy,
};
use qxfx0_types::stance::{StancePolarity, StanceSource, StanceTopic, TemporalStanceContradiction};

/// Explicit rollout mode supplied by a future integrating boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalRecoveryMode {
    Disabled,
    ShadowOnly,
    LimitedNonProduction,
}

/// Environment class admitted by the v1 contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalRecoveryEnvironment {
    NonProduction,
    Production,
}

/// Typed proof class for the historical rejected stance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalRecoveryAuthority {
    Unavailable,
    VerifiedSignedIssuer {
        issuer_id: String,
        key_id: String,
        decision_id_hex: String,
        signed_payload_fingerprint_hex: String,
    },
}

/// Caller-supplied, replay-visible context for pure eligibility evaluation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalRecoveryEligibilityContext {
    pub mode: TemporalRecoveryMode,
    pub environment: TemporalRecoveryEnvironment,
    pub session_id: String,
    pub session_allowlisted: bool,
    pub window_id: String,
    pub authority: TemporalRecoveryAuthority,
    pub turn_blocked: bool,
    pub provenance_fresh: bool,
    pub replay_consistent: bool,
    pub audit_ready: bool,
    pub prior_requests_for_topic_in_window: u8,
}

/// Stable fail-closed reasons, evaluated in declaration order below.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalRecoveryDenialReason {
    ModeDisabled,
    ShadowOnly,
    ProductionForbidden,
    SessionNotAllowlisted,
    AuthorityUnavailable,
    InvalidAuthorityProof,
    StrategyNotPermitted,
    EvidenceMismatch,
    TopicMismatch,
    TurnBlocked,
    StaleProvenance,
    ReplayMismatch,
    AuditUnavailable,
    AlreadyRequested,
    InvalidScope,
}

/// A pure capability-shaped value that is not consumed by any runtime path.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalRecoveryPermit {
    pub session_id: String,
    pub topic: StanceTopic,
    pub window_id: String,
    pub strategy: AnomalyRecoveryStrategy,
    pub result: AnomalyRecoveryResult,
    pub issuer_id: String,
    pub key_id: String,
    pub decision_id_hex: String,
    pub signed_payload_fingerprint_hex: String,
    pub max_requests_for_topic_in_window: u8,
    pub idempotency_key: String,
}

/// Pure eligibility result. Neither variant changes state.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TemporalRecoveryEligibility {
    Denied(TemporalRecoveryDenialReason),
    Eligible(TemporalRecoveryPermit),
}

/// Evaluates the narrow v1 policy without applying recovery or mutating state.
pub fn evaluate_temporal_recovery_eligibility(
    decision: &AnomalyRecoveryDecision,
    contradiction: &TemporalStanceContradiction,
    context: &TemporalRecoveryEligibilityContext,
) -> TemporalRecoveryEligibility {
    match context.mode {
        TemporalRecoveryMode::Disabled => {
            return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::ModeDisabled)
        }
        TemporalRecoveryMode::ShadowOnly => {
            return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::ShadowOnly)
        }
        TemporalRecoveryMode::LimitedNonProduction => {}
    }
    if context.environment != TemporalRecoveryEnvironment::NonProduction {
        return TemporalRecoveryEligibility::Denied(
            TemporalRecoveryDenialReason::ProductionForbidden,
        );
    }
    if !context.session_allowlisted {
        return TemporalRecoveryEligibility::Denied(
            TemporalRecoveryDenialReason::SessionNotAllowlisted,
        );
    }
    let TemporalRecoveryAuthority::VerifiedSignedIssuer {
        issuer_id,
        key_id,
        decision_id_hex,
        signed_payload_fingerprint_hex,
    } = &context.authority
    else {
        return TemporalRecoveryEligibility::Denied(
            TemporalRecoveryDenialReason::AuthorityUnavailable,
        );
    };
    if issuer_id.trim().is_empty()
        || key_id.trim().is_empty()
        || !is_lower_hex_len(decision_id_hex, 32)
        || !is_lower_hex_len(signed_payload_fingerprint_hex, 64)
    {
        return TemporalRecoveryEligibility::Denied(
            TemporalRecoveryDenialReason::InvalidAuthorityProof,
        );
    }
    if decision.kind != AnomalyKind::Temporal
        || decision.strategy != AnomalyRecoveryStrategy::RequestRevision
        || decision.result != AnomalyRecoveryResult::RevisionRequested
        || decision.max_retries != 0
    {
        return TemporalRecoveryEligibility::Denied(
            TemporalRecoveryDenialReason::StrategyNotPermitted,
        );
    }
    if decision.evidence != contradiction.to_anomaly_evidence()
        || contradiction.current.polarity != StancePolarity::Affirmed
        || contradiction.historical.polarity != StancePolarity::Rejected
        || contradiction.current.source != StanceSource::SystemDecision
        || contradiction.historical.source != StanceSource::SystemDecision
    {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::EvidenceMismatch);
    }
    if contradiction.current.topic != contradiction.historical.topic {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::TopicMismatch);
    }
    if context.turn_blocked {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::TurnBlocked);
    }
    if !context.provenance_fresh {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::StaleProvenance);
    }
    if !context.replay_consistent {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::ReplayMismatch);
    }
    if !context.audit_ready {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::AuditUnavailable);
    }
    if context.prior_requests_for_topic_in_window > 0 {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::AlreadyRequested);
    }
    if context.session_id.trim().is_empty()
        || context.session_id.chars().count() > 128
        || context.session_id.chars().any(char::is_control)
        || context.window_id.trim().is_empty()
        || context.window_id.chars().count() > 64
        || context.window_id.chars().any(char::is_control)
    {
        return TemporalRecoveryEligibility::Denied(TemporalRecoveryDenialReason::InvalidScope);
    }

    TemporalRecoveryEligibility::Eligible(TemporalRecoveryPermit {
        session_id: context.session_id.clone(),
        topic: contradiction.current.topic.clone(),
        window_id: context.window_id.clone(),
        strategy: decision.strategy,
        result: decision.result,
        issuer_id: issuer_id.clone(),
        key_id: key_id.clone(),
        decision_id_hex: decision_id_hex.clone(),
        signed_payload_fingerprint_hex: signed_payload_fingerprint_hex.clone(),
        max_requests_for_topic_in_window: 1,
        idempotency_key: format!(
            "session:{}:window:{}:topic:{}:decision:{}:request-revision",
            context.session_id,
            context.window_id,
            contradiction.current.topic.as_str(),
            decision_id_hex
        ),
    })
}

fn is_lower_hex_len(value: &str, len: usize) -> bool {
    value.len() == len
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

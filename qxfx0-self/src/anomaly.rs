//! Pure, typed, bounded anomaly recovery contracts.
//!
//! This module detects and records recovery decisions without mutating the
//! renderer, route, persistence layer, or `EssenceState`. A caller may consume
//! the typed decision in a later feature-flagged integration.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

/// Closed set of anomaly kinds admitted by the first recovery contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyKind {
    /// A high-angst self-reference requires an Essence reset proposal.
    SelfReferentialCollapse,
    /// A contradiction between a current and historical stance.
    Temporal,
    /// Evidence could not be assigned to an admitted anomaly kind.
    Unclassifiable,
    /// A confident but inconsistent stance with low conatus.
    AntiConatus,
}

/// Evidence accepted by anomaly detection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnomalyEvidence {
    /// Evidence for a self-referential Essence collapse.
    SelfReference {
        /// Turn where the evidence was observed.
        turn: usize,
        /// Normalized subject considered for self-reference.
        subject: String,
        /// Current Essence angst.
        angst: f64,
        /// Number of witnessed trajectory entries.
        witness_count: usize,
    },
    /// Evidence for an anti-conatus choice.
    AntiConatus {
        /// Turn where the evidence was observed.
        turn: usize,
        /// Confidence in the active stance.
        stance_confidence: f64,
        /// Whether the stance agrees with current evidence.
        stance_consistent: bool,
        /// Current Essence angst.
        angst: f64,
        /// Current conatus scalar.
        conatus: f64,
    },
    /// Evidence for a temporal stance contradiction.
    Temporal {
        /// Turn where the contradiction was observed.
        turn: usize,
        /// Current stance label.
        current_stance: String,
        /// Earlier stance label.
        historical_stance: String,
    },
}

/// Typed strategy selected for an anomaly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyRecoveryStrategy {
    /// Propose an Essence reset through a later state boundary.
    ResetEssence,
    /// Propose a restricted route through a later plan boundary.
    RestrictRoute,
    /// Propose a stance revision through a later plan boundary.
    RequestRevision,
}

/// Typed result expected from a selected strategy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalyRecoveryResult {
    /// An Essence reset was proposed.
    EssenceReset,
    /// A route restriction was proposed.
    RouteRestricted,
    /// A stance revision was proposed.
    RevisionRequested,
}

/// A pure anomaly decision with bounded retry semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnomalyRecoveryDecision {
    /// Detected anomaly kind.
    pub kind: AnomalyKind,
    /// Replay-visible source evidence.
    pub evidence: AnomalyEvidence,
    /// Selected recovery strategy.
    pub strategy: AnomalyRecoveryStrategy,
    /// Expected typed result.
    pub result: AnomalyRecoveryResult,
    /// Stable idempotency key for the observed event.
    pub idempotency_key: String,
    /// Maximum retries permitted by this decision.
    pub max_retries: u8,
}

/// Complete evidence emitted by a recovery ledger without mutating runtime state.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AnomalyRecoveryTrace {
    /// Detected anomaly kind.
    pub kind: AnomalyKind,
    /// Source evidence.
    pub evidence: AnomalyEvidence,
    /// Strategy selected from the typed decision.
    pub strategy: AnomalyRecoveryStrategy,
    /// Expected typed result.
    pub result: AnomalyRecoveryResult,
    /// Idempotency key for replay.
    pub idempotency_key: String,
    /// Caller-supplied digest of the state being observed.
    pub state_digest: String,
}

/// Whether a ledger entry is new or an idempotent replay.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnomalyReplayOutcome {
    /// The decision has not appeared in the bounded ledger before.
    Proposed(AnomalyRecoveryTrace),
    /// The same idempotency key was already recorded; no state transition occurs.
    NoStateTransition(AnomalyRecoveryTrace),
}

/// Bounded, deterministic ledger for anomaly recovery proposals.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AnomalyRecoveryLedger {
    capacity: usize,
    keys: VecDeque<String>,
}

impl AnomalyRecoveryLedger {
    /// Creates a ledger with at least one retained idempotency key.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            keys: VecDeque::new(),
        }
    }

    /// Returns the number of retained idempotency keys.
    pub fn len(&self) -> usize {
        self.keys.len()
    }

    /// Returns whether the ledger has no retained idempotency keys.
    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Records a decision or returns an idempotent no-transition replay outcome.
    pub fn record(
        &mut self,
        decision: AnomalyRecoveryDecision,
        state_digest: impl Into<String>,
    ) -> AnomalyReplayOutcome {
        let trace = AnomalyRecoveryTrace {
            kind: decision.kind,
            evidence: decision.evidence,
            strategy: decision.strategy,
            result: decision.result,
            idempotency_key: decision.idempotency_key.clone(),
            state_digest: state_digest.into(),
        };
        if self.keys.iter().any(|key| key == &decision.idempotency_key) {
            return AnomalyReplayOutcome::NoStateTransition(trace);
        }
        self.keys.push_back(decision.idempotency_key);
        while self.keys.len() > self.capacity {
            self.keys.pop_front();
        }
        AnomalyReplayOutcome::Proposed(trace)
    }
}

impl Default for AnomalyRecoveryLedger {
    fn default() -> Self {
        Self::new(64)
    }
}

/// Detects one admitted anomaly and returns its typed recovery decision.
pub fn detect_anomaly(evidence: AnomalyEvidence) -> Option<AnomalyRecoveryDecision> {
    let (kind, strategy, result, idempotency_key) = match &evidence {
        AnomalyEvidence::SelfReference {
            turn,
            subject,
            angst,
            ..
        } if *angst > 0.9 && is_self_reference(subject) => (
            AnomalyKind::SelfReferentialCollapse,
            AnomalyRecoveryStrategy::ResetEssence,
            AnomalyRecoveryResult::EssenceReset,
            format!("turn:{turn}:self-referential-collapse"),
        ),
        AnomalyEvidence::AntiConatus {
            turn,
            stance_confidence,
            stance_consistent,
            angst,
            conatus,
        } if *stance_confidence >= 0.8
            && !stance_consistent
            && *angst >= 0.9
            && *conatus <= 3.0 =>
        {
            (
                AnomalyKind::AntiConatus,
                AnomalyRecoveryStrategy::RestrictRoute,
                AnomalyRecoveryResult::RouteRestricted,
                format!("turn:{turn}:anti-conatus"),
            )
        }
        AnomalyEvidence::Temporal { turn, .. } => (
            AnomalyKind::Temporal,
            AnomalyRecoveryStrategy::RequestRevision,
            AnomalyRecoveryResult::RevisionRequested,
            format!("turn:{turn}:temporal-contradiction"),
        ),
        _ => return None,
    };
    Some(AnomalyRecoveryDecision {
        kind,
        evidence,
        strategy,
        result,
        idempotency_key,
        max_retries: 0,
    })
}

fn is_self_reference(subject: &str) -> bool {
    matches!(
        subject.trim().to_lowercase().as_str(),
        "я" | "ты" | "qxfx0" | "система"
    )
}

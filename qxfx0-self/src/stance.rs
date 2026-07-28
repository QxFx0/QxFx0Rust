//! Pure, bounded typed stance provenance for temporal anomaly evidence.
//!
//! This module deliberately has no `SystemState`, persistence or pipeline
//! dependency. A later feature-flagged integration may supply only explicit
//! system decisions as observations; free-form history must not be inferred as
//! a stance.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use crate::anomaly::AnomalyEvidence;

/// A normalized topic admitted as a stance subject.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StanceTopic(String);

/// Rejection reasons for a stance topic at the typed boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StanceTopicError {
    Empty,
    TooLong,
    ContainsControl,
}

impl StanceTopic {
    /// Validates an already-normalized topic without changing its spelling.
    pub fn new(value: impl Into<String>) -> Result<Self, StanceTopicError> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err(StanceTopicError::Empty);
        }
        if value.chars().count() > 128 {
            return Err(StanceTopicError::TooLong);
        }
        if value.chars().any(char::is_control) {
            return Err(StanceTopicError::ContainsControl);
        }
        Ok(Self(value))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The typed polarity of a system stance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StancePolarity {
    Affirmed,
    Rejected,
}

impl StancePolarity {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Affirmed => "affirmed",
            Self::Rejected => "rejected",
        }
    }
}

/// Provenance authority for a candidate stance.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StanceSource {
    /// A typed system decision that may participate in temporal comparison.
    SystemDecision,
    /// User text is evidence, not a system stance.
    UserInput,
    /// An imported source is not an adopted system stance.
    ExternalReference,
}

impl StanceSource {
    const fn is_system_decision(self) -> bool {
        matches!(self, Self::SystemDecision)
    }

    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SystemDecision => "system_decision",
            Self::UserInput => "user_input",
            Self::ExternalReference => "external_reference",
        }
    }
}

/// A single replay-visible typed stance observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StanceObservation {
    pub turn: usize,
    pub topic: StanceTopic,
    pub polarity: StancePolarity,
    pub source: StanceSource,
}

impl StanceObservation {
    pub fn idempotency_key(&self) -> String {
        format!(
            "turn:{}:topic:{}:polarity:{}:source:{}",
            self.turn,
            self.topic.as_str(),
            self.polarity.as_str(),
            self.source.as_str()
        )
    }
}

/// The bounded record outcome for a stance observation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StanceRecordOutcome {
    Recorded,
    NoStateTransition,
}

/// Deterministic, bounded store for typed stance observations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedStanceProvenance {
    capacity: usize,
    observations: VecDeque<StanceObservation>,
}

impl BoundedStanceProvenance {
    /// Creates a store with at least one retained observation.
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            observations: VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.observations.len()
    }

    pub fn is_empty(&self) -> bool {
        self.observations.is_empty()
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn observations(&self) -> &VecDeque<StanceObservation> {
        &self.observations
    }

    /// Records a new observation, or reports a duplicate without mutation.
    pub fn record(&mut self, observation: StanceObservation) -> StanceRecordOutcome {
        let key = observation.idempotency_key();
        if self
            .observations
            .iter()
            .any(|existing| existing.idempotency_key() == key)
        {
            return StanceRecordOutcome::NoStateTransition;
        }
        self.observations.push_back(observation);
        while self.observations.len() > self.capacity {
            self.observations.pop_front();
        }
        StanceRecordOutcome::Recorded
    }
}

impl Default for BoundedStanceProvenance {
    fn default() -> Self {
        Self::new(64)
    }
}

/// A typed contradiction between a current and earlier system stance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalStanceContradiction {
    pub current: StanceObservation,
    pub historical: StanceObservation,
}

impl TemporalStanceContradiction {
    /// Converts typed polarity into the legacy-compatible temporal anomaly
    /// labels at the one explicit bridge boundary.
    pub fn to_anomaly_evidence(&self) -> AnomalyEvidence {
        AnomalyEvidence::Temporal {
            turn: self.current.turn,
            current_stance: self.current.polarity.as_str().into(),
            historical_stance: self.historical.polarity.as_str().into(),
        }
    }
}

/// Finds the newest earlier contradictory system decision for `current`.
///
/// User and external observations cannot trigger a temporal anomaly, and an
/// observation never contradicts one from the same or a future turn.
pub fn detect_temporal_contradiction(
    provenance: &BoundedStanceProvenance,
    current: &StanceObservation,
) -> Option<TemporalStanceContradiction> {
    if !current.source.is_system_decision() {
        return None;
    }
    provenance
        .observations()
        .iter()
        .rev()
        .find(|historical| {
            historical.source.is_system_decision()
                && historical.turn < current.turn
                && historical.topic == current.topic
                && historical.polarity != current.polarity
        })
        .cloned()
        .map(|historical| TemporalStanceContradiction {
            current: current.clone(),
            historical,
        })
}

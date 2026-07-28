//! Versioned, bounded typed provenance for persisted system stances.
use crate::anomaly::AnomalyEvidence;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;

pub const STANCE_PROVENANCE_VERSION: u8 = 1;
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct StanceTopic(String);
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StanceTopicError {
    Empty,
    TooLong,
    ContainsControl,
}
impl StanceTopic {
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StanceSource {
    SystemDecision,
    UserInput,
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum StanceRecordOutcome {
    Recorded,
    NoStateTransition,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundedStanceProvenance {
    version: u8,
    capacity: usize,
    observations: VecDeque<StanceObservation>,
}
impl BoundedStanceProvenance {
    pub fn new(capacity: usize) -> Self {
        Self {
            version: STANCE_PROVENANCE_VERSION,
            capacity: capacity.max(1),
            observations: VecDeque::new(),
        }
    }
    pub fn version(&self) -> u8 {
        self.version
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
    pub fn record(&mut self, observation: StanceObservation) -> StanceRecordOutcome {
        let key = observation.idempotency_key();
        if self.observations.iter().any(|x| x.idempotency_key() == key) {
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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TemporalStanceContradiction {
    pub current: StanceObservation,
    pub historical: StanceObservation,
}
impl TemporalStanceContradiction {
    pub fn to_anomaly_evidence(&self) -> AnomalyEvidence {
        AnomalyEvidence::Temporal {
            turn: self.current.turn,
            current_stance: self.current.polarity.as_str().into(),
            historical_stance: self.historical.polarity.as_str().into(),
        }
    }
}
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
        .find(|h| {
            h.source.is_system_decision()
                && h.turn < current.turn
                && h.topic == current.topic
                && h.polarity != current.polarity
        })
        .cloned()
        .map(|historical| TemporalStanceContradiction {
            current: current.clone(),
            historical,
        })
}

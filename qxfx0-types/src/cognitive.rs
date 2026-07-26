//! Typed, replay-friendly inputs and outcomes for bounded metacognition.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoubtDriver {
    Resonance,
    Counterfactual,
    ConatusGate,
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DoubtInput {
    pub confidence: f64,
    pub driver: DoubtDriver,
}

/// A finite value in `[0, 1]` produced by the pure doubt policy.
#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(transparent)]
pub struct DoubtScore(f64);

impl DoubtScore {
    pub fn new(value: f64) -> Self {
        Self(if value.is_finite() {
            value.clamp(0.0, 1.0)
        } else {
            0.0
        })
    }

    pub fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EpisodicKind {
    UserInput,
    SystemDecision,
    Commitment,
    Contradiction,
    Retraction,
    Unresolved,
}

/// Minimal replay-safe episodic fact; raw user or response text is excluded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EpisodicEvent {
    pub id: u64,
    pub turn: u64,
    pub kind: EpisodicKind,
    pub topic: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DoubtRoute {
    RetainCurrent,
    Clarify,
    SuppressedByRecentDecision,
}

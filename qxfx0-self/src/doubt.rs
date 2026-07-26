//! Pure bounded doubt and episodic-recall policy.
//!
//! This module mirrors the Haskell conformance laws without connecting them to
//! the production route or persistence path. Its inputs and results are typed,
//! deterministic and replayable.

use std::collections::VecDeque;

use qxfx0_types::{DoubtDriver, DoubtInput, DoubtRoute, DoubtScore, EpisodicEvent, EpisodicKind};

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
pub struct EpisodicConfig {
    pub capacity: usize,
    pub recall_window_turns: u64,
    pub recall_limit: usize,
}

impl Default for EpisodicConfig {
    fn default() -> Self {
        Self {
            capacity: 64,
            recall_window_turns: 50,
            recall_limit: 8,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, serde::Deserialize)]
pub struct DoubtPolicy {
    pub clarification_threshold: f64,
}

impl Default for DoubtPolicy {
    fn default() -> Self {
        Self {
            clarification_threshold: 0.75,
        }
    }
}

/// Bounded append-only event buffer. Presentation receives selected events or
/// a route outcome, never a mutable store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedEpisodicStore {
    config: EpisodicConfig,
    events: VecDeque<EpisodicEvent>,
}

impl BoundedEpisodicStore {
    pub fn new(config: EpisodicConfig) -> Self {
        Self {
            config,
            events: VecDeque::new(),
        }
    }

    pub fn record(mut self, event: EpisodicEvent) -> Self {
        if self.config.capacity == 0 {
            return self;
        }
        self.events.push_back(event);
        while self.events.len() > self.config.capacity {
            self.events.pop_front();
        }
        self
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Newest-first, bounded recall for exactly the requested topic.
    pub fn recall(&self, current_turn: u64, topic: Option<&str>) -> Vec<EpisodicEvent> {
        self.events
            .iter()
            .rev()
            .filter(|event| is_recent(event, current_turn, self.config.recall_window_turns))
            .filter(|event| same_topic(event.topic.as_deref(), topic))
            .take(self.config.recall_limit)
            .cloned()
            .collect()
    }
}

impl Default for BoundedEpisodicStore {
    fn default() -> Self {
        Self::new(EpisodicConfig::default())
    }
}

/// Haskell parity: complement confidence, counterfactual ambiguity adds 0.2,
/// and a conatus gate is a structural high-doubt floor of 0.9.
pub fn compute_doubt(input: DoubtInput) -> DoubtScore {
    let confidence = clamp01(input.confidence);
    let base = 1.0 - confidence;
    let value = match input.driver {
        DoubtDriver::ConatusGate => base.max(0.9),
        DoubtDriver::Counterfactual => (base + 0.2).min(1.0),
        DoubtDriver::Resonance | DoubtDriver::Other => base,
    };
    DoubtScore::new(value)
}

/// Determines the allowable doubt response without changing a production
/// route. A recent same-topic system decision prevents re-asking.
pub fn route_for_doubt(
    score: DoubtScore,
    policy: DoubtPolicy,
    recalled: &[EpisodicEvent],
) -> DoubtRoute {
    if score.value() < clamp01(policy.clarification_threshold) {
        DoubtRoute::RetainCurrent
    } else if recalled
        .iter()
        .any(|event| event.kind == EpisodicKind::SystemDecision)
    {
        DoubtRoute::SuppressedByRecentDecision
    } else {
        DoubtRoute::Clarify
    }
}

fn clamp01(value: f64) -> f64 {
    if value.is_finite() {
        value.clamp(0.0, 1.0)
    } else {
        0.0
    }
}

fn is_recent(event: &EpisodicEvent, current_turn: u64, window: u64) -> bool {
    event.turn <= current_turn && current_turn.saturating_sub(event.turn) <= window
}

fn same_topic(event_topic: Option<&str>, query_topic: Option<&str>) -> bool {
    event_topic == query_topic
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(id: u64, turn: u64, kind: EpisodicKind, topic: &str) -> EpisodicEvent {
        EpisodicEvent {
            id,
            turn,
            kind,
            topic: Some(topic.into()),
        }
    }

    #[test]
    fn hsm_reference_doubt_is_complement_of_confidence() {
        let low = compute_doubt(DoubtInput {
            confidence: 0.1,
            driver: DoubtDriver::Resonance,
        });
        let high = compute_doubt(DoubtInput {
            confidence: 0.95,
            driver: DoubtDriver::Resonance,
        });
        assert!(low.value() >= 0.85);
        assert!(high.value() <= 0.15);
        assert!(low.value() > high.value());
    }

    #[test]
    fn hsm_reference_conatus_and_ambiguity_amplify_doubt() {
        let conatus = compute_doubt(DoubtInput {
            confidence: 0.8,
            driver: DoubtDriver::ConatusGate,
        });
        let base = compute_doubt(DoubtInput {
            confidence: 0.5,
            driver: DoubtDriver::Resonance,
        });
        let ambiguity = compute_doubt(DoubtInput {
            confidence: 0.5,
            driver: DoubtDriver::Counterfactual,
        });
        assert!(conatus.value() >= 0.9);
        assert!(ambiguity.value() > base.value());
    }

    #[test]
    fn recent_same_topic_decision_suppresses_clarification() {
        let store = BoundedEpisodicStore::default()
            .record(event(1, 5, EpisodicKind::UserInput, "freedom"))
            .record(event(2, 10, EpisodicKind::SystemDecision, "freedom"));
        let recalled = store.recall(12, Some("freedom"));
        assert_eq!(
            route_for_doubt(DoubtScore::new(0.9), DoubtPolicy::default(), &recalled),
            DoubtRoute::SuppressedByRecentDecision
        );
        assert_eq!(
            route_for_doubt(
                DoubtScore::new(0.9),
                DoubtPolicy::default(),
                &store.recall(12, Some("memory"))
            ),
            DoubtRoute::Clarify
        );
    }

    #[test]
    fn recall_is_bounded_by_age_capacity_and_limit() {
        let mut store = BoundedEpisodicStore::new(EpisodicConfig {
            capacity: 3,
            recall_window_turns: 5,
            recall_limit: 2,
        });
        for turn in 1..=4 {
            store = store.record(event(turn, turn, EpisodicKind::UserInput, "freedom"));
        }
        assert_eq!(store.len(), 3);
        let recalled = store.recall(8, Some("freedom"));
        assert_eq!(recalled.len(), 2);
        assert_eq!(recalled[0].turn, 4);
    }
}

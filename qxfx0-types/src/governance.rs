use crate::system_state::GuardStatus;
use crate::CanonicalMoveFamily;
use serde::{Deserialize, Serialize};

/// Governance event — append-only history entry.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GovernanceEvent {
    pub turn: usize,
    pub event_type: GovernanceEventType,
    pub family: CanonicalMoveFamily,
    pub guard_status: GuardStatus,
    pub timestamp: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GovernanceEventType {
    TurnCompleted,
    GuardBlocked,
    GuardWarning,
    CommitmentRevised,
    CommitmentContradicted,
    GraphEnriched { new_relations: usize },
}

/// Governance log — append-only history of governance events.
/// Deterministic: events are stored in order, never modified.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GovernanceLog {
    #[serde(default)]
    pub events: Vec<GovernanceEvent>,
}

impl GovernanceLog {
    pub fn new() -> Self {
        Self::default()
    }

    /// Append an event (immutable — never modify existing events).
    pub fn append(&mut self, event: GovernanceEvent) {
        self.events.push(event);
    }

    /// Trim old events, keeping only the most recent `cap`.
    /// Mirrors the dialogue history cap to prevent unbounded SystemState growth.
    pub fn trim(&mut self, cap: usize) {
        if self.events.len() > cap {
            let excess = self.events.len() - cap;
            self.events.drain(0..excess);
        }
    }

    /// Get the last N events.
    pub fn recent(&self, n: usize) -> &[GovernanceEvent] {
        let start = self.events.len().saturating_sub(n);
        &self.events[start..]
    }

    /// Count events by type.
    ///
    /// **Discriminant comparison only**: payload data is ignored. For variants
    /// that carry data (e.g. `GraphEnriched { new_relations: usize }`), any
    /// event of that variant matches regardless of payload. Callers asking
    /// "how many enrichments added exactly 5 relations?" will get the total
    /// count of `GraphEnriched` events. Use a manual `iter().filter(...)` if
    /// you need exact-match semantics on payload fields.
    pub fn count_by_type(&self, event_type: &GovernanceEventType) -> usize {
        self.events
            .iter()
            .filter(|e| std::mem::discriminant(&e.event_type) == std::mem::discriminant(event_type))
            .count()
    }

    /// Total event count.
    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Check if any turn was blocked by guard.
    pub fn has_blocks(&self) -> bool {
        self.events
            .iter()
            .any(|e| matches!(e.event_type, GovernanceEventType::GuardBlocked))
    }

    /// Replay gate — verify that the event log is consistent.
    /// Returns violations (empty = ok).
    pub fn replay_check(&self) -> Vec<String> {
        let mut violations = Vec::new();

        for (i, event) in self.events.iter().enumerate() {
            // Turns should be monotonically non-decreasing
            if i > 0 && event.turn < self.events[i - 1].turn {
                violations.push(format!(
                    "turn regression at event {}: {} < {}",
                    i,
                    event.turn,
                    self.events[i - 1].turn
                ));
            }

            // GuardBlocked should have InvariantBlock status
            if matches!(event.event_type, GovernanceEventType::GuardBlocked)
                && !matches!(event.guard_status, GuardStatus::InvariantBlock(_))
            {
                violations.push(format!("GuardBlocked event {} has non-block status", i));
            }
        }

        violations
    }
}

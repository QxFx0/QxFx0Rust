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
    /// The bounded commitment store rejected a new observation because it is
    /// full. Recorded instead of silently dropping the commitment; the store
    /// is deliberately not evicted — commitments are semantic positions and
    /// silent eviction would corrupt lineage guarantees.
    CommitmentCapacityReached,
    GraphEnriched {
        new_relations: usize,
    },
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

            // GuardBlocked should have a blocking status (InvariantBlock or Blocked)
            if matches!(event.event_type, GovernanceEventType::GuardBlocked)
                && !matches!(
                    event.guard_status,
                    GuardStatus::InvariantBlock(_) | GuardStatus::Blocked(_)
                )
            {
                violations.push(format!("GuardBlocked event {} has non-block status", i));
            }
        }

        violations
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_event(turn: usize, etype: GovernanceEventType) -> GovernanceEvent {
        GovernanceEvent {
            turn,
            event_type: etype,
            family: CanonicalMoveFamily::CMDefine,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "2026-01-01T00:00:00Z".into(),
        }
    }

    #[test]
    fn test_append_and_retrieve() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted));

        assert_eq!(log.len(), 2);
        assert_eq!(log.recent(1).len(), 1);
        assert_eq!(log.recent(1)[0].turn, 2);
    }

    #[test]
    fn test_replay_check_ok() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted));
        log.append(make_event(3, GovernanceEventType::TurnCompleted));

        assert!(log.replay_check().is_empty());
    }

    #[test]
    fn test_replay_check_turn_regression() {
        let mut log = GovernanceLog::new();
        log.append(make_event(3, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted)); // regression

        let violations = log.replay_check();
        assert!(!violations.is_empty());
        assert!(violations[0].contains("regression"));
    }

    #[test]
    fn test_replay_check_guard_blocked_status() {
        let mut log = GovernanceLog::new();
        log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::GuardBlocked,
            family: CanonicalMoveFamily::CMDefine,
            guard_status: GuardStatus::InvariantOk, // wrong! should be Block
            timestamp: "2026-01-01T00:00:00Z".into(),
        });

        let violations = log.replay_check();
        assert!(!violations.is_empty());
        assert!(violations[0].contains("non-block status"));
    }

    #[test]
    fn test_replay_check_guard_blocked_with_blocked_status_ok() {
        let mut log = GovernanceLog::new();
        log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::GuardBlocked,
            family: CanonicalMoveFamily::CMDefine,
            guard_status: GuardStatus::Blocked("content quality".into()),
            timestamp: "2026-01-01T00:00:00Z".into(),
        });

        let violations = log.replay_check();
        assert!(
            violations.is_empty(),
            "Blocked status should pass replay check"
        );
    }

    #[test]
    fn test_has_blocks() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        assert!(!log.has_blocks());

        log.append(GovernanceEvent {
            turn: 2,
            event_type: GovernanceEventType::GuardBlocked,
            family: CanonicalMoveFamily::CMRepair,
            guard_status: GuardStatus::InvariantBlock("test".into()),
            timestamp: "2026-01-01T00:00:00Z".into(),
        });
        assert!(log.has_blocks());
    }

    #[test]
    fn test_count_by_type() {
        let mut log = GovernanceLog::new();
        log.append(make_event(1, GovernanceEventType::TurnCompleted));
        log.append(make_event(2, GovernanceEventType::TurnCompleted));
        log.append(make_event(
            3,
            GovernanceEventType::GraphEnriched { new_relations: 2 },
        ));

        assert_eq!(log.count_by_type(&GovernanceEventType::TurnCompleted), 2);
        assert_eq!(
            log.count_by_type(&GovernanceEventType::GraphEnriched { new_relations: 0 }),
            1
        );
    }
}

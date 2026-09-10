//! Quarantine — evidence the bridge must not let become graph state
//! (ADR-0043 U4). A candidate event is quarantined, never silently
//! discarded: the operator's U5 review queue is exactly this list, and
//! law 2 (fail-closed) means a rejected event has to be visible, bounded
//! and attributable.

use serde::{Deserialize, Serialize};

use qxfx0_types::AtomId;

use crate::corroboration::CorroborationEvent;

/// Why an event was refused entry to the runtime store.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum QuarantineReason {
    /// An endpoint atom is not in the session graph: a candidate may
    /// corroborate an existing relation, never mint atoms.
    UnknownEndpoint { atom: AtomId },
    /// An empty identity or a self-loop: structurally meaningless.
    Degenerate { detail: String },
    /// The queue was full when the event arrived (back-pressure drop).
    QueueOverflow,
    /// The session is already at its quarantine bound; the newest
    /// evidence is refused rather than evicting older review material.
    QuarantineFull,
}

/// A quarantined event plus its reason and the turn boundary at which it
/// was refused. Serialized into the quarantine table verbatim (U5 reads
/// it back for the operator review queue).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QuarantinedEvent {
    pub event: CorroborationEvent,
    pub reason: QuarantineReason,
    pub turn: u64,
}

/// The default quarantine bound per session. Bounded like every other
/// persisted structure in this system; not calibrated.
pub const DEFAULT_QUARANTINE_CAPACITY: usize = 1_024;

/// An insertion-ordered, bounded quarantine. `push` is total and reports
/// acceptance, so a caller overflowing the bound observes a
/// `QuarantineFull`-style back-pressure rather than silent loss.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuarantineLedger {
    capacity: usize,
    entries: std::collections::VecDeque<QuarantinedEvent>,
}

impl Default for QuarantineLedger {
    fn default() -> Self {
        Self::new(DEFAULT_QUARANTINE_CAPACITY)
    }
}

impl QuarantineLedger {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            entries: std::collections::VecDeque::new(),
        }
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entries(&self) -> &std::collections::VecDeque<QuarantinedEvent> {
        &self.entries
    }

    /// Append an entry; `false` means the ledger is full and the entry was
    /// refused (the caller may re-queue it as `QuarantineFull`).
    pub fn push(&mut self, entry: QuarantinedEvent) -> bool {
        if self.capacity == 0 || self.entries.len() >= self.capacity {
            return false;
        }
        self.entries.push_back(entry);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_edges::BridgeOutcome;
    use qxfx0_types::RelationType;

    fn event() -> CorroborationEvent {
        CorroborationEvent::new(
            AtomId::new("a"),
            AtomId::new("b"),
            RelationType::RelRelatedTo,
            BridgeOutcome::Positive,
        )
    }

    fn entry(atom: &str) -> QuarantinedEvent {
        QuarantinedEvent {
            event: event(),
            reason: QuarantineReason::UnknownEndpoint {
                atom: AtomId::new(atom),
            },
            turn: 1,
        }
    }

    #[test]
    fn ledger_is_bounded_and_reports_back_pressure() {
        let mut ledger = QuarantineLedger::new(2);
        assert!(ledger.push(entry("x")));
        assert!(ledger.push(entry("y")));
        assert!(!ledger.push(entry("z")));
        assert_eq!(ledger.len(), 2);
        // Insertion order preserved for the review queue.
        assert_eq!(ledger.entries()[0].event, event());
    }

    #[test]
    fn zero_capacity_refuses_everything() {
        let mut ledger = QuarantineLedger::new(0);
        assert!(!ledger.push(entry("x")));
        assert!(ledger.is_empty());
    }
}

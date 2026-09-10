//! Bounded corroboration queue — the between-turn feed that carries
//! associative evidence into the runtime bridge (ADR-0043 U4).
//!
//! The queue is deliberately a pure data structure: a worker drains it
//! between turns (never on the turn path) and folds each event through
//! [`crate::runtime_edges::apply_corroboration_event`]. It is bounded so a
//! chatty or hostile source cannot grow unreasoned memory, and it preserves
//! insertion order for deterministic replay. Events carry only ids, a
//! relation type and an outcome — never surface text (law 1: associative
//! evidence only, never rendered).

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};

use qxfx0_types::{AtomId, RelationType};

use crate::runtime_edges::{
    apply_corroboration_event, BridgeOutcome, Corroboration, RuntimeEdgeStore,
};

/// One queued corroboration request. No text, no timestamp — the clock and
/// any provenance metadata live in the (out-of-tree) worker, not here, so
/// the queue stays a deterministic replay artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CorroborationEvent {
    pub from: AtomId,
    pub to: AtomId,
    pub rel_type: RelationType,
    pub outcome: BridgeOutcome,
}

impl CorroborationEvent {
    pub fn new(
        from: impl Into<AtomId>,
        to: impl Into<AtomId>,
        rel_type: RelationType,
        outcome: BridgeOutcome,
    ) -> Self {
        Self {
            from: from.into(),
            to: to.into(),
            rel_type,
            outcome,
        }
    }
}

/// The default queue bound. Chosen to hold many sessions' worth of a modest
/// between-turn batch; not calibrated (ADR-0043 — calibration waits for a
/// trace corpus).
pub const DEFAULT_QUEUE_CAPACITY: usize = 1_024;

/// A FIFO queue bounded at `capacity`. `push` returns whether the event was
/// accepted or dropped for capacity, so the worker can meter back-pressure
/// without the queue silently swallowing evidence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoundedCorroborationQueue {
    capacity: usize,
    events: VecDeque<CorroborationEvent>,
}

impl Default for BoundedCorroborationQueue {
    fn default() -> Self {
        Self::new(DEFAULT_QUEUE_CAPACITY)
    }
}

impl BoundedCorroborationQueue {
    pub fn new(capacity: usize) -> Self {
        Self {
            capacity,
            events: VecDeque::new(),
        }
    }

    pub fn capacity(&self) -> usize {
        self.capacity
    }

    pub fn len(&self) -> usize {
        self.events.len()
    }

    pub fn is_empty(&self) -> bool {
        self.events.is_empty()
    }

    /// Append an event; `false` means it was dropped because the queue is
    /// full (a zero capacity always drops).
    pub fn push(&mut self, event: CorroborationEvent) -> bool {
        if self.capacity == 0 || self.events.len() >= self.capacity {
            return false;
        }
        self.events.push_back(event);
        true
    }

    /// Drain the queue in insertion order and fold each event into the
    /// runtime store. Returns the new store and the per-event traces; a
    /// no-op event (absent key, or an off-store edge for a conflict) still
    /// reports its [`Corroboration`]. The store keys not yet present are
    /// *created* only by positive evidence reaching the threshold through
    /// prior reinforcement — a first sighting of an unseen pair records the
    /// edge at `co_occurrence = 1`, mirroring the graph-activation path.
    pub fn drain_into(
        self,
        mut store: RuntimeEdgeStore,
    ) -> (RuntimeEdgeStore, Vec<(CorroborationEvent, Corroboration)>) {
        let mut traces = Vec::with_capacity(self.events.len());
        for event in self.events {
            ensure_edge(&mut store, &event);
            let (next, trace) =
                apply_corroboration_event(store, &event.from, &event.to, event.outcome);
            store = next;
            traces.push((event, trace));
        }
        (store, traces)
    }
}

/// Make sure the pair exists before reinforcement so a first positive
/// sighting creates the working edge rather than no-op'ing. Negative and
/// conflict evidence on an unseen pair must not fabricate an edge, so
/// `ensure_edge` only inserts for `Positive`.
fn ensure_edge(store: &mut RuntimeEdgeStore, event: &CorroborationEvent) {
    if event.outcome != BridgeOutcome::Positive {
        return;
    }
    let key = (event.from.clone(), event.to.clone());
    if store.contains_key(&key) {
        return;
    }
    store.insert(
        key,
        crate::runtime_edges::BridgeEdge {
            from: event.from.clone(),
            to: event.to.clone(),
            rel_type: event.rel_type,
            topic: event.from.as_str().to_string(),
            confidence: 0.0,
            co_occurrence: 0,
            weight: 0.0,
            source: crate::runtime_edges::BridgeEdgeSource::RuntimeBridge,
        },
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_edges::BridgeEdgeSource;
    use qxfx0_types::RelationType;

    fn positive(from: &str, to: &str) -> CorroborationEvent {
        CorroborationEvent::new(
            AtomId::new(from),
            AtomId::new(to),
            RelationType::RelRelatedTo,
            BridgeOutcome::Positive,
        )
    }

    #[test]
    fn push_is_bounded_and_reports_back_pressure() {
        let mut queue = BoundedCorroborationQueue::new(2);
        assert!(queue.push(positive("a", "b")));
        assert!(queue.push(positive("b", "c")));
        assert!(!queue.push(positive("c", "d")));
        assert_eq!(queue.len(), 2);
        // A zero-capacity queue drops everything.
        let mut full = BoundedCorroborationQueue::new(0);
        assert!(!full.push(positive("a", "b")));
    }

    #[test]
    fn drain_preserves_order_and_creates_on_first_positive() {
        let mut queue = BoundedCorroborationQueue::default();
        queue.push(positive("свобода", "выбор"));
        queue.push(positive("разум", "мысль"));
        let (store, traces) = queue.drain_into(RuntimeEdgeStore::new());
        assert_eq!(traces.len(), 2);
        assert_eq!(store.len(), 2);
        // First positive sighting: created at 0 then reinforced to 0.05,
        // co_occurrence 1.
        let created = store
            .get(&(AtomId::new("свобода"), AtomId::new("выбор")))
            .expect("created");
        assert!((created.confidence - 0.05).abs() < 1e-9);
        assert_eq!(created.co_occurrence, 1);
        assert_eq!(created.source, BridgeEdgeSource::RuntimeBridge);
    }

    #[test]
    fn negative_on_unseen_pair_does_not_fabricate_an_edge() {
        let mut queue = BoundedCorroborationQueue::default();
        queue.push(CorroborationEvent::new(
            AtomId::new("a"),
            AtomId::new("b"),
            RelationType::RelRelatedTo,
            BridgeOutcome::Negative,
        ));
        let (store, traces) = queue.drain_into(RuntimeEdgeStore::new());
        assert!(store.is_empty());
        assert!(!traces[0].1.applied);
    }

    #[test]
    fn repeated_positive_drains_to_promotion() {
        use crate::runtime_edges::BridgeEdge;
        let mut store = RuntimeEdgeStore::new();
        store.insert(
            (AtomId::new("свобода"), AtomId::new("выбор")),
            BridgeEdge::new(
                AtomId::new("свобода"),
                AtomId::new("выбор"),
                RelationType::RelRelatedTo,
                "свобода",
                0.6,
            ),
        );
        for _ in 0..3 {
            let mut queue = BoundedCorroborationQueue::default();
            queue.push(positive("свобода", "выбор"));
            let (next, _) = queue.drain_into(store);
            store = next;
        }
        let promoted = store
            .get(&(AtomId::new("свобода"), AtomId::new("выбор")))
            .expect("edge survives");
        assert_eq!(promoted.source, BridgeEdgeSource::Promoted);
    }
}

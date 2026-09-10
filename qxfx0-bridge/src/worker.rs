//! The between-turn worker — the cycle that consumes the corroboration
//! queue and maintains the runtime store (ADR-0043 U4).
//!
//! This is the whole cadence, expressed as one pure function:
//!
//! ```text
//! process_turn_boundary(store, queue, turn, topic, known_atoms, config)
//!   -> (new_store, report, quarantine_entries)
//! ```
//!
//! The cycle is three passes over bounded data. Admission first: every
//! queued event is checked against the session graph, and anything that
//! could mint atoms or corrupt a relation is quarantined with a named
//! reason — never silently dropped. Then the fold: admitted events go
//! through the reinforce/retire ladder. Then the boundary decay: unused
//! runtime edges fade, the store prunes to its cap.
//!
//! No clock, no socket, no persistence in here — the caller (an operator
//! scheduler or a CLI maintenance subcommand) runs it between turns,
//! and the persistence layer stores the outcome. On the turn path the
//! bridge sleeps: nothing in the pipeline imports this module.

use std::collections::BTreeSet;

use qxfx0_types::AtomId;

use crate::corroboration::{BoundedCorroborationQueue, CorroborationEvent};
use crate::quarantine::{QuarantineReason, QuarantinedEvent};
use crate::runtime_edges::{
    apply_corroboration_event, apply_edge_decay_and_retire, BridgeEdge, BridgeOutcome,
    Corroboration, DecayConfig, RuntimeEdgeStore,
};

/// Per-event results of one worker cycle, in queue order.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkerReport {
    /// The turn boundary this cycle ran at.
    pub turn: u64,
    /// Events admitted to the ladder, in queue order.
    pub admitted: Vec<(CorroborationEvent, Corroboration)>,
    /// Events refused and quarantined, in queue order.
    pub quarantined: Vec<QuarantinedEvent>,
    /// Runtime-bridge edge count after the decay/prune pass.
    pub runtime_edges_after: usize,
}

/// Validate an event against the session graph and its own shape. The
/// bridge may corroborate relations between atoms the session already
/// knows (the subject plus everything in the runtime graph); it may
/// never mint atoms, self-loop or carry an empty identity.
pub fn admission_reason(
    event: &CorroborationEvent,
    known_atoms: &BTreeSet<AtomId>,
) -> Option<QuarantineReason> {
    for atom in [&event.from, &event.to] {
        if atom.as_str().trim().is_empty() {
            return Some(QuarantineReason::Degenerate {
                detail: "empty atom identity".into(),
            });
        }
        if !known_atoms.contains(atom) {
            return Some(QuarantineReason::UnknownEndpoint { atom: atom.clone() });
        }
    }
    if event.from == event.to {
        return Some(QuarantineReason::Degenerate {
            detail: "self-loop".into(),
        });
    }
    None
}

/// One full between-turn cycle: admit (quarantining refusals), fold the
/// ladder over admitted events, then decay and prune the store. Pure;
/// identical inputs produce identical outputs and ordering.
pub fn process_turn_boundary(
    store: RuntimeEdgeStore,
    queue: BoundedCorroborationQueue,
    turn: u64,
    topic: &str,
    known_atoms: &BTreeSet<AtomId>,
    config: &DecayConfig,
) -> (RuntimeEdgeStore, WorkerReport, Vec<QuarantinedEvent>) {
    let mut admitted_store = store;
    let mut admitted = Vec::new();
    let mut quarantined = Vec::new();

    let events: Vec<CorroborationEvent> = queue.into_events();
    for event in events {
        if let Some(reason) = admission_reason(&event, known_atoms) {
            quarantined.push(QuarantinedEvent {
                event,
                reason,
                turn,
            });
            continue;
        }
        // Only positive evidence may create the working edge; negative
        // and conflict evidence on unseen pairs is a no-op in the fold.
        if event.outcome == BridgeOutcome::Positive {
            ensure_runtime_edge(&mut admitted_store, &event);
        }
        let (next, trace) =
            apply_corroboration_event(admitted_store, &event.from, &event.to, event.outcome);
        admitted_store = next;
        admitted.push((event, trace));
    }

    let final_store = apply_edge_decay_and_retire(admitted_store, topic, config);
    let report = WorkerReport {
        turn,
        admitted,
        quarantined: quarantined.clone(),
        runtime_edges_after: crate::runtime_edges::runtime_edge_count(&final_store),
    };
    (final_store, report, quarantined)
}

fn ensure_runtime_edge(store: &mut RuntimeEdgeStore, event: &CorroborationEvent) {
    let key = (event.from.clone(), event.to.clone());
    if store.contains_key(&key) {
        return;
    }
    store.insert(
        key,
        BridgeEdge {
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
    use crate::corroboration::BoundedCorroborationQueue;
    use crate::runtime_edges::BridgeEdgeSource;
    use qxfx0_types::RelationType;

    fn atoms(names: &[&str]) -> BTreeSet<AtomId> {
        names.iter().map(|name| AtomId::new(*name)).collect()
    }

    fn event(from: &str, to: &str, outcome: BridgeOutcome) -> CorroborationEvent {
        CorroborationEvent::new(
            AtomId::new(from),
            AtomId::new(to),
            RelationType::RelRelatedTo,
            outcome,
        )
    }

    #[test]
    fn admitted_events_fold_and_unknown_endpoints_quarantine() {
        let mut queue = BoundedCorroborationQueue::default();
        queue.push(event("свобода", "выбор", BridgeOutcome::Positive));
        queue.push(event("свобода", "химера", BridgeOutcome::Positive));
        let known = atoms(&["свобода", "выбор"]);
        let (store, report, quarantined) = process_turn_boundary(
            RuntimeEdgeStore::new(),
            queue,
            1,
            "свобода",
            &known,
            &DecayConfig::default(),
        );
        assert_eq!(report.admitted.len(), 1);
        assert_eq!(quarantined.len(), 1);
        assert_eq!(
            quarantined[0].reason,
            QuarantineReason::UnknownEndpoint {
                atom: AtomId::new("химера")
            }
        );
        assert_eq!(report.turn, 1);
        assert!(store.contains_key(&(AtomId::new("свобода"), AtomId::new("выбор"))));
    }

    #[test]
    fn self_loops_and_empty_atoms_are_degenerate() {
        let known = atoms(&["a", ""]);
        assert!(matches!(
            admission_reason(&event("a", "a", BridgeOutcome::Positive), &known),
            Some(QuarantineReason::Degenerate { .. })
        ));
        assert!(matches!(
            admission_reason(&event("", "a", BridgeOutcome::Positive), &known),
            Some(QuarantineReason::Degenerate { .. })
        ));
    }

    #[test]
    fn boundary_decay_retires_stale_edges_but_spares_the_topic() {
        let mut store = RuntimeEdgeStore::new();
        store.insert(
            (AtomId::new("свобода"), AtomId::new("воля")),
            BridgeEdge::new(
                AtomId::new("свобода"),
                AtomId::new("воля"),
                RelationType::RelRelatedTo,
                "свобода",
                0.31,
            ),
        );
        store.insert(
            (AtomId::new("разум"), AtomId::new("мысль")),
            BridgeEdge::new(
                AtomId::new("разум"),
                AtomId::new("мысль"),
                RelationType::RelRelatedTo,
                "разум",
                0.31,
            ),
        );
        let queue = BoundedCorroborationQueue::default();
        let known = atoms(&["свобода", "воля", "разум", "мысль"]);
        let (store, report, _) =
            process_turn_boundary(store, queue, 2, "свобода", &known, &DecayConfig::default());
        assert!(store.contains_key(&(AtomId::new("свобода"), AtomId::new("воля"))));
        assert!(!store.contains_key(&(AtomId::new("разум"), AtomId::new("мысль"))));
        assert_eq!(report.runtime_edges_after, 1);
    }

    #[test]
    fn empty_cycle_is_an_identity_on_the_store() {
        let mut store = RuntimeEdgeStore::new();
        store.insert(
            (AtomId::new("a"), AtomId::new("b")),
            BridgeEdge::new(
                AtomId::new("a"),
                AtomId::new("b"),
                RelationType::RelRelatedTo,
                "topic",
                0.9,
            ),
        );
        let known = atoms(&["a", "b"]);
        let queue = BoundedCorroborationQueue::default();
        let (next, report, quarantined) = process_turn_boundary(
            store.clone(),
            queue,
            3,
            "topic",
            &known,
            &DecayConfig {
                decay_rate: 1.0,
                retire_threshold: 0.0,
                max_edges: 500,
            },
        );
        assert_eq!(next, store);
        assert!(report.admitted.is_empty() && quarantined.is_empty());
    }

    #[test]
    fn repeated_positive_cycles_climb_to_promotion() {
        let known = atoms(&["свобода", "выбор"]);
        let mut store = RuntimeEdgeStore::new();
        // From a cold edge, +0.05 per cycle reaches the 0.75 promotion
        // threshold after 15 reinforcements; the topic-touching edge is
        // spared by the decay pass on every boundary.
        for turn in 0..16u64 {
            let mut queue = BoundedCorroborationQueue::default();
            queue.push(event("свобода", "выбор", BridgeOutcome::Positive));
            let (next, _, _) = process_turn_boundary(
                store,
                queue,
                turn,
                "свобода",
                &known,
                &DecayConfig::default(),
            );
            store = next;
        }
        let edge = store
            .get(&(AtomId::new("свобода"), AtomId::new("выбор")))
            .expect("edge survives");
        assert_eq!(edge.source, BridgeEdgeSource::Promoted);
    }
}

//! The graph's own counter-arguments: typed opposing edges of a topic.
//!
//! Relation types carry polarity, so an edge can honestly *oppose* a topic
//! («государство ограничивает свободу») without any invented semantics. This
//! module is the single deterministic selection shared by every surface
//! that challenges a held position — the reflection card and the turn
//! response — so they draw from the same sorted edge list and the same
//! salt-indexed rotation, never from randomness.

use crate::seed_graph;
use qxfx0_types::atom::AtomId;
use qxfx0_types::atom::Relation;
use qxfx0_types::relation_type::RelationType;

/// Relation types that encode tension against the topic — the graph's
/// counter-arguments.
const CHALLENGE_RELATIONS: &[RelationType] = &[
    RelationType::RelContrastsWith,
    RelationType::RelDestroys,
    RelationType::RelLimitedBy,
    RelationType::RelNegates,
    RelationType::RelDiffersFrom,
    RelationType::RelIsNot,
    RelationType::RelNotReducibleTo,
];

/// All opposing edges of a topic, both directions, cloned out of the seed
/// graph and put in a canonical order (from, type, to, sentence).
/// Deterministic: the same topic always yields the same sequence.
pub fn opposing_edges(topic: &str) -> Vec<Relation> {
    let graph = seed_graph();
    let atom = AtomId::new(topic);
    let mut edges: Vec<Relation> = graph
        .relations_from(&atom)
        .into_iter()
        .chain(graph.relations_to(&atom))
        .filter(|relation| CHALLENGE_RELATIONS.contains(&relation.rel_type))
        .cloned()
        .collect();
    edges.sort_by_key(|relation| {
        (
            relation.from.as_str().to_string(),
            relation.rel_type,
            relation.to.as_str().to_string(),
            relation.ru_original.clone(),
        )
    });
    edges
}

/// The graph's challenge sentence for a held position: the topic's opposing
/// edges indexed by `salt`. The caller owns the salt (the reflection card
/// salts with the practice day and the statement bytes; the turn response
/// salts with the statement bytes and the turn) — selection is deterministic
/// for a given salt and rotates across different ones. `None` when the
/// graph carries no opposing edge for the topic; callers must then honestly
/// fall back to corpus material.
pub fn opposing_challenge_sentence(topic: &str, salt: u64) -> Option<String> {
    let edges = opposing_edges(topic);
    if edges.is_empty() {
        return None;
    }
    let index = (salt % edges.len() as u64) as usize;
    Some(edges[index].ru_original.clone())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn freedom_carries_opposing_edges_in_canonical_order() {
        let edges = opposing_edges("свобода");
        assert!(edges.len() >= 2, "«свобода» has curated opposing edges");
        let keys: Vec<(String, RelationType, String)> = edges
            .iter()
            .map(|edge| {
                (
                    edge.from.as_str().to_string(),
                    edge.rel_type,
                    edge.to.as_str().to_string(),
                )
            })
            .collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "edges come back in canonical order");
        assert_eq!(
            opposing_edges("свобода"),
            edges,
            "the canonical order is stable across calls"
        );
        assert!(
            edges.iter().all(|edge| edge.ru_original.contains("свобод")),
            "every opposing edge speaks about the topic"
        );
    }

    #[test]
    fn selection_is_deterministic_and_rotates_with_the_salt() {
        let first = opposing_challenge_sentence("свобода", 0).expect("edges exist");
        assert_eq!(
            opposing_challenge_sentence("свобода", 0),
            Some(first.clone())
        );
        // Adjacent salts land on adjacent edges — guaranteed different while
        // the topic has at least two opposing edges.
        let second = opposing_challenge_sentence("свобода", 1).expect("edges exist");
        assert_ne!(first, second);
    }

    #[test]
    fn unknown_topic_honestly_has_no_challenge() {
        assert!(opposing_challenge_sentence("фырковистость", 0).is_none());
    }
}

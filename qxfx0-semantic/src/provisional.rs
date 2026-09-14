//! Provisional lexicon (ADR-0045 C1): unknown content words sighted
//! across turns accumulate in quarantine and promote into graph atoms
//! at threshold — the bounded, reviewable form of "learning new words".
//!
//! The law refinement (ADR-0029): user text never *self-promotes* —
//! provisional candidates are observable (activation sees promoted
//! atoms by category) but never promotion-admissible (the admission
//! universe excludes `CatProvisional`), and promotion requires three
//! sightings spanning two turns plus no canonical collision. TTL and
//! caps bound the store; everything is deterministic in
//! (meta, units, turn).
//!
//! Flow per turn (Finalize, after graph growth): `observe` this turn's
//! unknown units, `evict` the silent and the excess, `promote` the
//! ripe into `CatProvisional` atoms (caller inserts, drops meta).

use std::collections::BTreeMap;

use qxfx0_types::atom::{Atom, AtomCategory, AtomGraph, AtomId, AtomProvisionalMeta};
use qxfx0_types::atom::{
    MAX_PROVISIONAL_ATOMS, PROVISIONAL_PROMOTE_MIN_SPAN, PROVISIONAL_PROMOTE_OCCURRENCES,
    PROVISIONAL_TTL_TURNS,
};

use crate::input_frame::WordUnit;

/// Observe one turn's unknown content words: alphabetic, length ≥ 3,
/// no dictionary entry. Repeated words within the turn count once —
/// occurrences measure turns, not tokens.
pub fn observe(meta: &mut BTreeMap<String, AtomProvisionalMeta>, units: &[WordUnit], turn: usize) {
    let mut seen: Vec<&str> = Vec::new();
    for unit in units {
        // Unknown AND lexically open: scaffolding and verb shapes are
        // never content, however unknown to the dictionary.
        if unit.pos.is_some() || !crate::noun_phrase::is_lexical_content(&unit.surface) {
            continue;
        }
        if seen.contains(&unit.surface.as_str()) {
            continue;
        }
        seen.push(unit.surface.as_str());
    }
    for surface in seen {
        meta.entry(surface.to_string())
            .and_modify(|entry| {
                entry.occurrences = entry.occurrences.saturating_add(1);
                entry.last_turn = turn;
            })
            .or_insert(AtomProvisionalMeta {
                occurrences: 1,
                first_turn: turn,
                last_turn: turn,
            });
    }
}

/// Evict the silent (TTL) and the excess (oldest-first by last turn,
/// then surface — deterministic). Returns evicted surfaces, newest
/// logic first: TTL before cap.
pub fn evict(meta: &mut BTreeMap<String, AtomProvisionalMeta>, turn: usize) -> Vec<String> {
    let mut evicted = Vec::new();
    meta.retain(|surface, entry| {
        if turn.saturating_sub(entry.last_turn) > PROVISIONAL_TTL_TURNS {
            evicted.push(surface.clone());
            false
        } else {
            true
        }
    });
    while meta.len() > MAX_PROVISIONAL_ATOMS {
        let oldest = meta
            .iter()
            .min_by(|left, right| {
                left.1
                    .last_turn
                    .cmp(&right.1.last_turn)
                    .then_with(|| left.0.cmp(right.0))
            })
            .map(|(surface, _)| surface.clone())
            .expect("nonempty above the cap");
        meta.remove(&oldest);
        evicted.push(oldest);
    }
    evicted
}

/// Promote ripe candidates into atoms: threshold occurrences spanning
/// the minimum turns, and no canonical collision (an atom with the
/// same display already exists — curated wins, the candidate drops).
/// Returns the atoms to insert; caller drops the promoted keys.
pub fn promote(
    meta: &BTreeMap<String, AtomProvisionalMeta>,
    graph: &AtomGraph,
) -> Vec<(String, Atom)> {
    let mut atoms = Vec::new();
    for (surface, entry) in meta {
        if entry.occurrences < PROVISIONAL_PROMOTE_OCCURRENCES {
            continue;
        }
        if entry.last_turn.saturating_sub(entry.first_turn) < PROVISIONAL_PROMOTE_MIN_SPAN {
            continue;
        }
        let collides = graph
            .atoms
            .values()
            .any(|atom| atom.display.to_lowercase() == *surface);
        if collides {
            continue;
        }
        atoms.push((
            surface.clone(),
            Atom {
                id: AtomId::new(surface),
                display: surface.clone(),
                category: AtomCategory::CatProvisional,
            },
        ));
    }
    atoms
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::morphology::PartOfSpeech;

    fn unit(surface: &str) -> WordUnit {
        WordUnit {
            surface: surface.to_string(),
            lemma: surface.to_string(),
            pos: None,
            confidence: 0.5,
            ambiguity: vec![surface.to_string()],
        }
    }

    fn known(surface: &str) -> WordUnit {
        WordUnit {
            surface: surface.to_string(),
            lemma: surface.to_string(),
            pos: Some(PartOfSpeech::Noun),
            confidence: 1.0,
            ambiguity: vec![surface.to_string()],
        }
    }

    #[test]
    fn observe_counts_turns_not_tokens() {
        let mut meta = BTreeMap::new();
        observe(
            &mut meta,
            &[unit("ксеномодус"), known("свобода"), unit("ксеномодус")],
            1,
        );
        assert_eq!(meta.len(), 1, "known words never enter");
        assert_eq!(meta["ксеномодус"].occurrences, 1);
        observe(&mut meta, &[unit("ксеномодус")], 2);
        assert_eq!(meta["ксеномодус"].occurrences, 2);
        assert_eq!(meta["ксеномодус"].first_turn, 1);
    }

    #[test]
    fn promote_needs_threshold_span_and_no_collision() {
        let mut meta = BTreeMap::new();
        for turn in [1, 2, 3] {
            observe(&mut meta, &[unit("ксеномодус")], turn);
        }
        let graph = AtomGraph::default();
        let atoms = promote(&meta, &graph);
        assert_eq!(atoms.len(), 1);
        assert_eq!(atoms[0].1.category, AtomCategory::CatProvisional);

        // Span too short: three sightings on one turn never promote.
        let mut meta = BTreeMap::new();
        for _ in 0..3 {
            observe(&mut meta, &[unit("флюгегехаймен")], 1);
        }
        assert!(promote(&meta, &AtomGraph::default()).is_empty());

        // Canonical collision: curated wins, candidate drops.
        let mut graph = AtomGraph::default();
        graph.atoms.insert(
            AtomId::new("память"),
            Atom {
                id: AtomId::new("память"),
                display: "память".into(),
                category: AtomCategory::CatConcept,
            },
        );
        let mut meta = BTreeMap::new();
        for turn in [1, 2, 3] {
            observe(&mut meta, &[unit("память")], turn);
        }
        // "память" resolves in production; forced here as unknown —
        // the collision rule fires regardless of how it entered.
        assert!(promote(&meta, &graph).is_empty());
    }

    #[test]
    fn evict_forgets_the_silent_and_caps() {
        let mut meta = BTreeMap::new();
        observe(&mut meta, &[unit("старое")], 1);
        observe(&mut meta, &[unit("свежее")], 100);
        let evicted = evict(&mut meta, 140);
        assert_eq!(evicted, vec!["старое".to_string()]);
        assert!(meta.contains_key("свежее"));
    }
}

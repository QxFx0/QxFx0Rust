//! ContentSelector — field-modulated predicate selection via cosine similarity
//! between predicate token vectors and Field prototypes.
//!
//! Uses spreading activation from SemanticNetwork for cross-topic composition.

use qxfx0_types::atom::{AtomGraph, AtomId, Relation};
use qxfx0_types::field::FieldProfile;
use qxfx0_types::RelationType;
use std::collections::{BTreeMap, BTreeSet};

use crate::network::{activate_topic, build_topic_atoms, get_activated_atoms};
use crate::pathfinder::PathFinder;
use qxfx0_types::network::SemanticNetwork;

/// A selected predicate with its score.
#[derive(Debug, Clone)]
pub struct SelectedPredicate {
    pub topic: String,
    pub score: f64,
    pub relation: Relation,
    pub source_topic: String,
}

/// ContentSelector holds indices for fast predicate lookup.
pub struct ContentSelector {
    pub topic_predicates: BTreeMap<String, Vec<Relation>>,
    pub topic_atoms: BTreeMap<AtomId, BTreeSet<AtomId>>,
    pub genericity_limit: usize,
}

impl ContentSelector {
    /// Build from an AtomGraph.
    pub fn build(graph: &AtomGraph) -> Self {
        let mut topic_predicates: BTreeMap<String, Vec<Relation>> = BTreeMap::new();
        for edge in &graph.edges {
            topic_predicates
                .entry(edge.topic.clone())
                .or_default()
                .push(edge.clone());
        }

        let total_topics = topic_predicates.len();
        let total_preds: usize = topic_predicates.values().map(|v| v.len()).sum();
        let avg_preds = if total_topics > 0 {
            total_preds as f64 / total_topics as f64
        } else {
            0.0
        };
        let genericity_limit = (avg_preds * 3.0).ceil() as usize;

        let topic_atoms = build_topic_atoms(graph);

        ContentSelector {
            topic_predicates,
            topic_atoms,
            genericity_limit,
        }
    }

    /// Select the best predicate for a topic, field-modulated.
    pub fn select_predicates(
        &self,
        fp: &FieldProfile,
        topic: &str,
        activated_network: Option<&SemanticNetwork>,
    ) -> Vec<SelectedPredicate> {
        let predicates = match self.topic_predicates.get(topic) {
            Some(preds) => preds.clone(),
            None => return Vec::new(),
        };

        let mut scored: Vec<(Relation, f64)> = predicates
            .iter()
            .filter_map(|p| {
                let s = score_pred(fp, p, activated_network);
                if s > 0.1 {
                    Some((p.clone(), s))
                } else {
                    None
                }
            })
            .collect();

        // Deterministic order: descending score, then (from, to, rel_type, topic)
        // as a stable tie-breaker. Scores come from `score_pred`, which
        // combines relation-type affinity with floating-point activation
        // bonuses; ties are common and Rust's sort does not guarantee a
        // stable order for equal elements, so we break ties on the
        // canonical Relation fields (AtomId + RelationType + topic).
        scored.sort_by(|a, b| {
            b.1.total_cmp(&a.1).then_with(|| {
                a.0.from
                    .cmp(&b.0.from)
                    .then_with(|| a.0.to.cmp(&b.0.to))
                    .then_with(|| a.0.rel_type.cmp(&b.0.rel_type))
                    .then_with(|| a.0.topic.cmp(&b.0.topic))
            })
        });

        scored
            .into_iter()
            .map(|(rel, score)| SelectedPredicate {
                topic: topic.to_string(),
                score,
                relation: rel,
                source_topic: topic.to_string(),
            })
            .collect()
    }

    /// Compose predicates from activation — selects top-3 related topics.
    pub fn compose_from_activation(
        &self,
        fp: &FieldProfile,
        topic: &str,
        network: &SemanticNetwork,
    ) -> Vec<SelectedPredicate> {
        let mut results: Vec<SelectedPredicate> = Vec::new();

        // Phase 1: Direct topic predicates — always included, take top-3 by score
        if let Some(preds) = self.topic_predicates.get(topic) {
            let mut scored: Vec<(Relation, f64)> = preds
                .iter()
                .filter_map(|p| {
                    let s = score_pred(fp, p, None);
                    if s > 0.05 {
                        Some((p.clone(), s))
                    } else {
                        None
                    }
                })
                .collect();
            // Deterministic order: descending score, with a Relation-keyed
            // tie-breaker. See `select_predicates` for the rationale — the
            // same `(from, to, rel_type, topic)` chain guarantees identical
            // ordering across runs for this Phase-1 candidate list.
            scored.sort_by(|a, b| {
                b.1.total_cmp(&a.1).then_with(|| {
                    a.0.from
                        .cmp(&b.0.from)
                        .then_with(|| a.0.to.cmp(&b.0.to))
                        .then_with(|| a.0.rel_type.cmp(&b.0.rel_type))
                        .then_with(|| a.0.topic.cmp(&b.0.topic))
                })
            });
            for (rel, score) in scored.into_iter().take(3) {
                results.push(SelectedPredicate {
                    topic: topic.to_string(),
                    score,
                    relation: rel,
                    source_topic: topic.to_string(),
                });
            }
        }

        // Phase 2: Cross-topic from activation — fill remaining slots to 3 total
        let remaining = 3usize.saturating_sub(results.len());
        if remaining > 0 {
            let topic_atoms = self
                .topic_atoms
                .get(&AtomId::new(topic.to_string()))
                .cloned()
                .unwrap_or_default();
            let activated_network = activate_topic(&topic_atoms, network);
            let activated_atoms: BTreeSet<AtomId> = get_activated_atoms(&activated_network, 0.05)
                .into_iter()
                .map(|(a, _)| a)
                .collect();

            let overlapping: Vec<String> = self
                .topic_atoms
                .iter()
                .filter(|(id, atoms)| {
                    id.as_str() != topic
                        && self
                            .topic_predicates
                            .get(id.as_str())
                            .is_some_and(|p| p.len() <= self.genericity_limit)
                        && !atoms.is_disjoint(&activated_atoms)
                })
                .map(|(id, _)| id.as_str().to_string())
                .collect();

            let all_activated: Vec<(AtomId, f64)> = get_activated_atoms(&activated_network, 0.0);

            let mut cross: Vec<(String, Relation, f64)> = Vec::new();
            for t in &overlapping {
                if let Some(preds) = self.topic_predicates.get(t) {
                    // Check minimum activation weight for this cross-topic
                    let max_act: f64 = all_activated
                        .iter()
                        .filter(|(a, _)| {
                            self.topic_atoms
                                .get(&AtomId::new(t.clone()))
                                .map(|atoms| atoms.contains(a))
                                .unwrap_or(false)
                        })
                        .map(|(_, w)| *w)
                        .fold(0.0, f64::max);
                    if max_act < 0.15 {
                        continue;
                    }

                    let mut best: Option<(Relation, f64)> = None;
                    for p in preds {
                        let s = score_pred(fp, p, Some(&activated_network)) * 0.3;
                        if s > 0.15 {
                            match &best {
                                Some((_, bs)) if s > *bs => best = Some((p.clone(), s)),
                                None => best = Some((p.clone(), s)),
                                _ => {}
                            }
                        }
                    }
                    if let Some((rel, s)) = best {
                        cross.push((t.clone(), rel, s));
                    }
                }
            }
            // Deterministic order: descending score, with (topic, from, to,
            // rel_type) as the stable tie-breaker. Cross-topic candidates
            // are scored against an activated network; ties happen whenever
            // multiple predicates share the same field affinity and
            // activation bonus, so we lock ordering on the underlying
            // identifiers.
            cross.sort_by(|a, b| {
                b.2.total_cmp(&a.2).then_with(|| {
                    a.0.cmp(&b.0)
                        .then_with(|| a.1.from.cmp(&b.1.from))
                        .then_with(|| a.1.to.cmp(&b.1.to))
                        .then_with(|| a.1.rel_type.cmp(&b.1.rel_type))
                })
            });
            for (t, rel, score) in cross.into_iter().take(remaining) {
                // Skip if already in results (dedup by relation ID)
                if !results.iter().any(|r| {
                    r.relation.from == rel.from
                        && r.relation.to == rel.to
                        && r.relation.rel_type == rel.rel_type
                }) {
                    results.push(SelectedPredicate {
                        topic: t.clone(),
                        score,
                        relation: rel,
                        source_topic: t,
                    });
                }
            }
        }

        results
    }
}

/// Score a single predicate with field-weighted contributions + activation bonus.
fn score_pred(
    fp: &FieldProfile,
    pred: &Relation,
    activated_network: Option<&SemanticNetwork>,
) -> f64 {
    let base = relation_type_affinity(fp, pred.rel_type);

    let activation_bonus = match activated_network {
        Some(sn) => {
            let from_act = sn.activation.get(&pred.from).copied().unwrap_or(0.0);
            let to_act = sn.activation.get(&pred.to).copied().unwrap_or(0.0);
            (from_act + to_act) * 0.15
        }
        None => 0.0,
    };

    base * (1.0 + activation_bonus)
}

/// Map relation type to field affinity score.
/// Delegates to the single source of truth in `PathFinder::relation_type_bias`
/// to avoid drift between path ranking and predicate selection.
fn relation_type_affinity(fp: &FieldProfile, rt: RelationType) -> f64 {
    PathFinder::relation_type_bias(fp, rt)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::build_semantic_network;
    use crate::seed_graph;

    #[test]
    fn test_content_selector_builds() {
        let graph = seed_graph();
        let cs = ContentSelector::build(&graph);
        assert!(!cs.topic_predicates.is_empty());
        assert!(cs.topic_predicates.contains_key("свобода"));
    }

    #[test]
    fn test_select_predicates_for_topic() {
        let graph = seed_graph();
        let cs = ContentSelector::build(&graph);
        let fp = FieldProfile::default();
        let results = cs.select_predicates(&fp, "свобода", None);
        assert!(!results.is_empty());
        assert!(results.iter().any(|sp| sp.topic == "свобода"));
    }

    #[test]
    fn test_select_predicates_top_scored_first() {
        let graph = seed_graph();
        let cs = ContentSelector::build(&graph);
        let fp = FieldProfile::default();
        let results = cs.select_predicates(&fp, "свобода", None);
        if results.len() >= 2 {
            assert!(results[0].score >= results[1].score);
        }
    }

    #[test]
    fn test_compose_from_activation() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        let cs = ContentSelector::build(&graph);
        let fp = FieldProfile::default();
        let results = cs.compose_from_activation(&fp, "свобода", &sn);
        assert!(!results.is_empty());
        assert!(results.len() <= 3);
    }
}

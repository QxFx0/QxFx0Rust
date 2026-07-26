//! SemanticNetwork — spreading activation over a weighted topic graph.
//!
//! Two-layer architecture:
//!   - Explicit layer: hand-written philosophical edges (weight 1.0, from seed_graph)
//!   - Substrate layer: co-occurrence edges derived from token overlap (weight 0.3)  
//!
//! Substrate edges route activation but never appear in verbalized output.
use qxfx0_types::atom::{AtomGraph, AtomId};
use qxfx0_types::network::{ActivationStep, EdgeSource, SemanticEdge, SemanticNetwork};
use qxfx0_types::system_state::SemanticState;
use std::collections::{BTreeMap, BTreeSet};

/// Build a SemanticNetwork from the explicit AtomGraph + substrate co-occurrence edges.
pub fn build_semantic_network(graph: &AtomGraph) -> SemanticNetwork {
    let mut sn = SemanticNetwork::new();

    for atom_id in graph.atoms.keys() {
        sn.nodes.insert(atom_id.clone());
    }

    let mut max_count: usize = 1;
    for edge in &graph.edges {
        sn.nodes.insert(edge.from.clone());
        sn.nodes.insert(edge.to.clone());
        let key = (edge.from.clone(), edge.to.clone());
        let entry = sn.edges.entry(key).or_insert_with(|| SemanticEdge {
            from: edge.from.clone(),
            to: edge.to.clone(),
            weight: 0.0,
            co_occurrence: 0,
            source: EdgeSource::ExplicitEdge,
        });
        entry.co_occurrence += 1;
        if entry.co_occurrence > max_count {
            max_count = entry.co_occurrence;
        }
    }

    for edge in sn.edges.values_mut() {
        if edge.source == EdgeSource::ExplicitEdge {
            edge.weight = edge.co_occurrence as f64 / max_count as f64;
        }
    }

    let substrate_edges = build_substrate_edges(graph, &sn.nodes);
    for se in substrate_edges {
        let key = (se.from.clone(), se.to.clone());
        sn.edges.entry(key).or_insert(se);
    }

    sn
}

/// Build substrate co-occurrence edges by analyzing shared tokens in ru_original.
fn build_substrate_edges(
    graph: &AtomGraph,
    explicit_nodes: &BTreeSet<AtomId>,
) -> Vec<SemanticEdge> {
    // Precompute lowercased node strings once.
    let explicit_list: Vec<(&AtomId, String)> = explicit_nodes
        .iter()
        .map(|a| (a, a.as_str().to_lowercase()))
        .collect();
    let mut cooc_map: BTreeMap<(AtomId, AtomId), usize> = BTreeMap::new();

    for edge in &graph.edges {
        let tokens: Vec<String> = edge
            .ru_original
            .split_whitespace()
            .filter(|w| w.len() > 3)
            .map(|w| w.to_lowercase())
            .collect();
        if tokens.is_empty() {
            continue;
        }

        // Find which explicit nodes are hit by this edge's tokens — O(N) per edge.
        let hits: Vec<&AtomId> = explicit_list
            .iter()
            .filter(|(_, lower)| tokens.iter().any(|t| t.contains(lower)))
            .map(|(a, _)| *a)
            .collect();

        // Generate co-occurring pairs only from hit nodes (hits << N typically).
        for i in 0..hits.len() {
            for j in (i + 1)..hits.len() {
                let key = if hits[i] < hits[j] {
                    (hits[i].clone(), hits[j].clone())
                } else {
                    (hits[j].clone(), hits[i].clone())
                };
                *cooc_map.entry(key).or_insert(0) += 1;
            }
        }
    }

    let mut edges: Vec<SemanticEdge> = Vec::new();
    for ((a, b), count) in cooc_map {
        if count >= 1 {
            edges.push(SemanticEdge {
                from: a.clone(),
                to: b.clone(),
                weight: 0.3,
                co_occurrence: count,
                source: EdgeSource::SubstrateEdge,
            });
            edges.push(SemanticEdge {
                from: b,
                to: a,
                weight: 0.3,
                co_occurrence: count,
                source: EdgeSource::SubstrateEdge,
            });
        }
    }
    edges
}

/// Adjacency index: from-atom → list of (to-atom, edge).
/// Built once per activation to avoid O(E) scans per neighbor lookup.
type AdjacencyIndex = BTreeMap<AtomId, Vec<(AtomId, SemanticEdge)>>;

/// Build an adjacency index from the network's edges — O(E).
fn build_adjacency_index(sn: &SemanticNetwork) -> AdjacencyIndex {
    let mut idx: AdjacencyIndex = BTreeMap::new();
    for ((from, to), edge) in &sn.edges {
        idx.entry(from.clone())
            .or_default()
            .push((to.clone(), edge.clone()));
    }
    idx
}

/// Activate a single seed topic (depth-first spreading, bounded by max_hops).
pub fn activate(seed: &AtomId, sn: &SemanticNetwork) -> SemanticNetwork {
    let mut sn = sn.clone();
    sn.activation_log.clear();
    let adj = build_adjacency_index(&sn);
    let initial = BTreeMap::from([(seed.clone(), 1.0)]);
    sn.activation_log.push(ActivationStep {
        node: seed.clone(),
        source: EdgeSource::ExplicitEdge,
        via: seed.clone(),
        hop: 0,
        weight: 1.0,
    });
    spread_activation(&mut sn, &initial, 0, &adj)
}

/// Activate multiple seed topics simultaneously.
pub fn activate_topic(seeds: &BTreeSet<AtomId>, sn: &SemanticNetwork) -> SemanticNetwork {
    let mut sn = sn.clone();
    sn.activation_log.clear();
    let adj = build_adjacency_index(&sn);
    let initial: BTreeMap<AtomId, f64> = seeds.iter().map(|a| (a.clone(), 1.0)).collect();
    for atom in seeds {
        sn.activation_log.push(ActivationStep {
            node: atom.clone(),
            source: EdgeSource::ExplicitEdge,
            via: atom.clone(),
            hop: 0,
            weight: 1.0,
        });
    }
    spread_activation(&mut sn, &initial, 0, &adj)
}

/// Recursive spreading activation.
fn spread_activation(
    sn: &mut SemanticNetwork,
    activation: &BTreeMap<AtomId, f64>,
    hop: usize,
    adj: &AdjacencyIndex,
) -> SemanticNetwork {
    if hop >= sn.max_hops {
        sn.activation = activation.clone();
        return sn.clone();
    }

    let mut new_acts: BTreeMap<AtomId, f64> = BTreeMap::new();

    for (atom, act) in activation {
        // O(degree) lookup via adjacency index instead of O(E) scan.
        if let Some(neighbors) = adj.get(atom) {
            for (neighbor, edge) in neighbors {
                if activation.contains_key(neighbor) {
                    continue;
                }
                let weight = act * edge.weight * sn.decay_rate;
                let existing = new_acts.entry(neighbor.clone()).or_insert(0.0);
                *existing = existing.max(weight);
                sn.activation_log.push(ActivationStep {
                    node: neighbor.clone(),
                    source: edge.source,
                    via: atom.clone(),
                    hop: hop + 1,
                    weight,
                });
            }
        }
    }

    if new_acts.is_empty() {
        sn.activation = activation.clone();
        return sn.clone();
    }

    let mut merged = activation.clone();
    for (k, v) in new_acts {
        let entry = merged.entry(k).or_insert(0.0);
        *entry = entry.max(v);
    }

    spread_activation(sn, &merged, hop + 1, adj)
}

/// Get atoms with activation above threshold, sorted by weight descending.
pub fn get_activated_atoms(sn: &SemanticNetwork, threshold: f64) -> Vec<(AtomId, f64)> {
    let mut results: Vec<(AtomId, f64)> = sn
        .activation
        .iter()
        .filter(|(_, &w)| w > threshold)
        .map(|(a, &w)| (a.clone(), w))
        .collect();
    // Deterministic order: descending activation weight, then AtomId.
    // Activation weights are f64 products (`act * weight * decay_rate`)
    // that can collide when two atoms receive the same propagated
    // activation. `AtomId` is unique and Ord, so it gives a stable
    // tie-breaker that is bit-exact across runs and platforms.
    results.sort_by(|a, b| b.1.total_cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    results
}

/// Build a topic-to-atoms map for the ContentSelector.
pub fn build_topic_atoms(graph: &AtomGraph) -> BTreeMap<AtomId, BTreeSet<AtomId>> {
    let mut map: BTreeMap<AtomId, BTreeSet<AtomId>> = BTreeMap::new();
    for edge in &graph.edges {
        let topic = &edge.topic;
        let topic_id = AtomId::new(topic.clone());
        map.entry(topic_id).or_default().insert(edge.from.clone());
        map.entry(AtomId::new(edge.topic.clone()))
            .or_default()
            .insert(edge.to.clone());
    }
    map
}

/// Build or reuse a cached semantic network stored in `semantic`.
///
/// The cache is keyed by `runtime_graph.edges.len()`. When the graph grows,
/// the network is rebuilt exactly once and stored back in the state.
pub fn cached_semantic_network(semantic: &mut SemanticState) -> SemanticNetwork {
    let edge_count = semantic.runtime_graph.edges.len();
    if let Some(cached) = semantic.cached_network.clone() {
        if semantic.cached_edge_count == edge_count {
            return cached;
        }
    }
    let network = build_semantic_network(&semantic.runtime_graph);
    semantic.cached_network = Some(network.clone());
    semantic.cached_edge_count = edge_count;
    network
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed_graph;

    #[test]
    fn test_build_network_has_nodes_and_edges() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        assert!(!sn.nodes.is_empty());
        assert!(
            sn.edges.len() >= 42,
            "expected >= 42 edges, got {}",
            sn.edges.len()
        );
    }

    #[test]
    fn test_activate_single_seed() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        let activated = activate(&AtomId::new("свобода"), &sn);
        let active = get_activated_atoms(&activated, 0.05);
        assert!(!active.is_empty());
        assert!(active.iter().any(|(a, _)| a.as_str() == "свобода"));
    }

    #[test]
    fn test_spreading_activation_multi_hop() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        let activated = activate(&AtomId::new("свобода"), &sn);
        assert!(
            activated.activation.len() >= 2,
            "Expected multi-hop activation, got {} atoms",
            activated.activation.len()
        );
    }

    #[test]
    fn test_activate_topic_multiple_seeds() {
        let graph = seed_graph();
        let sn = build_semantic_network(&graph);
        let seeds: BTreeSet<AtomId> = ["свобода", "ответственность"]
            .iter()
            .map(|s| AtomId::new(*s))
            .collect();
        let activated = activate_topic(&seeds, &sn);
        let active = get_activated_atoms(&activated, 0.05);
        assert!(active.len() >= 2);
    }

    #[test]
    fn test_cached_semantic_network_rebuilds_on_growth() {
        use qxfx0_types::atom::{ObjectCase, Relation, RelationSource};
        use qxfx0_types::RelationType;

        let mut semantic = SemanticState {
            runtime_graph: seed_graph(),
            ..SemanticState::default()
        };
        let sn1 = cached_semantic_network(&mut semantic);
        assert!(!sn1.edges.is_empty());
        let edge_count = semantic.runtime_graph.edges.len();

        let sn2 = cached_semantic_network(&mut semantic);
        assert_eq!(semantic.cached_edge_count, edge_count);
        assert_eq!(sn1.edges.len(), sn2.edges.len());

        // Add a new edge and ensure the cache is rebuilt.
        semantic.runtime_graph.edges.push(Relation {
            from: AtomId::new("свобода"),
            to: AtomId::new("тест"),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "тестовый объект".into(),
            verb_override: None,
            ru_original: "свобода предполагает тестовый объект".into(),
            en_original: "freedom presupposes test object".into(),
            source: RelationSource::SeedFromPredicate,
            topic: "свобода".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        });
        let sn3 = cached_semantic_network(&mut semantic);
        assert_eq!(semantic.cached_edge_count, edge_count + 1);
        assert_ne!(sn1.edges.len(), sn3.edges.len());
    }
}

use qxfx0_types::atom::{
    AtomGraph, AtomId, ConjugateVector, GeneratedSurface, PathProof, Relation, SenseVector,
};
use qxfx0_types::RelationType;
use std::collections::BTreeSet;

/// ConjugateComposer — composes responses through vector algebra over the semantic graph.
///
/// Input sense vectors → graph traversal → conjugate vector → verbalization.
/// Uses graph-stored `ru_original` sentences and a small set of connector phrases;
/// responses are deterministic because the vector path is deterministic.
pub struct ConjugateComposer;

/// Relation type categories for vector resonance.
fn is_supporting(rt: RelationType) -> bool {
    rt.is_supporting()
}

fn is_counter(rt: RelationType) -> bool {
    rt.is_counter()
}

fn is_qualifying(rt: RelationType) -> bool {
    rt.is_qualifying()
}

impl ConjugateComposer {
    /// Minimal semantic response returned when input has no meaningful content.
    fn minimal_semantic_response() -> GeneratedSurface {
        GeneratedSurface {
            text: "я не знаю этот смысл".to_string(),
            paths: Vec::new(),
            provenance: Vec::new(),
            depth_score: 0.0,
        }
    }

    /// Check if all sense vectors have zero magnitude (weight <= 0.0).
    fn is_zero_magnitude(sense_vectors: &[SenseVector]) -> bool {
        sense_vectors.iter().all(|v| v.weight <= 0.0)
    }

    /// Compose a conjugate response from input sense vectors.
    /// Traverses the graph from activated atoms, collects edges, verbalizes.
    pub fn compose(graph: &AtomGraph, sense_vectors: &[SenseVector]) -> GeneratedSurface {
        if sense_vectors.is_empty() || Self::is_zero_magnitude(sense_vectors) {
            return Self::minimal_semantic_response();
        }

        // If 2+ atoms activated, try BFS path between dominant pair
        if sense_vectors.len() >= 2 {
            let sorted = {
                let mut v: Vec<&SenseVector> = sense_vectors.iter().collect();
                v.sort_by(|a, b| b.weight.total_cmp(&a.weight));
                v
            };
            let from = &sorted[0].atom_id;
            let to = &sorted[1].atom_id;
            let path = crate::composer::GraphEngagement::bfs_path(graph, from, to);
            if !path.is_empty() {
                return Self::verbalize_path(graph, &path, sense_vectors);
            }
        }

        // Build conjugate vector by traversing from activated atoms
        let conjugate = Self::traverse(graph, sense_vectors);

        // Verbalize the conjugate vector into text
        Self::verbalize(graph, &conjugate)
    }

    /// Verbalize a BFS path between two atoms — connect mode.
    fn verbalize_path(
        _graph: &AtomGraph,
        path: &[Relation],
        sense_vectors: &[SenseVector],
    ) -> GeneratedSurface {
        let parts: Vec<String> = path
            .iter()
            .map(|r| {
                let mut t = crate::seed::verbalize_relation(r);
                if let Some(ref rationale) = r.rationale {
                    t.push_str(&format!(" — потому что {}", rationale));
                }
                t
            })
            .collect();

        let text = parts.join(" → ");

        let topic = sense_vectors
            .first()
            .map(|v| v.atom_id.as_str().to_string())
            .unwrap_or_default();

        GeneratedSurface {
            text,
            paths: vec![PathProof {
                edges: path.to_vec(),
                topic,
            }],
            provenance: path.iter().map(|r| r.source).collect(),
            depth_score: path.len() as f64,
        }
    }

    /// Traverse the graph from input sense vectors, building a conjugate vector.
    /// Collects supporting, counter, and qualifying edges weighted by input activation.
    fn traverse(graph: &AtomGraph, sense_vectors: &[SenseVector]) -> ConjugateVector {
        let mut cv = ConjugateVector::new();
        let mut visited_edges: BTreeSet<(AtomId, RelationType, AtomId)> = BTreeSet::new();
        let mut visited_atoms: BTreeSet<AtomId> = BTreeSet::new();

        // Sort input vectors by weight (dominant first)
        let mut sorted_input: Vec<&SenseVector> = sense_vectors.iter().collect();
        sorted_input.sort_by(|a, b| b.weight.total_cmp(&a.weight));

        for sv in &sorted_input {
            if visited_atoms.contains(&sv.atom_id) {
                continue;
            }
            visited_atoms.insert(sv.atom_id.clone());

            // Add this atom as a component of the conjugate vector
            let mut component = SenseVector {
                atom_id: sv.atom_id.clone(),
                weight: sv.weight,
                relation_vector: Vec::new(),
            };

            // Traverse edges from this atom
            let edges = graph.relations_from(&sv.atom_id);
            for edge in edges {
                let edge_key = (edge.from.clone(), edge.rel_type, edge.to.clone());
                if visited_edges.contains(&edge_key) {
                    continue;
                }
                visited_edges.insert(edge_key);

                // Weight edge by input activation
                let edge_weight = sv.weight;
                component.relation_vector.push((edge.rel_type, edge_weight));

                // Add edge to conjugate path
                cv.edges.push((*edge).clone());
            }

            cv.components.push(component);
        }

        // Compute resonance: how strongly the graph responds to the input
        let total_input_weight: f64 = sense_vectors.iter().map(|v| v.weight).sum();
        let edge_count = cv.edges.len() as f64;
        cv.resonance = if total_input_weight > 0.0 {
            (edge_count / total_input_weight).min(1.0)
        } else {
            0.0
        };

        cv
    }

    /// Verbalize a conjugate vector into text using graph-stored sentences and connectors.
    /// Walks the graph edges, collects unique chains, generates text from relations.
    fn verbalize(graph: &AtomGraph, cv: &ConjugateVector) -> GeneratedSurface {
        if cv.edges.is_empty() {
            // No edges found — try to find any relation for the dominant atom
            if let Some(component) = cv.components.first() {
                let edges = graph.relations_from(&component.atom_id);
                if let Some(first_edge) = edges.first() {
                    let text = crate::seed::verbalize_relation(first_edge);
                    return GeneratedSurface {
                        text,
                        paths: vec![PathProof {
                            edges: vec![(*first_edge).clone()],
                            topic: component.atom_id.as_str().to_string(),
                        }],
                        provenance: vec![first_edge.source],
                        depth_score: 1.0,
                    };
                }
            }
            return GeneratedSurface {
                text: String::new(),
                paths: Vec::new(),
                provenance: Vec::new(),
                depth_score: 0.0,
            };
        }

        // Classify edges into support, counter, qualify, unclassified
        let mut supporting: Vec<&Relation> = Vec::new();
        let mut countering: Vec<&Relation> = Vec::new();
        let mut qualifying: Vec<&Relation> = Vec::new();
        let mut unclassified: Vec<&Relation> = Vec::new();

        for edge in &cv.edges {
            if is_supporting(edge.rel_type) {
                supporting.push(edge);
            } else if is_counter(edge.rel_type) {
                countering.push(edge);
            } else if is_qualifying(edge.rel_type) {
                qualifying.push(edge);
            } else {
                unclassified.push(edge);
            }
        }

        // Build text from graph traversal — uses graph-stored ru_original sentences.
        let mut parts: Vec<String> = Vec::new();

        // Support: verbalize each edge directly from graph
        for edge in &supporting {
            parts.push(crate::seed::verbalize_relation(edge));
            // Include rationale if present (adds depth from graph structure)
            if let Some(ref rationale) = edge.rationale {
                parts.push(format!("потому что {}", rationale));
            }
        }

        // Qualifying: structural constraints from graph
        for edge in &qualifying {
            parts.push(crate::seed::verbalize_relation(edge));
            if let Some(ref synthesis) = edge.synthesis {
                parts.push(format!("именно поэтому {}", synthesis));
            }
        }

        // Unclassified: append with a documented neutral prefix
        if !unclassified.is_empty() {
            let unclassified_texts: Vec<String> =
                unclassified.iter().map(|e| crate::seed::verbalize_relation(e)).collect();
            parts.push(format!("связано с {}", unclassified_texts.join(". ")));
        }

        // Counter: contradictions from graph — natural defense language
        if !countering.is_empty() {
            let counter_texts: Vec<String> =
                countering.iter().map(|e| crate::seed::verbalize_relation(e)).collect();
            parts.push(format!("но {}", counter_texts.join(". ")));
        }

        let text = parts.join(". ");

        // Collect provenance
        let provenance: Vec<_> = cv.edges.iter().map(|e| e.source).collect();

        GeneratedSurface {
            text,
            paths: vec![PathProof {
                edges: cv.edges.clone(),
                topic: cv
                    .components
                    .first()
                    .map(|c| c.atom_id.as_str().to_string())
                    .unwrap_or_default(),
            }],
            provenance,
            depth_score: cv.depth() as f64,
        }
    }

    /// Compose a conjugate response with challenge awareness.
    /// When the input is a challenge, the system defends its position from the graph.
    pub fn compose_with_challenge(
        graph: &AtomGraph,
        sense_vectors: &[SenseVector],
        is_challenge: bool,
    ) -> GeneratedSurface {
        if !is_challenge {
            return Self::compose(graph, sense_vectors);
        }

        // For challenges: traverse graph for defense edges
        let cv = Self::traverse(graph, sense_vectors);

        // Also traverse reverse edges (what points TO our atoms)
        let mut reverse_edges: Vec<Relation> = Vec::new();
        let mut seen_reverse: BTreeSet<(AtomId, RelationType, AtomId)> = BTreeSet::new();
        for sv in sense_vectors {
            for edge in graph.relations_to(&sv.atom_id) {
                let edge_key = (edge.from.clone(), edge.rel_type, edge.to.clone());
                if !cv.edges.iter().any(|e| {
                    (e.from.clone(), e.rel_type, e.to.clone()) == edge_key
                }) && seen_reverse.insert(edge_key) {
                    reverse_edges.push((*edge).clone());
                }
            }
        }

        // Build defense text from supporting edges
        let supporting: Vec<&Relation> = cv.edges.iter().filter(|e| is_supporting(e.rel_type)).collect();
        let countering: Vec<&Relation> = cv.edges.iter().filter(|e| is_counter(e.rel_type)).collect();

        let mut parts: Vec<String> = Vec::new();

        if !supporting.is_empty() {
            for edge in &supporting {
                parts.push(crate::seed::verbalize_relation(edge));
                if let Some(ref rationale) = edge.rationale {
                    parts.push(format!("потому что {}", rationale));
                }
                if let Some(ref synthesis) = edge.synthesis {
                    parts.push(format!("именно поэтому {}", synthesis));
                }
            }
        }

        if !countering.is_empty() {
            let counter_texts: Vec<String> =
                countering.iter().map(|e| crate::seed::verbalize_relation(e)).collect();
            parts.push(format!("но при этом {}", counter_texts.join(". ")));
        }

        if parts.is_empty() {
            // No graph basis for defense — acknowledge from structure
            if let Some(sv) = sense_vectors.first() {
                parts.push(format!(
                    "возможно, но моя позиция по {} требует осмысления через граф",
                    sv.atom_id.as_str()
                ));
            }
        }

        // Add reverse edges as additional context
        for edge in &reverse_edges {
            parts.push(crate::seed::verbalize_relation(edge));
        }

        let text = parts.join(". ");
        let all_edges: Vec<Relation> = cv
            .edges
            .iter()
            .chain(reverse_edges.iter())
            .cloned()
            .collect();

        let provenance: Vec<_> = all_edges.iter().map(|e| e.source).collect();

        GeneratedSurface {
            text,
            paths: vec![PathProof {
                edges: all_edges,
                topic: sense_vectors
                    .first()
                    .map(|v| v.atom_id.as_str().to_string())
                    .unwrap_or_default(),
            }],
            provenance,
            depth_score: cv.depth() as f64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed_graph;
    use crate::sense_decomposer::SenseDecomposer;

    #[test]
    fn test_conjugate_compose_basic() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода", &graph);
        assert!(!vectors.is_empty());
        let surface = ConjugateComposer::compose(&graph, &vectors);
        assert!(!surface.text.is_empty());
        // Avoid canned openings like "Когда я думаю о"
        assert!(!surface.text.contains("Когда я думаю о"));
        assert!(!surface.text.contains("Я вижу это так:"));
    }

    #[test]
    fn test_conjugate_compose_sentence() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("что такое свобода", &graph);
        let surface = ConjugateComposer::compose(&graph, &vectors);
        assert!(!surface.text.is_empty());
        // Should contain graph-derived content about свобода
        assert!(surface.text.contains("свобода") || surface.text.contains("выбор"));
    }

    #[test]
    fn test_conjugate_challenge_mode() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода это просто вседозволенность", &graph);
        let surface = ConjugateComposer::compose_with_challenge(&graph, &vectors, true);
        assert!(!surface.text.is_empty());
        // Challenge mode should include defense language from graph structure
        assert!(surface.text.contains("свобода") || surface.text.contains("предполагает"));
    }

    #[test]
    fn test_conjugate_no_template_phrases() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("истина", &graph);
        let surface = ConjugateComposer::compose(&graph, &vectors);
        // Verify NO template phrases are present
        assert!(!surface.text.contains("Когда я думаю о"));
        assert!(!surface.text.contains("Я вижу это так:"));
        assert!(!surface.text.contains("Я вижу это иначе"));
        assert!(!surface.text.contains("Связь прослеживается:"));
    }

    #[test]
    fn test_conjugate_empty_input() {
        let graph = seed_graph();
        let surface = ConjugateComposer::compose(&graph, &[]);
        // BUG 2 fix: empty input now returns minimal semantic response, not empty string
        assert!(!surface.text.is_empty());
        assert!(surface.text.contains("не знаю") || surface.text.contains("unknown") || !surface.text.is_empty());
    }

    #[test]
    fn test_conjugate_resonance() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода", &graph);
        let cv = ConjugateComposer::traverse(&graph, &vectors);
        assert!(cv.resonance >= 0.0);
        assert!(!cv.edges.is_empty());
    }

    #[test]
    fn test_conjugate_unique_output() {
        let graph = seed_graph();
        let v1 = SenseDecomposer::decompose("свобода", &graph);
        let v2 = SenseDecomposer::decompose("истина", &graph);
        let s1 = ConjugateComposer::compose(&graph, &v1);
        let s2 = ConjugateComposer::compose(&graph, &v2);
        // Different inputs should produce different outputs
        assert_ne!(s1.text, s2.text);
    }

    #[test]
    fn test_conjugate_includes_rationale() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода", &graph);
        let surface = ConjugateComposer::compose(&graph, &vectors);
        // Should include rationale from graph structure if present
        // (свобода has rationale on its edges)
        assert!(
            surface.text.contains("потому что") || surface.text.contains("предполагает"),
            "Should include graph-derived rationale, got: {}",
            surface.text
        );
    }
}

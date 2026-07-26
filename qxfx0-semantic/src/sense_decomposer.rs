use qxfx0_morphology::MorphologyData;
use qxfx0_types::atom::{AtomGraph, AtomId, SenseVector};
use qxfx0_types::RelationType;
use std::collections::BTreeMap;
use std::sync::OnceLock;

// Re-export PropositionParser for backward compatibility
pub use crate::composer::PropositionParser;

static MORPHOLOGY: OnceLock<MorphologyData> = OnceLock::new();

/// Vendored cosine similarity (replaces external context_engine dependency).
/// Tokenizes both strings into lowercase word-frequency vectors (BTreeMap for
/// deterministic iteration) and computes the cosine of the angle between them.
/// Returns a value in [0.0, 1.0].
fn cosine_similarity(query: &str, content: &str) -> f64 {
    let tokenize = |text: &str| -> Vec<String> {
        text.split_whitespace()
            .map(|w| {
                w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_')
                    .to_lowercase()
            })
            .filter(|w| !w.is_empty())
            .collect()
    };
    let qt = tokenize(query);
    let ct = tokenize(content);
    if qt.is_empty() || ct.is_empty() {
        return 0.0;
    }
    let freq = |tokens: &[String]| -> BTreeMap<String, f64> {
        let mut f = BTreeMap::new();
        for t in tokens {
            *f.entry(t.clone()).or_insert(0.0) += 1.0;
        }
        f
    };
    let v1 = freq(&qt);
    let v2 = freq(&ct);
    let mut dot = 0.0;
    let mut norm1 = 0.0;
    for (word, &f1) in &v1 {
        norm1 += f1 * f1;
        if let Some(&f2) = v2.get(word) {
            dot += f1 * f2;
        }
    }
    let norm2: f64 = v2.values().map(|f| f * f).sum();
    if norm1 == 0.0 || norm2 == 0.0 {
        return 0.0;
    }
    dot / (norm1.sqrt() * norm2.sqrt())
}

/// SenseDecomposer — decomposes input text into a vector field of SenseVectors.
///
/// Each word/phrase is projected onto the graph (nearest atoms + weights).
/// The sentence becomes a superposition of vectors, not a single mode.
pub struct SenseDecomposer;

impl SenseDecomposer {
    /// Decompose input text into a vector of SenseVectors.
    /// Each word maps to the nearest atom(s) in the graph with a weight.
    pub fn decompose(input: &str, graph: &AtomGraph) -> Vec<SenseVector> {
        let lower = input.to_lowercase();
        let words: Vec<&str> = lower.split_whitespace().collect();
        let mut vectors: Vec<SenseVector> = Vec::new();
        let mut seen_atoms: std::collections::BTreeSet<AtomId> = std::collections::BTreeSet::new();

        // Initialize morphology data once for the lifetime of the program
        let morphology = MORPHOLOGY.get_or_init(MorphologyData::new);

        for (i, word) in words.iter().enumerate() {
            let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
            if cleaned.is_empty() {
                continue;
            }

            // Find matching atoms: exact match, lemmatized match, substring, or contains
            let matches = Self::project_word(cleaned, graph, morphology);

            for (atom_id, base_weight) in matches {
                // Weight decays with word position and distance
                let position_factor = 1.0 / (1.0 + i as f64 * 0.1);
                let weight = base_weight * position_factor;

                // Build relation vector from this atom's edges
                let relation_vector = Self::relation_vector_for(&atom_id, graph);

                if seen_atoms.contains(&atom_id) {
                    // Merge: increase weight of existing vector
                    if let Some(existing) = vectors.iter_mut().find(|v| v.atom_id == atom_id) {
                        existing.weight += weight;
                    }
                } else {
                    seen_atoms.insert(atom_id.clone());
                    vectors.push(SenseVector {
                        atom_id: atom_id.clone(),
                        weight,
                        relation_vector,
                    });
                }
            }
        }

        // Normalize weights
        let total: f64 = vectors.iter().map(|v| v.weight).sum();
        if total > 0.0 {
            for v in vectors.iter_mut() {
                v.weight /= total;
            }
        }

        vectors
    }

    /// Project a single word onto the graph — find nearest atoms with weights.
    fn project_word(
        word: &str,
        graph: &AtomGraph,
        morphology: &MorphologyData,
    ) -> Vec<(AtomId, f64)> {
        let mut results = Vec::new();
        let word_lower = word.to_lowercase();

        // 0. Direct lookup by AtomId key
        let direct_id = AtomId::new(word_lower.clone());
        if graph.atoms.contains_key(&direct_id) {
            results.push((direct_id.clone(), 1.0));
        }

        // 1. Lemmatization lookup
        // If the word is inflected, resolve it to its nominative form and check the graph
        let lemma = morphology.lemmatize(&word_lower);
        if lemma != word_lower {
            let lemma_id = AtomId::new(lemma.clone());
            if graph.atoms.contains_key(&lemma_id) {
                results.push((lemma_id, 1.0));
            }
        }

        // 2. Case-insensitive lookup across all AtomIds and displays
        if results.is_empty() {
            for (id, atom) in &graph.atoms {
                if id.as_str().to_lowercase() == word_lower
                    || atom.display.to_lowercase() == word_lower
                {
                    results.push((id.clone(), 1.0));
                }
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 3. Atom display contains word or word contains atom display — weight 0.7
        for (id, atom) in &graph.atoms {
            let display = atom.display.to_lowercase();
            if (display.contains(&word_lower) || word_lower.contains(&display))
                && display.chars().count() >= 3
                && word_lower.chars().count() >= 3
            {
                results.push((id.clone(), 0.7));
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 4. Check object_text in relations — weight 0.5
        for rel in &graph.edges {
            let obj = rel.object_text.to_lowercase();
            if (obj.contains(&word_lower) || word_lower.contains(&obj))
                && word_lower.chars().count() >= 3
                && obj.chars().count() >= 3
                && !results.iter().any(|(id, _)| id == &rel.to)
            {
                results.push((rel.to.clone(), 0.5));
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 5. Fuzzy: check if word shares a stem (first 4+ chars) with any atom
        let word_chars: Vec<char> = word_lower.chars().collect();
        if word_chars.len() >= 4 {
            let stem: String = word_chars.iter().take(5).collect();
            for (id, atom) in &graph.atoms {
                let display = atom.display.to_lowercase();
                if display.chars().count() >= 4 && display.starts_with(&stem) {
                    results.push((id.clone(), 0.3));
                }
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 6. Cosine similarity fallback
        let word_len = word_lower.chars().count();
        let min_len = (word_len / 2).max(2);
        let max_len = word_len * 2;
        for (id, atom) in &graph.atoms {
            let display_len = atom.display.chars().count();
            if display_len < min_len || display_len > max_len {
                continue;
            }
            let sim = cosine_similarity(&word_lower, &atom.display);
            if sim > 0.3 {
                results.push((id.clone(), sim * 0.6));
            }
        }
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(5);

        results
    }

    /// Build relation vector for an atom — (RelationType, weight) pairs.
    fn relation_vector_for(atom_id: &AtomId, graph: &AtomGraph) -> Vec<(RelationType, f64)> {
        let rels = graph.relations_from(atom_id);
        let mut rv: Vec<(RelationType, f64)> = Vec::new();

        for rel in rels {
            if let Some(existing) = rv.iter_mut().find(|(rt, _)| rt == &rel.rel_type) {
                existing.1 += 1.0;
            } else {
                rv.push((rel.rel_type, 1.0));
            }
        }

        let total: f64 = rv.iter().map(|(_, w)| w).sum();
        if total > 0.0 {
            for (_, w) in rv.iter_mut() {
                *w /= total;
            }
        }

        rv
    }

    /// Superpose multiple sense vectors into a single weighted field.
    pub fn superpose(vectors: &[SenseVector]) -> Vec<(AtomId, f64)> {
        let mut map: BTreeMap<AtomId, f64> = BTreeMap::new();

        for v in vectors {
            *map.entry(v.atom_id.clone()).or_insert(0.0) += v.weight;
        }

        let mut superposition: Vec<(AtomId, f64)> = map.into_iter().collect();
        superposition.sort_by(|a, b| b.1.total_cmp(&a.1));
        superposition
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seed_graph;

    #[test]
    fn test_decompose_single_word() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода", &graph);
        assert!(!vectors.is_empty());
        assert!(vectors.iter().any(|v| v.atom_id.as_str() == "свобода"));
    }

    #[test]
    fn test_decompose_pamyat() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("память", &graph);
        assert!(
            !vectors.is_empty(),
            "decompose(память) should produce vectors"
        );
        assert!(
            vectors.iter().any(|v| v.atom_id.as_str() == "память"),
            "decompose(память) should find the память atom"
        );
    }

    #[test]
    fn test_decompose_pamyat_uppercase() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("Память", &graph);
        assert!(
            !vectors.is_empty(),
            "decompose(Память) should produce vectors"
        );
        assert!(
            vectors.iter().any(|v| v.atom_id.as_str() == "память"),
            "decompose(Память) should find the память atom (case-insensitive)"
        );
    }

    #[test]
    fn test_decompose_sentence() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("что такое свобода", &graph);
        assert!(!vectors.is_empty());
        assert!(vectors.iter().any(|v| v.atom_id.as_str() == "свобода"));
    }

    #[test]
    fn test_decompose_multiple_atoms() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода и ответственность", &graph);
        let has_svoboda = vectors.iter().any(|v| v.atom_id.as_str() == "свобода");
        let has_otvetstvennost = vectors
            .iter()
            .any(|v| v.atom_id.as_str() == "ответственность");
        assert!(has_svoboda || has_otvetstvennost);
    }

    #[test]
    fn test_decompose_unknown_word() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("абракадабра", &graph);
        assert!(vectors.is_empty() || vectors.len() <= 2);
    }

    #[test]
    fn test_relation_vector_for_atom() {
        let graph = seed_graph();
        let rv = SenseDecomposer::relation_vector_for(&AtomId::new("свобода"), &graph);
        assert!(!rv.is_empty());
        assert!(rv.iter().any(|(rt, _)| *rt == RelationType::RelPresupposes));
    }

    #[test]
    fn test_superpose() {
        let vectors = vec![
            SenseVector {
                atom_id: AtomId::new("свобода"),
                weight: 0.6,
                relation_vector: vec![(RelationType::RelPresupposes, 1.0)],
            },
            SenseVector {
                atom_id: AtomId::new("ответственность"),
                weight: 0.4,
                relation_vector: vec![(RelationType::RelRequires, 1.0)],
            },
        ];
        let sup = SenseDecomposer::superpose(&vectors);
        assert_eq!(sup.len(), 2);
        assert!(sup[0].1 >= sup[1].1);
    }

    #[test]
    fn test_decompose_weights_normalized() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода ответственность", &graph);
        if vectors.len() > 1 {
            let total: f64 = vectors.iter().map(|v| v.weight).sum();
            assert!(
                (total - 1.0).abs() < 0.01,
                "Weights should sum to 1.0, got {}",
                total
            );
        }
    }

    #[test]
    fn test_decompose_fuzzy_match() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобод", &graph);
        assert!(vectors.iter().any(|v| v.atom_id.as_str() == "свобода"));
    }
}

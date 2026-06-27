use qxfx0_types::atom::{AtomGraph, AtomId, SenseVector};
use qxfx0_types::RelationType;
use std::collections::BTreeMap;

// Re-export PropositionParser for backward compatibility
pub use crate::composer::PropositionParser;

/// Vendored cosine similarity (replaces external context_engine dependency).
/// Tokenizes both strings into lowercase word-frequency vectors (BTreeMap for
/// deterministic iteration) and computes the cosine of the angle between them.
/// Returns a value in [0.0, 1.0].
fn cosine_similarity(query: &str, content: &str) -> f64 {
    let tokenize = |text: &str| -> Vec<String> {
        text.split_whitespace()
            .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_lowercase())
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
        let mut seen_atoms: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();

        for (i, word) in words.iter().enumerate() {
            let cleaned = word.trim_matches(|c: char| !c.is_alphanumeric());
            if cleaned.is_empty() {
                continue;
            }

            // Find matching atoms: exact match, substring, or contains
            let matches = Self::project_word(cleaned, graph);

            for (atom_id, base_weight) in matches {
                // Weight decays with word position and distance
                let position_factor = 1.0 / (1.0 + i as f64 * 0.1);
                let weight = base_weight * position_factor;

                // Build relation vector from this atom's edges
                let relation_vector = Self::relation_vector_for(&atom_id, graph);

                let key = atom_id.as_str().to_string();
                if seen_atoms.contains(&key) {
                    // Merge: increase weight of existing vector
                    if let Some(existing) = vectors.iter_mut().find(|v| v.atom_id == atom_id) {
                        existing.weight += weight;
                    }
                } else {
                    seen_atoms.insert(key);
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
    fn project_word(word: &str, graph: &AtomGraph) -> Vec<(AtomId, f64)> {
        let mut results = Vec::new();

        // 0. Direct lookup by AtomId key — fastest and most reliable.
        // The graph stores atoms keyed by AtomId, and seed.rs inserts atoms
        // with AtomId::new(topic) where topic is already lowercase.
        // This handles the case where display might differ from the key.
        let direct_id = AtomId::new(word.to_string());
        if let Some(atom) = graph.atoms.get(&direct_id) {
            // Verify the atom display also matches (case-insensitive) to avoid
            // false positives from hash collisions or normalization differences.
            if atom.display.to_lowercase() == word {
                results.push((direct_id.clone(), 1.0));
            }
        }
        // Also try looking up by the AtomId string directly (id.as_str())
        // in case AtomId stores a different form than display.
        if results.is_empty() {
            for id in graph.atoms.keys() {
                if id.as_str().to_lowercase() == word {
                    results.push((id.clone(), 1.0));
                }
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 1. Exact match on display — weight 1.0
        for (id, atom) in &graph.atoms {
            if atom.display.to_lowercase() == word {
                results.push((id.clone(), 1.0));
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 2. Atom display contains word or word contains atom display — weight 0.7
        for (id, atom) in &graph.atoms {
            let display = atom.display.to_lowercase();
            if (display.contains(word) || word.contains(display.as_str()))
                && display.chars().count() >= 3
                && word.chars().count() >= 3
            {
                results.push((id.clone(), 0.7));
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 3. Check object_text in relations — weight 0.5
        for rel in &graph.edges {
            let obj = rel.object_text.to_lowercase();
            if (obj.contains(word) || word.contains(obj.as_str()))
                && !results.iter().any(|(id, _)| id == &rel.to)
            {
                results.push((rel.to.clone(), 0.5));
            }
        }

        if !results.is_empty() {
            return results;
        }

        // 4. Fuzzy: check if word shares a stem (first 4+ chars) with any atom
        let word_chars: Vec<char> = word.chars().collect();
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

        // 5. Cosine similarity fallback via context-engine scorer.
        // Treats the word as a query and each atom display as a candidate.
        // Length pre-filter: skip atoms whose display length is wildly different
        // from the word (cosine_similarity on very different-length strings is
        // unlikely to exceed the 0.3 threshold), reducing O(N) cosine calls.
        let word_len = word.chars().count();
        let min_len = (word_len / 2).max(2);
        let max_len = word_len * 2;
        for (id, atom) in &graph.atoms {
            let display_len = atom.display.chars().count();
            if display_len < min_len || display_len > max_len {
                continue;
            }
            let sim = cosine_similarity(word, &atom.display);
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
            // Weight by edge presence (1.0 per edge, could be enhanced)
            if let Some(existing) = rv.iter_mut().find(|(rt, _)| rt == &rel.rel_type) {
                existing.1 += 1.0;
            } else {
                rv.push((rel.rel_type, 1.0));
            }
        }

        // Normalize
        let total: f64 = rv.iter().map(|(_, w)| w).sum();
        if total > 0.0 {
            for (_, w) in rv.iter_mut() {
                *w /= total;
            }
        }

        rv
    }

    /// Superpose multiple sense vectors into a single weighted field.
    /// Returns a map of atom_id -> total weight (the superposition).
    pub fn superpose(vectors: &[SenseVector]) -> Vec<(AtomId, f64)> {
        let mut map: BTreeMap<AtomId, f64> = BTreeMap::new();

        for v in vectors {
            *map.entry(v.atom_id.clone()).or_insert(0.0) += v.weight;
        }

        let mut superposition: Vec<(AtomId, f64)> = map.into_iter().collect();
        // Sort by weight descending
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
        assert!(!vectors.is_empty(), "decompose(память) should produce vectors");
        assert!(
            vectors.iter().any(|v| v.atom_id.as_str() == "память"),
            "decompose(память) should find the память atom"
        );
    }

    #[test]
    fn test_decompose_pamyat_uppercase() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("Память", &graph);
        assert!(!vectors.is_empty(), "decompose(Память) should produce vectors");
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
        // Should find свобода
        assert!(vectors.iter().any(|v| v.atom_id.as_str() == "свобода"));
    }

    #[test]
    fn test_decompose_multiple_atoms() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода и ответственность", &graph);
        // Should find both свобода and ответственность
        let has_svoboda = vectors.iter().any(|v| v.atom_id.as_str() == "свобода");
        let has_otvetstvennost =
            vectors.iter().any(|v| v.atom_id.as_str() == "ответственность");
        assert!(has_svoboda || has_otvetstvennost);
    }

    #[test]
    fn test_decompose_unknown_word() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("абракадабра", &graph);
        // Unknown word should produce empty or minimal vectors
        // (no match in graph)
        assert!(vectors.is_empty() || vectors.len() <= 2);
    }

    #[test]
    fn test_relation_vector_for_atom() {
        let graph = seed_graph();
        let rv = SenseDecomposer::relation_vector_for(&AtomId::new("свобода"), &graph);
        assert!(!rv.is_empty());
        // свобода has RelPresupposes, RelLimitedBy, RelDetermines, RelRequires, RelContrastsWith
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
        // Sorted by weight descending
        assert!(sup[0].1 >= sup[1].1);
    }

    #[test]
    fn test_decompose_weights_normalized() {
        let graph = seed_graph();
        let vectors = SenseDecomposer::decompose("свобода ответственность", &graph);
        if vectors.len() > 1 {
            let total: f64 = vectors.iter().map(|v| v.weight).sum();
            assert!((total - 1.0).abs() < 0.01, "Weights should sum to 1.0, got {}", total);
        }
    }

    #[test]
    fn test_decompose_fuzzy_match() {
        let graph = seed_graph();
        // "свобод" is a stem of "свобода"
        let vectors = SenseDecomposer::decompose("свобод", &graph);
        assert!(vectors.iter().any(|v| v.atom_id.as_str() == "свобода"));
    }
}

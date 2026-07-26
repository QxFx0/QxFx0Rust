use qxfx0_types::atom::{AtomGraph, AtomId, GeneratedSurface, PathProof, Relation};
use qxfx0_types::field::FieldProfile;
use qxfx0_types::RelationType;
use std::collections::BTreeSet;

/// Deduplication key for length-3 paths (3 edges = 9 fields).
/// Extracted from a 9-tuple to satisfy clippy::type_complexity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct PathKey {
    e1_from: AtomId,
    e1_rt: RelationType,
    e1_to: AtomId,
    e2_from: AtomId,
    e2_rt: RelationType,
    e2_to: AtomId,
    e3_from: AtomId,
    e3_rt: RelationType,
    e3_to: AtomId,
}

/// Path finder — graph traversal with field-biased ranking.
/// Finds admissible paths through the AtomStore relation graph.
pub struct PathFinder;

impl PathFinder {
    /// Find paths from an atom up to max_len.
    /// Length 1: topic → relation → object
    /// Length 2: topic → rel1 → intermediate → rel2 → object
    /// Length 3: topic → rel1 → mid1 → rel2 → mid2 → rel3 → object
    pub fn find_paths(graph: &AtomGraph, max_len: usize, start: &AtomId) -> Vec<RankedPath> {
        let mut paths = Vec::new();

        // Length 1: topic → relation → object
        for rel in graph.relations_from(start) {
            paths.push(RankedPath {
                proof: PathProof {
                    edges: vec![rel.clone()],
                    topic: start.as_str().to_string(),
                },
                score: Self::score_path(&FieldProfile::default(), std::slice::from_ref(rel)),
            });
        }

        if max_len >= 2 {
            // Length 2: start → e1 → mid → e2 → obj
            for e1 in graph.relations_from(start) {
                let mid = &e1.to;
                for e2 in graph.relations_from(mid) {
                    if e2.to != *start {
                        paths.push(RankedPath {
                            proof: PathProof {
                                edges: vec![e1.clone(), e2.clone()],
                                topic: start.as_str().to_string(),
                            },
                            score: Self::score_path(
                                &FieldProfile::default(),
                                &[e1.clone(), e2.clone()],
                            ),
                        });
                    }
                }
            }
        }

        if max_len >= 3 {
            // Length 3: start → e1 → mid1 → e2 → mid2 → e3 → obj
            // Use BTreeSet per path to prevent cycles and dedup by edge triple.
            let mut seen_paths: BTreeSet<PathKey> = BTreeSet::new();
            for e1 in graph.relations_from(start) {
                let mid1 = &e1.to;
                if mid1 == start {
                    continue;
                }
                for e2 in graph.relations_from(mid1) {
                    let mid2 = &e2.to;
                    // Prevent mid2 from revisiting start or mid1 (cycle)
                    if mid2 == start || mid2 == mid1 {
                        continue;
                    }
                    let mut visited = BTreeSet::new();
                    visited.insert(start.clone());
                    visited.insert(mid1.clone());
                    visited.insert(mid2.clone());
                    for e3 in graph.relations_from(mid2) {
                        if visited.contains(&e3.to) {
                            continue;
                        }
                        let key = PathKey {
                            e1_from: e1.from.clone(),
                            e1_rt: e1.rel_type,
                            e1_to: e1.to.clone(),
                            e2_from: e2.from.clone(),
                            e2_rt: e2.rel_type,
                            e2_to: e2.to.clone(),
                            e3_from: e3.from.clone(),
                            e3_rt: e3.rel_type,
                            e3_to: e3.to.clone(),
                        };
                        if !seen_paths.insert(key) {
                            continue;
                        }
                        paths.push(RankedPath {
                            proof: PathProof {
                                edges: vec![e1.clone(), e2.clone(), e3.clone()],
                                topic: start.as_str().to_string(),
                            },
                            score: Self::score_path(
                                &FieldProfile::default(),
                                &[e1.clone(), e2.clone(), e3.clone()],
                            ),
                        });
                    }
                }
            }
        }

        paths
    }

    /// Score a path: sum of edge biases minus length penalty.
    pub fn score_path(fp: &FieldProfile, edges: &[Relation]) -> PathScore {
        let bias_sum: f64 = edges
            .iter()
            .map(|e| Self::relation_type_bias(fp, e.rel_type))
            .sum();
        let penalty = edges.len() as f64 * 0.15;
        let total = bias_sum - penalty;
        PathScore {
            bias: bias_sum,
            length_penalty: penalty,
            total,
        }
    }

    /// Map relation type to bias score given the Field profile.
    pub fn relation_type_bias(fp: &FieldProfile, rt: RelationType) -> f64 {
        let conf = fp.confidence;
        let cf = fp.counterfactual;
        let consolid = fp.consolidation;
        let reson = fp.resonance;

        match rt {
            // Confidence-favored
            RelationType::RelClaims => conf * 0.8,
            RelationType::RelVerifiedBy => conf * 0.9,
            RelationType::RelMeans => conf * 0.6,
            RelationType::RelDenotes => conf * 0.5,
            RelationType::RelDetermines => conf * 0.7,

            // Counterfactual-favored
            RelationType::RelDiffersFrom => cf * 0.9,
            RelationType::RelContrastsWith => cf * 0.8,
            RelationType::RelNotReducibleTo => cf * 0.7,
            RelationType::RelIsNot => cf * 0.6,
            RelationType::RelNegates => cf * 0.8,
            RelationType::RelDestroys => cf * 0.5,

            // Consolidation-favored
            RelationType::RelPresupposes => consolid * 0.7,
            RelationType::RelRequires => consolid * 0.8,
            RelationType::RelLimitedBy => consolid * 0.6,
            RelationType::RelStructures => consolid * 0.7,
            RelationType::RelIncludes => consolid * 0.5,
            RelationType::RelNecessaryFor => consolid * 0.6,

            // Resonance-favored
            RelationType::RelSignals => reson * 0.8,
            RelationType::RelExpresses => reson * 0.7,
            RelationType::RelEvokes => reson * 0.6,
            RelationType::RelSays => reson * 0.5,
            RelationType::RelGives => reson * 0.5,
            RelationType::RelReveals => reson * 0.6,
            RelationType::RelPointsTo => reson * 0.5,

            // Transformation
            RelationType::RelTransformsInto => 0.4 * cf.max(reson),
            RelationType::RelTransforms => 0.4 * consolid.max(reson),
            RelationType::RelCreatedFrom => 0.3 * consolid,

            // Direction/action
            RelationType::RelDirectedAt => 0.4 * consolid,
            RelationType::RelOrientsToward => 0.3 * reson,
            RelationType::RelPrescribes => 0.4 * consolid,
            RelationType::RelBuiltThrough => 0.3 * consolid,
            RelationType::RelCapableOf => 0.3 * conf,
            RelationType::RelCanBe => 0.2,
            RelationType::RelMakes => 0.3,
            RelationType::RelRecognizes => 0.3 * reson,
            RelationType::RelUnifies => 0.3 * consolid,
            RelationType::RelConnects => 0.3 * consolid,
            RelationType::RelDependsOn => 0.3 * consolid,
            RelationType::RelSupports => 0.3 * consolid,
            RelationType::RelSets => 0.3 * consolid,
            RelationType::RelPreserves => 0.3 * consolid,
            RelationType::RelReconstructs => 0.3 * consolid,
            RelationType::RelIsA => 0.2,
            RelationType::RelNotJustCopies => 0.2,
            RelationType::RelRelatedTo => 0.2,
            RelationType::RelPrecedes => 0.4,
            RelationType::RelReliesOn => 0.5 * consolid,
        }
    }

    /// Rank paths by score (highest first). Deterministic: ties broken
    /// by first edge's ru_original (alphabetical).
    pub fn rank_paths(paths: Vec<RankedPath>) -> Vec<RankedPath> {
        let mut sorted = paths;
        sorted.sort_by(|a, b| {
            let a_bonus = a
                .proof
                .edges
                .first()
                .map(|e| if e.rationale.is_some() { 2.0 } else { 0.0 })
                .unwrap_or(0.0);
            let b_bonus = b
                .proof
                .edges
                .first()
                .map(|e| if e.rationale.is_some() { 2.0 } else { 0.0 })
                .unwrap_or(0.0);
            let a_total = a.score.total + a_bonus;
            let b_total = b.score.total + b_bonus;

            b_total.total_cmp(&a_total).then_with(|| {
                let a_key = a
                    .proof
                    .edges
                    .first()
                    .map(|e| e.ru_original.as_str())
                    .unwrap_or("");
                let b_key = b
                    .proof
                    .edges
                    .first()
                    .map(|e| e.ru_original.as_str())
                    .unwrap_or("");
                a_key.cmp(b_key)
            })
        });
        sorted
    }

    /// Select top-N paths after ranking.
    pub fn select_top_paths(
        n: usize,
        _fp: &FieldProfile,
        graph: &AtomGraph,
        start: &AtomId,
        max_len: usize,
    ) -> Vec<RankedPath> {
        let paths = Self::find_paths(graph, max_len, start);
        let ranked = Self::rank_paths(paths);
        ranked.into_iter().take(n).collect()
    }

    /// Compose a definition for a topic using a specific graph.
    pub fn compose_definition(
        graph: &AtomGraph,
        fp: &FieldProfile,
        n: usize,
        topic: &AtomId,
    ) -> GeneratedSurface {
        // CF-4 fix: use conatus-driven path depth instead of hardcoded 1
        let depth = fp.path_depth();
        let paths = Self::select_top_paths(n, fp, graph, topic, depth);

        if paths.is_empty() {
            return GeneratedSurface {
                text: String::new(),
                paths: Vec::new(),
                provenance: Vec::new(),
                depth_score: 0.0,
            };
        }

        // Build text from paths
        let mut texts = Vec::new();
        let mut all_proofs = Vec::new();
        let mut all_sources = Vec::new();

        for rp in &paths {
            let main_text = crate::verbalize_path(&rp.proof);

            // Find rationale: length-2 path from object
            let obj = rp.proof.edges.last().map(|e| e.to.clone());
            let rationale_text = if let Some(obj_id) = &obj {
                let rationale_paths = Self::select_top_paths(3, fp, graph, obj_id, 2);
                if let Some(rp2) = rationale_paths.first() {
                    crate::verbalize_path(&rp2.proof)
                } else {
                    String::new()
                }
            } else {
                String::new()
            };

            // Find counter-paths
            let counter_text = graph
                .relations_from(topic)
                .iter()
                .filter(|r| r.rel_type.is_counter())
                .map(|r| crate::verbalize_relation(r))
                .collect::<Vec<_>>()
                .join(". ");

            // Build synthesis from rationale
            let first_edge = rp.proof.edges.first();
            let synthesis_text = first_edge
                .and_then(|e| e.synthesis.as_ref())
                .cloned()
                .unwrap_or_default();

            let mut full_text = main_text.clone();
            if !rationale_text.is_empty() {
                full_text.push_str(". Потому что ");
                full_text.push_str(&rationale_text);
            }
            if !counter_text.is_empty() {
                full_text.push_str(". Но ");
                full_text.push_str(&counter_text);
            }
            if !synthesis_text.is_empty() {
                full_text.push_str(". Именно поэтому ");
                full_text.push_str(&synthesis_text);
            }

            texts.push(full_text);
            all_proofs.push(rp.proof.clone());
            all_sources.extend(rp.proof.edges.iter().map(|e| e.source));
        }

        let full_text = texts.join(". ") + ".";

        GeneratedSurface {
            text: full_text,
            paths: all_proofs,
            provenance: all_sources,
            depth_score: paths.len() as f64,
        }
    }
}

#[derive(Debug, Clone)]
pub struct RankedPath {
    pub proof: PathProof,
    pub score: PathScore,
}

#[derive(Debug, Clone, Copy)]
pub struct PathScore {
    pub bias: f64,
    pub length_penalty: f64,
    pub total: f64,
}

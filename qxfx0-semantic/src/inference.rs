//! DeriveAtoms — rule-based inference for creating new atoms from
//! existing pattern combinations. WP-G: default-on promotion flag.
//!
//! Three inference rules:
//!   1. Contact under stress: NeedContact + Exhaustion → amplified NeedContact
//!   2. Contradiction under doubt: Contradiction + Doubt → amplified Contradiction
//!   3. Agency lost while searching: AgencyLost + Searching → exhaustion marker

use qxfx0_types::atom::AtomId;
use qxfx0_types::atom::{Relation, RelationSource};
use qxfx0_types::RelationType;

/// Atom tags that can be detected from system state.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum AtomTag {
    Searching(String),
    Exhaustion(String),
    Verification(String),
    Doubt(String),
    NeedContact(String),
    NeedMeaning(String),
    AgencyLost(String),  // conatus energy as string
    AgencyFound(String), // conatus energy as string
    Anchoring(String),
    Contradiction(String, String),
    CustomAtom(String, String),
    AffectiveAtom(String, String),
}

/// A derived atom produced by inference rules.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct DerivedAtom {
    pub id: AtomId,
    pub tag: AtomTag,
    pub rule: DeriveRule,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DeriveRule {
    ContactUnderStress,
    ContradictionUnderDoubt,
    AgencySearchExhaustion,
}

/// Derive additional atoms from existing atom tags via multi-step patterns.
pub fn derive_atoms(tags: &[AtomTag]) -> Vec<DerivedAtom> {
    let mut result = Vec::new();

    let has_need_contact = tags.iter().any(|t| matches!(t, AtomTag::NeedContact(_)));
    let has_exhaustion = tags.iter().any(|t| matches!(t, AtomTag::Exhaustion(_)));
    let has_contradiction = tags
        .iter()
        .any(|t| matches!(t, AtomTag::Contradiction(_, _)));
    let has_doubt = tags.iter().any(|t| matches!(t, AtomTag::Doubt(_)));
    let has_agency_lost = tags.iter().any(|t| matches!(t, AtomTag::AgencyLost(_)));
    let has_searching = tags.iter().any(|t| matches!(t, AtomTag::Searching(_)));

    if has_need_contact && has_exhaustion {
        result.push(DerivedAtom {
            id: AtomId::new("derived_contact_stress"),
            tag: AtomTag::NeedContact("stressed".into()),
            rule: DeriveRule::ContactUnderStress,
        });
    }

    if has_contradiction && has_doubt {
        result.push(DerivedAtom {
            id: AtomId::new("derived_contradiction_doubt"),
            tag: AtomTag::Contradiction("amplified".into(), "amplified".into()),
            rule: DeriveRule::ContradictionUnderDoubt,
        });
    }

    if has_agency_lost && has_searching {
        result.push(DerivedAtom {
            id: AtomId::new("derived_agency_search"),
            tag: AtomTag::Exhaustion("search_exhausted".into()),
            rule: DeriveRule::AgencySearchExhaustion,
        });
    }

    result
}

/// Relation types supporting transitive chaining (Haskell
/// `transitiveRelationTypes`, minus the five with no Rust vocabulary:
/// Enables, Causes, Influences, PartOf, Opposes-is-symmetric-only).
/// A → B and B → C (same type) ⇒ A → C.
pub const TRANSITIVE_TYPES: &[RelationType] = &[
    RelationType::RelRequires,
    RelationType::RelNecessaryFor,
    RelationType::RelPresupposes,
    RelationType::RelDependsOn,
    RelationType::RelIncludes,
    RelationType::RelIsA,
    RelationType::RelStructures,
    RelationType::RelDetermines,
    RelationType::RelOrientsToward,
    RelationType::RelPrecedes,
];

/// Relation types that are symmetric (Haskell `symmetricRelationTypes`,
/// minus RelOpposes which has no Rust vocabulary): A → B ⇒ B → A.
pub const SYMMETRIC_TYPES: &[RelationType] = &[
    RelationType::RelContrastsWith,
    RelationType::RelDiffersFrom,
    RelationType::RelNegates,
    RelationType::RelNotReducibleTo,
    RelationType::RelDestroys,
];

/// Fixpoint bound: at most this many inference rounds per call.
pub const INFERENCE_MAX_ITERATIONS: usize = 5;
/// Cap on new edges per call: inference enriches, never floods.
pub const INFERENCE_MAX_NEW_EDGES: usize = 128;

fn edge_key(edge: &Relation) -> (AtomId, AtomId, RelationType) {
    (edge.from.clone(), edge.to.clone(), edge.rel_type)
}

fn transitive_step(
    known: &std::collections::BTreeSet<(AtomId, AtomId, RelationType)>,
    edges: &[Relation],
) -> Vec<Relation> {
    let mut out = Vec::new();
    for first in edges {
        if !TRANSITIVE_TYPES.contains(&first.rel_type) {
            continue;
        }
        for second in edges {
            if second.rel_type != first.rel_type || second.from != first.to {
                continue;
            }
            if second.to == first.from {
                continue; // no self-loops: A → B → A derives nothing
            }
            let key = (first.from.clone(), second.to.clone(), first.rel_type);
            if known.contains(&key) {
                continue; // existing edges win on conflict
            }
            out.push(Relation {
                from: first.from.clone(),
                to: second.to.clone(),
                rel_type: first.rel_type,
                object_case: second.object_case,
                object_text: second.object_text.clone(),
                verb_override: None,
                ru_original: format!(
                    "[выведено: {} —{:?}→ {}]",
                    first.from.as_str(),
                    first.rel_type,
                    second.to.as_str()
                ),
                en_original: format!(
                    "[inferred: {} —{:?}→ {}]",
                    first.from.as_str(),
                    first.rel_type,
                    second.to.as_str()
                ),
                source: RelationSource::Inferred,
                topic: first.topic.clone(),
                rationale: Some(format!(
                    "Transitivity: {} —{:?}→ {} —{:?}→ {}",
                    first.from.as_str(),
                    first.rel_type,
                    second.from.as_str(),
                    second.rel_type,
                    second.to.as_str()
                )),
                counter: None,
                synthesis: None,
            });
        }
    }
    out
}

fn symmetric_step(
    known: &std::collections::BTreeSet<(AtomId, AtomId, RelationType)>,
    edges: &[Relation],
) -> Vec<Relation> {
    let mut out = Vec::new();
    for edge in edges {
        if !SYMMETRIC_TYPES.contains(&edge.rel_type) {
            continue;
        }
        let key = (edge.to.clone(), edge.from.clone(), edge.rel_type);
        if key.0 == key.1 || known.contains(&key) {
            continue;
        }
        out.push(Relation {
            from: edge.to.clone(),
            to: edge.from.clone(),
            rel_type: edge.rel_type,
            object_case: edge.object_case,
            object_text: edge.object_text.clone(),
            verb_override: None,
            ru_original: format!(
                "[выведено: {} —{:?}→ {}]",
                edge.to.as_str(),
                edge.rel_type,
                edge.from.as_str()
            ),
            en_original: format!(
                "[inferred: {} —{:?}→ {}]",
                edge.to.as_str(),
                edge.rel_type,
                edge.from.as_str()
            ),
            source: RelationSource::Inferred,
            topic: edge.topic.clone(),
            rationale: Some(format!(
                "Symmetry: {} —{:?}→ {}",
                edge.from.as_str(),
                edge.rel_type,
                edge.to.as_str()
            )),
            counter: None,
            synthesis: None,
        });
    }
    out
}

/// Derive graph edges to fixpoint (transitivity + symmetry), bounded by
/// [`INFERENCE_MAX_ITERATIONS`] rounds and [`INFERENCE_MAX_NEW_EDGES`]
/// new edges. Pure and deterministic: inputs iterate in slice order,
/// each round's output is sorted by `(from, to, type)` before merging,
/// so identical graphs infer byte-identical edges. Existing triples
/// always win; self-loops never derive.
pub fn infer_graph_edges(edges: &[Relation]) -> Vec<Relation> {
    let mut known: std::collections::BTreeSet<(AtomId, AtomId, RelationType)> =
        edges.iter().map(edge_key).collect();
    let mut workspace: Vec<Relation> = edges.to_vec();
    let mut derived = Vec::new();
    for _ in 0..INFERENCE_MAX_ITERATIONS {
        let mut round = transitive_step(&known, &workspace);
        round.extend(symmetric_step(&known, &workspace));
        round.sort_by_key(edge_key);
        round.dedup_by_key(|edge| edge_key(edge));
        let fresh: Vec<Relation> = round
            .into_iter()
            .filter(|edge| !known.contains(&edge_key(edge)))
            .collect();
        if fresh.is_empty() {
            break;
        }
        for edge in fresh {
            if derived.len() >= INFERENCE_MAX_NEW_EDGES {
                break;
            }
            known.insert(edge_key(&edge));
            workspace.push(edge.clone());
            derived.push(edge);
        }
        if derived.len() >= INFERENCE_MAX_NEW_EDGES {
            break;
        }
    }
    derived
}

/// Classify the current system state into atom tags for inference.
pub fn classify_state_tags(
    topic_in_graph: bool,
    field_confidence: f64,
    field_counterfactual: f64,
    field_resonance: f64,
    conatus_energy: f64,
    angst: f64,
) -> Vec<AtomTag> {
    let mut tags = Vec::new();

    if !topic_in_graph {
        tags.push(AtomTag::Searching("unknown_topic".into()));
    }

    if conatus_energy < 0.5 {
        tags.push(AtomTag::Exhaustion("low_conatus".into()));
    }

    if angst > 0.7 {
        tags.push(AtomTag::Doubt("high_angst".into()));
    }

    if field_counterfactual > 0.7 {
        tags.push(AtomTag::Contradiction(
            "high_counterfactual".into(),
            "".into(),
        ));
    }

    if field_resonance < 0.2 {
        tags.push(AtomTag::NeedMeaning("low_resonance".into()));
    }

    if field_resonance < 0.3 && field_counterfactual < 0.4 {
        tags.push(AtomTag::NeedContact("low_resonance_flat".into()));
    }

    if conatus_energy < 0.3 {
        tags.push(AtomTag::AgencyLost(format!("{:.2}", conatus_energy)));
    } else if conatus_energy > 1.2 {
        tags.push(AtomTag::AgencyFound(format!("{:.2}", conatus_energy)));
    }

    if topic_in_graph && field_confidence > 0.7 {
        tags.push(AtomTag::Anchoring("confident_topic".into()));
    }

    tags
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::atom::ObjectCase;

    fn leg(from: &str, to: &str, rel_type: RelationType) -> Relation {
        Relation {
            from: AtomId::new(from),
            to: AtomId::new(to),
            rel_type,
            object_case: ObjectCase::CaseNominative,
            object_text: to.to_string(),
            verb_override: None,
            ru_original: format!("{from} {to}"),
            en_original: format!("{from} {to}"),
            source: RelationSource::SeedFromPredicate,
            topic: "t".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        }
    }

    #[test]
    fn test_no_derivation_without_patterns() {
        let tags = vec![AtomTag::Searching("x".into())];
        let derived = derive_atoms(&tags);
        assert!(derived.is_empty());
    }

    #[test]
    fn test_contact_under_stress() {
        let tags = vec![
            AtomTag::NeedContact("test".into()),
            AtomTag::Exhaustion("test".into()),
        ];
        let derived = derive_atoms(&tags);
        assert_eq!(derived.len(), 1);
        assert!(matches!(derived[0].rule, DeriveRule::ContactUnderStress));
    }

    #[test]
    fn test_contradiction_under_doubt() {
        let tags = vec![
            AtomTag::Contradiction("a".into(), "b".into()),
            AtomTag::Doubt("test".into()),
        ];
        let derived = derive_atoms(&tags);
        assert_eq!(derived.len(), 1);
        assert!(matches!(
            derived[0].rule,
            DeriveRule::ContradictionUnderDoubt
        ));
    }

    #[test]
    fn test_agency_search_exhaustion() {
        let tags = vec![
            AtomTag::AgencyLost("0.50".into()),
            AtomTag::Searching("test".into()),
        ];
        let derived = derive_atoms(&tags);
        assert_eq!(derived.len(), 1);
        assert!(matches!(
            derived[0].rule,
            DeriveRule::AgencySearchExhaustion
        ));
    }

    #[test]
    fn test_multiple_rules_fire() {
        let tags = vec![
            AtomTag::NeedContact("a".into()),
            AtomTag::Exhaustion("b".into()),
            AtomTag::Contradiction("c".into(), "d".into()),
            AtomTag::Doubt("e".into()),
        ];
        let derived = derive_atoms(&tags);
        assert!(derived.len() >= 2);
    }

    #[test]
    fn test_classify_state_tags() {
        let tags = classify_state_tags(true, 0.8, 0.3, 0.5, 1.5, 0.1);
        assert!(tags.iter().any(|t| matches!(t, AtomTag::Anchoring(_))));
        assert!(tags.iter().any(|t| matches!(t, AtomTag::AgencyFound(_))));
    }

    #[test]
    fn test_classify_exhaustion_fires_below_threshold() {
        let tags = classify_state_tags(true, 0.5, 0.3, 0.5, 0.3, 0.1);
        assert!(tags.iter().any(|t| matches!(t, AtomTag::Exhaustion(_))));
    }

    #[test]
    fn test_classify_exhaustion_does_not_fire_above_threshold() {
        let tags = classify_state_tags(true, 0.5, 0.3, 0.5, 1.5, 0.1);
        assert!(!tags.iter().any(|t| matches!(t, AtomTag::Exhaustion(_))));
    }

    #[test]
    fn test_classify_need_contact_fires() {
        let tags = classify_state_tags(true, 0.5, 0.3, 0.15, 1.0, 0.1);
        assert!(tags.iter().any(|t| matches!(t, AtomTag::NeedContact(_))));
    }

    #[test]
    fn transitivity_chains_and_marks_provenance() {
        let edges = vec![
            leg("a", "b", RelationType::RelIsA),
            leg("b", "c", RelationType::RelIsA),
        ];
        let derived = infer_graph_edges(&edges);
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].from.as_str(), "a");
        assert_eq!(derived[0].to.as_str(), "c");
        assert_eq!(derived[0].rel_type, RelationType::RelIsA);
        assert_eq!(derived[0].source, RelationSource::Inferred);
        assert!(derived[0]
            .rationale
            .as_deref()
            .unwrap()
            .contains("Transitivity"));
        assert!(derived[0].validate().is_ok(), "derived edges validate");
    }

    #[test]
    fn fixpoint_reaches_two_hops_and_stops() {
        let edges = vec![
            leg("a", "b", RelationType::RelRequires),
            leg("b", "c", RelationType::RelRequires),
            leg("c", "d", RelationType::RelRequires),
        ];
        let derived = infer_graph_edges(&edges);
        let keys: Vec<String> = derived
            .iter()
            .map(|edge| format!("{}-{}", edge.from.as_str(), edge.to.as_str()))
            .collect();
        assert!(keys.contains(&"a-c".to_string()));
        assert!(keys.contains(&"b-d".to_string()));
        assert!(
            keys.contains(&"a-d".to_string()),
            "second round closes: {keys:?}"
        );
        assert_eq!(derived.len(), 3);
    }

    #[test]
    fn symmetry_reverses_and_skips_cycles() {
        let edges = vec![leg("a", "b", RelationType::RelContrastsWith)];
        let derived = infer_graph_edges(&edges);
        assert_eq!(derived.len(), 1);
        assert_eq!(derived[0].from.as_str(), "b");
        assert_eq!(derived[0].to.as_str(), "a");
        // Non-symmetric types never reverse; existing triples win.
        let plain = vec![leg("a", "b", RelationType::RelRelatedTo)];
        assert!(infer_graph_edges(&plain).is_empty());
        let dup = vec![
            leg("a", "b", RelationType::RelContrastsWith),
            leg("b", "a", RelationType::RelContrastsWith),
        ];
        assert!(infer_graph_edges(&dup).is_empty(), "existing reverse wins");
    }

    #[test]
    fn inference_is_deterministic_and_bounded() {
        let edges = vec![
            leg("b", "c", RelationType::RelIsA),
            leg("a", "b", RelationType::RelIsA),
            leg("x", "y", RelationType::RelContrastsWith),
        ];
        let first = infer_graph_edges(&edges);
        let second = infer_graph_edges(&edges);
        assert_eq!(
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
        assert!(first.len() <= INFERENCE_MAX_NEW_EDGES);
    }
}

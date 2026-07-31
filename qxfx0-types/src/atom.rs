use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::RelationType;

/// Atom identifier — topic or concept in the semantic graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AtomId(pub String);

/// Concept identifier — uniquely identifies a curated concept.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ConceptId(pub String);

impl AtomId {
    pub fn new(s: impl Into<String>) -> Self {
        AtomId(s.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Atom category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AtomCategory {
    CatTopic,
    CatConcept,
    CatProperty,
    CatObject,
}

/// An atom in the store.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Atom {
    pub id: AtomId,
    pub display: String,
    pub category: AtomCategory,
}

/// Source of a relation — controls gate admission.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationSource {
    SeedFromPredicate,
    Curated,
    PromotedSubstrate,
    SubstrateExtractedRaw,
    LlmDiscovered,
}

/// Grammatical case for object inflection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ObjectCase {
    CaseNominative,
    CaseGenitive,
    CaseDative,
    CaseAccusative,
    CaseInstrumental,
    CasePrepositional,
}

/// A typed edge: Atom(from) --RelationType--> Atom(to)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Relation {
    pub from: AtomId,
    pub to: AtomId,
    pub rel_type: RelationType,
    pub object_case: ObjectCase,
    pub object_text: String,
    pub verb_override: Option<String>,
    pub ru_original: String,
    pub en_original: String,
    pub source: RelationSource,
    pub topic: String,
    pub rationale: Option<String>,
    pub counter: Option<String>,
    pub synthesis: Option<String>,
}

impl Relation {
    /// Validate that the relation has the minimum text content required for
    /// verbalization. Both `ru_original` and `en_original` must contain at
    /// least one non-whitespace character. Empty or whitespace-only originals
    /// slip past `gate.rs::validate_relation` (which only checks
    /// `!ru_original.is_empty()`) and render as `"{from} {verb} "` which is
    /// ugly and confuses downstream consumers.
    pub fn validate(&self) -> Result<(), String> {
        if self.ru_original.trim().is_empty() {
            return Err(format!(
                "Relation {}-{:?}->{}: ru_original is empty or whitespace",
                self.from.as_str(),
                self.rel_type,
                self.to.as_str()
            ));
        }
        if self.en_original.trim().is_empty() {
            return Err(format!(
                "Relation {}-{:?}->{}: en_original is empty or whitespace",
                self.from.as_str(),
                self.rel_type,
                self.to.as_str()
            ));
        }
        Ok(())
    }
}

/// Path proof — trace of edges traversed in graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PathProof {
    pub edges: Vec<Relation>,
    pub topic: String,
}

/// The semantic graph — typed edges over atoms.
/// Uses BTreeMap for deterministic iteration order.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AtomGraph {
    pub atoms: BTreeMap<AtomId, Atom>,
    pub edges: Vec<Relation>,
    pub edges_from: BTreeMap<AtomId, Vec<usize>>,
    pub edges_to: BTreeMap<AtomId, Vec<usize>>,
}

impl AtomGraph {
    pub fn new() -> Self {
        AtomGraph::default()
    }

    pub fn relations_from(&self, atom: &AtomId) -> Vec<&Relation> {
        self.edges_from
            .get(atom)
            .map(|indices| indices.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    /// Reverse-edge lookup: relations pointing *to* this atom.
    pub fn relations_to(&self, atom: &AtomId) -> Vec<&Relation> {
        self.edges_to
            .get(atom)
            .map(|indices| indices.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    pub fn add_relation(&mut self, rel: Relation) {
        let idx = self.edges.len();
        self.edges_from
            .entry(rel.from.clone())
            .or_default()
            .push(idx);
        self.edges_to.entry(rel.to.clone()).or_default().push(idx);
        self.edges.push(rel);
    }

    /// Rebuild `edges_from`/`edges_to` indexes from `edges`.
    ///
    /// Call this after deserializing an `AtomGraph` or after any manual
    /// mutation of `edges` that bypassed `add_relation`.
    pub fn rebuild_indices(&mut self) {
        self.edges_from.clear();
        self.edges_to.clear();
        for (idx, rel) in self.edges.iter().enumerate() {
            self.edges_from
                .entry(rel.from.clone())
                .or_default()
                .push(idx);
            self.edges_to.entry(rel.to.clone()).or_default().push(idx);
        }
    }

    /// Validate graph referential integrity and the two derived indexes.
    /// This is suitable for persistence boundaries and health checks; callers
    /// that mutate a graph manually should rebuild indexes before validating.
    pub fn validate(&self) -> Result<(), Vec<String>> {
        let mut violations = Vec::new();

        for (key, atom) in &self.atoms {
            if key != &atom.id {
                violations.push(format!(
                    "atom map key '{}' differs from atom id '{}'",
                    key.as_str(),
                    atom.id.as_str()
                ));
            }
            if key.as_str().trim().is_empty() || atom.display.trim().is_empty() {
                violations.push(format!(
                    "atom '{}' has empty identity/display",
                    key.as_str()
                ));
            }
        }

        let mut expected_from: BTreeMap<AtomId, Vec<usize>> = BTreeMap::new();
        let mut expected_to: BTreeMap<AtomId, Vec<usize>> = BTreeMap::new();
        for (index, relation) in self.edges.iter().enumerate() {
            if !self.atoms.contains_key(&relation.from) {
                violations.push(format!(
                    "edge {index} references missing source '{}'",
                    relation.from.as_str()
                ));
            }
            if !self.atoms.contains_key(&relation.to) {
                violations.push(format!(
                    "edge {index} references missing target '{}'",
                    relation.to.as_str()
                ));
            }
            if relation.topic.trim().is_empty() {
                violations.push(format!("edge {index} has an empty topic"));
            }
            if let Err(reason) = relation.validate() {
                violations.push(reason);
            }
            expected_from
                .entry(relation.from.clone())
                .or_default()
                .push(index);
            expected_to
                .entry(relation.to.clone())
                .or_default()
                .push(index);
        }

        if self.edges_from != expected_from {
            violations.push("edges_from index does not match edges".into());
        }
        if self.edges_to != expected_to {
            violations.push("edges_to index does not match edges".into());
        }

        if violations.is_empty() {
            Ok(())
        } else {
            Err(violations)
        }
    }
}

/// Generated surface — structured output with provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GeneratedSurface {
    pub text: String,
    pub paths: Vec<PathProof>,
    pub provenance: Vec<RelationSource>,
    pub depth_score: f64,
}

// ═══════════════════════════════════════════════════════════════════════════
// Sense Vector Algebra — structural vector representation of meaning.
//
// Each atom in the graph is a basis vector in an N-dimensional sense-space
// (N = number of atoms). A SenseVector is a weighted projection onto one
// basis vector, carrying the relation-transformation directions available
// from that atom.
//
// A SenseField is a superposition (weighted sum) of SenseVectors — the
// vector decomposition of a human sentence.
//
// A ConjugateVector is the system's response vector: the result of
// traversing the graph from the input field and projecting back.
// ═══════════════════════════════════════════════════════════════════════════

/// A single sense vector — projection onto one atom with relation directions.
///
/// `atom_id` identifies the basis vector (one per atom in the graph).
/// `weight` is the projection coefficient (how strongly the input activates
/// this atom).
/// `relation_vector` lists the transformation directions available from
/// this atom, each weighted by the relation type's semantic charge.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SenseVector {
    pub atom_id: AtomId,
    pub weight: f64,
    pub relation_vector: Vec<(RelationType, f64)>,
}

impl SenseVector {
    /// Create a unit sense vector for an atom (weight = 1.0, no relations yet).
    pub fn unit(atom_id: AtomId) -> Self {
        SenseVector {
            atom_id,
            weight: 1.0,
            relation_vector: Vec::new(),
        }
    }

    /// Create a weighted sense vector.
    pub fn weighted(atom_id: AtomId, weight: f64) -> Self {
        SenseVector {
            atom_id,
            weight,
            relation_vector: Vec::new(),
        }
    }

    /// Attach relation directions from the graph.
    pub fn with_relations(mut self, graph: &AtomGraph) -> Self {
        let rels = graph.relations_from(&self.atom_id);
        // Each relation contributes a direction; weight decays by edge count
        // so hubs don't dominate.
        let n = rels.len().max(1) as f64;
        self.relation_vector = rels.iter().map(|r| (r.rel_type, 1.0 / n)).collect();
        self
    }

    /// Magnitude (L2 norm) of this sense vector.
    pub fn magnitude(&self) -> f64 {
        let rel_sq: f64 = self.relation_vector.iter().map(|(_, w)| w * w).sum();
        (self.weight * self.weight + rel_sq).sqrt()
    }
}

/// A sense field — superposition of sense vectors (decomposition of a sentence).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SenseField {
    pub vectors: Vec<SenseVector>,
}

impl SenseField {
    pub fn new() -> Self {
        SenseField::default()
    }

    pub fn push(&mut self, v: SenseVector) {
        self.vectors.push(v);
    }

    pub fn is_empty(&self) -> bool {
        self.vectors.is_empty()
    }

    pub fn len(&self) -> usize {
        self.vectors.len()
    }

    /// Total weight across all vectors.
    pub fn total_weight(&self) -> f64 {
        self.vectors.iter().map(|v| v.weight).sum()
    }

    /// Normalize weights to sum to 1.0 (if total is safely positive).
    ///
    /// Guards against negative weights or cancellation producing a very
    /// small positive total: only normalizes when `total > f64::EPSILON`.
    pub fn normalize(&mut self) {
        let total = self.total_weight();
        if total > f64::EPSILON {
            for v in &mut self.vectors {
                v.weight /= total;
            }
        }
    }

    /// Get the dominant atom (highest weight), if any.
    ///
    /// Uses `f64::total_cmp` for deterministic ordering even in the
    /// presence of NaN values. NaN-weighted vectors are filtered out
    /// so they can never be returned as dominant.
    pub fn dominant_atom(&self) -> Option<&SenseVector> {
        self.vectors
            .iter()
            .filter(|v| !v.weight.is_nan())
            .max_by(|a, b| a.weight.total_cmp(&b.weight))
    }
}

/// A conjugate vector — the system's response in sense-space.
///
/// Produced by traversing the graph from the input sense field.
/// `resonance` measures how strongly the system's graph resonates with
/// the input. `edges` is the traversal path used for verbalization.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct ConjugateVector {
    pub components: Vec<SenseVector>,
    pub edges: Vec<Relation>,
    pub resonance: f64,
}

impl ConjugateVector {
    pub fn new() -> Self {
        ConjugateVector::default()
    }

    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Depth = number of edges traversed.
    pub fn depth(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod sense_vector_tests {
    use super::*;

    #[test]
    fn test_sense_vector_unit() {
        let v = SenseVector::unit(AtomId::new("свобода"));
        assert_eq!(v.atom_id.as_str(), "свобода");
        assert!((v.weight - 1.0).abs() < 1e-9);
        assert!(v.relation_vector.is_empty());
    }

    #[test]
    fn test_sense_vector_weighted() {
        let v = SenseVector::weighted(AtomId::new("истина"), 0.5);
        assert!((v.weight - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_sense_vector_with_relations() {
        let mut graph = AtomGraph::new();
        graph.atoms.insert(
            AtomId::new("свобода"),
            Atom {
                id: AtomId::new("свобода"),
                display: "свобода".into(),
                category: AtomCategory::CatTopic,
            },
        );
        graph.add_relation(Relation {
            from: AtomId::new("свобода"),
            to: AtomId::new("выбор"),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "выбор".into(),
            verb_override: None,
            ru_original: "свобода предполагает выбор".into(),
            en_original: String::new(),
            source: RelationSource::SeedFromPredicate,
            topic: "свобода".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        });
        let v = SenseVector::unit(AtomId::new("свобода")).with_relations(&graph);
        assert_eq!(v.relation_vector.len(), 1);
        assert_eq!(v.relation_vector[0].0, RelationType::RelPresupposes);
    }

    #[test]
    fn test_sense_field_normalize() {
        let mut field = SenseField::new();
        field.push(SenseVector::weighted(AtomId::new("a"), 2.0));
        field.push(SenseVector::weighted(AtomId::new("b"), 2.0));
        field.normalize();
        assert!((field.vectors[0].weight - 0.5).abs() < 1e-9);
        assert!((field.vectors[1].weight - 0.5).abs() < 1e-9);
    }

    #[test]
    fn test_sense_field_dominant() {
        let mut field = SenseField::new();
        field.push(SenseVector::weighted(AtomId::new("a"), 0.3));
        field.push(SenseVector::weighted(AtomId::new("b"), 0.7));
        let dom = field.dominant_atom().unwrap();
        assert_eq!(dom.atom_id.as_str(), "b");
    }

    #[test]
    fn test_conjugate_vector_depth() {
        let cv = ConjugateVector::new();
        assert_eq!(cv.depth(), 0);
    }

    #[test]
    fn test_relations_to_reverse_lookup() {
        let mut graph = AtomGraph::new();
        graph.add_relation(Relation {
            from: AtomId::new("a"),
            to: AtomId::new("b"),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "b".into(),
            verb_override: None,
            ru_original: "a предполагает b".into(),
            en_original: String::new(),
            source: RelationSource::SeedFromPredicate,
            topic: "a".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        });
        let to_b = graph.relations_to(&AtomId::new("b"));
        assert_eq!(to_b.len(), 1);
        assert_eq!(to_b[0].from.as_str(), "a");
    }

    #[test]
    fn test_sense_field_dominant_empty() {
        let field = SenseField::new();
        assert!(field.dominant_atom().is_none());
    }

    #[test]
    fn test_sense_field_normalize_zero_total() {
        let mut field = SenseField::new();
        field.push(SenseVector::weighted(AtomId::new("a"), 0.0));
        field.push(SenseVector::weighted(AtomId::new("b"), 0.0));
        field.normalize();
        assert!((field.vectors[0].weight - 0.0).abs() < 1e-9);
        assert!((field.vectors[1].weight - 0.0).abs() < 1e-9);
    }

    #[test]
    fn test_sense_field_normalize_negative_total() {
        let mut field = SenseField::new();
        field.push(SenseVector::weighted(AtomId::new("a"), 1.0));
        field.push(SenseVector::weighted(AtomId::new("b"), -1.0));
        field.normalize();
        assert!((field.vectors[0].weight - 1.0).abs() < 1e-9);
        assert!((field.vectors[1].weight - (-1.0)).abs() < 1e-9);
    }

    #[test]
    fn test_sense_field_dominant_nan_filtered() {
        let mut field = SenseField::new();
        field.push(SenseVector::weighted(AtomId::new("a"), f64::NAN));
        field.push(SenseVector::weighted(AtomId::new("b"), 0.5));
        let dom = field.dominant_atom().unwrap();
        assert_eq!(dom.atom_id.as_str(), "b");
    }

    #[test]
    fn test_sense_field_dominant_all_nan() {
        let mut field = SenseField::new();
        field.push(SenseVector::weighted(AtomId::new("a"), f64::NAN));
        field.push(SenseVector::weighted(AtomId::new("b"), f64::NAN));
        assert!(field.dominant_atom().is_none());
    }

    #[test]
    fn test_conjugate_vector_with_edges() {
        let rel = Relation {
            from: AtomId::new("свобода"),
            to: AtomId::new("выбор"),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "выбор".into(),
            verb_override: None,
            ru_original: "свобода предполагает выбор".into(),
            en_original: String::new(),
            source: RelationSource::SeedFromPredicate,
            topic: "свобода".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        };
        let cv = ConjugateVector {
            components: vec![SenseVector::weighted(AtomId::new("выбор"), 1.0)],
            edges: vec![rel],
            resonance: 0.8,
        };
        assert_eq!(cv.depth(), 1);
        assert!(!cv.is_empty());
        assert!((cv.resonance - 0.8).abs() < 1e-9);
        assert_eq!(cv.components[0].atom_id.as_str(), "выбор");
        assert_eq!(cv.edges[0].from.as_str(), "свобода");
        assert_eq!(cv.edges[0].to.as_str(), "выбор");
    }

    #[test]
    fn test_relations_from_no_outgoing() {
        let mut graph = AtomGraph::new();
        graph.atoms.insert(
            AtomId::new("a"),
            Atom {
                id: AtomId::new("a"),
                display: "a".into(),
                category: AtomCategory::CatTopic,
            },
        );
        graph.add_relation(Relation {
            from: AtomId::new("x"),
            to: AtomId::new("a"),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "a".into(),
            verb_override: None,
            ru_original: "x предполагает a".into(),
            en_original: String::new(),
            source: RelationSource::SeedFromPredicate,
            topic: "x".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        });
        let from_a = graph.relations_from(&AtomId::new("a"));
        assert!(from_a.is_empty());
    }

    fn make_rel(ru: &str, en: &str) -> Relation {
        Relation {
            from: AtomId::new("a"),
            to: AtomId::new("b"),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "b".into(),
            verb_override: None,
            ru_original: ru.into(),
            en_original: en.into(),
            source: RelationSource::SeedFromPredicate,
            topic: "a".into(),
            rationale: None,
            counter: None,
            synthesis: None,
        }
    }

    #[test]
    fn test_relation_validate_ok() {
        let rel = make_rel("a предполагает b", "a presupposes b");
        assert!(rel.validate().is_ok());
    }

    #[test]
    fn test_relation_validate_empty_ru() {
        let rel = make_rel("", "a presupposes b");
        assert!(rel.validate().is_err());
    }

    #[test]
    fn test_relation_validate_whitespace_ru() {
        let rel = make_rel("   \t\n", "a presupposes b");
        assert!(rel.validate().is_err());
    }

    #[test]
    fn test_relation_validate_empty_en() {
        let rel = make_rel("a предполагает b", "");
        assert!(rel.validate().is_err());
    }

    #[test]
    fn test_atom_graph_serde_round_trip() {
        let mut graph = AtomGraph::new();
        graph.atoms.insert(
            AtomId::new("свобода"),
            Atom {
                id: AtomId::new("свобода"),
                display: "свобода".into(),
                category: AtomCategory::CatTopic,
            },
        );
        graph.atoms.insert(
            AtomId::new("выбор"),
            Atom {
                id: AtomId::new("выбор"),
                display: "выбор".into(),
                category: AtomCategory::CatConcept,
            },
        );
        graph.add_relation(Relation {
            from: AtomId::new("свобода"),
            to: AtomId::new("выбор"),
            rel_type: RelationType::RelPresupposes,
            object_case: ObjectCase::CaseAccusative,
            object_text: "выбор".into(),
            verb_override: None,
            ru_original: "свобода предполагает выбор".into(),
            en_original: "freedom presupposes choice".into(),
            source: RelationSource::SeedFromPredicate,
            topic: "свобода".into(),
            rationale: Some("axiomatic".into()),
            counter: None,
            synthesis: None,
        });

        let json = serde_json::to_string(&graph).expect("serialize graph");
        let restored: AtomGraph = serde_json::from_str(&json).expect("deserialize graph");
        assert_eq!(restored.atoms.len(), 2);
        assert_eq!(restored.edges.len(), 1);
        assert_eq!(restored.edges[0].from.as_str(), "свобода");
        assert_eq!(restored.edges[0].to.as_str(), "выбор");
        assert_eq!(restored.edges[0].rel_type, RelationType::RelPresupposes);
        assert_eq!(restored.edges[0].en_original, "freedom presupposes choice");
        assert!(restored.validate().is_ok());
    }

    #[test]
    fn test_atom_graph_validate_detects_missing_endpoint() {
        let mut graph = AtomGraph::new();
        graph.atoms.insert(
            AtomId::new("a"),
            Atom {
                id: AtomId::new("a"),
                display: "a".into(),
                category: AtomCategory::CatTopic,
            },
        );
        graph.add_relation(make_rel("a предполагает b", "a presupposes b"));
        let violations = graph.validate().unwrap_err();
        assert!(violations
            .iter()
            .any(|reason| reason.contains("missing target")));
    }

    #[test]
    fn test_atom_graph_validate_detects_stale_index() {
        let mut graph = AtomGraph::new();
        for id in ["a", "b"] {
            graph.atoms.insert(
                AtomId::new(id),
                Atom {
                    id: AtomId::new(id),
                    display: id.into(),
                    category: AtomCategory::CatTopic,
                },
            );
        }
        graph.add_relation(make_rel("a предполагает b", "a presupposes b"));
        graph.edges_from.clear();
        let violations = graph.validate().unwrap_err();
        assert!(violations
            .iter()
            .any(|reason| reason.contains("edges_from")));
    }
}

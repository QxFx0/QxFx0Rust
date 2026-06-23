use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::RelationType;

/// Atom identifier — topic or concept in the semantic graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct AtomId(pub String);

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

    pub fn add_relation(&mut self, rel: Relation) {
        let idx = self.edges.len();
        self.edges_from
            .entry(rel.from.clone())
            .or_default()
            .push(idx);
        self.edges_to.entry(rel.to.clone()).or_default().push(idx);
        self.edges.push(rel);
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

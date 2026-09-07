//! Immutable, curated fact model separated from graph edges and dialogue state.

use crate::{ConceptResolver, PredicateRef, SemanticId};
pub use qxfx0_types::FactId;
use qxfx0_types::{
    ConceptId, RelationId, RelationType, Thesis, ThesisGraph, ThesisGraphError, ThesisId,
    ThesisKind, ThesisRelation, ThesisRelationKind,
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactKind {
    Definition,
    InterpretiveClaim,
    EmpiricalClaim,
    NormativeClaim,
    Hypothesis,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactStatus {
    Curated,
    Deprecated,
    Retracted,
    Draft,
}

/// Curated dependencies between facts. Conditions never contain observed or
/// generated text and cannot substitute for subject/relation/object identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FactCondition {
    Counters(FactId),
    FollowsFrom(FactId),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FactRecord {
    pub id: FactId,
    pub subject: ConceptId,
    pub relation: SemanticId,
    pub object: ConceptId,
    pub kind: FactKind,
    pub conditions: Vec<FactCondition>,
    pub confidence_basis_points: u16,
    pub source_pack: String,
    pub source_ref: String,
    pub valid_from: Option<String>,
    pub valid_to: Option<String>,
    pub status: FactStatus,
}

impl FactRecord {
    /// Compatibility adapter into the P0 typed thesis model. Legacy FactId,
    /// response-plan contracts, and knowledge-pack fingerprints remain unchanged.
    pub fn canonical_thesis(&self) -> Result<Thesis, FactRegistryError> {
        let id = ThesisId::try_new(self.id.as_str())
            .map_err(|error| FactRegistryError::ThesisAdapter(error.to_string()))?;
        let predicate = RelationId::try_new(self.relation.as_str())
            .map_err(|error| FactRegistryError::ThesisAdapter(error.to_string()))?;
        let kind = match self.kind {
            FactKind::Definition => ThesisKind::Definition,
            FactKind::InterpretiveClaim => ThesisKind::InterpretiveClaim,
            FactKind::EmpiricalClaim => ThesisKind::EmpiricalClaim,
            FactKind::NormativeClaim => ThesisKind::NormativeClaim,
            FactKind::Hypothesis => ThesisKind::Hypothesis,
        };
        Ok(Thesis {
            id,
            subject: self.subject.clone(),
            predicate,
            object: self.object.clone(),
            kind,
            qualifiers: BTreeMap::new(),
            surface_text: None,
            confidence_basis_points: Some(self.confidence_basis_points),
            provenance: BTreeMap::from([
                ("source_pack".into(), self.source_pack.clone()),
                ("source_ref".into(), self.source_ref.clone()),
            ]),
            valid_from: self.valid_from.clone(),
            valid_to: self.valid_to.clone(),
        })
    }

    pub fn validate_shape(&self) -> Result<(), FactRegistryError> {
        if self.id.as_str().trim().is_empty() {
            return Err(FactRegistryError::Validation(
                "fact id must not be empty".into(),
            ));
        }
        if self.source_pack.trim().is_empty() || self.source_ref.trim().is_empty() {
            return Err(FactRegistryError::MissingProvenance(self.id.clone()));
        }
        if self.confidence_basis_points > 10_000 {
            return Err(FactRegistryError::Validation(format!(
                "fact '{}' confidence exceeds 10000 basis points",
                self.id.as_str()
            )));
        }
        Ok(())
    }
}

/// Typed allowlist backed by the graph relation algebra. A free-form
/// `SemanticId` is not sufficient for fact admission.
#[derive(Debug, Clone)]
pub struct TypedRelationModel {
    relations: BTreeSet<SemanticId>,
}

impl Default for TypedRelationModel {
    fn default() -> Self {
        Self {
            relations: RelationType::ALL
                .into_iter()
                .map(|relation| {
                    SemanticId::try_new(format!("{relation:?}"))
                        .expect("typed relation names are non-empty")
                })
                .collect(),
        }
    }
}

impl TypedRelationModel {
    pub fn semantic_id(relation: RelationType) -> SemanticId {
        SemanticId::try_new(format!("{relation:?}")).expect("typed relation names are non-empty")
    }

    pub fn contains(&self, relation: &SemanticId) -> bool {
        self.relations.contains(relation)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FactRegistryError {
    #[error("fact registry validation failed: {0}")]
    Validation(String),
    #[error("fact '{0:?}' is missing source_pack or source_ref")]
    MissingProvenance(FactId),
    #[error("duplicate fact id: {0}")]
    DuplicateFactId(FactId),
    #[error("unknown concept in fact '{fact_id}': {concept_id}")]
    UnknownConcept { fact_id: String, concept_id: String },
    #[error("unknown typed relation in fact '{fact_id}': {relation}")]
    UnknownRelation { fact_id: String, relation: String },
    #[error("unknown fact: {0}")]
    UnknownFact(FactId),
    #[error("fact is not curated and cannot be selected: {0}")]
    NotCurated(FactId),
    #[error("fact has a temporal validity window and requires explicit as-of resolution: {0}")]
    TemporalValidityRequired(FactId),
    #[error("fact is outside its validity window: {0}")]
    OutsideValidityWindow(FactId),
    #[error("fact-to-thesis adapter failed: {0}")]
    ThesisAdapter(String),
    #[error("fact relation graph failed: {0}")]
    ThesisGraph(#[from] ThesisGraphError),
}

#[derive(Debug, Clone, Default)]
pub struct FactRegistry {
    records: BTreeMap<FactId, FactRecord>,
    predicate_facts: BTreeMap<PredicateRef, FactId>,
}

impl FactRegistry {
    pub fn load(
        records: impl IntoIterator<Item = FactRecord>,
        predicate_facts: impl IntoIterator<Item = (PredicateRef, FactId)>,
        concepts: &ConceptResolver,
        relations: &TypedRelationModel,
    ) -> Result<Self, FactRegistryError> {
        let known_concepts = concepts
            .records()
            .map(|record| record.concept_id.clone())
            .collect::<BTreeSet<_>>();
        let mut registry = Self::default();
        for record in records {
            record.validate_shape()?;
            if registry.records.contains_key(&record.id) {
                return Err(FactRegistryError::DuplicateFactId(record.id));
            }
            for concept_id in [&record.subject, &record.object] {
                if !known_concepts.contains(concept_id) {
                    return Err(FactRegistryError::UnknownConcept {
                        fact_id: record.id.as_str().into(),
                        concept_id: concept_id.0.clone(),
                    });
                }
            }
            if !relations.contains(&record.relation) {
                return Err(FactRegistryError::UnknownRelation {
                    fact_id: record.id.as_str().into(),
                    relation: record.relation.as_str().into(),
                });
            }
            registry.records.insert(record.id.clone(), record);
        }
        for (predicate_ref, fact_id) in predicate_facts {
            if !registry.records.contains_key(&fact_id) {
                return Err(FactRegistryError::UnknownFact(fact_id));
            }
            if registry
                .predicate_facts
                .insert(predicate_ref.clone(), fact_id)
                .is_some()
            {
                return Err(FactRegistryError::Validation(format!(
                    "duplicate predicate-to-fact binding '{}'",
                    predicate_ref.as_str()
                )));
            }
        }
        for record in registry.records.values() {
            for condition in &record.conditions {
                let fact_id = match condition {
                    FactCondition::Counters(fact_id) | FactCondition::FollowsFrom(fact_id) => {
                        fact_id
                    }
                };
                if !registry.records.contains_key(fact_id) {
                    return Err(FactRegistryError::UnknownFact(fact_id.clone()));
                }
            }
        }
        Ok(registry)
    }

    pub fn select(&self, fact_id: &FactId) -> Result<&FactRecord, FactRegistryError> {
        let record = self
            .records
            .get(fact_id)
            .ok_or_else(|| FactRegistryError::UnknownFact(fact_id.clone()))?;
        if record.valid_from.is_some() || record.valid_to.is_some() {
            return Err(FactRegistryError::TemporalValidityRequired(fact_id.clone()));
        }
        self.select_curated(record)
    }

    /// Resolve a curated fact against an explicit deterministic ISO date.
    /// Callers that do not carry temporal context must use [`select`], which
    /// rejects temporal records instead of guessing their validity.
    pub fn select_at(
        &self,
        fact_id: &FactId,
        as_of: &str,
    ) -> Result<&FactRecord, FactRegistryError> {
        let record = self
            .records
            .get(fact_id)
            .ok_or_else(|| FactRegistryError::UnknownFact(fact_id.clone()))?;
        self.select_curated(record)?;
        if record
            .valid_from
            .as_deref()
            .is_some_and(|from| as_of < from)
            || record.valid_to.as_deref().is_some_and(|to| as_of >= to)
        {
            return Err(FactRegistryError::OutsideValidityWindow(fact_id.clone()));
        }
        Ok(record)
    }

    fn select_curated<'a>(
        &self,
        record: &'a FactRecord,
    ) -> Result<&'a FactRecord, FactRegistryError> {
        if record.status != FactStatus::Curated {
            return Err(FactRegistryError::NotCurated(record.id.clone()));
        }
        Ok(record)
    }

    pub fn select_by_predicate(
        &self,
        predicate_ref: &PredicateRef,
    ) -> Result<&FactRecord, FactRegistryError> {
        let fact_id = self.predicate_facts.get(predicate_ref).ok_or_else(|| {
            FactRegistryError::Validation(format!(
                "predicate '{}' has no FactId",
                predicate_ref.as_str()
            ))
        })?;
        self.select(fact_id)
    }

    pub fn fact_id_for_predicate(&self, predicate_ref: &PredicateRef) -> Option<&FactId> {
        self.predicate_facts.get(predicate_ref)
    }

    /// The set of fact ids bound to predicate references. Used by the
    /// admission profile to test static membership: a fact that is not bound
    /// to any predicate belongs to no profile.
    pub fn fact_id_for_predicate_members(&self) -> BTreeSet<&FactId> {
        self.predicate_facts.values().collect()
    }

    pub fn get(&self, fact_id: &FactId) -> Option<&FactRecord> {
        self.records.get(fact_id)
    }

    pub fn records(&self) -> impl Iterator<Item = &FactRecord> {
        self.records.values()
    }

    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Materializes every current fact and condition into the deterministic
    /// thesis authority graph. Condition direction is dependent -> referenced.
    pub fn canonical_thesis_graph(&self) -> Result<ThesisGraph, FactRegistryError> {
        let mut graph = ThesisGraph::default();
        for record in self.records.values() {
            graph.insert_thesis(record.canonical_thesis()?)?;
        }
        for record in self.records.values() {
            for condition in &record.conditions {
                let (kind, target) = match condition {
                    FactCondition::Counters(target) => (ThesisRelationKind::Counters, target),
                    FactCondition::FollowsFrom(target) => (ThesisRelationKind::FollowsFrom, target),
                };
                graph.insert_relation(ThesisRelation {
                    from: ThesisId::try_new(record.id.as_str())
                        .map_err(|error| FactRegistryError::ThesisAdapter(error.to_string()))?,
                    kind,
                    to: ThesisId::try_new(target.as_str())
                        .map_err(|error| FactRegistryError::ThesisAdapter(error.to_string()))?,
                })?;
            }
        }
        Ok(graph)
    }

    pub fn count_by_status(&self, status: FactStatus) -> usize {
        self.records
            .values()
            .filter(|record| record.status == status)
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::get_resolver;

    fn record(id: &str, status: FactStatus) -> FactRecord {
        FactRecord {
            id: FactId::try_new(id).unwrap(),
            subject: ConceptId("concept.свобода".into()),
            relation: TypedRelationModel::semantic_id(RelationType::RelPresupposes),
            object: ConceptId("concept.свобода".into()),
            kind: FactKind::InterpretiveClaim,
            conditions: Vec::new(),
            confidence_basis_points: 9_000,
            source_pack: "test-facts-v1".into(),
            source_ref: "test:fact".into(),
            valid_from: None,
            valid_to: None,
            status,
        }
    }

    #[test]
    fn missing_provenance_is_rejected() {
        let mut fact = record("fact.test", FactStatus::Curated);
        fact.source_ref.clear();
        assert!(matches!(
            FactRegistry::load([fact], [], get_resolver(), &TypedRelationModel::default()),
            Err(FactRegistryError::MissingProvenance(_))
        ));
    }

    #[test]
    fn duplicate_fact_id_is_rejected() {
        let fact = record("fact.test", FactStatus::Curated);
        let mut fact_from_another_pack = fact.clone();
        fact_from_another_pack.source_pack = "other-test-facts-v1".into();
        fact_from_another_pack.source_ref = "other:fact".into();
        assert!(matches!(
            FactRegistry::load(
                [fact, fact_from_another_pack],
                [],
                get_resolver(),
                &TypedRelationModel::default()
            ),
            Err(FactRegistryError::DuplicateFactId(_))
        ));
    }

    #[test]
    fn temporal_fact_requires_explicit_as_of_and_rejects_stale_selection() {
        let mut fact = record("fact.temporal", FactStatus::Curated);
        fact.valid_from = Some("2026-01-01".into());
        fact.valid_to = Some("2026-02-01".into());
        let registry =
            FactRegistry::load([fact], [], get_resolver(), &TypedRelationModel::default()).unwrap();
        let id = FactId::try_new("fact.temporal").unwrap();
        assert!(matches!(
            registry.select(&id),
            Err(FactRegistryError::TemporalValidityRequired(_))
        ));
        assert!(registry.select_at(&id, "2026-01-15").is_ok());
        assert!(matches!(
            registry.select_at(&id, "2026-02-01"),
            Err(FactRegistryError::OutsideValidityWindow(_))
        ));
    }

    #[test]
    fn unknown_concept_is_rejected() {
        let mut fact = record("fact.test", FactStatus::Curated);
        fact.object = ConceptId("concept.missing".into());
        assert!(matches!(
            FactRegistry::load([fact], [], get_resolver(), &TypedRelationModel::default()),
            Err(FactRegistryError::UnknownConcept { .. })
        ));
    }

    #[test]
    fn selector_accepts_only_curated_facts() {
        let curated = record("fact.curated", FactStatus::Curated);
        let deprecated = record("fact.deprecated", FactStatus::Deprecated);
        let retracted = record("fact.retracted", FactStatus::Retracted);
        let draft = record("fact.draft", FactStatus::Draft);
        let registry = FactRegistry::load(
            [
                curated.clone(),
                deprecated.clone(),
                retracted.clone(),
                draft.clone(),
            ],
            [],
            get_resolver(),
            &TypedRelationModel::default(),
        )
        .unwrap();
        assert_eq!(registry.select(&curated.id).unwrap(), &curated);
        assert!(matches!(
            registry.select(&deprecated.id),
            Err(FactRegistryError::NotCurated(_))
        ));
        assert!(matches!(
            registry.select(&retracted.id),
            Err(FactRegistryError::NotCurated(_))
        ));
        assert!(matches!(
            registry.select(&draft.id),
            Err(FactRegistryError::NotCurated(_))
        ));
    }

    #[test]
    fn unknown_typed_relation_is_rejected() {
        let mut fact = record("fact.relation", FactStatus::Curated);
        fact.relation = SemanticId::try_new("not-a-typed-relation").unwrap();
        assert!(matches!(
            FactRegistry::load([fact], [], get_resolver(), &TypedRelationModel::default()),
            Err(FactRegistryError::UnknownRelation { .. })
        ));
    }

    #[test]
    fn missing_fact_dependency_is_rejected() {
        let mut fact = record("fact.dependent", FactStatus::Curated);
        fact.conditions = vec![FactCondition::Counters(
            FactId::try_new("fact.missing").unwrap(),
        )];
        assert!(matches!(
            FactRegistry::load([fact], [], get_resolver(), &TypedRelationModel::default()),
            Err(FactRegistryError::UnknownFact(_))
        ));
    }
    #[test]
    fn active_pack_maps_all_271_facts_and_39_conditions() {
        let registry = crate::active_pack_set().facts();
        let graph = registry.canonical_thesis_graph().unwrap();
        assert_eq!(registry.len(), 271);
        assert_eq!(graph.theses().len(), 271);
        assert_eq!(graph.relations().len(), 39);
        assert_eq!(
            graph
                .relations()
                .iter()
                .filter(|edge| edge.kind == ThesisRelationKind::Counters)
                .count(),
            30
        );
        assert_eq!(
            graph
                .relations()
                .iter()
                .filter(|edge| edge.kind == ThesisRelationKind::FollowsFrom)
                .count(),
            9
        );
        graph.validate().unwrap();
    }

    #[test]
    fn fact_adapter_preserves_slots_and_excludes_legacy_metadata_from_digest() {
        let fact = record("fact.adapter", FactStatus::Curated);
        let thesis = fact.canonical_thesis().unwrap();
        assert_eq!(thesis.id.as_str(), fact.id.as_str());
        assert_eq!(thesis.subject, fact.subject);
        assert_eq!(thesis.predicate.as_str(), fact.relation.as_str());
        assert_eq!(thesis.object, fact.object);

        let mut changed = fact.clone();
        changed.id = FactId::try_new("fact.other-id").unwrap();
        changed.confidence_basis_points = 1;
        changed.source_pack = "other-pack".into();
        changed.source_ref = "other-ref".into();
        changed.valid_from = Some("2099-01-01".into());
        assert_eq!(
            thesis.canonical_digest().unwrap(),
            changed
                .canonical_thesis()
                .unwrap()
                .canonical_digest()
                .unwrap()
        );
    }
}

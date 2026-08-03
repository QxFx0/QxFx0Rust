//! Immutable, curated fact model separated from graph edges and dialogue state.

use crate::{ConceptResolver, PredicateRef, SemanticId};
pub use qxfx0_types::FactId;
use qxfx0_types::{ConceptId, RelationType};
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
}

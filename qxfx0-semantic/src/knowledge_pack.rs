//! Immutable, manifest-validated semantic knowledge packs.

use crate::{
    ConceptManifest, ConceptRecord, ConceptResolver, FactRecord, FactRegistry, PredicateRef,
    SemanticId, TypedRelationModel,
};
use qxfx0_types::{AtomGraph, ConceptId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KnowledgePackManifest {
    pub pack_id: String,
    pub pack_version: u32,
    pub schema_version: u32,
    pub source_repository: String,
    pub source_commit: String,
    pub license: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PackRelationRecord {
    pub semantic_id: SemanticId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
struct PackFactBinding {
    predicate_ref: PredicateRef,
    record: FactRecord,
}

#[derive(Debug, Clone, Copy)]
pub struct KnowledgePackSource<'a> {
    pub manifest: &'a [u8],
    pub concepts: &'a [u8],
    pub facts: &'a [u8],
    pub relations: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct KnowledgePackSummary {
    pub pack_id: String,
    pub pack_version: u32,
    pub schema_version: u32,
    pub concept_count: usize,
    pub fact_count: usize,
    pub relation_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum KnowledgePackError {
    #[error("knowledge pack JSON parse failed: {0}")]
    Json(String),
    #[error("knowledge pack validation failed: {0}")]
    Validation(String),
    #[error("duplicate active pack id: {0}")]
    DuplicatePackId(String),
    #[error("duplicate concept id across active packs: {0}")]
    DuplicateConceptId(String),
    #[error("duplicate fact id across active packs: {0}")]
    DuplicateFactId(String),
    #[error("fact conflict for subject={subject}, relation={relation}: objects={objects:?}")]
    FactConflict {
        subject: String,
        relation: String,
        objects: Vec<String>,
    },
}

#[derive(Debug, Clone)]
pub struct KnowledgePackSet {
    summaries: Vec<KnowledgePackSummary>,
    resolver: ConceptResolver,
    facts: FactRegistry,
    fingerprint: String,
    ambiguous_alias_count: usize,
    fact_conflict_count: usize,
}

impl KnowledgePackSet {
    pub fn load(
        sources: &[KnowledgePackSource<'_>],
        graph: &AtomGraph,
    ) -> Result<Self, KnowledgePackError> {
        if sources.is_empty() {
            return Err(KnowledgePackError::Validation(
                "active pack set must not be empty".into(),
            ));
        }

        let typed_relations = TypedRelationModel::default();
        let mut pack_ids = BTreeSet::new();
        let mut concept_ids = BTreeSet::new();
        let mut fact_ids = BTreeSet::new();
        let mut all_concepts = Vec::new();
        let mut all_facts = Vec::new();
        let mut predicate_facts = Vec::new();
        let mut summaries = Vec::new();
        let mut pack_fingerprints = Vec::new();
        let mut conflict_index: BTreeMap<(ConceptId, SemanticId), BTreeSet<ConceptId>> =
            BTreeMap::new();

        for source in sources {
            let manifest: KnowledgePackManifest = serde_json::from_slice(source.manifest)
                .map_err(|error| KnowledgePackError::Json(error.to_string()))?;
            validate_manifest(&manifest, source)?;
            if !pack_ids.insert(manifest.pack_id.clone()) {
                return Err(KnowledgePackError::DuplicatePackId(manifest.pack_id));
            }

            let concepts: Vec<ConceptRecord> = serde_json::from_slice(source.concepts)
                .map_err(|error| KnowledgePackError::Json(error.to_string()))?;
            let facts: Vec<PackFactBinding> = serde_json::from_slice(source.facts)
                .map_err(|error| KnowledgePackError::Json(error.to_string()))?;
            let relations: Vec<PackRelationRecord> = serde_json::from_slice(source.relations)
                .map_err(|error| KnowledgePackError::Json(error.to_string()))?;
            if concepts.is_empty() || relations.is_empty() {
                return Err(KnowledgePackError::Validation(format!(
                    "pack '{}' must contain concepts and typed relations",
                    manifest.pack_id
                )));
            }

            let mut relation_ids = BTreeSet::new();
            for relation in &relations {
                if !typed_relations.contains(&relation.semantic_id) {
                    return Err(KnowledgePackError::Validation(format!(
                        "pack '{}' contains unknown typed relation '{}'",
                        manifest.pack_id,
                        relation.semantic_id.as_str()
                    )));
                }
                if !relation_ids.insert(relation.semantic_id.clone()) {
                    return Err(KnowledgePackError::Validation(format!(
                        "pack '{}' repeats relation '{}'",
                        manifest.pack_id,
                        relation.semantic_id.as_str()
                    )));
                }
            }

            for concept in concepts {
                if concept.source_pack != manifest.pack_id {
                    return Err(KnowledgePackError::Validation(format!(
                        "concept '{}' declares source_pack '{}', expected '{}'",
                        concept.concept_id.0, concept.source_pack, manifest.pack_id
                    )));
                }
                if !concept_ids.insert(concept.concept_id.clone()) {
                    return Err(KnowledgePackError::DuplicateConceptId(concept.concept_id.0));
                }
                all_concepts.push(concept);
            }

            for binding in facts {
                if binding.record.source_pack != manifest.pack_id {
                    return Err(KnowledgePackError::Validation(format!(
                        "fact '{}' declares source_pack '{}', expected '{}'",
                        binding.record.id, binding.record.source_pack, manifest.pack_id
                    )));
                }
                if !relation_ids.contains(&binding.record.relation) {
                    return Err(KnowledgePackError::Validation(format!(
                        "fact '{}' uses relation '{}' absent from pack '{}'",
                        binding.record.id,
                        binding.record.relation.as_str(),
                        manifest.pack_id
                    )));
                }
                if !fact_ids.insert(binding.record.id.clone()) {
                    return Err(KnowledgePackError::DuplicateFactId(binding.record.id.0));
                }
                conflict_index
                    .entry((
                        binding.record.subject.clone(),
                        binding.record.relation.clone(),
                    ))
                    .or_default()
                    .insert(binding.record.object.clone());
                predicate_facts.push((binding.predicate_ref, binding.record.id.clone()));
                all_facts.push(binding.record);
            }

            summaries.push(KnowledgePackSummary {
                pack_id: manifest.pack_id.clone(),
                pack_version: manifest.pack_version,
                schema_version: manifest.schema_version,
                concept_count: all_concepts
                    .iter()
                    .filter(|record| record.source_pack == manifest.pack_id)
                    .count(),
                fact_count: all_facts
                    .iter()
                    .filter(|record| record.source_pack == manifest.pack_id)
                    .count(),
                relation_count: relations.len(),
            });
            pack_fingerprints.push((
                manifest.pack_id,
                manifest.pack_version,
                format!("{:x}", Sha256::digest(source.manifest)),
            ));
        }

        for ((subject, relation), objects) in conflict_index {
            if objects.len() > 1 {
                return Err(KnowledgePackError::FactConflict {
                    subject: subject.0,
                    relation: relation.as_str().into(),
                    objects: objects.into_iter().map(|object| object.0).collect(),
                });
            }
        }

        summaries.sort_by(|left, right| left.pack_id.cmp(&right.pack_id));
        pack_fingerprints.sort();
        let fingerprint_bytes = serde_json::to_vec(&pack_fingerprints)
            .map_err(|error| KnowledgePackError::Json(error.to_string()))?;
        let fingerprint = format!("{:x}", Sha256::digest(fingerprint_bytes));
        let concept_manifest = summaries.first().map(|summary| ConceptManifest {
            pack_id: summary.pack_id.clone(),
            schema_version: summary.schema_version,
            source_repository: "active-knowledge-pack-set".into(),
            source_commit: fingerprint.clone(),
            license: "MIT".into(),
            files: BTreeMap::new(),
        });
        let resolver = ConceptResolver::from_records(
            all_concepts,
            graph,
            concept_manifest,
            fingerprint.clone(),
        )
        .map_err(|error| KnowledgePackError::Validation(error.to_string()))?;
        let ambiguous_alias_count = resolver.ambiguous_alias_count();
        let facts = FactRegistry::load(all_facts, predicate_facts, &resolver, &typed_relations)
            .map_err(|error| KnowledgePackError::Validation(error.to_string()))?;

        Ok(Self {
            summaries,
            resolver,
            facts,
            fingerprint,
            ambiguous_alias_count,
            fact_conflict_count: 0,
        })
    }

    pub fn summaries(&self) -> &[KnowledgePackSummary] {
        &self.summaries
    }

    pub fn resolver(&self) -> &ConceptResolver {
        &self.resolver
    }

    pub fn facts(&self) -> &FactRegistry {
        &self.facts
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn ambiguous_alias_count(&self) -> usize {
        self.ambiguous_alias_count
    }

    pub fn fact_conflict_count(&self) -> usize {
        self.fact_conflict_count
    }
}

fn validate_manifest(
    manifest: &KnowledgePackManifest,
    source: &KnowledgePackSource<'_>,
) -> Result<(), KnowledgePackError> {
    if manifest.pack_id.trim().is_empty()
        || manifest.pack_version == 0
        || manifest.schema_version != 1
        || manifest.source_repository.trim().is_empty()
        || manifest.license != "MIT"
        || !is_full_commit(&manifest.source_commit)
    {
        return Err(KnowledgePackError::Validation(
            "invalid pack identity, version, provenance, or license".into(),
        ));
    }
    let files = [
        ("concepts.json", source.concepts),
        ("facts.json", source.facts),
        ("relations.json", source.relations),
    ];
    if manifest.files.len() != files.len() {
        return Err(KnowledgePackError::Validation(format!(
            "pack '{}' manifest must hash exactly concepts.json, facts.json, and relations.json",
            manifest.pack_id
        )));
    }
    for (name, bytes) in files {
        let expected = manifest.files.get(name).ok_or_else(|| {
            KnowledgePackError::Validation(format!(
                "pack '{}' manifest is missing hash for {name}",
                manifest.pack_id
            ))
        })?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if expected != &actual {
            return Err(KnowledgePackError::Validation(format!(
                "pack '{}' hash mismatch for {name}: expected {expected}, got {actual}",
                manifest.pack_id
            )));
        }
    }
    Ok(())
}

fn is_full_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.chars().all(|character| character.is_ascii_hexdigit())
}

pub fn active_pack_set() -> &'static KnowledgePackSet {
    static ACTIVE: OnceLock<KnowledgePackSet> = OnceLock::new();
    ACTIVE.get_or_init(|| {
        KnowledgePackSet::load(
            &[KnowledgePackSource {
                manifest: include_bytes!("../../data/packs/philosophy-core-v1/manifest.json"),
                concepts: include_bytes!("../../data/packs/philosophy-core-v1/concepts.json"),
                facts: include_bytes!("../../data/packs/philosophy-core-v1/facts.json"),
                relations: include_bytes!("../../data/packs/philosophy-core-v1/relations.json"),
            }],
            &crate::seed_graph(),
        )
        .expect("embedded active knowledge packs must be valid")
    })
}

/// SHA-256 digests of the embedded active pack asset bytes, keyed by file
/// name. The gates use these to lock the census manifests to the exact bytes
/// a release binary carries, so a drifted data file cannot silently change
/// what a manifest claims to approve.
pub fn active_pack_asset_digests() -> Vec<(&'static str, String)> {
    let digest = |bytes: &[u8]| format!("{:x}", Sha256::digest(bytes));
    vec![
        (
            "manifest.json",
            digest(include_bytes!(
                "../../data/packs/philosophy-core-v1/manifest.json"
            )),
        ),
        (
            "concepts.json",
            digest(include_bytes!(
                "../../data/packs/philosophy-core-v1/concepts.json"
            )),
        ),
        (
            "facts.json",
            digest(include_bytes!(
                "../../data/packs/philosophy-core-v1/facts.json"
            )),
        ),
        (
            "relations.json",
            digest(include_bytes!(
                "../../data/packs/philosophy-core-v1/relations.json"
            )),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};

    struct OwnedPack {
        manifest: Vec<u8>,
        concepts: Vec<u8>,
        facts: Vec<u8>,
        relations: Vec<u8>,
    }

    impl OwnedPack {
        fn new(pack_id: &str, concepts: Value, facts: Value) -> Self {
            let concepts = serde_json::to_vec(&concepts).unwrap();
            let facts = serde_json::to_vec(&facts).unwrap();
            let relations = serde_json::to_vec(&json!([
                {"semantic_id": "RelRelatedTo"}
            ]))
            .unwrap();
            let manifest = serde_json::to_vec(&json!({
                "pack_id": pack_id,
                "pack_version": 1,
                "schema_version": 1,
                "source_repository": "test",
                "source_commit": "0000000000000000000000000000000000000000",
                "license": "MIT",
                "files": {
                    "concepts.json": format!("{:x}", Sha256::digest(&concepts)),
                    "facts.json": format!("{:x}", Sha256::digest(&facts)),
                    "relations.json": format!("{:x}", Sha256::digest(&relations)),
                }
            }))
            .unwrap();
            Self {
                manifest,
                concepts,
                facts,
                relations,
            }
        }

        fn source(&self) -> KnowledgePackSource<'_> {
            KnowledgePackSource {
                manifest: &self.manifest,
                concepts: &self.concepts,
                facts: &self.facts,
                relations: &self.relations,
            }
        }
    }

    fn concept(pack: &str, id: &str, atom: &str, alias: &str) -> Value {
        json!({
            "concept_id": id,
            "graph_atom_id": atom,
            "canonical_lemma": alias,
            "aliases": [],
            "ontology_kind": "test_concept",
            "status": "curated",
            "source_pack": pack,
            "source_ref": format!("test:{id}"),
            "version": 1
        })
    }

    fn fact(pack: &str, id: &str, predicate: &str, subject: &str, object: &str) -> Value {
        json!({
            "predicate_ref": predicate,
            "record": {
                "id": id,
                "subject": subject,
                "relation": "RelRelatedTo",
                "object": object,
                "kind": "interpretive_claim",
                "conditions": [],
                "confidence_basis_points": 9000,
                "source_pack": pack,
                "source_ref": format!("test:{id}"),
                "valid_from": null,
                "valid_to": null,
                "status": "curated"
            }
        })
    }

    #[test]
    fn active_pack_is_manifest_valid_and_complete() {
        let packs = active_pack_set();
        assert_eq!(packs.summaries().len(), 1);
        assert_eq!(packs.summaries()[0].pack_id, "philosophy-core-v1");
        assert_eq!(packs.resolver().concept_count(), 137);
        assert_eq!(packs.facts().len(), 69);
        assert_eq!(packs.fact_conflict_count(), 0);
        assert_eq!(packs.fingerprint().len(), 64);
    }

    #[test]
    fn hash_mismatch_is_rejected_before_load() {
        let mut pack = OwnedPack::new(
            "test-pack-v1",
            json!([concept("test-pack-v1", "concept.test", "свобода", "тест")]),
            json!([]),
        );
        pack.concepts.push(b' ');

        assert!(matches!(
            KnowledgePackSet::load(&[pack.source()], &crate::seed_graph()),
            Err(KnowledgePackError::Validation(message)) if message.contains("hash mismatch")
        ));
    }

    #[test]
    fn duplicate_pack_and_concept_ids_are_rejected() {
        let first = OwnedPack::new(
            "pack-a",
            json!([concept("pack-a", "concept.shared", "свобода", "первый")]),
            json!([]),
        );
        assert!(matches!(
            KnowledgePackSet::load(&[first.source(), first.source()], &crate::seed_graph()),
            Err(KnowledgePackError::DuplicatePackId(_))
        ));

        let second = OwnedPack::new(
            "pack-b",
            json!([concept(
                "pack-b",
                "concept.shared",
                "ответственность",
                "второй"
            )]),
            json!([]),
        );
        assert!(matches!(
            KnowledgePackSet::load(&[first.source(), second.source()], &crate::seed_graph()),
            Err(KnowledgePackError::DuplicateConceptId(_))
        ));
    }

    #[test]
    fn duplicate_aliases_across_packs_become_ambiguity() {
        let first = OwnedPack::new(
            "pack-a",
            json!([concept("pack-a", "concept.a", "свобода", "общий термин")]),
            json!([]),
        );
        let second = OwnedPack::new(
            "pack-b",
            json!([concept(
                "pack-b",
                "concept.b",
                "ответственность",
                "общий термин"
            )]),
            json!([]),
        );
        let set = KnowledgePackSet::load(&[first.source(), second.source()], &crate::seed_graph())
            .unwrap();

        assert!(matches!(
            set.resolver().resolve("общий термин"),
            crate::ResolutionOutcome::Ambiguous(entries) if entries.len() == 2
        ));
        assert_eq!(set.ambiguous_alias_count(), 1);
    }

    #[test]
    fn duplicate_fact_ids_across_packs_are_rejected() {
        let first = OwnedPack::new(
            "pack-a",
            json!([
                concept("pack-a", "concept.subject", "свобода", "субъект"),
                concept("pack-a", "concept.object", "ответственность", "объект")
            ]),
            json!([fact(
                "pack-a",
                "fact.shared",
                "predicate.a",
                "concept.subject",
                "concept.object"
            )]),
        );
        let second = OwnedPack::new(
            "pack-b",
            json!([concept("pack-b", "concept.other", "истина", "другое")]),
            json!([fact(
                "pack-b",
                "fact.shared",
                "predicate.b",
                "concept.subject",
                "concept.object"
            )]),
        );

        assert!(matches!(
            KnowledgePackSet::load(&[first.source(), second.source()], &crate::seed_graph()),
            Err(KnowledgePackError::DuplicateFactId(_))
        ));
    }

    #[test]
    fn conflicting_facts_fail_closed() {
        let first = OwnedPack::new(
            "pack-a",
            json!([
                concept("pack-a", "concept.subject", "свобода", "субъект"),
                concept("pack-a", "concept.object-a", "ответственность", "объект а")
            ]),
            json!([fact(
                "pack-a",
                "fact.a",
                "predicate.a",
                "concept.subject",
                "concept.object-a"
            )]),
        );
        let second = OwnedPack::new(
            "pack-b",
            json!([concept("pack-b", "concept.object-b", "истина", "объект б")]),
            json!([fact(
                "pack-b",
                "fact.b",
                "predicate.b",
                "concept.subject",
                "concept.object-b"
            )]),
        );

        assert!(matches!(
            KnowledgePackSet::load(&[first.source(), second.source()], &crate::seed_graph()),
            Err(KnowledgePackError::FactConflict { .. })
        ));
    }
}

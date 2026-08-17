//! Immutable, manifest-validated semantic knowledge packs.

use crate::evidence_registry::EvidenceRegistry;
use crate::{
    ConceptManifest, ConceptRecord, ConceptResolver, FactRecord, FactRegistry, PredicateRef,
    SemanticId, TypedRelationModel,
};
use qxfx0_types::{
    AtomGraph, ConceptId, ConfidenceAssessment, EvidenceDigest, EvidenceRecord, FactId,
    ThesisDigest, ThesisEvidenceLink, ThesisId,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::OnceLock;

const MAX_PACKS: usize = 64;
const MAX_OVERLAY_THESES: usize = 1024;
const MAX_OVERLAY_RELATIONS: usize = 4096;
const MAX_LIFECYCLE_SCENARIOS: usize = 1024;
const MAX_METADATA_BYTES: usize = 2048;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgePackManifest {
    pub pack_id: String,
    pub pack_version: u32,
    pub schema_version: u32,
    pub source_repository: String,
    pub source_commit: String,
    pub license: String,
    #[serde(default)]
    pub dependencies: Vec<String>,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PackRelationRecord {
    pub semantic_id: SemanticId,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackEvidenceRecord {
    record: EvidenceRecord,
    canonical_digest: EvidenceDigest,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackFactBinding {
    predicate_ref: PredicateRef,
    record: FactRecord,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayThesisMetadata {
    pub thesis_id: String,
    pub authority_fact_id: FactId,
    pub thesis_digest: ThesisDigest,
    pub theme: String,
    pub tags: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum OverlayRelationKind {
    Supports,
    Counters,
    Contradicts,
    Qualifies,
    Entails,
    DependsOn,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayRelationMetadata {
    pub from: ThesisDigest,
    pub kind: OverlayRelationKind,
    pub to: ThesisDigest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OverlayLifecycleAction {
    Counterargument,
    Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OverlayLifecycleScenario {
    pub scenario_id: String,
    pub action: OverlayLifecycleAction,
    pub head: ThesisDigest,
    pub trigger: ThesisDigest,
    pub result: ThesisDigest,
    pub relation: OverlayRelationKind,
    pub rationale: String,
}

#[derive(Debug, Clone, Copy)]
pub struct KnowledgePackSource<'a> {
    pub manifest: &'a [u8],
    pub concepts: &'a [u8],
    pub facts: &'a [u8],
    pub theses: &'a [u8],
    pub relations: &'a [u8],
    pub lifecycle: &'a [u8],
    pub evidence: &'a [u8],
    pub evidence_links: &'a [u8],
    pub assessments: &'a [u8],
}

#[derive(Debug, Clone)]
pub struct KnowledgePackSummary {
    pub pack_id: String,
    pub pack_version: u32,
    pub schema_version: u32,
    pub concept_count: usize,
    pub fact_count: usize,
    pub thesis_count: usize,
    pub relation_count: usize,
    pub lifecycle_scenario_count: usize,
    pub evidence_count: usize,
    pub assessment_count: usize,
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
    overlay_theses: BTreeMap<ThesisDigest, OverlayThesisMetadata>,
    overlay_relations: BTreeSet<OverlayRelationMetadata>,
    lifecycle_scenarios: Vec<OverlayLifecycleScenario>,
    evidence: EvidenceRegistry,
    assessments: BTreeMap<qxfx0_types::AssessmentId, ConfidenceAssessment>,
}

struct Parsed<'a> {
    manifest: KnowledgePackManifest,
    source: &'a KnowledgePackSource<'a>,
}

impl KnowledgePackSet {
    pub fn load(
        sources: &[KnowledgePackSource<'_>],
        graph: &AtomGraph,
    ) -> Result<Self, KnowledgePackError> {
        if sources.is_empty() || sources.len() > MAX_PACKS {
            return Err(v("active pack set size is invalid"));
        }
        let mut parsed = Vec::new();
        let mut pack_ids = BTreeSet::new();
        for source in sources {
            let manifest: KnowledgePackManifest = parse(source.manifest)?;
            validate_manifest(&manifest, source)?;
            if !pack_ids.insert(manifest.pack_id.clone()) {
                return Err(KnowledgePackError::DuplicatePackId(manifest.pack_id));
            }
            parsed.push(Parsed { manifest, source });
        }
        for pack in &parsed {
            for dependency in &pack.manifest.dependencies {
                if dependency == &pack.manifest.pack_id || !pack_ids.contains(dependency) {
                    return Err(v(format!(
                        "pack '{}' has missing or cyclic dependency '{dependency}'",
                        pack.manifest.pack_id
                    )));
                }
            }
        }

        let typed_relations = TypedRelationModel::default();
        let mut concept_ids = BTreeSet::new();
        let mut fact_ids = BTreeSet::new();
        let mut all_concepts = Vec::new();
        let mut all_facts = Vec::new();
        let mut predicate_facts = Vec::new();
        let mut conflict_index: BTreeMap<(ConceptId, SemanticId), BTreeSet<ConceptId>> =
            BTreeMap::new();
        let mut summaries = Vec::new();

        for pack in parsed.iter().filter(|p| p.manifest.schema_version == 1) {
            let concepts: Vec<ConceptRecord> = parse(pack.source.concepts)?;
            let facts: Vec<PackFactBinding> = parse(pack.source.facts)?;
            let relations: Vec<PackRelationRecord> = parse(pack.source.relations)?;
            if concepts.is_empty() || relations.is_empty() {
                return Err(v(format!(
                    "pack '{}' must contain concepts and typed relations",
                    pack.manifest.pack_id
                )));
            }
            let mut relation_ids = BTreeSet::new();
            for relation in &relations {
                if !typed_relations.contains(&relation.semantic_id)
                    || !relation_ids.insert(relation.semantic_id.clone())
                {
                    return Err(v(format!(
                        "pack '{}' has unknown or repeated relation '{}'",
                        pack.manifest.pack_id,
                        relation.semantic_id.as_str()
                    )));
                }
            }
            for concept in concepts {
                if concept.source_pack != pack.manifest.pack_id {
                    return Err(v(format!(
                        "concept '{}' ownership mismatch",
                        concept.concept_id.0
                    )));
                }
                if !concept_ids.insert(concept.concept_id.clone()) {
                    return Err(KnowledgePackError::DuplicateConceptId(concept.concept_id.0));
                }
                all_concepts.push(concept);
            }
            for binding in facts {
                if binding.record.source_pack != pack.manifest.pack_id {
                    return Err(v(format!(
                        "fact '{}' ownership mismatch",
                        binding.record.id
                    )));
                }
                if !relation_ids.contains(&binding.record.relation) {
                    return Err(v(format!(
                        "fact '{}' uses undeclared relation",
                        binding.record.id
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
                pack_id: pack.manifest.pack_id.clone(),
                pack_version: pack.manifest.pack_version,
                schema_version: 1,
                concept_count: all_concepts
                    .iter()
                    .filter(|r| r.source_pack == pack.manifest.pack_id)
                    .count(),
                fact_count: all_facts
                    .iter()
                    .filter(|r| r.source_pack == pack.manifest.pack_id)
                    .count(),
                thesis_count: 0,
                relation_count: relations.len(),
                lifecycle_scenario_count: 0,
                evidence_count: 0,
                assessment_count: 0,
            });
        }
        if !pack_ids.contains("philosophy-core-v1") {
            return Err(v("philosophy-core-v1 authority pack is required"));
        }
        for ((subject, relation), objects) in conflict_index {
            if objects.len() > 1 {
                return Err(KnowledgePackError::FactConflict {
                    subject: subject.0,
                    relation: relation.as_str().into(),
                    objects: objects.into_iter().map(|o| o.0).collect(),
                });
            }
        }
        let authority_manifest = summaries.first().map(|s| ConceptManifest {
            pack_id: s.pack_id.clone(),
            schema_version: s.schema_version,
            source_repository: "active-knowledge-pack-set".into(),
            source_commit: "pending".into(),
            license: "MIT".into(),
            files: BTreeMap::new(),
        });
        let resolver = ConceptResolver::from_records(
            all_concepts,
            graph,
            authority_manifest,
            "pending".into(),
        )
        .map_err(|e| v(e.to_string()))?;
        let ambiguous_alias_count = resolver.ambiguous_alias_count();
        let facts = FactRegistry::load(all_facts, predicate_facts, &resolver, &typed_relations)
            .map_err(|e| v(e.to_string()))?;

        let mut overlay_theses = BTreeMap::new();
        let mut thesis_ids = BTreeSet::new();
        let mut overlay_relations = BTreeSet::new();
        let mut lifecycle_scenarios = Vec::new();
        let mut scenario_ids = BTreeSet::new();
        let mut evidence_rows: Vec<(EvidenceRecord, EvidenceDigest)> = Vec::new();
        let mut evidence_links: Vec<ThesisEvidenceLink> = Vec::new();
        let mut declared_assessments: Vec<ConfidenceAssessment> = Vec::new();
        let mut known_theses = BTreeMap::new();
        for pack in parsed.iter().filter(|p| p.manifest.schema_version == 2) {
            if pack.manifest.dependencies != ["philosophy-core-v1"] {
                return Err(v(format!(
                    "overlay '{}' must depend exactly on philosophy-core-v1",
                    pack.manifest.pack_id
                )));
            }
            let theses: Vec<OverlayThesisMetadata> = parse(pack.source.theses)?;
            let relations: Vec<OverlayRelationMetadata> = parse(pack.source.relations)?;
            let scenarios: Vec<OverlayLifecycleScenario> = parse(pack.source.lifecycle)?;
            if theses.len() < 4
                || theses.len() > MAX_OVERLAY_THESES
                || relations.len() < 6
                || relations.len() > MAX_OVERLAY_RELATIONS
                || scenarios.is_empty()
                || scenarios.len() > MAX_LIFECYCLE_SCENARIOS
            {
                return Err(v(format!(
                    "overlay '{}' violates content bounds",
                    pack.manifest.pack_id
                )));
            }
            let mut local = BTreeSet::new();
            for thesis in &theses {
                bounded("thesis_id", &thesis.thesis_id)?;
                bounded("theme", &thesis.theme)?;
                if thesis.tags.is_empty()
                    || thesis.tags.len() > 32
                    || !thesis_ids.insert(thesis.thesis_id.clone())
                {
                    return Err(v("invalid or duplicate overlay thesis id/tags"));
                }
                for tag in &thesis.tags {
                    bounded("tag", tag)?;
                }
                let authority = facts.get(&thesis.authority_fact_id).ok_or_else(|| {
                    v(format!(
                        "dangling authority fact '{}'",
                        thesis.authority_fact_id
                    ))
                })?;
                let actual = authority
                    .canonical_thesis()
                    .map_err(|e| v(e.to_string()))?
                    .canonical_digest()
                    .map_err(|e| v(e.to_string()))?;
                if actual != thesis.thesis_digest {
                    return Err(v(format!(
                        "tampered thesis digest for '{}'",
                        thesis.thesis_id
                    )));
                }
                known_theses.insert(
                    ThesisId::try_new(thesis.thesis_id.clone()).map_err(|e| v(e.to_string()))?,
                    actual,
                );
                if !local.insert(actual) || overlay_theses.insert(actual, thesis.clone()).is_some()
                {
                    return Err(v("duplicate thesis digest ownership"));
                }
            }
            let kinds = relations.iter().map(|r| r.kind).collect::<BTreeSet<_>>();
            if kinds.len() != 6 {
                return Err(v(format!(
                    "overlay '{}' lacks required relation coverage",
                    pack.manifest.pack_id
                )));
            }
            for relation in &relations {
                if relation.from == relation.to
                    || !local.contains(&relation.from)
                    || !local.contains(&relation.to)
                    || !overlay_relations.insert(relation.clone())
                {
                    return Err(v("dangling, self, or duplicate overlay relation"));
                }
            }
            for scenario in &scenarios {
                bounded("scenario_id", &scenario.scenario_id)?;
                bounded("rationale", &scenario.rationale)?;
                if !scenario_ids.insert(scenario.scenario_id.clone())
                    || !local.contains(&scenario.head)
                    || !local.contains(&scenario.trigger)
                    || !local.contains(&scenario.result)
                {
                    return Err(v("duplicate scenario or dangling lifecycle endpoint"));
                }
                if !relations.iter().any(|r| {
                    r.from == scenario.trigger
                        && r.to == scenario.head
                        && r.kind == scenario.relation
                }) {
                    return Err(v("lifecycle trigger relation is not explicit"));
                }
                match scenario.action {
                    OverlayLifecycleAction::Counterargument
                        if !matches!(
                            scenario.relation,
                            OverlayRelationKind::Counters | OverlayRelationKind::Contradicts
                        ) =>
                    {
                        return Err(v("counterargument scenario requires Counters/Contradicts"))
                    }
                    OverlayLifecycleAction::Revision if scenario.result == scenario.head => {
                        return Err(v("revision scenario must change head"))
                    }
                    _ => {}
                }
                lifecycle_scenarios.push(scenario.clone());
            }
            let pack_evidence: Vec<PackEvidenceRecord> = parse(pack.source.evidence)?;
            let pack_links: Vec<ThesisEvidenceLink> = parse(pack.source.evidence_links)?;
            let pack_assessments: Vec<ConfidenceAssessment> = parse(pack.source.assessments)?;
            if pack_evidence.is_empty() || pack_links.is_empty() || pack_assessments.is_empty() {
                return Err(v("overlay evidence census must be non-empty"));
            }
            let evidence_count = pack_evidence.len();
            let assessment_count = pack_assessments.len();
            evidence_rows.extend(
                pack_evidence
                    .into_iter()
                    .map(|x| (x.record, x.canonical_digest)),
            );
            evidence_links.extend(pack_links);
            declared_assessments.extend(pack_assessments);
            summaries.push(KnowledgePackSummary {
                pack_id: pack.manifest.pack_id.clone(),
                pack_version: pack.manifest.pack_version,
                schema_version: 2,
                concept_count: 0,
                fact_count: 0,
                thesis_count: theses.len(),
                relation_count: relations.len(),
                lifecycle_scenario_count: scenarios.len(),
                evidence_count,
                assessment_count,
            });
        }
        let evidence = EvidenceRegistry::load(evidence_rows, evidence_links, &known_theses)
            .map_err(|e| v(e.to_string()))?;
        let mut assessments = BTreeMap::new();
        for assessment in declared_assessments {
            evidence
                .verify_assessment(&assessment)
                .map_err(|e| v(e.to_string()))?;
            if assessments
                .insert(assessment.id.clone(), assessment)
                .is_some()
            {
                return Err(v("duplicate assessment id"));
            }
        }
        summaries.sort_by(|a, b| a.pack_id.cmp(&b.pack_id));
        lifecycle_scenarios.sort_by(|a, b| a.scenario_id.cmp(&b.scenario_id));
        let mut fps = parsed
            .iter()
            .map(|p| {
                (
                    p.manifest.pack_id.clone(),
                    p.manifest.pack_version,
                    format!("{:x}", Sha256::digest(p.source.manifest)),
                )
            })
            .collect::<Vec<_>>();
        fps.sort();
        let fingerprint = format!(
            "{:x}",
            Sha256::digest(
                serde_json::to_vec(&fps).map_err(|e| KnowledgePackError::Json(e.to_string()))?
            )
        );
        Ok(Self {
            summaries,
            resolver,
            facts,
            fingerprint,
            ambiguous_alias_count,
            fact_conflict_count: 0,
            overlay_theses,
            overlay_relations,
            lifecycle_scenarios,
            evidence,
            assessments,
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
    pub fn overlay_theses(&self) -> &BTreeMap<ThesisDigest, OverlayThesisMetadata> {
        &self.overlay_theses
    }
    pub fn overlay_relations(&self) -> &BTreeSet<OverlayRelationMetadata> {
        &self.overlay_relations
    }
    pub fn lifecycle_scenarios(&self) -> &[OverlayLifecycleScenario] {
        &self.lifecycle_scenarios
    }
    pub fn evidence(&self) -> &EvidenceRegistry {
        &self.evidence
    }
    pub fn assessments(&self) -> &BTreeMap<qxfx0_types::AssessmentId, ConfidenceAssessment> {
        &self.assessments
    }
}

fn parse<T: for<'de> Deserialize<'de>>(bytes: &[u8]) -> Result<T, KnowledgePackError> {
    serde_json::from_slice(bytes).map_err(|e| KnowledgePackError::Json(e.to_string()))
}
fn v(message: impl Into<String>) -> KnowledgePackError {
    KnowledgePackError::Validation(message.into())
}
fn bounded(name: &str, value: &str) -> Result<(), KnowledgePackError> {
    if value.trim().is_empty() || value.len() > MAX_METADATA_BYTES {
        Err(v(format!("{name} is empty or exceeds bound")))
    } else {
        Ok(())
    }
}
fn validate_manifest(
    manifest: &KnowledgePackManifest,
    source: &KnowledgePackSource<'_>,
) -> Result<(), KnowledgePackError> {
    bounded("pack_id", &manifest.pack_id)?;
    if manifest.pack_version == 0
        || !matches!(manifest.schema_version, 1 | 2)
        || manifest.source_repository.trim().is_empty()
        || manifest.license != "MIT"
        || !is_full_commit(&manifest.source_commit)
    {
        return Err(v(
            "invalid pack identity, version, schema, provenance, or license",
        ));
    }
    let files: Vec<(&str, &[u8])> = match manifest.schema_version {
        1 => {
            if !source.theses.is_empty()
                || !source.lifecycle.is_empty()
                || !source.evidence.is_empty()
                || !source.evidence_links.is_empty()
                || !source.assessments.is_empty()
            {
                return Err(v("v1 source contains v2 payloads"));
            }
            vec![
                ("concepts.json", source.concepts),
                ("facts.json", source.facts),
                ("relations.json", source.relations),
            ]
        }
        2 => {
            if !source.concepts.is_empty() || !source.facts.is_empty() {
                return Err(v("v2 overlay contains authority payloads"));
            }
            vec![
                ("theses.json", source.theses),
                ("relations.json", source.relations),
                ("lifecycle.json", source.lifecycle),
                ("evidence.json", source.evidence),
                ("evidence-links.json", source.evidence_links),
                ("assessments.json", source.assessments),
            ]
        }
        _ => unreachable!(),
    };
    if manifest.files.len() != files.len() {
        return Err(v(format!(
            "pack '{}' manifest file census mismatch",
            manifest.pack_id
        )));
    }
    for (name, bytes) in files {
        let expected = manifest
            .files
            .get(name)
            .ok_or_else(|| v(format!("missing hash for {name}")))?;
        let actual = format!("{:x}", Sha256::digest(bytes));
        if expected != &actual {
            return Err(v(format!(
                "pack '{}' hash mismatch for {name}",
                manifest.pack_id
            )));
        }
    }
    Ok(())
}
fn is_full_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.chars().all(|c| c.is_ascii_hexdigit())
}

macro_rules! embedded_source {
    ($id:literal, false) => {{
        KnowledgePackSource {
            manifest: include_bytes!(concat!("../../data/packs/", $id, "/manifest.json")),
            concepts: include_bytes!(concat!("../../data/packs/", $id, "/concepts.json")),
            facts: include_bytes!(concat!("../../data/packs/", $id, "/facts.json")),
            theses: b"",
            relations: include_bytes!(concat!("../../data/packs/", $id, "/relations.json")),
            lifecycle: b"",
            evidence: b"",
            evidence_links: b"",
            assessments: b"",
        }
    }};
    ($id:literal, true) => {{
        KnowledgePackSource {
            manifest: include_bytes!(concat!("../../data/packs/", $id, "/manifest.json")),
            concepts: b"",
            facts: b"",
            theses: include_bytes!(concat!("../../data/packs/", $id, "/theses.json")),
            relations: include_bytes!(concat!("../../data/packs/", $id, "/relations.json")),
            lifecycle: include_bytes!(concat!("../../data/packs/", $id, "/lifecycle.json")),
            evidence: include_bytes!(concat!("../../data/packs/", $id, "/evidence.json")),
            evidence_links: include_bytes!(concat!(
                "../../data/packs/",
                $id,
                "/evidence-links.json"
            )),
            assessments: include_bytes!(concat!("../../data/packs/", $id, "/assessments.json")),
        }
    }};
}

pub fn active_pack_set() -> &'static KnowledgePackSet {
    static ACTIVE: OnceLock<KnowledgePackSet> = OnceLock::new();
    ACTIVE.get_or_init(|| {
        let registry = crate::catalog_registry();
        let allowlist = crate::embedded_activation_allowlist();
        let requested = allowlist.iter().cloned().collect::<Vec<_>>();
        let activated = registry
            .activate(&requested, &allowlist)
            .expect("embedded active pack pins are release-validated");
        assert_eq!(activated.len(), 4, "active catalog census changed");
        KnowledgePackSet::load(
            &[
                embedded_source!("philosophy-core-v1", false),
                embedded_source!("agency-responsibility-v1", true),
                embedded_source!("epistemology-truth-v1", true),
                embedded_source!("mind-memory-language-v1", true),
            ],
            &crate::seed_graph(),
        )
        .expect("embedded active knowledge packs are release-validated")
    })
}

pub fn active_pack_asset_digests() -> Vec<(&'static str, String)> {
    let d = |b: &[u8]| format!("{:x}", Sha256::digest(b));
    vec![
        (
            "manifest.json",
            d(include_bytes!(
                "../../data/packs/philosophy-core-v1/manifest.json"
            )),
        ),
        (
            "concepts.json",
            d(include_bytes!(
                "../../data/packs/philosophy-core-v1/concepts.json"
            )),
        ),
        (
            "facts.json",
            d(include_bytes!(
                "../../data/packs/philosophy-core-v1/facts.json"
            )),
        ),
        (
            "relations.json",
            d(include_bytes!(
                "../../data/packs/philosophy-core-v1/relations.json"
            )),
        ),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sources() -> [KnowledgePackSource<'static>; 4] {
        [
            embedded_source!("philosophy-core-v1", false),
            embedded_source!("agency-responsibility-v1", true),
            embedded_source!("epistemology-truth-v1", true),
            embedded_source!("mind-memory-language-v1", true),
        ]
    }
    #[test]
    fn active_pack_census_and_coverage() {
        let p = active_pack_set();
        assert_eq!(p.summaries().len(), 4);
        assert_eq!(p.resolver().concept_count(), 166);
        assert_eq!(p.facts().len(), 129);
        assert_eq!(p.overlay_theses().len(), 15);
        assert_eq!(p.overlay_relations().len(), 21);
        assert_eq!(p.lifecycle_scenarios().len(), 6);
        assert_eq!(p.evidence().records().len(), 15);
        assert_eq!(p.evidence().links().len(), 15);
        assert_eq!(p.assessments().len(), 15);
        for s in p.summaries().iter().filter(|s| s.schema_version == 2) {
            assert!(
                s.thesis_count >= 4 && s.lifecycle_scenario_count >= 1 && s.relation_count >= 6
            );
            assert_eq!(s.concept_count, 0);
            assert_eq!(s.fact_count, 0);
            assert_eq!(s.evidence_count, s.thesis_count);
            assert_eq!(s.assessment_count, s.thesis_count);
        }
    }
    #[test]
    fn source_order_is_invariant() {
        let a = sources();
        let mut b = sources();
        b.reverse();
        let x = KnowledgePackSet::load(&a, &crate::seed_graph()).unwrap();
        let y = KnowledgePackSet::load(&b, &crate::seed_graph()).unwrap();
        assert_eq!(x.fingerprint(), y.fingerprint());
        assert_eq!(
            x.summaries().iter().map(|s| &s.pack_id).collect::<Vec<_>>(),
            y.summaries().iter().map(|s| &s.pack_id).collect::<Vec<_>>()
        )
    }
    #[test]
    fn tampered_hash_fails() {
        let a = sources();
        let mut bad = a[1].theses.to_vec();
        bad.push(b' ');
        let source = KnowledgePackSource {
            theses: &bad,
            ..a[1]
        };
        assert!(
            matches!(KnowledgePackSet::load(&[a[0],source],&crate::seed_graph()),Err(KnowledgePackError::Validation(m))if m.contains("hash mismatch"))
        )
    }
    #[test]
    fn dangling_digest_fails_after_rehash() {
        let a = sources();
        let mut rel: serde_json::Value = serde_json::from_slice(a[1].relations).unwrap();
        rel[0]["from"] =
            serde_json::json!("0000000000000000000000000000000000000000000000000000000000000000");
        let relations = serde_json::to_vec(&rel).unwrap();
        let mut man: serde_json::Value = serde_json::from_slice(a[1].manifest).unwrap();
        man["files"]["relations.json"] =
            serde_json::json!(format!("{:x}", Sha256::digest(&relations)));
        let manifest = serde_json::to_vec(&man).unwrap();
        let source = KnowledgePackSource {
            manifest: &manifest,
            relations: &relations,
            ..a[1]
        };
        assert!(matches!(
            KnowledgePackSet::load(&[a[0], source], &crate::seed_graph()),
            Err(KnowledgePackError::Validation(_))
        ))
    }
    #[test]
    fn duplicated_ownership_fails() {
        let a = sources();
        assert!(matches!(
            KnowledgePackSet::load(&[a[0], a[1], a[1]], &crate::seed_graph()),
            Err(KnowledgePackError::DuplicatePackId(_))
        ))
    }
}

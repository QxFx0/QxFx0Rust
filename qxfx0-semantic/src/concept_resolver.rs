//! Asset-backed resolution of lexical surfaces to stable concept identities.

use qxfx0_types::{AtomGraph, AtomId, ConceptId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

/// The semantic identity used by the graph pipeline. `ConceptId` never replaces
/// `AtomId`; the latter remains the graph key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptEntry {
    pub concept_id: ConceptId,
    pub atom_id: AtomId,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResolutionOutcome {
    Resolved(ConceptEntry),
    Ambiguous(Vec<ConceptEntry>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConceptRecord {
    pub concept_id: ConceptId,
    pub graph_atom_id: String,
    pub canonical_lemma: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub ontology_kind: String,
    pub status: String,
    pub source_pack: String,
    pub source_ref: String,
    pub version: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ConceptManifest {
    pub pack_id: String,
    pub schema_version: u32,
    pub source_repository: String,
    pub source_commit: String,
    pub license: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ConceptRegistryError {
    #[error("concept registry JSON parse failed: {0}")]
    Json(String),
    #[error("concept registry validation failed: {0}")]
    Validation(String),
}

/// Versioned registry. Alias values are vectors intentionally: an alias that
/// belongs to multiple concepts is an explicit ambiguity, never an overwrite.
#[derive(Debug, Clone, Default)]
pub struct ConceptResolver {
    registry: BTreeMap<String, Vec<ConceptEntry>>,
    records: BTreeMap<ConceptId, ConceptRecord>,
    manifest: Option<ConceptManifest>,
    fingerprint: String,
}

impl ConceptResolver {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an in-memory mapping, primarily for unit tests and adapters.
    pub fn register(&mut self, phrase: &str, concept_id: ConceptId, atom_id: AtomId) {
        let normalized = normalize_alias(phrase);
        if normalized.is_empty() {
            return;
        }
        let entry = ConceptEntry {
            concept_id,
            atom_id,
        };
        let aliases = self.registry.entry(normalized).or_default();
        if !aliases.contains(&entry) {
            aliases.push(entry);
            aliases.sort_by(|left, right| left.concept_id.cmp(&right.concept_id));
        }
    }

    pub fn load_from_bytes(
        concepts_bytes: &[u8],
        manifest_bytes: &[u8],
        graph: &AtomGraph,
    ) -> Result<Self, ConceptRegistryError> {
        let manifest: ConceptManifest = serde_json::from_slice(manifest_bytes)
            .map_err(|error| ConceptRegistryError::Json(error.to_string()))?;
        validate_manifest(&manifest, concepts_bytes)?;
        let records: Vec<ConceptRecord> = serde_json::from_slice(concepts_bytes)
            .map_err(|error| ConceptRegistryError::Json(error.to_string()))?;
        if records.is_empty() {
            return Err(ConceptRegistryError::Validation("registry is empty".into()));
        }

        Self::from_records(
            records,
            graph,
            Some(manifest),
            format!("{:x}", Sha256::digest(concepts_bytes)),
        )
    }

    pub(crate) fn from_records(
        records: Vec<ConceptRecord>,
        graph: &AtomGraph,
        manifest: Option<ConceptManifest>,
        fingerprint: String,
    ) -> Result<Self, ConceptRegistryError> {
        let mut resolver = Self {
            registry: BTreeMap::new(),
            records: BTreeMap::new(),
            manifest,
            fingerprint,
        };
        for record in records {
            if resolver.records.contains_key(&record.concept_id) {
                return Err(ConceptRegistryError::Validation(format!(
                    "duplicate concept ID: {}",
                    record.concept_id.0
                )));
            }
            if !graph
                .atoms
                .contains_key(&AtomId::new(record.graph_atom_id.clone()))
            {
                return Err(ConceptRegistryError::Validation(format!(
                    "dangling graph atom ID: {}",
                    record.graph_atom_id
                )));
            }
            if record.canonical_lemma.trim().is_empty()
                || record.source_pack.trim().is_empty()
                || record.source_ref.trim().is_empty()
                || record.version == 0
            {
                return Err(ConceptRegistryError::Validation(format!(
                    "incomplete record: {}",
                    record.concept_id.0
                )));
            }
            let entry = ConceptEntry {
                concept_id: record.concept_id.clone(),
                atom_id: AtomId::new(record.graph_atom_id.clone()),
            };
            resolver.register(
                &record.canonical_lemma,
                entry.concept_id.clone(),
                entry.atom_id.clone(),
            );
            for alias in &record.aliases {
                resolver.register(alias, entry.concept_id.clone(), entry.atom_id.clone());
            }
            resolver.records.insert(record.concept_id.clone(), record);
        }
        Ok(resolver)
    }

    pub fn resolve(&self, phrase: &str) -> ResolutionOutcome {
        let normalized = normalize_alias(phrase);
        if normalized.is_empty() {
            return ResolutionOutcome::Unknown;
        }
        if let Some(outcome) = self.resolve_spans(normalized.split_whitespace().collect()) {
            return outcome;
        }

        let morph = qxfx0_morphology::get_runtime();
        let words: Vec<String> = normalized
            .split_whitespace()
            .map(|word| match morph.lemmatize(word) {
                qxfx0_types::morphology::MorphologyLookup::Resolved(result) => result.lemma,
                _ => word.to_string(),
            })
            .collect();
        self.resolve_spans(words.iter().map(String::as_str).collect())
            .unwrap_or(ResolutionOutcome::Unknown)
    }

    fn resolve_spans(&self, words: Vec<&str>) -> Option<ResolutionOutcome> {
        for length in (1..=words.len()).rev() {
            for start in 0..=(words.len() - length) {
                let candidate = words[start..start + length].join(" ");
                if let Some(entries) = self.registry.get(&candidate) {
                    return Some(if entries.len() == 1 {
                        ResolutionOutcome::Resolved(entries[0].clone())
                    } else {
                        ResolutionOutcome::Ambiguous(entries.clone())
                    });
                }
            }
        }
        None
    }

    pub fn records(&self) -> impl Iterator<Item = &ConceptRecord> {
        self.records.values()
    }

    pub fn concept_count(&self) -> usize {
        self.records.len()
    }

    pub fn ambiguous_alias_count(&self) -> usize {
        self.registry
            .values()
            .filter(|entries| entries.len() > 1)
            .count()
    }

    pub fn manifest(&self) -> Option<&ConceptManifest> {
        self.manifest.as_ref()
    }

    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    pub fn load_from_map(map: BTreeMap<String, (String, String)>) -> Self {
        let mut resolver = Self::new();
        for (phrase, (concept_id, atom_id)) in map {
            resolver.register(&phrase, ConceptId(concept_id), AtomId::new(atom_id));
        }
        resolver
    }
}

pub fn normalize_alias(input: &str) -> String {
    // Unicode policy: preserve Unicode scalar values, apply full Unicode
    // lowercase mapping, retain letters/numbers, and treat every punctuation
    // character as a phrase separator. Registry assets are stored as UTF-8.
    let mut normalized = String::with_capacity(input.len());
    for character in input.trim().chars() {
        if character.is_alphanumeric() || character.is_whitespace() {
            normalized.extend(character.to_lowercase());
        } else {
            normalized.push(' ');
        }
    }
    normalized.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn resolve_input_status(
    input: &str,
) -> (
    qxfx0_types::InputSemanticStatus,
    Vec<qxfx0_types::ObservedToken>,
) {
    let morphology = qxfx0_morphology::get_runtime();
    let observed = input
        .split_whitespace()
        .enumerate()
        .map(|(position, surface)| {
            let normalized = normalize_alias(surface);
            let morphology = match morphology.lemmatize(&normalized) {
                qxfx0_types::morphology::MorphologyLookup::Resolved(result) => {
                    qxfx0_types::MorphologyLookupSummary::Resolved {
                        lemma: result.lemma,
                        source_tier: result.source_tier,
                    }
                }
                qxfx0_types::morphology::MorphologyLookup::Ambiguous(_) => {
                    qxfx0_types::MorphologyLookupSummary::Ambiguous
                }
                qxfx0_types::morphology::MorphologyLookup::Unknown => {
                    qxfx0_types::MorphologyLookupSummary::Unknown
                }
            };
            qxfx0_types::ObservedToken {
                surface: surface.to_string(),
                normalized,
                position,
                morphology,
            }
        })
        .collect();
    let status = match get_resolver().resolve(input) {
        ResolutionOutcome::Resolved(entry) => {
            qxfx0_types::InputSemanticStatus::Resolved(entry.concept_id)
        }
        ResolutionOutcome::Ambiguous(_) => qxfx0_types::InputSemanticStatus::Ambiguous,
        ResolutionOutcome::Unknown => qxfx0_types::InputSemanticStatus::Unknown,
    };
    (status, observed)
}

fn validate_manifest(
    manifest: &ConceptManifest,
    concepts_bytes: &[u8],
) -> Result<(), ConceptRegistryError> {
    if manifest.schema_version != 1 {
        return Err(ConceptRegistryError::Validation(format!(
            "unsupported manifest version: {}",
            manifest.schema_version
        )));
    }
    if manifest.source_repository.trim().is_empty()
        || manifest.license != "MIT"
        || !(manifest.source_commit.len() == 40 || manifest.source_commit.len() == 64)
        || !manifest
            .source_commit
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(ConceptRegistryError::Validation(
            "invalid provenance manifest".into(),
        ));
    }
    let expected = manifest
        .files
        .get("concepts-v1.json")
        .ok_or_else(|| ConceptRegistryError::Validation("missing concepts hash".into()))?;
    let actual = format!("{:x}", Sha256::digest(concepts_bytes));
    if expected != &actual {
        return Err(ConceptRegistryError::Validation(format!(
            "concept hash mismatch: expected {}, got {}",
            expected, actual
        )));
    }
    Ok(())
}

pub fn get_resolver() -> &'static ConceptResolver {
    crate::knowledge_pack::active_pack_set().resolver()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_and_longest_alias_resolution() {
        let mut resolver = ConceptResolver::new();
        resolver.register("свобода", ConceptId("c1".into()), AtomId::new("a1"));
        resolver.register("свобода слова", ConceptId("c2".into()), AtomId::new("a2"));
        assert!(matches!(
            resolver.resolve("СВОБОДА"),
            ResolutionOutcome::Resolved(_)
        ));
        assert_eq!(resolver.resolve("свобода слова").unwrap_concept(), "c2");
    }

    #[test]
    fn duplicate_alias_is_ambiguous() {
        let mut resolver = ConceptResolver::new();
        resolver.register("термин", ConceptId("c1".into()), AtomId::new("a1"));
        resolver.register("термин", ConceptId("c2".into()), AtomId::new("a2"));
        assert!(matches!(
            resolver.resolve("термин"),
            ResolutionOutcome::Ambiguous(_)
        ));
    }

    #[test]
    fn normalization_removes_punctuation() {
        assert_eq!(normalize_alias("  Свобода!  "), "свобода");
        assert_eq!(normalize_alias("ЁЖ—ИСТИНА"), "ёж истина");
    }

    #[test]
    fn unknown_phrase_is_unknown() {
        assert_eq!(
            get_resolver().resolve("квантобус"),
            ResolutionOutcome::Unknown
        );
    }

    #[test]
    fn embedded_registry_covers_every_seed_topic() {
        let resolver = get_resolver();
        assert_eq!(resolver.concept_count(), 137);
        for topic in crate::COVERED_TOPICS {
            match resolver.resolve(topic) {
                ResolutionOutcome::Resolved(entry) => assert_eq!(entry.atom_id.as_str(), *topic),
                outcome => panic!("topic {topic} did not resolve uniquely: {outcome:?}"),
            }
        }
        assert_eq!(
            resolver
                .records()
                .filter(|record| record.ontology_kind == "semantic_object")
                .count(),
            30
        );
    }

    #[test]
    fn embedded_registry_resolves_inflected_surface() {
        assert!(matches!(
            get_resolver().resolve("свободы"),
            ResolutionOutcome::Resolved(_)
        ));
    }

    trait OutcomeExt {
        fn unwrap_concept(self) -> String;
    }

    impl OutcomeExt for ResolutionOutcome {
        fn unwrap_concept(self) -> String {
            match self {
                ResolutionOutcome::Resolved(entry) => entry.concept_id.0,
                other => panic!("unexpected outcome: {other:?}"),
            }
        }
    }
}

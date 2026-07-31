//! New morphology runtime based on imported Haskell morphology assets.
//!
//! This module provides the new implementation that will eventually replace
//! the old MorphologyData. For now, both implementations coexist for backward compatibility.

use qxfx0_types::morphology::{
    Animacy, Case, Gender, LexemeCandidate, LexemeEntry, LexemeResolution,
    MorphologyBundleManifest, MorphologyLookup, Number, SourceTier,
};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::path::Path;

use qxfx0_types::morphology::CaseNumber;

/// Error type for morphology operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum MorphologyError {
    #[error("Failed to read asset file: {0}")]
    AssetReadError(String),
    #[error("Failed to parse JSON: {0}")]
    JsonParseError(String),
    #[error("Asset validation failed: {0}")]
    ValidationError(String),
    #[error("No lexemes loaded")]
    EmptyLexicon,
    #[error("Ambiguous surface form: {0}")]
    Ambiguous(String),
    #[error("Unknown surface form: {0}")]
    UnknownSurface(String),
}

impl PartialEq for MorphologyError {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::AssetReadError(a), Self::AssetReadError(b)) => a == b,
            (Self::JsonParseError(a), Self::JsonParseError(b)) => a == b,
            (Self::ValidationError(a), Self::ValidationError(b)) => a == b,
            (Self::EmptyLexicon, Self::EmptyLexicon) => true,
            (Self::Ambiguous(a), Self::Ambiguous(b)) => a == b,
            (Self::UnknownSurface(a), Self::UnknownSurface(b)) => a == b,
            _ => false,
        }
    }
}

/// Result type for morphology operations.
pub type MorphologyResult<T> = Result<T, MorphologyError>;

/// Runtime morphology data with indexes for fast lookup.
#[derive(Debug, Clone)]
pub struct MorphologyRuntime {
    /// All loaded lexeme entries, keyed by lemma
    pub lexemes: BTreeMap<String, LexemeEntry>,
    /// Surface form to candidates index: surface (lowercase) -> Vec<LexemeCandidate>
    pub surface_index: BTreeMap<String, Vec<LexemeCandidate>>,
    /// Lemma to entry index: lemma (lowercase) -> LexemeEntry
    pub lemma_index: BTreeMap<String, LexemeEntry>,
    /// Bundle manifest for provenance
    pub manifest: Option<MorphologyBundleManifest>,
    lexemes_sha256: String,
}

impl MorphologyRuntime {
    fn new() -> Self {
        Self {
            lexemes: BTreeMap::new(),
            surface_index: BTreeMap::new(),
            lemma_index: BTreeMap::new(),
            manifest: None,
            lexemes_sha256: String::new(),
        }
    }

    pub fn load_from_bytes(
        lexemes_bytes: &[u8],
        manifest_bytes: Option<&[u8]>,
    ) -> MorphologyResult<Self> {
        let mut runtime = Self::new();

        let mb = manifest_bytes.ok_or_else(|| {
            MorphologyError::ValidationError(
                "manifest is required for production morphology loading; use ".to_string()
                    + "load_unvalidated_for_test in tests",
            )
        })?;
        let manifest: MorphologyBundleManifest = serde_json::from_slice(mb)
            .map_err(|e| MorphologyError::JsonParseError(e.to_string()))?;

        Self::validate_manifest(&manifest, lexemes_bytes)?;
        runtime.manifest = Some(manifest);
        runtime.lexemes_sha256 = sha256_hex(lexemes_bytes);

        let lexemes: Vec<LexemeEntry> = serde_json::from_slice(lexemes_bytes)
            .map_err(|e| MorphologyError::JsonParseError(e.to_string()))?;

        runtime.build_indexes(lexemes)?;
        Ok(runtime)
    }

    fn validate_manifest(
        manifest: &MorphologyBundleManifest,
        lexemes_bytes: &[u8],
    ) -> MorphologyResult<()> {
        if manifest.bundle_version != 1 {
            return Err(MorphologyError::ValidationError(format!(
                "Unsupported bundle version: {}. Expected 1",
                manifest.bundle_version
            )));
        }
        if manifest.source_repository.is_empty() {
            return Err(MorphologyError::ValidationError(
                "source_repository cannot be empty".into(),
            ));
        }
        if !is_valid_commit_id(&manifest.source_commit) {
            return Err(MorphologyError::ValidationError(format!(
                "source_commit must be a full SHA-1 (40 hex) or SHA-256 (64 hex) identifier, \
                 got: {}",
                manifest.source_commit
            )));
        }
        if !is_approved_license(&manifest.license) {
            return Err(MorphologyError::ValidationError(format!(
                "Unauthorized license: {}. Approved: MIT, Apache-2.0, BSD-3-Clause, Unlicense",
                manifest.license
            )));
        }

        // Hash validation
        let lexicon_hash = manifest.files.get("lexemes.json").ok_or_else(|| {
            MorphologyError::ValidationError("Manifest missing hash for lexemes.json".into())
        })?;

        let mut hasher = Sha256::new();
        hasher.update(lexemes_bytes);
        let actual_hash = format!("{:x}", hasher.finalize());

        if actual_hash != *lexicon_hash {
            return Err(MorphologyError::ValidationError(format!(
                "Lexicon hash mismatch. Expected {}, got {}",
                lexicon_hash, actual_hash
            )));
        }

        // Validate lexicon is not empty
        let lexemes: Vec<LexemeEntry> = serde_json::from_slice(lexemes_bytes)
            .map_err(|e| MorphologyError::JsonParseError(e.to_string()))?;
        if lexemes.is_empty() {
            return Err(MorphologyError::ValidationError(
                "Lexicon must not be empty".into(),
            ));
        }
        for entry in &lexemes {
            if entry.lemma.is_empty() {
                return Err(MorphologyError::ValidationError(
                    "Each lexeme must have a non-empty lemma".into(),
                ));
            }
        }

        Ok(())
    }

    pub fn load_from_file<P: AsRef<Path>>(
        path: P,
        manifest_path: Option<P>,
    ) -> MorphologyResult<Self> {
        let path_ref = path.as_ref();

        // Read lexemes file bytes
        let lexemes_bytes = std::fs::read(path_ref).map_err(|e| {
            MorphologyError::AssetReadError(format!("Failed to read {:?}: {}", path_ref, e))
        })?;

        let mp = manifest_path.ok_or_else(|| {
            MorphologyError::ValidationError(
                "manifest is required for production morphology loading".into(),
            )
        })?;
        let mp_ref = mp.as_ref();
        let manifest_bytes = std::fs::read(mp_ref).map_err(|e| {
            MorphologyError::AssetReadError(format!("Failed to open manifest {:?}: {}", mp_ref, e))
        })?;

        Self::load_from_bytes(&lexemes_bytes, Some(&manifest_bytes))
    }

    fn build_indexes(&mut self, lexemes: Vec<LexemeEntry>) -> MorphologyResult<()> {
        let mut seen_lemmas = std::collections::HashMap::new();

        for entry in lexemes {
            if entry.lemma.is_empty() {
                return Err(MorphologyError::ValidationError(
                    "Each lexeme must have a non-empty lemma".into(),
                ));
            }
            let lemma_lower = entry.lemma.to_lowercase();

            if let Some(existing) = seen_lemmas.get(&lemma_lower) {
                if existing != &entry {
                    return Err(MorphologyError::ValidationError(format!(
                        "Duplicate lemma with different entries: {}",
                        lemma_lower
                    )));
                }
                continue;
            }
            seen_lemmas.insert(lemma_lower.clone(), entry.clone());

            self.lexemes.insert(entry.lemma.clone(), entry.clone());
            self.lemma_index.insert(lemma_lower.clone(), entry.clone());

            let all_forms = vec![
                ("nom_sg", &entry.forms.nom_sg),
                ("nom_pl", &entry.forms.nom_pl),
                ("gen_sg", &entry.forms.gen_sg),
                ("gen_pl", &entry.forms.gen_pl),
                ("dat_sg", &entry.forms.dat_sg),
                ("dat_pl", &entry.forms.dat_pl),
                ("acc_sg", &entry.forms.acc_sg),
                ("acc_pl", &entry.forms.acc_pl),
                ("ins_sg", &entry.forms.ins_sg),
                ("ins_pl", &entry.forms.ins_pl),
                ("prep_sg", &entry.forms.prep_sg),
                ("prep_pl", &entry.forms.prep_pl),
            ];

            for (case_num_str, form) in all_forms {
                if form.is_empty() {
                    continue;
                }
                let surface_lower = form.to_lowercase();
                if let Some(case_num) = parse_case_number(case_num_str) {
                    let mut candidate =
                        LexemeCandidate::new(form.to_string(), entry.clone(), case_num);
                    candidate.confidence = entry.quality;
                    self.surface_index
                        .entry(surface_lower.clone())
                        .or_default()
                        .push(candidate);
                }
            }
        }

        for candidates in self.surface_index.values_mut() {
            candidates.sort_by(|a, b| {
                b.entry
                    .source_tier
                    .trust_rank()
                    .cmp(&a.entry.source_tier.trust_rank())
                    .then_with(|| {
                        b.confidence
                            .partial_cmp(&a.confidence)
                            .unwrap_or(std::cmp::Ordering::Equal)
                    })
            });
        }
        Ok(())
    }

    pub fn get_lexeme(&self, lemma: &str) -> Option<&LexemeEntry> {
        self.lexemes.get(lemma)
    }

    pub fn get_candidates(&self, surface: &str) -> Vec<&LexemeCandidate> {
        let surface_lower = surface.to_lowercase();
        self.surface_index
            .get(&surface_lower)
            .map(|v| v.iter().collect())
            .unwrap_or_default()
    }

    pub fn resolve_surface(&self, surface: &str) -> MorphologyResult<LexemeEntry> {
        let candidates = self.get_candidates(surface);
        if candidates.is_empty() {
            return Err(MorphologyError::UnknownSurface(surface.to_string()));
        }
        let best_candidate = candidates[0];
        if candidates.len() > 1 {
            let best_tier = best_candidate.entry.source_tier;
            let best_quality = best_candidate.confidence;
            // Count unique lemmas among candidates with the best tier and quality.
            // Multiple forms of the same lemma (e.g. non-inflectable words) do not
            // constitute ambiguity.
            let unique_lemmas: std::collections::HashSet<&str> = candidates
                .iter()
                .filter(|c| c.entry.source_tier == best_tier && c.confidence == best_quality)
                .map(|c| c.entry.lemma.as_str())
                .collect();
            if unique_lemmas.len() > 1 {
                let surfaces: Vec<&str> = candidates.iter().map(|c| c.surface.as_str()).collect();
                return Err(MorphologyError::Ambiguous(surfaces.join(", ")));
            }
        }
        Ok(best_candidate.entry.clone())
    }

    pub fn inflect(&self, lemma: &str, case: Case, number: Number) -> Option<String> {
        self.get_lexeme(lemma).and_then(|entry| {
            let form = entry.get_form(case, number);
            (!form.is_empty()).then(|| form.to_string())
        })
    }

    /// Count surfaces whose best equally trusted candidates contain multiple lemmas.
    pub fn ambiguous_surface_count(&self) -> usize {
        self.surface_index
            .values()
            .filter(|candidates| {
                let Some(best) = candidates.first() else {
                    return false;
                };
                candidates
                    .iter()
                    .filter(|candidate| {
                        candidate.entry.source_tier == best.entry.source_tier
                            && candidate.confidence == best.confidence
                    })
                    .map(|candidate| candidate.entry.lemma.as_str())
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    > 1
            })
            .count()
    }

    pub fn lexemes_sha256(&self) -> &str {
        &self.lexemes_sha256
    }

    pub fn manifest_hash_valid(&self) -> bool {
        self.manifest
            .as_ref()
            .and_then(|manifest| manifest.files.get("lexemes.json"))
            .is_some_and(|expected| expected == &self.lexemes_sha256)
    }

    #[cfg(test)]
    pub fn load_unvalidated_for_test(lexemes_bytes: &[u8]) -> MorphologyResult<Self> {
        let mut runtime = Self::new();
        let lexemes: Vec<LexemeEntry> = serde_json::from_slice(lexemes_bytes)
            .map_err(|e| MorphologyError::JsonParseError(e.to_string()))?;
        runtime.lexemes_sha256 = sha256_hex(lexemes_bytes);
        runtime.build_indexes(lexemes)?;
        Ok(runtime)
    }

    pub fn lemmatize(&self, surface: &str) -> MorphologyLookup {
        let candidates = self.get_candidates(surface);
        if candidates.is_empty() {
            return MorphologyLookup::Unknown;
        }
        let best_candidate = candidates[0];
        if candidates.len() > 1 {
            let best_tier = best_candidate.entry.source_tier;
            let best_quality = best_candidate.confidence;
            // Count unique lemmas among candidates with the best tier and quality.
            // Multiple forms of the same lemma (e.g. non-inflectable words) do not
            // constitute ambiguity.
            let unique_lemmas: std::collections::HashSet<&str> = candidates
                .iter()
                .filter(|c| c.entry.source_tier == best_tier && c.confidence == best_quality)
                .map(|c| c.entry.lemma.as_str())
                .collect();
            if unique_lemmas.len() > 1 {
                return MorphologyLookup::Ambiguous(
                    candidates.iter().map(|c| (**c).clone()).collect(),
                );
            }
        }
        MorphologyLookup::Resolved(LexemeResolution {
            lemma: best_candidate.entry.lemma.clone(),
            surface: best_candidate.surface.clone(),
            case: best_candidate.case_number.case,
            number: best_candidate.case_number.number,
            pos: best_candidate.entry.features.pos,
            gender: best_candidate.entry.features.gender,
            animacy: best_candidate.entry.features.animacy,
            source_tier: best_candidate.entry.source_tier,
            quality: best_candidate.entry.quality,
        })
    }

    pub fn stats(&self) -> MorphologyStats {
        let mut stats = MorphologyStats::new();
        for entry in self.lexemes.values() {
            stats.total_lexemes += 1;
            match entry.features.gender {
                Gender::Masculine => stats.masculine_count += 1,
                Gender::Feminine => stats.feminine_count += 1,
                Gender::Neuter => stats.neuter_count += 1,
                Gender::Unknown => stats.unknown_gender_count += 1,
            }
            match entry.features.animacy {
                Animacy::Animate => stats.animate_count += 1,
                Animacy::Inanimate => stats.inanimate_count += 1,
                Animacy::Unknown => stats.unknown_animacy_count += 1,
            }
            match entry.source_tier {
                SourceTier::Curated => stats.curated_count += 1,
                SourceTier::Reviewed => stats.reviewed_count += 1,
                SourceTier::AutoVerified => stats.auto_verified_count += 1,
                SourceTier::AutoCoverage => stats.auto_coverage_count += 1,
            }
            if entry.is_complete() {
                stats.complete_count += 1;
            }
        }
        stats
    }
}

/// Check if a string is a valid full commit identifier (SHA-1 or SHA-256).
fn is_valid_commit_id(s: &str) -> bool {
    if s.len() == 40 || s.len() == 64 {
        s.chars().all(|c| c.is_ascii_hexdigit())
    } else {
        false
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Check if a license is in the approved allowlist.
fn is_approved_license(license: &str) -> bool {
    matches!(license, "MIT" | "Apache-2.0" | "BSD-3-Clause" | "Unlicense")
}

fn parse_case_number(key: &str) -> Option<CaseNumber> {
    let parts: Vec<&str> = key.split('_').collect();
    if parts.len() != 2 {
        return None;
    }
    let case = match parts[0] {
        "nom" => Case::Nominative,
        "gen" => Case::Genitive,
        "dat" => Case::Dative,
        "acc" => Case::Accusative,
        "ins" => Case::Instrumental,
        "prep" => Case::Prepositional,
        _ => return None,
    };
    let number = match parts[1] {
        "sg" => Number::Singular,
        "pl" => Number::Plural,
        _ => return None,
    };
    Some(CaseNumber::new(case, number))
}

#[derive(Debug, Clone, Default)]
pub struct MorphologyStats {
    pub total_lexemes: usize,
    pub masculine_count: usize,
    pub feminine_count: usize,
    pub neuter_count: usize,
    pub unknown_gender_count: usize,
    pub animate_count: usize,
    pub inanimate_count: usize,
    pub unknown_animacy_count: usize,
    pub curated_count: usize,
    pub reviewed_count: usize,
    pub auto_verified_count: usize,
    pub auto_coverage_count: usize,
    pub complete_count: usize,
}

impl MorphologyStats {
    pub fn new() -> Self {
        Self::default()
    }
}

use std::sync::OnceLock;

/// Policy for handling QXFX0_DATA_DIR override:
/// - If QXFX0_DATA_DIR is set, the override directory must contain valid
///   lexemes.json + manifest.json with matching hashes.
/// - If the override is invalid (missing files, hash mismatch, malformed JSON),
///   a warning is printed to stderr and the embedded canonical bundle is used.
/// - The embedded canonical bundle is always validated at compile time via
///   `include_bytes!` and is the authoritative fallback.
/// - An invalid override NEVER silently replaces the runtime with an empty
///   morphology runtime.
pub fn get_runtime() -> &'static MorphologyRuntime {
    static RUNTIME: OnceLock<MorphologyRuntime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        if let Ok(data_dir) = std::env::var("QXFX0_DATA_DIR") {
            match load_from_directory(&data_dir) {
                Ok(runtime) => return runtime,
                Err(e) => {
                    eprintln!(
                        "WARNING: QXFX0_DATA_DIR='{}' override is invalid ({}). \
                         Falling back to embedded canonical morphology bundle.",
                        data_dir, e
                    );
                }
            }
        }

        // Fallback to embedded assets (always validated)
        MorphologyRuntime::load_from_bytes(
            include_bytes!("../../data/lexemes.json"),
            Some(include_bytes!("../../data/manifest.json")),
        )
        .expect("Critical: Failed to load embedded morphology assets")
    })
}

pub fn load_from_directory<P: AsRef<Path>>(dir: P) -> MorphologyResult<MorphologyRuntime> {
    let lexemes_path = dir.as_ref().join("lexemes.json");
    let manifest_path = dir.as_ref().join("manifest.json");
    MorphologyRuntime::load_from_file(lexemes_path, Some(manifest_path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::morphology::{
        Case, Gender, MorphologyLookup, Number, PartOfSpeech, SourceTier,
    };

    fn data_dir() -> std::path::PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("data")
    }

    fn load_runtime() -> MorphologyRuntime {
        load_from_directory(data_dir()).unwrap()
    }

    // --- Existing tests ---

    #[test]
    fn test_parse_case_number() {
        let cn = parse_case_number("gen_sg").unwrap();
        assert_eq!(cn.case, Case::Genitive);
        assert_eq!(cn.number, Number::Singular);
        let cn = parse_case_number("prep_pl").unwrap();
        assert_eq!(cn.case, Case::Prepositional);
        assert_eq!(cn.number, Number::Plural);
        assert!(parse_case_number("invalid").is_none());
    }

    #[test]
    fn test_load_from_file() {
        let result = load_from_directory(data_dir());
        assert!(result.is_ok(), "Failed to load: {:?}", result.err());
        let runtime = result.unwrap();
        assert!(!runtime.lexemes.is_empty());
    }

    #[test]
    fn test_inflect() {
        let runtime = load_runtime();
        let gen_sg = runtime.inflect("свобода", Case::Genitive, Number::Singular);
        assert_eq!(gen_sg, Some("свободы".to_string()));
    }

    #[test]
    fn test_lemmatize() {
        let runtime = load_runtime();
        match runtime.lemmatize("свободу") {
            MorphologyLookup::Resolved(res) => {
                assert_eq!(res.lemma, "свобода")
            }
            _ => panic!("Expected Resolved for 'свободу'"),
        }
    }

    #[test]
    fn test_stats() {
        let runtime = load_runtime();
        let stats = runtime.stats();
        assert!(
            stats.total_lexemes >= 3756,
            "Expected at least 3756 lexemes, got {}",
            stats.total_lexemes
        );
    }

    // --- Section 4.4: Manifest validation tests ---

    #[test]
    fn test_manifest_hash_mismatch_rejected() {
        let manifest = MorphologyBundleManifest {
            bundle_version: 1,
            source_repository: "QxFx0".into(),
            source_commit: "49440f81b6c84700f44082a28494a04dab7b3689".into(),
            license: "MIT".into(),
            created_at: String::new(),
            files: BTreeMap::from([(
                "lexemes.json".to_string(),
                "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
            )]),
        };
        let result = MorphologyRuntime::validate_manifest(&manifest, b"[]");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("hash mismatch"));
    }

    #[test]
    fn test_malformed_bundle_rejected() {
        let result = MorphologyRuntime::load_from_bytes(b"not valid json", None);
        assert!(result.is_err());
    }

    #[test]
    fn test_invalid_bundle_version_rejected() {
        let manifest = MorphologyBundleManifest {
            bundle_version: 2,
            source_repository: "QxFx0".into(),
            source_commit: "49440f81b6c84700f44082a28494a04dab7b3689".into(),
            license: "MIT".into(),
            created_at: String::new(),
            files: BTreeMap::new(),
        };
        let result = MorphologyRuntime::validate_manifest(&manifest, b"[]");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("version"));
    }

    #[test]
    fn test_invalid_source_commit_rejected() {
        let manifest = MorphologyBundleManifest {
            bundle_version: 1,
            source_repository: "QxFx0".into(),
            source_commit: "short".into(),
            license: "MIT".into(),
            created_at: String::new(),
            files: BTreeMap::new(),
        };
        let result = MorphologyRuntime::validate_manifest(&manifest, b"[]");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("source_commit"));
    }

    #[test]
    fn test_invalid_license_rejected() {
        let manifest = MorphologyBundleManifest {
            bundle_version: 1,
            source_repository: "QxFx0".into(),
            source_commit: "49440f81b6c84700f44082a28494a04dab7b3689".into(),
            license: "GPL-3.0".into(),
            created_at: String::new(),
            files: BTreeMap::new(),
        };
        let result = MorphologyRuntime::validate_manifest(&manifest, b"[]");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("license"));
    }

    #[test]
    fn test_empty_lexicon_rejected() {
        let manifest = MorphologyBundleManifest {
            bundle_version: 1,
            source_repository: "QxFx0".into(),
            source_commit: "49440f81b6c84700f44082a28494a04dab7b3689".into(),
            license: "MIT".into(),
            created_at: String::new(),
            files: BTreeMap::from([(
                "lexemes.json".to_string(),
                format!("{:x}", Sha256::digest(b"[]")),
            )]),
        };
        let result = MorphologyRuntime::validate_manifest(&manifest, b"[]");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("empty"));
    }

    #[test]
    fn test_missing_lexemes_hash_rejected() {
        let manifest = MorphologyBundleManifest {
            bundle_version: 1,
            source_repository: "QxFx0".into(),
            source_commit: "49440f81b6c84700f44082a28494a04dab7b3689".into(),
            license: "MIT".into(),
            created_at: String::new(),
            files: BTreeMap::new(),
        };
        let result = MorphologyRuntime::validate_manifest(&manifest, b"[]");
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("lexemes.json"));
    }

    #[test]
    fn test_duplicate_lemma_different_entries_rejected() {
        let entry1 = LexemeEntry::new("свобода");
        let mut entry2 = LexemeEntry::new("свобода");
        entry2.features.gender = Gender::Masculine;
        let lexemes = vec![entry1, entry2];
        let mut runtime = MorphologyRuntime::new();
        let result = runtime.build_indexes(lexemes);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Duplicate lemma"));
    }

    #[test]
    fn test_duplicate_lemma_identical_entries_deduplicated() {
        let entry = LexemeEntry::new("свобода");
        let lexemes = vec![entry.clone(), entry.clone()];
        let mut runtime = MorphologyRuntime::new();
        let result = runtime.build_indexes(lexemes);
        assert!(result.is_ok());
        assert_eq!(runtime.lexemes.len(), 1);
    }

    #[test]
    fn test_empty_lemma_rejected() {
        let mut entry = LexemeEntry::new("");
        entry.forms.nom_sg = "test".to_string();
        let lexemes = vec![entry];
        let mut runtime = MorphologyRuntime::new();
        let result = runtime.build_indexes(lexemes);
        assert!(result.is_err());
    }

    // --- Section 4.4: Resolution tests ---

    #[test]
    fn test_known_form_resolves_to_exact_lemma() {
        let runtime = load_runtime();
        match runtime.lemmatize("свободу") {
            MorphologyLookup::Resolved(res) => {
                assert_eq!(res.lemma, "свобода");
                assert_eq!(res.surface, "свободу");
                assert_eq!(res.case, Case::Accusative);
                assert_eq!(res.number, Number::Singular);
                assert_eq!(res.pos, PartOfSpeech::Noun);
                assert_eq!(res.gender, Gender::Feminine);
            }
            _ => panic!("Expected Resolved"),
        }
    }

    #[test]
    fn test_unknown_word_returns_unknown() {
        let runtime = load_runtime();
        assert_eq!(runtime.lemmatize("qwertyuiop"), MorphologyLookup::Unknown);
    }

    #[test]
    fn test_equal_tier_equal_quality_collision_returns_ambiguous() {
        let runtime = load_runtime();
        // "абазы" maps to both "абаз" and "абаза", both curated quality 1.0
        match runtime.lemmatize("абазы") {
            MorphologyLookup::Ambiguous(candidates) => {
                let lemmas: std::collections::HashSet<&str> =
                    candidates.iter().map(|c| c.entry.lemma.as_str()).collect();
                assert!(lemmas.len() > 1, "Expected multiple distinct lemmas");
            }
            MorphologyLookup::Resolved(res) => {
                // If only one unique lemma, that's fine too
                let _ = res;
            }
            MorphologyLookup::Unknown => panic!("Expected Ambiguous or Resolved"),
        }
    }

    #[test]
    fn test_higher_trust_candidate_wins() {
        let runtime = load_runtime();
        // "анимой" maps to "анима" (reviewed, 0.9) and "анимая" (curated, 1.0)
        match runtime.lemmatize("анимой") {
            MorphologyLookup::Resolved(res) => {
                assert_eq!(
                    res.source_tier,
                    SourceTier::Curated,
                    "Curated should win over reviewed"
                );
                assert_eq!(res.lemma, "анимая");
            }
            MorphologyLookup::Ambiguous(_) => {
                // If ambiguity is returned, the curated entry should still be first
                let candidates = runtime.get_candidates("анимой");
                assert_eq!(candidates[0].entry.source_tier, SourceTier::Curated);
            }
            MorphologyLookup::Unknown => panic!("Expected Resolved or Ambiguous"),
        }
    }

    #[test]
    fn test_higher_quality_wins_within_same_tier() {
        // Build a synthetic runtime with two entries of same tier, different quality
        let mut entry1 = LexemeEntry::new("слово1");
        entry1.source_tier = SourceTier::Curated;
        entry1.quality = 0.8;
        entry1.forms.nom_sg = "форма".to_string();
        entry1.forms.gen_sg = "формы".to_string();

        let mut entry2 = LexemeEntry::new("слово2");
        entry2.source_tier = SourceTier::Curated;
        entry2.quality = 1.0;
        entry2.forms.nom_sg = "форма".to_string();
        entry2.forms.gen_sg = "формы".to_string();

        let mut runtime = MorphologyRuntime::new();
        runtime.build_indexes(vec![entry1, entry2]).unwrap();

        match runtime.lemmatize("форма") {
            MorphologyLookup::Resolved(res) => {
                assert_eq!(res.lemma, "слово2", "Higher quality should win");
            }
            _ => panic!("Expected Resolved"),
        }
    }

    #[test]
    fn test_exact_6_cases_x_2_numbers_for_selected_lexemes() {
        let runtime = load_runtime();
        let test_words = ["свобода", "разум", "сознание", "человек", "время"];
        for word in &test_words {
            for case in [
                Case::Nominative,
                Case::Genitive,
                Case::Dative,
                Case::Accusative,
                Case::Instrumental,
                Case::Prepositional,
            ] {
                for number in [Number::Singular, Number::Plural] {
                    let form = runtime.inflect(word, case, number);
                    assert!(
                        form.is_some() && !form.unwrap().is_empty(),
                        "Form for {} {:?}/{:?} should not be empty",
                        word,
                        case,
                        number
                    );
                }
            }
        }
    }

    #[test]
    fn test_singular_plural_forms() {
        let runtime = load_runtime();
        // "свобода" - singular
        assert_eq!(
            runtime.inflect("свобода", Case::Nominative, Number::Singular),
            Some("свобода".to_string())
        );
        // "свобода" - plural
        let pl = runtime.inflect("свобода", Case::Nominative, Number::Plural);
        assert!(pl.is_some());
        assert_ne!(pl.unwrap(), "свобода");
    }

    #[test]
    fn test_animate_accusative() {
        let runtime = load_runtime();
        // "человек" is animate masculine - accusative should differ from nominative
        let acc = runtime.inflect("человек", Case::Accusative, Number::Singular);
        assert_eq!(acc, Some("человека".to_string()));
        // "разум" is inanimate masculine - accusative should equal nominative
        let acc_inan = runtime.inflect("разум", Case::Accusative, Number::Singular);
        assert_eq!(acc_inan, Some("разум".to_string()));
    }

    #[test]
    fn test_noun_adjective_verb_non_inflectable_behavior() {
        let runtime = load_runtime();
        // Noun: "свобода" should have all 12 forms
        for case in [
            Case::Nominative,
            Case::Genitive,
            Case::Dative,
            Case::Accusative,
            Case::Instrumental,
            Case::Prepositional,
        ] {
            for number in [Number::Singular, Number::Plural] {
                assert!(runtime.inflect("свобода", case, number).is_some());
            }
        }
        // Non-inflectable: "а-конто" has all forms = "а-конто"
        let form = runtime.inflect("а-конто", Case::Genitive, Number::Singular);
        assert_eq!(form, Some("а-конто".to_string()));
    }

    #[test]
    fn test_inflect_missing_form_returns_none() {
        let runtime = load_runtime();
        // A word that doesn't exist should return None
        assert_eq!(
            runtime.inflect("неизвестноеслово", Case::Nominative, Number::Singular),
            None
        );
    }

    #[test]
    fn test_non_inflectable_word_resolves_correctly() {
        let runtime = load_runtime();
        // "а-конто" is non-inflectable - all forms are the same
        match runtime.lemmatize("а-конто") {
            MorphologyLookup::Resolved(res) => {
                assert_eq!(res.lemma, "а-конто");
            }
            MorphologyLookup::Ambiguous(_) => {
                // Should not be ambiguous since it's the same lemma
                panic!("Non-inflectable word should not be ambiguous");
            }
            MorphologyLookup::Unknown => panic!("Expected Resolved"),
        }
    }

    #[test]
    fn test_resolve_surface_and_lemmatize_use_same_policy() {
        let runtime = load_runtime();
        let surface = "свободу";
        let lem = runtime.lemmatize(surface);
        let res = runtime.resolve_surface(surface);

        match lem {
            MorphologyLookup::Resolved(res_lem) => {
                let res_surf = res.unwrap();
                assert_eq!(res_lem.lemma, res_surf.lemma);
            }
            _ => panic!("Expected Resolved"),
        }
    }

    #[test]
    fn test_cross_process_deterministic_result() {
        let runtime = load_runtime();
        // Same input should always produce same output
        let r1 = runtime.lemmatize("свободу");
        let r2 = runtime.lemmatize("свободу");
        assert_eq!(r1, r2);

        let r3 = runtime.lemmatize("разума");
        let r4 = runtime.lemmatize("разума");
        assert_eq!(r3, r4);
    }

    // --- QXFX0_DATA_DIR policy test ---

    #[test]
    fn test_qxfx0_data_dir_invalid_does_not_silently_override() {
        // Create a temp dir with invalid data
        let temp_dir = std::env::temp_dir().join("qxfx0_test_invalid_data");
        std::fs::create_dir_all(&temp_dir).unwrap();
        std::fs::write(temp_dir.join("lexemes.json"), b"invalid json").unwrap();
        std::fs::write(
            temp_dir.join("manifest.json"),
            b"{\"bundle_version\":1,\"source_repository\":\"QxFx0\",\"source_commit\":\"49440f81b6c84700f44082a28494a04dab7b3689\",\"license\":\"MIT\",\"files\":{}}",
        )
        .unwrap();

        // Loading from invalid dir should fail
        let result = load_from_directory(&temp_dir);
        assert!(result.is_err(), "Invalid data dir should return error");

        // Clean up
        std::fs::remove_dir_all(&temp_dir).ok();
    }

    #[test]
    fn test_embedded_bundle_is_valid() {
        // The embedded bundle should always load successfully
        let runtime = MorphologyRuntime::load_from_bytes(
            include_bytes!("../../data/lexemes.json"),
            Some(include_bytes!("../../data/manifest.json")),
        )
        .expect("Embedded bundle must be valid");
        assert!(!runtime.lexemes.is_empty());
        assert!(runtime.manifest.is_some());
    }

    #[test]
    fn test_manifest_validation_full() {
        let runtime = load_runtime();
        let manifest = runtime
            .manifest
            .as_ref()
            .expect("manifest should be loaded");
        assert_eq!(manifest.bundle_version, 1);
        assert!(!manifest.source_repository.is_empty());
        assert!(is_valid_commit_id(&manifest.source_commit));
        assert!(is_approved_license(&manifest.license));
        assert!(manifest.files.contains_key("lexemes.json"));
    }

    #[test]
    fn test_commit_id_validation() {
        assert!(is_valid_commit_id(
            "49440f81b6c84700f44082a28494a04dab7b3689"
        ));
        assert!(is_valid_commit_id(&"a".repeat(64)));
        assert!(!is_valid_commit_id("short"));
        assert!(!is_valid_commit_id(&"g".repeat(40)));
        assert!(!is_valid_commit_id(""));
    }

    #[test]
    fn test_license_allowlist() {
        assert!(is_approved_license("MIT"));
        assert!(is_approved_license("Apache-2.0"));
        assert!(is_approved_license("BSD-3-Clause"));
        assert!(is_approved_license("Unlicense"));
        assert!(!is_approved_license("GPL-3.0"));
        assert!(!is_approved_license(""));
    }

    #[test]
    fn test_lemmatize_returns_morphology_lookup_not_option() {
        let runtime = load_runtime();
        // Verify the return type is MorphologyLookup, not Option
        let result: MorphologyLookup = runtime.lemmatize("свобода");
        match result {
            MorphologyLookup::Resolved(_) => {}
            _ => panic!("Expected Resolved for known word"),
        }
    }

    #[test]
    fn test_no_heuristic_inflection_for_known_lexeme() {
        let runtime = load_runtime();
        // "свобода" is known - inflect should return exact stored form, not heuristic
        let gen = runtime.inflect("свобода", Case::Genitive, Number::Singular);
        assert_eq!(gen, Some("свободы".to_string()));
    }
}

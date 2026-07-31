//! Hash-validated reports produced by offline, non-promoting corpus imports.

use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::sync::OnceLock;

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CorpusImportManifest {
    pub import_id: String,
    pub schema_version: u32,
    pub source_repository: String,
    pub source_commit: String,
    pub license: String,
    pub files: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CorpusImportMetrics {
    pub schema_version: u32,
    pub import_id: String,
    pub status: String,
    pub promotion_enabled: bool,
    pub source_repository: String,
    pub source_commit: String,
    pub source_worktree_dirty: bool,
    pub source_sha256: String,
    pub ontology_sha256: String,
    pub source_rows: usize,
    pub source_raw_unique_topics: usize,
    pub source_normalized_unique_topics: usize,
    pub source_raw_duplicate_topic_rows: usize,
    pub source_normalized_duplicate_topic_rows: usize,
    pub source_predicates: usize,
    pub ontology_records: usize,
    pub pilot_unique_topics: usize,
    pub already_active: usize,
    pub quarantined: usize,
    pub quarantine_reason_counts: BTreeMap<String, usize>,
}

#[derive(Debug, Clone)]
pub struct CorpusImportReport {
    pub manifest: CorpusImportManifest,
    pub metrics: CorpusImportMetrics,
    pub fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CorpusImportError {
    #[error("corpus import JSON parse failed: {0}")]
    Json(String),
    #[error("corpus import validation failed: {0}")]
    Validation(String),
}

impl CorpusImportReport {
    pub fn load(
        manifest_bytes: &[u8],
        metrics_bytes: &[u8],
        inventory_bytes: &[u8],
        quarantine_bytes: &[u8],
    ) -> Result<Self, CorpusImportError> {
        let manifest: CorpusImportManifest = serde_json::from_slice(manifest_bytes)
            .map_err(|error| CorpusImportError::Json(error.to_string()))?;
        let metrics: CorpusImportMetrics = serde_json::from_slice(metrics_bytes)
            .map_err(|error| CorpusImportError::Json(error.to_string()))?;
        if manifest.schema_version != 1
            || metrics.schema_version != 1
            || manifest.import_id != metrics.import_id
            || manifest.source_repository != metrics.source_repository
            || manifest.source_commit != metrics.source_commit
            || manifest.license != "MIT"
            || !is_full_commit(&manifest.source_commit)
        {
            return Err(CorpusImportError::Validation(
                "invalid import identity, provenance, schema, or license".into(),
            ));
        }
        if metrics.status != "audit_only" || metrics.promotion_enabled {
            return Err(CorpusImportError::Validation(
                "offline pilot must be audit_only with promotion disabled".into(),
            ));
        }
        let files = [
            ("metrics.json", metrics_bytes),
            ("inventory.jsonl", inventory_bytes),
            ("quarantine.jsonl", quarantine_bytes),
        ];
        if manifest.files.len() != files.len() {
            return Err(CorpusImportError::Validation(
                "import manifest must hash exactly three report files".into(),
            ));
        }
        for (name, bytes) in files {
            let expected = manifest.files.get(name).ok_or_else(|| {
                CorpusImportError::Validation(format!("missing report hash for {name}"))
            })?;
            let actual = format!("{:x}", Sha256::digest(bytes));
            if expected != &actual {
                return Err(CorpusImportError::Validation(format!(
                    "report hash mismatch for {name}: expected {expected}, got {actual}"
                )));
            }
        }

        let inventory_count = nonempty_line_count(inventory_bytes);
        let quarantine_count = nonempty_line_count(quarantine_bytes);
        if inventory_count != metrics.pilot_unique_topics
            || quarantine_count != metrics.quarantined
            || metrics.already_active + metrics.quarantined != metrics.pilot_unique_topics
            || metrics.source_rows
                != metrics.source_raw_unique_topics + metrics.source_raw_duplicate_topic_rows
            || metrics.source_rows
                != metrics.source_normalized_unique_topics
                    + metrics.source_normalized_duplicate_topic_rows
        {
            return Err(CorpusImportError::Validation(
                "import report counts are internally inconsistent".into(),
            ));
        }

        Ok(Self {
            manifest,
            metrics,
            fingerprint: format!("{:x}", Sha256::digest(manifest_bytes)),
        })
    }
}

fn nonempty_line_count(bytes: &[u8]) -> usize {
    bytes
        .split(|byte| *byte == b'\n')
        .filter(|line| !line.iter().all(u8::is_ascii_whitespace))
        .count()
}

fn is_full_commit(value: &str) -> bool {
    matches!(value.len(), 40 | 64) && value.chars().all(|character| character.is_ascii_hexdigit())
}

pub fn corpus_import_report() -> Result<&'static CorpusImportReport, &'static str> {
    static REPORT: OnceLock<Result<CorpusImportReport, String>> = OnceLock::new();
    REPORT
        .get_or_init(|| {
            CorpusImportReport::load(
                include_bytes!("../../data/imports/haskell-curated-pilot-v1/manifest.json"),
                include_bytes!("../../data/imports/haskell-curated-pilot-v1/metrics.json"),
                include_bytes!("../../data/imports/haskell-curated-pilot-v1/inventory.jsonl"),
                include_bytes!("../../data/imports/haskell-curated-pilot-v1/quarantine.jsonl"),
            )
            .map_err(|error| error.to_string())
        })
        .as_ref()
        .map_err(String::as_str)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_report_is_valid_and_non_promoting() {
        let report = corpus_import_report().unwrap();
        assert_eq!(report.metrics.pilot_unique_topics, 300);
        assert!(!report.metrics.promotion_enabled);
        assert_eq!(
            report.metrics.already_active + report.metrics.quarantined,
            300
        );
        assert_eq!(report.fingerprint.len(), 64);
    }

    #[test]
    fn tampered_inventory_is_rejected() {
        let mut inventory =
            include_bytes!("../../data/imports/haskell-curated-pilot-v1/inventory.jsonl").to_vec();
        inventory.push(b' ');
        assert!(matches!(
            CorpusImportReport::load(
                include_bytes!(
                    "../../data/imports/haskell-curated-pilot-v1/manifest.json"
                ),
                include_bytes!("../../data/imports/haskell-curated-pilot-v1/metrics.json"),
                &inventory,
                include_bytes!(
                    "../../data/imports/haskell-curated-pilot-v1/quarantine.jsonl"
                ),
            ),
            Err(CorpusImportError::Validation(message)) if message.contains("hash mismatch")
        ));
    }
}

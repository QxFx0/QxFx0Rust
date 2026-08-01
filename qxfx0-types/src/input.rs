use crate::morphology::SourceTier;
use crate::ConceptId;
use serde::{Deserialize, Serialize};

/// Redacted per-turn morphology evidence. It is never persisted in `SystemState`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MorphologyLookupSummary {
    Resolved {
        lemma: String,
        source_tier: SourceTier,
    },
    Ambiguous,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObservedToken {
    pub surface: String,
    pub normalized: String,
    pub position: usize,
    pub morphology: MorphologyLookupSummary,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InputSemanticStatus {
    Resolved(ConceptId),
    Ambiguous,
    Unknown,
}

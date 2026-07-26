//! Immutable DTOs and mutation inputs for the perspective subsystem.
//!
//! These types deliberately contain no renderer concerns. A renderer receives
//! [`PerspectiveProjection`] only; registry state stays in the self layer.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum PerspectiveScope {
    Topic(String),
    Theme(String),
    Cluster(String),
}

impl PerspectiveScope {
    pub fn render(&self) -> String {
        match self {
            Self::Topic(value) => format!("topic:{value}"),
            Self::Theme(value) => format!("theme:{value}"),
            Self::Cluster(value) => format!("cluster:{value}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PerspectiveId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PerspectiveVersion(pub u64);

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NormativeProfileId(pub String);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerspectiveStatus {
    Active,
    Contested,
    Suspended,
    Revised,
    Withdrawn,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfidenceBand {
    High,
    Medium,
    Low,
    Minimal,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CautionLevel {
    Low,
    Medium,
    High,
}

/// The only perspective shape that presentation code may consume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerspectiveProjection {
    pub scope: PerspectiveScope,
    pub summary: String,
    pub orientation: String,
    pub confidence_band: ConfidenceBand,
    pub caution_level: CautionLevel,
    pub contested: bool,
    pub perspective_version: PerspectiveVersion,
    pub normative_profile_id: NormativeProfileId,
    pub normative_profile_version: u64,
    pub evidence_count: usize,
    pub counterargument_count: usize,
    pub explanation_handle: String,
}

/// Explicit, replayable mutation decision. It is intentionally separate from
/// a projection so presentation cannot mutate perspective state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PerspectiveDecision {
    ObserveOnly,
    Quarantine,
    AcceptBounded,
    PromoteEndorsed,
    ReviseActive,
    SuspendActive,
    RollbackPrior,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerspectiveMutation {
    pub turn: u64,
    pub scope: PerspectiveScope,
    pub decision: PerspectiveDecision,
    pub thesis: String,
    pub orientation: String,
    pub confidence: f64,
    pub normative_profile_id: NormativeProfileId,
    pub normative_profile_version: u64,
    pub evidence: Vec<String>,
    pub counterarguments: Vec<String>,
}

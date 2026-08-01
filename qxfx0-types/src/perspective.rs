//! Immutable DTOs and mutation inputs for the perspective subsystem.
//!
//! These types deliberately contain no renderer concerns. A renderer receives
//! [`PerspectiveProjection`] only; registry state stays in the self layer.

use crate::{ConceptId, FactId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

pub const MAX_PERSPECTIVE_OPINIONS: usize = 1_024;
pub const MAX_PERSPECTIVE_EPISODES: usize = 2_048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BeliefPolarity {
    Affirmed,
    Qualified,
    Opposed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpinionCore {
    pub topic: ConceptId,
    pub primary_fact: FactId,
    pub polarity: BeliefPolarity,
    pub grounding_facts: BTreeSet<FactId>,
    pub confidence_basis_points: u16,
    pub revision_seq: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PerspectiveRevisionReason {
    EstablishedFromCuratedFact,
    QualifiedByCuratedCounterpoint,
    ReinforcedByCuratedConsequence,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PerspectiveEpisodeId(pub usize);

/// Bounded semantic memory. It contains only typed ConceptId/FactId values;
/// raw input and rendered response text have no representation here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerspectiveEpisode {
    pub id: PerspectiveEpisodeId,
    pub turn_seq: usize,
    pub topic: ConceptId,
    pub previous_polarity: Option<BeliefPolarity>,
    pub resulting_polarity: BeliefPolarity,
    pub cited_facts: Vec<FactId>,
    pub reason: PerspectiveRevisionReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct PerspectiveState {
    pub opinions: BTreeMap<ConceptId, OpinionCore>,
    pub episodes: Vec<PerspectiveEpisode>,
    pub next_episode_id: usize,
}

impl PerspectiveState {
    pub fn validate(&self) -> Vec<String> {
        let mut violations = Vec::new();
        if self.opinions.len() > MAX_PERSPECTIVE_OPINIONS {
            violations.push(format!(
                "perspective opinions exceed {MAX_PERSPECTIVE_OPINIONS} entries"
            ));
        }
        if self.episodes.len() > MAX_PERSPECTIVE_EPISODES {
            violations.push(format!(
                "perspective episodes exceed {MAX_PERSPECTIVE_EPISODES} entries"
            ));
        }
        for (topic, opinion) in &self.opinions {
            if topic != &opinion.topic {
                violations.push(format!(
                    "perspective opinion key '{}' differs from payload topic '{}'",
                    topic.0, opinion.topic.0
                ));
            }
            if opinion.grounding_facts.is_empty()
                || !opinion.grounding_facts.contains(&opinion.primary_fact)
            {
                violations.push(format!(
                    "perspective opinion '{}' lacks its primary grounding fact",
                    topic.0
                ));
            }
            if opinion.confidence_basis_points > 10_000 {
                violations.push(format!(
                    "perspective opinion '{}' confidence exceeds 10000 basis points",
                    topic.0
                ));
            }
        }
        let mut previous_id = None;
        for episode in &self.episodes {
            if episode.cited_facts.is_empty() {
                violations.push(format!(
                    "perspective episode {} has no cited FactId",
                    episode.id.0
                ));
            }
            if episode.id.0 >= self.next_episode_id {
                violations.push(format!(
                    "perspective episode {} is not below next_episode_id {}",
                    episode.id.0, self.next_episode_id
                ));
            }
            if previous_id.is_some_and(|previous| episode.id.0 <= previous) {
                violations.push("perspective episode ids are not strictly increasing".into());
            }
            previous_id = Some(episode.id.0);
        }
        violations
    }
}

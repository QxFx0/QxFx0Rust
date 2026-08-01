use crate::{ConceptId, FactId};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

/// A replay-stable semantic episode. It contains only typed knowledge-pack
/// identities and transition metadata; observed or generated surface text has
/// no field through which it could enter the perspective store.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn fact(value: &str) -> FactId {
        FactId::try_new(value).unwrap()
    }

    #[test]
    fn default_perspective_is_valid() {
        assert!(PerspectiveState::default().validate().is_empty());
    }

    #[test]
    fn opinion_requires_primary_fact_in_grounding_set() {
        let topic = ConceptId("concept.freedom".into());
        let state = PerspectiveState {
            opinions: BTreeMap::from([(
                topic.clone(),
                OpinionCore {
                    topic,
                    primary_fact: fact("fact.freedom"),
                    polarity: BeliefPolarity::Affirmed,
                    grounding_facts: BTreeSet::new(),
                    confidence_basis_points: 9_000,
                    revision_seq: 1,
                },
            )]),
            ..Default::default()
        };
        assert!(!state.validate().is_empty());
    }
}

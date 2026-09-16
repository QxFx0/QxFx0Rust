use qxfx0_semantic::{ClaimRole, FactCondition, FactRegistry, FactRegistryError};
use qxfx0_types::{
    BeliefPolarity, ConceptId, FactId, OpinionCore, PerspectiveEpisode, PerspectiveEpisodeId,
    PerspectiveRevisionReason, PerspectiveState, MAX_PERSPECTIVE_EPISODES,
    MAX_PERSPECTIVE_OPINIONS,
};
use std::collections::BTreeSet;

/// Typed Perspective failure (Phase D2): every `Err` these operators
/// return is one of these variants. `Display` reproduces each
/// historical message byte-for-byte, so pipeline diagnostics and
/// persistence rejections never drift.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PerspectiveError {
    DialogueActRejected,
    FactSelection(FactRegistryError),
    MultiSubject,
    OpinionCapacity {
        max: usize,
    },
    ThesisTopicMismatch {
        fact: FactId,
        topic: ConceptId,
    },
    OpinionKeyMismatch {
        key: ConceptId,
        payload: ConceptId,
    },
    PrimaryMismatch {
        topic: ConceptId,
        primary: FactId,
        thesis: FactId,
    },
    GroundingOmitsThesis {
        topic: ConceptId,
        thesis: FactId,
    },
    GroundingTopicMismatch {
        grounding: FactId,
        topic: ConceptId,
    },
    AffirmedDespiteCounterpoint {
        topic: ConceptId,
    },
    QualifiedWithoutCounterpoint {
        topic: ConceptId,
    },
    OpposedUnsupported {
        topic: ConceptId,
    },
    PrimaryConflict {
        subject: ConceptId,
        existing: FactId,
        new: FactId,
    },
    NoThesisForCounterpoint {
        fact: FactId,
    },
    CounterpointDoesNotCite {
        fact: FactId,
        primary: FactId,
    },
    NoThesisForConsequence {
        fact: FactId,
    },
    ConsequenceDoesNotCite {
        fact: FactId,
        primary: FactId,
    },
    RevisionOverflow,
    EpisodeOverflow,
}

impl std::fmt::Display for PerspectiveError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PerspectiveError::DialogueActRejected => {
                write!(f, "dialogue acts cannot enter Perspective evidence")
            }
            PerspectiveError::FactSelection(error) => write!(f, "{error}"),
            PerspectiveError::MultiSubject => {
                write!(f, "Perspective evidence spans multiple concept subjects")
            }
            PerspectiveError::OpinionCapacity { max } => {
                write!(f, "Perspective opinion capacity {max} reached")
            }
            PerspectiveError::ThesisTopicMismatch { fact, topic } => write!(
                f,
                "Perspective thesis '{fact}' is not about topic '{}'",
                topic.0
            ),
            PerspectiveError::OpinionKeyMismatch { key, payload } => write!(
                f,
                "Perspective opinion key '{}' differs from payload topic '{}'",
                key.0, payload.0
            ),
            PerspectiveError::PrimaryMismatch {
                topic,
                primary,
                thesis,
            } => write!(
                f,
                "Perspective opinion for '{}' cites primary fact '{primary}' instead of rendered thesis '{thesis}'",
                topic.0
            ),
            PerspectiveError::GroundingOmitsThesis { topic, thesis } => write!(
                f,
                "Perspective opinion for '{}' omits rendered thesis '{thesis}' from its grounding",
                topic.0
            ),
            PerspectiveError::GroundingTopicMismatch { grounding, topic } => write!(
                f,
                "Perspective grounding fact '{grounding}' is not about topic '{}'",
                topic.0
            ),
            PerspectiveError::AffirmedDespiteCounterpoint { topic } => write!(
                f,
                "Perspective opinion for '{}' is affirmed despite a curated counterpoint",
                topic.0
            ),
            PerspectiveError::QualifiedWithoutCounterpoint { topic } => write!(
                f,
                "Perspective opinion for '{}' is qualified without a curated counterpoint",
                topic.0
            ),
            PerspectiveError::OpposedUnsupported { topic } => write!(
                f,
                "Perspective opinion for '{}' uses unsupported opposed polarity",
                topic.0
            ),
            PerspectiveError::PrimaryConflict {
                subject,
                existing,
                new,
            } => write!(
                f,
                "Perspective primary fact conflict for '{}': '{existing}' versus '{new}'",
                subject.0
            ),
            PerspectiveError::NoThesisForCounterpoint { fact } => write!(
                f,
                "Perspective counterpoint '{fact}' has no established thesis"
            ),
            PerspectiveError::CounterpointDoesNotCite { fact, primary } => write!(
                f,
                "Perspective counterpoint '{fact}' does not cite primary fact '{primary}'"
            ),
            PerspectiveError::NoThesisForConsequence { fact } => write!(
                f,
                "Perspective consequence '{fact}' has no established thesis"
            ),
            PerspectiveError::ConsequenceDoesNotCite { fact, primary } => write!(
                f,
                "Perspective consequence '{fact}' does not cite primary fact '{primary}'"
            ),
            PerspectiveError::RevisionOverflow => {
                write!(f, "Perspective revision sequence overflow")
            }
            PerspectiveError::EpisodeOverflow => write!(f, "Perspective episode id overflow"),
        }
    }
}

impl std::error::Error for PerspectiveError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PerspectiveUpdate {
    pub topic: Option<ConceptId>,
    pub opinion_changed: bool,
    pub episodes_added: usize,
}

/// A renderer-facing decision derived only from persisted semantic identity.
/// The enum deliberately contains no surface text: wording remains owned by
/// the renderer, while this module only authorizes which stance it may expose.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PerspectiveRenderStance {
    Neutral,
    Affirmed,
    Qualified,
}

impl PerspectiveUpdate {
    fn unchanged() -> Self {
        Self {
            topic: None,
            opinion_changed: false,
            episodes_added: 0,
        }
    }
}

/// Deterministically integrate already-selected curated claims into a
/// per-session position. This operator cannot accept user or generated text:
/// its only evidence channel is `(ClaimRole, FactId)` resolved by the active
/// immutable `FactRegistry`.
pub fn integrate_curated_claims(
    state: &PerspectiveState,
    turn_seq: usize,
    claims: &[(ClaimRole, FactId)],
    facts: &FactRegistry,
) -> Result<(PerspectiveState, PerspectiveUpdate), PerspectiveError> {
    if claims.is_empty() {
        return Ok((state.clone(), PerspectiveUpdate::unchanged()));
    }

    let mut selected = Vec::with_capacity(claims.len());
    let mut topic = None;
    for (role, fact_id) in claims {
        if *role == ClaimRole::DialogueAct {
            return Err(PerspectiveError::DialogueActRejected);
        }
        let fact = facts
            .select(fact_id)
            .map_err(PerspectiveError::FactSelection)?;
        if let Some(expected) = &topic {
            if expected != &fact.subject {
                return Err(PerspectiveError::MultiSubject);
            }
        } else {
            topic = Some(fact.subject.clone());
        }
        selected.push((*role, fact));
    }
    let topic = topic.expect("non-empty claims must select one subject");
    if !state.opinions.contains_key(&topic) && state.opinions.len() >= MAX_PERSPECTIVE_OPINIONS {
        return Err(PerspectiveError::OpinionCapacity {
            max: MAX_PERSPECTIVE_OPINIONS,
        });
    }

    let mut next = state.clone();
    let mut episodes_added = 0;
    for (role, fact) in selected {
        let changed = match role {
            ClaimRole::Thesis => establish_opinion(&mut next, turn_seq, fact)?,
            ClaimRole::Counterpoint => qualify_opinion(&mut next, turn_seq, fact)?,
            ClaimRole::Consequence | ClaimRole::Support => {
                reinforce_opinion(&mut next, turn_seq, fact)?
            }
            ClaimRole::DialogueAct => panic!(
                "fact_perspective: DialogueAct reached curated-claim integration at turn {turn_seq}; it is rejected before fact selection"
            ),
        };
        episodes_added += usize::from(changed);
    }

    Ok((
        next,
        PerspectiveUpdate {
            topic: Some(topic),
            opinion_changed: episodes_added > 0,
            episodes_added,
        },
    ))
}

/// Resolve the stance that may be exposed while rendering a curated thesis.
///
/// The current thesis and every persisted grounding id are re-selected through
/// the immutable registry. Corrupt, stale or unsupported state therefore
/// fails closed before any stance wording is emitted. In particular,
/// `Opposed` is not renderable in v1 because no Perspective transition can
/// establish it from curated evidence yet.
pub fn resolve_render_stance(
    state: &PerspectiveState,
    topic: &ConceptId,
    thesis_fact_id: &FactId,
    facts: &FactRegistry,
) -> Result<PerspectiveRenderStance, PerspectiveError> {
    let thesis = facts
        .select(thesis_fact_id)
        .map_err(PerspectiveError::FactSelection)?;
    if &thesis.subject != topic {
        return Err(PerspectiveError::ThesisTopicMismatch {
            fact: thesis_fact_id.clone(),
            topic: topic.clone(),
        });
    }

    let Some(opinion) = state.opinions.get(topic) else {
        return Ok(PerspectiveRenderStance::Neutral);
    };
    if &opinion.topic != topic {
        return Err(PerspectiveError::OpinionKeyMismatch {
            key: topic.clone(),
            payload: opinion.topic.clone(),
        });
    }
    if &opinion.primary_fact != thesis_fact_id {
        return Err(PerspectiveError::PrimaryMismatch {
            topic: topic.clone(),
            primary: opinion.primary_fact.clone(),
            thesis: thesis_fact_id.clone(),
        });
    }
    if !opinion.grounding_facts.contains(thesis_fact_id) {
        return Err(PerspectiveError::GroundingOmitsThesis {
            topic: topic.clone(),
            thesis: thesis_fact_id.clone(),
        });
    }

    let mut has_curated_counterpoint = false;
    for grounding_id in &opinion.grounding_facts {
        let grounding = facts
            .select(grounding_id)
            .map_err(PerspectiveError::FactSelection)?;
        if &grounding.subject != topic {
            return Err(PerspectiveError::GroundingTopicMismatch {
                grounding: grounding_id.clone(),
                topic: topic.clone(),
            });
        }
        has_curated_counterpoint |= grounding.conditions.iter().any(|condition| {
            matches!(condition, FactCondition::Counters(target) if target == thesis_fact_id)
        });
    }

    match opinion.polarity {
        BeliefPolarity::Affirmed if !has_curated_counterpoint => {
            Ok(PerspectiveRenderStance::Affirmed)
        }
        BeliefPolarity::Affirmed => Err(PerspectiveError::AffirmedDespiteCounterpoint {
            topic: topic.clone(),
        }),
        BeliefPolarity::Qualified if has_curated_counterpoint => {
            Ok(PerspectiveRenderStance::Qualified)
        }
        BeliefPolarity::Qualified => Err(PerspectiveError::QualifiedWithoutCounterpoint {
            topic: topic.clone(),
        }),
        BeliefPolarity::Opposed => Err(PerspectiveError::OpposedUnsupported {
            topic: topic.clone(),
        }),
    }
}

/// Validate persisted Perspective semantics against one immutable pack set.
/// This is shared by persistence save/load paths so well-formed JSON cannot
/// bypass the transition rules enforced by the mutation operators above.
pub fn validate_perspective_against_pack(
    state: &PerspectiveState,
    packs: &qxfx0_semantic::KnowledgePackSet,
) -> Vec<String> {
    let mut violations = state.validate();
    let facts = packs.facts();

    for (topic, opinion) in &state.opinions {
        let primary = match facts.select(&opinion.primary_fact) {
            Ok(fact) if &fact.subject == topic => Some(fact),
            Ok(fact) => {
                violations.push(format!(
                    "perspective primary fact '{}' belongs to '{}' instead of '{}'",
                    opinion.primary_fact, fact.subject.0, topic.0
                ));
                None
            }
            Err(error) => {
                violations.push(format!(
                    "perspective opinion '{}' has invalid primary authority: {}",
                    topic.0, error
                ));
                None
            }
        };
        let mut has_counterpoint = false;
        for fact_id in &opinion.grounding_facts {
            match facts.select(fact_id) {
                Ok(fact) if &fact.subject == topic => {
                    has_counterpoint |= fact.conditions.iter().any(|condition| {
                        matches!(
                            condition,
                            FactCondition::Counters(target) if target == &opinion.primary_fact
                        )
                    });
                }
                Ok(fact) => violations.push(format!(
                    "perspective fact '{}' belongs to '{}' instead of '{}'",
                    fact_id, fact.subject.0, topic.0
                )),
                Err(error) => violations.push(format!(
                    "perspective opinion '{}' has invalid authority: {}",
                    topic.0, error
                )),
            }
        }
        match opinion.polarity {
            BeliefPolarity::Affirmed if has_counterpoint => violations.push(format!(
                "perspective opinion '{}' is affirmed despite a curated counterpoint",
                topic.0
            )),
            BeliefPolarity::Qualified if !has_counterpoint => violations.push(format!(
                "perspective opinion '{}' is qualified without a curated counterpoint",
                topic.0
            )),
            BeliefPolarity::Opposed => violations.push(format!(
                "perspective opinion '{}' uses unsupported opposed polarity",
                topic.0
            )),
            BeliefPolarity::Affirmed | BeliefPolarity::Qualified => {}
        }
        if primary.is_some() && opinion.revision_seq == 0 {
            violations.push(format!(
                "perspective opinion '{}' has zero revision sequence",
                topic.0
            ));
        }
    }

    let mut topic_polarity = std::collections::BTreeMap::new();
    let mut topic_episode_count = std::collections::BTreeMap::<ConceptId, usize>::new();
    let mut previous_turn = None;
    for episode in &state.episodes {
        if episode.turn_seq == 0 {
            violations.push(format!(
                "perspective episode {} has zero turn sequence",
                episode.id.0
            ));
        }
        if previous_turn.is_some_and(|turn| episode.turn_seq < turn) {
            violations.push("perspective episode turn sequences are not monotonic".into());
        }
        previous_turn = Some(episode.turn_seq);

        let Some(opinion) = state.opinions.get(&episode.topic) else {
            violations.push(format!(
                "perspective episode {} has no opinion for '{}'",
                episode.id.0, episode.topic.0
            ));
            continue;
        };
        let expected_previous = topic_polarity.get(&episode.topic).copied();
        if expected_previous.is_some() && episode.previous_polarity != expected_previous {
            violations.push(format!(
                "perspective episode {} previous polarity breaks topic history",
                episode.id.0
            ));
        }

        let mut has_primary = false;
        let mut has_counter = false;
        let mut has_consequence = false;
        for fact_id in &episode.cited_facts {
            match facts.select(fact_id) {
                Ok(fact) if fact.subject == episode.topic => {
                    has_primary |= fact_id == &opinion.primary_fact;
                    has_counter |= fact.conditions.iter().any(|condition| {
                        matches!(
                            condition,
                            FactCondition::Counters(target) if target == &opinion.primary_fact
                        )
                    });
                    has_consequence |= fact.conditions.iter().any(|condition| {
                        matches!(
                            condition,
                            FactCondition::FollowsFrom(target) if target == &opinion.primary_fact
                        )
                    });
                }
                Ok(fact) => violations.push(format!(
                    "perspective episode {} cites fact '{}' for another topic '{}'",
                    episode.id.0, fact_id, fact.subject.0
                )),
                Err(error) => violations.push(format!(
                    "perspective episode {} has invalid authority: {}",
                    episode.id.0, error
                )),
            }
        }
        match episode.reason {
            PerspectiveRevisionReason::EstablishedFromCuratedFact => {
                if episode.previous_polarity.is_some()
                    || episode.resulting_polarity != BeliefPolarity::Affirmed
                    || !has_primary
                {
                    violations.push(format!(
                        "perspective episode {} is not a valid establishment transition",
                        episode.id.0
                    ));
                }
            }
            PerspectiveRevisionReason::QualifiedByCuratedCounterpoint => {
                if episode.previous_polarity.is_none()
                    || episode.resulting_polarity != BeliefPolarity::Qualified
                    || !has_primary
                    || !has_counter
                {
                    violations.push(format!(
                        "perspective episode {} is not a valid qualification transition",
                        episode.id.0
                    ));
                }
            }
            PerspectiveRevisionReason::ReinforcedByCuratedConsequence => {
                if episode.previous_polarity != Some(episode.resulting_polarity)
                    || !has_primary
                    || !has_consequence
                {
                    violations.push(format!(
                        "perspective episode {} is not a valid reinforcement transition",
                        episode.id.0
                    ));
                }
            }
        }
        topic_polarity.insert(episode.topic.clone(), episode.resulting_polarity);
        *topic_episode_count
            .entry(episode.topic.clone())
            .or_default() += 1;
    }

    for (topic, opinion) in &state.opinions {
        let episode_count = topic_episode_count.get(topic).copied().unwrap_or(0);
        if episode_count == 0 {
            violations.push(format!(
                "perspective opinion '{}' has no revision episode",
                topic.0
            ));
        } else if opinion.revision_seq < episode_count {
            violations.push(format!(
                "perspective opinion '{}' revision sequence precedes its episodes",
                topic.0
            ));
        }
        if let Some(resulting) = topic_polarity.get(topic) {
            if resulting != &opinion.polarity {
                violations.push(format!(
                    "perspective opinion '{}' polarity differs from its latest episode",
                    topic.0
                ));
            }
        }
    }
    violations
}

fn establish_opinion(
    state: &mut PerspectiveState,
    turn_seq: usize,
    fact: &qxfx0_semantic::FactRecord,
) -> Result<bool, PerspectiveError> {
    if let Some(existing) = state.opinions.get(&fact.subject) {
        if existing.primary_fact != fact.id {
            return Err(PerspectiveError::PrimaryConflict {
                subject: fact.subject.clone(),
                existing: existing.primary_fact.clone(),
                new: fact.id.clone(),
            });
        }
        return Ok(false);
    }

    let opinion = OpinionCore {
        topic: fact.subject.clone(),
        primary_fact: fact.id.clone(),
        polarity: BeliefPolarity::Affirmed,
        grounding_facts: BTreeSet::from([fact.id.clone()]),
        confidence_basis_points: fact.confidence_basis_points,
        revision_seq: 1,
    };
    state.opinions.insert(fact.subject.clone(), opinion.clone());
    append_episode(
        state,
        turn_seq,
        &fact.subject,
        None,
        opinion.polarity,
        vec![fact.id.clone()],
        PerspectiveRevisionReason::EstablishedFromCuratedFact,
    )?;
    Ok(true)
}

fn qualify_opinion(
    state: &mut PerspectiveState,
    turn_seq: usize,
    fact: &qxfx0_semantic::FactRecord,
) -> Result<bool, PerspectiveError> {
    let opinion = state.opinions.get_mut(&fact.subject).ok_or_else(|| {
        PerspectiveError::NoThesisForCounterpoint {
            fact: fact.id.clone(),
        }
    })?;
    let counters_primary = fact.conditions.iter().any(|condition| {
        matches!(condition, FactCondition::Counters(target) if target == &opinion.primary_fact)
    });
    if !counters_primary {
        return Err(PerspectiveError::CounterpointDoesNotCite {
            fact: fact.id.clone(),
            primary: opinion.primary_fact.clone(),
        });
    }
    if opinion.grounding_facts.contains(&fact.id) {
        return Ok(false);
    }

    let previous = opinion.polarity;
    let primary = opinion.primary_fact.clone();
    opinion.grounding_facts.insert(fact.id.clone());
    opinion.polarity = BeliefPolarity::Qualified;
    opinion.confidence_basis_points = opinion
        .confidence_basis_points
        .min(fact.confidence_basis_points);
    opinion.revision_seq = opinion
        .revision_seq
        .checked_add(1)
        .ok_or(PerspectiveError::RevisionOverflow)?;
    let resulting = opinion.polarity;
    append_episode(
        state,
        turn_seq,
        &fact.subject,
        Some(previous),
        resulting,
        vec![primary, fact.id.clone()],
        PerspectiveRevisionReason::QualifiedByCuratedCounterpoint,
    )?;
    Ok(true)
}

fn reinforce_opinion(
    state: &mut PerspectiveState,
    turn_seq: usize,
    fact: &qxfx0_semantic::FactRecord,
) -> Result<bool, PerspectiveError> {
    let opinion = state.opinions.get_mut(&fact.subject).ok_or_else(|| {
        PerspectiveError::NoThesisForConsequence {
            fact: fact.id.clone(),
        }
    })?;
    let follows_primary = fact.conditions.iter().any(|condition| {
        matches!(condition, FactCondition::FollowsFrom(target) if target == &opinion.primary_fact)
    });
    if !follows_primary {
        return Err(PerspectiveError::ConsequenceDoesNotCite {
            fact: fact.id.clone(),
            primary: opinion.primary_fact.clone(),
        });
    }
    if opinion.grounding_facts.contains(&fact.id) {
        return Ok(false);
    }

    let polarity = opinion.polarity;
    let primary = opinion.primary_fact.clone();
    opinion.grounding_facts.insert(fact.id.clone());
    opinion.confidence_basis_points = opinion
        .confidence_basis_points
        .max(fact.confidence_basis_points);
    opinion.revision_seq = opinion
        .revision_seq
        .checked_add(1)
        .ok_or(PerspectiveError::RevisionOverflow)?;
    append_episode(
        state,
        turn_seq,
        &fact.subject,
        Some(polarity),
        polarity,
        vec![primary, fact.id.clone()],
        PerspectiveRevisionReason::ReinforcedByCuratedConsequence,
    )?;
    Ok(true)
}

#[allow(clippy::too_many_arguments)]
fn append_episode(
    state: &mut PerspectiveState,
    turn_seq: usize,
    topic: &ConceptId,
    previous_polarity: Option<BeliefPolarity>,
    resulting_polarity: BeliefPolarity,
    cited_facts: Vec<FactId>,
    reason: PerspectiveRevisionReason,
) -> Result<(), PerspectiveError> {
    let episode_id = state.next_episode_id;
    state.next_episode_id = state
        .next_episode_id
        .checked_add(1)
        .ok_or(PerspectiveError::EpisodeOverflow)?;
    state.episodes.push(PerspectiveEpisode {
        id: PerspectiveEpisodeId(episode_id),
        turn_seq,
        topic: topic.clone(),
        previous_polarity,
        resulting_polarity,
        cited_facts,
        reason,
    });
    if state.episodes.len() > MAX_PERSPECTIVE_EPISODES {
        let excess = state.episodes.len() - MAX_PERSPECTIVE_EPISODES;
        state.episodes.drain(0..excess);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_semantic::{KnowledgePackSet, KnowledgePackSource};
    use serde_json::{json, Value};
    use sha2::{Digest, Sha256};

    fn freedom_claims() -> Vec<(ClaimRole, FactId)> {
        [
            (ClaimRole::Thesis, "fact.freedom_choice"),
            (ClaimRole::Counterpoint, "fact.freedom_choice.counterpoint"),
            (ClaimRole::Consequence, "fact.freedom_choice.consequence"),
        ]
        .into_iter()
        .map(|(role, id)| (role, FactId::try_new(id).unwrap()))
        .collect()
    }

    fn active_pack_without_freedom_counter_condition() -> KnowledgePackSet {
        let concepts = include_bytes!("../../data/packs/philosophy-core-v1/concepts.json").to_vec();
        let relations =
            include_bytes!("../../data/packs/philosophy-core-v1/relations.json").to_vec();
        let mut facts: Value = serde_json::from_slice(include_bytes!(
            "../../data/packs/philosophy-core-v1/facts.json"
        ))
        .unwrap();
        let counterpoint = facts
            .as_array_mut()
            .unwrap()
            .iter_mut()
            .find(|binding| binding["record"]["id"] == json!("fact.freedom_choice.counterpoint"))
            .unwrap();
        counterpoint["record"]["conditions"] = json!([]);
        let facts = serde_json::to_vec(&facts).unwrap();

        let mut manifest: Value = serde_json::from_slice(include_bytes!(
            "../../data/packs/philosophy-core-v1/manifest.json"
        ))
        .unwrap();
        manifest["files"]["facts.json"] = json!(format!("{:x}", Sha256::digest(&facts)));
        let manifest = serde_json::to_vec(&manifest).unwrap();
        KnowledgePackSet::load(
            &[KnowledgePackSource {
                manifest: &manifest,
                concepts: &concepts,
                facts: &facts,
                theses: b"",
                relations: &relations,
                lifecycle: b"",
                evidence: b"",
                evidence_links: b"",
                assessments: b"",
            }],
            &qxfx0_semantic::seed_graph(),
        )
        .unwrap()
    }

    #[test]
    fn curated_counterpoint_produces_cited_revision_episode() {
        let facts = qxfx0_semantic::active_pack_set().facts();
        let (state, update) =
            integrate_curated_claims(&PerspectiveState::default(), 1, &freedom_claims(), facts)
                .unwrap();

        assert!(update.opinion_changed);
        assert_eq!(update.episodes_added, 3);
        let opinion = state
            .opinions
            .get(&ConceptId("concept.свобода".into()))
            .unwrap();
        assert_eq!(opinion.polarity, BeliefPolarity::Qualified);
        assert_eq!(opinion.grounding_facts.len(), 3);
        let revision = &state.episodes[1];
        assert_eq!(
            revision.reason,
            PerspectiveRevisionReason::QualifiedByCuratedCounterpoint
        );
        assert_eq!(revision.previous_polarity, Some(BeliefPolarity::Affirmed));
        assert_eq!(revision.resulting_polarity, BeliefPolarity::Qualified);
        assert_eq!(revision.cited_facts.len(), 2);
    }

    #[test]
    fn replaying_same_claims_is_idempotent() {
        let facts = qxfx0_semantic::active_pack_set().facts();
        let (first, _) =
            integrate_curated_claims(&PerspectiveState::default(), 1, &freedom_claims(), facts)
                .unwrap();
        let (second, update) =
            integrate_curated_claims(&first, 2, &freedom_claims(), facts).unwrap();
        assert!(!update.opinion_changed);
        assert_eq!(update.episodes_added, 0);
        assert_eq!(second, first);
    }

    #[test]
    fn unknown_fact_cannot_enter_perspective() {
        let claims = vec![(
            ClaimRole::Thesis,
            FactId::try_new("fact.not-in-pack").unwrap(),
        )];
        assert!(integrate_curated_claims(
            &PerspectiveState::default(),
            1,
            &claims,
            qxfx0_semantic::active_pack_set().facts(),
        )
        .is_err());
    }

    #[test]
    fn render_stance_is_neutral_before_evidence_and_qualified_after_integration() {
        let facts = qxfx0_semantic::active_pack_set().facts();
        let topic = ConceptId("concept.свобода".into());
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        assert_eq!(
            resolve_render_stance(&PerspectiveState::default(), &topic, &thesis, facts).unwrap(),
            PerspectiveRenderStance::Neutral
        );

        let thesis_only = vec![(ClaimRole::Thesis, thesis.clone())];
        let (affirmed, _) =
            integrate_curated_claims(&PerspectiveState::default(), 1, &thesis_only, facts).unwrap();
        assert_eq!(
            resolve_render_stance(&affirmed, &topic, &thesis, facts).unwrap(),
            PerspectiveRenderStance::Affirmed
        );

        let (state, _) =
            integrate_curated_claims(&PerspectiveState::default(), 1, &freedom_claims(), facts)
                .unwrap();
        assert_eq!(
            resolve_render_stance(&state, &topic, &thesis, facts).unwrap(),
            PerspectiveRenderStance::Qualified
        );
    }

    #[test]
    fn render_stance_rejects_forged_grounding_and_unsupported_opposition() {
        let facts = qxfx0_semantic::active_pack_set().facts();
        let topic = ConceptId("concept.свобода".into());
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        let (mut state, _) =
            integrate_curated_claims(&PerspectiveState::default(), 1, &freedom_claims(), facts)
                .unwrap();
        state
            .opinions
            .get_mut(&topic)
            .unwrap()
            .grounding_facts
            .insert(FactId::try_new("fact.forged").unwrap());
        assert!(resolve_render_stance(&state, &topic, &thesis, facts).is_err());

        state
            .opinions
            .get_mut(&topic)
            .unwrap()
            .grounding_facts
            .remove(&FactId::try_new("fact.forged").unwrap());
        state.opinions.get_mut(&topic).unwrap().polarity = BeliefPolarity::Opposed;
        assert!(resolve_render_stance(&state, &topic, &thesis, facts).is_err());
    }

    #[test]
    fn typed_errors_carry_variants_and_historical_messages() {
        let facts = qxfx0_semantic::active_pack_set().facts();
        // DialogueAct never enters evidence: typed, not a string.
        let dialogue = vec![(
            ClaimRole::DialogueAct,
            FactId::try_new("fact.freedom_choice").unwrap(),
        )];
        assert_eq!(
            integrate_curated_claims(&PerspectiveState::default(), 1, &dialogue, facts)
                .unwrap_err(),
            PerspectiveError::DialogueActRejected
        );
        // Unknown fact wraps the registry error; Display still names it.
        let unknown = vec![(
            ClaimRole::Thesis,
            FactId::try_new("fact.no-such-fact").unwrap(),
        )];
        let error =
            integrate_curated_claims(&PerspectiveState::default(), 1, &unknown, facts).unwrap_err();
        assert!(matches!(error, PerspectiveError::FactSelection(_)));
        assert!(error.to_string().contains("fact.no-such-fact"));
        // Every Display reproduces the historical message byte-for-byte.
        let topic = ConceptId("concept.свобода".into());
        for (error, message) in [
            (
                PerspectiveError::MultiSubject,
                "Perspective evidence spans multiple concept subjects",
            ),
            (
                PerspectiveError::OpinionCapacity { max: 16 },
                "Perspective opinion capacity 16 reached",
            ),
            (
                PerspectiveError::QualifiedWithoutCounterpoint {
                    topic: topic.clone(),
                },
                "Perspective opinion for 'concept.свобода' is qualified without a curated counterpoint",
            ),
            (
                PerspectiveError::OpposedUnsupported {
                    topic: topic.clone(),
                },
                "Perspective opinion for 'concept.свобода' uses unsupported opposed polarity",
            ),
            (
                PerspectiveError::RevisionOverflow,
                "Perspective revision sequence overflow",
            ),
            (
                PerspectiveError::EpisodeOverflow,
                "Perspective episode id overflow",
            ),
        ] {
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn semantic_validation_rejects_qualified_opinion_with_only_its_thesis() {
        let packs = qxfx0_semantic::active_pack_set();
        let topic = ConceptId("concept.свобода".into());
        let thesis = FactId::try_new("fact.freedom_choice").unwrap();
        let (mut state, _) = integrate_curated_claims(
            &PerspectiveState::default(),
            1,
            &[(ClaimRole::Thesis, thesis)],
            packs.facts(),
        )
        .unwrap();
        state.opinions.get_mut(&topic).unwrap().polarity = BeliefPolarity::Qualified;
        assert!(
            state.validate().is_empty(),
            "fixture must remain well formed"
        );

        let violations = validate_perspective_against_pack(&state, packs);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("qualified without a curated counterpoint")),
            "{violations:?}"
        );
    }

    #[test]
    fn changed_pack_conditions_with_same_fact_ids_invalidate_perspective() {
        let active = qxfx0_semantic::active_pack_set();
        let (qualified, _) = integrate_curated_claims(
            &PerspectiveState::default(),
            1,
            &freedom_claims(),
            active.facts(),
        )
        .unwrap();
        assert!(validate_perspective_against_pack(&qualified, active).is_empty());

        let changed = active_pack_without_freedom_counter_condition();
        assert_ne!(changed.fingerprint(), active.fingerprint());
        for (_, fact_id) in freedom_claims() {
            assert!(changed.facts().select(&fact_id).is_ok());
        }
        let violations = validate_perspective_against_pack(&qualified, &changed);
        assert!(
            violations
                .iter()
                .any(|violation| violation.contains("qualified without a curated counterpoint")),
            "{violations:?}"
        );
    }
}

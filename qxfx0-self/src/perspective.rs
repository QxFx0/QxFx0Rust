use qxfx0_semantic::{ClaimRole, FactCondition, FactRegistry};
use qxfx0_types::{
    BeliefPolarity, ConceptId, FactId, OpinionCore, PerspectiveEpisode, PerspectiveEpisodeId,
    PerspectiveRevisionReason, PerspectiveState, MAX_PERSPECTIVE_EPISODES,
    MAX_PERSPECTIVE_OPINIONS,
};
use std::collections::BTreeSet;

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
) -> Result<(PerspectiveState, PerspectiveUpdate), String> {
    if claims.is_empty() {
        return Ok((state.clone(), PerspectiveUpdate::unchanged()));
    }

    let mut selected = Vec::with_capacity(claims.len());
    let mut topic = None;
    for (role, fact_id) in claims {
        if *role == ClaimRole::DialogueAct {
            return Err("dialogue acts cannot enter Perspective evidence".into());
        }
        let fact = facts.select(fact_id).map_err(|error| error.to_string())?;
        if let Some(expected) = &topic {
            if expected != &fact.subject {
                return Err("Perspective evidence spans multiple concept subjects".into());
            }
        } else {
            topic = Some(fact.subject.clone());
        }
        selected.push((*role, fact));
    }
    let topic = topic.expect("non-empty claims must select one subject");
    if !state.opinions.contains_key(&topic) && state.opinions.len() >= MAX_PERSPECTIVE_OPINIONS {
        return Err(format!(
            "Perspective opinion capacity {MAX_PERSPECTIVE_OPINIONS} reached"
        ));
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
            ClaimRole::DialogueAct => unreachable!("rejected before fact selection"),
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
) -> Result<PerspectiveRenderStance, String> {
    let thesis = facts
        .select(thesis_fact_id)
        .map_err(|error| error.to_string())?;
    if &thesis.subject != topic {
        return Err(format!(
            "Perspective thesis '{}' is not about topic '{}'",
            thesis_fact_id, topic.0
        ));
    }

    let Some(opinion) = state.opinions.get(topic) else {
        return Ok(PerspectiveRenderStance::Neutral);
    };
    if &opinion.topic != topic {
        return Err(format!(
            "Perspective opinion key '{}' differs from payload topic '{}'",
            topic.0, opinion.topic.0
        ));
    }
    if &opinion.primary_fact != thesis_fact_id {
        return Err(format!(
            "Perspective opinion for '{}' cites primary fact '{}' instead of rendered thesis '{}'",
            topic.0, opinion.primary_fact, thesis_fact_id
        ));
    }
    if !opinion.grounding_facts.contains(thesis_fact_id) {
        return Err(format!(
            "Perspective opinion for '{}' omits rendered thesis '{}' from its grounding",
            topic.0, thesis_fact_id
        ));
    }

    let mut has_curated_counterpoint = false;
    for grounding_id in &opinion.grounding_facts {
        let grounding = facts
            .select(grounding_id)
            .map_err(|error| error.to_string())?;
        if &grounding.subject != topic {
            return Err(format!(
                "Perspective grounding fact '{}' is not about topic '{}'",
                grounding_id, topic.0
            ));
        }
        has_curated_counterpoint |= grounding.conditions.iter().any(|condition| {
            matches!(condition, FactCondition::Counters(target) if target == thesis_fact_id)
        });
    }

    match opinion.polarity {
        BeliefPolarity::Affirmed if !has_curated_counterpoint => {
            Ok(PerspectiveRenderStance::Affirmed)
        }
        BeliefPolarity::Affirmed => Err(format!(
            "Perspective opinion for '{}' is affirmed despite a curated counterpoint",
            topic.0
        )),
        BeliefPolarity::Qualified if has_curated_counterpoint => {
            Ok(PerspectiveRenderStance::Qualified)
        }
        BeliefPolarity::Qualified => Err(format!(
            "Perspective opinion for '{}' is qualified without a curated counterpoint",
            topic.0
        )),
        BeliefPolarity::Opposed => Err(format!(
            "Perspective opinion for '{}' uses unsupported opposed polarity",
            topic.0
        )),
    }
}

fn establish_opinion(
    state: &mut PerspectiveState,
    turn_seq: usize,
    fact: &qxfx0_semantic::FactRecord,
) -> Result<bool, String> {
    if let Some(existing) = state.opinions.get(&fact.subject) {
        if existing.primary_fact != fact.id {
            return Err(format!(
                "Perspective primary fact conflict for '{}': '{}' versus '{}'",
                fact.subject.0, existing.primary_fact, fact.id
            ));
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
) -> Result<bool, String> {
    let opinion = state.opinions.get_mut(&fact.subject).ok_or_else(|| {
        format!(
            "Perspective counterpoint '{}' has no established thesis",
            fact.id
        )
    })?;
    let counters_primary = fact.conditions.iter().any(|condition| {
        matches!(condition, FactCondition::Counters(target) if target == &opinion.primary_fact)
    });
    if !counters_primary {
        return Err(format!(
            "Perspective counterpoint '{}' does not cite primary fact '{}'",
            fact.id, opinion.primary_fact
        ));
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
        .ok_or_else(|| "Perspective revision sequence overflow".to_string())?;
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
) -> Result<bool, String> {
    let opinion = state.opinions.get_mut(&fact.subject).ok_or_else(|| {
        format!(
            "Perspective consequence '{}' has no established thesis",
            fact.id
        )
    })?;
    let follows_primary = fact.conditions.iter().any(|condition| {
        matches!(condition, FactCondition::FollowsFrom(target) if target == &opinion.primary_fact)
    });
    if !follows_primary {
        return Err(format!(
            "Perspective consequence '{}' does not cite primary fact '{}'",
            fact.id, opinion.primary_fact
        ));
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
        .ok_or_else(|| "Perspective revision sequence overflow".to_string())?;
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
) -> Result<(), String> {
    let episode_id = state.next_episode_id;
    state.next_episode_id = state
        .next_episode_id
        .checked_add(1)
        .ok_or_else(|| "Perspective episode id overflow".to_string())?;
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
}

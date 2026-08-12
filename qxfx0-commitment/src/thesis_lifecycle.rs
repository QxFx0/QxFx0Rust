//! Pure P1 thesis lifecycle operations, separate from the legacy commitment API.

use qxfx0_types::{
    RelationTriggerBasis, Thesis, ThesisDigest, ThesisError, ThesisLifecycle, ThesisLifecycleEvent,
    ThesisLifecycleValidationError, ThesisRelation, ThesisRelationKind, ThesisRelationTrigger,
    ThesisRevision, ThesisRevisionAction, ThesisRevisionReason, ThesisStatus, MAX_THESIS_EVENTS,
    MAX_THESIS_REVISIONS, MAX_THESIS_TRIGGERS,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LifecycleApply {
    Applied,
    Replayed,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LifecycleError {
    #[error(transparent)]
    Thesis(#[from] ThesisError),
    #[error(transparent)]
    InvalidState(#[from] ThesisLifecycleValidationError),
    #[error("expected head is stale or absent")]
    StaleHead,
    #[error("digest is unknown")]
    UnknownDigest,
    #[error("event sequence or logical turn is stale")]
    StaleEvent,
    #[error("event sequence was already used by a different event")]
    ReplayConflict,
    #[error("transition from {from:?} via {action:?} is illegal")]
    IllegalTransition {
        from: ThesisStatus,
        action: ThesisRevisionAction,
    },
    #[error("relation kind is invalid for this operation")]
    WrongRelationKind,
    #[error("relation endpoints do not target the active thesis")]
    WrongRelationEndpoints,
    #[error("contradiction requires an explicit Contradicts relation or a closed typed rule")]
    UnprovenContradiction,
    #[error("revision must preserve the stable thesis id")]
    UnstableThesisId,
    #[error("revision digest is already in history; rollback/cycle is forbidden")]
    RevisionCycle,
    #[error("lifecycle bound reached")]
    CapacityReached,
}

pub struct ThesisLifecycleOps;

impl ThesisLifecycleOps {
    pub fn activate(
        state: &ThesisLifecycle,
        expected_head: ThesisDigest,
        sequence: u64,
        logical_turn: u64,
    ) -> Result<(ThesisLifecycle, LifecycleApply), LifecycleError> {
        let event = event(
            sequence,
            logical_turn,
            ThesisRevisionAction::Activate,
            ThesisRevisionReason::InitialActivation,
            Some(expected_head),
            Some(expected_head),
            None,
        );
        apply_event(state, event, |next| {
            require_head(next, expected_head)?;
            require_status(next, &[ThesisStatus::Draft], ThesisRevisionAction::Activate)?;
            next.status = ThesisStatus::Active;
            revision_mut(next, expected_head)?.status = ThesisStatus::Active;
            Ok(())
        })
    }

    pub fn register_counterargument(
        state: &ThesisLifecycle,
        expected_head: ThesisDigest,
        counterargument: &Thesis,
        relation: ThesisRelation,
        sequence: u64,
        logical_turn: u64,
    ) -> Result<(ThesisLifecycle, LifecycleApply), LifecycleError> {
        let trigger = checked_trigger(
            state,
            expected_head,
            counterargument,
            relation,
            Some(ThesisRelationKind::Counters),
            RelationTriggerBasis::ExplicitRelation,
        )?;
        let event = event(
            sequence,
            logical_turn,
            ThesisRevisionAction::RegisterCounterargument,
            ThesisRevisionReason::ExplicitCounterargument,
            Some(expected_head),
            Some(expected_head),
            Some(trigger.clone()),
        );
        apply_event(state, event, |next| {
            require_head(next, expected_head)?;
            require_status(
                next,
                &[ThesisStatus::Active, ThesisStatus::Contested],
                ThesisRevisionAction::RegisterCounterargument,
            )?;
            insert_trigger(next, trigger)?;
            next.status = ThesisStatus::Contested;
            revision_mut(next, expected_head)?.status = ThesisStatus::Contested;
            Ok(())
        })
    }

    pub fn register_contradiction(
        state: &ThesisLifecycle,
        expected_head: ThesisDigest,
        contradicting: &Thesis,
        relation: ThesisRelation,
        basis: RelationTriggerBasis,
        sequence: u64,
        logical_turn: u64,
    ) -> Result<(ThesisLifecycle, LifecycleApply), LifecycleError> {
        let required_kind = match basis {
            RelationTriggerBasis::ExplicitRelation => {
                if relation.kind != ThesisRelationKind::Contradicts {
                    return Err(LifecycleError::UnprovenContradiction);
                }
                Some(ThesisRelationKind::Contradicts)
            }
            // ClosedRule variants are the exhaustive non-textual allow-list.
            RelationTriggerBasis::ClosedRule(_) => None,
        };
        let trigger = checked_trigger(
            state,
            expected_head,
            contradicting,
            relation,
            required_kind,
            basis,
        )?;
        let event = event(
            sequence,
            logical_turn,
            ThesisRevisionAction::RegisterContradiction,
            ThesisRevisionReason::ExplicitContradiction,
            Some(expected_head),
            Some(expected_head),
            Some(trigger.clone()),
        );
        apply_event(state, event, |next| {
            require_head(next, expected_head)?;
            require_status(
                next,
                &[ThesisStatus::Active, ThesisStatus::Contested],
                ThesisRevisionAction::RegisterContradiction,
            )?;
            insert_trigger(next, trigger)?;
            next.status = ThesisStatus::Contested;
            revision_mut(next, expected_head)?.status = ThesisStatus::Contested;
            Ok(())
        })
    }

    pub fn revise(
        state: &ThesisLifecycle,
        expected_head: ThesisDigest,
        revised: Thesis,
        reason: ThesisRevisionReason,
        sequence: u64,
        logical_turn: u64,
    ) -> Result<(ThesisLifecycle, LifecycleApply), LifecycleError> {
        if revised.id != state.thesis_id {
            return Err(LifecycleError::UnstableThesisId);
        }
        let digest = revised.canonical_digest()?;
        let event = event(
            sequence,
            logical_turn,
            ThesisRevisionAction::Revise,
            reason,
            Some(expected_head),
            Some(digest),
            None,
        );
        apply_event(state, event, |next| {
            require_head(next, expected_head)?;
            require_status(
                next,
                &[ThesisStatus::Active, ThesisStatus::Contested],
                ThesisRevisionAction::Revise,
            )?;
            if next.revisions.contains_key(&digest) {
                return Err(LifecycleError::RevisionCycle);
            }
            if next.revisions.len() >= MAX_THESIS_REVISIONS {
                return Err(LifecycleError::CapacityReached);
            }
            revision_mut(next, expected_head)?.status = ThesisStatus::Superseded;
            next.revisions.insert(
                digest,
                ThesisRevision {
                    thesis: revised,
                    digest,
                    status: ThesisStatus::Active,
                },
            );
            next.active_head = Some(digest);
            next.status = ThesisStatus::Active;
            Ok(())
        })
    }

    pub fn supersede(
        state: &ThesisLifecycle,
        expected_head: ThesisDigest,
        replacement: &Thesis,
        relation: ThesisRelation,
        sequence: u64,
        logical_turn: u64,
    ) -> Result<(ThesisLifecycle, LifecycleApply), LifecycleError> {
        let trigger = checked_trigger(
            state,
            expected_head,
            replacement,
            relation,
            Some(ThesisRelationKind::Supersedes),
            RelationTriggerBasis::ExplicitRelation,
        )?;
        let event = event(
            sequence,
            logical_turn,
            ThesisRevisionAction::Supersede,
            ThesisRevisionReason::ReplacedByStrongerThesis,
            Some(expected_head),
            None,
            Some(trigger.clone()),
        );
        apply_event(state, event, |next| {
            require_head(next, expected_head)?;
            require_status(
                next,
                &[ThesisStatus::Active, ThesisStatus::Contested],
                ThesisRevisionAction::Supersede,
            )?;
            insert_trigger(next, trigger)?;
            revision_mut(next, expected_head)?.status = ThesisStatus::Superseded;
            next.status = ThesisStatus::Superseded;
            next.active_head = None;
            Ok(())
        })
    }

    pub fn retract(
        state: &ThesisLifecycle,
        expected_head: ThesisDigest,
        sequence: u64,
        logical_turn: u64,
    ) -> Result<(ThesisLifecycle, LifecycleApply), LifecycleError> {
        let event = event(
            sequence,
            logical_turn,
            ThesisRevisionAction::Retract,
            ThesisRevisionReason::ExplicitRetraction,
            Some(expected_head),
            None,
            None,
        );
        apply_event(state, event, |next| {
            require_head(next, expected_head)?;
            require_status(
                next,
                &[ThesisStatus::Active, ThesisStatus::Contested],
                ThesisRevisionAction::Retract,
            )?;
            revision_mut(next, expected_head)?.status = ThesisStatus::Retracted;
            next.status = ThesisStatus::Retracted;
            next.active_head = None;
            Ok(())
        })
    }
}

fn event(
    sequence: u64,
    logical_turn: u64,
    action: ThesisRevisionAction,
    reason: ThesisRevisionReason,
    previous_head: Option<ThesisDigest>,
    resulting_head: Option<ThesisDigest>,
    relation_trigger: Option<ThesisRelationTrigger>,
) -> ThesisLifecycleEvent {
    ThesisLifecycleEvent {
        sequence,
        logical_turn,
        action,
        reason,
        previous_head,
        resulting_head,
        relation_trigger,
    }
}

fn apply_event<F>(
    state: &ThesisLifecycle,
    event: ThesisLifecycleEvent,
    transition: F,
) -> Result<(ThesisLifecycle, LifecycleApply), LifecycleError>
where
    F: FnOnce(&mut ThesisLifecycle) -> Result<(), LifecycleError>,
{
    state.validate_lifecycle()?;
    if let Some(existing) = state.history.get(&event.sequence) {
        return if existing == &event {
            Ok((state.clone(), LifecycleApply::Replayed))
        } else {
            Err(LifecycleError::ReplayConflict)
        };
    }
    let (last_sequence, last_turn) = state
        .history
        .last_key_value()
        .map(|(_, event)| (event.sequence, event.logical_turn))
        .unwrap_or((0, 0));
    if event.sequence != last_sequence + 1 || event.logical_turn <= last_turn {
        return Err(LifecycleError::StaleEvent);
    }
    if state.history.len() >= MAX_THESIS_EVENTS {
        return Err(LifecycleError::CapacityReached);
    }
    let mut next = state.clone();
    transition(&mut next)?;
    next.history.insert(event.sequence, event);
    next.validate_lifecycle()?;
    Ok((next, LifecycleApply::Applied))
}

fn require_head(state: &ThesisLifecycle, expected: ThesisDigest) -> Result<(), LifecycleError> {
    if !state.revisions.contains_key(&expected) {
        return Err(LifecycleError::UnknownDigest);
    }
    if state.active_head != Some(expected) {
        return Err(LifecycleError::StaleHead);
    }
    Ok(())
}
fn revision_mut(
    state: &mut ThesisLifecycle,
    digest: ThesisDigest,
) -> Result<&mut ThesisRevision, LifecycleError> {
    state
        .revisions
        .get_mut(&digest)
        .ok_or(LifecycleError::UnknownDigest)
}
fn require_status(
    state: &ThesisLifecycle,
    allowed: &[ThesisStatus],
    action: ThesisRevisionAction,
) -> Result<(), LifecycleError> {
    if allowed.contains(&state.status) {
        Ok(())
    } else {
        Err(LifecycleError::IllegalTransition {
            from: state.status,
            action,
        })
    }
}
fn insert_trigger(
    state: &mut ThesisLifecycle,
    trigger: ThesisRelationTrigger,
) -> Result<(), LifecycleError> {
    if !state.relation_triggers.contains(&trigger)
        && state.relation_triggers.len() >= MAX_THESIS_TRIGGERS
    {
        return Err(LifecycleError::CapacityReached);
    }
    state.relation_triggers.insert(trigger);
    Ok(())
}
fn checked_trigger(
    state: &ThesisLifecycle,
    expected_head: ThesisDigest,
    other: &Thesis,
    relation: ThesisRelation,
    required: Option<ThesisRelationKind>,
    basis: RelationTriggerBasis,
) -> Result<ThesisRelationTrigger, LifecycleError> {
    other.validate()?;
    if required.is_some_and(|required| relation.kind != required) {
        return Err(LifecycleError::WrongRelationKind);
    }
    if relation.from != other.id || relation.to != state.thesis_id {
        return Err(LifecycleError::WrongRelationEndpoints);
    }
    Ok(ThesisRelationTrigger {
        relation,
        from_digest: other.canonical_digest()?,
        to_digest: expected_head,
        basis,
    })
}

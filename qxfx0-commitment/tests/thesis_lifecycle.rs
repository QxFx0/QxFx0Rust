use qxfx0_commitment::{LifecycleApply, LifecycleError, ThesisLifecycleOps};
use qxfx0_types::{
    ClosedContradictionRule, ConceptId, RelationId, RelationTriggerBasis, Thesis, ThesisId,
    ThesisKind, ThesisLifecycle, ThesisRelation, ThesisRelationKind, ThesisRevisionAction,
    ThesisRevisionReason, ThesisStatus,
};
use std::collections::BTreeMap;

fn thesis(id: &str, object: &str) -> Thesis {
    Thesis {
        id: ThesisId::try_new(id).unwrap(),
        subject: ConceptId("subject".into()),
        predicate: RelationId::try_new("is").unwrap(),
        object: ConceptId(object.into()),
        kind: ThesisKind::InterpretiveClaim,
        qualifiers: BTreeMap::new(),
        surface_text: None,
        confidence_basis_points: None,
        provenance: BTreeMap::new(),
        valid_from: None,
        valid_to: None,
    }
}
fn relation(from: &Thesis, kind: ThesisRelationKind, to: &Thesis) -> ThesisRelation {
    ThesisRelation {
        from: from.id.clone(),
        kind,
        to: to.id.clone(),
    }
}
fn active() -> (Thesis, ThesisLifecycle) {
    let value = thesis("stable", "v1");
    let draft = ThesisLifecycle::draft(value.clone()).unwrap();
    let head = draft.active_head.unwrap();
    let (state, applied) = ThesisLifecycleOps::activate(&draft, head, 1, 1).unwrap();
    assert_eq!(applied, LifecycleApply::Applied);
    (value, state)
}

#[test]
fn transition_and_replay_matrix_is_pure_and_idempotent() {
    let (base, state) = active();
    let head = state.active_head.unwrap();
    let (same, replay) =
        ThesisLifecycleOps::activate(&ThesisLifecycle::draft(base).unwrap(), head, 1, 1).unwrap();
    assert_eq!(replay, LifecycleApply::Applied); // independent deterministic replay from draft
    let (unchanged, replay) = ThesisLifecycleOps::activate(&same, head, 1, 1).unwrap();
    assert_eq!(replay, LifecycleApply::Replayed);
    assert_eq!(same, unchanged);

    for terminal in [ThesisStatus::Superseded, ThesisStatus::Retracted] {
        let mut invalid = state.clone();
        invalid.status = terminal;
        invalid.active_head = None;
        assert!(ThesisLifecycleOps::activate(&invalid, head, 2, 2).is_err());
    }
}

#[test]
fn counters_are_not_contradictions_and_no_text_heuristic_is_used() {
    let (base, state) = active();
    let head = state.active_head.unwrap();
    let other = thesis("other", "not-v1 wrong contradict");
    let counters = relation(&other, ThesisRelationKind::Counters, &base);
    let (contested, _) =
        ThesisLifecycleOps::register_counterargument(&state, head, &other, counters.clone(), 2, 2)
            .unwrap();
    assert_eq!(contested.status, ThesisStatus::Contested);
    assert_eq!(
        contested.history[&2].action,
        ThesisRevisionAction::RegisterCounterargument
    );
    assert!(matches!(
        ThesisLifecycleOps::register_contradiction(
            &state,
            head,
            &other,
            counters,
            RelationTriggerBasis::ExplicitRelation,
            2,
            2
        ),
        Err(LifecycleError::UnprovenContradiction)
    ));
    let explicit = relation(&other, ThesisRelationKind::Contradicts, &base);
    let (contradicted, _) = ThesisLifecycleOps::register_contradiction(
        &state,
        head,
        &other,
        explicit.clone(),
        RelationTriggerBasis::ExplicitRelation,
        2,
        2,
    )
    .unwrap();
    assert_eq!(
        contradicted.history[&2].action,
        ThesisRevisionAction::RegisterContradiction
    );
    let (typed, _) = ThesisLifecycleOps::register_contradiction(
        &state,
        head,
        &other,
        relation(&other, ThesisRelationKind::Counters, &base),
        RelationTriggerBasis::ClosedRule(ClosedContradictionRule::MutuallyExclusiveObjects),
        2,
        2,
    )
    .unwrap();
    assert_eq!(typed.status, ThesisStatus::Contested);
}

#[test]
fn revision_preserves_old_digest_rejects_stale_receipts_and_cycles() {
    let (_, state) = active();
    let old = state.active_head.unwrap();
    let revised = thesis("stable", "v2");
    let new = revised.canonical_digest().unwrap();
    let (state, _) = ThesisLifecycleOps::revise(
        &state,
        old,
        revised,
        ThesisRevisionReason::NewEvidence,
        2,
        4,
    )
    .unwrap();
    assert_eq!(state.active_head, Some(new));
    assert!(state.revisions.contains_key(&old));
    assert_eq!(state.revisions[&old].status, ThesisStatus::Superseded);
    assert!(matches!(
        ThesisLifecycleOps::retract(&state, old, 3, 5),
        Err(LifecycleError::StaleHead)
    ));
    let rollback = thesis("stable", "v1");
    assert!(matches!(
        ThesisLifecycleOps::revise(
            &state,
            new,
            rollback,
            ThesisRevisionReason::CorrectedError,
            3,
            5
        ),
        Err(LifecycleError::RevisionCycle)
    ));
    let alien = thesis("alien", "v3");
    assert!(matches!(
        ThesisLifecycleOps::revise(&state, new, alien, ThesisRevisionReason::NewEvidence, 3, 5),
        Err(LifecycleError::UnstableThesisId)
    ));
}

#[test]
fn ordering_terminal_transitions_and_terminal_replay_are_fail_closed() {
    let (base, state) = active();
    let head = state.active_head.unwrap();
    assert!(matches!(
        ThesisLifecycleOps::retract(&state, head, 3, 3),
        Err(LifecycleError::StaleEvent)
    ));
    assert!(matches!(
        ThesisLifecycleOps::retract(&state, head, 2, 1),
        Err(LifecycleError::StaleEvent)
    ));
    let (retracted, _) = ThesisLifecycleOps::retract(&state, head, 2, 2).unwrap();
    let (same, replay) = ThesisLifecycleOps::retract(&retracted, head, 2, 2).unwrap();
    assert_eq!(replay, LifecycleApply::Replayed);
    assert_eq!(same, retracted);
    assert!(matches!(
        ThesisLifecycleOps::retract(&retracted, head, 3, 3),
        Err(LifecycleError::StaleHead)
    ));

    let replacement = thesis("replacement", "v2");
    let rel = relation(&replacement, ThesisRelationKind::Supersedes, &base);
    let (superseded, _) =
        ThesisLifecycleOps::supersede(&state, head, &replacement, rel.clone(), 2, 2).unwrap();
    let (same, replay) =
        ThesisLifecycleOps::supersede(&superseded, head, &replacement, rel, 2, 2).unwrap();
    assert_eq!(replay, LifecycleApply::Replayed);
    assert_eq!(same, superseded);
}

#[test]
fn serde_defaults_and_stable_round_trip_are_deterministic() {
    let (base, state) = active();
    let json = serde_json::to_string(&state).unwrap();
    assert_eq!(
        json,
        serde_json::to_string(&serde_json::from_str::<ThesisLifecycle>(&json).unwrap()).unwrap()
    );
    let minimal = format!(
        r#"{{"thesis_id":"stable","active_head":"{}","revisions":{{}}}}"#,
        base.canonical_digest().unwrap()
    );
    let decoded: ThesisLifecycle = serde_json::from_str(&minimal).unwrap();
    assert_eq!(decoded.status, ThesisStatus::Draft);
    assert!(decoded.history.is_empty() && decoded.relation_triggers.is_empty());
}

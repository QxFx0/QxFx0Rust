//! Additive, read-only projection of the P1 thesis lifecycle for Perspective consumers.
//! Existing `PerspectiveRegistry` behavior and DTOs are intentionally untouched.

use qxfx0_types::{ThesisDigest, ThesisId, ThesisLifecycle, ThesisStatus};

/// Minimal deterministic shadow DTO. It contains no wall-clock or floating-point data.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ThesisPerspectiveProjection {
    pub thesis_id: ThesisId,
    pub status: ThesisStatus,
    pub active_head: Option<ThesisDigest>,
    pub revision_count: usize,
    pub event_count: usize,
    pub relation_trigger_count: usize,
    pub last_sequence: Option<u64>,
    pub last_logical_turn: Option<u64>,
    pub contested: bool,
    pub terminal: bool,
}

/// Additive adapter: projection cannot mutate either lifecycle or PerspectiveRegistry.
pub fn project_thesis_lifecycle(state: &ThesisLifecycle) -> ThesisPerspectiveProjection {
    let last = state.history.last_key_value().map(|(_, event)| event);
    ThesisPerspectiveProjection {
        thesis_id: state.thesis_id.clone(),
        status: state.status,
        active_head: state.active_head,
        revision_count: state.revisions.len(),
        event_count: state.history.len(),
        relation_trigger_count: state.relation_triggers.len(),
        last_sequence: last.map(|event| event.sequence),
        last_logical_turn: last.map(|event| event.logical_turn),
        contested: state.status == ThesisStatus::Contested,
        terminal: matches!(
            state.status,
            ThesisStatus::Superseded | ThesisStatus::Retracted
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::perspective::PerspectiveRegistry;
    use qxfx0_types::{ConceptId, RelationId, Thesis, ThesisKind, ThesisLifecycle, ThesisStatus};
    use std::collections::BTreeMap;

    fn lifecycle() -> ThesisLifecycle {
        ThesisLifecycle::draft(Thesis {
            id: qxfx0_types::ThesisId::try_new("shadow").unwrap(),
            subject: ConceptId("subject".into()),
            predicate: RelationId::try_new("is").unwrap(),
            object: ConceptId("object".into()),
            kind: ThesisKind::InterpretiveClaim,
            qualifiers: BTreeMap::new(),
            surface_text: None,
            confidence_basis_points: None,
            provenance: BTreeMap::new(),
            valid_from: None,
            valid_to: None,
        })
        .unwrap()
    }

    #[test]
    fn projection_is_deterministic_and_registry_shadow_is_unchanged() {
        let state = lifecycle();
        let registry = PerspectiveRegistry::default();
        let before = registry.replay_digest();
        let left = project_thesis_lifecycle(&state);
        let right = project_thesis_lifecycle(&state);
        assert_eq!(left, right);
        assert_eq!(left.status, ThesisStatus::Draft);
        assert_eq!(before, registry.replay_digest());
        assert_eq!(
            serde_json::to_string(&left).unwrap(),
            serde_json::to_string(&right).unwrap()
        );
    }
}

//! Authority-routed reads of the live essence layer (ADR-0044
//! migration M3). Turn-scoped sites read the turn's own authority;
//! post-hoc surfaces (codex reports) read the session's last recorded
//! authority, defaulting to V1. The V1 `EssenceState` is no longer
//! written under V2 authority — only read as the pre-migration record.

use qxfx0_self_v2::{empty_essence, Essence};
use qxfx0_types::system_state::SystemState;

use crate::turn_types::SubjectAuthority;

/// Decode the opaque V2 trajectory. `None` (never advanced) reads as the
/// empty carrier; a corrupt value fails closed — the caller decides
/// whether the turn (propagate) or a degraded surface (report) follows.
pub fn decode_essence_v2(state: &SystemState) -> Result<Essence, String> {
    match state.semantic.essence_v2.as_ref() {
        None => Ok(empty_essence()),
        Some(value) => serde_json::from_value(value.clone())
            .map_err(|error| format!("essence_v2 shadow state failed to decode: {error}")),
    }
}

/// The behaviourally-relevant essence numbers, whichever layer is live.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct EssenceView {
    pub angst: f64,
    pub committed: bool,
    pub witness_count: usize,
    pub capacity: usize,
    /// Newest witnessed conatus scalar (`f64::MAX` when unwitnessed —
    /// the same empty-carrier convention the anomaly evidence uses).
    pub last_conatus: f64,
}

/// Read the live layer. Infallible by contract: an undecodable V2 value
/// reads as the empty carrier with a warning (the turn itself still
/// fails closed at the advance site, so corruption can never hide
/// behind this read for long).
pub fn essence_view(state: &SystemState, authority: SubjectAuthority) -> EssenceView {
    match authority {
        SubjectAuthority::V1Authority => {
            let essence = &state.semantic.essence;
            EssenceView {
                angst: essence.angst,
                committed: essence.commitment.is_some(),
                witness_count: essence.witnesses.len(),
                capacity: essence.capacity,
                last_conatus: essence
                    .witnesses
                    .last()
                    .map(|witness| witness.conatus_scalar)
                    .unwrap_or(f64::MAX),
            }
        }
        SubjectAuthority::V2Authority => match decode_essence_v2(state) {
            Ok(Essence::Committed(trajectory, _)) => EssenceView {
                angst: trajectory.angst_level,
                committed: true,
                witness_count: trajectory.witnesses.len(),
                capacity: trajectory.capacity,
                last_conatus: trajectory
                    .witnesses
                    .back()
                    .map(|witness| witness.conatus_scalar)
                    .unwrap_or(f64::MAX),
            },
            Ok(Essence::Uncommitted(trajectory)) => EssenceView {
                angst: trajectory.angst_level,
                committed: false,
                witness_count: trajectory.witnesses.len(),
                capacity: trajectory.capacity,
                last_conatus: trajectory
                    .witnesses
                    .back()
                    .map(|witness| witness.conatus_scalar)
                    .unwrap_or(f64::MAX),
            },
            Err(error) => {
                tracing::warn!("essence_view fell back to the empty carrier: {error}");
                EssenceView {
                    angst: 0.0,
                    committed: false,
                    witness_count: 0,
                    capacity: 0,
                    last_conatus: f64::MAX,
                }
            }
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn v1_view_reads_the_working_layer() {
        let mut state = SystemState {
            session_id: "view".into(),
            ..SystemState::default()
        };
        state.semantic.essence.angst = 0.4;
        let view = essence_view(&state, SubjectAuthority::V1Authority);
        assert_eq!(view.angst, 0.4);
        assert!(!view.committed);
        assert_eq!(view.last_conatus, f64::MAX);
    }

    #[test]
    fn v2_view_reads_the_empty_carrier_and_fails_closed_on_corruption() {
        let state = SystemState {
            session_id: "view".into(),
            ..SystemState::default()
        };
        let view = essence_view(&state, SubjectAuthority::V2Authority);
        assert_eq!(view.angst, 0.0);
        assert!(!view.committed);
        assert!(decode_essence_v2(&state).is_ok());

        let mut corrupt = SystemState {
            session_id: "view".into(),
            ..SystemState::default()
        };
        corrupt.semantic.essence_v2 = Some(serde_json::json!({"nope": true}));
        assert!(decode_essence_v2(&corrupt).is_err());
        // The infallible view degrades loudly, never panics.
        let degraded = essence_view(&corrupt, SubjectAuthority::V2Authority);
        assert_eq!(degraded.witness_count, 0);
    }
}

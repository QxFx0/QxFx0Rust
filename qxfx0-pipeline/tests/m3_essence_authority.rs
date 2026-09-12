//! ADR-0044 migration M3: the V2 advance is the authority under
//! `V2Authority` — the V1 essence layer is neither written nor read.
//! Collapse still journals (authority-agnostic event shape); the V1
//! path is pinned unchanged beside it.

use qxfx0_pipeline::{process_turn_with_options, SubjectAuthority, TurnInput, TurnOptions};
use qxfx0_types::system_state::SystemState;

fn v2_options() -> TurnOptions {
    TurnOptions::new().with_subject_authority(SubjectAuthority::V2Authority)
}

fn v1_options() -> TurnOptions {
    TurnOptions::new()
}

fn turn(state: &mut SystemState, text: &str, options: TurnOptions) {
    let input = TurnInput {
        session_id: state.session_id.clone(),
        raw_text: text.to_string(),
    };
    process_turn_with_options(&input, state, options);
}

fn v2_angst(state: &SystemState) -> f64 {
    qxfx0_pipeline::essence_view::essence_view(state, SubjectAuthority::V2Authority).angst
}

fn v2_witnesses(state: &SystemState) -> usize {
    qxfx0_pipeline::essence_view::essence_view(state, SubjectAuthority::V2Authority).witness_count
}

#[test]
fn v2_authority_advances_v2_and_leaves_v1_untouched() {
    let mut state = SystemState {
        session_id: "m3-untouched".into(),
        ..SystemState::default()
    };
    for _ in 0..3 {
        turn(&mut state, "что такое свобода?", v2_options());
    }
    assert_eq!(v2_witnesses(&state), 3, "V2 is the live layer");
    assert_eq!(state.semantic.essence.angst, 0.0, "V1 unwritten");
    assert!(
        state.semantic.essence.witnesses.is_empty(),
        "V1 unwitnessed"
    );
    assert!(
        state.semantic.essence.commitment.is_none(),
        "V1 uncommitted"
    );
}

#[test]
fn v1_authority_keeps_the_old_write_path() {
    let mut state = SystemState {
        session_id: "m3-v1".into(),
        ..SystemState::default()
    };
    for _ in 0..3 {
        turn(&mut state, "что такое свобода?", v1_options());
    }
    assert!(
        !state.semantic.essence.witnesses.is_empty(),
        "V1 still witnesses"
    );
    assert!(
        state.semantic.essence_v2.is_some(),
        "V2 shadow still observes"
    );
}

#[test]
fn collapse_under_v2_journals_and_clears_v2() {
    use qxfx0_self_v2::{empty_essence, Essence as EssenceV2};
    let EssenceV2::Uncommitted(mut trajectory) = empty_essence() else {
        panic!("empty carrier starts uncommitted");
    };
    trajectory.angst_level = 0.95;
    let mut state = SystemState {
        session_id: "m3-collapse".into(),
        ..SystemState::default()
    };
    state.semantic.essence_v2 =
        Some(serde_json::to_value(EssenceV2::Uncommitted(trajectory)).unwrap());
    turn(&mut state, "что такое я?", v2_options());
    assert_eq!(v2_angst(&state), 0.0, "V2 collapsed");
    assert_eq!(state.semantic.essence.reset_events.len(), 1, "journal kept");
    assert_eq!(state.semantic.essence.reset_events[0].turn, 1);
    assert_eq!(state.semantic.essence.angst, 0.0, "V1 untouched");
}

#[test]
fn collapse_under_v1_keeps_the_old_path() {
    let mut state = SystemState {
        session_id: "m3-collapse-v1".into(),
        ..SystemState::default()
    };
    state.semantic.essence.angst = 0.95;
    turn(&mut state, "что такое я?", v1_options());
    assert_eq!(state.semantic.essence.angst, 0.0, "V1 collapsed");
    assert_eq!(state.semantic.essence.reset_events.len(), 1, "journal kept");
}

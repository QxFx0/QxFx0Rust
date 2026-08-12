use qxfx0_pipeline::fact_grounded::ThesisProjectionRollout;
use qxfx0_pipeline::{
    process_turn_with_options, process_turn_with_options_and_trace, RendererAuthority,
    ResponsePlanV2Authority, TurnInput, TurnOptions,
};
use qxfx0_types::{SystemState, ThesisObservationOutcome};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{fs, path::PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Corpus {
    schema: String,
    corpus_id: String,
    authority_change: String,
    persistence_change: String,
    feedback_authority_change: String,
    raw_user_logs: bool,
    reviewed_formulations_only: bool,
    source_policy: String,
    cases: Vec<Case>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct Case {
    case_id: String,
    input: String,
    renderer: String,
    v2_authority: bool,
    expected_outcome: ThesisObservationOutcome,
}

fn root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn corpus_bytes() -> Vec<u8> {
    fs::read(root().join("data/gates/thesis-observation-v1/corpus.json")).unwrap()
}

fn state(session_id: &str) -> SystemState {
    SystemState {
        session_id: session_id.into(),
        ..SystemState::default()
    }
}

fn options(case: &Case, shadow: bool) -> TurnOptions {
    let renderer = match case.renderer.as_str() {
        "audited_plan" => RendererAuthority::AuditedPlan,
        "legacy_shadow" => RendererAuthority::LegacyShadow,
        unexpected => panic!("unknown corpus renderer: {unexpected}"),
    };
    let options = TurnOptions::new().with_renderer(renderer);
    let options = if shadow {
        options.with_thesis_projection(ThesisProjectionRollout::Shadow)
    } else {
        options
    };
    if case.v2_authority {
        options.with_response_plan_v2_authority(ResponsePlanV2Authority::Canary)
    } else {
        options
    }
}

#[test]
fn thesis_observation_local_v1_corpus_has_zero_budget_failures() {
    let bytes = corpus_bytes();
    let corpus: Corpus = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(corpus.schema, "qxfx0.thesis-observation-corpus.v1");
    assert_eq!(corpus.corpus_id, "thesis-observation-local-v1");
    assert_eq!(corpus.authority_change, "none");
    assert_eq!(corpus.persistence_change, "none");
    assert_eq!(corpus.feedback_authority_change, "none");
    assert!(!corpus.raw_user_logs);
    assert!(corpus.reviewed_formulations_only);
    assert_eq!(corpus.source_policy, "curated_synthetic_only");
    assert_eq!(corpus.cases.len(), 12);

    let mut observed = 0;
    for case in &corpus.cases {
        let session_id = format!("thesis-observation-corpus:{}", case.case_id);
        let input = TurnInput {
            session_id: session_id.clone(),
            raw_text: case.input.clone(),
        };
        let mut baseline = state(&session_id);
        let baseline_output =
            process_turn_with_options(&input, &mut baseline, options(case, false));
        let baseline_state = serde_json::to_vec(&baseline).unwrap();

        let mut shadow = state(&session_id);
        let (shadow_output, trace) =
            process_turn_with_options_and_trace(&input, &mut shadow, options(case, true));
        assert_eq!(
            serde_json::to_vec(&shadow_output).unwrap(),
            serde_json::to_vec(&baseline_output).unwrap(),
            "output parity: {}",
            case.case_id
        );
        assert_eq!(
            serde_json::to_vec(&shadow).unwrap(),
            baseline_state,
            "state parity: {}",
            case.case_id
        );
        assert!(
            shadow.semantic.thesis_state.is_empty(),
            "no thesis state: {}",
            case.case_id
        );

        let receipt = trace
            .thesis_observation_receipt
            .as_ref()
            .expect("shadow must produce exactly one receipt");
        receipt.validate().unwrap();
        assert_eq!(
            receipt.outcome(),
            case.expected_outcome,
            "outcome: {}",
            case.case_id
        );
        let artifact = serde_json::to_string(receipt).unwrap();
        if !case.input.is_empty() {
            assert!(
                !artifact.contains(&case.input),
                "raw input leak: {}",
                case.case_id
            );
        }
        assert!(
            !artifact.contains(&shadow_output.response),
            "response leak: {}",
            case.case_id
        );
        assert!(
            !artifact.contains(&session_id),
            "session leak: {}",
            case.case_id
        );
        if receipt.outcome() == ThesisObservationOutcome::Observed {
            observed += 1;
            assert!(receipt.thesis_id().is_some());
            assert!(receipt.thesis_digest().is_some());
            assert!(receipt.fact_id().is_some());
            assert!(receipt.pack_fingerprint().is_some());
        } else {
            assert!(receipt.thesis_id().is_none());
            assert!(receipt.thesis_digest().is_none());
            assert!(receipt.fact_id().is_none());
            assert!(receipt.pack_fingerprint().is_none());
        }

        let mut replay = state(&session_id);
        let (replay_output, replay_trace) =
            process_turn_with_options_and_trace(&input, &mut replay, options(case, true));
        assert_eq!(
            serde_json::to_vec(&replay_output).unwrap(),
            serde_json::to_vec(&shadow_output).unwrap(),
            "output replay: {}",
            case.case_id
        );
        assert_eq!(
            serde_json::to_vec(&replay).unwrap(),
            serde_json::to_vec(&shadow).unwrap()
        );
        assert_eq!(replay_trace.replay_signature(), trace.replay_signature());
        assert_eq!(
            replay_trace.thesis_observation_receipt.unwrap().digest(),
            receipt.digest(),
            "receipt replay: {}",
            case.case_id
        );
    }
    assert_eq!(observed, 7);
    assert_eq!(format!("{:x}", Sha256::digest(&bytes)).len(), 64);
}

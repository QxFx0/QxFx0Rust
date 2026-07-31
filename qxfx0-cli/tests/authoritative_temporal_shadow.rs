use ed25519_dalek::{Signer, SigningKey};
use qxfx0_cli::{
    create_anomaly_shadow_trace_sink, run_turn_with_renderer,
    run_turn_with_renderer_anomaly_shadow_trace, write_anomaly_shadow_trace_jsonl,
};
use qxfx0_pipeline::{
    process_turn_with_renderer_and_signed_stance_decision, RendererAuthority,
    SignedStanceDecisionOutcome, TurnInput,
};
use qxfx0_types::{
    calculate_stance_request_digest, Ed25519StanceDecisionVerifier, SignedStanceDecision,
    StanceAuthorityVerificationPolicy, StanceDecisionAttestation, StancePolarity, StanceTopic,
    SystemState, STANCE_ATTESTATION_VERSION,
};

const SESSION: &str = "authoritative-temporal-shadow";
const RAW_TEXT: &str = "что такое свобода?";
const ISSUER: &str = "qxfx0-test-issuer";
const KEY_ID: &str = "authoritative-shadow-v1";
const AUDIENCE: &str = "qxfx0-turn-service-v1";

#[test]
fn signed_rejected_provenance_round_trips_into_external_temporal_shadow_evidence() {
    let input = TurnInput {
        session_id: SESSION.into(),
        raw_text: RAW_TEXT.into(),
    };
    let mut state = SystemState {
        session_id: SESSION.into(),
        ..SystemState::default()
    };
    let signing_key = SigningKey::from_bytes(&[73; 32]);
    let attestation = StanceDecisionAttestation {
        version: STANCE_ATTESTATION_VERSION,
        issuer_id: ISSUER.into(),
        key_id: KEY_ID.into(),
        audience: AUDIENCE.into(),
        session_id: SESSION.into(),
        expected_pre_turn: 0,
        topic: StanceTopic::new("свобода").unwrap(),
        polarity: StancePolarity::Rejected,
        request_digest: calculate_stance_request_digest(SESSION, RAW_TEXT),
        decision_id: [19; 16],
        issued_at_unix_seconds: 1_700_000_000,
        expires_at_unix_seconds: 1_700_000_060,
    };
    let signed = SignedStanceDecision {
        signature: signing_key
            .sign(&attestation.canonical_bytes().unwrap())
            .to_bytes(),
        attestation,
    };
    let verifier = Ed25519StanceDecisionVerifier::new([(
        (ISSUER.into(), KEY_ID.into()),
        signing_key.verifying_key().to_bytes(),
    )]);
    let policy = StanceAuthorityVerificationPolicy {
        audience: AUDIENCE.into(),
        verification_time_unix_seconds: 1_700_000_030,
        max_validity_seconds: 300,
    };

    let (_, outcome) = process_turn_with_renderer_and_signed_stance_decision(
        &input,
        &mut state,
        RendererAuthority::LegacyShadow,
        Some(&signed),
        &verifier,
        &policy,
    );
    assert_eq!(outcome, SignedStanceDecisionOutcome::Recorded);
    assert_eq!(state.semantic.stance_provenance.len(), 1);
    let recorded = state
        .semantic
        .stance_provenance
        .observations()
        .front()
        .unwrap();
    assert_eq!(recorded.polarity, StancePolarity::Rejected);

    let baseline_db = qxfx0_persistence::Persistence::open_memory().unwrap();
    let traced_db = qxfx0_persistence::Persistence::open_memory().unwrap();
    let replay_db = qxfx0_persistence::Persistence::open_memory().unwrap();
    for db in [&baseline_db, &traced_db, &replay_db] {
        db.save_state(SESSION, &state).unwrap();
        let reloaded = db.load_state(SESSION).unwrap().unwrap();
        assert_eq!(reloaded.semantic.stance_provenance.len(), 1);
    }

    let baseline = run_turn_with_renderer(
        &baseline_db,
        SESSION,
        RAW_TEXT,
        RendererAuthority::LegacyShadow,
    )
    .unwrap();
    let traced = run_turn_with_renderer_anomaly_shadow_trace(
        &traced_db,
        SESSION,
        RAW_TEXT,
        RendererAuthority::LegacyShadow,
    )
    .unwrap();
    let replay = run_turn_with_renderer_anomaly_shadow_trace(
        &replay_db,
        SESSION,
        RAW_TEXT,
        RendererAuthority::LegacyShadow,
    )
    .unwrap();
    assert_eq!(baseline, traced.response);
    assert_eq!(traced.response, replay.response);
    assert_eq!(
        serde_json::to_vec(&traced.trace).unwrap(),
        serde_json::to_vec(&replay.trace).unwrap()
    );

    let anomaly = traced
        .trace
        .steps
        .iter()
        .find(|step| step.stage == "anomaly_shadow")
        .unwrap();
    assert_eq!(
        anomaly.metadata["anomaly_temporal_evidence"],
        "typed_persisted_provenance"
    );
    assert_eq!(anomaly.metadata["anomaly_proposed_kind"], "temporal");
    assert_eq!(anomaly.metadata["anomaly_strategy"], "request_revision");
    assert_eq!(anomaly.metadata["anomaly_reason"], "observation_only");

    let baseline_state = baseline_db.load_state(SESSION).unwrap().unwrap();
    let traced_state = traced_db.load_state(SESSION).unwrap().unwrap();
    let replay_state = replay_db.load_state(SESSION).unwrap().unwrap();
    assert_eq!(traced_state.semantic.stance_provenance.len(), 1);
    assert_eq!(replay_state.semantic.stance_provenance.len(), 1);
    let baseline_digest =
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&baseline_state).unwrap();
    assert_eq!(
        baseline_digest,
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&traced_state).unwrap()
    );
    assert_eq!(
        baseline_digest,
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&replay_state).unwrap()
    );

    let trace_path = std::env::temp_dir().join(format!(
        "qxfx0-authoritative-temporal-shadow-{}.jsonl",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&trace_path);
    let mut sink = create_anomaly_shadow_trace_sink(&trace_path).unwrap();
    write_anomaly_shadow_trace_jsonl(&mut sink, &traced.trace).unwrap();
    drop(sink);
    let external_jsonl = std::fs::read_to_string(&trace_path).unwrap();
    assert!(!external_jsonl.contains(RAW_TEXT));
    assert!(!external_jsonl.contains("signature"));
    assert!(!external_jsonl.contains("private_key"));
    let record: serde_json::Value = serde_json::from_str(&external_jsonl).unwrap();
    assert_eq!(record["schema"], "qxfx0.anomaly-shadow-trace.v1");
    assert_eq!(
        record["trace"]["steps"]
            .as_array()
            .unwrap()
            .iter()
            .find(|step| step["stage"] == "anomaly_shadow")
            .unwrap()["metadata"]["anomaly_proposed_kind"],
        "temporal"
    );
    std::fs::remove_file(trace_path).unwrap();
}

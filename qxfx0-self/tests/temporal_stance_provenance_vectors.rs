use std::fs;

use qxfx0_self::anomaly::{
    detect_anomaly, AnomalyKind, AnomalyRecoveryResult, AnomalyRecoveryStrategy,
};
use qxfx0_self::stance::{
    detect_temporal_contradiction, BoundedStanceProvenance, StanceObservation, StancePolarity,
    StanceRecordOutcome, StanceSource, StanceTopic,
};
use serde_json::Value;

fn observation(value: &Value) -> StanceObservation {
    StanceObservation {
        turn: value["turn"].as_u64().expect("turn") as usize,
        topic: StanceTopic::new(value["topic"].as_str().expect("topic")).expect("valid topic"),
        polarity: match value["polarity"].as_str().expect("polarity") {
            "affirmed" => StancePolarity::Affirmed,
            "rejected" => StancePolarity::Rejected,
            other => panic!("unsupported polarity: {other}"),
        },
        source: match value["source"].as_str().expect("source") {
            "system_decision" => StanceSource::SystemDecision,
            "user_input" => StanceSource::UserInput,
            "external_reference" => StanceSource::ExternalReference,
            other => panic!("unsupported source: {other}"),
        },
    }
}

#[test]
fn temporal_stance_provenance_v1_conforms_to_reference_vectors() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/reference-vectors/temporal-stance-provenance-v1.json"
    ))
    .expect("reference vector must be present");
    let vectors: Value = serde_json::from_str(&source).expect("reference vector must parse");
    assert_eq!(
        vectors["schema"].as_str(),
        Some("qxfx0.temporal-stance-provenance.v1")
    );

    for vector in vectors["vectors"].as_array().expect("vectors") {
        let mut provenance =
            BoundedStanceProvenance::new(vectors["capacity"].as_u64().expect("capacity") as usize);
        for historical in vector["history"].as_array().expect("history") {
            assert_eq!(
                provenance.record(observation(historical)),
                StanceRecordOutcome::Recorded
            );
        }
        let current = observation(&vector["current"]);
        let contradiction = detect_temporal_contradiction(&provenance, &current);
        if !vector["expect"]["contradiction"]
            .as_bool()
            .expect("expectation")
        {
            assert!(contradiction.is_none(), "{}", vector["name"]);
            continue;
        }
        let contradiction = contradiction.expect("expected contradiction");
        assert_eq!(
            contradiction.historical.turn,
            vector["expect"]["historical_turn"]
                .as_u64()
                .expect("historical turn") as usize
        );
        let evidence = contradiction.to_anomaly_evidence();
        let qxfx0_self::anomaly::AnomalyEvidence::Temporal {
            current_stance,
            historical_stance,
            ..
        } = &evidence
        else {
            panic!("typed bridge must produce temporal evidence");
        };
        assert_eq!(
            current_stance,
            vector["expect"]["current_stance"]
                .as_str()
                .expect("current stance")
        );
        assert_eq!(
            historical_stance,
            vector["expect"]["historical_stance"]
                .as_str()
                .expect("historical stance")
        );
        let decision = detect_anomaly(evidence).expect("temporal evidence must be detected");
        assert_eq!(decision.kind, AnomalyKind::Temporal);
        assert_eq!(decision.strategy, AnomalyRecoveryStrategy::RequestRevision);
        assert_eq!(decision.result, AnomalyRecoveryResult::RevisionRequested);
        assert_eq!(
            decision.idempotency_key,
            vector["expect"]["idempotency_key"]
                .as_str()
                .expect("idempotency key")
        );
    }
}

#[test]
fn stance_provenance_is_idempotent_bounded_and_replay_deterministic() {
    let first = StanceObservation {
        turn: 3,
        topic: StanceTopic::new("свобода").unwrap(),
        polarity: StancePolarity::Affirmed,
        source: StanceSource::SystemDecision,
    };
    let second = StanceObservation {
        turn: 4,
        topic: StanceTopic::new("ответственность").unwrap(),
        polarity: StancePolarity::Affirmed,
        source: StanceSource::SystemDecision,
    };
    let third = StanceObservation {
        turn: 5,
        topic: StanceTopic::new("свобода").unwrap(),
        polarity: StancePolarity::Rejected,
        source: StanceSource::SystemDecision,
    };
    let mut first_replay = BoundedStanceProvenance::new(2);
    let mut second_replay = BoundedStanceProvenance::new(2);
    for store in [&mut first_replay, &mut second_replay] {
        assert_eq!(store.record(first.clone()), StanceRecordOutcome::Recorded);
        assert_eq!(
            store.record(first.clone()),
            StanceRecordOutcome::NoStateTransition
        );
        assert_eq!(store.record(second.clone()), StanceRecordOutcome::Recorded);
        assert_eq!(store.record(third.clone()), StanceRecordOutcome::Recorded);
        assert_eq!(store.len(), 2);
        assert!(store.len() <= store.capacity());
    }
    assert_eq!(
        serde_json::to_vec(&first_replay).unwrap(),
        serde_json::to_vec(&second_replay).unwrap()
    );
    assert!(
        detect_temporal_contradiction(&first_replay, &third).is_none(),
        "the old affirmative observation was evicted at the configured bound"
    );
}

use std::fs;

use qxfx0_self::doubt::{
    compute_doubt, route_for_doubt, BoundedEpisodicStore, DoubtPolicy, EpisodicConfig,
};
use qxfx0_types::{DoubtInput, DoubtRoute, EpisodicEvent};
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct ScoreCase {
    input: DoubtInput,
    expected_score: f64,
}

#[derive(Debug, Deserialize)]
struct RecallCase {
    config: EpisodicConfig,
    current_turn: u64,
    topic: String,
    events: Vec<EpisodicEvent>,
    expected_route: DoubtRoute,
}

#[derive(Debug, Deserialize)]
struct ReferenceVector {
    policy: DoubtPolicy,
    cases: Vec<ScoreCase>,
    recall: RecallCase,
}

#[test]
fn doubt_episodic_v1_conforms_to_reference_vector() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/doubt_episodic_v1.json"
    ))
    .expect("reference vector must be present");
    let vector: ReferenceVector =
        serde_json::from_str(&source).expect("reference vector must parse");

    for case in vector.cases {
        assert!((compute_doubt(case.input).value() - case.expected_score).abs() < f64::EPSILON);
    }
    let store = vector.recall.events.into_iter().fold(
        BoundedEpisodicStore::new(vector.recall.config),
        |store, event| store.record(event),
    );
    let recalled = store.recall(vector.recall.current_turn, Some(&vector.recall.topic));
    assert_eq!(
        route_for_doubt(
            compute_doubt(DoubtInput {
                confidence: 0.1,
                driver: qxfx0_types::DoubtDriver::Resonance
            }),
            vector.policy,
            &recalled
        ),
        vector.recall.expected_route
    );
}

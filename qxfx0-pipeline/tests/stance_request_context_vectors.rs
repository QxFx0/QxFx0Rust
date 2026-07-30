use qxfx0_pipeline::{
    execution_trace::calculate_stable_digest,
    stance_request::{prepare_stance_request_context, STANCE_REQUEST_CONTEXT_VERSION},
    TurnInput,
};
use qxfx0_types::SystemState;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Vector {
    version: u8,
    session_id: String,
    expected_pre_turn: usize,
    raw_text: String,
    normalized_topic: String,
    request_digest_hex: String,
}

#[test]
fn documented_stance_request_context_vector_is_executable_and_pure() {
    let vector: Vector = serde_json::from_str(include_str!(
        "../../docs/reference-vectors/stance-request-context-v1.json"
    ))
    .unwrap();
    let state = SystemState {
        session_id: vector.session_id.clone(),
        dialogue: qxfx0_types::system_state::DialogueState {
            turn_count: vector.expected_pre_turn,
            ..Default::default()
        },
        ..SystemState::default()
    };
    let input = TurnInput {
        session_id: vector.session_id,
        raw_text: vector.raw_text.clone(),
    };
    let before = calculate_stable_digest(&state).unwrap();
    let context = prepare_stance_request_context(&input, &state).unwrap();

    assert_eq!(context.version, STANCE_REQUEST_CONTEXT_VERSION);
    assert_eq!(context.version, vector.version);
    assert_eq!(context.expected_pre_turn, vector.expected_pre_turn);
    assert_eq!(context.normalized_topic.as_str(), vector.normalized_topic);
    assert_eq!(
        encode_hex(&context.request_digest),
        vector.request_digest_hex
    );
    assert_eq!(calculate_stable_digest(&state).unwrap(), before);
    assert!(!serde_json::to_string(&context)
        .unwrap()
        .contains(&vector.raw_text));
}

fn encode_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

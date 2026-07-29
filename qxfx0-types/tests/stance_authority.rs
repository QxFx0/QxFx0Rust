use ed25519_dalek::{Signer, SigningKey};
use qxfx0_types::{
    calculate_stance_request_digest, verify_signed_stance_decision, Ed25519StanceDecisionVerifier,
    SignedStanceDecision, StanceDecisionAttestation, StancePolarity, StanceTopic,
    StanceVerificationContext, StanceVerificationError, STANCE_ATTESTATION_VERSION,
};

const ISSUER_ID: &str = "test-issuer";
const KEY_ID: &str = "test-key-1";
const AUDIENCE: &str = "qxfx0-test";
const PRIVATE_KEY: [u8; 32] = [11; 32];

fn context() -> StanceVerificationContext {
    StanceVerificationContext {
        audience: AUDIENCE.into(),
        session_id: "session-1".into(),
        expected_pre_turn: 7,
        request_digest: calculate_stance_request_digest("session-1", "что такое свобода?"),
        verification_time_unix_seconds: 1_700_000_010,
        max_validity_seconds: 300,
    }
}

fn signed_attestation() -> (SignedStanceDecision, Ed25519StanceDecisionVerifier) {
    let signing_key = SigningKey::from_bytes(&PRIVATE_KEY);
    let attestation = StanceDecisionAttestation {
        version: STANCE_ATTESTATION_VERSION,
        issuer_id: ISSUER_ID.into(),
        key_id: KEY_ID.into(),
        audience: AUDIENCE.into(),
        session_id: "session-1".into(),
        expected_pre_turn: 7,
        topic: StanceTopic::new("свобода").unwrap(),
        polarity: StancePolarity::Rejected,
        request_digest: calculate_stance_request_digest("session-1", "что такое свобода?"),
        decision_id: [42; 16],
        issued_at_unix_seconds: 1_700_000_000,
        expires_at_unix_seconds: 1_700_000_060,
    };
    let signature = signing_key.sign(&attestation.canonical_bytes().unwrap());
    let verifier = Ed25519StanceDecisionVerifier::new([(
        (ISSUER_ID.into(), KEY_ID.into()),
        signing_key.verifying_key().to_bytes(),
    )]);
    (
        SignedStanceDecision {
            attestation,
            signature: signature.to_bytes(),
        },
        verifier,
    )
}

#[test]
fn ed25519_attestation_binds_the_exact_turn_request_and_decision() {
    let (signed, verifier) = signed_attestation();
    let verified = verify_signed_stance_decision(&verifier, &signed, &context()).unwrap();

    assert_eq!(verified.decision().topic.as_str(), "свобода");
    assert_eq!(verified.decision().polarity, StancePolarity::Rejected);
    assert_eq!(verified.decision_id(), [42; 16]);
    assert_eq!(verified.issuer_id(), ISSUER_ID);
}

#[test]
fn attestation_rejects_changed_signature_and_every_context_binding() {
    let (signed, verifier) = signed_attestation();
    let mut bad_signature = signed.clone();
    bad_signature.signature[0] ^= 1;
    assert_eq!(
        verify_signed_stance_decision(&verifier, &bad_signature, &context()).unwrap_err(),
        StanceVerificationError::InvalidSignature
    );

    let mut wrong_audience = context();
    wrong_audience.audience = "another-runtime".into();
    assert_eq!(
        verify_signed_stance_decision(&verifier, &signed, &wrong_audience).unwrap_err(),
        StanceVerificationError::AudienceMismatch
    );

    let mut wrong_session = context();
    wrong_session.session_id = "other-session".into();
    assert_eq!(
        verify_signed_stance_decision(&verifier, &signed, &wrong_session).unwrap_err(),
        StanceVerificationError::SessionMismatch
    );

    let mut wrong_turn = context();
    wrong_turn.expected_pre_turn += 1;
    assert_eq!(
        verify_signed_stance_decision(&verifier, &signed, &wrong_turn).unwrap_err(),
        StanceVerificationError::TurnMismatch
    );

    let mut wrong_digest = context();
    wrong_digest.request_digest = [0; 32];
    assert_eq!(
        verify_signed_stance_decision(&verifier, &signed, &wrong_digest).unwrap_err(),
        StanceVerificationError::RequestDigestMismatch
    );
}

#[test]
fn attestation_expiry_is_deterministic_from_the_explicit_context_time() {
    let (signed, verifier) = signed_attestation();
    let mut too_early = context();
    too_early.verification_time_unix_seconds = 1_699_999_999;
    assert_eq!(
        verify_signed_stance_decision(&verifier, &signed, &too_early).unwrap_err(),
        StanceVerificationError::NotYetValid
    );

    let mut expired = context();
    expired.verification_time_unix_seconds = 1_700_000_061;
    assert_eq!(
        verify_signed_stance_decision(&verifier, &signed, &expired).unwrap_err(),
        StanceVerificationError::Expired
    );

    let mut too_long = context();
    too_long.max_validity_seconds = 30;
    assert_eq!(
        verify_signed_stance_decision(&verifier, &signed, &too_long).unwrap_err(),
        StanceVerificationError::ValidityWindowTooLong
    );
}

#[test]
fn canonical_bytes_and_request_digest_are_repeatable() {
    let (signed, verifier) = signed_attestation();
    let first = signed.attestation.canonical_bytes().unwrap();
    let second = signed.attestation.canonical_bytes().unwrap();
    assert_eq!(first, second);
    assert_eq!(
        calculate_stance_request_digest("session-1", "что такое свобода?"),
        context().request_digest
    );
    assert!(verify_signed_stance_decision(&verifier, &signed, &context()).is_ok());
}

#[test]
fn signed_stance_reference_vector_is_executable() {
    let vector: serde_json::Value = serde_json::from_str(include_str!(
        "../../docs/reference-vectors/signed-stance-attestation-v1.json"
    ))
    .unwrap();
    let topic = vector["topic"].as_str().unwrap();
    let raw_text = vector["raw_text"].as_str().unwrap();
    let session_id = vector["session_id"].as_str().unwrap();
    let request_digest = array32(&decode_hex(vector["request_digest_hex"].as_str().unwrap()));
    let attestation = StanceDecisionAttestation {
        version: vector["version"].as_u64().unwrap() as u8,
        issuer_id: vector["issuer_id"].as_str().unwrap().into(),
        key_id: vector["key_id"].as_str().unwrap().into(),
        audience: vector["audience"].as_str().unwrap().into(),
        session_id: session_id.into(),
        expected_pre_turn: vector["expected_pre_turn"].as_u64().unwrap() as usize,
        topic: StanceTopic::new(topic).unwrap(),
        polarity: StancePolarity::Rejected,
        request_digest,
        decision_id: array16(&decode_hex(vector["decision_id_hex"].as_str().unwrap())),
        issued_at_unix_seconds: vector["issued_at_unix_seconds"].as_i64().unwrap(),
        expires_at_unix_seconds: vector["expires_at_unix_seconds"].as_i64().unwrap(),
    };
    assert_eq!(
        calculate_stance_request_digest(session_id, raw_text),
        request_digest
    );
    assert_eq!(
        hex(&attestation.canonical_bytes().unwrap()),
        vector["canonical_payload_hex"].as_str().unwrap()
    );
    let private = array32(&decode_hex(
        vector["ed25519_private_key_test_only_hex"]
            .as_str()
            .unwrap(),
    ));
    let signing_key = SigningKey::from_bytes(&private);
    let signature = signing_key.sign(&attestation.canonical_bytes().unwrap());
    assert_eq!(
        hex(&signature.to_bytes()),
        vector["signature_hex"].as_str().unwrap()
    );
    let verifier = Ed25519StanceDecisionVerifier::new([(
        (ISSUER_ID.into(), KEY_ID.into()),
        signing_key.verifying_key().to_bytes(),
    )]);
    let signed = SignedStanceDecision {
        attestation,
        signature: signature.to_bytes(),
    };
    let verified = verify_signed_stance_decision(
        &verifier,
        &signed,
        &StanceVerificationContext {
            audience: AUDIENCE.into(),
            session_id: session_id.into(),
            expected_pre_turn: 7,
            request_digest,
            verification_time_unix_seconds: vector["verification_time_unix_seconds"]
                .as_i64()
                .unwrap(),
            max_validity_seconds: 300,
        },
    )
    .unwrap();
    assert_eq!(verified.decision().topic.as_str(), topic);
}

#[test]
fn signed_attestation_deserializes_only_an_exactly_sized_signature() {
    let (signed, _) = signed_attestation();
    let mut encoded = serde_json::to_value(signed).unwrap();
    let signature = encoded["signature"].as_array_mut().unwrap();
    signature.pop();
    assert!(serde_json::from_value::<SignedStanceDecision>(encoded).is_err());
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn decode_hex(value: &str) -> Vec<u8> {
    assert_eq!(value.len() % 2, 0);
    (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap())
        .collect()
}

fn array16(value: &[u8]) -> [u8; 16] {
    value.try_into().unwrap()
}

fn array32(value: &[u8]) -> [u8; 32] {
    value.try_into().unwrap()
}

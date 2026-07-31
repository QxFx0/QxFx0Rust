use qxfx0_self::{
    anomaly::{detect_anomaly, AnomalyEvidence, AnomalyRecoveryDecision},
    temporal_recovery::{
        evaluate_temporal_recovery_eligibility, TemporalRecoveryDenialReason,
        TemporalRecoveryEligibility, TemporalRecoveryEligibilityContext,
    },
};
use qxfx0_types::stance::TemporalStanceContradiction;
use serde_json::Value;

fn decision(name: &str) -> AnomalyRecoveryDecision {
    let evidence = match name {
        "TemporalRequestRevision" => AnomalyEvidence::Temporal {
            turn: 16,
            current_stance: "affirmed".into(),
            historical_stance: "rejected".into(),
        },
        "AntiConatusRestrictRoute" => AnomalyEvidence::AntiConatus {
            turn: 16,
            stance_confidence: 0.9,
            stance_consistent: false,
            angst: 0.95,
            conatus: 2.0,
        },
        other => panic!("unsupported decision fixture: {other}"),
    };
    detect_anomaly(evidence).expect("fixture must produce an anomaly decision")
}

fn merge(base: &Value, overrides: &Value) -> Value {
    let mut merged = base.clone();
    if let (Some(target), Some(source)) = (merged.as_object_mut(), overrides.as_object()) {
        for (key, value) in source {
            let replacement = match target.get(key) {
                Some(existing) if existing.is_object() && value.is_object() => {
                    merge(existing, value)
                }
                _ => value.clone(),
            };
            target.insert(key.clone(), replacement);
        }
    }
    merged
}

#[test]
fn temporal_recovery_enablement_v1_conforms_to_state_machine_vectors() {
    let vectors: Value = serde_json::from_str(include_str!(
        "../../docs/reference-vectors/temporal-recovery-enablement-v1.json"
    ))
    .unwrap();
    assert_eq!(vectors["schema"], "qxfx0.temporal-recovery-enablement.v1");

    for vector in vectors["vectors"].as_array().unwrap() {
        let decision = decision(vector["decision"].as_str().unwrap());
        let contradiction: TemporalStanceContradiction = serde_json::from_value(merge(
            &vectors["base_contradiction"],
            vector.get("contradiction").unwrap_or(&Value::Null),
        ))
        .expect("contradiction must match the typed contract");
        let context: TemporalRecoveryEligibilityContext =
            serde_json::from_value(merge(&vectors["base_context"], &vector["context"]))
                .expect("context must match the typed contract");
        let decision_before = decision.clone();
        let contradiction_before = contradiction.clone();
        let context_before = context.clone();
        let first = evaluate_temporal_recovery_eligibility(&decision, &contradiction, &context);
        let replay = evaluate_temporal_recovery_eligibility(&decision, &contradiction, &context);

        assert_eq!(decision, decision_before, "decision must remain immutable");
        assert_eq!(
            contradiction, contradiction_before,
            "contradiction must remain immutable"
        );
        assert_eq!(context, context_before, "context must remain immutable");
        assert_eq!(first, replay, "eligibility must replay deterministically");
        assert_eq!(
            serde_json::to_vec(&first).unwrap(),
            serde_json::to_vec(&replay).unwrap(),
            "serialized eligibility must be replay stable"
        );

        match vector["expect"]["outcome"].as_str().unwrap() {
            "Eligible" => {
                let TemporalRecoveryEligibility::Eligible(permit) = first else {
                    panic!("{} must be eligible", vector["name"]);
                };
                assert_eq!(permit.max_requests_for_topic_in_window, 1);
                assert_eq!(
                    permit.idempotency_key,
                    vector["expect"]["idempotency_key"].as_str().unwrap()
                );
                let serialized = serde_json::to_value(&permit).unwrap();
                assert!(serialized.get("signature").is_none());
                assert!(serialized.get("attestation").is_none());
                assert!(serialized.get("request_digest").is_none());
            }
            "Denied" => {
                let expected: TemporalRecoveryDenialReason =
                    serde_json::from_value(vector["expect"]["reason"].clone()).unwrap();
                assert_eq!(first, TemporalRecoveryEligibility::Denied(expected));
            }
            other => panic!("unsupported expected outcome: {other}"),
        }
    }
}

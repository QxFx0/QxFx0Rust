use std::fs;

use qxfx0_self::anomaly::{
    detect_anomaly, AnomalyEvidence, AnomalyKind, AnomalyRecoveryLedger, AnomalyRecoveryResult,
    AnomalyRecoveryStrategy, AnomalyReplayOutcome,
};
use serde_json::Value;

fn vector<'a>(vectors: &'a Value, name: &str) -> &'a Value {
    vectors["vectors"]
        .as_array()
        .expect("anomaly vectors must contain cases")
        .iter()
        .find(|vector| vector["name"].as_str() == Some(name))
        .unwrap_or_else(|| panic!("missing anomaly vector: {name}"))
}

fn decision_for(vector: &Value) -> qxfx0_self::anomaly::AnomalyRecoveryDecision {
    let evidence = &vector["evidence"];
    let name = vector["name"].as_str().expect("vector name");
    let evidence = match name {
        "self-referential-collapse" => AnomalyEvidence::SelfReference {
            turn: evidence["turn"].as_u64().unwrap() as usize,
            subject: evidence["subject"].as_str().unwrap().into(),
            angst: evidence["angst"].as_f64().unwrap(),
            witness_count: evidence["witness_count"].as_u64().unwrap() as usize,
        },
        "anti-conatus" => AnomalyEvidence::AntiConatus {
            turn: evidence["turn"].as_u64().unwrap() as usize,
            stance_confidence: evidence["stance_confidence"].as_f64().unwrap(),
            stance_consistent: evidence["stance_consistent"].as_bool().unwrap(),
            angst: evidence["angst"].as_f64().unwrap(),
            conatus: evidence["conatus"].as_f64().unwrap(),
        },
        "temporal-contradiction" => AnomalyEvidence::Temporal {
            turn: evidence["turn"].as_u64().unwrap() as usize,
            current_stance: evidence["current_stance"].as_str().unwrap().into(),
            historical_stance: evidence["historical_stance"].as_str().unwrap().into(),
        },
        other => panic!("unsupported anomaly vector: {other}"),
    };
    detect_anomaly(evidence).expect("reference evidence must be detected")
}

#[test]
fn anomaly_recovery_v1_conforms_to_reference_vectors() {
    let source = fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../docs/reference-vectors/anomaly-recovery-v1.json"
    ))
    .expect("reference vector must be present");
    let vectors: Value = serde_json::from_str(&source).expect("reference vector must parse");
    assert_eq!(
        vectors["schema"].as_str(),
        Some("qxfx0.anomaly-recovery.v1")
    );

    for (name, kind, strategy, result) in [
        (
            "self-referential-collapse",
            AnomalyKind::SelfReferentialCollapse,
            AnomalyRecoveryStrategy::ResetEssence,
            AnomalyRecoveryResult::EssenceReset,
        ),
        (
            "anti-conatus",
            AnomalyKind::AntiConatus,
            AnomalyRecoveryStrategy::RestrictRoute,
            AnomalyRecoveryResult::RouteRestricted,
        ),
        (
            "temporal-contradiction",
            AnomalyKind::Temporal,
            AnomalyRecoveryStrategy::RequestRevision,
            AnomalyRecoveryResult::RevisionRequested,
        ),
    ] {
        let case = vector(&vectors, name);
        let decision = decision_for(case);
        assert_eq!(decision.kind, kind);
        assert_eq!(decision.strategy, strategy);
        assert_eq!(decision.result, result);
        assert_eq!(
            decision.idempotency_key,
            case["expect"]["idempotency_key"].as_str().unwrap()
        );
        assert_eq!(
            decision.max_retries,
            case["expect"]["max_retries"].as_u64().unwrap_or(0) as u8
        );
    }
}

#[test]
fn anomaly_recovery_is_idempotent_and_bounded() {
    let evidence = AnomalyEvidence::Temporal {
        turn: 19,
        current_stance: "Revised".into(),
        historical_stance: "Held".into(),
    };
    let decision = detect_anomaly(evidence).unwrap();
    let mut ledger = AnomalyRecoveryLedger::new(2);
    assert!(matches!(
        ledger.record(decision.clone(), "sha256:state"),
        AnomalyReplayOutcome::Proposed(_)
    ));
    assert!(matches!(
        ledger.record(decision, "sha256:state"),
        AnomalyReplayOutcome::NoStateTransition(_)
    ));
    assert_eq!(ledger.len(), 1);
}

//! Cross-twin conformance, fourth target (ADR-0043 U6 follow-up):
//! `qxfx0-self-v2` Deliberation `reconcile` against
//! `QxFx0.Self.Deliberation` golden cases.
//!
//! The port reduced the plan vocabulary (family×recovery instead of
//! family×style×recovery×tone), so parity runs over the shared subset:
//! style/tone pinned equal, recovery None/Gate only, courtesy None.
//! Compared per case: rule, agreement, reconciled family/confidence/
//! recovery — except on the gate-driver path, where the reconciled plan
//! is structurally diverged (Haskell keeps the formal plan, Rust builds
//! `CMRepair` fresh) and only rule/agreement/divergence compare.
//! Divergence values compare only when `divcmp=1` (identical plans);
//! elsewhere the divisor differs by construction (count/4 vs count/2),
//! documented in the fixture header.

use qxfx0_self_v2::deliberation::{
    reconcile, AgreementV2, DeliberationModulationV2, PlanV2, ReconcileRuleV2,
};
use qxfx0_self_v2::salience::{SalienceVerdictV2, V2SalienceDriver};
use qxfx0_types::CanonicalMoveFamily;

const GOLDEN: &str = include_str!("fixtures/haskell_deliberation_golden.tsv");

fn parse_family(name: &str) -> CanonicalMoveFamily {
    match name {
        "reflect" | "CMReflect" => CanonicalMoveFamily::CMReflect,
        "define" | "CMDefine" => CanonicalMoveFamily::CMDefine,
        "repair" | "CMRepair" => CanonicalMoveFamily::CMRepair,
        _ => panic!("unknown family: {name}"),
    }
}

fn parse_recovery(name: &str) -> Option<String> {
    match name {
        "none" | "Nothing" => None,
        "gate" | "Just RecoveryConatusGate" => Some("conatus_gate_fired".to_string()),
        _ => panic!("unknown recovery: {name}"),
    }
}

fn render_rule(rule: ReconcileRuleV2) -> &'static str {
    match rule {
        ReconcileRuleV2::ConatusOverride => "RuleConatusOverride",
        ReconcileRuleV2::Agreement => "RuleAgreement",
        ReconcileRuleV2::SalienceLead => "RuleSalienceLead",
        ReconcileRuleV2::HolisticAdvantage => "RuleHolisticAdvantage",
        ReconcileRuleV2::FormalAdvantage => "RuleFormalAdvantage",
        ReconcileRuleV2::TiedFallback => "RuleTiedFallback",
    }
}

fn render_agreement(agreement: AgreementV2) -> &'static str {
    match agreement {
        AgreementV2::Agree => "Agree",
        AgreementV2::DivergeOnFamily => "DivergeOnFamily",
        AgreementV2::DivergeOnRecovery => "DivergeOnRecovery",
        AgreementV2::DivergeMultiple => "DivergeMultiple",
    }
}

fn render_recovery(recovery: &Option<String>) -> String {
    match recovery.as_deref() {
        None => "Nothing".to_string(),
        Some("conatus_gate_fired") => "Just RecoveryConatusGate".to_string(),
        Some(other) => panic!("unexpected recovery: {other}"),
    }
}

#[test]
fn deliberation_matches_haskell_on_the_shared_subset() {
    let modulation = DeliberationModulationV2::default();
    let mut cases = 0;
    for line in GOLDEN.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (head, tail) = line.split_once('\t').expect("head/tail");
        let input: Vec<&str> = head.split_whitespace().collect();
        let golden: Vec<&str> = tail.split_whitespace().collect();
        assert_eq!(input.len(), 10, "input head: {line}");
        let tag = input[0].to_string();
        // Tail: rule agree div fam conf rec divcmp; `Just X` carries a
        // space, divcmp anchors the end.
        let divcmp = golden[golden.len() - 1];
        let (rule_g, agree_g, div_g, fam_g, conf_g, rec_g) = if golden.len() == 8 {
            (
                golden[0].to_string(),
                golden[1].to_string(),
                golden[2],
                golden[3],
                golden[4],
                format!("{} {}", golden[5], golden[6]),
            )
        } else {
            assert_eq!(golden.len(), 7, "golden tail: {line}");
            (
                golden[0].to_string(),
                golden[1].to_string(),
                golden[2],
                golden[3],
                golden[4],
                golden[5].to_string(),
            )
        };
        let holistic = PlanV2 {
            family: parse_family(input[1]),
            recovery_cause: parse_recovery(input[3]),
            confidence: input[2].parse().expect("hconf parses"),
        };
        let formal = PlanV2 {
            family: parse_family(input[4]),
            recovery_cause: parse_recovery(input[6]),
            confidence: input[5].parse().expect("fconf parses"),
        };
        let verdict = SalienceVerdictV2 {
            holistic_bias: input[7].parse().expect("bias parses"),
            confidence: input[8].parse().expect("conf parses"),
            driver: match input[9] {
                "resonance" => V2SalienceDriver::Resonance,
                "gate" => V2SalienceDriver::ConatusGate,
                _ => panic!("unknown driver: {}", input[9]),
            },
        };
        let result = reconcile(&modulation, &verdict, &holistic, &formal);
        assert_eq!(render_rule(result.rule), rule_g, "{tag} rule");
        assert_eq!(
            render_agreement(result.agreement),
            agree_g,
            "{tag} agreement"
        );
        if divcmp == "1" {
            // Exactly 0.0 on both twins (identical plans); parsed, since
            // `show`/`to_string` spell zero differently.
            assert_eq!(
                result.divergence,
                div_g.parse::<f64>().unwrap(),
                "{tag} divergence"
            );
        }
        if input[9] != "gate" {
            assert_eq!(format!("{:?}", result.plan.family), fam_g, "{tag} family");
            assert_eq!(
                result.plan.confidence.to_string(),
                conf_g,
                "{tag} confidence"
            );
            assert_eq!(
                render_recovery(&result.plan.recovery_cause),
                rec_g,
                "{tag} recovery"
            );
        }
        cases += 1;
    }
    assert_eq!(cases, 8, "all golden cases ran");
}

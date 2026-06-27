//! Deliberation framework — 6-rule priority-ordered reconciliation of
//! Holistic and Formal proposals into a unified Plan.
//!
//! Ported from Haskell QxFx0.Self.Deliberation (ADR-0011).

use qxfx0_types::field::Field;
use qxfx0_types::CanonicalMoveFamily;
use serde::{Deserialize, Serialize};

/// A composite proposal from one hemisphere.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Plan {
    pub family: CanonicalMoveFamily,
    pub holistic_dominant: bool,
    pub recovery_cause: Option<String>,
    pub confidence: f64,
}

impl Default for Plan {
    fn default() -> Self {
        Plan {
            family: CanonicalMoveFamily::CMReflect,
            holistic_dominant: false,
            recovery_cause: None,
            confidence: 0.5,
        }
    }
}

/// Salience driver — what triggered the verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SalienceDriver {
    DrivenByConatusGate,
    DrivenByField,
    DrivenBySalienceDefault,
}

/// Agreement level between the two proposals.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Agreement {
    FullAgreement,
    PartialAgreement,
    NoAgreement,
}

/// Reconciliation rule applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconcileRule {
    RuleConatusOverride,
    RuleAgreement,
    RuleSalienceLead,
    RuleHolisticAdvantage,
    RuleFormalAdvantage,
    RuleTiedFallback,
}

/// Structured trace of the deliberation process.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliberationTrace {
    pub salience_driver: SalienceDriver,
    pub rule: ReconcileRule,
    pub agreement: Agreement,
    pub divergence: f64,
}

/// The full deliberation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Deliberation {
    pub plan: Plan,
    pub trace: DeliberationTrace,
}

impl Default for Deliberation {
    fn default() -> Self {
        Deliberation {
            plan: Plan::default(),
            trace: DeliberationTrace {
                salience_driver: SalienceDriver::DrivenBySalienceDefault,
                rule: ReconcileRule::RuleTiedFallback,
                agreement: Agreement::NoAgreement,
                divergence: 0.0,
            },
        }
    }
}

/// Tunable modulation parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeliberationModulation {
    pub escalation_confidence_floor: f64,
    pub conatus_gate_threshold: f64,
    pub divergence_deadband: f64,
}

impl Default for DeliberationModulation {
    fn default() -> Self {
        DeliberationModulation {
            escalation_confidence_floor: 0.7,
            conatus_gate_threshold: 0.3,
            divergence_deadband: 0.1,
        }
    }
}

/// Reconcile two proposals into a single Plan using 6-rule priority ordering.
///
/// Rules in priority:
/// 1. RuleConatusOverride — conatus gate fires → formal forced to recovery
/// 2. RuleAgreement — proposals match → merge with max confidence
/// 3. RuleSalienceLead — high-confidence salience → verdict side wins wholesale
/// 4. RuleHolisticAdvantage / RuleFormalAdvantage — single-axis difference
/// 5. RuleTiedFallback — formal wins as safe default
pub fn reconcile(
    modulation: &DeliberationModulation,
    holistic: &Plan,
    formal: &Plan,
    _field: &Field,
    conatus_energy: f64,
    salience_bias: f64,
    holistic_dominant: bool,
) -> Deliberation {
    // Rule 1: Conatus override
    if conatus_energy < modulation.conatus_gate_threshold {
        return Deliberation {
            plan: Plan {
                family: CanonicalMoveFamily::CMRepair,
                holistic_dominant: false,
                recovery_cause: Some("conatus_gate_fired".into()),
                confidence: 1.0,
            },
            trace: DeliberationTrace {
                salience_driver: SalienceDriver::DrivenByConatusGate,
                rule: ReconcileRule::RuleConatusOverride,
                agreement: Agreement::NoAgreement,
                divergence: 1.0,
            },
        };
    }

    // Rule 2: Agreement — both proposals have same family
    let families_match = holistic.family == formal.family;
    let divergence = if families_match { 0.0 } else { 0.5 };

    if families_match {
        let merged_confidence = holistic.confidence.max(formal.confidence);
        return Deliberation {
            plan: Plan {
                family: holistic.family,
                holistic_dominant,
                recovery_cause: holistic.recovery_cause.clone(),
                confidence: merged_confidence,
            },
            trace: DeliberationTrace {
                salience_driver: SalienceDriver::DrivenByField,
                rule: ReconcileRule::RuleAgreement,
                agreement: Agreement::FullAgreement,
                divergence: 0.0,
            },
        };
    }

    // Rule 3: Salience lead — high confidence → verdict side wins
    if salience_bias.abs() > modulation.escalation_confidence_floor {
        let winner = if holistic_dominant {
            holistic.clone()
        } else {
            formal.clone()
        };
        return Deliberation {
            plan: winner,
            trace: DeliberationTrace {
                salience_driver: SalienceDriver::DrivenByField,
                rule: ReconcileRule::RuleSalienceLead,
                agreement: Agreement::PartialAgreement,
                divergence,
            },
        };
    }

    // Rules 4/5/6: Single-axis advantage or default
    if holistic_dominant {
        return Deliberation {
            plan: holistic.clone(),
            trace: DeliberationTrace {
                salience_driver: SalienceDriver::DrivenByField,
                rule: ReconcileRule::RuleHolisticAdvantage,
                agreement: Agreement::PartialAgreement,
                divergence,
            },
        };
    }

    // Rule 5/6: Formal advantage (non-holistic) or tied fallback.
    // When salience is effectively zero the proposals are tied → formal wins as
    // the safe default (RuleTiedFallback).  Otherwise formal has an advantage.
    let rule = if salience_bias.abs() < f64::EPSILON {
        ReconcileRule::RuleTiedFallback
    } else {
        ReconcileRule::RuleFormalAdvantage
    };
    Deliberation {
        plan: formal.clone(),
        trace: DeliberationTrace {
            salience_driver: SalienceDriver::DrivenBySalienceDefault,
            rule,
            agreement: Agreement::PartialAgreement,
            divergence,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_types::field::Field;

    #[test]
    fn test_conatus_override_when_low_energy() {
        let modln = DeliberationModulation::default();
        let holistic = Plan::default();
        let formal = Plan::default();
        let field = Field::default();
        // Conatus 0.1 is below the gate threshold (0.3) → RuleConatusOverride fires.
        let result = reconcile(&modln, &holistic, &formal, &field, 0.1, 0.5, false);
        assert_eq!(result.trace.rule, ReconcileRule::RuleConatusOverride);
        assert_eq!(result.plan.family, CanonicalMoveFamily::CMRepair);
    }

    #[test]
    fn test_conatus_above_gate_uses_rules() {
        // Conatus 1.0 is above the gate threshold (0.3) → deliberation rules apply.
        let modln = DeliberationModulation::default();
        let holistic = Plan::default();
        let formal = Plan::default();
        let field = Field::default();
        let result = reconcile(&modln, &holistic, &formal, &field, 1.0, 0.5, false);
        assert_ne!(result.trace.rule, ReconcileRule::RuleConatusOverride);
    }

    #[test]
    fn test_agreement_when_families_match() {
        let modln = DeliberationModulation::default();
        let plan = Plan {
            family: CanonicalMoveFamily::CMDefine,
            confidence: 0.8,
            ..Default::default()
        };
        let field = Field::default();
        let result = reconcile(&modln, &plan, &plan, &field, 10.0, 0.5, false);
        assert_eq!(result.trace.rule, ReconcileRule::RuleAgreement);
        assert_eq!(result.trace.agreement, Agreement::FullAgreement);
    }

    #[test]
    fn test_salience_lead_when_high_confidence() {
        let modln = DeliberationModulation::default();
        let holistic = Plan {
            family: CanonicalMoveFamily::CMHypothesis,
            confidence: 0.9,
            ..Default::default()
        };
        let formal = Plan {
            family: CanonicalMoveFamily::CMDefine,
            confidence: 0.5,
            ..Default::default()
        };
        let field = Field::default();
        let result = reconcile(&modln, &holistic, &formal, &field, 10.0, 0.8, true);
        assert_eq!(result.trace.rule, ReconcileRule::RuleSalienceLead);
        assert_eq!(result.plan.family, CanonicalMoveFamily::CMHypothesis);
    }

    #[test]
    fn test_formal_advantage_default() {
        let modln = DeliberationModulation::default();
        let holistic = Plan {
            family: CanonicalMoveFamily::CMHypothesis,
            confidence: 0.6,
            ..Default::default()
        };
        let formal = Plan {
            family: CanonicalMoveFamily::CMDefine,
            confidence: 0.5,
            ..Default::default()
        };
        let field = Field::default();
        let result = reconcile(&modln, &holistic, &formal, &field, 10.0, 0.3, false);
        assert_eq!(result.trace.rule, ReconcileRule::RuleFormalAdvantage);
    }

    #[test]
    fn test_tied_fallback() {
        let modln = DeliberationModulation::default();
        let holistic = Plan {
            family: CanonicalMoveFamily::CMHypothesis,
            confidence: 0.5,
            ..Default::default()
        };
        let formal = Plan {
            family: CanonicalMoveFamily::CMDefine,
            confidence: 0.5,
            ..Default::default()
        };
        let field = Field::default();
        let result = reconcile(&modln, &holistic, &formal, &field, 10.0, 0.0, false);
        assert_eq!(result.trace.rule, ReconcileRule::RuleTiedFallback);
    }

    #[test]
    fn test_holistic_advantage() {
        let modln = DeliberationModulation::default();
        let holistic = Plan {
            family: CanonicalMoveFamily::CMReflect,
            confidence: 0.6,
            ..Default::default()
        };
        let formal = Plan {
            family: CanonicalMoveFamily::CMDefine,
            confidence: 0.5,
            ..Default::default()
        };
        let field = Field::default();
        let result = reconcile(&modln, &holistic, &formal, &field, 10.0, 0.4, true);
        assert_eq!(result.trace.rule, ReconcileRule::RuleHolisticAdvantage);
    }
}

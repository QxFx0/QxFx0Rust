//! Deliberation — the canonical six-rule reconciliation of the two
//! hemispheric proposals into one outgoing plan (ADR-0043 U3, ported
//! from Haskell `QxFx0.Self.Deliberation`, Phase-8 there).
//!
//! ADR-0043 U3 pins this as the replacement for the route's priority
//! switching: instead of the salience verdict picking one value to
//! forward, both proposals are always materialised and a typed ladder
//! reconciles them — rule by rule, first match wins:
//!
//! 1. `RuleConatusOverride` — the canonical Conatus gate fired: the
//!    formal proposal is forced into the recovery shape (CMRepair,
//!    confidence 1.0) regardless of the holistic proposal.
//! 2. `RuleAgreement` — the proposals match on every carried axis:
//!    merge with the max of the two confidences.
//! 3. `RuleSalienceLead` — the controller's confidence exceeds the
//!    escalation floor: the verdict side wins wholesale.
//! 4. `RuleHolisticAdvantage` / `RuleFormalAdvantage` — exactly one
//!    non-recovery axis differs: the verdict side decides it.
//! 5. `RuleTiedFallback` — the formal proposal wins as the safe
//!    default (ADR-0010 §5 single-output discipline).
//!
//! The Haskell Plan carries four axes (family / render style / recovery
//! cause / narrative tone); the Rust plan vocabulary carries family,
//! recovery cause and confidence, so the differ classification and the
//! divergence denominator use the two non-confidence axes (4 → 2 in
//! lockstep with the record). Tone/style land when the plan vocabulary
//! grows — the same documented deferral as the essence plan validation.
//!
//! The reconciliation is keyed on the canonical [`SelfVerdictV2`] from
//! [`crate::salience`], not on the V1 `salience > 0.5` heuristic. V1
//! `qxfx0_self::deliberation` is untouched and remains the authority the
//! pipeline routes on; this module is shadow evidence until a separate
//! release flips dispatch.

use serde::{Deserialize, Serialize};

use qxfx0_types::field::Field;
use qxfx0_types::CanonicalMoveFamily;

use crate::salience::{Hemisphere, SalienceVerdictV2, V2SalienceDriver};

/// The reconcilable payload of one hemisphere. The confidence axis is
/// excluded from equality/differ classification (it is merged, never
/// disputed).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlanV2 {
    pub family: CanonicalMoveFamily,
    pub recovery_cause: Option<String>,
    pub confidence: f64,
}

/// Tunable thresholds of the reconciliation ladder. The escalation floor
/// matches the Haskell `defaultSalienceModulation` (0.7), which is what
/// the Phase-8 `reconcile` consumes from the salience layer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct DeliberationModulationV2 {
    pub escalation_confidence_floor: f64,
}

/// The reference builtin modulation (`defaultDeliberationModulation`).
pub const BUILTIN_DELIBERATION_MODULATION: DeliberationModulationV2 = DeliberationModulationV2 {
    escalation_confidence_floor: 0.7,
};

impl Default for DeliberationModulationV2 {
    fn default() -> Self {
        BUILTIN_DELIBERATION_MODULATION
    }
}

/// Agreement level between the two proposals over the axes the Rust
/// plan vocabulary carries (family, recovery cause).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AgreementV2 {
    Agree,
    DivergeOnFamily,
    DivergeOnRecovery,
    DivergeMultiple,
}

/// Stable snake_case tags; any change breaks the replay-trace schema.
pub fn render_agreement_v2(agreement: AgreementV2) -> &'static str {
    match agreement {
        AgreementV2::Agree => "agree",
        AgreementV2::DivergeOnFamily => "diverge_on_family",
        AgreementV2::DivergeOnRecovery => "diverge_on_recovery",
        AgreementV2::DivergeMultiple => "diverge_multiple",
    }
}

/// Which rule of the ladder fired.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReconcileRuleV2 {
    ConatusOverride,
    Agreement,
    SalienceLead,
    HolisticAdvantage,
    FormalAdvantage,
    TiedFallback,
}

/// Stable snake_case tags, mirroring `renderReconcileRule`.
pub fn render_reconcile_rule_v2(rule: ReconcileRuleV2) -> &'static str {
    match rule {
        ReconcileRuleV2::ConatusOverride => "conatus_override",
        ReconcileRuleV2::Agreement => "agreement",
        ReconcileRuleV2::SalienceLead => "salience_lead",
        ReconcileRuleV2::HolisticAdvantage => "holistic_advantage",
        ReconcileRuleV2::FormalAdvantage => "formal_advantage",
        ReconcileRuleV2::TiedFallback => "tied_fallback",
    }
}

/// Severity of a recovery cause (`recoveryCauseSeverity`). The Rust plan
/// vocabulary carries string causes; the canonical gate cause keeps the
/// Haskell top rung, unknown causes rank above "no recovery" so a
/// recovery is never silenced by a winning proposal that lacks one.
fn recovery_cause_severity(cause: &Option<String>) -> i32 {
    match cause.as_deref() {
        None => 0,
        Some("conatus_gate_fired") => 100,
        Some(_) => 50,
    }
}

fn pick_higher_severity(left: &Option<String>, right: &Option<String>) -> Option<String> {
    if recovery_cause_severity(left) >= recovery_cause_severity(right) {
        left.clone()
    } else {
        right.clone()
    }
}

/// Do the two proposals agree on every non-confidence axis?
pub fn plans_equal_mod_confidence(left: &PlanV2, right: &PlanV2) -> bool {
    left.family == right.family && left.recovery_cause == right.recovery_cause
}

/// Classify agreement over the carried axes (family, recovery).
pub fn classify_agreement(left: &PlanV2, right: &PlanV2) -> AgreementV2 {
    let family_differs = left.family != right.family;
    let recovery_differs = left.recovery_cause != right.recovery_cause;
    match (family_differs, recovery_differs) {
        (false, false) => AgreementV2::Agree,
        (true, false) => AgreementV2::DivergeOnFamily,
        (false, true) => AgreementV2::DivergeOnRecovery,
        (true, true) => AgreementV2::DivergeMultiple,
    }
}

/// Differing axes / axis count. Always in `[0, 1]`. The divisor is two
/// for the Rust plan vocabulary — extend in lockstep with the record.
pub fn compute_divergence(left: &PlanV2, right: &PlanV2) -> f64 {
    let differs = usize::from(left.family != right.family)
        + usize::from(left.recovery_cause != right.recovery_cause);
    differs as f64 / 2.0
}

/// The full result of one reconciliation — the outgoing plan plus the
/// structured trace of how it was chosen.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliberationV2 {
    pub plan: PlanV2,
    pub rule: ReconcileRuleV2,
    pub agreement: AgreementV2,
    pub divergence: f64,
}

/// Build the canonical hemispheric proposals from the affective field.
/// The editorial content mirrors V1's prepare-stage proposal shapes
/// (holistic leans on resonance/counterfactual, formal on
/// confidence/consolidation); the reconciliation algebra below is the
/// canonical part. The proposals are total: non-finite components read
/// as 0.0 rather than poisoning the confidences.
pub fn proposal_pair_from_field(field: &Field) -> (PlanV2, PlanV2) {
    let term = |value: f64| if value.is_finite() { value } else { 0.0 };
    let holistic = PlanV2 {
        family: if term(field.resonance) + term(field.counterfactual)
            >= term(field.confidence) + term(field.consolidation)
        {
            CanonicalMoveFamily::CMReflect
        } else {
            CanonicalMoveFamily::CMGround
        },
        recovery_cause: None,
        confidence: (0.6 * term(field.resonance) + 0.4 * term(field.counterfactual))
            .clamp(0.0, 1.0),
    };
    let formal = PlanV2 {
        family: CanonicalMoveFamily::CMDefine,
        recovery_cause: None,
        confidence: (0.7 * term(field.confidence) + 0.3 * term(field.consolidation))
            .clamp(0.0, 1.0),
    };
    (holistic, formal)
}

/// The deliberation morphism: `reconcile :: Salience -> HolisticPlan ->
/// FormalPlan -> Deliberation`. Pure, total, deterministic; identical
/// inputs produce identical verdicts including the rule tag.
pub fn reconcile(
    modulation: &DeliberationModulationV2,
    verdict: &SalienceVerdictV2,
    holistic: &PlanV2,
    formal: &PlanV2,
) -> DeliberationV2 {
    let agreement = classify_agreement(holistic, formal);
    let divergence = compute_divergence(holistic, formal);
    let merged_recovery = pick_higher_severity(&holistic.recovery_cause, &formal.recovery_cause);
    let finish = |plan: PlanV2, rule: ReconcileRuleV2| DeliberationV2 {
        plan,
        rule,
        agreement,
        divergence,
    };

    // Rule 1: the canonical Conatus gate has uncontested priority.
    if verdict.driver == V2SalienceDriver::ConatusGate {
        return finish(
            PlanV2 {
                family: CanonicalMoveFamily::CMRepair,
                recovery_cause: Some("conatus_gate_fired".into()),
                confidence: 1.0,
            },
            ReconcileRuleV2::ConatusOverride,
        );
    }

    let lead_holistic = match hemisphere_of(verdict) {
        Hemisphere::PreferHolistic(_) => true,
        // Tied dispatches formal-first: the single-output discipline.
        Hemisphere::PreferFormal(_) | Hemisphere::Tied => false,
    };

    // Rule 2: agreement merges with the max confidence.
    if plans_equal_mod_confidence(holistic, formal) {
        let merged = PlanV2 {
            family: holistic.family,
            recovery_cause: merged_recovery,
            confidence: holistic.confidence.max(formal.confidence),
        };
        return finish(merged, ReconcileRuleV2::Agreement);
    }

    // Rule 3: a confident controller verdict carries the whole plan.
    if verdict.confidence > modulation.escalation_confidence_floor {
        let winner = if lead_holistic { holistic } else { formal };
        let chosen = PlanV2 {
            recovery_cause: merged_recovery.clone(),
            ..winner.clone()
        };
        return finish(chosen, ReconcileRuleV2::SalienceLead);
    }

    // Rule 4: exactly one non-recovery axis differs — with the Rust plan
    // vocabulary that is the family axis (recovery never counts: it is
    // always merged via severity, never silenced). The verdict side wins
    // it; Tied dispatches formal.
    if holistic.family != formal.family {
        let winner = if lead_holistic { holistic } else { formal };
        let chosen = PlanV2 {
            recovery_cause: merged_recovery,
            ..winner.clone()
        };
        let rule = if lead_holistic {
            ReconcileRuleV2::HolisticAdvantage
        } else {
            ReconcileRuleV2::FormalAdvantage
        };
        return finish(chosen, rule);
    }

    // Rule 5: tied fallback — formal wins as the safe default. Reached
    // when the proposals agree on family and differ only on recovery.
    finish(
        PlanV2 {
            recovery_cause: merged_recovery,
            ..formal.clone()
        },
        ReconcileRuleV2::TiedFallback,
    )
}

/// The dispatched hemisphere of a verdict under the builtin weights —
/// exposed because the doubt loop and route flip consume the dispatch,
/// not the continuous bias.
fn hemisphere_of(verdict: &SalienceVerdictV2) -> Hemisphere {
    crate::salience::salience_hemisphere(crate::salience::SalienceWeightsV2::default(), *verdict)
}

/// The shadow surface the pipeline records: the reconciled verdict plus
/// the doubt-loop escalation computed from the canonical controller
/// signals. Distinct from the V1 `DeliberationTrace` this rides next to:
/// V1 stays authority, this is the comparison evidence for the flip.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeliberationShadowTrace {
    /// The rule the canonical ladder applied to this turn's proposals.
    pub rule: ReconcileRuleV2,
    /// Agreement between the hemispheric proposals.
    pub agreement: AgreementV2,
    /// Axis divergence of the proposals.
    pub divergence: f64,
    /// The family V2 would route to.
    pub reconciled_family: CanonicalMoveFamily,
    /// The family actually routed to (V1 authority). Shadow comparison:
    /// `reconciled_family == applied_family` is the flip-readiness
    /// statistic over a trace corpus.
    pub applied_family: CanonicalMoveFamily,
    /// The doubt score computed from the canonical signals: the V2
    /// Conatus-gate floor overrides the confidence complement, exactly
    /// the Haskell doubt law keyed on the canonical gate.
    pub doubt_score: f64,
    /// True when the doubt loop would escalate this turn to CMClarify
    /// (score over the clarification threshold, no recent same-topic
    /// system decision suppressing it).
    pub doubt_escalates: bool,
}

/// Clarification threshold — the Haskell/V1 doubt policy default (0.75),
/// pinned not calibrated (calibration needs a trace corpus; ADR-0043).
pub const DOUBT_CLARIFICATION_THRESHOLD: f64 = 0.75;

/// Compute the shadow deliberation for one turn: reconcile the canonical
/// proposals and derive the doubt-loop escalation from the controller
/// verdict. `applied_family` is what V1 routed to (recorded for the
/// comparison, never overwritten). `same_topic_decision_confirmed`
/// suppresses re-asking exactly like V1's episodic policy: a confirmed
/// system decision on this topic in the immediate history means the
/// doubt loop retains instead of clarifying. Pure and total.
///
/// Map one canonical reconciliation onto the V1 working-layer shape
/// (ADR-0044 migration M2). The ladder's rule tags are 1:1 with V1's;
/// agreement collapses to the V1 three-way (`Agree` only on full
/// agreement — there is no V1 partial signal in the canonical tally);
/// divergence passes through (0/0.5/1.0 over the two carried axes, vs
/// V1's 0.0/0.5 — the V1 witness angst law keys on the same 0 and
/// ≥0.5 boundaries, so the dynamics direction is preserved);
/// the trace driver follows V1's own convention (only the override
/// carries the gate, everything else reads as field-driven).
/// Removed when the witness migrates to V2-native (M3).
pub fn v2_result_to_v1_deliberation(
    result: &DeliberationV2,
    driver: crate::salience::V2SalienceDriver,
    holistic_dominant: bool,
) -> qxfx0_self::deliberation::Deliberation {
    use qxfx0_self::deliberation::{
        Agreement as AgreementV1, Deliberation as DeliberationV1,
        DeliberationTrace as DeliberationTraceV1, Plan as PlanV1, ReconcileRule as ReconcileRuleV1,
        SalienceDriver as SalienceDriverV1,
    };
    let rule = match result.rule {
        ReconcileRuleV2::ConatusOverride => ReconcileRuleV1::RuleConatusOverride,
        ReconcileRuleV2::Agreement => ReconcileRuleV1::RuleAgreement,
        ReconcileRuleV2::SalienceLead => ReconcileRuleV1::RuleSalienceLead,
        ReconcileRuleV2::HolisticAdvantage => ReconcileRuleV1::RuleHolisticAdvantage,
        ReconcileRuleV2::FormalAdvantage => ReconcileRuleV1::RuleFormalAdvantage,
        ReconcileRuleV2::TiedFallback => ReconcileRuleV1::RuleTiedFallback,
    };
    let agreement = match result.agreement {
        AgreementV2::Agree => AgreementV1::FullAgreement,
        AgreementV2::DivergeOnFamily
        | AgreementV2::DivergeOnRecovery
        | AgreementV2::DivergeMultiple => AgreementV1::NoAgreement,
    };
    let override_fired = matches!(result.rule, ReconcileRuleV2::ConatusOverride);
    DeliberationV1 {
        plan: PlanV1 {
            family: result.plan.family,
            holistic_dominant: if override_fired {
                false
            } else {
                holistic_dominant
            },
            recovery_cause: result.plan.recovery_cause.clone(),
            confidence: result.plan.confidence,
        },
        trace: DeliberationTraceV1 {
            salience_driver: match driver {
                crate::salience::V2SalienceDriver::ConatusGate => {
                    SalienceDriverV1::DrivenByConatusGate
                }
                _ => SalienceDriverV1::DrivenByField,
            },
            rule,
            agreement,
            divergence: result.divergence,
        },
    }
}
pub fn deliberate_shadow(
    verdict: &SalienceVerdictV2,
    field: &Field,
    applied_family: CanonicalMoveFamily,
    same_topic_decision_confirmed: bool,
) -> DeliberationShadowTrace {
    let modulation = DeliberationModulationV2::default();
    let (holistic, formal) = proposal_pair_from_field(field);
    let deliberation = reconcile(&modulation, verdict, &holistic, &formal);
    // Doubt law (Haskell parity): complement of confidence; the canonical
    // Conatus gate is a structural high-doubt floor of 0.9; a
    // counterfactual-led controller adds 0.2 of ambiguity.
    let confidence = if field.confidence.is_finite() {
        field.confidence.clamp(0.0, 1.0)
    } else {
        0.0
    };
    let base = 1.0 - confidence;
    let score = match verdict.driver {
        V2SalienceDriver::ConatusGate => base.max(0.9),
        V2SalienceDriver::Counterfactual | V2SalienceDriver::ContentSaliency => {
            (base + 0.2).min(1.0)
        }
        _ => base,
    };
    let doubt_escalates = score >= DOUBT_CLARIFICATION_THRESHOLD && !same_topic_decision_confirmed;
    DeliberationShadowTrace {
        rule: deliberation.rule,
        agreement: deliberation.agreement,
        divergence: deliberation.divergence,
        reconciled_family: if doubt_escalates {
            CanonicalMoveFamily::CMClarify
        } else {
            deliberation.plan.family
        },
        applied_family,
        doubt_score: score,
        doubt_escalates,
    }
}

/// Structural invariant checks for `doctor`.
pub fn validate_deliberation_invariants() -> Vec<String> {
    let mut violations = Vec::new();
    let modulation = DeliberationModulationV2::default();
    if !modulation.escalation_confidence_floor.is_finite()
        || !(0.0..=1.0).contains(&modulation.escalation_confidence_floor)
    {
        violations.push(format!(
            "deliberation-v2 escalation_confidence_floor out of [0,1]: {}",
            modulation.escalation_confidence_floor
        ));
    }
    if !(0.0..=1.0).contains(&DOUBT_CLARIFICATION_THRESHOLD) {
        violations.push("deliberation-v2 doubt threshold out of [0,1]".into());
    }
    // The safe-default law: on the mid field the ladder must be total and
    // the Tied dispatch must never pick holistic.
    let field = Field::default();
    let (holistic, formal) = proposal_pair_from_field(&field);
    let tied_verdict = SalienceVerdictV2 {
        holistic_bias: 0.5,
        confidence: 1.0,
        driver: V2SalienceDriver::Default,
    };
    let deliberation = reconcile(&modulation, &tied_verdict, &holistic, &formal);
    if deliberation.plan.family != formal.family && deliberation.plan.family != holistic.family {
        violations.push("deliberation-v2 tied fallback produced an off-proposal family".into());
    }
    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    fn verdict(bias: f64, confidence: f64, driver: V2SalienceDriver) -> SalienceVerdictV2 {
        SalienceVerdictV2 {
            holistic_bias: bias,
            confidence,
            driver,
        }
    }

    fn plan(family: CanonicalMoveFamily, confidence: f64) -> PlanV2 {
        PlanV2 {
            family,
            recovery_cause: None,
            confidence,
        }
    }

    #[test]
    fn conatus_gate_overrides_everything() {
        let holistic = plan(CanonicalMoveFamily::CMReflect, 0.9);
        let formal = plan(CanonicalMoveFamily::CMDefine, 0.9);
        let gated = verdict(0.0, 1.0, V2SalienceDriver::ConatusGate);
        let result = reconcile(
            &DeliberationModulationV2::default(),
            &gated,
            &holistic,
            &formal,
        );
        assert_eq!(result.rule, ReconcileRuleV2::ConatusOverride);
        assert_eq!(result.plan.family, CanonicalMoveFamily::CMRepair);
        assert_eq!(
            result.plan.recovery_cause.as_deref(),
            Some("conatus_gate_fired")
        );
        assert_eq!(result.plan.confidence, 1.0);
    }

    #[test]
    fn agreement_merges_with_max_confidence() {
        let left = plan(CanonicalMoveFamily::CMDefine, 0.4);
        let right = plan(CanonicalMoveFamily::CMDefine, 0.8);
        let result = reconcile(
            &DeliberationModulationV2::default(),
            &verdict(0.5, 0.5, V2SalienceDriver::Resonance),
            &left,
            &right,
        );
        assert_eq!(result.rule, ReconcileRuleV2::Agreement);
        assert_eq!(result.agreement, AgreementV2::Agree);
        assert_eq!(result.divergence, 0.0);
        assert_eq!(result.plan.confidence, 0.8);
    }

    #[test]
    fn confident_verdict_leads_wholesale() {
        let holistic = plan(CanonicalMoveFamily::CMReflect, 0.3);
        let formal = plan(CanonicalMoveFamily::CMDefine, 0.9);
        let result = reconcile(
            &DeliberationModulationV2::default(),
            &verdict(0.8, 0.95, V2SalienceDriver::Resonance),
            &holistic,
            &formal,
        );
        assert_eq!(result.rule, ReconcileRuleV2::SalienceLead);
        assert_eq!(result.plan.family, CanonicalMoveFamily::CMReflect);
        // The verdict side wins wholesale: its confidence rides, only the
        // recovery cause merges (here both None).
        assert_eq!(result.plan.confidence, 0.3);
        assert_eq!(result.plan.recovery_cause, None);
    }

    #[test]
    fn single_axis_divergence_uses_the_verdict_side_below_the_floor() {
        let holistic = plan(CanonicalMoveFamily::CMReflect, 0.3);
        let formal = plan(CanonicalMoveFamily::CMDefine, 0.9);
        let modulation = DeliberationModulationV2::default();
        let holistic_led = reconcile(
            &modulation,
            &verdict(0.8, 0.5, V2SalienceDriver::Resonance),
            &holistic,
            &formal,
        );
        assert_eq!(holistic_led.rule, ReconcileRuleV2::HolisticAdvantage);
        assert_eq!(holistic_led.plan.family, CanonicalMoveFamily::CMReflect);
        let formal_led = reconcile(
            &modulation,
            &verdict(0.2, 0.5, V2SalienceDriver::Consolidation),
            &holistic,
            &formal,
        );
        assert_eq!(formal_led.rule, ReconcileRuleV2::FormalAdvantage);
        assert_eq!(formal_led.plan.family, CanonicalMoveFamily::CMDefine);
        assert_eq!(formal_led.divergence, 0.5);
    }

    #[test]
    fn tied_dispatch_wins_formal_as_safe_default() {
        let holistic = plan(CanonicalMoveFamily::CMReflect, 0.3);
        let formal = plan(CanonicalMoveFamily::CMDefine, 0.9);
        let result = reconcile(
            &DeliberationModulationV2::default(),
            &verdict(0.5, 0.5, V2SalienceDriver::Default),
            &holistic,
            &formal,
        );
        assert_eq!(result.rule, ReconcileRuleV2::FormalAdvantage);
        assert_eq!(result.plan.family, CanonicalMoveFamily::CMDefine);
        // Exactly at the escalation floor the lead rule does NOT fire
        // (strict >), so the single-axis rule takes it — still formal.
    }

    #[test]
    fn recovery_is_never_silenced() {
        let holistic = PlanV2 {
            family: CanonicalMoveFamily::CMReflect,
            recovery_cause: None,
            confidence: 0.9,
        };
        let formal = PlanV2 {
            family: CanonicalMoveFamily::CMReflect,
            recovery_cause: Some("shadow_divergence".into()),
            confidence: 0.2,
        };
        let result = reconcile(
            &DeliberationModulationV2::default(),
            &verdict(0.9, 0.95, V2SalienceDriver::Resonance),
            &holistic,
            &formal,
        );
        // Agreement on family: recovery diverges → not equal-mod-confidence,
        // lead rule fires (confident holistic), yet the merged recovery rides.
        assert_eq!(result.rule, ReconcileRuleV2::SalienceLead);
        assert_eq!(
            result.plan.recovery_cause.as_deref(),
            Some("shadow_divergence")
        );
        assert_eq!(result.agreement, AgreementV2::DivergeOnRecovery);
    }

    #[test]
    fn severity_ladder_keeps_the_gate_on_top() {
        assert_eq!(recovery_cause_severity(&None), 0);
        assert_eq!(
            recovery_cause_severity(&Some("shadow_divergence".into())),
            50
        );
        assert_eq!(
            recovery_cause_severity(&Some("conatus_gate_fired".into())),
            100
        );
        assert_eq!(
            pick_higher_severity(&Some("conatus_gate_fired".into()), &None).as_deref(),
            Some("conatus_gate_fired")
        );
        // Ties favour the left argument, matching the Haskell fold.
        assert_eq!(
            pick_higher_severity(&Some("a".into()), &Some("b".into())).as_deref(),
            Some("a")
        );
    }

    #[test]
    fn proposals_are_total_and_range_bounded() {
        let field = Field {
            resonance: f64::NAN,
            confidence: f64::INFINITY,
            consolidation: 0.5,
            counterfactual: -2.0,
            atmosphere: qxfx0_types::field::Atmosphere::new(0.0, 0.4),
        };
        let (holistic, formal) = proposal_pair_from_field(&field);
        assert!((0.0..=1.0).contains(&holistic.confidence));
        assert!((0.0..=1.0).contains(&formal.confidence));
        // Non-finite reads as zero: counterfactual -2.0 clamps to its
        // magnitude only through the proposal formulas, never NaN.
        let verdict = verdict(0.4, 0.4, V2SalienceDriver::Consolidation);
        let result = reconcile(
            &DeliberationModulationV2::default(),
            &verdict,
            &holistic,
            &formal,
        );
        assert!(result.plan.confidence.is_finite());
    }

    #[test]
    fn shadow_trace_records_applied_vs_reconciled_and_doubt_floor() {
        // Healthy field, default controller: no escalation, comparison
        // fields carried verbatim.
        let trace = deliberate_shadow(
            &verdict(0.6, 0.5, V2SalienceDriver::Resonance),
            &Field::default(),
            CanonicalMoveFamily::CMGround,
            false,
        );
        assert!(!trace.doubt_escalates);
        assert_eq!(trace.applied_family, CanonicalMoveFamily::CMGround);
        // The gate driver floors doubt at 0.9 ≥ 0.75: escalation, and the
        // reconciled family becomes CMClarify even though rule 1 produced
        // CMRepair — the doubt loop reads the canonical gate too.
        let gated = deliberate_shadow(
            &verdict(0.0, 1.0, V2SalienceDriver::ConatusGate),
            &Field::default(),
            CanonicalMoveFamily::CMGround,
            false,
        );
        assert_eq!(gated.doubt_score, 0.9);
        assert!(gated.doubt_escalates);
        assert_eq!(gated.reconciled_family, CanonicalMoveFamily::CMClarify);
        // A confirmed same-topic decision suppresses the ask.
        let suppressed = deliberate_shadow(
            &verdict(0.0, 1.0, V2SalienceDriver::ConatusGate),
            &Field::default(),
            CanonicalMoveFamily::CMGround,
            true,
        );
        assert!(!suppressed.doubt_escalates);
        assert_eq!(suppressed.reconciled_family, CanonicalMoveFamily::CMRepair);
    }

    #[test]
    fn counterfactual_driver_adds_ambiguity_to_doubt() {
        let low_confidence = Field {
            confidence: 0.3,
            ..Field::default()
        };
        let plain = deliberate_shadow(
            &verdict(0.4, 0.4, V2SalienceDriver::Resonance),
            &low_confidence,
            CanonicalMoveFamily::CMGround,
            false,
        );
        let ambiguous = deliberate_shadow(
            &verdict(0.4, 0.4, V2SalienceDriver::Counterfactual),
            &low_confidence,
            CanonicalMoveFamily::CMGround,
            false,
        );
        assert!((plain.doubt_score - 0.7).abs() < 1e-12);
        assert!((ambiguous.doubt_score - 0.9).abs() < 1e-12);
        assert!(!plain.doubt_escalates);
        assert!(ambiguous.doubt_escalates);
    }

    #[test]
    fn reconciliation_is_deterministic() {
        let field = Field {
            resonance: 0.8,
            confidence: 0.2,
            consolidation: 0.3,
            counterfactual: 0.7,
            atmosphere: qxfx0_types::field::Atmosphere::new(0.1, 0.6),
        };
        let (holistic, formal) = proposal_pair_from_field(&field);
        let verdict = verdict(0.7, 0.8, V2SalienceDriver::Resonance);
        let first = reconcile(
            &DeliberationModulationV2::default(),
            &verdict,
            &holistic,
            &formal,
        );
        let second = reconcile(
            &DeliberationModulationV2::default(),
            &verdict,
            &holistic,
            &formal,
        );
        assert_eq!(first, second);
    }

    #[test]
    fn invariants_hold_on_the_builtins() {
        assert!(validate_deliberation_invariants().is_empty());
    }

    #[test]
    fn v1_mapping_covers_every_rule_and_agreement() {
        use qxfx0_self::deliberation::{Agreement, ReconcileRule};
        let map_rule = |rule: ReconcileRuleV2| {
            v2_result_to_v1_deliberation(
                &DeliberationV2 {
                    plan: PlanV2 {
                        family: CanonicalMoveFamily::CMReflect,
                        recovery_cause: None,
                        confidence: 0.5,
                    },
                    rule,
                    agreement: AgreementV2::Agree,
                    divergence: 0.0,
                },
                V2SalienceDriver::Resonance,
                true,
            )
            .trace
            .rule
        };
        assert_eq!(
            map_rule(ReconcileRuleV2::ConatusOverride),
            ReconcileRule::RuleConatusOverride
        );
        assert_eq!(
            map_rule(ReconcileRuleV2::Agreement),
            ReconcileRule::RuleAgreement
        );
        assert_eq!(
            map_rule(ReconcileRuleV2::SalienceLead),
            ReconcileRule::RuleSalienceLead
        );
        assert_eq!(
            map_rule(ReconcileRuleV2::HolisticAdvantage),
            ReconcileRule::RuleHolisticAdvantage
        );
        assert_eq!(
            map_rule(ReconcileRuleV2::FormalAdvantage),
            ReconcileRule::RuleFormalAdvantage
        );
        assert_eq!(
            map_rule(ReconcileRuleV2::TiedFallback),
            ReconcileRule::RuleTiedFallback
        );

        let map_agreement = |agreement: AgreementV2| {
            v2_result_to_v1_deliberation(
                &DeliberationV2 {
                    plan: PlanV2 {
                        family: CanonicalMoveFamily::CMDefine,
                        recovery_cause: None,
                        confidence: 0.5,
                    },
                    rule: ReconcileRuleV2::TiedFallback,
                    agreement,
                    divergence: 0.5,
                },
                V2SalienceDriver::Atmosphere,
                false,
            )
            .trace
            .agreement
        };
        assert_eq!(map_agreement(AgreementV2::Agree), Agreement::FullAgreement);
        assert_eq!(
            map_agreement(AgreementV2::DivergeOnFamily),
            Agreement::NoAgreement
        );
        assert_eq!(
            map_agreement(AgreementV2::DivergeOnRecovery),
            Agreement::NoAgreement
        );
        assert_eq!(
            map_agreement(AgreementV2::DivergeMultiple),
            Agreement::NoAgreement
        );
    }

    #[test]
    fn v1_mapping_keeps_override_shape_and_divergence() {
        use qxfx0_self::deliberation::SalienceDriver;
        let mapped = v2_result_to_v1_deliberation(
            &DeliberationV2 {
                plan: PlanV2 {
                    family: CanonicalMoveFamily::CMRepair,
                    recovery_cause: Some("conatus_gate_fired".into()),
                    confidence: 1.0,
                },
                rule: ReconcileRuleV2::ConatusOverride,
                agreement: AgreementV2::Agree,
                divergence: 0.0,
            },
            V2SalienceDriver::ConatusGate,
            true,
        );
        assert!(!mapped.plan.holistic_dominant, "override forces formal");
        assert_eq!(
            mapped.trace.salience_driver,
            SalienceDriver::DrivenByConatusGate
        );
        assert_eq!(mapped.trace.divergence, 0.0);

        let mapped = v2_result_to_v1_deliberation(
            &DeliberationV2 {
                plan: PlanV2 {
                    family: CanonicalMoveFamily::CMDefine,
                    recovery_cause: None,
                    confidence: 0.7,
                },
                rule: ReconcileRuleV2::FormalAdvantage,
                agreement: AgreementV2::DivergeOnFamily,
                divergence: 0.5,
            },
            V2SalienceDriver::Resonance,
            false,
        );
        assert_eq!(mapped.trace.salience_driver, SalienceDriver::DrivenByField);
        assert_eq!(mapped.trace.divergence, 0.5, "divergence passes through");
        assert_eq!(mapped.plan.family, CanonicalMoveFamily::CMDefine);
        assert_eq!(mapped.plan.confidence, 0.7);
    }
}

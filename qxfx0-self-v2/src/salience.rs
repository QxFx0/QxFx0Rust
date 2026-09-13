//! Salience — the canonical controller verdict over Field × Conatus
//! (ADR-0043 U3, ported from Haskell `QxFx0.Self.Salience`, Phase 5 /
//! ADR-0010 there).
//!
//! The controller decides, per turn, which hemisphere should lead:
//! Holistic (right, generative) or Formal (left, constraint-respecting).
//! It is a pure morphism
//!
//! ```text
//! raw =  w_r·resonance + w_a·arousal − w_c·consolidation
//!      + w_f·counterfactual − w_fc·confidence + w_s·saliency
//! bias = sigmoid(raw / temperature)
//! ```
//!
//! with per-signal contributions carrying the verdict driver tag, and
//! confidence as `1 − normalised dispersion` of those contributions. The
//! Conatus gate has uncontested priority: below the gate threshold the
//! verdict short-circuits to formal with full confidence, and no
//! Field-derived signal contributes.
//!
//! The default weights are pinned to the Haskell builtins, not calibrated
//! against empirical ground truth — calibration stays deferred until a
//! trace corpus exists (ADR-0043: discipline, not debt). The V1
//! `qxfx0_self::Salience` computes an untyped scalar with different fixed
//! constants; V1 is untouched and remains what the pipeline routes on
//! today. This module is shadow evidence until a separate release flips
//! dispatch.

use serde::{Deserialize, Serialize};

use qxfx0_types::field::Field;
use qxfx0_types::CanonicalMoveFamily;

use crate::conatus::ConatusEnergy;

/// Closed enumeration of which input dominated the controller's decision.
/// Used in trace records; the snake_case tags are replay-schema-stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum V2SalienceDriver {
    Resonance,
    Atmosphere,
    Consolidation,
    Counterfactual,
    FieldConfidence,
    ConatusGate,
    ContentSaliency,
    Default,
}

/// Stable snake_case tag; any change is a breaking change to the
/// replay-trace JSON schema.
pub fn render_v2_salience_driver(driver: V2SalienceDriver) -> &'static str {
    match driver {
        V2SalienceDriver::Resonance => "resonance",
        V2SalienceDriver::Atmosphere => "atmosphere",
        V2SalienceDriver::Consolidation => "consolidation",
        V2SalienceDriver::Counterfactual => "counterfactual",
        V2SalienceDriver::FieldConfidence => "field_confidence",
        V2SalienceDriver::ConatusGate => "conatus_gate",
        V2SalienceDriver::ContentSaliency => "content_saliency",
        V2SalienceDriver::Default => "default",
    }
}

/// The controller's verdict for a single turn.
///
/// Invariants: `holistic_bias ∈ [0,1]`, `confidence ∈ [0,1]`, and
/// identical inputs produce identical verdicts including the driver tag.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SalienceVerdictV2 {
    pub holistic_bias: f64,
    pub confidence: f64,
    pub driver: V2SalienceDriver,
}

/// The dispatched form of a verdict: `Tied` falls inside the dead band
/// around 0.5 and dispatches to the formal branch (the safe default —
/// the anti-correlation discipline of ADR-0010 §5: one output channel).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Hemisphere {
    PreferHolistic(f64),
    PreferFormal(f64),
    Tied,
}

impl Hemisphere {
    pub fn leads_holistic(self) -> bool {
        matches!(self, Hemisphere::PreferHolistic(_))
    }
}

/// Tunable coefficients of the decision rule. Pinned to the Haskell
/// `builtinSalienceWeights`; explicitly *not* calibrated.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SalienceWeightsV2 {
    pub resonance: f64,
    pub atmosphere: f64,
    pub consolidation: f64,
    pub counterfactual: f64,
    pub field_confidence: f64,
    pub content_saliency: f64,
    /// `ceScalar` can be negative under heavy violation; the gate trips at 0.
    pub conatus_gate_threshold: f64,
    /// Dead-band half-width around 0.5 for the dispatch verdict.
    pub verdict_threshold: f64,
    pub sigmoid_temperature: f64,
}

/// The reference builtin weights (`QxFx0.Self.Salience.builtinSalienceWeights`).
pub const BUILTIN_SALIENCE_WEIGHTS: SalienceWeightsV2 = SalienceWeightsV2 {
    resonance: 1.0,
    atmosphere: 0.5,
    consolidation: 0.75,
    counterfactual: 0.75,
    field_confidence: 0.5,
    content_saliency: 0.6,
    conatus_gate_threshold: 0.0,
    verdict_threshold: 0.05,
    sigmoid_temperature: 1.0,
};

impl Default for SalienceWeightsV2 {
    fn default() -> Self {
        BUILTIN_SALIENCE_WEIGHTS
    }
}

/// Calibrated weights (density doctrine): the Haskell corpus-tuning
/// promotion (`resources/config/tuned_salience_weights.json` —
/// best-non-regressing grid candidate over adaptation signals
/// ±0.30/±0.15/0, evaluated on the corpus dataset). Deltas are
/// uniformly +0.003 (a holistic nudge that improved net score without
/// regressing), thresholds untouched. Adopted as the production source
/// with provenance instead of re-running the grid: the formula is
/// parity-pinned, only the coefficients travel. `Default` stays
/// builtin (stable, parity-pinned); production call sites opt into
/// `calibrated()` explicitly so the choice is reviewable per site.
pub const CALIBRATED_SALIENCE_WEIGHTS: SalienceWeightsV2 = SalienceWeightsV2 {
    resonance: 1.003,
    atmosphere: 0.503,
    consolidation: 0.753,
    counterfactual: 0.753,
    field_confidence: 0.503,
    content_saliency: 0.603,
    conatus_gate_threshold: 0.0,
    verdict_threshold: 0.05,
    sigmoid_temperature: 1.0,
};

impl SalienceWeightsV2 {
    /// Production weights: the calibrated set above.
    pub fn calibrated() -> Self {
        CALIBRATED_SALIENCE_WEIGHTS
    }
}

/// Per-driver signed contributions to the raw score. The sign matches the
/// rule direction (positive pushes toward Holistic); confidence and driver
/// attribution use absolute values.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SalienceContributions {
    pub resonance: f64,
    pub atmosphere: f64,
    pub consolidation: f64,
    pub counterfactual: f64,
    pub field_confidence: f64,
    pub content_saliency: f64,
}

/// The aggregated pre-turn self decision surface: the continuous verdict
/// plus its discrete dispatch, so consumers read one canonical shape
/// instead of recomputing and reclassifying locally.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SelfVerdictV2 {
    pub salience: SalienceVerdictV2,
    pub hemisphere: Hemisphere,
}

/// The signed contribution vector of a field under explicit weights.
/// Total: non-finite inputs read as 0.0 rather than poisoning the score.
pub fn contributions(
    weights: SalienceWeightsV2,
    field: &Field,
    content_saliency: f64,
) -> SalienceContributions {
    let term = |weight: f64, value: f64| weight * finite_or_zero(value);
    SalienceContributions {
        resonance: term(weights.resonance, field.resonance),
        atmosphere: term(weights.atmosphere, field.atmosphere.arousal),
        consolidation: term(weights.consolidation, field.consolidation),
        counterfactual: term(weights.counterfactual, field.counterfactual),
        field_confidence: term(weights.field_confidence, field.confidence),
        content_saliency: term(weights.content_saliency, content_saliency),
    }
}

/// Compute the salience verdict: total, deterministic, pure. The Conatus
/// gate fires unconditionally when the energy scalar is below the gate
/// threshold, short-circuiting to a formal verdict with full confidence.
/// `content_saliency` is the reserved top-down signal (spectral clustering
/// is deferred; pass 0.0 until it lands).
pub fn compute_salience(
    weights: SalienceWeightsV2,
    conatus: ConatusEnergy,
    field: &Field,
    content_saliency: f64,
) -> SalienceVerdictV2 {
    if finite_or_zero(conatus.scalar) < weights.conatus_gate_threshold {
        return SalienceVerdictV2 {
            holistic_bias: 0.0,
            confidence: 1.0,
            driver: V2SalienceDriver::ConatusGate,
        };
    }
    let cs = contributions(weights, field, content_saliency);
    let raw = cs.resonance + cs.atmosphere - cs.consolidation + cs.counterfactual
        - cs.field_confidence
        + cs.content_saliency;
    SalienceVerdictV2 {
        holistic_bias: sigmoid(raw / weights.sigmoid_temperature),
        confidence: compute_confidence(&cs),
        driver: dominant_driver(&cs),
    }
}

/// Compute the verdict with the builtin weights.
pub fn compute_salience_builtin(
    conatus: ConatusEnergy,
    field: &Field,
    content_saliency: f64,
) -> SalienceVerdictV2 {
    compute_salience(
        SalienceWeightsV2::default(),
        conatus,
        field,
        content_saliency,
    )
}

/// Dispatch a verdict through the dead band: inside it, `Tied`
/// (formal-first downstream); outside, the leading hemisphere with its
/// margin as confidence-carrying weight.
pub fn salience_hemisphere(weights: SalienceWeightsV2, salience: SalienceVerdictV2) -> Hemisphere {
    let bias = salience.holistic_bias;
    let threshold = weights.verdict_threshold;
    if bias > 0.5 + threshold {
        Hemisphere::PreferHolistic(bias)
    } else if bias < 0.5 - threshold {
        Hemisphere::PreferFormal(1.0 - bias)
    } else {
        Hemisphere::Tied
    }
}

/// The aggregated self verdict: continuous controller output plus its
/// discrete dispatch, single canonical shape.
pub fn compute_self_verdict(
    weights: SalienceWeightsV2,
    conatus: ConatusEnergy,
    field: &Field,
    content_saliency: f64,
) -> SelfVerdictV2 {
    let salience = compute_salience(weights, conatus, field, content_saliency);
    SelfVerdictV2 {
        salience,
        hemisphere: salience_hemisphere(weights, salience),
    }
}

/// Predicate: does the Conatus gate fire on this energy under the builtin
/// weights? Single source of the decision boundary for recovery call sites.
pub fn conatus_gate_fires(conatus: ConatusEnergy) -> bool {
    finite_or_zero(conatus.scalar) < BUILTIN_SALIENCE_WEIGHTS.conatus_gate_threshold
}

/// Classify a move family as holistic (right-hemispheric). Single source
/// of truth for the Holistic/Formal partition.
pub fn is_holistic_family(family: CanonicalMoveFamily) -> bool {
    matches!(
        family,
        CanonicalMoveFamily::CMReflect
            | CanonicalMoveFamily::CMDefine
            | CanonicalMoveFamily::CMHypothesis
            | CanonicalMoveFamily::CMDeepen
            | CanonicalMoveFamily::CMPurpose
    )
}

/// Bounded post-commitment adaptation of the salience weights (Phase-B).
///
/// * `signal` in `[-1, 1]`: positive reinforces holistic-bias weights,
///   negative the formal-bias ones.
/// * Learning rate capped at 0.02 per turn; weights clamped to `[0, 2]`;
///   anti-drift: deviation from the defaults per weight cannot exceed 1.0.
/// * `signal = 0` is the exact identity (gating contract).
///
/// Empirical signal generation is deferred; this provides the bounded
/// mechanics so later calibration only changes the caller's signal.
pub fn adapt_salience_weights(raw_signal: f64, weights: SalienceWeightsV2) -> SalienceWeightsV2 {
    const LEARNING_RATE: f64 = 0.02;
    const DEVIATION_BOUND: f64 = 1.0;
    let signal = finite_or_zero(raw_signal).clamp(-1.0, 1.0);
    let delta = signal * LEARNING_RATE;
    if delta.abs() < 1e-12 {
        return weights;
    }
    let default = SalienceWeightsV2::default();
    let bounded = |target: f64, current: f64| {
        (current + delta)
            .clamp(0.0, 2.0)
            .clamp(target - DEVIATION_BOUND, target + DEVIATION_BOUND)
    };
    SalienceWeightsV2 {
        resonance: bounded(default.resonance, weights.resonance),
        atmosphere: bounded(default.atmosphere, weights.atmosphere),
        consolidation: bounded(default.consolidation, weights.consolidation),
        counterfactual: bounded(default.counterfactual, weights.counterfactual),
        field_confidence: bounded(default.field_confidence, weights.field_confidence),
        content_saliency: bounded(default.content_saliency, weights.content_saliency),
        ..weights
    }
}

/// Structural invariant checks for `doctor`: the builtin weights must be
/// non-negative with a positive sigmoid temperature and an ordered dead
/// band. Returns the violations (empty = ok).
pub fn validate_salience_invariants() -> Vec<String> {
    let mut violations = Vec::new();
    let weights = SalienceWeightsV2::default();
    for (name, value) in [
        ("resonance", weights.resonance),
        ("atmosphere", weights.atmosphere),
        ("consolidation", weights.consolidation),
        ("counterfactual", weights.counterfactual),
        ("field_confidence", weights.field_confidence),
        ("content_saliency", weights.content_saliency),
        ("verdict_threshold", weights.verdict_threshold),
    ] {
        if !value.is_finite() || value < 0.0 {
            violations.push(format!(
                "salience-v2 builtin weight {name} must be finite and non-negative: {value}"
            ));
        }
    }
    if !weights.sigmoid_temperature.is_finite() || weights.sigmoid_temperature <= 0.0 {
        violations.push(format!(
            "salience-v2 builtin sigmoid_temperature must be positive: {}",
            weights.sigmoid_temperature
        ));
    }
    if weights.verdict_threshold >= 0.5 {
        violations
            .push("salience-v2 builtin verdict_threshold must be below the 0.5 band edge".into());
    }
    // Calibration discipline: the production set must stay a nudge,
    // not a rewrite — every coefficient within 0.05 of its builtin,
    // thresholds byte-identical. A retuning that moves further must
    // update this bound explicitly, with evidence.
    let calibrated = SalienceWeightsV2::calibrated();
    for (name, tuned, base) in [
        ("resonance", calibrated.resonance, weights.resonance),
        ("atmosphere", calibrated.atmosphere, weights.atmosphere),
        (
            "consolidation",
            calibrated.consolidation,
            weights.consolidation,
        ),
        (
            "counterfactual",
            calibrated.counterfactual,
            weights.counterfactual,
        ),
        (
            "field_confidence",
            calibrated.field_confidence,
            weights.field_confidence,
        ),
        (
            "content_saliency",
            calibrated.content_saliency,
            weights.content_saliency,
        ),
    ] {
        if !tuned.is_finite() || (tuned - base).abs() > 0.05 {
            violations.push(format!(
                "salience-v2 calibrated weight {name} drifted past the 0.05 nudge bound: {tuned} vs builtin {base}"
            ));
        }
    }
    if calibrated.conatus_gate_threshold != weights.conatus_gate_threshold
        || calibrated.verdict_threshold != weights.verdict_threshold
        || calibrated.sigmoid_temperature != weights.sigmoid_temperature
    {
        violations.push("salience-v2 calibration must not move thresholds".into());
    }
    violations
}

fn finite_or_zero(value: f64) -> f64 {
    if value.is_finite() {
        value
    } else {
        0.0
    }
}

/// Closed-form logistic squash, total on the non-finite inputs.
fn sigmoid(x: f64) -> f64 {
    if !x.is_finite() {
        return if x > 0.0 { 1.0 } else { 0.0 };
    }
    1.0 / (1.0 + (-x).exp())
}

/// The driver with the largest absolute contribution; all-zero
/// contributions return `Default`. First-wins on exact magnitude ties,
/// matching the Haskell `pickLarger` (`>=`) fold order.
fn dominant_driver(cs: &SalienceContributions) -> V2SalienceDriver {
    let ranked = [
        (V2SalienceDriver::Resonance, cs.resonance.abs()),
        (V2SalienceDriver::Atmosphere, cs.atmosphere.abs()),
        (V2SalienceDriver::Consolidation, cs.consolidation.abs()),
        (V2SalienceDriver::Counterfactual, cs.counterfactual.abs()),
        (V2SalienceDriver::FieldConfidence, cs.field_confidence.abs()),
        (V2SalienceDriver::ContentSaliency, cs.content_saliency.abs()),
    ];
    let total: f64 = ranked
        .iter()
        .map(|(_, magnitude)| finite_or_zero(*magnitude))
        .sum();
    if total == 0.0 {
        return V2SalienceDriver::Default;
    }
    ranked
        .into_iter()
        .reduce(|best, candidate| {
            if best.1 >= candidate.1 {
                best
            } else {
                candidate
            }
        })
        .map(|(driver, _)| driver)
        .unwrap_or(V2SalienceDriver::Default)
}

/// Confidence as 1 minus normalised dispersion of the contributions:
/// `1.0` when one contribution dominates alone, `0.0` when all are equal
/// in magnitude, `1.0` when there is no signal to disagree on. The
/// divisor is `n−1 = 5` for six contributions — extend in lockstep with
/// `SalienceContributions`.
fn compute_confidence(cs: &SalienceContributions) -> f64 {
    let magnitudes = [
        cs.resonance.abs(),
        cs.atmosphere.abs(),
        cs.consolidation.abs(),
        cs.counterfactual.abs(),
        cs.field_confidence.abs(),
        cs.content_saliency.abs(),
    ];
    let magnitudes: Vec<f64> = magnitudes.into_iter().map(finite_or_zero).collect();
    let dominant = magnitudes.iter().cloned().fold(0.0f64, f64::max);
    if dominant == 0.0 {
        return 1.0;
    }
    let other: f64 = magnitudes.iter().cloned().sum::<f64>() - dominant;
    (1.0 - other / (5.0 * dominant)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blanket_energy(scalar: f64) -> ConatusEnergy {
        ConatusEnergy {
            scalar,
            ..ConatusEnergy::default()
        }
    }

    fn field_with(
        resonance: f64,
        confidence: f64,
        consolidation: f64,
        counterfactual: f64,
        arousal: f64,
    ) -> Field {
        Field {
            resonance,
            confidence,
            consolidation,
            counterfactual,
            atmosphere: qxfx0_types::field::Atmosphere::new(0.0, arousal),
        }
    }

    #[test]
    fn builtin_weights_match_the_haskell_reference() {
        let w = SalienceWeightsV2::default();
        assert_eq!(w.resonance, 1.0);
        assert_eq!(w.atmosphere, 0.5);
        assert_eq!(w.consolidation, 0.75);
        assert_eq!(w.counterfactual, 0.75);
        assert_eq!(w.field_confidence, 0.5);
        assert_eq!(w.content_saliency, 0.6);
        assert_eq!(w.conatus_gate_threshold, 0.0);
        assert_eq!(w.verdict_threshold, 0.05);
        assert_eq!(w.sigmoid_temperature, 1.0);
    }

    #[test]
    fn conatus_gate_short_circuits_below_threshold() {
        let verdict = compute_salience_builtin(
            blanket_energy(-0.5),
            &field_with(1.0, 0.0, 0.0, 1.0, 1.0),
            1.0,
        );
        assert_eq!(verdict.driver, V2SalienceDriver::ConatusGate);
        assert_eq!(verdict.holistic_bias, 0.0);
        assert_eq!(verdict.confidence, 1.0);
        assert!(conatus_gate_fires(blanket_energy(-0.5)));
        assert!(!conatus_gate_fires(blanket_energy(0.0)));
        // Exactly at the threshold the gate is closed (strict <).
        let at_gate = compute_salience_builtin(blanket_energy(0.0), &Field::default(), 0.0);
        assert_ne!(at_gate.driver, V2SalienceDriver::ConatusGate);
    }

    #[test]
    fn verdicts_stay_in_range_and_are_deterministic() {
        let field = field_with(0.9, 0.2, 0.1, 0.8, 0.7);
        let first = compute_salience_builtin(blanket_energy(12.0), &field, 0.4);
        let second = compute_salience_builtin(blanket_energy(12.0), &field, 0.4);
        assert_eq!(first, second);
        assert!((0.0..=1.0).contains(&first.holistic_bias));
        assert!((0.0..=1.0).contains(&first.confidence));
        // The builtin default field is resonance-driven (w_r = 1 is the
        // largest coefficient over equal 0.5-unit inputs).
        assert_eq!(first.driver, V2SalienceDriver::Resonance);
    }

    #[test]
    fn monotonicity_laws_hold() {
        let base = blanket_energy(10.0);
        let anchor = field_with(0.5, 0.5, 0.5, 0.5, 0.4);
        let bias_of = |field: &Field| compute_salience_builtin(base, field, 0.0).holistic_bias;
        for delta in [0.05, 0.2, 0.45] {
            assert!(
                bias_of(&field_with(0.5 + delta, 0.5, 0.5, 0.5, 0.4)) >= bias_of(&anchor),
                "bias is non-decreasing in resonance (delta {delta})"
            );
            assert!(
                bias_of(&field_with(0.5, 0.5, 0.5 + delta, 0.5, 0.4)) <= bias_of(&anchor),
                "bias is non-increasing in consolidation (delta {delta})"
            );
            assert!(
                bias_of(&field_with(0.5, 0.5, 0.5, 0.5 + delta, 0.4)) >= bias_of(&anchor),
                "bias is non-decreasing in counterfactual (delta {delta})"
            );
            assert!(
                bias_of(&field_with(0.5, 0.5 + delta, 0.5, 0.5, 0.4)) <= bias_of(&anchor),
                "bias is non-increasing in field confidence (delta {delta})"
            );
        }
    }

    #[test]
    fn dead_band_dispatches_tied_to_formal_default() {
        let weights = SalienceWeightsV2::default();
        let near_tied = SalienceVerdictV2 {
            holistic_bias: 0.5,
            confidence: 1.0,
            driver: V2SalienceDriver::Default,
        };
        assert_eq!(salience_hemisphere(weights, near_tied), Hemisphere::Tied);
        assert!(!Hemisphere::Tied.leads_holistic());
        let holistic = SalienceVerdictV2 {
            holistic_bias: 0.62,
            ..near_tied
        };
        assert_eq!(
            salience_hemisphere(weights, holistic),
            Hemisphere::PreferHolistic(0.62)
        );
        let formal = SalienceVerdictV2 {
            holistic_bias: 0.31,
            ..near_tied
        };
        assert_eq!(
            salience_hemisphere(weights, formal),
            Hemisphere::PreferFormal(0.69)
        );
    }

    #[test]
    fn self_verdict_aggregates_salience_and_dispatch() {
        let verdict = compute_self_verdict(
            SalienceWeightsV2::default(),
            blanket_energy(12.0),
            &field_with(1.0, 0.0, 0.0, 1.0, 1.0),
            0.0,
        );
        assert!(verdict.hemisphere.leads_holistic());
        assert_eq!(verdict.salience.driver, V2SalienceDriver::Resonance);
        let gated = compute_self_verdict(
            SalienceWeightsV2::default(),
            blanket_energy(-1.0),
            &field_with(1.0, 0.0, 0.0, 1.0, 1.0),
            0.0,
        );
        assert_eq!(gated.salience.driver, V2SalienceDriver::ConatusGate);
        assert_eq!(gated.hemisphere, Hemisphere::PreferFormal(1.0));
    }

    #[test]
    fn confidence_tracks_dominance() {
        // One dominant contribution, no disagreement.
        let solo = field_with(1.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            compute_salience_builtin(blanket_energy(1.0), &solo, 0.0).confidence,
            1.0
        );
        // Zero signal everywhere: no disagreement either.
        assert!(
            compute_salience_builtin(blanket_energy(1.0), &Field::default(), 0.0).confidence > 0.0
        );
        // All-equal magnitudes drive confidence to zero: resonance r and
        // atmosphere 0.5·a equal with weights 1.0/0.5 when arousal = 2r,
        // then add the inverse consolidation −0.75c and counterfactual
        // 0.75f with c = f = r… the full construction lives in the
        // contributions directly:
        let cs = SalienceContributions {
            resonance: 1.0,
            atmosphere: -1.0,
            consolidation: 1.0,
            counterfactual: -1.0,
            field_confidence: 1.0,
            content_saliency: 1.0,
        };
        assert_eq!(compute_confidence(&cs), 0.0);
    }

    #[test]
    fn dominant_driver_ties_break_to_first_listed() {
        let cs = SalienceContributions {
            resonance: -0.5,
            atmosphere: 0.5,
            consolidation: 0.0,
            counterfactual: 0.0,
            field_confidence: 0.0,
            content_saliency: 0.0,
        };
        assert_eq!(dominant_driver(&cs), V2SalienceDriver::Resonance);
        let zero = SalienceContributions {
            resonance: 0.0,
            atmosphere: 0.0,
            consolidation: 0.0,
            counterfactual: 0.0,
            field_confidence: 0.0,
            content_saliency: 0.0,
        };
        assert_eq!(dominant_driver(&zero), V2SalienceDriver::Default);
        assert_eq!(compute_confidence(&zero), 1.0);
    }

    #[test]
    fn non_finite_inputs_never_poison_the_verdict() {
        let field = field_with(f64::NAN, f64::INFINITY, 0.5, 0.5, 0.4);
        let verdict = compute_salience_builtin(blanket_energy(f64::NAN), &field, f64::NEG_INFINITY);
        assert!(verdict.holistic_bias.is_finite());
        assert!(verdict.confidence.is_finite());
        assert!((0.0..=1.0).contains(&verdict.holistic_bias));
        assert!((0.0..=1.0).contains(&verdict.confidence));
    }

    #[test]
    fn holistic_family_partition_matches_the_reference() {
        for family in [
            CanonicalMoveFamily::CMReflect,
            CanonicalMoveFamily::CMDefine,
            CanonicalMoveFamily::CMHypothesis,
            CanonicalMoveFamily::CMDeepen,
            CanonicalMoveFamily::CMPurpose,
        ] {
            assert!(is_holistic_family(family), "{family:?} is holistic");
        }
        for family in [
            CanonicalMoveFamily::CMGround,
            CanonicalMoveFamily::CMDescribe,
            CanonicalMoveFamily::CMRepair,
            CanonicalMoveFamily::CMContact,
            CanonicalMoveFamily::CMConnect,
            CanonicalMoveFamily::CMConfront,
            CanonicalMoveFamily::CMNextStep,
            CanonicalMoveFamily::CMClarify,
            CanonicalMoveFamily::CMAnchor,
        ] {
            assert!(!is_holistic_family(family), "{family:?} is formal");
        }
    }

    #[test]
    fn weight_adaptation_is_bounded_and_zero_signal_is_identity() {
        let default = SalienceWeightsV2::default();
        assert_eq!(adapt_salience_weights(0.0, default), default);
        let adapted = adapt_salience_weights(1.0, default);
        assert!((adapted.resonance - 1.02).abs() < 1e-12);
        // Saturation cannot leave [0, 2], and deviation cannot exceed 1.0.
        let saturated = adapt_salience_weights(
            1.0,
            SalienceWeightsV2 {
                resonance: 1.99,
                ..default
            },
        );
        assert!(saturated.resonance <= 2.0);
        assert!(saturated.resonance - default.resonance <= 1.0 + 1e-12);
        // Negative signal pushes the other way.
        let damped = adapt_salience_weights(-1.0, default);
        assert!((damped.resonance - 0.98).abs() < 1e-12);
        // The non-adapted control fields are untouched.
        assert_eq!(damped.verdict_threshold, default.verdict_threshold);
        assert_eq!(
            damped.conatus_gate_threshold,
            default.conatus_gate_threshold
        );
    }

    #[test]
    fn invariants_hold_on_the_builtins() {
        assert!(validate_salience_invariants().is_empty());
    }

    #[test]
    fn calibrated_is_a_nudge_with_identical_thresholds() {
        let base = SalienceWeightsV2::default();
        let tuned = SalienceWeightsV2::calibrated();
        assert!((tuned.resonance - base.resonance - 0.003).abs() < 1e-12);
        assert_eq!(tuned.verdict_threshold, base.verdict_threshold);
        assert_eq!(tuned.conatus_gate_threshold, base.conatus_gate_threshold);
        assert_eq!(tuned.sigmoid_temperature, base.sigmoid_temperature);
    }
}

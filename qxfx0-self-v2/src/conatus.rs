//! Conatus — the canonical scalar functional over the self blanket
//! (ADR-0043 U2, ported from Haskell `QxFx0.Self.Conatus`, Phase 2 /
//! ADR-0007 there).
//!
//! A Spinozan reading: a finite mode persists by an inner endeavour. This
//! module does not model subjective drive — it projects the structural
//! self-identity summary `SelfBlanketSnapshot (m, c, t)` plus the blanket
//! violations `v` onto a scalar
//!
//! ```text
//! C(b, v) = w_m·ln(1+m) + w_c·ln(1+c) + w_t·ln(1+t) − λ·|v|
//! ```
//!
//! with tunable [`ConatusWeights`]. The logarithmic shape encodes
//! diminishing returns; the violation penalty is discrete and large — a
//! structural rupture must not be wallpapered over by accumulation. The
//! analytic gradient of the smooth part points into the strictly-growing
//! orthant and decreases per axis: when several axes degrade, recovery
//! gains most by restoring the smallest one.
//!
//! This is the V2 canonical form. The V1 `qxfx0_self::Conatus` computes a
//! related scalar over the affective `Field` with fixed constants; V1 is
//! untouched and remains what the pipeline uses today.

use serde::{Deserialize, Serialize};

/// Tunable coefficients of the Conatus functional. The builtin defaults
/// encode the editorial judgement that morphological substance is the
/// primary carrier of identity, accumulated identity claims are secondary,
/// and raw turn count is the weakest indicator. The violation penalty is
/// deliberately large relative to any single log contribution.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ConatusWeights {
    pub morphology: f64,
    pub identity: f64,
    pub turns: f64,
    pub violation: f64,
}

/// The reference builtin weights (`QxFx0.Self.Conatus.builtinConatusWeights`).
pub const BUILTIN_CONATUS_WEIGHTS: ConatusWeights = ConatusWeights {
    morphology: 1.0,
    identity: 0.5,
    turns: 0.25,
    violation: 10.0,
};

impl Default for ConatusWeights {
    fn default() -> Self {
        BUILTIN_CONATUS_WEIGHTS
    }
}

/// The structural self summary the functional is defined over: morphology
/// size, identity-claim count, turn count (Haskell `SelfBlanket` fields).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfBlanketSnapshot {
    pub morphology_total_size: u64,
    pub identity_claims_count: u64,
    pub turn_count: u64,
}

/// One violated structural invariant of the blanket. Opaque to the
/// functional — only the count enters the penalty — but carried so traces
/// can name what ruptured.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BlanketViolation {
    pub code: String,
    pub detail: String,
}

/// Per-axis decomposition of the scalar. Invariant:
/// `ConatusEnergy::scalar` equals the sum of all five components.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ConatusComponents {
    pub morphology: f64,
    pub identity: f64,
    pub turns: f64,
    pub penalty: f64,
    /// Reserved parity field (always 0.0 in V2); the Haskell counterpart
    /// carries a self-divergence contribution reserved for later phases.
    pub self_divergence: f64,
}

/// The scalar value of the functional together with its decomposition.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ConatusEnergy {
    pub scalar: f64,
    pub components: ConatusComponents,
}

/// The gradient of the smooth part of `C` at a blanket. Each component is
/// the partial derivative with respect to that axis; strictly positive and
/// decreasing on every legitimately constructed blanket. The violation
/// penalty contributes nothing (it is a step in `|v|`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct ConatusGradient {
    pub morphology: f64,
    pub identity: f64,
    pub turns: f64,
}

/// Below this energy the system is considered structurally low and
/// self-preservation routing restrictions apply (Haskell ADR-0045
/// integration there; consumer-side policy here). Calibrated against the
/// log-scale codomain: a moderate blanket (20, 5, 10) yields ≈ 4.5.
pub const LOW_ENERGY_THRESHOLD: f64 = 3.0;

/// Compute the Conatus energy of a blanket under a violation list with the
/// builtin weights. Pure and total; the degenerate `(0,0,0)` blanket
/// yields exactly `0.0` before penalties.
pub fn compute_conatus_energy(
    blanket: SelfBlanketSnapshot,
    violations: &[BlanketViolation],
) -> ConatusEnergy {
    compute_conatus_energy_with(ConatusWeights::default(), blanket, violations)
}

/// Compute the Conatus energy under explicit weights (tests, experiments).
pub fn compute_conatus_energy_with(
    weights: ConatusWeights,
    blanket: SelfBlanketSnapshot,
    violations: &[BlanketViolation],
) -> ConatusEnergy {
    let m = blanket.morphology_total_size as f64;
    let c = blanket.identity_claims_count as f64;
    let t = blanket.turn_count as f64;
    let morphology = weights.morphology * (1.0 + m).ln();
    let identity = weights.identity * (1.0 + c).ln();
    let turns = weights.turns * (1.0 + t).ln();
    let penalty = -conatus_violation_magnitude(weights, violations.len());
    ConatusEnergy {
        scalar: morphology + identity + turns + penalty,
        components: ConatusComponents {
            morphology,
            identity,
            turns,
            penalty,
            self_divergence: 0.0,
        },
    }
}

/// Compute the gradient of the smooth part under the builtin weights.
pub fn compute_conatus_gradient(blanket: SelfBlanketSnapshot) -> ConatusGradient {
    compute_conatus_gradient_with(ConatusWeights::default(), blanket)
}

/// Compute the gradient under explicit weights.
pub fn compute_conatus_gradient_with(
    weights: ConatusWeights,
    blanket: SelfBlanketSnapshot,
) -> ConatusGradient {
    let m = blanket.morphology_total_size as f64;
    let c = blanket.identity_claims_count as f64;
    let t = blanket.turn_count as f64;
    ConatusGradient {
        morphology: weights.morphology / (1.0 + m),
        identity: weights.identity / (1.0 + c),
        turns: weights.turns / (1.0 + t),
    }
}

/// Euclidean magnitude of a gradient — an /urgency/ scalar for recovery.
pub fn gradient_magnitude(gradient: ConatusGradient) -> f64 {
    (gradient.morphology * gradient.morphology
        + gradient.identity * gradient.identity
        + gradient.turns * gradient.turns)
        .sqrt()
}

/// Normalize a gradient to unit magnitude; `None` on the zero vector
/// (reachable only with all-zero weights).
pub fn gradient_normalize(gradient: ConatusGradient) -> Option<ConatusGradient> {
    let magnitude = gradient_magnitude(gradient);
    if magnitude == 0.0 {
        None
    } else {
        Some(ConatusGradient {
            morphology: gradient.morphology / magnitude,
            identity: gradient.identity / magnitude,
            turns: gradient.turns / magnitude,
        })
    }
}

/// The signed magnitude of the violation penalty for `count` violations
/// (exposed for diagnostics; the same value appears as
/// `ConatusComponents::penalty`).
pub fn conatus_violation_penalty(weights: ConatusWeights, count: usize) -> f64 {
    -conatus_violation_magnitude(weights, count)
}

fn conatus_violation_magnitude(weights: ConatusWeights, count: usize) -> f64 {
    weights.violation * count as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degenerate_blanket_is_zero_before_penalties() {
        let energy = compute_conatus_energy(SelfBlanketSnapshot::default(), &[]);
        assert_eq!(energy.scalar, 0.0);
        assert_eq!(energy.components.penalty, 0.0);
    }

    #[test]
    fn healthy_moderate_blanket_matches_the_haskell_doc_example() {
        // Haskell Conatus.hs: blanket (20, 5, 10), no violations ≈ 4.5
        // (1.0·ln21 + 0.5·ln6 + 0.25·ln11).
        let blanket = SelfBlanketSnapshot {
            morphology_total_size: 20,
            identity_claims_count: 5,
            turn_count: 10,
        };
        let energy = compute_conatus_energy(blanket, &[]);
        let expected = (21.0f64).ln() + 0.5 * (6.0f64).ln() + 0.25 * (11.0f64).ln();
        assert!((energy.scalar - expected).abs() < 1e-12);
        assert!((energy.scalar - 4.54).abs() < 0.01, "{}", energy.scalar);
        // Doc claim: this healthy blanket sits above the low-energy line.
        assert!(energy.scalar > LOW_ENERGY_THRESHOLD);
    }

    #[test]
    fn each_violation_costs_exactly_lambda() {
        let blanket = SelfBlanketSnapshot {
            morphology_total_size: 20,
            identity_claims_count: 5,
            turn_count: 10,
        };
        let clean = compute_conatus_energy(blanket, &[]).scalar;
        let violations: Vec<BlanketViolation> = (0..3)
            .map(|i| BlanketViolation {
                code: format!("v{i}"),
                detail: "test".into(),
            })
            .collect();
        let dirty = compute_conatus_energy(blanket, &violations).scalar;
        assert!((clean - dirty - 30.0).abs() < 1e-12);
        // A rupture must not be wallpapered over: 3 violations sink a
        // healthy blanket far below the low-energy threshold.
        assert!(dirty < LOW_ENERGY_THRESHOLD);
        assert_eq!(
            conatus_violation_penalty(ConatusWeights::default(), 3),
            -30.0
        );
    }

    #[test]
    fn scalar_equals_the_sum_of_its_components() {
        let blanket = SelfBlanketSnapshot {
            morphology_total_size: 7,
            identity_claims_count: 3,
            turn_count: 42,
        };
        let violation = BlanketViolation {
            code: "rupture".into(),
            detail: "identity".into(),
        };
        let energy = compute_conatus_energy(blanket, &[violation]);
        let sum = energy.components.morphology
            + energy.components.identity
            + energy.components.turns
            + energy.components.penalty
            + energy.components.self_divergence;
        assert!((energy.scalar - sum).abs() < 1e-12);
    }

    #[test]
    fn gradient_is_positive_and_decreasing_per_axis() {
        let small = SelfBlanketSnapshot {
            morphology_total_size: 1,
            identity_claims_count: 1,
            turn_count: 1,
        };
        let large = SelfBlanketSnapshot {
            morphology_total_size: 1000,
            identity_claims_count: 1000,
            turn_count: 1000,
        };
        let g_small = compute_conatus_gradient(small);
        let g_large = compute_conatus_gradient(large);
        assert!(g_small.morphology > 0.0 && g_small.identity > 0.0 && g_small.turns > 0.0);
        assert!(g_large.morphology < g_small.morphology);
        assert!(g_large.identity < g_small.identity);
        assert!(g_large.turns < g_small.turns);
    }

    #[test]
    fn normalize_rescales_and_rejects_zero() {
        let gradient = compute_conatus_gradient(SelfBlanketSnapshot {
            morphology_total_size: 4,
            identity_claims_count: 9,
            turn_count: 16,
        });
        let normalized = gradient_normalize(gradient).expect("non-zero gradient");
        assert!((gradient_magnitude(normalized) - 1.0).abs() < 1e-12);
        assert!(gradient_normalize(ConatusGradient::default()).is_none());
    }
}

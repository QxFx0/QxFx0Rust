//! Cross-twin conformance, first target (ADR-0043 U6 follow-up):
//! `qxfx0-self-v2` Conatus against `QxFx0.Self.Conatus` golden vectors.
//!
//! The fixture is real Haskell output (provenance in the TSV header), not
//! a re-encoding of the Rust code — a drift on either side fails here and
//! must be re-pinned explicitly. Tolerance is 1e-12 relative: both sides
//! are IEEE-754 Doubles over the same operations on the same machine, so
//! anything larger is a semantic drift, not libm noise.

use qxfx0_self_v2::conatus::{
    compute_conatus_energy, compute_conatus_gradient, gradient_magnitude, BlanketViolation,
    SelfBlanketSnapshot,
};

const GOLDEN: &str = include_str!("fixtures/haskell_conatus_golden.tsv");
/// Relative tolerance for twin agreement.
const REL_TOL: f64 = 1e-12;

fn close_enough(name: &str, expected: f64, actual: f64) {
    let scale = expected.abs().max(actual.abs().max(1.0));
    let diff = (expected - actual).abs();
    assert!(
        diff <= REL_TOL * scale,
        "{name}: haskell={expected:?} rust={actual:?} diff={diff:?}"
    );
}

#[test]
fn conatus_matches_haskell_on_every_golden_case() {
    let mut cases = 0;
    for line in GOLDEN.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let head: Vec<&str> = line.split_whitespace().take(4).collect();
        let tail: Vec<&str> = line.split('\t').skip(1).collect();
        assert_eq!(head.len(), 4, "input head: {line}");
        assert_eq!(tail.len(), 9, "golden tail: {line}");
        let parse = |slot: &str| slot.parse::<f64>().expect("golden parses");
        let (m, c, t) = (parse(head[0]), parse(head[1]), parse(head[2]));
        let nviol = parse(head[3]) as usize;
        let blanket = SelfBlanketSnapshot {
            morphology_total_size: m as u64,
            identity_claims_count: c as u64,
            turn_count: t as u64,
        };
        let violations: Vec<BlanketViolation> = (0..nviol)
            .map(|index| BlanketViolation {
                code: format!("parity-{index}"),
                detail: String::new(),
            })
            .collect();

        let energy = compute_conatus_energy(blanket, &violations);
        let gradient = compute_conatus_gradient(blanket);
        let magnitude = gradient_magnitude(gradient);
        let actual = [
            energy.scalar,
            energy.components.morphology,
            energy.components.identity,
            energy.components.turns,
            energy.components.penalty,
            gradient.morphology,
            gradient.identity,
            gradient.turns,
            magnitude,
        ];
        let names = [
            "scalar",
            "comp_m",
            "comp_c",
            "comp_t",
            "penalty",
            "grad_m",
            "grad_c",
            "grad_t",
            "magnitude",
        ];
        for (index, golden) in tail.iter().enumerate() {
            close_enough(
                &format!("m={m} c={c} t={t} n={nviol} {}", names[index]),
                parse(golden),
                actual[index],
            );
        }
        // The Haskell invariant, re-checked on the Rust side: the scalar is
        // the sum of the (zero-divergence) components.
        let sum = energy.components.morphology
            + energy.components.identity
            + energy.components.turns
            + energy.components.penalty
            + energy.components.self_divergence;
        close_enough("scalar-vs-sum", energy.scalar, sum);
        cases += 1;
    }
    assert_eq!(cases, 12, "all golden cases ran");
}

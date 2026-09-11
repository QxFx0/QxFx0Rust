//! Cross-twin conformance, third target (ADR-0043 U6 follow-up):
//! `qxfx0-self-v2` Salience controller against `QxFx0.Self.Salience`
//! golden cases.
//!
//! Each fixture row carries its own inputs (conatus scalar, five field
//! floats, content saliency); the test runs `compute_self_verdict` with
//! the builtin weights and compares bias, confidence, driver tag and
//! dispatch verdict. Haskell driver `Show` spellings map to the
//! twin-native `V2SalienceDriver`; verdict margins compare as floats.
//! Weight calibration and `adaptSalienceWeights` are explicitly out of
//! scope (see the fixture header).

use qxfx0_self_v2::conatus::{ConatusComponents, ConatusEnergy};
use qxfx0_self_v2::salience::{
    compute_self_verdict, Hemisphere, SalienceWeightsV2, V2SalienceDriver,
};
use qxfx0_types::field::{Atmosphere, Field};

const GOLDEN: &str = include_str!("fixtures/haskell_salience_golden.tsv");
const REL_TOL: f64 = 1e-12;

fn close_enough(name: &str, expected: f64, actual: f64) {
    let scale = expected.abs().max(actual.abs().max(1.0));
    let diff = (expected - actual).abs();
    assert!(
        diff <= REL_TOL * scale,
        "{name}: haskell={expected:?} rust={actual:?} diff={diff:?}"
    );
}

fn render_driver(driver: V2SalienceDriver) -> &'static str {
    match driver {
        V2SalienceDriver::Resonance => "DrivenByResonance",
        V2SalienceDriver::Atmosphere => "DrivenByAtmosphere",
        V2SalienceDriver::Consolidation => "DrivenByConsolidation",
        V2SalienceDriver::Counterfactual => "DrivenByCounterfactual",
        V2SalienceDriver::FieldConfidence => "DrivenByFieldConfidence",
        V2SalienceDriver::ConatusGate => "DrivenByConatusGate",
        V2SalienceDriver::ContentSaliency => "DrivenByContentSaliency",
        V2SalienceDriver::Default => "DrivenByDefault",
    }
}

#[test]
fn salience_matches_haskell_on_every_golden_case() {
    let mut cases = 0;
    for line in GOLDEN.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (head, tail) = line.split_once('\t').expect("head/tail");
        let input: Vec<&str> = head.split_whitespace().collect();
        let golden: Vec<&str> = tail.split_whitespace().collect();
        assert_eq!(input.len(), 9, "input head: {line}");
        assert_eq!(golden.len(), 5, "golden tail: {line}");
        let parse = |slot: &str| slot.parse::<f64>().expect("golden parses");
        let energy = ConatusEnergy {
            scalar: parse(input[1]),
            components: ConatusComponents {
                morphology: 0.0,
                identity: 0.0,
                turns: 0.0,
                penalty: 0.0,
                self_divergence: 0.0,
            },
        };
        let field = Field {
            resonance: parse(input[2]),
            atmosphere: Atmosphere {
                valence: parse(input[4]),
                arousal: parse(input[3]),
            },
            confidence: parse(input[5]),
            consolidation: parse(input[6]),
            counterfactual: parse(input[7]),
        };
        let verdict = compute_self_verdict(
            SalienceWeightsV2::default(),
            energy,
            &field,
            parse(input[8]),
        );
        let tag = input[0].to_string();
        close_enough(
            &format!("{tag} bias"),
            parse(golden[0]),
            verdict.salience.holistic_bias,
        );
        close_enough(
            &format!("{tag} confidence"),
            parse(golden[1]),
            verdict.salience.confidence,
        );
        assert_eq!(
            render_driver(verdict.salience.driver),
            golden[2],
            "{tag} driver"
        );
        match verdict.hemisphere {
            Hemisphere::PreferHolistic(margin) => {
                assert_eq!("PreferHolistic", golden[3], "{tag} verdict");
                close_enough(&format!("{tag} margin"), parse(golden[4]), margin);
            }
            Hemisphere::PreferFormal(margin) => {
                assert_eq!("PreferFormal", golden[3], "{tag} verdict");
                close_enough(&format!("{tag} margin"), parse(golden[4]), margin);
            }
            Hemisphere::Tied => {
                assert_eq!("Tied", golden[3], "{tag} verdict");
            }
        }
        cases += 1;
    }
    assert_eq!(cases, 14, "all golden cases ran");
}

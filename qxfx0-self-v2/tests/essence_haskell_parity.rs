//! Cross-twin conformance, second target (ADR-0043 U6 follow-up):
//! `qxfx0-self-v2` Essence witness dynamics against `QxFx0.Self.Essence`
//! golden trajectories.
//!
//! Each fixture row carries its own inputs (rule, divergence,
//! agreement class, conatus scalar, five field floats), so the test
//! replays the same scripted trajectory natively: Haskell enum
//! spellings map to the twin-native enums (`Agree`→`FullAgreement`,
//! `Diverge*`→`NoAgreement`; rules share all six names), drivers cycle
//! per-twin (stored, never dynamics). Compared per step: angst, conatus
//! floor, witness count, the five bands of the *stored* witness,
//! `shouldCommit` trigger, and `extractMode`. Floats at 1e-12 relative;
//! enums by Haskell `Show` spelling. Witness hashes and `validatePlan`
//! are explicitly out of scope (see the fixture header).

use qxfx0_self::deliberation::{Agreement, DeliberationTrace, ReconcileRule, SalienceDriver};
use qxfx0_self_v2::conatus::{ConatusComponents, ConatusEnergy};
use qxfx0_self_v2::essence::{
    empty_trajectory, extract_mode, should_commit, witness, Band, CommitmentTrigger, EssenceMode,
    EssenceModulation, ValenceBand,
};
use qxfx0_types::field::{Atmosphere, Field};

const GOLDEN: &str = include_str!("fixtures/haskell_essence_golden.tsv");
const REL_TOL: f64 = 1e-12;

fn close_enough(name: &str, expected: f64, actual: f64) {
    let scale = expected.abs().max(actual.abs().max(1.0));
    let diff = (expected - actual).abs();
    assert!(
        diff <= REL_TOL * scale,
        "{name}: haskell={expected:?} rust={actual:?} diff={diff:?}"
    );
}

fn parse_rule(name: &str) -> ReconcileRule {
    match name {
        "RuleConatusOverride" => ReconcileRule::RuleConatusOverride,
        "RuleAgreement" => ReconcileRule::RuleAgreement,
        "RuleSalienceLead" => ReconcileRule::RuleSalienceLead,
        "RuleHolisticAdvantage" => ReconcileRule::RuleHolisticAdvantage,
        "RuleFormalAdvantage" => ReconcileRule::RuleFormalAdvantage,
        "RuleTiedFallback" => ReconcileRule::RuleTiedFallback,
        _ => panic!("unknown rule: {name}"),
    }
}

fn parse_agreement(name: &str) -> Agreement {
    match name {
        "Agree" => Agreement::FullAgreement,
        _ => Agreement::NoAgreement,
    }
}

fn render_band(band: Band) -> &'static str {
    match band {
        Band::Low => "BandLow",
        Band::Mid => "BandMid",
        Band::High => "BandHigh",
    }
}

fn render_valence(band: ValenceBand) -> &'static str {
    match band {
        ValenceBand::Negative => "ValenceNegative",
        ValenceBand::Neutral => "ValenceNeutral",
        ValenceBand::Positive => "ValencePositive",
    }
}

fn render_commit(trigger: Option<CommitmentTrigger>) -> String {
    match trigger {
        None => "Nothing".to_string(),
        Some(CommitmentTrigger::AngstThreshold) => "Just TriggerAngstThreshold".to_string(),
        Some(CommitmentTrigger::ConatusErosion) => "Just TriggerConatusErosion".to_string(),
    }
}

fn render_mode(mode: EssenceMode) -> &'static str {
    match mode {
        EssenceMode::Witnessing => "EssenceWitnessing",
        EssenceMode::Contemplative => "EssenceContemplative",
        EssenceMode::Dialogical => "EssenceDialogical",
        EssenceMode::Integrative => "EssenceIntegrative",
    }
}

#[test]
fn essence_replays_haskell_trajectories_step_for_step() {
    let modulation = EssenceModulation::default();
    let drivers = [
        SalienceDriver::DrivenByConatusGate,
        SalienceDriver::DrivenByField,
        SalienceDriver::DrivenBySalienceDefault,
    ];
    let mut trajectory = empty_trajectory();
    let mut scenario = String::new();
    let mut rows = 0;
    for line in GOLDEN.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (head, tail) = line.split_once('\t').expect("head/tail");
        let input: Vec<&str> = head.split_whitespace().collect();
        // The tail splits into 10 or 11 tokens: `Just Trigger…` carries a
        // space, so commit/mode are anchored from the end.
        let golden: Vec<&str> = tail.split_whitespace().collect();
        assert_eq!(input.len(), 11, "input head: {line}");
        assert!(
            golden.len() == 10 || golden.len() == 11,
            "golden tail: {line}"
        );
        let (commit_golden, mode_golden) = if golden.len() == 11 {
            (
                format!("{} {}", golden[8], golden[9]),
                golden[10].to_string(),
            )
        } else {
            (golden[8].to_string(), golden[9].to_string())
        };
        if input[0] != scenario {
            scenario = input[0].to_string();
            trajectory = empty_trajectory();
        }
        let step: usize = input[1].parse().expect("step parses");
        let trace = DeliberationTrace {
            salience_driver: drivers[(step - 1) % drivers.len()],
            rule: parse_rule(input[2]),
            agreement: parse_agreement(input[4]),
            divergence: input[3].parse().expect("div parses"),
        };
        let field = Field {
            resonance: input[6].parse().expect("res parses"),
            atmosphere: Atmosphere {
                valence: input[8].parse().expect("val parses"),
                arousal: input[7].parse().expect("aro parses"),
            },
            confidence: 1.0,
            consolidation: input[9].parse().expect("cfl parses"),
            counterfactual: input[10].parse().expect("cf parses"),
        };
        let conatus = ConatusEnergy {
            scalar: input[5].parse().expect("conatus parses"),
            components: ConatusComponents {
                morphology: 0.0,
                identity: 0.0,
                turns: 0.0,
                penalty: 0.0,
                self_divergence: 0.0,
            },
        };
        witness(&modulation, step, conatus, &field, &trace, &mut trajectory);
        let tag = format!("{} step {step}", input[0]);
        close_enough(
            &format!("{tag} angst"),
            golden[0].parse().unwrap(),
            trajectory.angst_level,
        );
        close_enough(
            &format!("{tag} floor"),
            golden[1].parse().unwrap(),
            trajectory.conatus_floor,
        );
        assert_eq!(
            trajectory.witnesses.len(),
            golden[2].parse::<usize>().unwrap(),
            "{tag} count"
        );
        let stored = trajectory.witnesses.back().expect("just witnessed");
        for (index, rendered) in [
            render_band(stored.field_signature.resonance),
            render_band(stored.field_signature.arousal),
            render_valence(stored.field_signature.valence),
            render_band(stored.field_signature.consolidation),
            render_band(stored.field_signature.counterfactual),
        ]
        .iter()
        .enumerate()
        {
            assert_eq!(rendered, &golden[3 + index], "{tag} band {index}");
        }
        assert_eq!(
            render_commit(should_commit(&modulation, &trajectory)),
            commit_golden,
            "{tag} commit"
        );
        assert_eq!(
            render_mode(extract_mode(&trajectory)),
            mode_golden,
            "{tag} mode"
        );
        rows += 1;
    }
    assert_eq!(rows, 31, "all golden rows replayed");
}

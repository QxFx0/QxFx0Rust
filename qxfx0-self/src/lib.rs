pub mod deliberation;
pub mod perspective;

pub use perspective::{integrate_curated_claims, PerspectiveUpdate};

use qxfx0_types::field::Field;
use qxfx0_types::system_state::*;
use sha2::{Digest, Sha256};

/// Conatus energy functional:
/// C(b,v) = w_m·log(1+m) + w_c·log(1+c) + w_t·log(1+t) − λ·|v|
///
/// Spinozan striving — the system's drive to continue being what it is.
/// Higher = more coherent self. Death = Markov blanket violation.
///
/// The arousal component of Atmosphere contributes as an additive boost:
/// higher arousal → higher conatus (the system is more "awake").
pub struct Conatus;

impl Conatus {
    pub const W_MEANING: f64 = 1.0;
    pub const W_COHERENCE: f64 = 1.0;
    pub const W_TRUST: f64 = 0.5;
    pub const W_AROUSAL: f64 = 0.3;
    pub const LAMBDA: f64 = 0.1;
    pub const STRUCTURAL_FLOOR: f64 = 0.5;

    /// Compute conatus energy from field components.
    /// All intermediate values are clamped to [0, ∞) for log safety.
    /// If any field component is NaN or infinite, returns 0.0.
    pub fn compute(field: &Field) -> f64 {
        if !field.resonance.is_finite()
            || !field.confidence.is_finite()
            || !field.consolidation.is_finite()
            || !field.counterfactual.is_finite()
            || !field.atmosphere.arousal.is_finite()
        {
            return 0.0;
        }
        let m = field.resonance.max(0.0);
        let c = field.consolidation.max(0.0);
        let t = field.confidence.max(0.0);
        let v = (field.counterfactual - 0.5).abs();
        let a = field.atmosphere.arousal.max(0.0);

        Self::W_MEANING * (1.0 + m).ln()
            + Self::W_COHERENCE * (1.0 + c).ln()
            + Self::W_TRUST * (1.0 + t).ln()
            + Self::W_AROUSAL * (1.0 + a).ln()
            - Self::LAMBDA * v
    }

    /// Check if conatus gate fires (energy below threshold).
    pub fn gate_fired(energy: f64, threshold: f64) -> bool {
        energy < threshold
    }
}

/// Holistic mode — right-hemispheric, resonance-driven.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Holistic(pub f64);

/// Formal mode — left-hemispheric, structure-driven.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Formal(pub f64);

impl Holistic {
    pub fn from_field(field: &Field) -> Self {
        Holistic(field.resonance * 0.6 + field.counterfactual * 0.4)
    }
}

impl Formal {
    pub fn from_field(field: &Field) -> Self {
        Formal(field.confidence * 0.7 + field.consolidation * 0.3)
    }
}

/// Combine holistic and formal field modes into a single scalar factor.
///
/// For non-degenerate fields (where `holistic * formal >= 0.01`), this
/// returns `1.0`. For degenerate fields where the composed product falls
/// below `0.01`, it returns `composed / 0.01`, which preserves a finite
/// scaled value without claiming an identity.
///
/// This helper intentionally does not model a categorical adjunction —
/// it is a normalization utility that prevents division by zero while
/// keeping the non-degenerate path transparent.
pub fn combine_modes(field: &Field) -> f64 {
    let h = Holistic::from_field(field);
    let f = Formal::from_field(field);
    let composed = h.0 * f.0;
    composed / composed.max(0.01)
}

/// Adjunction: Holistic ⊣ Formal
///
/// Provides the `reconcile` helper for blending holistic and formal
/// proposals into a unified plan.
pub struct Adjunction;

impl Adjunction {
    /// Reconcile holistic and formal proposals into a plan.
    /// Weighted by field confidence — high confidence → formal, low → holistic.
    pub fn reconcile(holistic: f64, formal: f64, field: &Field) -> f64 {
        let w = field.confidence;
        w * formal + (1.0 - w) * holistic
    }
}

/// Essence — Σ-typed commitment trajectory.
/// Operational mode for pipeline routing (runtime, not commitment mode).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EssenceMode {
    Define,
    Defend,
    Revise,
    Commit,
}

/// Essence modulation parameters (tunable).
#[derive(Debug, Clone)]
pub struct EssenceModulation {
    pub angst_commitment_threshold: f64,
    pub angst_accrual_rate: f64,
    pub angst_decay_rate: f64,
    pub angst_accrual_divergence_floor: f64,
    pub conatus_floor_window: usize,
    pub conatus_structural_floor: f64,
    pub trajectory_capacity: usize,
}

impl Default for EssenceModulation {
    fn default() -> Self {
        EssenceModulation {
            angst_commitment_threshold: 0.75,
            angst_accrual_rate: 0.05,
            angst_decay_rate: 0.02,
            angst_accrual_divergence_floor: 0.5,
            conatus_floor_window: 8,
            // Calibrated for Conatus::compute() range [0, ~1.5] (Rust version uses
            // Field components in [0,1], not the log-scale [~5,~20] of the Haskell
            // version). A value of 0.5 means: every witness in the last 8 turns
            // must have conatus < 0.5 for ConatusErosion to trigger.
            conatus_structural_floor: 0.5,
            trajectory_capacity: 32,
        }
    }
}

/// Witness input — groups the 6 contextual parameters of `witness_essence`
/// into a single struct to reduce argument count below the clippy threshold.
#[derive(Debug, Clone)]
pub struct WitnessInput<'a> {
    pub mode: EssenceMode,
    pub statement: String,
    pub salience_driver: &'a str,
    pub reconcile_rule: &'a str,
    pub agreement: &'a str,
    pub divergence: f64,
}

/// Ingest one turn's deliberation into the trajectory.
pub fn witness_essence(
    em: &EssenceModulation,
    turn: usize,
    conatus_scalar: f64,
    state: &mut EssenceState,
    input: &WitnessInput,
) {
    if state.capacity == 0 {
        state.capacity = em.trajectory_capacity;
    }

    state.witnesses.push(EssenceWitness {
        turn,
        mode: format!("{:?}", input.mode),
        statement: input.statement.clone(),
        salience_driver: input.salience_driver.to_string(),
        reconcile_rule: input.reconcile_rule.to_string(),
        agreement: input.agreement.to_string(),
        divergence: input.divergence,
        conatus_scalar,
    });

    while state.witnesses.len() > state.capacity {
        state.witnesses.remove(0);
    }

    // Update angst
    if input.divergence == 0.0 {
        state.angst = (state.angst - em.angst_decay_rate).max(0.0);
    } else if input.divergence >= em.angst_accrual_divergence_floor {
        state.angst = (state.angst + em.angst_accrual_rate).min(1.0);
    }

    // Update conatus floor
    state.conatus_floor = state.conatus_floor.min(conatus_scalar);

    state.trajectory_committed = true;
}

/// Sliding-window commitment check.
pub fn should_commit_essence(
    em: &EssenceModulation,
    state: &EssenceState,
) -> Option<CommitmentTrigger> {
    let angst_fires = state.angst >= em.angst_commitment_threshold;

    let window = em.conatus_floor_window;
    let ws = &state.witnesses;
    let start = if ws.len() > window {
        ws.len() - window
    } else {
        0
    };
    let last_n: Vec<&EssenceWitness> = ws[start..].iter().collect();
    let all_sub_floor = last_n.len() >= window
        && last_n
            .iter()
            .all(|w| w.conatus_scalar < em.conatus_structural_floor);

    match (angst_fires, all_sub_floor) {
        (true, _) => Some(CommitmentTrigger::TriggerAngstThreshold),
        (false, true) => Some(CommitmentTrigger::TriggerConatusErosion),
        _ => None,
    }
}

/// Deterministic mode extraction from trajectory.
pub fn extract_commitment_mode(state: &EssenceState) -> CommitmentMode {
    let ws = &state.witnesses;
    let n = ws.len() as f64;
    if n == 0.0 {
        return CommitmentMode::Contemplative;
    }

    let mut integrative = 0usize;
    let mut dialogical = 0usize;
    let mut contemplative = 0usize;

    for w in ws {
        match (w.agreement.as_str(), w.reconcile_rule.as_str()) {
            (_, "RuleHolisticAdvantage") => dialogical += 1,
            (_, "RuleFormalAdvantage") => contemplative += 1,
            _ => integrative += 1,
        }
    }

    let a_rate = integrative as f64 / n;
    let h_rate = dialogical as f64 / n;
    let f_rate = contemplative as f64 / n;

    let candidates = [
        (CommitmentMode::Integrative, a_rate),
        (CommitmentMode::Dialogical, h_rate),
        (CommitmentMode::Contemplative, f_rate),
    ];

    let (best, best_rate) = candidates
        .iter()
        .max_by(|a, b| a.1.total_cmp(&b.1))
        .copied()
        .expect("candidates is non-empty");

    if best_rate > 0.0 {
        best
    } else {
        CommitmentMode::Contemplative
    }
}

/// Commit the essence trajectory.
pub fn commit_essence(
    turn: usize,
    trigger: CommitmentTrigger,
    state: &EssenceState,
) -> EssenceCommitment {
    let mode = extract_commitment_mode(state);
    let hash = hash_witnesses(&state.witnesses);
    EssenceCommitment {
        mode,
        trigger,
        committed_at: turn,
        witness_hash: hash,
    }
}

/// SHA-256 hash of the witness sequence for tamper detection.
/// Uses fixed-precision f64 formatting for deterministic hashing across Rust versions.
fn hash_witnesses(witnesses: &[EssenceWitness]) -> String {
    let mut hasher = Sha256::new();
    for w in witnesses {
        hasher.update(
            format!(
                "{}|{}|{}|{}|{}|{:.17}|{}|{:.17}",
                w.turn,
                w.mode,
                w.salience_driver,
                w.reconcile_rule,
                w.agreement,
                w.divergence,
                w.statement,
                w.conatus_scalar
            )
            .as_bytes(),
        );
    }
    format!("sha256:{:x}", hasher.finalize())
}

/// Anomaly-3: self-referential collapse of essence trajectory.
pub fn collapse_essence(turn: usize, state: &mut EssenceState) -> EssenceResetEvent {
    let event = EssenceResetEvent {
        turn,
        previous_angst: state.angst,
        previous_witness_count: state.witnesses.len(),
    };
    state.witnesses.clear();
    state.angst = 0.0;
    state.conatus_floor = f64::MAX;
    state.trajectory_committed = false;
    state.commitment = None;
    state.reset_events.push(event.clone());
    event
}

/// Self blanket — structural invariants for self-preservation.
pub struct SelfBlanket;

impl SelfBlanket {
    pub fn check(field: &Field, conatus: f64) -> Vec<String> {
        let mut violations = Vec::new();
        if conatus <= 0.0 {
            violations.push("negative_conatus_energy".into());
        }
        if !(0.0..=1.0).contains(&field.resonance) {
            violations.push("resonance_out_of_range".into());
        }
        if !(0.0..=1.0).contains(&field.confidence) {
            violations.push("confidence_out_of_range".into());
        }
        if !(0.0..=1.0).contains(&field.consolidation) {
            violations.push("consolidation_out_of_range".into());
        }
        if !(0.0..=1.0).contains(&field.counterfactual) {
            violations.push("counterfactual_out_of_range".into());
        }
        if !(-1.0..=1.0).contains(&field.atmosphere.valence) {
            violations.push("atmosphere_valence_out_of_range".into());
        }
        if !(0.0..=1.0).contains(&field.atmosphere.arousal) {
            violations.push("atmosphere_arousal_out_of_range".into());
        }
        violations
    }
}

/// Salience controller — biases Holistic/Formal balance.
pub struct Salience;

impl Salience {
    pub fn compute(field: &Field) -> f64 {
        field.resonance * 0.35 + (1.0 - field.confidence) * 0.25 + field.counterfactual * 0.25
            - field.consolidation * 0.15
            + field.atmosphere.arousal * 0.15
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conatus_positive() {
        let field = Field::default();
        let energy = Conatus::compute(&field);
        assert!(energy > 0.0);
    }

    #[test]
    fn test_nan_conatus_returns_zero() {
        assert_eq!(
            Conatus::compute(&Field {
                resonance: f64::NAN,
                ..Default::default()
            }),
            0.0
        );

        assert_eq!(
            Conatus::compute(&Field {
                confidence: f64::INFINITY,
                ..Default::default()
            }),
            0.0
        );

        assert_eq!(
            Conatus::compute(&Field {
                counterfactual: f64::NEG_INFINITY,
                ..Default::default()
            }),
            0.0
        );
    }

    #[test]
    fn test_conatus_increases_with_resonance() {
        let low = Field {
            resonance: 0.1,
            ..Default::default()
        };
        let high = Field {
            resonance: 0.9,
            ..Default::default()
        };
        assert!(Conatus::compute(&high) > Conatus::compute(&low));
    }

    #[test]
    fn test_adjunction_roundtrip_non_degenerate() {
        let field = Field {
            confidence: 0.5,
            resonance: 0.5,
            consolidation: 0.5,
            counterfactual: 0.5,
            ..Default::default()
        };
        let h = Holistic::from_field(&field).0;
        let f = Formal::from_field(&field).0;
        assert!(
            h * f >= 0.01,
            "test assumes a non-degenerate composed weight"
        );
        assert!(
            (combine_modes(&field) - 1.0).abs() < 1e-10,
            "combine_modes should return identity factor for non-degenerate fields"
        );
    }

    #[test]
    fn test_adjunction_factor_for_non_degenerate_field() {
        let field = Field {
            confidence: 0.3,
            resonance: 0.8,
            ..Default::default()
        };
        let result = combine_modes(&field);
        assert!(
            (result - 1.0).abs() < 1e-10,
            "combine_modes should return 1.0 for non-degenerate fields"
        );
    }

    #[test]
    fn test_adjunction_reconcile() {
        let field = Field {
            confidence: 0.8,
            ..Default::default()
        };
        let result = Adjunction::reconcile(0.3, 0.7, &field);
        assert!(result > 0.5, "High confidence should favor formal");
    }

    #[test]
    fn test_essence_lifecycle() {
        let mut state = EssenceState::default();
        let em = EssenceModulation::default();
        assert!(!state.trajectory_committed);
        let input = WitnessInput {
            mode: EssenceMode::Define,
            statement: "свобода".into(),
            salience_driver: "DrivenByField",
            reconcile_rule: "RuleAgreement",
            agreement: "FullAgreement",
            divergence: 0.0,
        };
        witness_essence(&em, 1, 10.0, &mut state, &input);
        assert!(state.trajectory_committed);
        assert_eq!(state.witnesses.len(), 1);
        collapse_essence(2, &mut state);
        assert!(!state.trajectory_committed);
        assert!(state.witnesses.is_empty());
    }

    #[test]
    fn test_essence_should_commit_angst() {
        let mut state = EssenceState {
            angst: 0.8,
            ..EssenceState::default()
        };
        for _ in 0..8 {
            state.witnesses.push(EssenceWitness {
                turn: 1,
                mode: "Define".into(),
                statement: "test".into(),
                salience_driver: "field".into(),
                reconcile_rule: "RuleAgreement".into(),
                agreement: "FullAgreement".into(),
                divergence: 0.0,
                conatus_scalar: 1.0,
            });
        }
        let em = EssenceModulation::default();
        let trigger = should_commit_essence(&em, &state);
        assert!(trigger.is_some());
        assert!(matches!(
            trigger.unwrap(),
            CommitmentTrigger::TriggerAngstThreshold
        ));
    }

    #[test]
    fn test_essence_should_commit_conatus_erosion() {
        let mut state = EssenceState {
            angst: 0.1,
            ..EssenceState::default()
        };
        for _ in 0..8 {
            state.witnesses.push(EssenceWitness {
                turn: 1,
                mode: "Define".into(),
                statement: "test".into(),
                salience_driver: "field".into(),
                reconcile_rule: "RuleAgreement".into(),
                agreement: "FullAgreement".into(),
                divergence: 0.0,
                conatus_scalar: 0.4, // below conatus_structural_floor = 0.5
            });
        }
        let em = EssenceModulation::default();
        let trigger = should_commit_essence(&em, &state);
        assert!(trigger.is_some());
        assert!(matches!(
            trigger.unwrap(),
            CommitmentTrigger::TriggerConatusErosion
        ));
    }

    #[test]
    fn test_self_blanket_no_violations() {
        let field = Field::default();
        let violations = SelfBlanket::check(&field, Conatus::compute(&field));
        assert!(violations.is_empty());
    }

    #[test]
    fn test_self_blanket_negative_conatus() {
        let field = Field::default();
        let violations = SelfBlanket::check(&field, -1.0);
        assert!(violations.contains(&"negative_conatus_energy".into()));
    }

    #[test]
    fn test_self_blanket_counterfactual_out_of_range() {
        let field = Field {
            counterfactual: 1.5,
            ..Default::default()
        };
        let violations = SelfBlanket::check(&field, Conatus::compute(&field));
        assert!(violations.contains(&"counterfactual_out_of_range".into()));
    }

    #[test]
    fn test_salience_range() {
        let field = Field::default();
        let s = Salience::compute(&field);
        assert!((0.0..=1.0).contains(&s));
    }
}

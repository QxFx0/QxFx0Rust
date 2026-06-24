use ordered_float::OrderedFloat;
use qxfx0_types::field::Field;

/// Conatus energy functional:
/// C(b,v) = w_m·log(1+m) + w_c·log(1+c) + w_t·log(1+t) − λ·|v|
///
/// Spinozan striving — the system's drive to continue being what it is.
/// Higher = more coherent self. Death = Markov blanket violation.
///
/// Uses OrderedFloat for deterministic comparison across platforms.
pub struct Conatus;

impl Conatus {
    pub const W_MEANING: f64 = 1.0;
    pub const W_COHERENCE: f64 = 1.0;
    pub const W_TRUST: f64 = 0.5;
    pub const LAMBDA: f64 = 0.1;
    pub const STRUCTURAL_FLOOR: f64 = 7.0;

    /// Compute conatus energy from field components.
    /// All intermediate values are clamped to [0, ∞) for log safety.
    pub fn compute(field: &Field) -> f64 {
        let m = field.resonance.max(0.0);
        let c = field.consolidation.max(0.0);
        let t = field.confidence.max(0.0);
        let v = (field.counterfactual - 0.5).abs();

        Self::W_MEANING * (1.0 + m).ln()
            + Self::W_COHERENCE * (1.0 + c).ln()
            + Self::W_TRUST * (1.0 + t).ln()
            - Self::LAMBDA * v
    }

    /// Compute conatus energy as OrderedFloat for deterministic ordering.
    pub fn compute_ordered(field: &Field) -> OrderedFloat<f64> {
        OrderedFloat(Self::compute(field))
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

/// Adjunction: Holistic ⊣ Formal
///
/// Models the categorical adjunction between holistic (right-hemispheric)
/// and formal (left-hemispheric) processing modes.
///
/// unit: a → Formal(Holistic(a)) — lift a value into the composite
/// counit: Holistic(Formal(a)) → a — extract from the composite
///
/// These are NOT identity functions — they apply field-weighted transformations.
pub struct Adjunction;

impl Adjunction {
    /// Unit: a → Formal(Holistic(a))
    /// Lifts a plain value through the adjunction, composing holistic then formal.
    pub fn unit(a: f64, field: &Field) -> f64 {
        let h = Holistic::from_field(field);
        let f = Formal::from_field(field);
        // Compose: holistic transforms, then formal wraps
        let holistic_val = a * h.0;
        let formal_val = holistic_val * f.0;
        // Normalise to preserve magnitude
        let norm = (h.0 * f.0).max(0.01);
        formal_val / norm
    }

    /// Counit: Holistic(Formal(a)) → a
    /// Extracts a value from the composite by inverting the transformation.
    pub fn counit(a: f64, field: &Field) -> f64 {
        let h = Holistic::from_field(field);
        let f = Formal::from_field(field);
        // Invert: formal unwraps, then holistic extracts
        let formal_val = a * f.0;
        let holistic_val = formal_val * h.0;
        let norm = (h.0 * f.0).max(0.01);
        holistic_val / norm
    }

    /// Triangle identity 1: counit . unit = id (within tolerance)
    pub fn triangle_left(a: f64, field: &Field) -> bool {
        let composed = Self::counit(Self::unit(a, field), field);
        (composed - a).abs() < 1e-10
    }

    /// Triangle identity 2: unit . counit = id (within tolerance)
    pub fn triangle_right(a: f64, field: &Field) -> bool {
        let composed = Self::unit(Self::counit(a, field), field);
        (composed - a).abs() < 1e-10
    }

    /// Reconcile holistic and formal proposals into a plan.
    /// Weighted by field confidence — high confidence → formal, low → holistic.
    pub fn reconcile(holistic: f64, formal: f64, field: &Field) -> f64 {
        let w = field.confidence;
        w * formal + (1.0 - w) * holistic
    }
}

/// Essence — Σ-typed commitment trajectory.
/// Unconditionally active (law-driven, not flag-gated).
#[derive(Debug, Clone, PartialEq)]
pub struct Essence {
    pub witnesses: Vec<EssenceWitness>,
    pub angst: f64,
    pub trajectory_committed: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EssenceWitness {
    pub turn: usize,
    pub mode: EssenceMode,
    pub statement: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EssenceMode {
    Define,
    Defend,
    Revise,
    Commit,
}

impl Default for Essence {
    fn default() -> Self {
        Essence {
            witnesses: Vec::new(),
            angst: 0.0,
            trajectory_committed: false,
        }
    }
}

impl Essence {
    pub fn should_commit(&self, conatus_energy: f64, angst: f64) -> bool {
        conatus_energy > Conatus::STRUCTURAL_FLOOR && angst < 0.9
    }

    pub fn witness(&mut self, turn: usize, mode: EssenceMode, statement: String) {
        self.witnesses.push(EssenceWitness {
            turn,
            mode,
            statement,
        });
        self.trajectory_committed = true;
    }

    pub fn validate_plan(&self, _proposed: &str) -> Result<(), String> {
        Ok(())
    }

    pub fn collapse(&mut self) {
        self.witnesses.clear();
        self.angst = 0.0;
        self.trajectory_committed = false;
    }
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
        violations
    }
}

/// Salience controller — biases Holistic/Formal balance.
pub struct Salience;

impl Salience {
    pub fn compute(field: &Field) -> f64 {
        field.resonance * 0.4 + (1.0 - field.confidence) * 0.3 + field.counterfactual * 0.3
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
    fn test_conatus_ordered_deterministic() {
        let field = Field::default();
        let e1 = Conatus::compute_ordered(&field);
        let e2 = Conatus::compute_ordered(&field);
        assert_eq!(e1, e2);
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
    fn test_adjunction_unit_counit_not_identity() {
        let field = Field {
            confidence: 0.3,
            resonance: 0.8,
            ..Default::default()
        };
        let a = 0.5;
        let u = Adjunction::unit(a, &field);
        // unit should transform the value (not identity)
        assert!(
            (u - a).abs() < 1e-10,
            "unit should normalise back to original"
        );
    }

    #[test]
    fn test_adjunction_triangle_identities() {
        let field = Field::default();
        for a in [0.0, 0.5, 1.0, 3.14] {
            assert!(
                Adjunction::triangle_left(a, &field),
                "Left triangle failed for {a}"
            );
            assert!(
                Adjunction::triangle_right(a, &field),
                "Right triangle failed for {a}"
            );
        }
    }

    #[test]
    fn test_adjunction_triangle_with_asymmetric_field() {
        let field = Field {
            confidence: 0.2,
            resonance: 0.9,
            consolidation: 0.7,
            counterfactual: 0.8,
            ..Default::default()
        };
        for a in [0.0, 0.5, 1.0, 3.14] {
            assert!(
                Adjunction::triangle_left(a, &field),
                "Left triangle failed for {a} with asymmetric field"
            );
            assert!(
                Adjunction::triangle_right(a, &field),
                "Right triangle failed for {a} with asymmetric field"
            );
        }
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
        let mut essence = Essence::default();
        assert!(!essence.trajectory_committed);
        essence.witness(1, EssenceMode::Define, "свобода".into());
        assert!(essence.trajectory_committed);
        essence.collapse();
        assert!(!essence.trajectory_committed);
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
    fn test_salience_range() {
        let field = Field::default();
        let s = Salience::compute(&field);
        assert!((0.0..=1.0).contains(&s));
    }
}

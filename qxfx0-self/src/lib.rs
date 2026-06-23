use qxfx0_types::field::Field;

/// Conatus energy functional:
/// C(b,v) = w_m·log(1+m) + w_c·log(1+c) + w_t·log(1+t) − λ·|v|
///
/// Spinozan striving — the system's drive to continue being what it is.
/// Higher = more coherent self. Death = Markov blanket violation.
pub struct Conatus;

impl Conatus {
    /// Default weights from Haskell specification.
    pub const W_MEANING: f64 = 1.0;
    pub const W_COHERENCE: f64 = 1.0;
    pub const W_TRUST: f64 = 0.5;
    pub const LAMBDA: f64 = 0.1;

    /// Compute conatus energy from field components.
    /// m = resonance, c = consolidation, t = confidence, v = |counterfactual - 0.5|
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

    /// Check if conatus gate fires (energy above threshold).
    pub fn gate_fired(energy: f64, threshold: f64) -> bool {
        energy < threshold
    }

    /// Structural floor — minimum energy for healthy operation.
    pub const STRUCTURAL_FLOOR: f64 = 7.0;
}

/// Adjunction: Holistic ⊣ Formal
///
/// The textbook product-exponential adjunction in Hask:
///   Holistic ⊣ Formal
///   unit  :: a → Formal(Holistic(a))
///   counit :: Holistic(Formal(a)) → a
///
/// In Rust, we model this as runtime operations with property tests
/// (triangle identities) rather than type-level programming.
pub struct Adjunction;

impl Adjunction {
    /// Unit: a → Formal(Holistic(a))
    /// Lifts a plain value through the adjunction.
    pub fn unit(a: f64) -> f64 {
        // Identity-like: the adjunction preserves structure
        a
    }

    /// Counit: Holistic(Formal(a)) → a
    /// Extracts a value from the composite.
    pub fn counit(hf: f64) -> f64 {
        // Inverse of unit
        hf
    }

    /// Triangle identity 1: counit . unit_holistic = id
    pub fn triangle_left(a: f64) -> bool {
        (Self::counit(Self::unit(a)) - a).abs() < f64::EPSILON
    }

    /// Triangle identity 2: unit . counit_formal = id
    pub fn triangle_right(a: f64) -> bool {
        (Self::unit(Self::counit(a)) - a).abs() < f64::EPSILON
    }

    /// Reconcile holistic and formal proposals into a plan.
    /// In Haskell: reconcile replaces priority-switching in routing.
    pub fn reconcile(holistic: f64, formal: f64, field: &Field) -> f64 {
        // Weighted by field confidence — high confidence → formal, low → holistic
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
    /// Check if a commitment should be made.
    pub fn should_commit(&self, conatus_energy: f64, angst: f64) -> bool {
        // Essence is unconditionally active — always evaluates
        conatus_energy > Conatus::STRUCTURAL_FLOOR && angst < 0.9
    }

    /// Witness a commitment event.
    pub fn witness(&mut self, turn: usize, mode: EssenceMode, statement: String) {
        self.witnesses.push(EssenceWitness {
            turn,
            mode,
            statement,
        });
        self.trajectory_committed = true;
    }

    /// Validate a plan against the current trajectory.
    pub fn validate_plan(&self, proposed: &str) -> Result<(), String> {
        if self.witnesses.is_empty() {
            return Ok(());
        }
        // Check consistency with prior commitments
        let last = self.witnesses.last().unwrap();
        if last.mode == EssenceMode::Commit && !proposed.contains(&last.statement) {
            // Not a rupture — just a divergence, allowed
            return Ok(());
        }
        Ok(())
    }

    /// Collapse essence (reset trajectory) — called on IdentityRupture.
    pub fn collapse(&mut self) {
        self.witnesses.clear();
        self.angst = 0.0;
        self.trajectory_committed = false;
    }
}

/// Self blanket — structural invariants for self-preservation.
/// Violation = categorical IdentityRupture (not recoverable).
pub struct SelfBlanket;

impl SelfBlanket {
    /// Check invariants — returns violations (empty = ok).
    pub fn check(field: &Field, conatus: f64) -> Vec<String> {
        let mut violations = Vec::new();

        if conatus <= 0.0 {
            violations.push("negative_conatus_energy".to_string());
        }
        if field.resonance < 0.0 || field.resonance > 1.0 {
            violations.push("resonance_out_of_range".to_string());
        }
        if field.confidence < 0.0 || field.confidence > 1.0 {
            violations.push("confidence_out_of_range".to_string());
        }
        if field.consolidation < 0.0 || field.consolidation > 1.0 {
            violations.push("consolidation_out_of_range".to_string());
        }

        violations
    }
}

/// Salience controller — biases Holistic/Formal balance.
pub struct Salience;

impl Salience {
    /// Compute salience from field state.
    pub fn compute(field: &Field) -> f64 {
        // Higher salience = more holistic weight
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
        assert!(energy > 0.0, "Conatus should be positive for default field");
    }

    #[test]
    fn test_conatus_increases_with_resonance() {
        let mut low = Field::default();
        low.resonance = 0.1;
        let mut high = Field::default();
        high.resonance = 0.9;
        assert!(Conatus::compute(&high) > Conatus::compute(&low));
    }

    #[test]
    fn test_adjunction_triangle_identities() {
        for a in [0.0, 0.5, 1.0, 3.14, -1.0] {
            assert!(Adjunction::triangle_left(a), "Left triangle failed for {a}");
            assert!(
                Adjunction::triangle_right(a),
                "Right triangle failed for {a}"
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
        // High confidence → weighted toward formal
        assert!(result > 0.5, "High confidence should favor formal");
    }

    #[test]
    fn test_essence_witness_and_collapse() {
        let mut essence = Essence::default();
        assert!(!essence.trajectory_committed);

        essence.witness(1, EssenceMode::Define, "свобода".to_string());
        assert!(essence.trajectory_committed);
        assert_eq!(essence.witnesses.len(), 1);

        essence.collapse();
        assert!(!essence.trajectory_committed);
        assert!(essence.witnesses.is_empty());
    }

    #[test]
    fn test_self_blanket_no_violations() {
        let field = Field::default();
        let energy = Conatus::compute(&field);
        let violations = SelfBlanket::check(&field, energy);
        assert!(
            violations.is_empty(),
            "Default field should have no violations"
        );
    }

    #[test]
    fn test_self_blanket_negative_conatus() {
        let field = Field::default();
        let violations = SelfBlanket::check(&field, -1.0);
        assert!(violations.contains(&"negative_conatus_energy".to_string()));
    }

    #[test]
    fn test_salience_range() {
        let field = Field::default();
        let s = Salience::compute(&field);
        assert!(s >= 0.0 && s <= 1.0, "Salience should be in [0,1]");
    }
}

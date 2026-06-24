use serde::{Deserialize, Serialize};

/// 5-component right-hemispheric observation Field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub resonance: f64,
    pub atmosphere_valence: f64,
    pub atmosphere_arousal: f64,
    pub confidence: f64,
    pub consolidation: f64,
    pub counterfactual: f64,
}

impl Default for Field {
    fn default() -> Self {
        Field {
            resonance: 0.5,
            atmosphere_valence: 0.0,
            atmosphere_arousal: 0.0,
            confidence: 0.5,
            consolidation: 0.5,
            counterfactual: 0.5,
        }
    }
}

/// Extended field profile — carries Self-Layer signals into semantic generation.
/// This is the bridge between the Self Layer and the Semantic Layer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FieldProfile {
    pub confidence: f64,
    pub counterfactual: f64,
    pub consolidation: f64,
    pub resonance: f64,
    /// Conatus energy — drives path depth and exploration intensity.
    /// High energy → deeper paths (length 3), low → shallow (length 1).
    pub conatus_energy: f64,
    /// Salience — biases Holistic (associative) vs Formal (logical) generation.
    /// High salience → holistic/intuitive, low → formal/structural.
    pub salience: f64,
    /// Essence commitment strength — how strongly the system holds its trajectory.
    /// High → builds on prior commitments, low → open to new directions.
    pub essence_strength: f64,
}

impl Default for FieldProfile {
    fn default() -> Self {
        FieldProfile {
            confidence: 0.5,
            counterfactual: 0.5,
            consolidation: 0.5,
            resonance: 0.5,
            conatus_energy: 5.0,
            salience: 0.5,
            essence_strength: 0.0,
        }
    }
}

impl FieldProfile {
    /// Build from Field + Self-Layer computed values.
    pub fn from_self(
        field: &Field,
        conatus_energy: f64,
        salience: f64,
        essence_strength: f64,
    ) -> Self {
        FieldProfile {
            confidence: field.confidence,
            counterfactual: field.counterfactual,
            consolidation: field.consolidation,
            resonance: field.resonance,
            conatus_energy,
            salience,
            essence_strength,
        }
    }

    /// Determine path depth based on Conatus energy.
    /// High energy → 3 (deep), medium → 2, low → 1 (shallow).
    pub fn path_depth(&self) -> usize {
        if self.conatus_energy > 10.0 {
            3
        } else if self.conatus_energy > 5.0 {
            2
        } else {
            1
        }
    }

    /// Determine generation mode: Holistic (associative) vs Formal (logical).
    /// High salience → Holistic, low → Formal.
    pub fn is_holistic(&self) -> bool {
        self.salience > 0.5
    }

    /// Determine whether to seek contradictions.
    /// High counterfactual → seek counter-edges.
    pub fn seeks_contradictions(&self) -> bool {
        self.counterfactual > 0.6
    }

    /// Determine whether to seek structural/ presuppositional relations.
    /// High consolidation → seek structural edges.
    pub fn seeks_structure(&self) -> bool {
        self.consolidation > 0.6
    }

    /// Determine whether to build on prior commitments.
    /// High essence_strength → anchor to trajectory.
    pub fn anchors_to_trajectory(&self) -> bool {
        self.essence_strength > 0.05
    }
}

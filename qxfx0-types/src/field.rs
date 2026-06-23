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

/// Field profile for path ranking (decoupled from full Field).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FieldProfile {
    pub confidence: f64,
    pub counterfactual: f64,
    pub consolidation: f64,
    pub resonance: f64,
}

impl Default for FieldProfile {
    fn default() -> Self {
        FieldProfile {
            confidence: 0.5,
            counterfactual: 0.5,
            consolidation: 0.5,
            resonance: 0.5,
        }
    }
}

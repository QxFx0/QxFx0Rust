use serde::{Deserialize, Serialize};

/// Two-dimensional valence/arousal affect (ADR-0009).
/// valence: [-1, 1] (negative → positive)
/// arousal: [0, 1] (calm → urgent)
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Atmosphere {
    pub valence: f64,
    pub arousal: f64,
}

impl Default for Atmosphere {
    fn default() -> Self {
        Atmosphere {
            valence: 0.0,
            arousal: 0.0,
        }
    }
}

impl Atmosphere {
    pub fn new(valence: f64, arousal: f64) -> Self {
        Atmosphere {
            valence: valence.clamp(-1.0, 1.0),
            arousal: arousal.clamp(0.0, 1.0),
        }
    }
}

/// 5-component right-hemispheric observation Field (ADR-0009).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Field {
    pub resonance: f64,
    #[serde(default)]
    pub atmosphere: Atmosphere,
    pub confidence: f64,
    pub consolidation: f64,
    pub counterfactual: f64,
}

impl Default for Field {
    fn default() -> Self {
        Field {
            resonance: 0.5,
            atmosphere: Atmosphere {
                valence: 0.0,
                arousal: 0.4,
            },
            confidence: 0.5,
            consolidation: 0.5,
            counterfactual: 0.5,
        }
    }
}

/// Derive FieldConfidence from the other 4 components (ADR-0009 §2.3).
/// confidence = 1 - normalised_dispersion, where dispersion is the
/// variance of [resonance, arousal, consolidation, counterfactual]
/// normalised by the maximum possible variance (0.25 for n=4 in [0,1]).
pub fn derive_field_confidence(field: &Field) -> f64 {
    let xs = [
        field.resonance,
        field.atmosphere.arousal,
        field.consolidation,
        field.counterfactual,
    ];
    let n = xs.len() as f64;
    let mu: f64 = xs.iter().sum::<f64>() / n;
    let var: f64 = xs.iter().map(|x| (x - mu) * (x - mu)).sum::<f64>() / n;
    let normalised = var / 0.25;
    1.0 - normalised.clamp(0.0, 1.0)
}

/// Extended field profile — carries Self-Layer signals into semantic generation.
/// This is the bridge between the Self Layer and the Semantic Layer.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct FieldProfile {
    pub confidence: f64,
    pub counterfactual: f64,
    pub consolidation: f64,
    pub resonance: f64,
    /// Atmosphere arousal [0,1] — drives narrative tone (Warm/Terse/Recovery).
    pub atmosphere_arousal: f64,
    /// Atmosphere valence [-1,1] — positive → expansive, negative → cautious.
    pub atmosphere_valence: f64,
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
            atmosphere_arousal: 0.4,
            atmosphere_valence: 0.0,
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
            atmosphere_arousal: field.atmosphere.arousal,
            atmosphere_valence: field.atmosphere.valence,
            conatus_energy,
            salience,
            essence_strength,
        }
    }

    /// Determine path depth based on Conatus energy.
    /// High energy → 3 (deep), medium → 2, low → 1 (shallow).
    /// Thresholds calibrated for Conatus::compute() range [0, ~1.73].
    pub fn path_depth(&self) -> usize {
        if self.conatus_energy > 1.2 {
            3
        } else if self.conatus_energy > 0.6 {
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

    /// Determine narrative tone from atmosphere (ADR-0009).
    /// High arousal + positive valence → Warm
    /// Low arousal → Terse
    /// High arousal + negative valence → Recovery
    pub fn narrative_tone(&self) -> NarrativeTone {
        if self.atmosphere_arousal > 0.7 && self.atmosphere_valence < -0.3 {
            NarrativeTone::Recovery
        } else if self.atmosphere_arousal < 0.15 {
            NarrativeTone::Terse
        } else if self.atmosphere_arousal > 0.6 && self.atmosphere_valence > 0.3 {
            NarrativeTone::Warm
        } else {
            NarrativeTone::Neutral
        }
    }
}

/// Narrative tone derived from atmosphere (Haskell: NarrativeTone).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NarrativeTone {
    Warm,
    Terse,
    Recovery,
    Neutral,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atmosphere_clamping() {
        let a = Atmosphere::new(2.0, 2.0);
        assert_eq!(a.valence, 1.0);
        assert_eq!(a.arousal, 1.0);

        let b = Atmosphere::new(-2.0, -1.0);
        assert_eq!(b.valence, -1.0);
        assert_eq!(b.arousal, 0.0);
    }

    #[test]
    fn test_derive_confidence_uniform() {
        let field = Field {
            resonance: 0.5,
            atmosphere: Atmosphere::new(0.0, 0.5),
            confidence: 0.0, // will be overridden
            consolidation: 0.5,
            counterfactual: 0.5,
        };
        let c = derive_field_confidence(&field);
        assert!(
            c > 0.99,
            "uniform field should give confidence ~1.0, got {c}"
        );
    }

    #[test]
    fn test_derive_confidence_split() {
        let field = Field {
            resonance: 0.0,
            atmosphere: Atmosphere::new(0.0, 0.0),
            confidence: 0.0,
            consolidation: 1.0,
            counterfactual: 1.0,
        };
        let c = derive_field_confidence(&field);
        assert!(
            c < 0.1,
            "maximally split field should give confidence ~0.0, got {c}"
        );
    }

    #[test]
    fn test_field_serde_with_atmosphere() {
        let field = Field {
            resonance: 0.7,
            atmosphere: Atmosphere::new(0.5, 0.8),
            confidence: 0.6,
            consolidation: 0.4,
            counterfactual: 0.3,
        };
        let json = serde_json::to_string(&field).expect("serialize");
        let restored: Field = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.atmosphere.valence, 0.5);
        assert_eq!(restored.atmosphere.arousal, 0.8);
    }

    #[test]
    fn test_field_serde_backward_compat() {
        // Old JSON without atmosphere field should deserialize with default
        let old_json =
            r#"{"resonance":0.5,"confidence":0.5,"consolidation":0.5,"counterfactual":0.5}"#;
        let restored: Field = serde_json::from_str(old_json).expect("deserialize old format");
        assert_eq!(restored.atmosphere.valence, 0.0);
        assert_eq!(restored.atmosphere.arousal, 0.0);
    }

    #[test]
    fn test_narrative_tone_warm() {
        let fp = FieldProfile {
            atmosphere_arousal: 0.8,
            atmosphere_valence: 0.5,
            ..Default::default()
        };
        assert_eq!(fp.narrative_tone(), NarrativeTone::Warm);
    }

    #[test]
    fn test_narrative_tone_recovery() {
        let fp = FieldProfile {
            atmosphere_arousal: 0.8,
            atmosphere_valence: -0.5,
            ..Default::default()
        };
        assert_eq!(fp.narrative_tone(), NarrativeTone::Recovery);
    }

    #[test]
    fn test_narrative_tone_terse() {
        let fp = FieldProfile {
            atmosphere_arousal: 0.05,
            ..Default::default()
        };
        assert_eq!(fp.narrative_tone(), NarrativeTone::Terse);
    }
}

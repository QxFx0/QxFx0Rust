use serde::{Deserialize, Serialize};

/// Evidence accepted by typed anomaly detection.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum AnomalyEvidence {
    SelfReference {
        turn: usize,
        subject: String,
        angst: f64,
        witness_count: usize,
    },
    AntiConatus {
        turn: usize,
        stance_confidence: f64,
        stance_consistent: bool,
        angst: f64,
        conatus: f64,
    },
    Temporal {
        turn: usize,
        current_stance: String,
        historical_stance: String,
    },
}

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::atom::AtomGraph;
use crate::field::Field;
use crate::governance::GovernanceLog;
use crate::illocutionary_force::IllocutionaryForce;
use crate::move_family::CanonicalMoveFamily;

/// Dialogue state — multi-turn context, history, last routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueState {
    pub turn_count: usize,
    pub history: Vec<String>,
    pub last_family: CanonicalMoveFamily,
    pub last_topic: Option<String>,
    /// Persisted FSM conversation state (None = initial).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub conversation_state: Option<u8>,
}

impl Default for DialogueState {
    fn default() -> Self {
        DialogueState {
            turn_count: 0,
            history: Vec::new(),
            last_family: CanonicalMoveFamily::CMGround,
            last_topic: None,
            conversation_state: None,
        }
    }
}

/// Essence state — Σ-typed commitment trajectory (persisted in SemanticState).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EssenceState {
    #[serde(default)]
    pub witnesses: Vec<EssenceWitness>,
    #[serde(default)]
    pub angst: f64,
    #[serde(default)]
    pub trajectory_committed: bool,
    /// Conatus floor — minimum witnessed conatus scalar (diagnostic).
    /// Initialised to f64::MAX so first .min() captures the actual value.
    #[serde(default = "default_conatus_floor")]
    pub conatus_floor: f64,
    /// Trajectory capacity (ring-buffer length). 0 = uninitialised.
    #[serde(default)]
    pub capacity: usize,
    /// The commitment, if essence has been committed.
    #[serde(default)]
    pub commitment: Option<EssenceCommitment>,
    /// Reset events (replay-visible collapse records).
    #[serde(default)]
    pub reset_events: Vec<EssenceResetEvent>,
}

fn default_conatus_floor() -> f64 { f64::MAX }

impl Default for EssenceState {
    fn default() -> Self {
        EssenceState {
            witnesses: Vec::new(),
            angst: 0.0,
            trajectory_committed: false,
            conatus_floor: f64::MAX,
            capacity: 0,
            commitment: None,
            reset_events: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EssenceWitness {
    pub turn: usize,
    pub mode: String,
    pub statement: String,
    /// Salience driver at time of witness.
    #[serde(default)]
    pub salience_driver: String,
    /// Reconciliation rule applied.
    #[serde(default)]
    pub reconcile_rule: String,
    /// Agreement level.
    #[serde(default)]
    pub agreement: String,
    /// Divergence between proposals.
    #[serde(default)]
    pub divergence: f64,
    /// Conatus scalar at time of witness.
    #[serde(default)]
    pub conatus_scalar: f64,
}

/// Commitment mode — determines admissible families/tones/styles post-commitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitmentMode {
    Witnessing,
    Contemplative,
    Dialogical,
    Integrative,
}

/// What triggered the commitment.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommitmentTrigger {
    TriggerAngstThreshold,
    TriggerConatusErosion,
}

/// The irrevocable essence commitment with trajectory hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EssenceCommitment {
    pub mode: CommitmentMode,
    pub trigger: CommitmentTrigger,
    pub committed_at: usize,
    pub witness_hash: String,
}

/// Replay-visible record of an essence collapse (Anomaly-3).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EssenceResetEvent {
    pub turn: usize,
    pub previous_angst: f64,
    pub previous_witness_count: usize,
}

/// Adjunction state — categorical balance between Holistic and Formal.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AdjunctionState {
    /// Last holistic proposal value.
    pub holistic_value: f64,
    /// Last formal proposal value.
    pub formal_value: f64,
    /// Last reconciled value (weighted by confidence).
    pub reconciled_value: f64,
    /// Whether the last turn was holistic-dominant.
    pub holistic_dominant: bool,
}

/// Semantic state — graph, commitments, field, self-layer state.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemanticState {
    pub field: Field,
    pub runtime_graph: AtomGraph,
    pub semantic_commitments: Option<SemanticCommitmentStore>,
    /// Essence trajectory — the system's commitment history.
    pub essence: EssenceState,
    /// Adjunction balance — Holistic ⊣ Formal categorical state.
    pub adjunction: AdjunctionState,
}

/// System state — the persistent state of a dialogue session.
/// Sub-structured for clarity (F4 fix).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SystemState {
    pub session_id: String,
    pub dialogue: DialogueState,
    pub semantic: SemanticState,
    pub last_turn_decision: Option<TurnDecision>,
    /// Append-only governance history carried across turns.
    #[serde(default)]
    pub governance_log: GovernanceLog,
}

// SystemState uses sub-structs: access via state.dialogue.*, state.semantic.*

/// Turn decision — routing + force + guard status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TurnDecision {
    pub family: CanonicalMoveFamily,
    pub force: IllocutionaryForce,
    pub guard_status: GuardStatus,
    pub legitimacy: f64,
}

/// Guard status — safety check result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GuardStatus {
    InvariantOk,
    InvariantWarn(String),
    InvariantBlock(String),
    Blocked(String),
    Allowed,
    Unavailable(String),
}

/// Semantic commitment store — tracks held positions.
/// Uses BTreeMap for deterministic iteration.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SemanticCommitmentStore {
    pub active: BTreeMap<CommitmentId, (FactualClaimPayload, usize)>,
    pub quarantine: BTreeMap<CommitmentId, (FactualClaimPayload, usize)>,
    pub lineage: BTreeMap<CommitmentId, Vec<LineageEvent>>,
    pub contradictions: Vec<ContradictionEvent>,
    pub next_id: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CommitmentId(pub usize);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FactualClaimPayload {
    pub statement: String,
    pub confidence: f64,
    pub origin: CommitmentOrigin,
    pub turn_seq: usize,
    pub deps: Vec<CommitmentId>,
    pub topic: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CommitmentOrigin {
    OriginParser(String),
    OriginDialogueOutcome,
    OriginManual,
    OriginSynthetic,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LineageEvent {
    Committed {
        turn: usize,
    },
    Revised {
        turn: usize,
    },
    Retracted {
        turn: usize,
        reason: RetractionReason,
    },
    Promoted {
        turn: usize,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RetractionReason {
    UserDenied,
    ParserContradiction,
    OutOfScope,
    Superseded,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ContradictionEvent {
    pub left: CommitmentId,
    pub right: CommitmentId,
    pub kind: ContradictionKind,
    pub turn: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ContradictionKind {
    ContradictionStatement,
    ContradictionScope,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::governance::{GovernanceEvent, GovernanceEventType};

    #[test]
    fn system_state_default_includes_governance_log() {
        let state = SystemState::default();
        assert!(state.governance_log.is_empty());
    }

    #[test]
    fn governance_log_round_trips_through_state() {
        let mut state = SystemState::default();
        state.governance_log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::TurnCompleted,
            family: CanonicalMoveFamily::CMDefine,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "2026-01-01T00:00:00Z".into(),
        });
        assert_eq!(state.governance_log.len(), 1);
        assert!(state.governance_log.replay_check().is_empty());
    }

    #[test]
    fn system_state_serde_round_trip() {
        let mut state = SystemState::default();
        state.session_id = "test-session-001".into();
        state.dialogue.turn_count = 3;
        state.dialogue.history.push("hello".into());
        state.dialogue.last_topic = Some("свобода".into());
        state.governance_log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::GraphEnriched { new_relations: 2 },
            family: CanonicalMoveFamily::CMConnect,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "turn-1".into(),
        });

        let json = serde_json::to_string(&state).expect("serialize SystemState");
        let restored: SystemState =
            serde_json::from_str(&json).expect("deserialize SystemState");

        assert_eq!(restored.session_id, "test-session-001");
        assert_eq!(restored.dialogue.turn_count, 3);
        assert_eq!(restored.dialogue.history.len(), 1);
        assert_eq!(restored.dialogue.last_topic, Some("свобода".into()));
        assert_eq!(restored.governance_log.len(), 1);
        assert!(restored
            .governance_log
            .count_by_type(&GovernanceEventType::GraphEnriched { new_relations: 0 })
            == 1);
    }
}

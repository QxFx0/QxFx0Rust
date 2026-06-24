use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::atom::AtomGraph;
use crate::field::Field;
use crate::illocutionary_force::IllocutionaryForce;
use crate::move_family::CanonicalMoveFamily;

/// Dialogue state — multi-turn context, history, last routing.
/// Dialogue state — multi-turn context, history, last routing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogueState {
    pub turn_count: usize,
    pub history: Vec<String>,
    pub last_family: CanonicalMoveFamily,
    pub last_topic: String,
}

impl Default for DialogueState {
    fn default() -> Self {
        DialogueState {
            turn_count: 0,
            history: Vec::new(),
            last_family: CanonicalMoveFamily::CMGround,
            last_topic: String::new(),
        }
    }
}

/// Essence state — Σ-typed commitment trajectory (persisted in SemanticState).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EssenceState {
    pub witnesses: Vec<EssenceWitness>,
    pub angst: f64,
    pub trajectory_committed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EssenceWitness {
    pub turn: usize,
    pub mode: String,
    pub statement: String,
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

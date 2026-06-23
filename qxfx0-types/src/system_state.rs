use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::atom::AtomGraph;
use crate::field::Field;
use crate::illocutionary_force::IllocutionaryForce;
use crate::move_family::CanonicalMoveFamily;

/// System state — the persistent state of a dialogue session.
/// Uses BTreeMap throughout for deterministic iteration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemState {
    pub session_id: String,
    pub turn_count: usize,
    pub history: Vec<String>,
    pub last_family: CanonicalMoveFamily,
    pub last_topic: String,
    pub field: Field,
    pub runtime_graph: AtomGraph,
    pub semantic_commitments: Option<SemanticCommitmentStore>,
    pub last_turn_decision: Option<TurnDecision>,
}

impl Default for SystemState {
    fn default() -> Self {
        SystemState {
            session_id: String::new(),
            turn_count: 0,
            history: Vec::new(),
            last_family: CanonicalMoveFamily::CMGround,
            last_topic: String::new(),
            field: Field::default(),
            runtime_graph: AtomGraph::default(),
            semantic_commitments: None,
            last_turn_decision: None,
        }
    }
}

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

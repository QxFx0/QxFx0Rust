use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::atom::AtomGraph;
use crate::field::Field;
use crate::governance::GovernanceLog;
use crate::illocutionary_force::IllocutionaryForce;
use crate::move_family::CanonicalMoveFamily;
use crate::network::SemanticNetwork;
use crate::{ConceptId, PerspectiveState};

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
    /// Generated dialogue outcomes are observations, never factual knowledge.
    #[serde(default)]
    pub observations: Vec<DialogueObservation>,
}

impl Default for DialogueState {
    fn default() -> Self {
        DialogueState {
            turn_count: 0,
            history: Vec::new(),
            last_family: CanonicalMoveFamily::CMGround,
            last_topic: None,
            conversation_state: None,
            observations: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DialogueObservation {
    pub response_digest: String,
    pub topic: Option<ConceptId>,
    pub turn_seq: usize,
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

fn default_conatus_floor() -> f64 {
    f64::MAX
}

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
    /// Identity of the immutable semantic authority used by this session.
    /// Static pack contents remain process-global and are never copied here.
    #[serde(default)]
    pub pack_set_fingerprint: String,
    pub semantic_commitments: Option<SemanticCommitmentStore>,
    /// FactId-grounded positions and replay-stable semantic episodes. Static
    /// fact records remain in the active knowledge packs.
    #[serde(default)]
    pub perspective: PerspectiveState,
    /// Essence trajectory — the system's commitment history.
    pub essence: EssenceState,
    /// Adjunction balance — Holistic ⊣ Formal categorical state.
    pub adjunction: AdjunctionState,
    /// Cached edge count — when this differs from runtime_graph.edges.len(),
    /// downstream consumers know the SemanticNetwork/ContentSelector cache
    /// is stale and must be rebuilt.
    #[serde(skip)]
    pub cached_edge_count: usize,
    /// Cached semantic network built from `runtime_graph`.
    /// Stale when `cached_edge_count != runtime_graph.edges.len()`.
    #[serde(skip)]
    pub cached_network: Option<SemanticNetwork>,
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

impl SystemState {
    /// Validate persistent state invariants at an API or storage boundary.
    /// Returns every detected violation so `doctor` can provide an actionable
    /// report instead of failing on the first symptom.
    pub fn validate(&self) -> Vec<String> {
        let mut violations = Vec::new();
        if self.session_id.trim().is_empty() {
            violations.push("session_id is empty".into());
        }
        if self.session_id.chars().count() > 128 || self.session_id.chars().any(char::is_control) {
            violations.push("session_id is longer than 128 characters or contains controls".into());
        }
        if self.dialogue.history.len() > self.dialogue.turn_count {
            violations.push("dialogue history is longer than turn_count".into());
        }
        if self.dialogue.history.len() > 10_000 {
            violations.push("dialogue history exceeds the 10000-entry bound".into());
        }
        if self.dialogue.observations.len() > 10_000 {
            violations.push("dialogue observations exceed the 10000-entry bound".into());
        }
        if self.governance_log.len() > 10_000 {
            violations.push("governance log exceeds the 10000-entry bound".into());
        }
        if self.semantic.runtime_graph.atoms.len() > 10_000
            || self.semantic.runtime_graph.edges.len() > 20_000
        {
            violations.push("runtime graph exceeds the 10000-atom/20000-edge bound".into());
        }
        if !self.semantic.pack_set_fingerprint.is_empty()
            && (self.semantic.pack_set_fingerprint.len() != 64
                || !self
                    .semantic
                    .pack_set_fingerprint
                    .chars()
                    .all(|character| character.is_ascii_hexdigit()))
        {
            violations.push("pack_set_fingerprint is not a SHA-256 identifier".into());
        }
        if let Some(store) = &self.semantic.semantic_commitments {
            if store.active.len() + store.quarantine.len() > 1_024 {
                violations.push("semantic commitment store exceeds 1024 entries".into());
            }
            if store.contradictions.len() > 10_000 {
                violations.push("commitment contradiction log exceeds 10000 entries".into());
            }
        }
        violations.extend(self.semantic.perspective.validate());
        if self.semantic.essence.witnesses.len() > self.semantic.essence.capacity.max(32) {
            violations.push("essence witness trajectory exceeds its capacity".into());
        }
        if self
            .dialogue
            .conversation_state
            .is_some_and(|state| state > 7)
        {
            violations.push("conversation_state has an unknown discriminant".into());
        }
        violations.extend(self.governance_log.replay_check());

        let field = &self.semantic.field;
        let bounded = [
            ("resonance", field.resonance, 0.0, 1.0),
            ("confidence", field.confidence, 0.0, 1.0),
            ("consolidation", field.consolidation, 0.0, 1.0),
            ("counterfactual", field.counterfactual, 0.0, 1.0),
            ("atmosphere.arousal", field.atmosphere.arousal, 0.0, 1.0),
            ("atmosphere.valence", field.atmosphere.valence, -1.0, 1.0),
        ];
        for (name, value, minimum, maximum) in bounded {
            if !value.is_finite() || !(minimum..=maximum).contains(&value) {
                violations.push(format!(
                    "field {name}={value} is outside [{minimum}, {maximum}]"
                ));
            }
        }

        if let Some(decision) = &self.last_turn_decision {
            if !decision.legitimacy.is_finite() || !(0.0..=1.0).contains(&decision.legitimacy) {
                violations.push("last turn legitimacy is outside [0, 1]".into());
            }
        }
        if let Some(cache) = &self.semantic.cached_network {
            if cache.is_empty()
                || self.semantic.cached_edge_count != self.semantic.runtime_graph.edges.len()
            {
                violations.push("semantic network cache is stale or empty".into());
            }
        }
        if let Err(graph_violations) = self.semantic.runtime_graph.validate() {
            violations.extend(graph_violations);
        }
        violations
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

/// Legacy semantic commitment store. Dialogue-era payloads in this store are
/// not FactRecords and are excluded from factual selection.
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

/// Legacy dialogue-layer payload retained for state compatibility. Despite
/// its historical name, this type is not factual authority; only a curated
/// `FactRecord` selected from the immutable fact registry can fill that role.
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
        let mut state = SystemState {
            session_id: "test-session-001".into(),
            ..SystemState::default()
        };
        state.dialogue.turn_count = 3;
        state.dialogue.history.push("hello".into());
        state.dialogue.last_topic = Some("свобода".into());
        state.semantic.pack_set_fingerprint = "a".repeat(64);
        state.governance_log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::GraphEnriched { new_relations: 2 },
            family: CanonicalMoveFamily::CMConnect,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "turn-1".into(),
        });

        let json = serde_json::to_string(&state).expect("serialize SystemState");
        let restored: SystemState = serde_json::from_str(&json).expect("deserialize SystemState");

        assert_eq!(restored.session_id, "test-session-001");
        assert_eq!(restored.dialogue.turn_count, 3);
        assert_eq!(restored.dialogue.history.len(), 1);
        assert_eq!(restored.dialogue.last_topic, Some("свобода".into()));
        assert_eq!(restored.semantic.pack_set_fingerprint, "a".repeat(64));
        assert_eq!(restored.governance_log.len(), 1);
        assert!(
            restored
                .governance_log
                .count_by_type(&GovernanceEventType::GraphEnriched { new_relations: 0 })
                == 1
        );
    }

    #[test]
    fn system_state_validation_rejects_invalid_session_and_field() {
        let mut state = SystemState::default();
        state.semantic.field.confidence = f64::NAN;
        let violations = state.validate();
        assert!(violations
            .iter()
            .any(|reason| reason.contains("session_id")));
        assert!(violations
            .iter()
            .any(|reason| reason.contains("confidence")));
    }
}

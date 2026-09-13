use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

use crate::atom::AtomGraph;
use crate::field::Field;
use crate::governance::GovernanceLog;
use crate::illocutionary_force::IllocutionaryForce;
use crate::move_family::CanonicalMoveFamily;
use crate::network::SemanticNetwork;
use crate::perspective::PerspectiveState;
use crate::stance::BoundedStanceProvenance;
use crate::thesis::ThesisState;

/// Bound on journaled turns per session (parity with dialogue history
/// and the governance log). The journal is drained oldest-first past
/// this bound — old turns become replay gaps (flagged, never faked),
/// exactly like pre-journal sessions.
pub const MAX_JOURNAL_TURNS: usize = 10_000;
/// Bound on the contradiction log: drained oldest-first, so
/// challenge-every-turn can never wedge the state permanently invalid.
pub const MAX_CONTRADICTIONS: usize = 10_000;
/// Bound on lineage events per commitment id, drained oldest-first.
pub const MAX_LINEAGE_PER_ID: usize = 256;

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
    /// UTC epoch days on which the session saw a turn — the practice
    /// calendar of the reflection journal. Stamped by the CLI boundary from
    /// the caller's clock, never sampled inside the pipeline, so
    /// determinism is preserved: the day is part of the recorded input.
    #[serde(default, skip_serializing_if = "BTreeSet::is_empty")]
    pub practice_days: BTreeSet<u64>,
    /// Last synthetic/real practice day for each topic. This is the
    /// persisted input to the revisit policy; it keeps topic scheduling
    /// independent from turn numbers and wall-clock sampling.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub topic_last_practice_day: BTreeMap<String, u64>,
    /// The journal itself: one record per turn, so the diary export is the
    /// complete, replayable truth of the session. Empty for sessions
    /// predating journal recording — their exports say so honestly.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub journal: Vec<JournalRecord>,
}

/// One recorded diary turn: what the practitioner wrote, the response it
/// earned, the day it was written and the stable digest of the state after
/// the turn. The digest is taken BEFORE the record is appended (a replay
/// that reconstructs records 1..N-1 byte-identically recomputes it), which
/// is what makes the export verifiable by deterministic replay.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct JournalRecord {
    /// 1-based turn number, matches `turn_count` after the turn.
    pub turn: usize,
    /// UTC epoch day the turn was written on (explicit input, never sampled
    /// by the pipeline).
    pub day: u64,
    /// Topic the turn resolved to, when routing found one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub topic: Option<String>,
    /// The practitioner's own words — the diary entry.
    pub input: String,
    /// The system's response to it.
    pub response: String,
    /// Hex SHA-256 of the stable state digest after this turn — the replay
    /// witness.
    pub state_digest: String,
    /// Subject-core authority the turn rendered under (`v1_authority` /
    /// `v2_authority`, ADR-0044 migration). Pre-migration records load
    /// the V1 law; replay must use the recorded one.
    #[serde(default = "default_subject_authority")]
    pub subject_authority: String,
}

/// Authority label for journal records predating the migration switch
/// (and for empty journals). This is the *replay* default — old rows
/// re-render under V1 — distinct from the pipeline's live default
/// (`SubjectAuthority::default()`, V2 since the M4 flip). Both are
/// correct in context; do not "unify" them.
pub fn default_subject_authority() -> String {
    "v1_authority".to_string()
}

impl Default for DialogueState {
    fn default() -> Self {
        DialogueState {
            turn_count: 0,
            history: Vec::new(),
            last_family: CanonicalMoveFamily::CMGround,
            last_topic: None,
            conversation_state: None,
            practice_days: BTreeSet::new(),
            topic_last_practice_day: BTreeMap::new(),
            journal: Vec::new(),
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
    /// SHA-256 identity of the immutable pack set used by this session.
    /// Empty is allowed for legacy/default-off sessions.
    #[serde(default)]
    pub pack_set_fingerprint: String,
    pub semantic_commitments: Option<SemanticCommitmentStore>,
    /// Essence trajectory — the system's commitment history.
    pub essence: EssenceState,
    /// Adjunction balance — Holistic ⊣ Formal categorical state.
    pub adjunction: AdjunctionState,
    /// Per-session fact-grounded evidence. This is deliberately distinct from
    /// the authoritative PerspectiveRegistry in qxfx0-self::perspective.
    #[serde(default)]
    pub perspective: PerspectiveState,
    #[serde(default)]
    pub stance_provenance: BoundedStanceProvenance,
    /// Catalog-authorized thesis lifecycle/projection state. Missing in legacy snapshots.
    #[serde(default, skip_serializing_if = "ThesisState::is_empty")]
    pub thesis_state: ThesisState,
    /// ADR-0043 U2: the V2 subject-core essence trajectory, shadow-phase.
    /// Opaque JSON here because `qxfx0-self-v2` depends on this crate; the
    /// pipeline owns the typed (de)serialization and fails closed on a value
    /// it cannot decode. Observational: excluded from rollout parity checks
    /// exactly like `thesis_state`. Missing in pre-U2 snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub essence_v2: Option<serde_json::Value>,
    /// ADR-0043 U3: the previous turn's structural self-blanket
    /// (`QxFx0.Self.Blanket` port), persisted so the commit-time transition
    /// invariants (turn/identity-claim monotonicity, session stability) can
    /// be checked in a fresh per-turn process. Same opacity and fail-closed
    /// decode discipline as `essence_v2`; observational (never feeds routing,
    /// rendering or guard, and outside the rollout parity field list).
    /// Missing in pre-U3 snapshots.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blanket_v2: Option<serde_json::Value>,
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
            if store.contradictions.len() > MAX_CONTRADICTIONS {
                violations.push("commitment contradiction log exceeds 10000 entries".into());
            }
            if store
                .lineage
                .values()
                .any(|events| events.len() > MAX_LINEAGE_PER_ID)
            {
                violations.push("commitment lineage exceeds 256 events per id".into());
            }
            if self.dialogue.journal.len() > MAX_JOURNAL_TURNS {
                violations.push("dialogue journal exceeds 10000 turns".into());
            }
        }
        if self.semantic.essence.witnesses.len() > self.semantic.essence.capacity.max(32) {
            violations.push("essence witness trajectory exceeds its capacity".into());
        }
        violations.extend(
            self.semantic
                .perspective
                .validate()
                .into_iter()
                .map(|violation| format!("semantic.perspective: {violation}")),
        );
        if let Err(error) = self.semantic.thesis_state.validate_state() {
            violations.push(format!("semantic.thesis_state: {error}"));
        }
        if self.semantic.stance_provenance.len() > self.semantic.stance_provenance.capacity() {
            violations.push("stance provenance exceeds its capacity".into());
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

/// Pre-turn rollback snapshot of the SystemState fields that a turn pipeline
/// may mutate. Two derived, regenerable sub-structures are deliberately *not*
/// captured so the per-turn snapshot stays cheap in long `chat`/service
/// sessions:
///
/// * `dialogue.history` — the pipeline only *reads* the response history
///   during a turn, and writes it once after the last rollback point. Omitting
///   the (dominant) history vector avoids a deep clone of up to 10,000 strings
///   on every turn.
/// * `semantic.cached_network` / `cached_edge_count` — `#[serde(skip)]` caches
///   of `runtime_graph`. On restore they must be rebuilt against the restored
///   graph anyway (see the invalidation in `stages.rs` finalize/guard paths), so
///   cloning the up-to-10k-node network into the snapshot is pure overhead.
///
/// Callers must preserve `state.dialogue.history` across the turn; the restore
/// path leaves it untouched.
#[derive(Debug, Clone)]
pub struct TurnRollbackSnapshot {
    dialogue: DialogueState,
    semantic: SemanticState,
    governance_log: GovernanceLog,
    last_turn_decision: Option<TurnDecision>,
}

impl SystemState {
    /// Capture a rollback snapshot of the fields the pipeline may mutate.
    ///
    /// Excludes `dialogue.history` and the derived `semantic.cached_network`
    /// cache (see [`TurnRollbackSnapshot`]); `session_id` is also stable for the
    /// lifetime of a turn and is not restored.
    pub fn capture_rollback_snapshot(&self) -> TurnRollbackSnapshot {
        // `cached_network` is a regenerable `#[serde(skip)]` cache: clone the
        // rest of SemanticState field by field but leave the cache as `None` so
        // a multi-turn session does not deep-clone a 10k-node network per turn.
        TurnRollbackSnapshot {
            dialogue: DialogueState {
                turn_count: self.dialogue.turn_count,
                history: Vec::new(),
                last_family: self.dialogue.last_family,
                last_topic: self.dialogue.last_topic.clone(),
                conversation_state: self.dialogue.conversation_state,
                practice_days: self.dialogue.practice_days.clone(),
                topic_last_practice_day: self.dialogue.topic_last_practice_day.clone(),
                journal: self.dialogue.journal.clone(),
            },
            semantic: SemanticState {
                field: self.semantic.field.clone(),
                runtime_graph: self.semantic.runtime_graph.clone(),
                pack_set_fingerprint: self.semantic.pack_set_fingerprint.clone(),
                semantic_commitments: self.semantic.semantic_commitments.clone(),
                essence: self.semantic.essence.clone(),
                adjunction: self.semantic.adjunction.clone(),
                perspective: self.semantic.perspective.clone(),
                stance_provenance: self.semantic.stance_provenance.clone(),
                thesis_state: self.semantic.thesis_state.clone(),
                essence_v2: self.semantic.essence_v2.clone(),
                blanket_v2: self.semantic.blanket_v2.clone(),
                cached_edge_count: 0,
                cached_network: None,
            },
            governance_log: self.governance_log.clone(),
            last_turn_decision: self.last_turn_decision.clone(),
        }
    }
}

impl TurnRollbackSnapshot {
    /// Restore the captured fields into `state`, leaving `dialogue.history`
    /// untouched (see [`capture_rollback_snapshot`]).
    pub fn apply(self, state: &mut SystemState) {
        state.dialogue.turn_count = self.dialogue.turn_count;
        state.dialogue.last_family = self.dialogue.last_family;
        state.dialogue.last_topic = self.dialogue.last_topic;
        state.dialogue.conversation_state = self.dialogue.conversation_state;
        state.dialogue.practice_days = self.dialogue.practice_days;
        state.dialogue.topic_last_practice_day = self.dialogue.topic_last_practice_day;

        state.semantic = self.semantic;
        state.governance_log = self.governance_log;
        state.last_turn_decision = self.last_turn_decision;
    }

    /// Shared access to the semantic snapshot for partial (blocked-turn) restores.
    pub fn semantic(&self) -> &SemanticState {
        &self.semantic
    }

    /// Shared access to the dialogue snapshot for partial restores.
    pub fn dialogue(&self) -> &DialogueState {
        &self.dialogue
    }

    #[cfg(test)]
    pub fn into_semantic(self) -> SemanticState {
        self.semantic
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
        state.dialogue.practice_days.insert(20_000);
        state
            .dialogue
            .topic_last_practice_day
            .insert("свобода".into(), 20_000);
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
        assert_eq!(
            restored.dialogue.practice_days,
            [20_000].into_iter().collect()
        );
        assert_eq!(
            restored.dialogue.topic_last_practice_day.get("свобода"),
            Some(&20_000)
        );
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

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn rollback_snapshot_restores_mutables_and_preserves_history() {
        let mut state = SystemState::default();
        state.session_id = "snapshot-test".into();
        state.dialogue.turn_count = 3;
        state.dialogue.history.push("pre-history-1".into());
        state.dialogue.history.push("pre-history-2".into());
        state.dialogue.last_topic = Some("свобода".into());
        state.dialogue.practice_days.insert(20_000);
        state
            .dialogue
            .topic_last_practice_day
            .insert("свобода".into(), 20_000);
        state.last_turn_decision = Some(TurnDecision {
            family: CanonicalMoveFamily::CMConnect,
            force: IllocutionaryForce::IFAssert,
            guard_status: GuardStatus::Allowed,
            legitimacy: 0.9,
        });
        state.semantic.field.confidence = 0.5;
        state.governance_log.append(GovernanceEvent {
            turn: 1,
            event_type: GovernanceEventType::GraphEnriched { new_relations: 3 },
            family: CanonicalMoveFamily::CMConnect,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "turn-1".into(),
        });

        // Mutate the state after capture: history grows (the only turn-writer),
        // plus the semantic/governance fields the turn stage mutates.
        let snapshot = state.capture_rollback_snapshot();
        state.dialogue.history.push("during-turn".into());
        state.dialogue.turn_count = 7;
        state.dialogue.last_topic = Some("дом".into());
        state.dialogue.practice_days.insert(21_000);
        state.dialogue.conversation_state = Some(2);
        state.semantic.field.confidence = 0.1;
        state.last_turn_decision = None;
        state.governance_log.append(GovernanceEvent {
            turn: 2,
            event_type: GovernanceEventType::GraphEnriched { new_relations: 9 },
            family: CanonicalMoveFamily::CMGround,
            guard_status: GuardStatus::InvariantOk,
            timestamp: "turn-2".into(),
        });

        snapshot.apply(&mut state);

        // Non-history mutables: restored to pre-turn values.
        assert_eq!(state.dialogue.turn_count, 3);
        assert_eq!(state.dialogue.last_topic, Some("свобода".into()));
        assert_eq!(state.dialogue.conversation_state, None);
        assert_eq!(state.dialogue.practice_days, [20_000].into_iter().collect());
        assert_eq!(
            state.dialogue.topic_last_practice_day.get("свобода"),
            Some(&20_000u64)
        );
        assert_eq!(state.last_turn_decision.as_ref().unwrap().legitimacy, 0.9);
        assert_eq!(
            state.last_turn_decision.as_ref().unwrap().guard_status,
            GuardStatus::Allowed
        );
        assert!((state.semantic.field.confidence - 0.5).abs() < 1e-12);
        assert_eq!(state.governance_log.len(), 1);

        // History is untouched by restore: it still contains the during-turn
        // append that the turn pipeline writes after capture.
        assert_eq!(
            state.dialogue.history.last().map(|s| s.as_str()),
            Some("during-turn")
        );
    }

    #[test]
    #[allow(clippy::field_reassign_with_default)]
    fn rollback_snapshot_excludes_the_derived_cache() {
        let mut state = SystemState::default();
        state.semantic.cached_network = Some(crate::network::SemanticNetwork::default());
        state.semantic.cached_edge_count = 4_321;

        let snapshot = state.capture_rollback_snapshot();

        // The derived cache is not carried into the snapshot: a restore must
        // force a lazy rebuild, so it must come back empty.
        assert!(snapshot.semantic().cached_network.is_none());
        assert_eq!(snapshot.semantic().cached_edge_count, 0);

        snapshot.apply(&mut state);
        assert!(state.semantic.cached_network.is_none());
        assert_eq!(state.semantic.cached_edge_count, 0);
    }
}

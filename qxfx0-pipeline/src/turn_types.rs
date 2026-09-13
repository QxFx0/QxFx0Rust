// Shared turn types: inputs, outputs, timings, staged feature switches.
//! Shared turn types: inputs, outputs, timings, and staged feature switches.
//!
//! Pure data plus tiny pure helpers. Stage logic lives in `stages`, entry
//! points in `turn_api`, V2 authority bookkeeping in `authority`.

use crate::execution_trace;
use qxfx0_semantic::PropositionParser;
use qxfx0_types::system_state::*;
use qxfx0_types::*;
use serde::Serialize;
use std::time::Duration;

pub(crate) const CHALLENGE_PATTERNS: &[&str] = &[
    "это просто",
    "не более чем",
    "сводится к",
    "всего лишь",
    "это лишь",
    "разве",
    "не согласен",
    "не согласна",
    "противореч",
    "неверно",
    "ошибаешься",
    "не прав",
    "спорю",
    "возраж",
    "сомневаюсь",
    "оспариваю",
];

/// Centralized challenge detection — single source of truth used by all pipelines.
/// Combines parser-based mode detection with substring pattern matching so that
/// no challenge is missed by either mechanism.
pub fn detect_challenge(text: &str) -> bool {
    let parsed = PropositionParser::parse(text);
    if matches!(parsed.mode, qxfx0_semantic::PropositionMode::Challenge) {
        return true;
    }
    let lower = text.to_lowercase();
    CHALLENGE_PATTERNS.iter().any(|p| lower.contains(p))
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnInput {
    pub session_id: String,
    pub raw_text: String,
}

/// Selects which component has authority over content-admitted response
/// surfaces. The default keeps the existing renderer authoritative while
/// recording plan-to-surface comparison evidence in the trace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum RendererAuthority {
    #[default]
    LegacyShadow,
    AuditedPlan,
    V2Canary,
}

/// Explicit authority switch for the v2 subject core (ADR-0044
/// migration). `V2Authority` is the law since the M4 flip: the
/// canonical energy scalar, bias and ladder drive Prepare/Finalize.
/// `V1Authority` remains for pinned comparisons (B2 experiment,
/// measurement baselines, pre-flip replay). Never persisted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum SubjectAuthority {
    V1Authority,
    #[default]
    V2Authority,
}

/// Stable label for journal records and manifests (pre-migration
/// artifacts carry `v1_authority`).
pub fn subject_authority_label(authority: SubjectAuthority) -> &'static str {
    match authority {
        SubjectAuthority::V1Authority => "v1_authority",
        SubjectAuthority::V2Authority => "v2_authority",
    }
}

/// Parse a recorded authority label. `None` fails the replay closed.
pub fn subject_authority_from_label(label: &str) -> Option<SubjectAuthority> {
    match label {
        "v1_authority" => Some(SubjectAuthority::V1Authority),
        "v2_authority" => Some(SubjectAuthority::V2Authority),
        _ => None,
    }
}

/// Explicit authority switch for the V2 canary. This is separate from the V2
/// observation mode so measuring V2 can never accidentally change output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum ResponsePlanV2Authority {
    #[default]
    Disabled,
    Canary,
}

/// Enables observation-only doubt evidence in an explicit execution trace.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum DoubtShadowMode {
    /// Preserve the current pipeline without calculating doubt evidence.
    #[default]
    Disabled,
    /// Calculate a proposed route but never apply it to routing or state.
    TraceOnly,
}

/// Enables observation-only typed anomaly-recovery evidence in an execution
/// trace. It never applies a recovery strategy or mutates persisted state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum AnomalyShadowMode {
    /// Preserve the current pipeline without calculating anomaly evidence.
    #[default]
    Disabled,
    /// Calculate a recovery proposal but never apply it to routing or state.
    TraceOnly,
}

/// Selects the ADR-0034 V2 rollout population. V1 remains the renderer and
/// the V2 result never enters turn state in any mode.
pub use qxfx0_plan_v2::ResponsePlanV2Mode;

#[derive(Debug, Clone, Serialize)]
pub struct AuthorityDecisionReceipt {
    pub topic: String,
    pub requested_mode: ResponsePlanV2Mode,
    pub effective_mode: ResponsePlanV2Mode,
    pub authority: ResponsePlanV2Authority,
    pub outcome: qxfx0_plan_v2::V2AuthorityOutcome,
    pub output_digest: Option<String>,
    pub artifact_digest: String,
    pub contract_digest: String,
    pub replay_bundle_digest: Option<String>,
    pub guard_classification: String,
}

impl AuthorityDecisionReceipt {
    pub fn output(&self) -> Option<String> {
        self.outcome.output().map(|surface| surface.joined())
    }

    pub fn can_emit_v2(&self) -> bool {
        self.authority == ResponsePlanV2Authority::Canary
            && self.topic_is_canary()
            && matches!(
                self.outcome,
                qxfx0_plan_v2::V2AuthorityOutcome::Compositional { .. }
                    | qxfx0_plan_v2::V2AuthorityOutcome::AuditedVerbatim { .. }
            )
    }

    fn topic_is_canary(&self) -> bool {
        RESPONSE_PLAN_V2_CANARY_ALLOWLIST.contains(&self.topic.as_str())
    }
}

pub(crate) const RESPONSE_PLAN_V2_CANARY_ALLOWLIST: [&str; 3] = ["правда", "произвол", "свобода"];

pub fn response_plan_v2_canary_allowlist() -> &'static [&'static str; 3] {
    &RESPONSE_PLAN_V2_CANARY_ALLOWLIST
}

pub fn response_plan_v2_canary_digest() -> String {
    execution_trace::calculate_stable_digest(&RESPONSE_PLAN_V2_CANARY_ALLOWLIST)
        .expect("static canary allowlist must serialize")
}

/// Compares persisted state attributes without considering trace-only evidence.
/// This is intentionally explicit so rollout tests cannot hide a state change
/// behind an aggregate digest. Observational state is deliberately excluded:
/// `thesis_state` and the ADR-0043 U2 shadow `semantic.essence_v2` accumulate
/// evidence without ever being an authority over visible behaviour.
pub fn response_plan_v2_state_parity(left: &SystemState, right: &SystemState) -> bool {
    fn equal<T: Serialize>(left: &T, right: &T) -> bool {
        match (serde_json::to_vec(left), serde_json::to_vec(right)) {
            (Ok(left_bytes), Ok(right_bytes)) => left_bytes == right_bytes,
            // Two serialization failures are an error, not evidence of
            // parity: report the failure and compare unequal.
            (left_result, right_result) => {
                tracing::error!(
                    "state parity comparison hit a serialization failure: {:?} vs {:?}",
                    left_result.err(),
                    right_result.err()
                );
                false
            }
        }
    }

    left.session_id == right.session_id
        && left.dialogue.turn_count == right.dialogue.turn_count
        && left.dialogue.history == right.dialogue.history
        && left.dialogue.last_family == right.dialogue.last_family
        && left.dialogue.last_topic == right.dialogue.last_topic
        && left.dialogue.conversation_state == right.dialogue.conversation_state
        && equal(&left.semantic.field, &right.semantic.field)
        && equal(&left.semantic.runtime_graph, &right.semantic.runtime_graph)
        && left.semantic.pack_set_fingerprint == right.semantic.pack_set_fingerprint
        && equal(
            &left.semantic.semantic_commitments,
            &right.semantic.semantic_commitments,
        )
        && equal(&left.semantic.essence, &right.semantic.essence)
        && equal(&left.semantic.adjunction, &right.semantic.adjunction)
        && equal(&left.semantic.perspective, &right.semantic.perspective)
        && equal(
            &left.semantic.stance_provenance,
            &right.semantic.stance_provenance,
        )
        && equal(&left.last_turn_decision, &right.last_turn_decision)
        && equal(&left.governance_log, &right.governance_log)
}

/// Explicit, default-off durable provenance recorder. It never feeds routing,
/// plans, rendering, temporal recovery, or user-visible output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum StanceProvenanceMode {
    #[default]
    Disabled,
    RecordAffirmedSystemDecision,
}

/// Observation result for a signed external stance attestation. This is
/// returned to the integrating service only; it is never stored in
/// `SystemState` and does not change routing, planning, rendering, or guard
/// behavior.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum SignedStanceDecisionOutcome {
    NoAttestation,
    VerificationRejected { reason: String },
    BlockedTurn,
    NormalizedTopicMismatch,
    Recorded,
    NoStateTransition,
}

/// Controls the staged clarification route. It is disabled in all standard
/// runtime paths; trace-only mode is evidence, while limited enablement is
/// available only to an explicit pipeline caller.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum ClarificationMode {
    #[default]
    Disabled,
    TraceOnly,
    LimitedEnabled,
}

/// Staged, immediate same-topic suppression for a proposed clarification.
/// It is independent from the clarification route and disabled by default.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum SameTopicSuppressionMode {
    #[default]
    Disabled,
    TraceOnly,
    LimitedEnabled,
}

impl SameTopicSuppressionMode {
    pub(crate) const fn observes(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    pub(crate) const fn applies(self) -> bool {
        matches!(self, Self::LimitedEnabled)
    }
}

impl ClarificationMode {
    pub(crate) const fn observes(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    pub(crate) const fn applies(self) -> bool {
        matches!(self, Self::LimitedEnabled)
    }
}

impl DoubtShadowMode {
    pub(crate) const fn enabled(self) -> bool {
        matches!(self, Self::TraceOnly)
    }
}

impl AnomalyShadowMode {
    pub(crate) const fn enabled(self) -> bool {
        matches!(self, Self::TraceOnly)
    }
}

impl RendererAuthority {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LegacyShadow => "legacy_shadow",
            Self::AuditedPlan => "audited_plan",
            Self::V2Canary => "v2_canary",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct TurnOutput {
    pub response: String,
    pub family: CanonicalMoveFamily,
    pub guard_status: GuardStatus,
    pub blocked: bool,
    pub commitment_engaged: bool,
    pub governance_events: usize,
    pub conatus_energy: f64,
    pub path_depth: usize,
    pub holistic_dominant: bool,
    pub conversation_state: String,
}

/// Lightweight per-stage timing for an individual pipeline turn.
///
/// Unlike [`execution_trace::PipelineTrace`], this structure does not compute
/// replay digests. It is therefore suitable for opt-in latency diagnostics
/// without adding serialization and hashing work to each measured stage.
#[derive(Debug, Clone, Default, Serialize)]
pub struct PipelineStageTimings {
    /// Parsing, topic normalization, and typed input construction.
    pub input_normalization_ms: u64,
    /// One-time, process-global morphology runtime build cost (embedded
    /// lexeme-bundle serde parse + index build), incurred on the first
    /// lemmatizer call in a process. Zero when the known-graph-atom path
    /// never touches the lemmatizer. Attribute for `input_normalization_ms`
    /// spikes that are not per-turn parse work.
    pub morphology_init_ms: u64,
    /// One-time (process-global) cost of eagerly faulting the embedded
    /// morphology runtime blob before deserialization. Zero on processes that
    /// never exercise the lemmatizer. Sub-component of the
    /// `input_normalization_ms` spike attribution; see
    /// `qxfx0_morphology::runtime_blob_warm_ms`.
    pub morphology_blob_warm_ms: u64,
    /// One-time (process-global) cost of making the adjective lexicon usable
    /// (blob deserialize; there is deliberately no JSON fallback — a stale
    /// or missing blob fails closed via `build.rs` + `doctor`).
    /// Zero when no adjective is ever resolved. Attribute for
    /// `input_normalization_ms` spikes that `morphology_init_ms` does not
    /// cover; see `qxfx0_morphology::adjective_lexicon_init_ms`.
    pub adjective_lexicon_init_ms: u64,
    /// Self-layer preparation.
    pub prepare_ms: u64,
    /// Typed family routing.
    pub route_ms: u64,
    /// Shadow-plan semantic selection.
    pub semantic_selection_ms: u64,
    /// Plan-surface or legacy rendering work.
    pub plan_render_ms: u64,
    /// State finalization before quality enforcement.
    pub finalize_ms: u64,
    /// Content and safety guard evaluation.
    pub guard_ms: u64,
    /// In-memory governance persistence stage.
    pub persist_ms: u64,
    /// Total pipeline duration, excluding database I/O.
    pub total_ms: u64,
}

impl PipelineStageTimings {
    pub(crate) fn duration_ms(duration: Duration) -> u64 {
        duration.as_millis().try_into().unwrap_or(u64::MAX)
    }

    pub(crate) fn record_stage(&mut self, stage_name: &str, duration: Duration) {
        let elapsed_ms = Self::duration_ms(duration);
        match stage_name {
            "prepare" => self.prepare_ms = elapsed_ms,
            "route" => self.route_ms = elapsed_ms,
            "plan_shadow" => self.semantic_selection_ms = elapsed_ms,
            "render" => self.plan_render_ms = elapsed_ms,
            "finalize" => self.finalize_ms = elapsed_ms,
            "guard" => self.guard_ms = elapsed_ms,
            "persist" => self.persist_ms = elapsed_ms,
            "morphology_init" => self.morphology_init_ms = elapsed_ms,
            _ => {}
        }
    }
}

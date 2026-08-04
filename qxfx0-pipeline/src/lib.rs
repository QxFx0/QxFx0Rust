//! QxFx0 Pipeline — synchronous sequential turn processing.
//!
//! 7 stages: Prepare → Route → PlanShadow → Render → Finalize → Guard → Persist.
//! No async, no Tokio, no external middleware — pure synchronous call chain.

#[cfg(test)]
mod conjugate_pipeline;
pub mod conversation_fsm;
mod debate;
#[path = "tracing.rs"]
pub mod execution_trace;
pub mod fact_grounded;
pub mod replay;
pub mod shadow_plan;
mod stages;
pub mod stance_request;
pub mod turn_context;
#[cfg(test)]
mod vector_pipeline;

pub use conversation_fsm::{
    fsm_state_discriminant, fsm_state_from_discriminant, initial_state, is_active,
    proposition_to_event, transition as fsm_transition, ConversationEvent, ConversationState,
};

use qxfx0_semantic::PropositionParser;
use qxfx0_types::atom::AtomId;
use qxfx0_types::system_state::*;
use qxfx0_types::*;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::fs::File;
use std::io::Read;
use std::time::{Duration, Instant};
use turn_context::{StageTraceContext, TurnInputContext};

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

/// Enables Debate Core as a pure post-plan observer.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum DebateCoreMode {
    #[default]
    Disabled,
    TraceOnly,
}

/// Selects the ADR-0034 V2 rollout population. V1 remains the renderer and
/// the V2 result never enters turn state in any mode.
pub use qxfx0_semantic::response_plan_v2::ResponsePlanV2Mode;

#[derive(Debug, Clone, Serialize)]
pub struct AuthorityDecisionReceipt {
    pub topic: String,
    pub requested_mode: ResponsePlanV2Mode,
    pub effective_mode: ResponsePlanV2Mode,
    pub authority: ResponsePlanV2Authority,
    pub outcome: qxfx0_semantic::response_plan_v2::V2AuthorityOutcome,
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
                qxfx0_semantic::response_plan_v2::V2AuthorityOutcome::Compositional { .. }
                    | qxfx0_semantic::response_plan_v2::V2AuthorityOutcome::AuditedVerbatim { .. }
            )
    }

    fn topic_is_canary(&self) -> bool {
        RESPONSE_PLAN_V2_CANARY_ALLOWLIST.contains(&self.topic.as_str())
    }
}

const RESPONSE_PLAN_V2_CANARY_ALLOWLIST: [&str; 6] = [
    "правда",
    "произвол",
    "свобода",
    "время",
    "справедливость",
    "ответственность",
];

pub fn response_plan_v2_canary_allowlist() -> &'static [&'static str; 6] {
    &RESPONSE_PLAN_V2_CANARY_ALLOWLIST
}

pub fn response_plan_v2_canary_digest() -> String {
    execution_trace::calculate_stable_digest(&RESPONSE_PLAN_V2_CANARY_ALLOWLIST)
        .expect("static canary allowlist must serialize")
}

/// Compares persisted state attributes without considering trace-only evidence.
/// This is intentionally explicit so rollout tests cannot hide a state change
/// behind an aggregate digest.
pub fn response_plan_v2_state_parity(left: &SystemState, right: &SystemState) -> bool {
    fn equal<T: Serialize>(left: &T, right: &T) -> bool {
        serde_json::to_vec(left).ok() == serde_json::to_vec(right).ok()
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
    const fn observes(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    const fn applies(self) -> bool {
        matches!(self, Self::LimitedEnabled)
    }
}

impl ClarificationMode {
    const fn observes(self) -> bool {
        !matches!(self, Self::Disabled)
    }

    const fn applies(self) -> bool {
        matches!(self, Self::LimitedEnabled)
    }
}

impl DoubtShadowMode {
    const fn enabled(self) -> bool {
        matches!(self, Self::TraceOnly)
    }
}

impl AnomalyShadowMode {
    const fn enabled(self) -> bool {
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
    fn duration_ms(duration: Duration) -> u64 {
        duration.as_millis().try_into().unwrap_or(u64::MAX)
    }

    fn record_stage(&mut self, stage_name: &str, duration: Duration) {
        let elapsed_ms = Self::duration_ms(duration);
        match stage_name {
            "prepare" => self.prepare_ms = elapsed_ms,
            "route" => self.route_ms = elapsed_ms,
            "plan_shadow" => self.semantic_selection_ms = elapsed_ms,
            "render" => self.plan_render_ms = elapsed_ms,
            "finalize" => self.finalize_ms = elapsed_ms,
            "guard" => self.guard_ms = elapsed_ms,
            "persist" => self.persist_ms = elapsed_ms,
            _ => {}
        }
    }
}

#[derive(Debug, Clone, Default)]
struct RecoverySnapshot {
    family: Option<CanonicalMoveFamily>,
    conversation_state: Option<ConversationState>,
    conatus_energy: Option<f64>,
    path_depth: Option<usize>,
}

/// Build a recovery output after a stage fault, using the rolled-back state.
fn recovery_output(state: &SystemState, recovery: &RecoverySnapshot) -> TurnOutput {
    let family = recovery.family.unwrap_or(CanonicalMoveFamily::CMGround);
    let conversation_state = recovery
        .conversation_state
        .map(|value| format!("{:?}", value))
        .unwrap_or_else(|| format!("{:?}", family));

    TurnOutput {
        response: "QxFx0: внутренняя ошибка обработки, состояние восстановлено.".into(),
        family,
        guard_status: GuardStatus::Blocked("stage error".into()),
        blocked: true,
        commitment_engaged: false,
        governance_events: state.governance_log.len(),
        conatus_energy: recovery.conatus_energy.unwrap_or(0.0),
        path_depth: recovery.path_depth.unwrap_or(0),
        holistic_dominant: state.semantic.adjunction.holistic_dominant,
        conversation_state,
    }
}

fn session_invariant_output(state: &SystemState, reason: &str) -> TurnOutput {
    TurnOutput {
        response: "QxFx0: идентификатор сессии отклонён; состояние не изменено.".into(),
        family: CanonicalMoveFamily::CMRepair,
        guard_status: GuardStatus::InvariantBlock(reason.into()),
        blocked: true,
        commitment_engaged: false,
        governance_events: state.governance_log.len(),
        conatus_energy: 0.0,
        path_depth: 0,
        holistic_dominant: state.semantic.adjunction.holistic_dominant,
        conversation_state: state
            .dialogue
            .conversation_state
            .map(|value| value.to_string())
            .unwrap_or_else(|| "Idle".into()),
    }
}

fn execute_stage<I, O, E, F>(
    trace: &mut Option<&mut execution_trace::PipelineTrace>,
    timings: &mut Option<&mut PipelineStageTimings>,
    stage_name: &str,
    state: &mut SystemState,
    input: I,
    stage: F,
) -> Result<O, E>
where
    I: Serialize,
    O: Serialize + StageTraceContext,
    E: Serialize,
    F: FnOnce(&mut SystemState, I) -> Result<O, E>,
{
    if trace.is_none() && timings.is_none() {
        return stage(state, input);
    }

    let input_digest = trace.as_ref().map(|_| {
        execution_trace::calculate_stable_digest(&(&*state, &input))
            .unwrap_or_else(|error| format!("digest-error:{error}"))
    });
    let start = Instant::now();
    let result = stage(state, input);
    let duration = start.elapsed();

    if let Some(timings) = timings.as_deref_mut() {
        timings.record_stage(stage_name, duration);
    }

    if trace.is_some() {
        let output_digest = execution_trace::calculate_stable_digest(&(&*state, &result))
            .unwrap_or_else(|error| format!("digest-error:{error}"));
        let mut metadata = BTreeMap::new();
        metadata.insert(
            "status".into(),
            result
                .as_ref()
                .map_or("error", StageTraceContext::trace_status)
                .into(),
        );
        if let Ok(output) = &result {
            if let Some(family) = output.trace_family() {
                metadata.insert("family".into(), format!("{:?}", family));
            }
            metadata.extend(output.trace_metadata());
        }
        if let Some(trace) = trace.as_deref_mut() {
            trace.record_step(
                stage_name,
                input_digest.expect("trace requires an input digest"),
                output_digest,
                duration,
                metadata,
            );
        }
    }
    result
}

/// Explicit, default-off feature selection for a single turn.
///
/// Every axis defaults to the standard production path: the legacy renderer
/// with all staged integrations disabled. A new staged feature extends this
/// struct instead of multiplying `process_turn_*` entry points, so behaviour
/// selection stays data rather than a combinatorial set of function names.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct TurnOptions {
    pub renderer_authority: RendererAuthority,
    pub doubt_shadow: DoubtShadowMode,
    pub anomaly_shadow: AnomalyShadowMode,
    pub clarification: ClarificationMode,
    pub suppression: SameTopicSuppressionMode,
    pub fact_grounded: fact_grounded::FactGroundedRollout,
    pub response_plan_v2: ResponsePlanV2Mode,
    pub response_plan_v2_authority: ResponsePlanV2Authority,
    pub debate_core: DebateCoreMode,
}

impl TurnOptions {
    /// Standard production path: legacy renderer, every staged feature off.
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_renderer(mut self, renderer_authority: RendererAuthority) -> Self {
        self.renderer_authority = renderer_authority;
        self
    }

    pub fn with_doubt_shadow(mut self, doubt_shadow: DoubtShadowMode) -> Self {
        self.doubt_shadow = doubt_shadow;
        self
    }

    pub fn with_anomaly_shadow(mut self, anomaly_shadow: AnomalyShadowMode) -> Self {
        self.anomaly_shadow = anomaly_shadow;
        self
    }

    pub fn with_clarification(mut self, clarification: ClarificationMode) -> Self {
        self.clarification = clarification;
        self
    }

    pub fn with_suppression(mut self, suppression: SameTopicSuppressionMode) -> Self {
        self.suppression = suppression;
        self
    }

    pub fn with_fact_grounded(mut self, fact_grounded: fact_grounded::FactGroundedRollout) -> Self {
        self.fact_grounded = fact_grounded;
        self
    }

    pub fn with_response_plan_v2(mut self, mode: ResponsePlanV2Mode) -> Self {
        self.response_plan_v2 = mode;
        self
    }

    pub fn with_response_plan_v2_authority(mut self, authority: ResponsePlanV2Authority) -> Self {
        self.response_plan_v2_authority = authority;
        if authority == ResponsePlanV2Authority::Canary {
            self.response_plan_v2 = ResponsePlanV2Mode::Canary;
        }
        self
    }

    pub fn with_debate_core(mut self, mode: DebateCoreMode) -> Self {
        self.debate_core = mode;
        self
    }
}

/// Process a turn with an explicit option set and no collected diagnostics.
///
/// This and its three sibling collection shapes are the only entry points that
/// reach the pipeline directly; every named `process_turn_*` wrapper below
/// delegates here so there is a single behavioural implementation.
pub fn process_turn_with_options(
    input: &TurnInput,
    state: &mut SystemState,
    options: TurnOptions,
) -> TurnOutput {
    process_turn_internal(input, state, None, None, options)
}

/// Process a turn with an option set, collecting observational stage timings.
pub fn process_turn_with_options_and_timing(
    input: &TurnInput,
    state: &mut SystemState,
    options: TurnOptions,
) -> (TurnOutput, PipelineStageTimings) {
    let started = Instant::now();
    let mut timings = PipelineStageTimings::default();
    let output = process_turn_internal(input, state, None, Some(&mut timings), options);
    timings.total_ms = PipelineStageTimings::duration_ms(started.elapsed());
    (output, timings)
}

/// Process a turn with an option set, collecting a replay-stable stage trace.
pub fn process_turn_with_options_and_trace(
    input: &TurnInput,
    state: &mut SystemState,
    options: TurnOptions,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    let (mut trace, initial_digest, trace_started) = new_pipeline_trace(input, state);
    let output = process_turn_internal(input, state, Some(&mut trace), None, options);
    finish_pipeline_trace(initial_digest, state, &output, trace_started, &mut trace);
    (output, trace)
}

/// Process a turn with an option set, collecting both timings and a trace
/// without running the pipeline twice.
pub fn process_turn_with_options_timing_and_trace(
    input: &TurnInput,
    state: &mut SystemState,
    options: TurnOptions,
) -> (
    TurnOutput,
    PipelineStageTimings,
    execution_trace::PipelineTrace,
) {
    let started = Instant::now();
    let mut timings = PipelineStageTimings::default();
    let (mut trace, initial_digest, trace_started) = new_pipeline_trace(input, state);
    let output = process_turn_internal(input, state, Some(&mut trace), Some(&mut timings), options);
    timings.total_ms = PipelineStageTimings::duration_ms(started.elapsed());
    finish_pipeline_trace(initial_digest, state, &output, trace_started, &mut trace);
    (output, timings, trace)
}

/// Process a single turn synchronously through all 7 stages.
///
/// If any stage before the guard fails, the state is rolled back to its
/// pre-turn snapshot and a blocked recovery output is returned. This prevents
/// partial side effects from corrupting the session.
pub fn process_turn(input: &TurnInput, state: &mut SystemState) -> TurnOutput {
    process_turn_with_options(input, state, TurnOptions::new())
}

/// Process a turn with an explicit renderer-authority feature flag.
pub fn process_turn_with_renderer(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
) -> TurnOutput {
    process_turn_with_options(
        input,
        state,
        TurnOptions::new().with_renderer(renderer_authority),
    )
}

/// Process a turn with an explicit fact-grounded rollout. Only a successful
/// audited-plan render can produce Perspective evidence.
pub fn process_turn_with_renderer_and_fact_grounded(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    fact_grounded_rollout: fact_grounded::FactGroundedRollout,
) -> TurnOutput {
    process_turn_with_options(
        input,
        state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_fact_grounded(fact_grounded_rollout),
    )
}

/// Process an ordinary turn, then record an accepted normalized subject as an
/// explicit affirmed system decision when the caller opted in. Recording is
/// after the guard, so rejected turns and failed-stage rollback retain none.
pub fn process_turn_with_renderer_and_stance_provenance(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    mode: StanceProvenanceMode,
) -> TurnOutput {
    let output = process_turn_with_renderer(input, state, renderer_authority);
    if matches!(mode, StanceProvenanceMode::RecordAffirmedSystemDecision) && !output.blocked {
        if let (Some(topic), turn) = (state.dialogue.last_topic.clone(), state.dialogue.turn_count)
        {
            if let Ok(topic) = qxfx0_types::stance::StanceTopic::new(topic) {
                state
                    .semantic
                    .stance_provenance
                    .record(qxfx0_types::stance::StanceObservation {
                        turn,
                        topic,
                        polarity: qxfx0_types::stance::StancePolarity::Affirmed,
                        source: qxfx0_types::stance::StanceSource::SystemDecision,
                    });
            }
        }
    }
    output
}

/// Process a turn with an explicit integrating-caller stance boundary.
///
/// Caller authorization is outside this library boundary. The supplied topic
/// must equal the pipeline-normalized topic; neither user input nor guard
/// outcome is ever converted into a polarity here.
pub fn process_turn_with_renderer_and_explicit_stance_decision(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    decision: qxfx0_types::stance::SystemStanceDecision,
) -> TurnOutput {
    let output = process_turn_with_renderer(input, state, renderer_authority);
    record_explicit_stance_decision_if_allowed(&output, state, decision);
    output
}

/// Process a normal turn and, only after it succeeds, optionally record a
/// verified signed external system stance. Signature verification is
/// transport-independent and uses an explicit caller-supplied time so replay
/// never reads a wall clock. A rejected attestation is fail-closed for the
/// provenance write while retaining the ordinary turn result.
pub fn process_turn_with_renderer_and_signed_stance_decision(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    signed_decision: Option<&qxfx0_types::SignedStanceDecision>,
    verifier: &impl qxfx0_types::StanceDecisionSignatureVerifier,
    verification_policy: &qxfx0_types::StanceAuthorityVerificationPolicy,
) -> (TurnOutput, SignedStanceDecisionOutcome) {
    let verification = signed_decision.map(|signed| {
        qxfx0_types::verify_signed_stance_decision(
            verifier,
            signed,
            &qxfx0_types::StanceVerificationContext {
                audience: verification_policy.audience.clone(),
                session_id: input.session_id.clone(),
                expected_pre_turn: state.dialogue.turn_count,
                request_digest: qxfx0_types::calculate_stance_request_digest(
                    &input.session_id,
                    &input.raw_text,
                ),
                verification_time_unix_seconds: verification_policy.verification_time_unix_seconds,
                max_validity_seconds: verification_policy.max_validity_seconds,
            },
        )
    });
    let output = process_turn_with_renderer(input, state, renderer_authority);
    let outcome = match verification {
        None => SignedStanceDecisionOutcome::NoAttestation,
        Some(Err(error)) => {
            tracing::warn!(reason = %error, "rejected signed stance decision");
            SignedStanceDecisionOutcome::VerificationRejected {
                reason: error.to_string(),
            }
        }
        Some(Ok(verified)) => match record_explicit_stance_decision_if_allowed(
            &output,
            state,
            verified.into_decision(),
        ) {
            None if output.blocked => SignedStanceDecisionOutcome::BlockedTurn,
            None => SignedStanceDecisionOutcome::NormalizedTopicMismatch,
            Some(qxfx0_types::stance::StanceRecordOutcome::Recorded) => {
                SignedStanceDecisionOutcome::Recorded
            }
            Some(qxfx0_types::stance::StanceRecordOutcome::NoStateTransition) => {
                SignedStanceDecisionOutcome::NoStateTransition
            }
        },
    };
    (output, outcome)
}

fn record_explicit_stance_decision_if_allowed(
    output: &TurnOutput,
    state: &mut SystemState,
    decision: qxfx0_types::stance::SystemStanceDecision,
) -> Option<qxfx0_types::stance::StanceRecordOutcome> {
    if output.blocked || state.dialogue.last_topic.as_deref() != Some(decision.topic.as_str()) {
        return None;
    }
    Some(
        state
            .semantic
            .stance_provenance
            .record(qxfx0_types::stance::StanceObservation {
                turn: state.dialogue.turn_count,
                topic: decision.topic,
                polarity: decision.polarity,
                source: qxfx0_types::stance::StanceSource::SystemDecision,
            }),
    )
}

/// Process a turn while collecting lightweight timing evidence for each
/// pipeline stage. The returned timing is observational and is not persisted
/// in the session state or included in replay signatures.
pub fn process_turn_with_timing_and_renderer(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
) -> (TurnOutput, PipelineStageTimings) {
    process_turn_with_options_and_timing(
        input,
        state,
        TurnOptions::new().with_renderer(renderer_authority),
    )
}

/// Process a turn and return a stage-level trace with cross-process stable
/// SHA-256 digests. Durations are diagnostic and excluded from replay
/// signatures.
pub fn process_turn_with_trace(
    input: &TurnInput,
    state: &mut SystemState,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    process_turn_with_trace_and_renderer(input, state, RendererAuthority::LegacyShadow)
}

/// Process a turn with trace evidence and explicit renderer authority.
pub fn process_turn_with_trace_and_renderer(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    process_turn_with_trace_and_renderer_and_doubt_shadow(
        input,
        state,
        renderer_authority,
        DoubtShadowMode::Disabled,
    )
}

/// Process a turn with deterministic fact-grounded rollout evidence.
pub fn process_turn_with_trace_and_renderer_and_fact_grounded(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    fact_grounded_rollout: fact_grounded::FactGroundedRollout,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    process_turn_with_options_and_trace(
        input,
        state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_fact_grounded(fact_grounded_rollout),
    )
}

/// Process a turn with explicit renderer and observation-only doubt settings.
pub fn process_turn_with_trace_and_renderer_and_doubt_shadow(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    doubt_shadow: DoubtShadowMode,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    process_turn_with_trace_and_renderer_and_features(
        input,
        state,
        renderer_authority,
        doubt_shadow,
        ClarificationMode::Disabled,
    )
}

/// Process a turn with explicit renderer and observation-only anomaly settings.
pub fn process_turn_with_trace_and_renderer_and_anomaly_shadow(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    anomaly_shadow: AnomalyShadowMode,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    process_turn_with_options_and_trace(
        input,
        state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_anomaly_shadow(anomaly_shadow),
    )
}

/// Process a turn with explicit staged cognitive integrations. Standard paths
/// pass both modes as disabled; `LimitedEnabled` is intentionally opt-in.
pub fn process_turn_with_trace_and_renderer_and_features(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    doubt_shadow: DoubtShadowMode,
    clarification: ClarificationMode,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    process_turn_with_trace_and_renderer_and_features_and_suppression(
        input,
        state,
        renderer_authority,
        doubt_shadow,
        clarification,
        SameTopicSuppressionMode::Disabled,
    )
}

/// Process a turn with the separately staged same-topic suppression bridge.
/// Both cognitive features remain disabled unless an explicit caller enables
/// their limited pipeline mode.
pub fn process_turn_with_trace_and_renderer_and_features_and_suppression(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    doubt_shadow: DoubtShadowMode,
    clarification: ClarificationMode,
    suppression: SameTopicSuppressionMode,
) -> (TurnOutput, execution_trace::PipelineTrace) {
    process_turn_with_options_and_trace(
        input,
        state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_doubt_shadow(doubt_shadow)
            .with_clarification(clarification)
            .with_suppression(suppression),
    )
}

/// Process a turn with both performance timings and deterministic trace
/// evidence. This supports independent opt-in diagnostics and doubt shadow
/// tracing without running the pipeline twice.
pub fn process_turn_with_timing_trace_and_renderer_and_doubt_shadow(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    doubt_shadow: DoubtShadowMode,
) -> (
    TurnOutput,
    PipelineStageTimings,
    execution_trace::PipelineTrace,
) {
    process_turn_with_timing_trace_and_features_and_suppression(
        input,
        state,
        renderer_authority,
        doubt_shadow,
        ClarificationMode::Disabled,
        SameTopicSuppressionMode::Disabled,
    )
}

/// Process a turn with both timing diagnostics and anomaly shadow evidence.
pub fn process_turn_with_timing_trace_and_renderer_and_anomaly_shadow(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    anomaly_shadow: AnomalyShadowMode,
) -> (
    TurnOutput,
    PipelineStageTimings,
    execution_trace::PipelineTrace,
) {
    process_turn_with_options_timing_and_trace(
        input,
        state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_anomaly_shadow(anomaly_shadow),
    )
}

/// Timing and deterministic trace evidence for all explicit cognitive modes.
pub fn process_turn_with_timing_trace_and_features_and_suppression(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    doubt_shadow: DoubtShadowMode,
    clarification: ClarificationMode,
    suppression: SameTopicSuppressionMode,
) -> (
    TurnOutput,
    PipelineStageTimings,
    execution_trace::PipelineTrace,
) {
    process_turn_with_options_timing_and_trace(
        input,
        state,
        TurnOptions::new()
            .with_renderer(renderer_authority)
            .with_doubt_shadow(doubt_shadow)
            .with_clarification(clarification)
            .with_suppression(suppression),
    )
}

fn new_pipeline_trace(
    input: &TurnInput,
    state: &SystemState,
) -> (execution_trace::PipelineTrace, String, Instant) {
    let request_id = execution_trace::calculate_stable_digest(&(
        input,
        state.dialogue.turn_count,
        state.session_id.as_str(),
    ))
    .unwrap_or_else(|_| "trace-unavailable".into());
    let initial_digest = execution_trace::calculate_stable_digest(&(state, input))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    (
        execution_trace::PipelineTrace::new(&request_id),
        initial_digest,
        Instant::now(),
    )
}

fn finish_pipeline_trace(
    initial_digest: String,
    state: &SystemState,
    output: &TurnOutput,
    started: Instant,
    trace: &mut execution_trace::PipelineTrace,
) {
    let final_digest = execution_trace::calculate_stable_digest(&(state, output))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "turn_output",
        initial_digest,
        final_digest,
        std::time::Duration::ZERO,
        BTreeMap::from([
            ("blocked".into(), output.blocked.to_string()),
            ("family".into(), format!("{:?}", output.family)),
        ]),
    );
    trace.set_total_duration(started.elapsed());
}

#[derive(Debug, Serialize)]
struct ResponsePlanV2Artifact {
    schema: &'static str,
    contract: qxfx0_semantic::response_plan_v2::TurnContractSnapshot,
    record: Option<qxfx0_semantic::response_plan_v2::TurnRecord>,
    result: qxfx0_semantic::response_plan_v2::V2ExecutionResult,
    realized: Option<qxfx0_semantic::response_plan_v2::RealizedSurface>,
    fallback: qxfx0_semantic::response_plan_v2::FallbackAction,
    authority_outcome: qxfx0_semantic::response_plan_v2::V2AuthorityOutcome,
}

pub fn current_binary_digest() -> Result<String, String> {
    let path = std::env::current_exe().map_err(|error| error.to_string())?;
    let mut file = File::open(path).map_err(|error| error.to_string())?;
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).map_err(|error| error.to_string())?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn response_plan_v2_is_eligible(mode: ResponsePlanV2Mode, topic: &str) -> bool {
    match mode {
        ResponsePlanV2Mode::Off => false,
        ResponsePlanV2Mode::Shadow => true,
        ResponsePlanV2Mode::Canary => RESPONSE_PLAN_V2_CANARY_ALLOWLIST.contains(&topic),
        ResponsePlanV2Mode::AuditedAuthority => qxfx0_semantic::argued_topic_registry()
            .ok()
            .is_some_and(|registry| registry.get(topic).is_some()),
    }
}

fn record_response_plan_v2(
    mut trace: Option<&mut execution_trace::PipelineTrace>,
    routed: &turn_context::RoutedTurnContext,
    logical_turn: u64,
    requested_mode: ResponsePlanV2Mode,
    authority: ResponsePlanV2Authority,
) -> Option<AuthorityDecisionReceipt> {
    use qxfx0_semantic::response_plan_v2::{
        execute_audited_topic_at, AssertionPolicy, AuthoritySnapshot, PlanningPolicySnapshot,
        RealizationSnapshot, SelectionPolicy, SelectionPolicySnapshot, SelfSelectionContext,
        TurnContractSnapshot, TurnRecord, V2BudgetPolicy,
    };

    let topic = routed.prepared().input().subject();
    let canary_eligible = RESPONSE_PLAN_V2_CANARY_ALLOWLIST.contains(&topic);
    let authority_intent_eligible = routed.family() == CanonicalMoveFamily::CMDefine;
    let eligible = response_plan_v2_is_eligible(requested_mode, topic)
        && (authority != ResponsePlanV2Authority::Canary || authority_intent_eligible);
    let effective_mode = if eligible {
        requested_mode
    } else {
        ResponsePlanV2Mode::Off
    };
    let scope_downgrade_count = usize::from(requested_mode != effective_mode);
    let downgrade_reason = if scope_downgrade_count == 1 {
        "topic_outside_rollout_scope"
    } else {
        "none"
    };
    let canary_digest = response_plan_v2_canary_digest();
    if effective_mode == ResponsePlanV2Mode::Off {
        let metadata = BTreeMap::from([
            ("requested_mode".into(), format!("{requested_mode:?}")),
            ("effective_mode".into(), "Off".into()),
            ("canary_eligible".into(), canary_eligible.to_string()),
            (
                "authority_intent_eligible".into(),
                authority_intent_eligible.to_string(),
            ),
            ("canary_digest".into(), canary_digest),
            ("attempted".into(), "false".into()),
            ("completed".into(), "false".into()),
            ("downgrade_count".into(), scope_downgrade_count.to_string()),
            ("downgrade_reason".into(), downgrade_reason.into()),
            ("semantic_parity".into(), "false".into()),
            ("authority_parity".into(), "false".into()),
            ("realization_parity".into(), "false".into()),
            ("replay_parity".into(), "false".into()),
            ("authority_outcome".into(), "not_attempted".into()),
            ("authority_outcome_digest".into(), "none".into()),
            ("authority_surface_digest".into(), "none".into()),
            ("claim_identity_digest".into(), "none".into()),
            ("fact_binding_digest".into(), "none".into()),
            ("claim_authority_digest".into(), "none".into()),
            ("v1_authoritative".into(), "true".into()),
            ("v1_fallback_used".into(), "false".into()),
        ]);
        let digest = execution_trace::calculate_stable_digest(&metadata)
            .unwrap_or_else(|error| format!("digest-error:{error}"));
        if let Some(trace) = trace.as_deref_mut() {
            trace.record_step(
                "response_plan_v2",
                digest.clone(),
                digest,
                Duration::ZERO,
                metadata,
            );
        }
        return None;
    }

    let policy = SelectionPolicy {
        response_plan_v2_mode: requested_mode,
        ..SelectionPolicy::default()
    };
    let budgets = V2BudgetPolicy::default();
    let contract = TurnContractSnapshot::new(
        AuthoritySnapshot::new(
            qxfx0_semantic::active_pack_set().fingerprint(),
            AssertionPolicy::v1().digest(),
        ),
        PlanningPolicySnapshot::new(budgets.digest(), "proposition-canon-v1"),
        RealizationSnapshot::new(
            qxfx0_semantic::response_plan_v2::valency_lexicon().fingerprint(),
            "clause-grammar-v1",
            qxfx0_morphology::get_runtime().lexemes_sha256(),
            qxfx0_semantic::response_plan_v2::preposition_allomorphs().fingerprint(),
        ),
        SelectionPolicySnapshot::new(policy),
    );
    if contract.verify_integrity().is_err() {
        let metadata = BTreeMap::from([
            ("requested_mode".into(), format!("{requested_mode:?}")),
            ("effective_mode".into(), "Off".into()),
            ("canary_eligible".into(), canary_eligible.to_string()),
            (
                "authority_intent_eligible".into(),
                authority_intent_eligible.to_string(),
            ),
            ("canary_digest".into(), canary_digest),
            ("attempted".into(), "true".into()),
            ("completed".into(), "false".into()),
            (
                "downgrade_count".into(),
                (scope_downgrade_count + 1).to_string(),
            ),
            ("downgrade_reason".into(), "snapshot_unavailable".into()),
            ("semantic_parity".into(), "false".into()),
            ("authority_parity".into(), "false".into()),
            ("realization_parity".into(), "false".into()),
            ("replay_parity".into(), "false".into()),
            ("authority_outcome".into(), "typed_non_declarative".into()),
            ("authority_outcome_digest".into(), "none".into()),
            ("authority_surface_digest".into(), "none".into()),
            ("claim_identity_digest".into(), "none".into()),
            ("fact_binding_digest".into(), "none".into()),
            ("claim_authority_digest".into(), "none".into()),
            ("v1_authoritative".into(), "true".into()),
            ("v1_fallback_used".into(), "false".into()),
        ]);
        let digest = execution_trace::calculate_stable_digest(&metadata)
            .unwrap_or_else(|error| format!("digest-error:{error}"));
        if let Some(trace) = trace.as_deref_mut() {
            trace.record_step(
                "response_plan_v2",
                digest.clone(),
                digest,
                Duration::ZERO,
                metadata,
            );
        }
        return None;
    }

    let context = SelfSelectionContext::quantize(
        routed.prepared().conatus_energy(),
        routed.prepared().salience(),
        0.0,
    );
    let execution = execute_audited_topic_at(
        routed.prepared().input().subject(),
        qxfx0_semantic::response_plan_v2::EvidenceEvaluationContext::new(logical_turn, None),
        &budgets,
        &contract,
        context,
        policy,
        qxfx0_semantic::response_plan_v2::valency_lexicon(),
        qxfx0_morphology::get_runtime(),
    );
    let record =
        execution
            .selection
            .zip(execution.exact_replay)
            .and_then(|(selection, exact_replay)| {
                let binary_digest = current_binary_digest().ok()?;
                Some(TurnRecord::new(
                    contract.clone(),
                    selection,
                    binary_digest,
                    exact_replay,
                ))
            });
    let result = execution.result;
    let realized_surface = execution.realized;
    let fallback = qxfx0_semantic::response_plan_v2::fallback_action_for_result(&result);
    let expected_source_digest =
        qxfx0_semantic::response_plan_v2::audited_surface_source_digest(topic).unwrap_or_default();
    let authority_outcome = match realized_surface.clone() {
        Some(surface) => qxfx0_semantic::response_plan_v2::authority_outcome(
            topic,
            qxfx0_semantic::response_plan_v2::AuthoritySurfaceStrategy::Compositional,
            Ok(surface),
            &expected_source_digest,
        ),
        None if fallback == qxfx0_semantic::response_plan_v2::FallbackAction::AuditedV1Renderer => {
            qxfx0_semantic::response_plan_v2::authority_outcome(
                topic,
                qxfx0_semantic::response_plan_v2::AuthoritySurfaceStrategy::Compositional,
                Err(format!("V2 realization failed: {result:?}")),
                &expected_source_digest,
            )
        }
        None => qxfx0_semantic::response_plan_v2::V2AuthorityOutcome::TypedNonDeclarative {
            reason: format!("no V2 realized surface: {result:?}"),
        },
    };
    let (
        claim_identity_digest,
        fact_binding_digest,
        claim_authority_digest,
        semantic_parity,
        authority_parity,
    ) = match &result {
        qxfx0_semantic::response_plan_v2::V2ExecutionResult::Attempt(
            qxfx0_semantic::response_plan_v2::V2Attempt::Realizable(plan),
        ) => {
            let authorized = plan.authorized();
            let projected = authorized.certified().candidate().projected_claims();
            let claim_identity_digest = execution_trace::calculate_stable_digest(&projected)
                .unwrap_or_else(|error| format!("digest-error:{error}"));
            let fact_binding_digest =
                execution_trace::calculate_stable_digest(authorized.certified().bindings())
                    .unwrap_or_else(|error| format!("digest-error:{error}"));
            let claim_authority_digest =
                execution_trace::calculate_stable_digest(authorized.authorities())
                    .unwrap_or_else(|error| format!("digest-error:{error}"));
            let expected_facts = qxfx0_semantic::argued_topic_registry()
                .ok()
                .and_then(|registry| registry.get(topic))
                .map(|entry| {
                    entry
                        .statements()
                        .map(|statement| statement.fact_id())
                        .collect::<Vec<_>>()
                });
            let semantic_parity = expected_facts.as_ref().is_some_and(|expected| {
                projected.len() == expected.len()
                    && projected
                        .iter()
                        .zip(expected)
                        .all(|(claim, expected_fact)| {
                            authorized.certified().bindings().get(&claim.claim_id)
                                == Some(*expected_fact)
                        })
            });
            let authority_parity = semantic_parity
                && projected
                    .iter()
                    .all(|claim| authorized.authority_for(&claim.claim_id).is_some());
            (
                claim_identity_digest,
                fact_binding_digest,
                claim_authority_digest,
                semantic_parity,
                authority_parity,
            )
        }
        _ => ("none".into(), "none".into(), "none".into(), false, false),
    };
    let replay_bundle_digest = record
        .as_ref()
        .map(|record| record.exact_replay.bundle_digest.clone());
    let artifact = ResponsePlanV2Artifact {
        schema: "qxfx0.response-plan-v2.shadow.v1",
        contract,
        record,
        result,
        realized: realized_surface,
        fallback,
        authority_outcome,
    };
    let input_digest = execution_trace::calculate_stable_digest(&(
        routed.prepared().input().subject(),
        logical_turn,
        artifact.contract.digest.as_str(),
    ))
    .unwrap_or_else(|error| format!("digest-error:{error}"));
    let output_digest = execution_trace::calculate_stable_digest(&artifact)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let receipt = AuthorityDecisionReceipt {
        topic: topic.to_string(),
        requested_mode,
        effective_mode,
        authority,
        outcome: artifact.authority_outcome.clone(),
        output_digest: artifact
            .authority_outcome
            .output()
            .map(|output| output.surface_digest.clone()),
        artifact_digest: output_digest.clone(),
        contract_digest: artifact.contract.digest.clone(),
        replay_bundle_digest,
        guard_classification: "pending".into(),
    };
    let execution_downgrade = !matches!(
        &artifact.result,
        qxfx0_semantic::response_plan_v2::V2ExecutionResult::Attempt(
            qxfx0_semantic::response_plan_v2::V2Attempt::Realizable(_)
        )
    );
    let authority_kind = artifact.authority_outcome.kind();
    let authority_downgrade = matches!(
        &artifact.authority_outcome,
        qxfx0_semantic::response_plan_v2::V2AuthorityOutcome::RealizationDowngrade { .. }
    );
    let downgrade_count =
        scope_downgrade_count + usize::from(execution_downgrade) + usize::from(authority_downgrade);
    let downgrade_reason = if execution_downgrade {
        if authority_downgrade {
            "realization_downgrade"
        } else {
            "v2_execution_failure"
        }
    } else {
        downgrade_reason
    };
    let realization_parity = matches!(
        &artifact.authority_outcome,
        qxfx0_semantic::response_plan_v2::V2AuthorityOutcome::Compositional { output }
            if !output.clauses.is_empty()
    );
    let replay_parity = artifact.record.is_some();
    let mut metadata = BTreeMap::from([
        ("requested_mode".into(), format!("{requested_mode:?}")),
        ("effective_mode".into(), format!("{effective_mode:?}")),
        ("canary_eligible".into(), canary_eligible.to_string()),
        (
            "authority_intent_eligible".into(),
            authority_intent_eligible.to_string(),
        ),
        ("canary_digest".into(), canary_digest),
        ("attempted".into(), "true".into()),
        ("completed".into(), "true".into()),
        ("downgrade_count".into(), downgrade_count.to_string()),
        ("downgrade_reason".into(), downgrade_reason.into()),
        (
            "v1_authoritative".into(),
            (!receipt.can_emit_v2()).to_string(),
        ),
        ("v1_fallback_used".into(), "false".into()),
    ]);
    metadata.extend([
        ("contract_digest".into(), artifact.contract.digest.clone()),
        ("replay_integrity".into(), "verified-by-construction".into()),
        ("semantic_parity".into(), semantic_parity.to_string()),
        ("authority_parity".into(), authority_parity.to_string()),
        ("realization_parity".into(), realization_parity.to_string()),
        ("replay_parity".into(), replay_parity.to_string()),
        (
            "attestation_presentation_surface_signed".into(),
            "false".into(),
        ),
        ("claim_identity_digest".into(), claim_identity_digest),
        ("fact_binding_digest".into(), fact_binding_digest),
        ("claim_authority_digest".into(), claim_authority_digest),
        ("authority_outcome".into(), authority_kind.into()),
        (
            "authority_outcome_digest".into(),
            execution_trace::calculate_stable_digest(&artifact.authority_outcome)
                .unwrap_or_else(|error| format!("digest-error:{error}")),
        ),
        (
            "authority_surface_digest".into(),
            artifact
                .authority_outcome
                .output()
                .map(|output| output.surface_digest.clone())
                .unwrap_or_else(|| "none".into()),
        ),
        (
            "authority_source_digest".into(),
            artifact
                .authority_outcome
                .source_digest()
                .unwrap_or("none")
                .into(),
        ),
        (
            "replay_bundle_digest".into(),
            receipt
                .replay_bundle_digest
                .clone()
                .unwrap_or_else(|| "none".into()),
        ),
        (
            "legacy_graph_v2_declarative_fallback".into(),
            "false".into(),
        ),
    ]);
    if let Some(trace) = trace {
        let _ = trace.set_authority_receipt(&receipt);
        trace.record_step(
            "response_plan_v2",
            input_digest,
            output_digest,
            Duration::ZERO,
            metadata,
        );
    }
    Some(receipt)
}

#[allow(clippy::too_many_arguments)] // explicit staged feature flags meet at this private boundary
fn process_turn_internal(
    input: &TurnInput,
    state: &mut SystemState,
    mut trace: Option<&mut execution_trace::PipelineTrace>,
    mut timings: Option<&mut PipelineStageTimings>,
    options: TurnOptions,
) -> TurnOutput {
    let TurnOptions {
        renderer_authority,
        doubt_shadow,
        anomaly_shadow,
        clarification,
        suppression,
        fact_grounded: fact_grounded_rollout,
        response_plan_v2,
        response_plan_v2_authority,
        debate_core,
    } = options;
    if input.session_id.trim().is_empty()
        || input.session_id.chars().count() > 128
        || input.session_id.chars().any(char::is_control)
    {
        return session_invariant_output(state, "invalid session_id");
    }
    if state.session_id.is_empty() {
        state.session_id = input.session_id.clone();
    } else if state.session_id != input.session_id {
        return session_invariant_output(state, "session_id does not match loaded state");
    }
    let state_violations = state.validate();
    if !state_violations.is_empty() {
        tracing::error!("state invariant violation: {}", state_violations.join("; "));
        return session_invariant_output(state, "loaded state violates invariants");
    }

    let snapshot = state.clone();
    let mut recovery = RecoverySnapshot::default();

    // Parse once and retain the typed proposition throughout the pipeline.
    let normalization_started = Instant::now();
    if state.semantic.runtime_graph.edges.is_empty() {
        state.semantic.runtime_graph = qxfx0_semantic::seed_graph();
    }
    let prop =
        stance_request::parse_and_normalize_topic(&input.raw_text, &state.semantic.runtime_graph);

    if let Some(trace) = trace.as_deref_mut() {
        record_doubt_shadow(trace, doubt_shadow, state, &prop);
    }
    let is_challenge = detect_challenge(&input.raw_text);
    if let Some(trace) = trace.as_deref_mut() {
        record_anomaly_shadow(trace, anomaly_shadow, state, &prop, is_challenge);
    }
    let clarification_decision = clarification_decision(clarification, suppression, state, &prop);
    if let Some(trace) = trace.as_deref_mut() {
        record_clarification_route(trace, clarification, state, &prop, clarification_decision);
        record_same_topic_suppression(trace, suppression, state, &prop, clarification_decision);
    }

    let input_context = TurnInputContext::new(
        input.session_id.clone(),
        input.raw_text.clone(),
        prop,
        is_challenge,
    );
    if let Some(timings) = timings.as_deref_mut() {
        timings.input_normalization_ms =
            PipelineStageTimings::duration_ms(normalization_started.elapsed());
    }

    // Stage 1: Prepare
    let prepared = match execute_stage(
        &mut trace,
        &mut timings,
        "prepare",
        state,
        input_context,
        stages::prepare_stage,
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("prepare_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };
    recovery.conatus_energy = Some(prepared.conatus_energy());

    // Stage 2: Route
    let routed = match execute_stage(
        &mut trace,
        &mut timings,
        "route",
        state,
        prepared,
        |state, prepared| stages::route_stage(state, prepared, clarification_decision.applied),
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("route_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };
    recovery.family = Some(routed.family());
    recovery.conversation_state = Some(routed.conversation_state());

    let authority_receipt = record_response_plan_v2(
        trace.as_deref_mut(),
        &routed,
        state.dialogue.turn_count as u64,
        response_plan_v2,
        response_plan_v2_authority,
    );
    let effective_renderer_authority = if authority_receipt
        .as_ref()
        .is_some_and(AuthorityDecisionReceipt::can_emit_v2)
    {
        RendererAuthority::V2Canary
    } else {
        renderer_authority
    };

    // Stage 3: Shadow plan (observational; renderer authority is unchanged)
    let planned = match execute_stage(
        &mut trace,
        &mut timings,
        "plan_shadow",
        state,
        routed,
        stages::plan_shadow_stage,
    ) {
        Ok(context) => context.with_authority_decision(authority_receipt),
        Err(error) => {
            tracing::error!("plan_shadow_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };
    if debate_core == DebateCoreMode::TraceOnly {
        if let Some(trace) = trace.as_deref_mut() {
            match debate::observe(&planned) {
                Ok(receipt) => trace.set_debate_receipt(receipt),
                Err(error) => tracing::warn!("debate observation skipped: {error}"),
            }
        }
    }

    // Stage 4: Render
    let rendered = match execute_stage(
        &mut trace,
        &mut timings,
        "render",
        state,
        planned,
        |state, planned| stages::render_stage(state, planned, effective_renderer_authority),
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("render_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };
    recovery.path_depth = Some(rendered.path_depth());
    let active_packs = qxfx0_semantic::active_pack_set();
    let rendered_receipt = if fact_grounded_rollout.observes() {
        match fact_grounded::RenderedPlanReceipt::from_rendered(&rendered, state, active_packs) {
            Ok(receipt) => Ok(receipt),
            Err(error) if fact_grounded_rollout.permits_render_authorization() => {
                tracing::error!("fact-grounded receipt failed: {error}");
                *state = snapshot;
                return recovery_output(state, &recovery);
            }
            Err(error) => Err(error),
        }
    } else {
        Ok(None)
    };

    // Stage 5: Finalize
    let finalized = match execute_stage(
        &mut trace,
        &mut timings,
        "finalize",
        state,
        rendered,
        stages::finalize_stage,
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("finalize_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };

    // Stage 6: Guard
    let guarded = match execute_stage(
        &mut trace,
        &mut timings,
        "guard",
        state,
        finalized,
        stages::guard_stage,
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("guard_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };
    if response_plan_v2_authority == ResponsePlanV2Authority::Canary {
        let v2_authorized = guarded
            .finalized()
            .rendered()
            .planned()
            .authority_decision()
            .is_some_and(AuthorityDecisionReceipt::can_emit_v2);
        let classification = if !v2_authorized {
            "authority_denied_before_render"
        } else if guarded.blocked() {
            "v2_rendered_guard_blocked"
        } else {
            "v2_successfully_emitted"
        };
        if let Some(trace) = trace.as_deref_mut() {
            trace.set_authority_guard_classification(classification);
        }
    }
    if let Some(rejection) = guarded.rejection() {
        // A guard rejection is an expected turn outcome, not a pipeline fault.
        tracing::warn!("guard rejected turn: {rejection}");
    }

    if fact_grounded_rollout.observes() {
        let outcome = if guarded.blocked() {
            Ok(None)
        } else {
            match &rendered_receipt {
                Ok(Some(receipt)) => fact_grounded::finalize_fact_grounded_state(
                    fact_grounded_rollout,
                    state,
                    receipt,
                    active_packs,
                )
                .map(Some),
                Ok(None) => Ok(None),
                Err(error) => Err(error.clone()),
            }
        };
        if let Some(trace) = trace.as_deref_mut() {
            record_fact_grounded_trace(
                trace,
                fact_grounded_rollout,
                state,
                guarded.blocked(),
                rendered_receipt.as_ref().ok().and_then(Option::as_ref),
                &outcome,
            );
        }
        if let Err(error) = outcome {
            if fact_grounded_rollout.permits_render_authorization() {
                tracing::error!("fact-grounded finalize failed: {error}");
                *state = snapshot;
                return recovery_output(state, &recovery);
            }
        }
    }

    // Stage 7: Persist
    let persisted = match execute_stage(
        &mut trace,
        &mut timings,
        "persist",
        state,
        guarded,
        stages::persist_stage,
    ) {
        Ok(context) => context,
        Err(never) => match never {},
    };

    let context = persisted.guarded();
    let mut response = context.finalized().rendered().response().to_owned();
    let family = context.family();
    let guard_status = context.guard_status().clone();
    let blocked = context.blocked();
    let routed = context.finalized().rendered().routed();
    let subject = routed.prepared().input().subject().to_owned();
    let conversation_state = format!("{:?}", routed.conversation_state());
    let conatus_energy = routed.prepared().conatus_energy();
    let path_depth = context.finalized().rendered().path_depth();

    // A rejected response must not mutate semantic/self state. Governance and
    // the explicit blocked decision remain, then dialogue bookkeeping below
    // records that a rejected turn occurred.
    if blocked {
        state.semantic = snapshot.semantic.clone();
        state.dialogue.conversation_state = snapshot.dialogue.conversation_state;
    }

    // W6: If the guard blocked this turn, replace the response with a recovery string
    // before it is stored in history or returned to the user.
    if blocked {
        response = "QxFx0: ответ отклонён системой безопасности.".into();
    }

    // State sync — on blocked turns, still advance turn_count and record
    // history, but skip field adjustments (the response was rejected).
    state.dialogue.turn_count += 1;
    state.dialogue.last_family = family;
    state.dialogue.last_topic = Some(subject.clone());
    state.dialogue.history.push(response.clone());
    if state.dialogue.history.len() > 10_000 {
        let excess = state.dialogue.history.len() - 10_000;
        state.dialogue.history.drain(0..excess);
    }

    // Field adjustments — skip on blocked turns (rejected output should not
    // reinforce confidence or counterfactual).
    if !blocked {
        let topic_in_graph = state
            .semantic
            .runtime_graph
            .atoms
            .contains_key(&AtomId::new(subject.clone()));
        if topic_in_graph {
            state.semantic.field.confidence = (state.semantic.field.confidence + 0.1).min(1.0);
            state.semantic.field.resonance = (state.semantic.field.resonance + 0.05).min(1.0);
            // Positive atmosphere: known topic → valence drifts positive,
            // arousal increases slightly (engagement).
            state.semantic.field.atmosphere.valence =
                (state.semantic.field.atmosphere.valence + 0.05).min(1.0);
            state.semantic.field.atmosphere.arousal =
                (state.semantic.field.atmosphere.arousal + 0.03).min(1.0);
        } else {
            state.semantic.field.counterfactual =
                (state.semantic.field.counterfactual + 0.1).min(1.0);
            // Unknown topic → valence drifts negative (uncertainty),
            // arousal increases (heightened alertness).
            state.semantic.field.atmosphere.valence =
                (state.semantic.field.atmosphere.valence - 0.05).max(-1.0);
            state.semantic.field.atmosphere.arousal =
                (state.semantic.field.atmosphere.arousal + 0.05).min(1.0);
        }
        // Decay arousal slightly each turn (baseline calm).
        state.semantic.field.atmosphere.arousal =
            (state.semantic.field.atmosphere.arousal - 0.02).max(0.0);
    }

    let commitment_engaged = if let Some(store) = &state.semantic.semantic_commitments {
        let eng = qxfx0_commitment::CommitmentOps::detect_engagement(store, &subject);
        !eng.engaged_ids.is_empty()
    } else {
        false
    };

    TurnOutput {
        response,
        family,
        guard_status,
        blocked,
        commitment_engaged,
        governance_events: state.governance_log.len(),
        conatus_energy,
        path_depth,
        holistic_dominant: state.semantic.adjunction.holistic_dominant,
        conversation_state,
    }
}

fn record_fact_grounded_trace(
    trace: &mut execution_trace::PipelineTrace,
    rollout: fact_grounded::FactGroundedRollout,
    state: &SystemState,
    blocked: bool,
    receipt: Option<&fact_grounded::RenderedPlanReceipt>,
    outcome: &Result<
        Option<fact_grounded::FactGroundedFinalize>,
        fact_grounded::FactGroundedCompositionError,
    >,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(
        rollout,
        blocked,
        receipt.map(fact_grounded::RenderedPlanReceipt::response_digest),
    ))
    .unwrap_or_else(|error| format!("digest-error:{error}"));
    let mut metadata = BTreeMap::from([
        ("rollout".into(), format!("{rollout:?}").to_lowercase()),
        ("blocked".into(), blocked.to_string()),
        ("receipt_present".into(), receipt.is_some().to_string()),
    ]);
    if let Some(receipt) = receipt {
        metadata.insert(
            "topic".into(),
            receipt.binding().stance_topic().as_str().into(),
        );
        metadata.insert(
            "concept_id".into(),
            receipt.binding().concept_id().0.clone(),
        );
        metadata.insert(
            "thesis_fact_id".into(),
            receipt.binding().thesis_fact_id().0.clone(),
        );
    }
    let status = match outcome {
        Ok(Some(fact_grounded::FactGroundedFinalize::Applied(update))) => {
            metadata.insert("episodes_added".into(), update.episodes_added.to_string());
            "applied"
        }
        Ok(Some(fact_grounded::FactGroundedFinalize::Observed { claim_count, .. })) => {
            metadata.insert("claim_count".into(), claim_count.to_string());
            "observed"
        }
        Ok(Some(fact_grounded::FactGroundedFinalize::Skipped(_))) => "skipped",
        Ok(None) if blocked => "blocked",
        Ok(None) => "no_audited_plan_receipt",
        Err(error) => {
            metadata.insert("error".into(), error.to_string());
            "rejected"
        }
    };
    metadata.insert("status".into(), status.into());
    let output_digest = execution_trace::calculate_stable_digest(&(state, &metadata))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "fact_grounded_finalize",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

/// Record pure doubt/episodic evidence after topic normalization. The local
/// store is constructed from an already-persisted confirmed decision only;
/// it is neither retained nor applied to routing.
fn record_doubt_shadow(
    trace: &mut execution_trace::PipelineTrace,
    doubt_shadow: DoubtShadowMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let config = qxfx0_self::doubt::EpisodicConfig::default();
    let mut metadata = BTreeMap::from([
        (
            "doubt_shadow_enabled".into(),
            doubt_shadow.enabled().to_string(),
        ),
        ("doubt_recall_count".into(), "0".into()),
        (
            "doubt_episodic_capacity".into(),
            config.capacity.to_string(),
        ),
        ("doubt_recall_limit".into(), config.recall_limit.to_string()),
    ]);

    if doubt_shadow.enabled() {
        let mut store = qxfx0_self::doubt::BoundedEpisodicStore::new(config);
        let confirmed_previous_decision =
            state.last_turn_decision.as_ref().is_some_and(|decision| {
                matches!(
                    decision.guard_status,
                    GuardStatus::Allowed | GuardStatus::InvariantWarn(_)
                )
            });
        if confirmed_previous_decision {
            if let Some(topic) = state.dialogue.last_topic.clone() {
                store = store.record(qxfx0_types::EpisodicEvent {
                    id: state.dialogue.turn_count as u64,
                    turn: state.dialogue.turn_count as u64,
                    kind: qxfx0_types::EpisodicKind::SystemDecision,
                    topic: Some(topic),
                });
            }
        }

        let driver = qxfx0_types::DoubtDriver::Other;
        let score = qxfx0_self::doubt::compute_doubt(qxfx0_types::DoubtInput {
            confidence: state.semantic.field.confidence,
            driver,
        });
        let recalled = store.recall(state.dialogue.turn_count as u64, Some(&proposition.subject));
        let proposed = qxfx0_self::doubt::route_for_doubt(
            score,
            qxfx0_self::doubt::DoubtPolicy::default(),
            &recalled,
        );
        metadata.extend([
            ("doubt_score".into(), score.value().to_string()),
            ("doubt_driver".into(), format!("{driver:?}").to_lowercase()),
            ("doubt_recall_count".into(), recalled.len().to_string()),
            (
                "doubt_proposed_route".into(),
                doubt_route_name(proposed).into(),
            ),
            ("doubt_reason".into(), "observation_only".into()),
        ]);
    } else {
        metadata.extend([
            ("doubt_score".into(), "not_evaluated".into()),
            ("doubt_driver".into(), "not_evaluated".into()),
            ("doubt_proposed_route".into(), "not_evaluated".into()),
            ("doubt_reason".into(), "disabled".into()),
        ]);
    }

    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "doubt_shadow",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

const ANOMALY_SHADOW_LEDGER_CAPACITY: usize = 1;

/// Record a typed recovery proposal after normalization without applying it.
///
/// Temporal evidence compares persisted typed system decisions with one local,
/// explicit affirmed candidate for the current turn. The candidate is never
/// retained here; this remains a trace-only recovery proposal.
fn record_anomaly_shadow(
    trace: &mut execution_trace::PipelineTrace,
    anomaly_shadow: AnomalyShadowMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
    is_challenge: bool,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let mut metadata = BTreeMap::from([
        (
            "anomaly_shadow_enabled".into(),
            anomaly_shadow.enabled().to_string(),
        ),
        (
            "anomaly_ledger_capacity".into(),
            ANOMALY_SHADOW_LEDGER_CAPACITY.to_string(),
        ),
        ("anomaly_temporal_evidence".into(), "disabled".into()),
        ("anomaly_temporal_history_count".into(), "0".into()),
    ]);

    if anomaly_shadow.enabled() {
        let mut ledger =
            qxfx0_self::anomaly::AnomalyRecoveryLedger::new(ANOMALY_SHADOW_LEDGER_CAPACITY);
        let observed_turn = state.dialogue.turn_count.saturating_add(1);
        let self_reference = qxfx0_self::anomaly::AnomalyEvidence::SelfReference {
            turn: observed_turn,
            subject: proposition.subject.clone(),
            angst: state.semantic.essence.angst,
            witness_count: state.semantic.essence.witnesses.len(),
        };
        let anti_conatus = qxfx0_self::anomaly::AnomalyEvidence::AntiConatus {
            turn: observed_turn,
            stance_confidence: state.semantic.field.confidence,
            stance_consistent: !is_challenge,
            angst: state.semantic.essence.angst,
            conatus: state
                .semantic
                .essence
                .witnesses
                .last()
                .map(|witness| witness.conatus_scalar)
                .unwrap_or(f64::MAX),
        };
        let temporal = qxfx0_types::stance::StanceTopic::new(proposition.subject.clone())
            .ok()
            .and_then(|topic| {
                let current = qxfx0_types::stance::StanceObservation {
                    turn: observed_turn,
                    topic,
                    polarity: qxfx0_types::stance::StancePolarity::Affirmed,
                    source: qxfx0_types::stance::StanceSource::SystemDecision,
                };
                qxfx0_types::stance::detect_temporal_contradiction(
                    &state.semantic.stance_provenance,
                    &current,
                )
            });
        metadata.extend([
            (
                "anomaly_temporal_evidence".into(),
                "typed_persisted_provenance".into(),
            ),
            (
                "anomaly_temporal_history_count".into(),
                state.semantic.stance_provenance.len().to_string(),
            ),
        ]);
        let decision = qxfx0_self::anomaly::detect_anomaly(self_reference)
            .or_else(|| qxfx0_self::anomaly::detect_anomaly(anti_conatus))
            .or_else(|| {
                temporal.and_then(|contradiction| {
                    qxfx0_self::anomaly::detect_anomaly(contradiction.to_anomaly_evidence())
                })
            });

        if let Some(decision) = decision {
            let outcome = ledger.record(decision, input_digest.clone());
            let (replay_outcome, recovery) = match outcome {
                qxfx0_self::anomaly::AnomalyReplayOutcome::Proposed(trace) => ("proposed", trace),
                qxfx0_self::anomaly::AnomalyReplayOutcome::NoStateTransition(trace) => {
                    ("no_state_transition", trace)
                }
            };
            metadata.extend([
                (
                    "anomaly_proposed_kind".into(),
                    anomaly_kind_name(recovery.kind).into(),
                ),
                (
                    "anomaly_strategy".into(),
                    anomaly_strategy_name(recovery.strategy).into(),
                ),
                (
                    "anomaly_result".into(),
                    anomaly_result_name(recovery.result).into(),
                ),
                ("anomaly_idempotency_key".into(), recovery.idempotency_key),
                ("anomaly_replay_outcome".into(), replay_outcome.into()),
                ("anomaly_ledger_len".into(), ledger.len().to_string()),
                ("anomaly_reason".into(), "observation_only".into()),
            ]);
        } else {
            metadata.extend([
                ("anomaly_proposed_kind".into(), "not_detected".into()),
                ("anomaly_strategy".into(), "not_applicable".into()),
                ("anomaly_result".into(), "not_applicable".into()),
                ("anomaly_idempotency_key".into(), "not_applicable".into()),
                ("anomaly_replay_outcome".into(), "not_applicable".into()),
                ("anomaly_ledger_len".into(), ledger.len().to_string()),
                ("anomaly_reason".into(), "no_admitted_evidence".into()),
            ]);
        }
    } else {
        metadata.extend([
            ("anomaly_proposed_kind".into(), "not_evaluated".into()),
            ("anomaly_strategy".into(), "not_evaluated".into()),
            ("anomaly_result".into(), "not_evaluated".into()),
            ("anomaly_idempotency_key".into(), "not_evaluated".into()),
            ("anomaly_replay_outcome".into(), "not_evaluated".into()),
            ("anomaly_ledger_len".into(), "0".into()),
            ("anomaly_reason".into(), "disabled".into()),
        ]);
    }

    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "anomaly_shadow",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

const fn anomaly_kind_name(kind: qxfx0_self::anomaly::AnomalyKind) -> &'static str {
    match kind {
        qxfx0_self::anomaly::AnomalyKind::SelfReferentialCollapse => "self_referential_collapse",
        qxfx0_self::anomaly::AnomalyKind::Temporal => "temporal",
        qxfx0_self::anomaly::AnomalyKind::Unclassifiable => "unclassifiable",
        qxfx0_self::anomaly::AnomalyKind::AntiConatus => "anti_conatus",
    }
}

const fn anomaly_strategy_name(
    strategy: qxfx0_self::anomaly::AnomalyRecoveryStrategy,
) -> &'static str {
    match strategy {
        qxfx0_self::anomaly::AnomalyRecoveryStrategy::ResetEssence => "reset_essence",
        qxfx0_self::anomaly::AnomalyRecoveryStrategy::RestrictRoute => "restrict_route",
        qxfx0_self::anomaly::AnomalyRecoveryStrategy::RequestRevision => "request_revision",
    }
}

const fn anomaly_result_name(result: qxfx0_self::anomaly::AnomalyRecoveryResult) -> &'static str {
    match result {
        qxfx0_self::anomaly::AnomalyRecoveryResult::EssenceReset => "essence_reset",
        qxfx0_self::anomaly::AnomalyRecoveryResult::RouteRestricted => "route_restricted",
        qxfx0_self::anomaly::AnomalyRecoveryResult::RevisionRequested => "revision_requested",
    }
}

#[derive(Debug, Clone, Copy)]
struct ClarificationDecision {
    proposed: Option<qxfx0_types::DoubtRoute>,
    score: Option<qxfx0_types::DoubtScore>,
    applied: bool,
    suppression_eligible: bool,
    suppression_applied: bool,
    recall_count: usize,
}

fn clarification_decision(
    clarification: ClarificationMode,
    suppression: SameTopicSuppressionMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
) -> ClarificationDecision {
    if !clarification.observes() || !clarification_mode_is_eligible(proposition.mode) {
        return ClarificationDecision {
            proposed: None,
            score: None,
            applied: false,
            suppression_eligible: false,
            suppression_applied: false,
            recall_count: 0,
        };
    }
    let score = qxfx0_self::doubt::compute_doubt(qxfx0_types::DoubtInput {
        confidence: state.semantic.field.confidence,
        driver: qxfx0_types::DoubtDriver::Other,
    });
    let proposed =
        qxfx0_self::doubt::route_for_doubt(score, qxfx0_self::doubt::DoubtPolicy::default(), &[]);
    let recalled = if suppression.observes() && proposed == qxfx0_types::DoubtRoute::Clarify {
        immediate_confirmed_same_topic(state, &proposition.subject)
    } else {
        Vec::new()
    };
    let suppression_route = qxfx0_self::doubt::route_for_doubt(
        score,
        qxfx0_self::doubt::DoubtPolicy::default(),
        &recalled,
    );
    let suppression_eligible =
        suppression_route == qxfx0_types::DoubtRoute::SuppressedByRecentDecision;
    let suppression_applied =
        clarification.applies() && suppression.applies() && suppression_eligible;
    ClarificationDecision {
        proposed: Some(proposed),
        score: Some(score),
        applied: clarification.applies()
            && proposed == qxfx0_types::DoubtRoute::Clarify
            && !suppression_applied,
        suppression_eligible,
        suppression_applied,
        recall_count: recalled.len(),
    }
}

fn immediate_confirmed_same_topic(
    state: &SystemState,
    topic: &str,
) -> Vec<qxfx0_types::EpisodicEvent> {
    let confirmed = state.last_turn_decision.as_ref().is_some_and(|decision| {
        matches!(
            decision.guard_status,
            GuardStatus::Allowed | GuardStatus::InvariantWarn(_)
        )
    });
    let Some(previous_topic) = confirmed
        .then(|| state.dialogue.last_topic.clone())
        .flatten()
    else {
        return Vec::new();
    };
    let store =
        qxfx0_self::doubt::BoundedEpisodicStore::default().record(qxfx0_types::EpisodicEvent {
            id: state.dialogue.turn_count as u64,
            turn: state.dialogue.turn_count as u64,
            kind: qxfx0_types::EpisodicKind::SystemDecision,
            topic: Some(previous_topic),
        });
    store.recall(state.dialogue.turn_count as u64, Some(topic))
}

const fn clarification_mode_is_eligible(mode: qxfx0_semantic::PropositionMode) -> bool {
    matches!(
        mode,
        qxfx0_semantic::PropositionMode::Define
            | qxfx0_semantic::PropositionMode::Assert
            | qxfx0_semantic::PropositionMode::Connect
            | qxfx0_semantic::PropositionMode::Reflect
    )
}

fn record_clarification_route(
    trace: &mut execution_trace::PipelineTrace,
    clarification: ClarificationMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
    decision: ClarificationDecision,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let proposed = decision
        .proposed
        .map(doubt_route_name)
        .unwrap_or("not_evaluated");
    let metadata = BTreeMap::from([
        (
            "clarification_enabled".into(),
            clarification.observes().to_string(),
        ),
        (
            "clarification_mode".into(),
            match clarification {
                ClarificationMode::Disabled => "disabled",
                ClarificationMode::TraceOnly => "trace_only",
                ClarificationMode::LimitedEnabled => "limited_enabled",
            }
            .into(),
        ),
        (
            "clarification_score".into(),
            decision
                .score
                .map(|score| score.value().to_string())
                .unwrap_or_else(|| "not_evaluated".into()),
        ),
        ("clarification_proposed_route".into(), proposed.into()),
        ("clarification_applied".into(), decision.applied.to_string()),
        (
            "clarification_reason".into(),
            if decision.applied {
                "low_confidence"
            } else if clarification.observes() {
                "observation_only"
            } else {
                "disabled"
            }
            .into(),
        ),
    ]);
    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "clarification_route",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

fn record_same_topic_suppression(
    trace: &mut execution_trace::PipelineTrace,
    suppression: SameTopicSuppressionMode,
    state: &SystemState,
    proposition: &qxfx0_semantic::ParsedProposition,
    decision: ClarificationDecision,
) {
    let input_digest = execution_trace::calculate_stable_digest(&(state, proposition))
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    let metadata = BTreeMap::from([
        (
            "same_topic_suppression_enabled".into(),
            suppression.observes().to_string(),
        ),
        (
            "same_topic_suppression_mode".into(),
            match suppression {
                SameTopicSuppressionMode::Disabled => "disabled",
                SameTopicSuppressionMode::TraceOnly => "trace_only",
                SameTopicSuppressionMode::LimitedEnabled => "limited_enabled",
            }
            .into(),
        ),
        (
            "same_topic_suppression_recall_count".into(),
            decision.recall_count.to_string(),
        ),
        (
            "same_topic_suppression_eligible".into(),
            decision.suppression_eligible.to_string(),
        ),
        (
            "same_topic_suppression_applied".into(),
            decision.suppression_applied.to_string(),
        ),
        (
            "same_topic_suppression_actual_route".into(),
            if decision.suppression_applied {
                "retain_current"
            } else {
                "unchanged"
            }
            .into(),
        ),
        (
            "same_topic_suppression_reason".into(),
            if decision.suppression_eligible {
                "immediate_confirmed_same_topic"
            } else if suppression.observes() {
                "not_eligible"
            } else {
                "disabled"
            }
            .into(),
        ),
    ]);
    let output_digest = execution_trace::calculate_stable_digest(&metadata)
        .unwrap_or_else(|error| format!("digest-error:{error}"));
    trace.record_step(
        "same_topic_suppression",
        input_digest,
        output_digest,
        Duration::ZERO,
        metadata,
    );
}

const fn doubt_route_name(route: qxfx0_types::DoubtRoute) -> &'static str {
    match route {
        qxfx0_types::DoubtRoute::RetainCurrent => "retain_current",
        qxfx0_types::DoubtRoute::Clarify => "clarify",
        qxfx0_types::DoubtRoute::SuppressedByRecentDecision => "suppressed_by_recent_decision",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct AcceptingSignatureVerifier;

    impl qxfx0_types::StanceDecisionSignatureVerifier for AcceptingSignatureVerifier {
        fn verify_signature(
            &self,
            _issuer_id: &str,
            _key_id: &str,
            canonical_payload: &[u8],
            _signature: &[u8; 64],
        ) -> Result<(), qxfx0_types::StanceVerificationError> {
            assert!(!canonical_payload.is_empty());
            Ok(())
        }
    }

    fn signed_stance_for(input: &TurnInput, topic: &str) -> qxfx0_types::SignedStanceDecision {
        qxfx0_types::SignedStanceDecision {
            attestation: qxfx0_types::StanceDecisionAttestation {
                version: qxfx0_types::STANCE_ATTESTATION_VERSION,
                issuer_id: "test-issuer".into(),
                key_id: "test-key-1".into(),
                audience: "qxfx0-test".into(),
                session_id: input.session_id.clone(),
                expected_pre_turn: 0,
                topic: qxfx0_types::StanceTopic::new(topic).unwrap(),
                polarity: qxfx0_types::StancePolarity::Rejected,
                request_digest: qxfx0_types::calculate_stance_request_digest(
                    &input.session_id,
                    &input.raw_text,
                ),
                decision_id: [7; 16],
                issued_at_unix_seconds: 100,
                expires_at_unix_seconds: 200,
            },
            signature: [1; 64],
        }
    }

    fn signed_stance_policy() -> qxfx0_types::StanceAuthorityVerificationPolicy {
        qxfx0_types::StanceAuthorityVerificationPolicy {
            audience: "qxfx0-test".into(),
            verification_time_unix_seconds: 150,
            max_validity_seconds: 300,
        }
    }

    fn test_state(session_id: &str) -> SystemState {
        SystemState {
            session_id: session_id.into(),
            ..SystemState::default()
        }
    }

    #[test]
    fn test_pipeline_process_turn_define() {
        let mut state = test_state("test");
        let input = TurnInput {
            session_id: "test".into(),
            raw_text: "что такое свобода?".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(!output.response.is_empty());
    }

    #[test]
    fn response_plan_v2_shadow_is_observational_and_replay_stable() {
        let input = parity_input("v2-shadow");
        let mut baseline = test_state("v2-shadow");
        let baseline_output = process_turn_with_options(&input, &mut baseline, TurnOptions::new());
        let mut shadow = test_state("v2-shadow");
        let (shadow_output, trace) = process_turn_with_options_and_trace(
            &input,
            &mut shadow,
            TurnOptions::new().with_response_plan_v2(ResponsePlanV2Mode::Shadow),
        );
        assert_eq!(baseline_output.response, shadow_output.response);
        assert_eq!(baseline_output.blocked, shadow_output.blocked);
        assert_eq!(
            execution_trace::calculate_stable_digest(&baseline).unwrap(),
            execution_trace::calculate_stable_digest(&shadow).unwrap()
        );
        let step = trace
            .steps
            .iter()
            .find(|step| step.stage == "response_plan_v2")
            .expect("V2 shadow trace step");
        assert_eq!(step.metadata.get("v1_authoritative"), Some(&"true".into()));
    }

    #[test]
    fn response_plan_v2_shadow_covers_30_topics_and_69_claims() {
        let registry = qxfx0_semantic::argued_topic_registry().expect("audited registry");
        let mut topics = 0usize;
        let mut claims = 0usize;
        for topic in registry.topics() {
            topics += 1;
            claims += topic.statement_count();
            let session = format!("v2-corpus-{topics}");
            let input = TurnInput {
                session_id: session.clone(),
                raw_text: format!("что такое {}?", topic.topic().as_str()),
            };
            let mut baseline = test_state(&session);
            let baseline_output =
                process_turn_with_options(&input, &mut baseline, TurnOptions::new());
            let mut shadow = test_state(&session);
            let (shadow_output, trace) = process_turn_with_options_and_trace(
                &input,
                &mut shadow,
                TurnOptions::new().with_response_plan_v2(ResponsePlanV2Mode::Shadow),
            );
            assert_eq!(
                baseline_output.response,
                shadow_output.response,
                "{}",
                topic.topic().as_str()
            );
            assert_eq!(
                execution_trace::calculate_stable_digest(&baseline).unwrap(),
                execution_trace::calculate_stable_digest(&shadow).unwrap(),
                "{}",
                topic.topic().as_str()
            );
            let step = trace
                .steps
                .iter()
                .find(|step| step.stage == "response_plan_v2")
                .expect("V2 corpus trace step");
            assert_eq!(
                step.metadata.get("semantic_parity"),
                Some(&"true".into()),
                "{}",
                topic.topic().as_str()
            );
            assert_eq!(
                step.metadata.get("authority_parity"),
                Some(&"true".into()),
                "{}",
                topic.topic().as_str()
            );
            assert_eq!(
                step.metadata.get("legacy_graph_v2_declarative_fallback"),
                Some(&"false".into())
            );
        }
        assert_eq!(topics, 30);
        assert_eq!(claims, 69);
    }

    fn parity_input(session_id: &str) -> TurnInput {
        TurnInput {
            session_id: session_id.into(),
            raw_text: "что такое свобода?".into(),
        }
    }

    /// A named wrapper under parity test, paired with the option set it must
    /// be equivalent to.
    type ParityCase<R> = (
        &'static str,
        TurnOptions,
        fn(&TurnInput, &mut SystemState) -> R,
    );
    type OutputParityCase = ParityCase<TurnOutput>;
    type TraceParityCase = ParityCase<(TurnOutput, execution_trace::PipelineTrace)>;

    /// Every named wrapper must be exactly its `TurnOptions` equivalent, in
    /// both the returned output and the resulting persisted state. This is the
    /// lock that lets the wrappers stay thin: if one ever grows behaviour of
    /// its own, this fails.
    #[test]
    fn named_wrappers_equal_their_turn_options_equivalent() {
        let expectations: Vec<OutputParityCase> = vec![
            ("process_turn", TurnOptions::new(), |input, state| {
                process_turn(input, state)
            }),
            (
                "with_renderer",
                TurnOptions::new().with_renderer(RendererAuthority::AuditedPlan),
                |input, state| {
                    process_turn_with_renderer(input, state, RendererAuthority::AuditedPlan)
                },
            ),
            (
                "with_renderer_and_fact_grounded",
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_fact_grounded(fact_grounded::FactGroundedRollout::Shadow),
                |input, state| {
                    process_turn_with_renderer_and_fact_grounded(
                        input,
                        state,
                        RendererAuthority::AuditedPlan,
                        fact_grounded::FactGroundedRollout::Shadow,
                    )
                },
            ),
        ];

        for (label, options, wrapper) in expectations {
            let mut wrapper_state = test_state("parity");
            let wrapper_output = wrapper(&parity_input("parity"), &mut wrapper_state);

            let mut options_state = test_state("parity");
            let options_output =
                process_turn_with_options(&parity_input("parity"), &mut options_state, options);

            assert_eq!(
                wrapper_output.response, options_output.response,
                "{label}: response diverged from its TurnOptions equivalent"
            );
            assert_eq!(
                wrapper_output.family, options_output.family,
                "{label}: family diverged"
            );
            assert_eq!(
                wrapper_output.blocked, options_output.blocked,
                "{label}: blocked flag diverged"
            );
            assert_eq!(
                execution_trace::calculate_stable_digest(&wrapper_state).unwrap(),
                execution_trace::calculate_stable_digest(&options_state).unwrap(),
                "{label}: persisted state diverged"
            );
        }
    }

    /// The trace-collecting wrappers must likewise match, including the
    /// replay-visible stage sequence.
    #[test]
    fn trace_wrappers_equal_their_turn_options_equivalent() {
        let cases: Vec<TraceParityCase> = vec![
            ("with_trace", TurnOptions::new(), |input, state| {
                process_turn_with_trace(input, state)
            }),
            (
                "with_trace_and_renderer_and_doubt_shadow",
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_doubt_shadow(DoubtShadowMode::TraceOnly),
                |input, state| {
                    process_turn_with_trace_and_renderer_and_doubt_shadow(
                        input,
                        state,
                        RendererAuthority::AuditedPlan,
                        DoubtShadowMode::TraceOnly,
                    )
                },
            ),
            (
                "with_trace_and_renderer_and_anomaly_shadow",
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_anomaly_shadow(AnomalyShadowMode::TraceOnly),
                |input, state| {
                    process_turn_with_trace_and_renderer_and_anomaly_shadow(
                        input,
                        state,
                        RendererAuthority::AuditedPlan,
                        AnomalyShadowMode::TraceOnly,
                    )
                },
            ),
            (
                "with_trace_and_renderer_and_features_and_suppression",
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_doubt_shadow(DoubtShadowMode::TraceOnly)
                    .with_clarification(ClarificationMode::TraceOnly)
                    .with_suppression(SameTopicSuppressionMode::TraceOnly),
                |input, state| {
                    process_turn_with_trace_and_renderer_and_features_and_suppression(
                        input,
                        state,
                        RendererAuthority::AuditedPlan,
                        DoubtShadowMode::TraceOnly,
                        ClarificationMode::TraceOnly,
                        SameTopicSuppressionMode::TraceOnly,
                    )
                },
            ),
        ];

        for (label, options, wrapper) in cases {
            let mut wrapper_state = test_state("trace-parity");
            let (wrapper_output, wrapper_trace) =
                wrapper(&parity_input("trace-parity"), &mut wrapper_state);

            let mut options_state = test_state("trace-parity");
            let (options_output, options_trace) = process_turn_with_options_and_trace(
                &parity_input("trace-parity"),
                &mut options_state,
                options,
            );

            assert_eq!(
                wrapper_output.response, options_output.response,
                "{label}: response diverged"
            );
            assert_eq!(
                wrapper_trace
                    .steps
                    .iter()
                    .map(|step| step.stage.as_str())
                    .collect::<Vec<_>>(),
                options_trace
                    .steps
                    .iter()
                    .map(|step| step.stage.as_str())
                    .collect::<Vec<_>>(),
                "{label}: trace stage sequence diverged"
            );
            assert_eq!(
                execution_trace::calculate_stable_digest(&wrapper_state).unwrap(),
                execution_trace::calculate_stable_digest(&options_state).unwrap(),
                "{label}: persisted state diverged"
            );
        }
    }

    /// A default option set is the standard production path, so it must be
    /// byte-identical to the bare `process_turn` entry point.
    #[test]
    fn default_turn_options_are_the_standard_production_path() {
        assert_eq!(
            TurnOptions::default().renderer_authority,
            RendererAuthority::LegacyShadow
        );
        assert_eq!(
            TurnOptions::default().fact_grounded,
            fact_grounded::FactGroundedRollout::Disabled
        );

        let mut bare_state = test_state("default-parity");
        let bare = process_turn(&parity_input("default-parity"), &mut bare_state);

        let mut options_state = test_state("default-parity");
        let via_options = process_turn_with_options(
            &parity_input("default-parity"),
            &mut options_state,
            TurnOptions::default(),
        );

        assert_eq!(bare.response, via_options.response);
        assert_eq!(
            execution_trace::calculate_stable_digest(&bare_state).unwrap(),
            execution_trace::calculate_stable_digest(&options_state).unwrap()
        );
    }

    #[test]
    fn timed_pipeline_preserves_the_standard_turn_output() {
        let input = TurnInput {
            session_id: "timed".into(),
            raw_text: "что такое свобода?".into(),
        };
        let mut standard_state = test_state("timed");
        let mut timed_state = test_state("timed");

        let standard = process_turn(&input, &mut standard_state);
        let (timed, timings) = process_turn_with_timing_and_renderer(
            &input,
            &mut timed_state,
            RendererAuthority::LegacyShadow,
        );

        assert_eq!(timed.response, standard.response);
        assert_eq!(timed.family, standard.family);
        let encoded = serde_json::to_value(timings).expect("timing should serialize");
        for field in [
            "input_normalization_ms",
            "semantic_selection_ms",
            "plan_render_ms",
            "guard_ms",
            "total_ms",
        ] {
            assert!(encoded.get(field).is_some(), "missing timing field {field}");
        }
    }

    #[test]
    fn test_pipeline_process_turn_challenge() {
        let mut state = test_state("test-ch");
        let input = TurnInput {
            session_id: "test-ch".into(),
            raw_text: "свобода это просто отсутствие ограничений".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(!output.response.is_empty());
    }

    #[test]
    fn test_pipeline_multi_turn_no_crash() {
        let mut state = test_state("multi");
        let inputs = [
            "что такое свобода?",
            "свобода это просто вседозволенность",
            "что ты думаешь об ответственности?",
        ];
        for text in &inputs {
            let input = TurnInput {
                session_id: "multi".into(),
                raw_text: text.to_string(),
            };
            let _output = process_turn(&input, &mut state);
        }
    }

    #[test]
    fn test_pipeline_determinism_same_input_same_output() {
        let inputs = ["что такое свобода?", "что ты думаешь об истине?"];
        for text in &inputs {
            let mut state1 = test_state("det");
            let mut state2 = test_state("det");

            let input = TurnInput {
                session_id: "det".into(),
                raw_text: text.to_string(),
            };
            let out1 = process_turn(&input, &mut state1);
            let out2 = process_turn(&input, &mut state2);
            assert_eq!(
                out1.response, out2.response,
                "same input must produce same output"
            );
            assert_eq!(out1.family, out2.family);
            assert_eq!(out1.conatus_energy, out2.conatus_energy);
            assert_eq!(out1.path_depth, out2.path_depth);
        }
    }

    #[test]
    fn test_pipeline_path_depth_nonzero() {
        let mut state = test_state("depth");
        let input = TurnInput {
            session_id: "depth".into(),
            raw_text: "что такое свобода?".into(),
        };
        let output = process_turn(&input, &mut state);
        assert!(
            output.path_depth > 0,
            "path_depth should be non-zero for known topic"
        );
    }

    #[test]
    fn test_pipeline_blocked_turn_no_field_change() {
        let mut state = test_state("block");
        let field_before = state.semantic.field.clone();

        // Use a very long string to trigger a safety/quality block in ContentQualityGate
        let input = TurnInput {
            session_id: "block".into(),
            raw_text: "a".repeat(10_000),
        };
        let output = process_turn(&input, &mut state);
        assert!(
            output.blocked,
            "Turn should be blocked for excessively long input"
        );
        assert_eq!(
            state.semantic.field.confidence, field_before.confidence,
            "blocked turn should not change field confidence"
        );
    }

    #[test]
    fn fact_grounded_pipeline_rejects_blocked_fallback_and_legacy_evidence() {
        let enabled = fact_grounded::FactGroundedRollout::Enabled;

        let mut blocked = test_state("fact-grounded-blocked");
        let blocked_before = blocked.semantic.perspective.clone();
        let blocked_output = process_turn_with_renderer_and_fact_grounded(
            &TurnInput {
                session_id: blocked.session_id.clone(),
                raw_text: "a".repeat(10_000),
            },
            &mut blocked,
            RendererAuthority::AuditedPlan,
            enabled,
        );
        assert!(blocked_output.blocked);
        assert_eq!(blocked.semantic.perspective, blocked_before);
        assert!(blocked.semantic.pack_set_fingerprint.is_empty());

        let mut fallback = test_state("fact-grounded-fallback");
        let (_, fallback_trace) = process_turn_with_trace_and_renderer_and_fact_grounded(
            &TurnInput {
                session_id: fallback.session_id.clone(),
                raw_text: "что такое совершенно-неизвестный-термин?".into(),
            },
            &mut fallback,
            RendererAuthority::AuditedPlan,
            enabled,
        );
        assert!(fallback.semantic.perspective.opinions.is_empty());
        assert!(fallback.semantic.pack_set_fingerprint.is_empty());
        let plan_step = fallback_trace
            .steps
            .iter()
            .find(|step| step.stage == "plan_shadow")
            .expect("fallback turn must retain plan evidence");
        assert_eq!(
            plan_step.metadata.get("plan_outcome").map(String::as_str),
            Some("fallback")
        );
        let fact_step = fallback_trace
            .steps
            .iter()
            .find(|step| step.stage == "fact_grounded_finalize")
            .expect("fallback turn must record fact-grounded evidence");
        assert_eq!(
            fact_step.metadata.get("status").map(String::as_str),
            Some("no_audited_plan_receipt")
        );

        let mut legacy = test_state("fact-grounded-legacy");
        process_turn_with_renderer_and_fact_grounded(
            &TurnInput {
                session_id: legacy.session_id.clone(),
                raw_text: "что такое свобода?".into(),
            },
            &mut legacy,
            RendererAuthority::LegacyShadow,
            enabled,
        );
        assert!(legacy.semantic.perspective.opinions.is_empty());
        assert!(legacy.semantic.pack_set_fingerprint.is_empty());
    }

    #[test]
    fn fact_grounded_shadow_and_trace_only_are_deterministic_and_observational() {
        let input = TurnInput {
            session_id: "fact-grounded-observe".into(),
            raw_text: "что такое свобода?".into(),
        };
        for rollout in [
            fact_grounded::FactGroundedRollout::Shadow,
            fact_grounded::FactGroundedRollout::TraceOnly,
        ] {
            let mut first_state = test_state(&input.session_id);
            let mut replay_state = first_state.clone();
            let (first_output, first_trace) =
                process_turn_with_trace_and_renderer_and_fact_grounded(
                    &input,
                    &mut first_state,
                    RendererAuthority::AuditedPlan,
                    rollout,
                );
            let (replay_output, replay_trace) =
                process_turn_with_trace_and_renderer_and_fact_grounded(
                    &input,
                    &mut replay_state,
                    RendererAuthority::AuditedPlan,
                    rollout,
                );

            assert_eq!(first_output.response, replay_output.response);
            assert_eq!(
                serde_json::to_vec(&first_state).unwrap(),
                serde_json::to_vec(&replay_state).unwrap()
            );
            assert!(first_state.semantic.perspective.opinions.is_empty());
            assert!(first_state.semantic.pack_set_fingerprint.is_empty());
            assert_eq!(
                serde_json::to_vec(&first_trace).unwrap(),
                serde_json::to_vec(&replay_trace).unwrap()
            );
            let fact_step = first_trace
                .steps
                .iter()
                .find(|step| step.stage == "fact_grounded_finalize")
                .expect("observational rollout must produce a trace step");
            assert_eq!(
                fact_step.metadata.get("status").map(String::as_str),
                Some("observed")
            );
            assert_eq!(
                fact_step
                    .metadata
                    .get("receipt_present")
                    .map(String::as_str),
                Some("true")
            );
        }
    }

    #[test]
    fn fact_grounded_enabled_turn_updates_perspective_inside_pipeline_snapshot() {
        let input = TurnInput {
            session_id: "fact-grounded-enabled".into(),
            raw_text: "что такое свобода?".into(),
        };
        let mut state = test_state(&input.session_id);
        let output = process_turn_with_renderer_and_fact_grounded(
            &input,
            &mut state,
            RendererAuthority::AuditedPlan,
            fact_grounded::FactGroundedRollout::LimitedNonProduction,
        );
        assert!(!output.blocked);
        assert_eq!(
            state.semantic.pack_set_fingerprint,
            qxfx0_semantic::active_pack_set().fingerprint()
        );
        let opinion = state
            .semantic
            .perspective
            .opinions
            .get(&ConceptId("concept.свобода".into()))
            .expect("audited factual leaves must be finalized");
        assert_eq!(opinion.polarity, BeliefPolarity::Qualified);
        assert_eq!(state.semantic.perspective.episodes.len(), 3);
    }

    #[test]
    fn stance_provenance_is_default_off_and_guard_bounded() {
        let input = TurnInput {
            session_id: "stance".into(),
            raw_text: "что такое свобода?".into(),
        };
        let mut disabled = test_state("stance");
        let mut enabled = test_state("stance");
        let standard =
            process_turn_with_renderer(&input, &mut disabled, RendererAuthority::LegacyShadow);
        let recorded = process_turn_with_renderer_and_stance_provenance(
            &input,
            &mut enabled,
            RendererAuthority::LegacyShadow,
            StanceProvenanceMode::RecordAffirmedSystemDecision,
        );
        assert_eq!(standard.response, recorded.response);
        assert_eq!(standard.family, recorded.family);
        assert!(disabled.semantic.stance_provenance.is_empty());
        assert_eq!(enabled.semantic.stance_provenance.len(), 1);
        assert_eq!(enabled.semantic.stance_provenance.version(), 1);

        let mut blocked = test_state("stance-blocked");
        let blocked_output = process_turn_with_renderer_and_stance_provenance(
            &TurnInput {
                session_id: "stance-blocked".into(),
                raw_text: "a".repeat(10_000),
            },
            &mut blocked,
            RendererAuthority::LegacyShadow,
            StanceProvenanceMode::RecordAffirmedSystemDecision,
        );
        assert!(blocked_output.blocked);
        assert!(blocked.semantic.stance_provenance.is_empty());
    }

    #[test]
    fn explicit_rejected_stance_requires_matching_allowed_turn() {
        let input = TurnInput {
            session_id: "explicit-stance".into(),
            raw_text: "что такое свобода?".into(),
        };
        let decision = qxfx0_types::stance::SystemStanceDecision {
            topic: qxfx0_types::stance::StanceTopic::new("свобода").unwrap(),
            polarity: qxfx0_types::stance::StancePolarity::Rejected,
        };
        let mut recorded = test_state("explicit-stance");
        let output = process_turn_with_renderer_and_explicit_stance_decision(
            &input,
            &mut recorded,
            RendererAuthority::LegacyShadow,
            decision,
        );
        assert!(!output.blocked);
        assert_eq!(recorded.semantic.stance_provenance.len(), 1);
        assert_eq!(
            recorded
                .semantic
                .stance_provenance
                .observations()
                .front()
                .unwrap()
                .polarity,
            qxfx0_types::stance::StancePolarity::Rejected
        );

        let mut mismatch = test_state("explicit-stance");
        let mismatch_decision = qxfx0_types::stance::SystemStanceDecision {
            topic: qxfx0_types::stance::StanceTopic::new("истина").unwrap(),
            polarity: qxfx0_types::stance::StancePolarity::Rejected,
        };
        process_turn_with_renderer_and_explicit_stance_decision(
            &input,
            &mut mismatch,
            RendererAuthority::LegacyShadow,
            mismatch_decision,
        );
        assert!(mismatch.semantic.stance_provenance.is_empty());

        let mut blocked = test_state("explicit-blocked");
        let blocked_decision = qxfx0_types::stance::SystemStanceDecision {
            topic: qxfx0_types::stance::StanceTopic::new("свобода").unwrap(),
            polarity: qxfx0_types::stance::StancePolarity::Rejected,
        };
        let blocked_output = process_turn_with_renderer_and_explicit_stance_decision(
            &TurnInput {
                session_id: "explicit-blocked".into(),
                raw_text: "a".repeat(10_000),
            },
            &mut blocked,
            RendererAuthority::LegacyShadow,
            blocked_decision,
        );
        assert!(blocked_output.blocked);
        assert!(blocked.semantic.stance_provenance.is_empty());
    }

    #[test]
    fn signed_stance_is_default_off_for_output_and_only_records_after_binding() {
        let input = TurnInput {
            session_id: "signed-stance".into(),
            raw_text: "что такое свобода?".into(),
        };
        let mut baseline = test_state("signed-stance");
        let mut signed = test_state("signed-stance");
        let baseline_output =
            process_turn_with_renderer(&input, &mut baseline, RendererAuthority::LegacyShadow);
        let (signed_output, outcome) = process_turn_with_renderer_and_signed_stance_decision(
            &input,
            &mut signed,
            RendererAuthority::LegacyShadow,
            Some(&signed_stance_for(&input, "свобода")),
            &AcceptingSignatureVerifier,
            &signed_stance_policy(),
        );

        assert_eq!(signed_output.response, baseline_output.response);
        assert_eq!(signed_output.family, baseline_output.family);
        assert_eq!(outcome, SignedStanceDecisionOutcome::Recorded);
        assert_eq!(signed.semantic.stance_provenance.len(), 1);
        signed.semantic.stance_provenance = Default::default();
        assert_eq!(
            serde_json::to_value(signed).unwrap(),
            serde_json::to_value(baseline).unwrap()
        );
    }

    #[test]
    fn invalid_signed_stance_leaves_the_normal_turn_and_state_unchanged() {
        let input = TurnInput {
            session_id: "invalid-signed-stance".into(),
            raw_text: "что такое свобода?".into(),
        };
        let mut baseline = test_state("invalid-signed-stance");
        let mut invalid = test_state("invalid-signed-stance");
        let baseline_output =
            process_turn_with_renderer(&input, &mut baseline, RendererAuthority::LegacyShadow);
        let mut signed = signed_stance_for(&input, "свобода");
        signed.attestation.request_digest = [0; 32];
        let (invalid_output, outcome) = process_turn_with_renderer_and_signed_stance_decision(
            &input,
            &mut invalid,
            RendererAuthority::LegacyShadow,
            Some(&signed),
            &AcceptingSignatureVerifier,
            &signed_stance_policy(),
        );

        assert_eq!(invalid_output.response, baseline_output.response);
        assert_eq!(invalid_output.family, baseline_output.family);
        assert!(matches!(
            outcome,
            SignedStanceDecisionOutcome::VerificationRejected { .. }
        ));
        assert_eq!(
            serde_json::to_value(invalid).unwrap(),
            serde_json::to_value(baseline).unwrap()
        );
    }

    #[test]
    fn signed_stance_requires_the_pipeline_normalized_topic_and_is_replay_deterministic() {
        let input = TurnInput {
            session_id: "signed-replay".into(),
            raw_text: "что такое свобода?".into(),
        };
        let signed = signed_stance_for(&input, "истина");
        let mut mismatch = test_state("signed-replay");
        let (_, mismatch_outcome) = process_turn_with_renderer_and_signed_stance_decision(
            &input,
            &mut mismatch,
            RendererAuthority::LegacyShadow,
            Some(&signed),
            &AcceptingSignatureVerifier,
            &signed_stance_policy(),
        );
        assert_eq!(
            mismatch_outcome,
            SignedStanceDecisionOutcome::NormalizedTopicMismatch
        );
        assert!(mismatch.semantic.stance_provenance.is_empty());

        let signed = signed_stance_for(&input, "свобода");
        let mut first = test_state("signed-replay");
        let mut second = test_state("signed-replay");
        let first_result = process_turn_with_renderer_and_signed_stance_decision(
            &input,
            &mut first,
            RendererAuthority::LegacyShadow,
            Some(&signed),
            &AcceptingSignatureVerifier,
            &signed_stance_policy(),
        );
        let second_result = process_turn_with_renderer_and_signed_stance_decision(
            &input,
            &mut second,
            RendererAuthority::LegacyShadow,
            Some(&signed),
            &AcceptingSignatureVerifier,
            &signed_stance_policy(),
        );
        assert_eq!(first_result.0.response, second_result.0.response);
        assert_eq!(first_result.1, second_result.1);
        assert_eq!(
            serde_json::to_value(first).unwrap(),
            serde_json::to_value(second).unwrap()
        );
    }
}

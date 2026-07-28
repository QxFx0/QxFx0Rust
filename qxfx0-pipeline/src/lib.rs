//! QxFx0 Pipeline — synchronous sequential turn processing.
//!
//! 7 stages: Prepare → Route → PlanShadow → Render → Finalize → Guard → Persist.
//! No async, no Tokio, no external middleware — pure synchronous call chain.

#[cfg(test)]
mod conjugate_pipeline;
pub mod conversation_fsm;
#[path = "tracing.rs"]
pub mod execution_trace;
pub mod shadow_plan;
mod stages;
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
use std::collections::BTreeMap;
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

/// Explicit, default-off durable provenance recorder. It never feeds routing,
/// plans, rendering, temporal recovery, or user-visible output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub enum StanceProvenanceMode {
    #[default]
    Disabled,
    RecordAffirmedSystemDecision,
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

/// Process a single turn synchronously through all 7 stages.
///
/// If any stage before the guard fails, the state is rolled back to its
/// pre-turn snapshot and a blocked recovery output is returned. This prevents
/// partial side effects from corrupting the session.
pub fn process_turn(input: &TurnInput, state: &mut SystemState) -> TurnOutput {
    process_turn_with_renderer(input, state, RendererAuthority::LegacyShadow)
}

/// Process a turn with an explicit renderer-authority feature flag.
pub fn process_turn_with_renderer(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
) -> TurnOutput {
    process_turn_internal(
        input,
        state,
        None,
        None,
        renderer_authority,
        DoubtShadowMode::Disabled,
        AnomalyShadowMode::Disabled,
        ClarificationMode::Disabled,
        SameTopicSuppressionMode::Disabled,
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

/// Process a turn while collecting lightweight timing evidence for each
/// pipeline stage. The returned timing is observational and is not persisted
/// in the session state or included in replay signatures.
pub fn process_turn_with_timing_and_renderer(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
) -> (TurnOutput, PipelineStageTimings) {
    let started = Instant::now();
    let mut timings = PipelineStageTimings::default();
    let output = process_turn_internal(
        input,
        state,
        None,
        Some(&mut timings),
        renderer_authority,
        DoubtShadowMode::Disabled,
        AnomalyShadowMode::Disabled,
        ClarificationMode::Disabled,
        SameTopicSuppressionMode::Disabled,
    );
    timings.total_ms = PipelineStageTimings::duration_ms(started.elapsed());
    (output, timings)
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
    let (mut trace, initial_digest, trace_started) = new_pipeline_trace(input, state);
    let output = process_turn_internal(
        input,
        state,
        Some(&mut trace),
        None,
        renderer_authority,
        DoubtShadowMode::Disabled,
        anomaly_shadow,
        ClarificationMode::Disabled,
        SameTopicSuppressionMode::Disabled,
    );
    finish_pipeline_trace(initial_digest, state, &output, trace_started, &mut trace);
    (output, trace)
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
    let (mut trace, initial_digest, trace_started) = new_pipeline_trace(input, state);
    let output = process_turn_internal(
        input,
        state,
        Some(&mut trace),
        None,
        renderer_authority,
        doubt_shadow,
        AnomalyShadowMode::Disabled,
        clarification,
        suppression,
    );
    finish_pipeline_trace(initial_digest, state, &output, trace_started, &mut trace);
    (output, trace)
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
    let started = Instant::now();
    let mut timings = PipelineStageTimings::default();
    let (mut trace, initial_digest, trace_started) = new_pipeline_trace(input, state);
    let output = process_turn_internal(
        input,
        state,
        Some(&mut trace),
        Some(&mut timings),
        renderer_authority,
        DoubtShadowMode::Disabled,
        anomaly_shadow,
        ClarificationMode::Disabled,
        SameTopicSuppressionMode::Disabled,
    );
    timings.total_ms = PipelineStageTimings::duration_ms(started.elapsed());
    finish_pipeline_trace(initial_digest, state, &output, trace_started, &mut trace);
    (output, timings, trace)
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
    let started = Instant::now();
    let mut timings = PipelineStageTimings::default();
    let (mut trace, initial_digest, trace_started) = new_pipeline_trace(input, state);
    let output = process_turn_internal(
        input,
        state,
        Some(&mut trace),
        Some(&mut timings),
        renderer_authority,
        doubt_shadow,
        AnomalyShadowMode::Disabled,
        clarification,
        suppression,
    );
    timings.total_ms = PipelineStageTimings::duration_ms(started.elapsed());
    finish_pipeline_trace(initial_digest, state, &output, trace_started, &mut trace);
    (output, timings, trace)
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

#[allow(clippy::too_many_arguments)] // explicit staged feature flags meet at this private boundary
fn process_turn_internal(
    input: &TurnInput,
    state: &mut SystemState,
    mut trace: Option<&mut execution_trace::PipelineTrace>,
    mut timings: Option<&mut PipelineStageTimings>,
    renderer_authority: RendererAuthority,
    doubt_shadow: DoubtShadowMode,
    anomaly_shadow: AnomalyShadowMode,
    clarification: ClarificationMode,
    suppression: SameTopicSuppressionMode,
) -> TurnOutput {
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
    let mut prop = PropositionParser::parse(&input.raw_text);

    // Normalize subject to nominative form using the runtime graph.
    // Users type topics in oblique cases ("об ответственности" → "ответственности")
    // but the graph stores atoms in nominative ("ответственность").
    if state.semantic.runtime_graph.edges.is_empty() {
        state.semantic.runtime_graph = qxfx0_semantic::seed_graph();
    }
    if matches!(
        prop.mode,
        qxfx0_semantic::PropositionMode::Define | qxfx0_semantic::PropositionMode::Assert
    ) {
        if let Some(known_topic) = qxfx0_semantic::PropositionParser::known_topic_in_input(
            &input.raw_text,
            &state.semantic.runtime_graph,
        ) {
            prop.subject = known_topic;
        }
    }
    prop.subject = qxfx0_semantic::PropositionParser::normalize_topic(
        &prop.subject,
        &state.semantic.runtime_graph,
    );

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

    // Stage 3: Shadow plan (observational; renderer authority is unchanged)
    let planned = match execute_stage(
        &mut trace,
        &mut timings,
        "plan_shadow",
        state,
        routed,
        stages::plan_shadow_stage,
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("plan_shadow_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };

    // Stage 4: Render
    let rendered = match execute_stage(
        &mut trace,
        &mut timings,
        "render",
        state,
        planned,
        |state, planned| stages::render_stage(state, planned, renderer_authority),
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("render_stage failed: {error}");
            *state = snapshot;
            return recovery_output(state, &recovery);
        }
    };
    recovery.path_depth = Some(rendered.path_depth());

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
    if let Some(rejection) = guarded.rejection() {
        // A guard rejection is an expected turn outcome, not a pipeline fault.
        tracing::warn!("guard rejected turn: {rejection}");
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
}

//! Public turn entry points: thin wrappers over `process_turn_internal`.
//!
//! Every `process_turn_*` wrapper delegates here with an explicit
//! `TurnOptions` so behaviour selection stays data, not names.

use crate::turn_types::{
    AnomalyShadowMode, ClarificationMode, DoubtShadowMode, PipelineStageTimings, RendererAuthority,
    ResponsePlanV2Authority, ResponsePlanV2Config, ResponsePlanV2Mode, SameTopicSuppressionMode,
    SignedStanceDecisionOutcome, StanceProvenanceMode, SubjectAuthority, TurnInput, TurnOutput,
};
use crate::{execution_trace, fact_grounded, EssenceAblation};
use crate::{finish_pipeline_trace, new_pipeline_trace, process_turn_internal};
use qxfx0_types::system_state::*;
use serde::Serialize;
use std::time::Instant;
/// Explicit, default-off feature selection for a single turn.
///
/// Every axis defaults to the standard production path: the legacy renderer
/// with all staged integrations disabled (struct default; the deployed CLI
/// selects `AuditedPlan` explicitly). A new staged feature extends this
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
    pub thesis_projection: fact_grounded::ThesisProjectionRollout,
    pub response_plan_v2: ResponsePlanV2Config,
    /// B2 ablation arm of the V2 subject core (ADR-0043 U2). `Enabled` is
    /// the law and the default; `CommitDisabled` is set only by explicit
    /// test/CLI switches for the ablated control group — never persisted,
    /// never a runtime default.
    pub essence_v2_ablation: EssenceAblation,
    /// Subject-core authority (ADR-0044 migration, flipped in M4).
    /// `V2Authority` is the law and the default; `V1Authority` is set
    /// only by explicit switches for pinned comparisons — never
    /// persisted, never a second default.
    pub subject_authority: SubjectAuthority,
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

    pub fn with_thesis_projection(
        mut self,
        thesis_projection: fact_grounded::ThesisProjectionRollout,
    ) -> Self {
        self.thesis_projection = thesis_projection;
        self
    }

    pub fn with_response_plan_v2(mut self, mode: ResponsePlanV2Mode) -> Self {
        self.response_plan_v2.mode = mode;
        self
    }

    pub fn with_response_plan_v2_authority(mut self, authority: ResponsePlanV2Authority) -> Self {
        // No side effects: mode is orthogonal and set explicitly by the
        // caller (`with_response_plan_v2`). A builder that silently sets
        // two fields was the coupling the audit flagged.
        self.response_plan_v2.authority = authority;
        self
    }

    /// Select the B2 ablation arm for the V2 subject core. Test/CLI-only by
    /// contract; production paths never call this.
    pub fn with_essence_v2_ablation(mut self, ablation: EssenceAblation) -> Self {
        self.essence_v2_ablation = ablation;
        self
    }

    pub fn with_subject_authority(mut self, authority: SubjectAuthority) -> Self {
        self.subject_authority = authority;
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
/// Process an ordinary turn, then record an accepted normalized subject as an
/// explicit affirmed system decision when the caller opted in. Recording is
/// after the guard, so rejected turns and failed-stage rollback retain none.
pub fn process_turn_with_renderer_and_stance_provenance(
    input: &TurnInput,
    state: &mut SystemState,
    renderer_authority: RendererAuthority,
    mode: StanceProvenanceMode,
) -> TurnOutput {
    let output = process_turn_with_options(
        input,
        state,
        TurnOptions::new().with_renderer(renderer_authority),
    );
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
    let output = process_turn_with_options(
        input,
        state,
        TurnOptions::new().with_renderer(renderer_authority),
    );
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
    let output = process_turn_with_options(
        input,
        state,
        TurnOptions::new().with_renderer(renderer_authority),
    );
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

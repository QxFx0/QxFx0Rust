//! QxFx0 Pipeline — synchronous sequential turn processing.
//!
//! 7 stages: Prepare → Route → PlanShadow → Render → Finalize → Guard → Persist.
//! No async, no Tokio, no external middleware — pure synchronous call chain.

pub mod b2_report;
#[cfg(test)]
mod conjugate_pipeline;
pub mod conversation_fsm;
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

pub use stages::{MAX_RUNTIME_ATOMS, MAX_RUNTIME_EDGES};

/// Re-exported so downstream crates (CLI, codex journal, serve) select the
/// B2 control arm without taking a direct dependency on `qxfx0-self-v2`.
pub use qxfx0_self_v2::EssenceAblation;

pub use conversation_fsm::{
    fsm_state_discriminant, fsm_state_from_discriminant, initial_state, is_active,
    proposition_to_event, transition as fsm_transition, ConversationEvent, ConversationState,
};

use qxfx0_types::atom::AtomId;
use qxfx0_types::system_state::*;
use qxfx0_types::*;
use serde::Serialize;
use std::collections::BTreeMap;
use std::time::Instant;
use turn_context::{StageTraceContext, TurnInputContext};

mod authority;
mod turn_api;
mod turn_assists;
mod turn_types;

pub use authority::current_binary_digest;
pub use turn_api::*;
pub use turn_types::*;

pub(crate) use authority::record_response_plan_v2;
pub(crate) use turn_assists::{
    clarification_decision, record_anomaly_shadow, record_clarification_route, record_doubt_shadow,
    record_fact_grounded_trace, record_same_topic_suppression,
};
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

/// The graph's challenge to the practitioner's newest earlier-held position
/// on this turn's topic, as a response appendix. `None` when the topic has
/// no prior held position or the graph carries no opposing edge for it.
///
/// Deterministic in `(state, topic)`: the newest position wins ties on
/// `(turn, turn_seq)`, and the opposing edge is selected by the byte-salt
/// of the position plus its turn — the same shared rotation the reflection
/// card applies (`qxfx0_semantic::challenge`), minus the practice day the
/// pipeline deliberately does not know.
fn position_challenge_appendix(state: &SystemState, subject: &str) -> Option<String> {
    let store = state.semantic.semantic_commitments.as_ref()?;
    let current_turn = state.dialogue.turn_count;
    let (payload, turn) = store
        .active
        .values()
        .filter(|(payload, turn)| payload.topic == subject && *turn < current_turn)
        .max_by_key(|(payload, turn)| (*turn, payload.turn_seq))?;
    let salt = payload
        .statement
        .bytes()
        .map(u64::from)
        .sum::<u64>()
        .wrapping_add(u64::try_from(*turn).unwrap_or(0));
    let sentence = qxfx0_semantic::challenge::opposing_challenge_sentence(subject, salt)?;
    Some(format!(
        "\n\n— Я помню твою позицию [ход {turn}]: «{}».\nГраф возражает: {sentence}.\nКак это совместить — или одна из них должна уйти?",
        payload.statement
    ))
}

/// Early rejection of oversized input, mirroring guard-blocked bookkeeping
/// (governance event, turn and history advance, recovery surface) without
/// paying for parse, routing, activation and render of a multi-megabyte
/// turn. The guard stage keeps its own bound check for direct stage callers.
fn oversized_input_blocked_output(state: &mut SystemState) -> TurnOutput {
    let turn = state.dialogue.turn_count + 1;
    state.last_turn_decision = Some(TurnDecision {
        family: CanonicalMoveFamily::CMRepair,
        force: IllocutionaryForce::IFAssert,
        guard_status: GuardStatus::InvariantBlock("слишком длинный ввод".into()),
        legitimacy: 0.0,
    });
    state
        .governance_log
        .append(qxfx0_types::governance::GovernanceEvent {
            turn,
            event_type: qxfx0_types::governance::GovernanceEventType::GuardBlocked,
            family: CanonicalMoveFamily::CMRepair,
            guard_status: GuardStatus::InvariantBlock("слишком длинный ввод".into()),
            timestamp: format!("turn-{turn}"),
        });
    state.governance_log.trim(10_000);

    let response = "QxFx0: ответ отклонён системой безопасности.".to_string();
    state.dialogue.turn_count = turn;
    state.dialogue.last_family = CanonicalMoveFamily::CMRepair;
    state.dialogue.history.push(response.clone());
    if state.dialogue.history.len() > 10_000 {
        let excess = state.dialogue.history.len() - 10_000;
        state.dialogue.history.drain(0..excess);
    }

    TurnOutput {
        response,
        family: CanonicalMoveFamily::CMRepair,
        guard_status: GuardStatus::InvariantBlock("слишком длинный ввод".into()),
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

pub(crate) fn new_pipeline_trace(
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

pub(crate) fn finish_pipeline_trace(
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
pub(crate) fn process_turn_internal(
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
        thesis_projection,
        response_plan_v2,
        response_plan_v2_authority,
        essence_v2_ablation,
        subject_authority,
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
    // Reject oversized input before any parse, routing, activation or render
    // work: the guard stage keeps the same bound for direct stage callers.
    let guard_limits = qxfx0_guard::GuardConfig::default();
    if input.raw_text.chars().count() > guard_limits.max_input_length {
        tracing::warn!(
            "input exceeds the {}-character bound and was rejected before staging",
            guard_limits.max_input_length
        );
        return oversized_input_blocked_output(state);
    }
    let state_violations = state.validate();
    if !state_violations.is_empty() {
        tracing::error!("state invariant violation: {}", state_violations.join("; "));
        return session_invariant_output(state, "loaded state violates invariants");
    }

    let snapshot = state.capture_rollback_snapshot();
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
        timings.morphology_init_ms = qxfx0_morphology::runtime_init_elapsed_ms();
        timings.morphology_blob_warm_ms = qxfx0_morphology::runtime_blob_warm_ms();
        timings.adjective_lexicon_init_ms = qxfx0_morphology::adjective_lexicon_init_ms();
    }

    // Stage 1: Prepare
    let prepared = match execute_stage(
        &mut trace,
        &mut timings,
        "prepare",
        state,
        input_context,
        |state, input| stages::prepare_stage(state, input, subject_authority),
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("prepare_stage failed: {error}");
            snapshot.apply(state);
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
            snapshot.apply(state);
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
            snapshot.apply(state);
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
        |state, planned| stages::render_stage(state, planned, effective_renderer_authority),
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("render_stage failed: {error}");
            snapshot.apply(state);
            return recovery_output(state, &recovery);
        }
    };
    recovery.path_depth = Some(rendered.path_depth());
    let active_packs = qxfx0_semantic::active_pack_set();
    // Thesis catalog observation and V2 renderer authority are independent
    // promotion boundaries. Do not collect even shadow thesis evidence in an
    // authority-enabled V2 turn; a later joint experiment requires its own
    // explicit contract and evidence window.
    let thesis_shadow_allowed = thesis_projection.observes()
        && response_plan_v2_authority == ResponsePlanV2Authority::Disabled;
    let rendered_receipt = if fact_grounded_rollout.observes() || thesis_shadow_allowed {
        match fact_grounded::RenderedPlanReceipt::from_rendered(&rendered, state, active_packs) {
            Ok(receipt) => Ok(receipt),
            Err(error) if fact_grounded_rollout.permits_render_authorization() => {
                tracing::error!("fact-grounded receipt failed: {error}");
                snapshot.apply(state);
                return recovery_output(state, &recovery);
            }
            Err(error) => Err(error),
        }
    } else {
        Ok(None)
    };

    // Stage 5: Finalize
    // The V2 subject-core advance summary flows through a side channel
    // (`essence_v2_advance`), never through the digested stage context — the
    // same replay discipline the thesis observation receipt follows.
    let mut essence_v2_advance: Option<qxfx0_self_v2::EssenceAdvanceTrace> = None;
    let finalized = match execute_stage(
        &mut trace,
        &mut timings,
        "finalize",
        state,
        rendered,
        |state, rendered| {
            stages::finalize_stage(
                state,
                rendered,
                essence_v2_ablation,
                &mut essence_v2_advance,
            )
        },
    ) {
        Ok(context) => context,
        Err(error) => {
            tracing::error!("finalize_stage failed: {error}");
            snapshot.apply(state);
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
            snapshot.apply(state);
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

    if thesis_projection.observes() {
        let outcome = if response_plan_v2_authority == ResponsePlanV2Authority::Canary {
            qxfx0_types::ThesisObservationOutcome::V2AuthorityIsolated
        } else if guarded.blocked() {
            qxfx0_types::ThesisObservationOutcome::GuardBlocked
        } else {
            match &rendered_receipt {
                Ok(Some(_)) => qxfx0_types::ThesisObservationOutcome::Observed,
                Ok(None) => qxfx0_types::ThesisObservationOutcome::NoAuditedPlan,
                Err(_) => qxfx0_types::ThesisObservationOutcome::ValidationRejected,
            }
        };
        if let Some(trace) = trace.as_deref_mut() {
            let receipt = match outcome {
                qxfx0_types::ThesisObservationOutcome::Observed => {
                    fact_grounded::thesis_observation_receipt(
                        outcome,
                        state.dialogue.turn_count,
                        &input.session_id,
                        &input.raw_text,
                        rendered_receipt.as_ref().ok().and_then(Option::as_ref),
                    )
                }
                _ => fact_grounded::thesis_observation_receipt(
                    outcome,
                    state.dialogue.turn_count,
                    &input.session_id,
                    &input.raw_text,
                    None,
                ),
            };
            match receipt.and_then(|receipt| {
                trace
                    .set_thesis_observation_receipt(receipt)
                    .map_err(|error| {
                        fact_grounded::FactGroundedCompositionError::InvalidState(error.to_string())
                    })
            }) {
                Ok(()) => {}
                Err(error) => tracing::warn!("thesis observation receipt skipped: {error}"),
            }
        }
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
                snapshot.apply(state);
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
        state.semantic = snapshot.semantic().clone();
        state.dialogue.conversation_state = snapshot.dialogue().conversation_state;
    }

    // The V2 subject-core shadow advance becomes replay-visible only when the
    // turn's semantic mutations survived: on a blocked turn the snapshot
    // restore above rolls `essence_v2` back, so recording the advance here
    // would testify to a witness that no longer exists.
    if !blocked {
        if let Some(trace) = trace {
            trace.essence_advance = essence_v2_advance;
        }
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
    // The journal's voice: when the topic already carries a held position
    // from an earlier turn, the response itself quotes it back with the
    // graph's opposing edge — the practitioner is answered, not just
    // recorded. Blocked turns keep the bare recovery surface. The
    // selection is the same deterministic one the reflection card uses, so
    // every turn mode (plain, chat, diagnostics) appends byte-identically
    // and replay verification is preserved.
    if !blocked {
        if let Some(appendix) = position_challenge_appendix(state, &subject) {
            response.push_str(&appendix);
        }
    }
    state.dialogue.history.push(response.clone());
    if state.dialogue.history.len() > 10_000 {
        let excess = state.dialogue.history.len() - 10_000;
        state.dialogue.history.drain(0..excess);
    }
    // The journal records every completed turn with placeholder day and
    // digest. The CLI boundary finalizes them (practice day + replay
    // witness) before persisting, so a replay of the journal reconstructs
    // the same records — and their digests — byte-identically.
    state
        .dialogue
        .journal
        .push(qxfx0_types::system_state::JournalRecord {
            turn: state.dialogue.turn_count,
            day: 0,
            topic: Some(subject.clone()),
            input: routed.prepared().input().raw_text().to_owned(),
            response: response.clone(),
            state_digest: String::new(),
            subject_authority: crate::turn_types::subject_authority_label(subject_authority)
                .to_string(),
        });

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
        let output = process_turn_with_options(&input, &mut state, TurnOptions::new());
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
    fn response_plan_v2_shadow_covers_60_topics_and_129_claims() {
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
        assert_eq!(topics, 141);
        assert_eq!(claims, 291);
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
                process_turn_with_options(input, state, TurnOptions::new())
            }),
            (
                "with_renderer",
                TurnOptions::new().with_renderer(RendererAuthority::AuditedPlan),
                |input, state| {
                    process_turn_with_options(
                        input,
                        state,
                        TurnOptions::new().with_renderer(RendererAuthority::AuditedPlan),
                    )
                },
            ),
            (
                "with_renderer_and_fact_grounded",
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_fact_grounded(fact_grounded::FactGroundedRollout::Shadow),
                |input, state| {
                    process_turn_with_options(
                        input,
                        state,
                        TurnOptions::new()
                            .with_renderer(RendererAuthority::AuditedPlan)
                            .with_fact_grounded(fact_grounded::FactGroundedRollout::Shadow),
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
                process_turn_with_options_and_trace(
                    input,
                    state,
                    TurnOptions::new().with_renderer(RendererAuthority::LegacyShadow),
                )
            }),
            (
                "with_trace_and_renderer_and_doubt_shadow",
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_doubt_shadow(DoubtShadowMode::TraceOnly),
                |input, state| {
                    process_turn_with_options_and_trace(
                        input,
                        state,
                        TurnOptions::new()
                            .with_renderer(RendererAuthority::AuditedPlan)
                            .with_doubt_shadow(DoubtShadowMode::TraceOnly),
                    )
                },
            ),
            (
                "with_trace_and_renderer_and_anomaly_shadow",
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_anomaly_shadow(AnomalyShadowMode::TraceOnly),
                |input, state| {
                    process_turn_with_options_and_trace(
                        input,
                        state,
                        TurnOptions::new()
                            .with_renderer(RendererAuthority::AuditedPlan)
                            .with_anomaly_shadow(AnomalyShadowMode::TraceOnly),
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
                    process_turn_with_options_and_trace(
                        input,
                        state,
                        TurnOptions::new()
                            .with_renderer(RendererAuthority::AuditedPlan)
                            .with_doubt_shadow(DoubtShadowMode::TraceOnly)
                            .with_clarification(ClarificationMode::TraceOnly)
                            .with_suppression(SameTopicSuppressionMode::TraceOnly),
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
        let bare = process_turn_with_options(
            &parity_input("default-parity"),
            &mut bare_state,
            TurnOptions::new(),
        );

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

        let standard = process_turn_with_options(&input, &mut standard_state, TurnOptions::new());
        let (timed, timings) = process_turn_with_options_and_timing(
            &input,
            &mut timed_state,
            TurnOptions::new().with_renderer(RendererAuthority::LegacyShadow),
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
        let output = process_turn_with_options(&input, &mut state, TurnOptions::new());
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
            let _output = process_turn_with_options(&input, &mut state, TurnOptions::new());
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
            let out1 = process_turn_with_options(&input, &mut state1, TurnOptions::new());
            let out2 = process_turn_with_options(&input, &mut state2, TurnOptions::new());
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
        let output = process_turn_with_options(&input, &mut state, TurnOptions::new());
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
        let output = process_turn_with_options(&input, &mut state, TurnOptions::new());
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
        let blocked_output = process_turn_with_options(
            &TurnInput {
                session_id: blocked.session_id.clone(),
                raw_text: "a".repeat(10_000),
            },
            &mut blocked,
            TurnOptions::new()
                .with_renderer(RendererAuthority::AuditedPlan)
                .with_fact_grounded(enabled),
        );
        assert!(blocked_output.blocked);
        assert_eq!(blocked.semantic.perspective, blocked_before);
        assert!(blocked.semantic.pack_set_fingerprint.is_empty());

        let mut fallback = test_state("fact-grounded-fallback");
        let (_, fallback_trace) = process_turn_with_options_and_trace(
            &TurnInput {
                session_id: fallback.session_id.clone(),
                raw_text: "что такое совершенно-неизвестный-термин?".into(),
            },
            &mut fallback,
            TurnOptions::new()
                .with_renderer(RendererAuthority::AuditedPlan)
                .with_fact_grounded(enabled),
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
        process_turn_with_options(
            &TurnInput {
                session_id: legacy.session_id.clone(),
                raw_text: "что такое свобода?".into(),
            },
            &mut legacy,
            TurnOptions::new()
                .with_renderer(RendererAuthority::LegacyShadow)
                .with_fact_grounded(enabled),
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
            let (first_output, first_trace) = process_turn_with_options_and_trace(
                &input,
                &mut first_state,
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_fact_grounded(rollout),
            );
            let (replay_output, replay_trace) = process_turn_with_options_and_trace(
                &input,
                &mut replay_state,
                TurnOptions::new()
                    .with_renderer(RendererAuthority::AuditedPlan)
                    .with_fact_grounded(rollout),
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
        let output = process_turn_with_options(
            &input,
            &mut state,
            TurnOptions::new()
                .with_renderer(RendererAuthority::AuditedPlan)
                .with_fact_grounded(fact_grounded::FactGroundedRollout::LimitedNonProduction),
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
        let standard = process_turn_with_options(
            &input,
            &mut disabled,
            TurnOptions::new().with_renderer(RendererAuthority::LegacyShadow),
        );
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
        let baseline_output = process_turn_with_options(
            &input,
            &mut baseline,
            TurnOptions::new().with_renderer(RendererAuthority::LegacyShadow),
        );
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
        let baseline_output = process_turn_with_options(
            &input,
            &mut baseline,
            TurnOptions::new().with_renderer(RendererAuthority::LegacyShadow),
        );
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

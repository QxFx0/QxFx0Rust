//! Pipeline stages — synchronous, sequential processing over typed contexts.

use crate::conversation_fsm::{
    fsm_state_discriminant, fsm_state_from_discriminant, initial_state, proposition_to_event,
    transition as fsm_transition,
};
use crate::turn_context::{
    FinalizedTurnContext, GuardedTurnContext, PersistedTurnContext, PlannedTurnContext,
    PreparedTurnContext, RenderEvidence, RenderedTurnContext, RendererSource, RoutedTurnContext,
    TurnInputContext,
};
use crate::RendererAuthority;
use qxfx0_commitment::{CommitResult, CommitmentOps};
use qxfx0_guard::ContentQualityGate;
use qxfx0_render::{content_plan::render_audited_plan, RenderEngine};
use qxfx0_self::{
    collapse_essence, commit_essence,
    deliberation::{self, DeliberationModulation, Plan},
    should_commit_essence, witness_essence, Conatus, EssenceMode, EssenceModulation, Salience,
    SelfBlanket,
};
use qxfx0_self_v2::{
    advance_essence, check_blanket_transition, check_initial_blanket, compute_conatus_energy,
    compute_salience, empty_essence, BlanketRecord, EssenceAblation, EssenceAdvanceTrace,
    EssenceTurnInput, SalienceWeightsV2, SelfBlanketSnapshot,
};
use qxfx0_semantic::{
    cached_semantic_network, derive_atoms, network::activate as network_activate,
    normalize_punctuation, seed_graph, ContentSelector, DiscourseComposer, DiscourseStyle,
    FallbackReason, PlanOutcome, PlanSubject, PropositionMode, PropositionParser, QualityGatePhase,
    RecoveryEvidence, RecoveryTrace, SenseDecomposer, Verbosity,
};
use qxfx0_types::atom::AtomId;
use qxfx0_types::field::FieldProfile;
use qxfx0_types::system_state::*;
use qxfx0_types::*;
use serde::Serialize;
use std::fmt;

/// Hard bounds for persistent per-session graph growth. Seed data is far
/// below these limits; they protect long-running sessions with novel inputs.
pub const MAX_RUNTIME_ATOMS: usize = 10_000;
pub const MAX_RUNTIME_EDGES: usize = 20_000;

/// Top-down novelty of a turn's text against its topic's admitted
/// surfaces (density doctrine, first reviewable signal). Registry
/// failure reads as 0.0 — no signal, never a spike on broken
/// infrastructure; an unadmitted topic reads through to
/// `content_novelty` (unknown territory is salient).
fn topic_novelty(raw_text: &str, subject: &str) -> f64 {
    let Ok(registry) = qxfx0_semantic::argued_topic_registry() else {
        return 0.0;
    };
    let surfaces: Vec<&str> = registry
        .get(subject)
        .map(|topic| {
            let mut surfaces = vec![topic.thesis().surface(), topic.counterpoint().surface()];
            if let Some(consequence) = topic.consequence() {
                surfaces.push(consequence.surface());
            }
            surfaces
        })
        .unwrap_or_default();
    qxfx0_semantic::content_saliency::content_novelty(raw_text, &surfaces)
}

/// Stage 1: Prepare — Self Layer: Conatus, Salience, Deliberation.
///
/// ADR-0044 migration M1: `authority` selects the Conatus/Salience
/// source. `V1Authority` computes exactly as before; `V2Authority`
/// reads the canonical energy scalar (blanket snapshot) and bias —
/// same `f64` plumbing downstream (thresholds, divergences,
/// deliberation input), different source. Deliberation, witness and
/// commitment stay V1 under both authorities.
pub fn prepare_stage(
    state: &mut SystemState,
    input: TurnInputContext,
    authority: crate::SubjectAuthority,
) -> Result<PreparedTurnContext, String> {
    let field = state.semantic.field.clone();
    // ADR-0044 migration M1+M2: the Conatus/Salience/Deliberation
    // source follows one authority. V1 computes exactly as before;
    // V2 reads the canonical energy scalar, bias and ladder —
    // same `f64` plumbing downstream (thresholds, divergences,
    // families, confidences), different source. Witness and
    // commitment stay V1 under both authorities.
    let (conatus_energy, salience, deliberation) = match authority {
        crate::SubjectAuthority::V1Authority => {
            let energy = Conatus::compute(&field);
            let bias = Salience::compute(&field);
            let holistic_dominant = bias > 0.5;
            let modln = DeliberationModulation::default();
            let holistic_plan = Plan {
                family: if holistic_dominant {
                    CanonicalMoveFamily::CMReflect
                } else {
                    CanonicalMoveFamily::CMGround
                },
                holistic_dominant: true,
                recovery_cause: None,
                confidence: (field.resonance * 0.6 + field.counterfactual * 0.4).clamp(0.0, 1.0),
            };
            let formal_plan = Plan {
                family: CanonicalMoveFamily::CMDefine,
                holistic_dominant: false,
                recovery_cause: None,
                confidence: (field.confidence * 0.7 + field.consolidation * 0.3).clamp(0.0, 1.0),
            };
            let reconciled = deliberation::reconcile(
                &modln,
                &holistic_plan,
                &formal_plan,
                &field,
                energy,
                bias,
                holistic_dominant,
            );
            (energy, bias, reconciled)
        }
        crate::SubjectAuthority::V2Authority => {
            let energy = compute_conatus_energy(
                SelfBlanketSnapshot {
                    morphology_total_size: qxfx0_morphology::get_runtime().stats().total_lexemes
                        as u64,
                    identity_claims_count: state
                        .semantic
                        .semantic_commitments
                        .as_ref()
                        .map(|store| store.active.len() as u64)
                        .unwrap_or(0),
                    turn_count: (state.dialogue.turn_count + 1) as u64,
                },
                &[],
            );
            let verdict = compute_salience(
                SalienceWeightsV2::default(),
                energy,
                &field,
                // Density doctrine: the top-down novelty signal
                // replaces the reserved 0.0 on the live path.
                topic_novelty(input.raw_text(), input.subject()),
            );
            let holistic_dominant = verdict.holistic_bias > 0.5;
            let (holistic, formal) = qxfx0_self_v2::proposal_pair_from_field(&field);
            let result = qxfx0_self_v2::reconcile(
                &qxfx0_self_v2::DeliberationModulationV2::default(),
                &verdict,
                &holistic,
                &formal,
            );
            let mapped = qxfx0_self_v2::v2_result_to_v1_deliberation(
                &result,
                verdict.driver,
                holistic_dominant,
            );
            (energy.scalar, verdict.holistic_bias, mapped)
        }
    };
    let holistic_dominant = salience > 0.5;
    let holistic_prop = field.resonance * 0.6 + field.counterfactual * 0.4;
    let formal_prop = field.confidence * 0.7 + field.consolidation * 0.3;

    let violations = SelfBlanket::check(&field, conatus_energy);
    if !violations.is_empty() {
        tracing::warn!("Self-blanket violations: {:?}", violations);
    }

    let essence_strength = {
        let view = crate::essence_view::essence_view(state, authority);
        if view.committed {
            view.witness_count as f64 / 10.0
        } else {
            0.0
        }
    };

    state.semantic.adjunction = AdjunctionState {
        holistic_value: holistic_prop,
        formal_value: formal_prop,
        reconciled_value: deliberation.plan.confidence,
        holistic_dominant,
    };
    state.last_turn_decision = Some(TurnDecision {
        family: deliberation.plan.family,
        force: IllocutionaryForce::IFAssert,
        guard_status: GuardStatus::Allowed,
        legitimacy: deliberation.plan.confidence,
    });

    // W3: Populate has_enough — true when the subject exists in the runtime graph,
    // meaning we have enough semantic context to reason about it.
    let has_enough = state
        .semantic
        .runtime_graph
        .atoms
        .contains_key(&AtomId::new(input.subject()));

    // ADR-0043 U3 shadow: the doubt-loop suppression fact, read from the
    // *previous* turn's state (last_turn_decision still carries it here —
    // line above overwrites it). Same semantics as V1's
    // immediate_confirmed_same_topic recall.
    let same_topic_decision_confirmed = {
        let confirmed = state.last_turn_decision.as_ref().is_some_and(|decision| {
            matches!(
                decision.guard_status,
                GuardStatus::Allowed | GuardStatus::InvariantWarn(_)
            )
        });
        confirmed
            && state
                .dialogue
                .last_topic
                .as_deref()
                .is_some_and(|previous| previous == input.subject())
    };

    Ok(PreparedTurnContext::new(
        input,
        conatus_energy,
        salience,
        holistic_dominant,
        essence_strength,
        deliberation.plan.family,
        deliberation.trace.rule,
        deliberation.trace,
        has_enough,
        same_topic_decision_confirmed,
    ))
}

/// Stage 2: Route — FSM-driven move family selection (persisted across turns).
pub fn route_stage(
    state: &mut SystemState,
    prepared: PreparedTurnContext,
    apply_clarification: bool,
) -> Result<RoutedTurnContext, String> {
    let mode = prepared.input().routed_mode();
    let event = proposition_to_event(mode, prepared.has_enough());

    // Restore FSM state from discriminant (or use initial).
    let current = match state
        .dialogue
        .conversation_state
        .and_then(fsm_state_from_discriminant)
    {
        Some(s) => s,
        None => {
            if state.dialogue.conversation_state.is_some() {
                tracing::warn!(
                    "Unknown conversation state discriminant {:?}, resetting to Idle",
                    state.dialogue.conversation_state
                );
            }
            initial_state()
        }
    };

    let next = if apply_clarification {
        crate::conversation_fsm::ConversationState::Clarifying
    } else {
        fsm_transition(current, event)
    };

    // Persist as discriminant (no JSON round-trip, no heap allocation).
    state.dialogue.conversation_state = Some(fsm_state_discriminant(&next));

    // Route-driven family selection: FSM mode determines the move family,
    // overriding the deliberation's family (which is the prepare-stage proposal).
    // This ensures distinct propositions map to distinct families.
    let family = if apply_clarification {
        CanonicalMoveFamily::CMClarify
    } else {
        family_for_mode(mode)
    };

    Ok(RoutedTurnContext::new(prepared, family, next))
}

fn family_for_mode(mode: PropositionMode) -> CanonicalMoveFamily {
    match mode {
        PropositionMode::Challenge => CanonicalMoveFamily::CMRepair,
        PropositionMode::Define => CanonicalMoveFamily::CMDefine,
        PropositionMode::Connect => CanonicalMoveFamily::CMConnect,
        PropositionMode::Assert => CanonicalMoveFamily::CMGround,
        PropositionMode::Reflect => CanonicalMoveFamily::CMReflect,
        PropositionMode::Greeting => CanonicalMoveFamily::CMContact,
        PropositionMode::Purpose => CanonicalMoveFamily::CMPurpose,
        PropositionMode::WorldCause => CanonicalMoveFamily::CMHypothesis,
    }
}

/// Stage 3: build an observational plan without changing renderer authority.
pub fn plan_shadow_stage(
    _state: &mut SystemState,
    routed: RoutedTurnContext,
) -> Result<PlannedTurnContext, String> {
    let shadow_plan = crate::shadow_plan::build_shadow_plan(&routed)?;
    Ok(PlannedTurnContext::new(routed, shadow_plan))
}

/// Stage 4: Render — compose response from graph (2-level cascade: Conjugate → ContentSelector).
pub fn render_stage(
    state: &mut SystemState,
    planned: PlannedTurnContext,
    renderer_authority: RendererAuthority,
    subject_authority: crate::SubjectAuthority,
) -> Result<RenderedTurnContext, String> {
    let routed = planned.routed();
    let raw = routed.prepared().input().raw_text().to_owned();
    let subject = routed.prepared().input().subject().to_owned();
    let mode = routed.prepared().input().mode();
    let is_challenge = routed.prepared().input().is_challenge();

    let conatus_energy = routed.prepared().conatus_energy();
    let salience = routed.prepared().salience();
    let essence_strength = routed.prepared().essence_strength();

    let fp = FieldProfile::from_self(
        &state.semantic.field,
        conatus_energy,
        salience,
        essence_strength,
    );
    let path_depth = fp.path_depth();

    if renderer_authority == RendererAuthority::V2Canary {
        let response = planned
            .authority_decision()
            .filter(|receipt| receipt.can_emit_v2())
            .and_then(crate::AuthorityDecisionReceipt::output)
            .ok_or_else(|| "V2 canary authority has no authorized surface".to_string())?;
        return Ok(RenderedTurnContext::new(
            planned,
            response,
            path_depth,
            false,
            RenderEvidence {
                renderer_authority,
                renderer_source: RendererSource::ResponsePlanV2,
                plan_surface_available: true,
                plan_surface_matches_output: Some(true),
                plan_render_error: None,
                boundary_marker: false,
            },
        ));
    }

    if routed.family() == CanonicalMoveFamily::CMClarify {
        let focus = routed.prepared().input().input_frame().focus.clone();
        let response = clarification_surface(&subject, focus.as_deref());
        return Ok(RenderedTurnContext::new(
            planned,
            response,
            path_depth,
            false,
            RenderEvidence {
                renderer_authority,
                renderer_source: RendererSource::Clarification,
                plan_surface_available: false,
                plan_surface_matches_output: None,
                plan_render_error: None,
                boundary_marker: false,
            },
        ));
    }

    // Build the plan surface in both modes. In shadow mode it remains trace
    // evidence only; in audited mode it gains authority only for an admitted
    // topic-backed Ready plan. This function never reads the raw graph.
    let (plan_surface, plan_render_error) = match audited_plan_surface(&planned) {
        Ok(surface) => (surface, None),
        Err(error) => (None, Some(error)),
    };
    if renderer_authority == RendererAuthority::AuditedPlan {
        if let Some(surface) = &plan_surface {
            return Ok(RenderedTurnContext::new(
                planned,
                normalize_punctuation(surface),
                path_depth,
                false,
                RenderEvidence {
                    renderer_authority,
                    renderer_source: RendererSource::AuditedPlan,
                    plan_surface_available: true,
                    plan_surface_matches_output: Some(true),
                    plan_render_error: None,
                    boundary_marker: false,
                },
            ));
        }
    }

    // Legacy rendering still depends on the runtime graph. The plan-authority
    // return above deliberately happens before this access.
    if state.semantic.runtime_graph.edges.is_empty() {
        state.semantic.runtime_graph = seed_graph();
    }

    // Specialized intents must reach their typed frames directly. The
    // generic discourse composer intentionally emits an introduction even
    // with no predicates, so these frames cannot be implemented as a late
    // fallback.
    if matches!(
        mode,
        PropositionMode::Greeting | PropositionMode::Purpose | PropositionMode::WorldCause
    ) {
        let mut prop = PropositionParser::parse(&raw);
        prop.subject = subject.clone();
        let frame = RenderEngine::frame_from_proposition(&prop);
        let response = RenderEngine::render_frame(&frame, &mut state.semantic, &fp, "");
        return Ok(RenderedTurnContext::new(
            planned,
            normalize_punctuation(&response),
            path_depth,
            false,
            RenderEvidence {
                renderer_authority,
                renderer_source: legacy_renderer_source(
                    renderer_authority,
                    plan_render_error.is_some(),
                ),
                plan_surface_available: plan_surface.is_some(),
                plan_surface_matches_output: plan_surface
                    .as_deref()
                    .map(|surface| surface == normalize_punctuation(&response)),
                plan_render_error,
                boundary_marker: false,
            },
        ));
    }

    let sn = cached_semantic_network(&mut state.semantic);
    let graph = &state.semantic.runtime_graph;

    let sense_vectors = SenseDecomposer::decompose(&raw, graph);

    // Build style from the live self layer (ADR-0044 M3).
    let holistic_dominant = routed.prepared().holistic_dominant();
    let view = crate::essence_view::essence_view(state, subject_authority);
    let angst: f64 = view.angst;
    let essence_committed = view.committed;
    let style = style_from_state(
        conatus_energy,
        angst,
        holistic_dominant,
        essence_committed,
        fp.narrative_tone(),
    );

    // Primary: DiscourseComposer (template-based, field-modulated). The
    // semantic network is a derived in-memory cache; ContentSelector remains
    // cheap and is rebuilt against the current graph.
    let cs = ContentSelector::build(graph);
    // Multi-turn coherence: if the current topic differs from last_topic,
    // also activate the previous topic to bridge context. This produces
    // cross-topic predicates that connect the current and prior subjects.
    let activated = network_activate(&AtomId::new(subject.clone()), &sn);
    let mut selected = cs.compose_from_activation(&fp, &subject, &activated);

    // Topic continuity: if we have a prior topic and it's different,
    // look for bridging predicates that connect last_topic → current topic.
    let mut has_bridge = false;
    if let Some(ref last_topic) = state.dialogue.last_topic {
        if last_topic != &subject {
            let bridge = qxfx0_semantic::GraphEngagement::bfs_path(
                graph,
                &AtomId::new(last_topic.clone()),
                &AtomId::new(subject.clone()),
            );
            if !bridge.is_empty() {
                has_bridge = true;
                // Boost consolidation when topics are bridged — the system
                // is building a coherent narrative thread.
                state.semantic.field.consolidation =
                    (state.semantic.field.consolidation + 0.05).min(1.0);
            }
        }
    }

    // Fallback: direct predicate selection if activation found nothing.
    if selected.is_empty() {
        selected = cs.select_predicates(&fp, &subject, Some(&activated));
    }

    let composer = DiscourseComposer::new();
    let turn_seed = state.dialogue.turn_count as u64;
    let history: &[String] = &state.dialogue.history;
    let mut response = composer.compose(&selected, &subject, &style, turn_seed, history);

    // Fallback: ConjugateComposer (if DiscourseComposer produced nothing)
    if response.is_empty() {
        let conjugate_surface = if is_challenge {
            qxfx0_semantic::ConjugateComposer::compose_with_challenge(graph, &sense_vectors, true)
        } else {
            qxfx0_semantic::ConjugateComposer::compose(graph, &sense_vectors)
        };
        response = conjugate_surface.text;
    }

    // Fallback: RenderEngine (frame-based rendering if both composers failed)
    if response.is_empty() {
        let prop = PropositionParser::parse(&raw);
        let frame = RenderEngine::frame_from_proposition(&prop);
        response = RenderEngine::render_frame(&frame, &mut state.semantic, &fp, "");
    }
    if response.is_empty() {
        response =
            "Я не знаю этот смысл, но он вызывает определенный резонанс в моей системе.".into();
    }
    // Density doctrine: past the arousal gate the legacy-composed
    // response collapses to its densest sentence (Haskell
    // `decompressForReceiver` analog; density measured, not positional,
    // because legacy output leads with an intro). The audited path
    // returned above and never concentrates.
    response =
        qxfx0_semantic::concentrate(&response, state.semantic.field.atmosphere.arousal, &subject);

    // Corpus-boundary honesty (141 recognized / 141 admitted): while the
    // boundary was open, legacy-graph responses for recognized-but-
    // unadmitted topics carried a deterministic boundary sentence. Coverage
    // is now complete, so this arm is unreachable defensive code: it fires
    // only if a topic ever leaves the admitted set again, keeping the
    // fallback honest instead of silent. Trace key is emitted only when
    // set, so admitted-topic digests are unchanged.
    let boundary_marker = !subject.trim().is_empty()
        && matches!(
            planned.shadow_plan(),
            PlanOutcome::Fallback(plan) if plan.reason() == FallbackReason::NoAdmissiblePredicate
        );
    if boundary_marker {
        response.push(' ');
        response.push_str(&format!(
            "Граница корпуса: тема «{}» распознана, но аудированного тезиса по ней пока нет — это размышление по связям графа, а не проверенное утверждение.",
            subject.trim()
        ));
    }

    Ok(RenderedTurnContext::new(
        planned,
        normalize_punctuation(&response),
        path_depth,
        has_bridge,
        RenderEvidence {
            renderer_authority,
            renderer_source: legacy_renderer_source(
                renderer_authority,
                plan_render_error.is_some(),
            ),
            plan_surface_available: plan_surface.is_some(),
            plan_surface_matches_output: plan_surface
                .as_deref()
                .map(|surface| surface == normalize_punctuation(&response)),
            plan_render_error,
            boundary_marker,
        },
    ))
}

fn clarification_surface(subject: &str, focus: Option<&str>) -> String {
    // Density doctrine: when the frame isolated an emphasis distinct
    // from the routed subject, the clarification names it — asking
    // about the emphasis, not re-asking the topic. Otherwise the
    // historical wording stands byte-identically.
    let about = match focus {
        Some(focus) if focus != subject => focus,
        _ => subject,
    };
    format!("Мне нужно уточнение: что именно вы хотите прояснить о «{about}»?")
}

fn audited_plan_surface(planned: &PlannedTurnContext) -> Result<Option<String>, String> {
    match planned.shadow_plan() {
        PlanOutcome::Ready(plan) if matches!(plan.subject(), PlanSubject::Topic(_)) => {
            render_audited_plan(plan).map(Some)
        }
        PlanOutcome::Ready(_) | PlanOutcome::Fallback(_) => Ok(None),
    }
}

fn legacy_renderer_source(
    renderer_authority: RendererAuthority,
    plan_render_failed: bool,
) -> RendererSource {
    if renderer_authority == RendererAuthority::AuditedPlan && plan_render_failed {
        RendererSource::LegacyFallback
    } else {
        RendererSource::LegacyGraph
    }
}

fn style_from_state(
    conatus: f64,
    angst: f64,
    _holistic: bool,
    committed: bool,
    tone: qxfx0_types::NarrativeTone,
) -> DiscourseStyle {
    let (verbosity, register) = match tone {
        qxfx0_types::NarrativeTone::Warm => (
            Verbosity::Elaborate,
            if committed {
                "philosophical"
            } else {
                "conversational"
            },
        ),
        qxfx0_types::NarrativeTone::Terse => (Verbosity::Brief, "philosophical"),
        qxfx0_types::NarrativeTone::Recovery => (Verbosity::Medium, "conversational"),
        qxfx0_types::NarrativeTone::Neutral => {
            let v = if conatus > 0.8 {
                3
            } else if conatus > 0.4 {
                2
            } else {
                1
            };
            let verb = match v {
                3 => Verbosity::Elaborate,
                2 => Verbosity::Medium,
                _ => Verbosity::Brief,
            };
            (
                verb,
                if committed {
                    "philosophical"
                } else {
                    "conversational"
                },
            )
        }
    };
    DiscourseStyle {
        register: register.into(),
        complexity: if conatus > 0.8 {
            3
        } else if conatus > 0.4 {
            2
        } else {
            1
        },
        hedging: angst.clamp(0.0, 1.0),
        verbosity,
        use_transitions: conatus > 0.6,
    }
}

/// Stage 5: Finalize — witness + commitment + graph growth + derive_atoms.
///
/// `essence_v2_ablation` selects the B2 control arm for the V2 subject core
/// (`Enabled` is the law; `CommitDisabled` exists for the ablated control
/// group only). `essence_v2_trace` receives the observational advance
/// summary for the pipeline trace; it is deliberately outside the stage's
/// typed context so replay digests never cover it. `subject_authority`
/// (ADR-0044 M3) selects which essence layer is live: under V2 the V1
/// witness/commit block is skipped (the V2 advance below is the
/// authority, not a shadow) and collapse/bump apply to the V2
/// trajectory; the collapse journal (`semantic.essence.reset_events`)
/// stays authority-agnostic.
pub fn finalize_stage(
    state: &mut SystemState,
    rendered: RenderedTurnContext,
    essence_v2_ablation: EssenceAblation,
    essence_v2_trace: &mut Option<EssenceAdvanceTrace>,
    subject_authority: crate::SubjectAuthority,
) -> Result<FinalizedTurnContext, String> {
    let edge_count_before = state.semantic.runtime_graph.edges.len();
    let response = rendered.response().to_owned();
    let subject = rendered.routed().prepared().input().subject().to_owned();
    let mode = rendered.routed().prepared().input().mode();

    let essence_mode = match mode {
        PropositionMode::Challenge | PropositionMode::Assert => EssenceMode::Defend,
        PropositionMode::Define
        | PropositionMode::Reflect
        | PropositionMode::Purpose
        | PropositionMode::WorldCause => EssenceMode::Define,
        PropositionMode::Connect => EssenceMode::Revise,
        PropositionMode::Greeting => EssenceMode::Commit,
    };

    let turn = state.dialogue.turn_count + 1;
    let conatus_energy = rendered.routed().prepared().conatus_energy();
    let holistic_dominant = rendered.routed().prepared().holistic_dominant();
    let salience = rendered.routed().prepared().salience();

    // Preserve the existing witness surface while carrying the rule as an enum.
    let driver = format!("{:?}", rendered.routed().prepared().deliberation_rule());
    let reconcile_rule = &driver;
    let agreement = "PartialAgreement";
    let divergence = if holistic_dominant {
        salience.abs()
    } else {
        1.0 - salience
    };

    let em = EssenceModulation::default();
    // ADR-0044 M3: the V1 witness/commitment write path runs only while
    // V1 is the authority. Under V2 the advance below testifies instead.
    if matches!(subject_authority, crate::SubjectAuthority::V1Authority) {
        let witness_input = qxfx0_self::WitnessInput {
            mode: essence_mode,
            statement: response.clone(),
            salience_driver: driver.as_str(),
            reconcile_rule,
            agreement,
            divergence,
        };
        witness_essence(
            &em,
            turn,
            conatus_energy,
            &mut state.semantic.essence,
            &witness_input,
        );

        if let Some(trigger) = should_commit_essence(&em, &state.semantic.essence) {
            if state.semantic.essence.commitment.is_none() {
                let commitment = commit_essence(turn, trigger, &state.semantic.essence);
                state.semantic.essence.commitment = Some(commitment);
            }
        }
    }

    // ADR-0043 U2: advance the V2 subject-core essence in shadow. The blanket
    // snapshot is read from real state — morphology runtime size, held
    // identity claims as of this turn's start, this turn's ordinal — and the
    // trajectory persists through `semantic.essence_v2` so fresh processes
    // (one per `qxfx0 turn`) accumulate witnesses across the session.
    // Observational: nothing here feeds routing, rendering, guard or the V1
    // self layer; the plan-family guard stays trace-only until a separate
    // release flips it. ADR-0043 U3: the V2 blanket transition is checked
    // across the turn — the previous record persists through
    // `semantic.blanket_v2` so a fresh per-turn process still witnesses
    // session stability, morphology presence and turn / identity-claim
    // monotonicity. Violations are fail-closed DATA: they feed the conatus
    // penalty (−λ·|v|) and ride the trace as named ruptures, never a panic.
    let essence_v2_blanket = BlanketRecord {
        session_id: state.session_id.clone(),
        morphology_total_size: qxfx0_morphology::get_runtime().stats().total_lexemes as u64,
        identity_claims_count: state
            .semantic
            .semantic_commitments
            .as_ref()
            .map(|store| store.active.len() as u64)
            .unwrap_or(0),
        turn_count: turn as u64,
    };
    let previous_blanket =
        match state.semantic.blanket_v2.take() {
            None => None,
            Some(value) => Some(serde_json::from_value::<BlanketRecord>(value).map_err(
                |error| format!("blanket_v2 shadow state failed to decode (fail-closed): {error}"),
            )?),
        };
    let structural_violations = match &previous_blanket {
        Some(previous) => check_blanket_transition(previous, &essence_v2_blanket),
        None => check_initial_blanket(&essence_v2_blanket),
    };
    let conatus_violations: Vec<qxfx0_self_v2::BlanketViolation> =
        structural_violations.into_iter().map(Into::into).collect();
    if !conatus_violations.is_empty() {
        tracing::warn!(
            "V2 self-blanket ruptures: {:?}",
            conatus_violations
                .iter()
                .map(|violation| violation.code.clone())
                .collect::<Vec<_>>()
        );
    }
    let mut essence_v2 = match state.semantic.essence_v2.take() {
        None => empty_essence(),
        Some(value) => serde_json::from_value(value).map_err(|error| {
            format!("essence_v2 shadow state failed to decode (fail-closed): {error}")
        })?,
    };
    let mut essence_v2_summary = advance_essence(
        &qxfx0_self_v2::EssenceModulation::default(),
        essence_v2_ablation,
        EssenceTurnInput {
            turn_ordinal: turn,
            conatus: compute_conatus_energy(
                SelfBlanketSnapshot {
                    morphology_total_size: essence_v2_blanket.morphology_total_size,
                    identity_claims_count: essence_v2_blanket.identity_claims_count,
                    turn_count: essence_v2_blanket.turn_count,
                },
                &conatus_violations,
            ),
            field: &state.semantic.field,
            trace: rendered.routed().prepared().deliberation_trace(),
            proposed_family: rendered.routed().family(),
            // ADR-0044 tuning: scope the violation counter to the turn's
            // topic. The borrow lives in this `let`: the input borrows it.
            topic: Some(subject.as_str()),
        },
        &mut essence_v2,
    );
    essence_v2_summary.blanket_violations = conatus_violations;
    // ADR-0043 U3 shadow: reconcile the canonical deliberation ladder over
    // the salience verdict the advance just computed, and record the
    // applied-vs-reconciled comparison. Observational only — route/family
    // is untouched (V1 authority); this is the flip-readiness evidence.
    if let Some(verdict) = &essence_v2_summary.self_verdict {
        essence_v2_summary.deliberation_shadow = Some(qxfx0_self_v2::deliberate_shadow(
            &verdict.salience,
            &state.semantic.field,
            rendered.routed().family(),
            rendered.routed().prepared().same_topic_decision_confirmed(),
        ));
    }
    state.semantic.essence_v2 = Some(
        serde_json::to_value(&essence_v2)
            .map_err(|error| format!("essence_v2 shadow state failed to encode: {error}"))?,
    );
    state.semantic.blanket_v2 = Some(
        serde_json::to_value(&essence_v2_blanket)
            .map_err(|error| format!("blanket_v2 shadow state failed to encode: {error}"))?,
    );
    *essence_v2_trace = Some(essence_v2_summary);

    // Derive atoms + enrich graph
    let subject_id = AtomId::new(subject.clone());
    let topic_in_graph = state.semantic.runtime_graph.atoms.contains_key(&subject_id);
    let world_id = AtomId::new("мир");
    let reserved_atoms = usize::from(!topic_in_graph)
        + usize::from(!state.semantic.runtime_graph.atoms.contains_key(&world_id));
    let can_register_topic = topic_in_graph
        || (subject.chars().count() > 2
            && state.semantic.runtime_graph.atoms.len() + reserved_atoms <= MAX_RUNTIME_ATOMS
            && state.semantic.runtime_graph.edges.len() < MAX_RUNTIME_EDGES);
    let tags = qxfx0_semantic::inference::classify_state_tags(
        topic_in_graph,
        state.semantic.field.confidence,
        state.semantic.field.counterfactual,
        state.semantic.field.resonance,
        conatus_energy,
        crate::essence_view::essence_view(state, subject_authority).angst,
    );
    let derived = derive_atoms(&tags);

    // Register the topic and world atoms before deriving edges: derived
    // edges reference the subject atom, and the edge bound below must never
    // leave an orphan edge pointing at an atom that was never admitted.
    if subject.chars().count() > 2 && !topic_in_graph && can_register_topic {
        let atom = qxfx0_types::atom::Atom {
            id: subject_id.clone(),
            display: subject.clone(),
            category: qxfx0_types::atom::AtomCategory::CatTopic,
        };
        state
            .semantic
            .runtime_graph
            .atoms
            .insert(subject_id.clone(), atom);
        // Register the "мир" atom if not already present.
        state
            .semantic
            .runtime_graph
            .atoms
            .entry(world_id.clone())
            .or_insert(qxfx0_types::atom::Atom {
                id: world_id.clone(),
                display: "мир".into(),
                category: qxfx0_types::atom::AtomCategory::CatTopic,
            });
    }

    for da in &derived {
        let id = da.id.clone();
        let derived_atom_limit = MAX_RUNTIME_ATOMS.saturating_sub(reserved_atoms);
        if can_register_topic
            && !state.semantic.runtime_graph.atoms.contains_key(&id)
            && state.semantic.runtime_graph.atoms.len() < derived_atom_limit
            && state.semantic.runtime_graph.edges.len() < MAX_RUNTIME_EDGES
        {
            state.semantic.runtime_graph.atoms.insert(
                id.clone(),
                qxfx0_types::atom::Atom {
                    id: id.clone(),
                    display: format!("{:?}", da.tag),
                    category: qxfx0_types::atom::AtomCategory::CatConcept,
                },
            );
            let rel = qxfx0_types::atom::Relation {
                from: id.clone(),
                to: subject_id.clone(),
                rel_type: RelationType::RelRelatedTo,
                object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
                object_text: subject.clone(),
                verb_override: None,
                ru_original: format!("производный атом ← {}", subject),
                en_original: format!("derived atom ← {}", subject),
                source: qxfx0_types::atom::RelationSource::SeedFromPredicate,
                topic: subject.clone(),
                rationale: Some(format!("derived via {:?}", da.rule)),
                counter: None,
                synthesis: None,
            };
            state.semantic.runtime_graph.add_relation(rel);
        }
    }

    // Anomaly-3 collapse (ADR-0044 M3): the trigger reads the live
    // layer; the collapse applies to it. The collapse journal stays
    // authority-agnostic (identical event shape on both layers).
    let self_ref_topics = ["я", "ты", "qxfx0", "система"];
    let live_angst = crate::essence_view::essence_view(state, subject_authority).angst;
    if live_angst > 0.9 && self_ref_topics.contains(&subject.to_lowercase().as_str()) {
        match subject_authority {
            crate::SubjectAuthority::V1Authority => {
                collapse_essence(turn, &mut state.semantic.essence);
            }
            crate::SubjectAuthority::V2Authority => {
                let essence = crate::essence_view::decode_essence_v2(state)?;
                let (collapsed, event) = qxfx0_self_v2::collapse_essence_at(turn, essence);
                state.semantic.essence_v2 =
                    Some(serde_json::to_value(&collapsed).map_err(|error| {
                        format!("essence_v2 collapse failed to encode: {error}")
                    })?);
                state.semantic.essence.reset_events.push(
                    qxfx0_types::system_state::EssenceResetEvent {
                        turn: event.turn,
                        previous_angst: event.previous_angst,
                        previous_witness_count: event.previous_witness_count,
                    },
                );
            }
        }
    }

    // Graph growth for new topics. The edge bound is re-checked here: the
    // derived-atom loop above may have consumed the last edge slot after
    // `can_register_topic` was evaluated, and one more insert would push the
    // graph past MAX_RUNTIME_EDGES, permanently failing state validation.
    if subject.chars().count() > 2
        && !topic_in_graph
        && can_register_topic
        && state.semantic.runtime_graph.edges.len() < MAX_RUNTIME_EDGES
    {
        let rel = qxfx0_types::atom::Relation {
            from: world_id,
            to: subject_id,
            rel_type: RelationType::RelRelatedTo,
            object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
            object_text: subject.clone(),
            verb_override: None,
            ru_original: format!("мир включает {}", subject),
            en_original: format!("world includes {}", subject),
            source: qxfx0_types::atom::RelationSource::SeedFromPredicate,
            topic: subject.clone(),
            rationale: None,
            counter: None,
            synthesis: None,
        };
        state.semantic.runtime_graph.add_relation(rel);
    }

    // Commitment — initialise store on first commit.
    // The held position is what the practitioner wrote, not the renderer's
    // response: the audited renderer is deterministic per topic, so a
    // response-text statement would deduplicate every revisit into a
    // non-event, and the card would echo the system's canned thesis instead
    // of the user's words. The statement is the raw turn text; engagement
    // still matches on a lowercased copy of it.
    let position_text = rendered.routed().prepared().input().raw_text();
    if subject.len() > 2 && position_text.len() > 10 {
        let payload = FactualClaimPayload {
            statement: position_text.to_string(),
            confidence: 0.7,
            origin: CommitmentOrigin::OriginDialogueOutcome,
            turn_seq: turn,
            deps: Vec::new(),
            topic: subject.clone(),
        };
        let store = state
            .semantic
            .semantic_commitments
            .get_or_insert_with(SemanticCommitmentStore::default);
        // ADR-0044 M3: snapshot the opaque V2 value before the store
        // borrow below; the contradiction bump decodes the snapshot.
        let v2_bump_snapshot = state.semantic.essence_v2.clone();
        // Engagement is read from the store BEFORE the commit so the new
        // position cannot match itself, and the contradiction signals come
        // from the full text the user wrote — the bare topic never carries
        // them. This is the live half of the belief protocol: a challenged
        // position becomes a recorded contradiction, visible in reports and
        // on the next reflection card, not a silent flag.
        let raw_text = position_text.to_lowercase();
        let engagement = qxfx0_commitment::CommitmentOps::detect_engagement(store, &raw_text);
        let (mut new_store, result) = CommitmentOps::commit_observation(payload, store);
        match result {
            CommitResult::Duplicate(_) => {
                tracing::info!("commitment duplicate detected for topic {subject}");
            }
            CommitResult::CapacityReached => {
                // A full store must stay visible instead of silently dropping
                // the observation. No eviction: commitments are held semantic
                // positions, and silent eviction would corrupt lineage.
                tracing::warn!(
                    "commitment store at capacity; observation for topic {subject} dropped"
                );
                let family = state
                    .last_turn_decision
                    .as_ref()
                    .map(|decision| decision.family)
                    .unwrap_or(CanonicalMoveFamily::CMGround);
                state
                    .governance_log
                    .append(qxfx0_types::governance::GovernanceEvent {
                        turn,
                        event_type:
                            qxfx0_types::governance::GovernanceEventType::CommitmentCapacityReached,
                        family,
                        guard_status: GuardStatus::Allowed,
                        timestamp: format!("turn-{turn}"),
                    });
            }
            CommitResult::New(new_id) => {
                if matches!(
                    engagement.match_kind,
                    qxfx0_commitment::MatchKind::ContradictedStrong
                ) {
                    if let Some(counterpart) = engagement
                        .engaged_ids
                        .iter()
                        .filter(|cid| **cid != new_id)
                        .cloned()
                        .max_by_key(|cid| {
                            new_store
                                .active
                                .get(cid)
                                .map(|(_, turn)| *turn)
                                .unwrap_or(0)
                        })
                    {
                        new_store = CommitmentOps::contradict(
                            &new_id,
                            &counterpart,
                            qxfx0_types::system_state::ContradictionKind::ContradictionStatement,
                            turn,
                            &new_store,
                        );
                        // A caught contradiction is existential tension by
                        // definition: the practitioner's own beliefs
                        // collided in their journal. It feeds the essence
                        // angst as a double divergent-witness accrual, so
                        // the belief protocol and the essence trajectory
                        // move as one practice. ADR-0044 M3: the live
                        // layer moves.
                        match subject_authority {
                            crate::SubjectAuthority::V1Authority => {
                                state.semantic.essence.angst = (state.semantic.essence.angst
                                    + em.angst_accrual_rate
                                    + em.angst_accrual_rate)
                                    .min(1.0);
                            }
                            crate::SubjectAuthority::V2Authority => {
                                let mut essence: qxfx0_self_v2::Essence = match v2_bump_snapshot {
                                    None => qxfx0_self_v2::empty_essence(),
                                    Some(value) => {
                                        serde_json::from_value(value).map_err(|error| {
                                            format!("essence_v2 bump failed to decode: {error}")
                                        })?
                                    }
                                };
                                let rate =
                                    qxfx0_self_v2::EssenceModulation::default().angst_accrual_rate;
                                let trajectory = match &mut essence {
                                    qxfx0_self_v2::Essence::Uncommitted(trajectory)
                                    | qxfx0_self_v2::Essence::Committed(trajectory, _) => {
                                        trajectory
                                    }
                                };
                                trajectory.angst_level =
                                    (trajectory.angst_level + rate + rate).min(1.0);
                                state.semantic.essence_v2 =
                                    Some(serde_json::to_value(&essence).map_err(|error| {
                                        format!("essence_v2 bump failed to encode: {error}")
                                    })?);
                            }
                        }
                        let family = state
                            .last_turn_decision
                            .as_ref()
                            .map(|decision| decision.family)
                            .unwrap_or(CanonicalMoveFamily::CMGround);
                        state
                            .governance_log
                            .append(qxfx0_types::governance::GovernanceEvent {
                                turn,
                                event_type: qxfx0_types::governance::GovernanceEventType::CommitmentContradicted,
                                family,
                                guard_status: GuardStatus::Allowed,
                                timestamp: format!("turn-{turn}"),
                            });
                    }
                }
            }
        }
        *store = new_store;
    }

    if state.semantic.runtime_graph.edges.len() != edge_count_before {
        state.semantic.cached_network = None;
        state.semantic.cached_edge_count = 0;
    }

    Ok(FinalizedTurnContext::new(rendered))
}

/// Stage 6: Guard — content quality + post-render safety.
pub fn guard_stage(
    state: &mut SystemState,
    finalized: FinalizedTurnContext,
) -> Result<GuardedTurnContext, String> {
    let response = finalized.rendered().response().to_owned();
    let topic = finalized
        .rendered()
        .routed()
        .prepared()
        .input()
        .subject()
        .to_owned();
    let raw_input = finalized
        .rendered()
        .routed()
        .prepared()
        .input()
        .raw_text()
        .to_owned();
    let history: &[String] = &state.dialogue.history;

    let guard_config = qxfx0_guard::GuardConfig::default();
    if raw_input.trim().is_empty() || raw_input.chars().count() > guard_config.max_input_length {
        let reason = if raw_input.trim().is_empty() {
            "пустой ввод"
        } else {
            "слишком длинный ввод"
        };
        let status = GuardStatus::InvariantBlock(reason.into());
        state.last_turn_decision = Some(TurnDecision {
            family: CanonicalMoveFamily::CMRepair,
            force: IllocutionaryForce::IFAssert,
            guard_status: status.clone(),
            legitimacy: 0.0,
        });
        return Ok(GuardedTurnContext::new(
            finalized,
            CanonicalMoveFamily::CMRepair,
            status,
            true,
            Some(reason.into()),
            Some(RecoveryTrace::enabled(
                FallbackReason::QualityRejection,
                RecoveryEvidence::QualityGate {
                    phase: QualityGatePhase::Input,
                    detail: reason.into(),
                },
            )),
        ));
    }

    let safety_status = ContentQualityGate::post_render_safety(&response, history, &guard_config);
    if matches!(&safety_status, GuardStatus::InvariantBlock(_)) {
        let recovery_detail = format!("{safety_status:?}");
        state.last_turn_decision = Some(TurnDecision {
            family: CanonicalMoveFamily::CMRepair,
            force: IllocutionaryForce::IFAssert,
            guard_status: safety_status.clone(),
            legitimacy: 0.0,
        });
        return Ok(GuardedTurnContext::new(
            finalized,
            CanonicalMoveFamily::CMRepair,
            safety_status,
            true,
            Some("Blocked by post-render safety".into()),
            Some(RecoveryTrace::enabled(
                FallbackReason::QualityRejection,
                RecoveryEvidence::QualityGate {
                    phase: QualityGatePhase::PostRenderSafety,
                    detail: recovery_detail,
                },
            )),
        ));
    }

    let verdict = ContentQualityGate::evaluate(&topic, &response);
    let (blocked, status, recovery_detail) = match verdict {
        qxfx0_guard::QualityVerdict::Block(reason) => {
            let detail = reason.clone();
            (true, GuardStatus::Blocked(reason), Some(detail))
        }
        qxfx0_guard::QualityVerdict::Pass => {
            let status = if matches!(safety_status, GuardStatus::InvariantWarn(_)) {
                safety_status
            } else {
                GuardStatus::Allowed
            };
            (false, status, None)
        }
    };

    let family = if blocked {
        CanonicalMoveFamily::CMRepair
    } else {
        finalized.rendered().routed().family()
    };

    state.last_turn_decision = Some(TurnDecision {
        family,
        force: IllocutionaryForce::IFAssert,
        guard_status: status.clone(),
        legitimacy: if blocked { 0.0 } else { 1.0 },
    });

    let rejection = blocked.then(|| "Blocked by content quality gate".into());
    let recovery = recovery_detail.map(|detail| {
        RecoveryTrace::enabled(
            FallbackReason::QualityRejection,
            RecoveryEvidence::QualityGate {
                phase: QualityGatePhase::ContentQuality,
                detail,
            },
        )
    });
    Ok(GuardedTurnContext::new(
        finalized, family, status, blocked, rejection, recovery,
    ))
}

/// Uninhabited because the in-memory governance append has no failure path.
#[derive(Debug, Serialize)]
pub enum PersistStageError {}

impl fmt::Display for PersistStageError {
    fn fmt(&self, _formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match *self {}
    }
}

/// Stage 7: Persist — governance log archiving.
pub fn persist_stage(
    state: &mut SystemState,
    guarded: GuardedTurnContext,
) -> Result<PersistedTurnContext, PersistStageError> {
    let blocked = guarded.blocked();
    let family = guarded.family();

    let warned = state
        .last_turn_decision
        .as_ref()
        .is_some_and(|decision| matches!(decision.guard_status, GuardStatus::InvariantWarn(_)));
    let event_type = if blocked {
        qxfx0_types::governance::GovernanceEventType::GuardBlocked
    } else if warned {
        qxfx0_types::governance::GovernanceEventType::GuardWarning
    } else {
        qxfx0_types::governance::GovernanceEventType::TurnCompleted
    };

    // Use the real guard status from guard_stage, not a synthesized one.
    let guard_status = state
        .last_turn_decision
        .as_ref()
        .map(|d| d.guard_status.clone())
        .unwrap_or(GuardStatus::InvariantOk);

    let turn = state.dialogue.turn_count + 1;
    let event = qxfx0_types::governance::GovernanceEvent {
        turn,
        event_type,
        family,
        guard_status,
        timestamp: format!("turn-{}", turn),
    };
    state.governance_log.append(event);
    state.governance_log.trim(10_000);

    Ok(PersistedTurnContext::new(guarded))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn subject_authority_defaults_to_v2_with_stable_labels() {
        assert_eq!(
            crate::turn_api::TurnOptions::default().subject_authority,
            crate::SubjectAuthority::V2Authority
        );
        assert_eq!(
            crate::turn_types::subject_authority_label(crate::SubjectAuthority::V2Authority),
            "v2_authority"
        );
        assert_eq!(
            crate::turn_types::subject_authority_from_label("v1_authority"),
            Some(crate::SubjectAuthority::V1Authority)
        );
        assert_eq!(
            crate::turn_types::subject_authority_from_label("v2_authority"),
            Some(crate::SubjectAuthority::V2Authority)
        );
        assert!(crate::turn_types::subject_authority_from_label("v3").is_none());
    }

    fn prepared_with(authority: crate::SubjectAuthority) -> PreparedTurnContext {
        let mut state = SystemState {
            session_id: "m1-authority".into(),
            ..SystemState::default()
        };
        let raw_text = "что такое свобода?".to_string();
        let input = TurnInputContext::new(
            state.session_id.clone(),
            raw_text.clone(),
            PropositionParser::parse(&raw_text),
            false,
        );
        prepare_stage(&mut state, input, authority).unwrap()
    }

    #[test]
    fn v2_prepare_reads_the_canonical_source() {
        let v1 = prepared_with(crate::SubjectAuthority::V1Authority);
        let v2 = prepared_with(crate::SubjectAuthority::V2Authority);
        // The canonical energy is a log-scale scalar over real blanket
        // substance (morphology runtime is loaded in tests), far above
        // the working layer's field-local value; the bias is a [0,1]
        // squash by construction.
        assert!(
            v2.conatus_energy() > 5.0,
            "v2 energy: {}",
            v2.conatus_energy()
        );
        assert!((0.0..=1.0).contains(&v2.salience()));
        assert_ne!(
            v1.conatus_energy(),
            v2.conatus_energy(),
            "the flip must have an effect, or there is nothing to migrate"
        );
    }

    #[test]
    fn v2_prepare_is_deterministic() {
        let first = prepared_with(crate::SubjectAuthority::V2Authority);
        let second = prepared_with(crate::SubjectAuthority::V2Authority);
        assert_eq!(first.conatus_energy(), second.conatus_energy());
        assert_eq!(first.salience(), second.salience());
        assert_eq!(first.holistic_dominant(), second.holistic_dominant());
        assert_eq!(first.deliberation_family(), second.deliberation_family());
    }

    #[test]
    fn concentrate_collapses_legacy_output_past_the_gate() {
        fn legacy_response(arousal: f64) -> String {
            let mut state = SystemState {
                session_id: "concentrate".into(),
                ..SystemState::default()
            };
            let raw_text = "размышляю о времени и памяти".to_string();
            let input = TurnInputContext::new(
                state.session_id.clone(),
                raw_text.clone(),
                PropositionParser::parse(&raw_text),
                false,
            );
            let prepared =
                prepare_stage(&mut state, input, crate::SubjectAuthority::V1Authority).unwrap();
            let routed = route_stage(&mut state, prepared, false).unwrap();
            let planned = plan_shadow_stage(&mut state, routed).unwrap();
            state.semantic.field.atmosphere.arousal = arousal;
            render_stage(
                &mut state,
                planned,
                RendererAuthority::LegacyShadow,
                crate::SubjectAuthority::V1Authority,
            )
            .unwrap()
            .response()
            .to_string()
        }
        let calm = legacy_response(0.2);
        assert!(
            calm.matches(". ").count() >= 1,
            "control must be multi-sentence: {calm}"
        );
        let pressed = legacy_response(0.9);
        assert!(
            !pressed[..pressed.len().saturating_sub(1)].contains(". "),
            "past the gate the response is one sentence: {pressed}"
        );
    }

    #[test]
    fn topic_novelty_separates_bare_substantive_and_unknown() {
        assert_eq!(
            super::topic_novelty("что такое свобода?", "свобода"),
            0.0,
            "bare topic question says nothing new"
        );
        let mid = super::topic_novelty("свобода без ответственности это произвол", "свобода");
        assert!(mid > 0.3, "substantive entry scores mid: {mid}");
        assert_eq!(
            super::topic_novelty("что такое ксеномодус?", "ксеномодус"),
            1.0,
            "unknown territory is salient"
        );
    }

    #[test]
    fn frame_hint_routes_mental_verb_questions_to_reflect() {
        use qxfx0_types::CanonicalMoveFamily;
        let mut state = SystemState {
            session_id: "frame-route".into(),
            ..SystemState::default()
        };
        // The legacy cascade has no `об`-shape here (would fall back
        // past Reflect); the frame's mental-verb hint routes Reflect.
        let raw_text = "ты помнишь меня?".to_string();
        let input = TurnInputContext::new(
            state.session_id.clone(),
            raw_text.clone(),
            PropositionParser::parse(&raw_text),
            false,
        );
        assert_ne!(
            input.mode(),
            qxfx0_semantic::composer::PropositionMode::Reflect,
            "precondition: legacy misses this shape"
        );
        let prepared =
            prepare_stage(&mut state, input, crate::SubjectAuthority::V1Authority).unwrap();
        let routed = route_stage(&mut state, prepared, false).unwrap();
        assert_eq!(routed.family(), CanonicalMoveFamily::CMReflect);
        // Deterministic: same input, same family.
        let mut again = SystemState {
            session_id: "frame-route".into(),
            ..SystemState::default()
        };
        let input = TurnInputContext::new(
            again.session_id.clone(),
            raw_text.clone(),
            PropositionParser::parse(&raw_text),
            false,
        );
        let prepared =
            prepare_stage(&mut again, input, crate::SubjectAuthority::V1Authority).unwrap();
        let rerouted = route_stage(&mut again, prepared, false).unwrap();
        assert_eq!(rerouted.family(), CanonicalMoveFamily::CMReflect);
    }

    #[test]
    fn clarification_names_divergent_focus() {
        assert_eq!(
            clarification_surface("памяти", None),
            "Мне нужно уточнение: что именно вы хотите прояснить о «памяти»?"
        );
        assert_eq!(
            clarification_surface("памяти", Some("памяти")),
            "Мне нужно уточнение: что именно вы хотите прояснить о «памяти»?",
            "focus equal to the subject keeps the historical wording"
        );
        assert_eq!(
            clarification_surface("памяти", Some("ответственности")),
            "Мне нужно уточнение: что именно вы хотите прояснить о «ответственности»?",
            "divergent emphasis is asked about, not re-asked as topic"
        );
    }

    #[test]
    fn v2_prepare_runs_the_ladder_not_the_v1_switch() {
        use qxfx0_self::deliberation::ReconcileRule;
        let mut state = SystemState {
            session_id: "m2-ladder".into(),
            ..SystemState::default()
        };
        let raw_text = "что такое свобода?".to_string();
        let input = TurnInputContext::new(
            state.session_id.clone(),
            raw_text.clone(),
            PropositionParser::parse(&raw_text),
            false,
        );
        let prepared =
            prepare_stage(&mut state, input, crate::SubjectAuthority::V2Authority).unwrap();
        // Default field (resonance 0.5, arousal 0.4, rest 0.5): the
        // canonical verdict leans holistic at 0.52 confidence — below
        // the 0.7 escalation floor — so the ladder takes
        // HolisticAdvantage for the reflective proposal, with the
        // mapped V1 rule tag. Deterministic ladder output, not the V1
        // switch's confidence-gate path.
        assert_eq!(
            prepared.deliberation_rule(),
            ReconcileRule::RuleHolisticAdvantage
        );
        assert_eq!(
            prepared.deliberation_family(),
            CanonicalMoveFamily::CMReflect
        );
    }

    #[test]
    fn every_proposition_mode_has_a_typed_move_family() {
        let cases = [
            (PropositionMode::Define, CanonicalMoveFamily::CMDefine),
            (PropositionMode::Assert, CanonicalMoveFamily::CMGround),
            (PropositionMode::Challenge, CanonicalMoveFamily::CMRepair),
            (PropositionMode::Connect, CanonicalMoveFamily::CMConnect),
            (PropositionMode::Reflect, CanonicalMoveFamily::CMReflect),
            (PropositionMode::Greeting, CanonicalMoveFamily::CMContact),
            (PropositionMode::Purpose, CanonicalMoveFamily::CMPurpose),
            (
                PropositionMode::WorldCause,
                CanonicalMoveFamily::CMHypothesis,
            ),
        ];

        for (mode, expected) in cases {
            assert_eq!(family_for_mode(mode), expected);
        }
    }

    #[test]
    fn guard_rejection_is_a_typed_outcome_after_finalize() {
        let mut state = SystemState {
            session_id: "guard-rollback".into(),
            ..SystemState::default()
        };
        let raw_text = String::new();
        let input = TurnInputContext::new(
            state.session_id.clone(),
            raw_text.clone(),
            PropositionParser::parse(&raw_text),
            false,
        );
        let prepared =
            prepare_stage(&mut state, input, crate::SubjectAuthority::V1Authority).unwrap();
        let routed = route_stage(&mut state, prepared, false).unwrap();
        let planned = plan_shadow_stage(&mut state, routed).unwrap();
        let rendered = render_stage(
            &mut state,
            planned,
            RendererAuthority::LegacyShadow,
            crate::SubjectAuthority::V1Authority,
        )
        .unwrap();

        let finalized = finalize_stage(
            &mut state,
            rendered,
            EssenceAblation::Enabled,
            &mut None,
            crate::SubjectAuthority::V1Authority,
        )
        .unwrap();
        let guarded = guard_stage(&mut state, finalized).unwrap();

        assert!(guarded.blocked(), "guard should block empty input");
        assert_eq!(guarded.family(), CanonicalMoveFamily::CMRepair);
        assert!(guarded.rejection().is_some());
    }

    /// Regression test for the runtime-edge off-by-one: the derived-atom loop
    /// can consume the last edge slot after `can_register_topic` was
    /// evaluated, and the following unconditional "мир → topic" insert used to
    /// push the graph past MAX_RUNTIME_EDGES, permanently failing state
    /// validation for the session.
    #[test]
    fn novel_topic_registration_respects_edge_bound_after_derived_atoms() {
        fn filler(from: AtomId, to: AtomId) -> qxfx0_types::atom::Relation {
            qxfx0_types::atom::Relation {
                from,
                to,
                rel_type: qxfx0_types::relation_type::RelationType::RelRelatedTo,
                object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
                object_text: "заполнитель".into(),
                verb_override: None,
                ru_original: "заполнитель".into(),
                en_original: "filler".into(),
                source: qxfx0_types::atom::RelationSource::SeedFromPredicate,
                topic: "заполнитель".into(),
                rationale: None,
                counter: None,
                synthesis: None,
            }
        }

        for remaining in [0usize, 1, 2] {
            let mut state = SystemState {
                session_id: format!("edge-bound-unit-{remaining}"),
                ..SystemState::default()
            };
            // A flat field keeps tag derivation deterministic: conatus drops
            // below 0.3, so a novel topic yields Searching + AgencyLost +
            // Exhaustion + NeedContact and the derived-atom loop registers
            // two edges before the "мир" insert runs.
            state.semantic.field = qxfx0_types::field::Field {
                resonance: 0.0,
                atmosphere: qxfx0_types::field::Atmosphere {
                    valence: 0.0,
                    arousal: 0.0,
                },
                confidence: 0.0,
                consolidation: 0.0,
                counterfactual: 0.3,
            };
            state.semantic.runtime_graph = qxfx0_semantic::seed_graph();
            state.semantic.cached_network = None;

            let raw_text = "что такое флюгегехаймен?".to_string();
            let input = TurnInputContext::new(
                state.session_id.clone(),
                raw_text,
                PropositionParser::parse("что такое флюгегехаймен?"),
                false,
            );
            let prepared =
                prepare_stage(&mut state, input, crate::SubjectAuthority::V1Authority).unwrap();
            let routed = route_stage(&mut state, prepared, false).unwrap();
            let planned = plan_shadow_stage(&mut state, routed).unwrap();
            let rendered = render_stage(
                &mut state,
                planned,
                RendererAuthority::LegacyShadow,
                crate::SubjectAuthority::V1Authority,
            )
            .unwrap();

            // Fill the graph between two existing seed atoms so every
            // endpoint stays valid, leaving `remaining` edge slots.
            let endpoints: Vec<AtomId> =
                state.semantic.runtime_graph.atoms.keys().cloned().collect();
            let (from, to) = (endpoints[0].clone(), endpoints[1].clone());
            let target = MAX_RUNTIME_EDGES - remaining;
            while state.semantic.runtime_graph.edges.len() < target {
                state
                    .semantic
                    .runtime_graph
                    .add_relation(filler(from.clone(), to.clone()));
            }
            assert_eq!(state.semantic.runtime_graph.edges.len(), target);
            // Filler inserts bypass the cache: drop it like persistence does
            // so validation stays clean and finalize rebuilds on demand.
            state.semantic.cached_network = None;

            let finalized = finalize_stage(
                &mut state,
                rendered,
                EssenceAblation::Enabled,
                &mut None,
                crate::SubjectAuthority::V1Authority,
            )
            .unwrap();
            assert!(!finalized.rendered().response().is_empty());
            assert!(
                state.semantic.runtime_graph.edges.len() <= MAX_RUNTIME_EDGES,
                "edge bound exceeded at remaining={remaining}: {}",
                state.semantic.runtime_graph.edges.len()
            );
            assert!(
                state.validate().is_empty(),
                "state invalid after bounded finalize at remaining={remaining}: {:?}",
                state.validate()
            );
        }
    }
}

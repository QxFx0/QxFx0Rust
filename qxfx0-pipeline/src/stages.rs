//! Pipeline stages — synchronous, sequential processing over typed contexts.

use crate::conversation_fsm::{
    fsm_state_discriminant, fsm_state_from_discriminant, initial_state, proposition_to_event,
    transition as fsm_transition,
};
use crate::turn_context::{
    FinalizedTurnContext, GuardedTurnContext, PersistedTurnContext, PlannedTurnContext,
    PreparedTurnContext, RenderedTurnContext, RoutedTurnContext, TurnInputContext,
};
use qxfx0_commitment::{CommitResult, CommitmentOps};
use qxfx0_guard::ContentQualityGate;
use qxfx0_render::RenderEngine;
use qxfx0_self::{
    collapse_essence, commit_essence,
    deliberation::{self, DeliberationModulation, Plan},
    integrate_curated_claims, resolve_render_stance, should_commit_essence, witness_essence,
    Conatus, EssenceMode, EssenceModulation, PerspectiveRenderStance, Salience, SelfBlanket,
};
use qxfx0_semantic::{
    argued_topic_registry, derive_atoms, normalize_punctuation, seed_graph, ClaimRole,
    DialogueObligation, FallbackPlan, FallbackReason, PlanOutcome, PlanSubject, PropositionMode,
    PropositionParser, QualityGatePhase, ReadyResponsePlan, RecoveryEvidence, RecoveryTrace,
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

/// Stage 1: Prepare — Self Layer: Conatus, Salience, Deliberation.
pub fn prepare_stage(
    state: &mut SystemState,
    input: TurnInputContext,
) -> Result<PreparedTurnContext, String> {
    let field = state.semantic.field.clone();
    let conatus_energy = Conatus::compute(&field);
    let salience = Salience::compute(&field);
    let holistic_prop = field.resonance * 0.6 + field.counterfactual * 0.4;
    let formal_prop = field.confidence * 0.7 + field.consolidation * 0.3;
    let holistic_dominant = salience > 0.5;

    let violations = SelfBlanket::check(&field, conatus_energy);
    if !violations.is_empty() {
        tracing::warn!("Self-blanket violations: {:?}", violations);
    }

    let modln = DeliberationModulation::default();
    let holistic_plan = Plan {
        family: if holistic_dominant {
            CanonicalMoveFamily::CMReflect
        } else {
            CanonicalMoveFamily::CMGround
        },
        holistic_dominant: true,
        recovery_cause: None,
        confidence: holistic_prop.clamp(0.0, 1.0),
    };
    let formal_plan = Plan {
        family: CanonicalMoveFamily::CMDefine,
        holistic_dominant: false,
        recovery_cause: None,
        confidence: formal_prop.clamp(0.0, 1.0),
    };
    let deliberation = deliberation::reconcile(
        &modln,
        &holistic_plan,
        &formal_plan,
        &field,
        conatus_energy,
        salience,
        holistic_dominant,
    );

    let essence_strength = if state.semantic.essence.trajectory_committed {
        state.semantic.essence.witnesses.len() as f64 / 10.0
    } else {
        0.0
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
    let subject_atom_id = match &input.proposition().subject_resolution {
        qxfx0_semantic::concept_resolver::ResolutionOutcome::Resolved(entry) => {
            entry.atom_id.clone()
        }
        _ => AtomId::new(input.subject()),
    };
    let has_enough = state
        .semantic
        .runtime_graph
        .atoms
        .contains_key(&subject_atom_id);

    Ok(PreparedTurnContext::new(
        input,
        conatus_energy,
        salience,
        holistic_dominant,
        essence_strength,
        deliberation.plan.family,
        deliberation.trace.rule,
        has_enough,
    ))
}

/// Stage 2: Route — FSM-driven move family selection (persisted across turns).
pub fn route_stage(
    state: &mut SystemState,
    prepared: PreparedTurnContext,
) -> Result<RoutedTurnContext, String> {
    let mode = prepared.input().mode();
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

    let next = fsm_transition(current, event);

    // Persist as discriminant (no JSON round-trip, no heap allocation).
    state.dialogue.conversation_state = Some(fsm_state_discriminant(&next));

    // Route-driven family selection: FSM mode determines the move family,
    // overriding the deliberation's family (which is the prepare-stage proposal).
    // This ensures distinct propositions map to distinct families.
    let family = family_for_mode(mode);

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

/// Stage 3: build the renderer-authoritative content plan. The historical
/// function name is retained for replay compatibility.
pub fn plan_shadow_stage(
    _state: &mut SystemState,
    routed: RoutedTurnContext,
) -> Result<PlannedTurnContext, String> {
    let shadow_plan = crate::shadow_plan::build_shadow_plan(&routed)?;
    Ok(PlannedTurnContext::new(routed, shadow_plan))
}

/// Stage 4: Render. Declarative content is rendered exclusively from a
/// FactId-validated ReadyResponsePlan; typed dialogue contracts retain their
/// dedicated system frames.
pub fn render_stage(
    state: &mut SystemState,
    planned: PlannedTurnContext,
) -> Result<RenderedTurnContext, String> {
    let routed = planned.routed();
    let raw = routed.prepared().input().raw_text().to_owned();
    let subject = routed.prepared().input().subject().to_owned();
    let mode = routed.prepared().input().mode();

    // Seed the runtime graph once if empty — persist it so subsequent turns
    // render against the full semantic graph, not a near-empty one.
    if state.semantic.runtime_graph.edges.is_empty() {
        state.semantic.runtime_graph = seed_graph();
    }
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

    match planned.shadow_plan() {
        PlanOutcome::Fallback(fallback) => {
            let response = render_typed_fallback(fallback);
            return Ok(RenderedTurnContext::new(
                planned,
                normalize_punctuation(&response),
                path_depth,
                false,
            ));
        }
        PlanOutcome::Ready(plan)
            if plan
                .claims()
                .iter()
                .any(|claim| claim.role() != ClaimRole::DialogueAct) =>
        {
            let response =
                render_curated_plan(plan, fp.narrative_tone(), &state.semantic.perspective)?;
            return Ok(RenderedTurnContext::new(
                planned, response, path_depth, false,
            ));
        }
        PlanOutcome::Ready(_) => {}
    }

    // Greeting is a non-factual system dialogue act.
    if mode == PropositionMode::Greeting {
        let mut prop = PropositionParser::parse(&raw);
        prop.subject = subject.clone();
        let frame = RenderEngine::frame_from_proposition(&prop);
        let response = RenderEngine::render_frame(&frame, &mut state.semantic, &fp, "");
        return Ok(RenderedTurnContext::new(
            planned,
            normalize_punctuation(&response),
            path_depth,
            false,
        ));
    }

    // External questions remain dialogue contracts. User-provided labels may
    // appear only as quoted input, never as declarative factual authority.
    if matches!(mode, PropositionMode::Purpose | PropositionMode::WorldCause) {
        let plan = planned
            .shadow_plan()
            .ready()
            .ok_or_else(|| "external dialogue contract unexpectedly fell back".to_string())?;
        let label = match plan.subject() {
            PlanSubject::External(external) => external.label(),
            other => {
                return Err(format!(
                    "external dialogue contract has subject kind '{}'",
                    other.kind()
                ))
            }
        };
        let response = match mode {
            PropositionMode::Purpose => format!(
                "Для вопроса о функции «{label}» нужен внешний источник. Уточни контекст использования."
            ),
            PropositionMode::WorldCause => format!(
                "Для вопроса «{label}» нужны внешние факты; локальный knowledge pack не даёт достаточного основания."
            ),
            _ => unreachable!("guarded by matches above"),
        };
        return Ok(RenderedTurnContext::new(
            planned,
            normalize_punctuation(&response),
            path_depth,
            false,
        ));
    }

    Err(format!(
        "ready dialogue contract has no renderer for mode {mode:?}"
    ))
}

fn render_curated_plan(
    plan: &ReadyResponsePlan,
    tone: qxfx0_types::NarrativeTone,
    perspective: &qxfx0_types::PerspectiveState,
) -> Result<String, String> {
    let registry = argued_topic_registry().map_err(str::to_owned)?;
    plan.validate_with_facts(registry.facts())?;
    let topic_id = match plan.subject() {
        PlanSubject::Topic(topic_id) => topic_id,
        other => {
            return Err(format!(
                "declarative plan has non-topic subject kind '{}'",
                other.kind()
            ))
        }
    };
    let topic = registry
        .get(topic_id.as_str())
        .ok_or_else(|| format!("no audited renderer entry for topic '{topic_id:?}'"))?;
    let topic_concept = match qxfx0_semantic::get_resolver().resolve(topic_id.as_str()) {
        qxfx0_semantic::ResolutionOutcome::Resolved(entry) => entry.concept_id,
        outcome => {
            return Err(format!(
                "audited renderer topic '{}' did not resolve uniquely: {outcome:?}",
                topic_id.as_str()
            ))
        }
    };

    let mut sentences = Vec::new();
    match tone {
        qxfx0_types::NarrativeTone::Warm => {
            sentences.push("Давай рассмотрим это вместе.".to_string())
        }
        qxfx0_types::NarrativeTone::Recovery => {
            sentences.push("Сформулирую осторожно.".to_string())
        }
        qxfx0_types::NarrativeTone::Neutral | qxfx0_types::NarrativeTone::Terse => {}
    }

    for claim in plan.claims().iter() {
        if claim.role() == ClaimRole::DialogueAct {
            continue;
        }
        let fact_id = claim
            .fact_id()
            .ok_or_else(|| format!("declarative claim '{}' has no FactId", claim.id().as_str()))?;
        let fact = registry
            .facts()
            .select(fact_id)
            .map_err(|error| error.to_string())?;
        if fact.subject != topic_concept {
            return Err(format!(
                "fact '{}' is not about plan topic '{}'",
                fact_id,
                topic_id.as_str()
            ));
        }
        let statement = topic.statement_for_fact_id(fact_id).ok_or_else(|| {
            format!(
                "fact '{}' has no audited renderer leaf for topic '{}'",
                fact_id,
                topic_id.as_str()
            )
        })?;
        if !claim
            .predicate_refs()
            .iter()
            .any(|predicate_ref| predicate_ref == statement.predicate_ref())
        {
            return Err(format!(
                "claim '{}' does not reference the renderer predicate for fact '{}'",
                claim.id().as_str(),
                fact_id
            ));
        }
        let prefix = match claim.role() {
            ClaimRole::Thesis => {
                match resolve_render_stance(perspective, &topic_concept, fact_id, registry.facts())?
                {
                    PerspectiveRenderStance::Neutral => "",
                    PerspectiveRenderStance::Affirmed => "Я сохраняю позицию: ",
                    PerspectiveRenderStance::Qualified => "Моя позиция остаётся с оговоркой: ",
                }
            }
            ClaimRole::Support => "Кроме того, ",
            ClaimRole::Counterpoint => "Однако ",
            ClaimRole::Consequence => "Поэтому ",
            ClaimRole::DialogueAct => unreachable!("filtered above"),
        };
        sentences.push(as_sentence(&format!("{prefix}{}", statement.surface())));
    }

    if sentences.is_empty() {
        return Err("declarative plan produced no curated renderer leaves".into());
    }
    if matches!(
        plan.obligation(),
        Some(DialogueObligation::CheckAgreement { .. })
    ) {
        sentences.push("Что думаешь об этом?".into());
    }
    Ok(normalize_punctuation(&sentences.join(" ")))
}

fn render_typed_fallback(plan: &FallbackPlan) -> String {
    match plan.reason() {
        FallbackReason::NoTopicProvided => "Уточни, какую тему нужно рассмотреть.".into(),
        FallbackReason::NoAdmissiblePredicate => match plan.subject() {
            Some(qxfx0_semantic::FallbackSubject::KnownTopic(topic)) => format!(
                "Я вижу тему «{}», но в локальном knowledge pack нет достаточного основания.",
                topic.as_str()
            ),
            _ => "Я вижу тему, но в локальном knowledge pack нет достаточного основания.".into(),
        },
        FallbackReason::UnknownTopic => {
            "Я вижу тему, но в локальном knowledge pack нет достаточного основания.".into()
        }
        FallbackReason::MorphologyFailure => {
            "Не удалось надёжно определить форму термина. Уточни формулировку.".into()
        }
        FallbackReason::QualityRejection => "Ответ не прошёл локальную проверку качества.".into(),
        FallbackReason::SemanticConflict => {
            "Локальные основания противоречат друг другу; требуется уточнение.".into()
        }
        FallbackReason::PersistenceRecovery => {
            "Состояние восстановлено после ошибки сохранения.".into()
        }
    }
}

fn as_sentence(surface: &str) -> String {
    let surface = surface.trim();
    let mut characters = surface.chars();
    let Some(first) = characters.next() else {
        return String::new();
    };
    let mut sentence = first.to_uppercase().collect::<String>();
    sentence.extend(characters);
    if !sentence.ends_with(['.', '!', '?']) {
        sentence.push('.');
    }
    sentence
}

/// Stage 5: Finalize — witness + commitment + graph growth + derive_atoms.
pub fn finalize_stage(
    state: &mut SystemState,
    rendered: RenderedTurnContext,
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

    if let Some(plan) = rendered.planned().shadow_plan().ready() {
        let fact_claims = plan
            .claims()
            .iter()
            .filter_map(|claim| {
                claim
                    .fact_id()
                    .map(|fact_id| (claim.role(), fact_id.clone()))
            })
            .collect::<Vec<_>>();
        if !fact_claims.is_empty() {
            let facts = argued_topic_registry().map_err(str::to_owned)?.facts();
            let (perspective, _update) =
                integrate_curated_claims(&state.semantic.perspective, turn, &fact_claims, facts)?;
            state.semantic.perspective = perspective;
        }
    }

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

    // Derive atoms + enrich graph
    let input = rendered.routed().prepared().input();
    let subject_id = match &input.proposition().subject_resolution {
        qxfx0_semantic::concept_resolver::ResolutionOutcome::Resolved(entry) => {
            entry.atom_id.clone()
        }
        _ => AtomId::new(subject.clone()),
    };
    let topic_in_graph = state.semantic.runtime_graph.atoms.contains_key(&subject_id);
    let world_id = AtomId::new("мир");
    let reserved_atoms = usize::from(!topic_in_graph)
        + usize::from(!state.semantic.runtime_graph.atoms.contains_key(&world_id));
    let can_register_topic = topic_in_graph;
    let tags = qxfx0_semantic::inference::classify_state_tags(
        topic_in_graph,
        state.semantic.field.confidence,
        state.semantic.field.counterfactual,
        state.semantic.field.resonance,
        conatus_energy,
        state.semantic.essence.angst,
    );
    let derived = derive_atoms(&tags);
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

    // Anomaly-3 collapse
    let self_ref_topics = ["я", "ты", "qxfx0", "система"];
    if state.semantic.essence.angst > 0.9
        && self_ref_topics.contains(&subject.to_lowercase().as_str())
    {
        collapse_essence(turn, &mut state.semantic.essence);
    }

    // Legacy-only dialogue commitment. This is intentionally not a FactRecord
    // and is never consulted by the curated FactRegistry selector.
    if topic_in_graph && subject.len() > 2 && response.len() > 10 {
        let payload = FactualClaimPayload {
            statement: response.clone(),
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
        let (new_store, result) = CommitmentOps::commit_observation(payload, store);
        if let CommitResult::Duplicate(_) = result {
            tracing::info!("legacy dialogue commitment duplicate for topic {subject}");
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
        let (semantic_status, observed_tokens) = qxfx0_semantic::resolve_input_status(&raw_text);
        let input = TurnInputContext::new(
            state.session_id.clone(),
            raw_text.clone(),
            PropositionParser::parse(&raw_text),
            semantic_status,
            observed_tokens,
            false,
        );
        let prepared = prepare_stage(&mut state, input).unwrap();
        let routed = route_stage(&mut state, prepared).unwrap();
        let planned = plan_shadow_stage(&mut state, routed).unwrap();
        let rendered = render_stage(&mut state, planned).unwrap();

        let finalized = finalize_stage(&mut state, rendered).unwrap();
        let guarded = guard_stage(&mut state, finalized).unwrap();

        assert!(guarded.blocked(), "guard should block empty input");
        assert_eq!(guarded.family(), CanonicalMoveFamily::CMRepair);
        assert!(guarded.rejection().is_some());
    }
}

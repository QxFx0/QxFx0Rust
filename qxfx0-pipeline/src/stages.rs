//! Pipeline stages — synchronous, sequential processing over typed contexts.

use crate::conversation_fsm::{
    fsm_state_discriminant, fsm_state_from_discriminant, initial_state, proposition_to_event,
    transition as fsm_transition,
};
use crate::turn_context::{
    FinalizedTurnContext, GuardedTurnContext, PersistedTurnContext, PreparedTurnContext,
    RenderedTurnContext, RoutedTurnContext, TurnInputContext,
};
use qxfx0_commitment::{CommitResult, CommitmentOps};
use qxfx0_guard::ContentQualityGate;
use qxfx0_render::RenderEngine;
use qxfx0_self::{
    collapse_essence, commit_essence,
    deliberation::{self, DeliberationModulation, Plan},
    should_commit_essence, witness_essence, Conatus, EssenceMode, EssenceModulation, Salience,
    SelfBlanket,
};
use qxfx0_semantic::{
    cached_semantic_network, derive_atoms, network::activate as network_activate,
    normalize_punctuation, seed_graph, ContentSelector, DiscourseComposer, DiscourseStyle,
    PropositionMode, PropositionParser, SenseDecomposer, Verbosity,
};
use qxfx0_types::atom::AtomId;
use qxfx0_types::field::FieldProfile;
use qxfx0_types::system_state::*;
use qxfx0_types::*;

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
    let has_enough = state
        .semantic
        .runtime_graph
        .atoms
        .contains_key(&AtomId::new(input.subject()));

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

/// Stage 3: Render — compose response from graph (2-level cascade: Conjugate → ContentSelector).
pub fn render_stage(
    state: &mut SystemState,
    routed: RoutedTurnContext,
) -> Result<RenderedTurnContext, String> {
    let raw = routed.prepared().input().raw_text().to_owned();
    let subject = routed.prepared().input().subject().to_owned();
    let mode = routed.prepared().input().mode();
    let is_challenge = routed.prepared().input().is_challenge();

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
            routed,
            normalize_punctuation(&response),
            path_depth,
            false,
        ));
    }

    let sn = cached_semantic_network(&mut state.semantic);
    let graph = &state.semantic.runtime_graph;

    let sense_vectors = SenseDecomposer::decompose(&raw, graph);

    // Build style from Self Layer state
    let holistic_dominant = routed.prepared().holistic_dominant();
    let angst: f64 = state.semantic.essence.angst;
    let essence_committed = state.semantic.essence.commitment.is_some();
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

    Ok(RenderedTurnContext::new(
        routed,
        normalize_punctuation(&response),
        path_depth,
        has_bridge,
    ))
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

/// Stage 4: Finalize — witness + commitment + graph growth + derive_atoms.
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

    // Graph growth for new topics
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
    if subject.len() > 2 && response.len() > 10 {
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
            tracing::info!("commitment duplicate detected for topic {subject}");
        }
        *store = new_store;
    }

    if state.semantic.runtime_graph.edges.len() != edge_count_before {
        state.semantic.cached_network = None;
        state.semantic.cached_edge_count = 0;
    }

    Ok(FinalizedTurnContext::new(rendered))
}

/// Stage 5: Guard — content quality + post-render safety.
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
        ));
    }

    let safety_status = ContentQualityGate::post_render_safety(&response, history, &guard_config);
    if matches!(&safety_status, GuardStatus::InvariantBlock(_)) {
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
        ));
    }

    let verdict = ContentQualityGate::evaluate(&topic, &response);
    let (blocked, status) = match verdict {
        qxfx0_guard::QualityVerdict::Block(reason) => (true, GuardStatus::Blocked(reason)),
        qxfx0_guard::QualityVerdict::Pass => {
            let status = if matches!(safety_status, GuardStatus::InvariantWarn(_)) {
                safety_status
            } else {
                GuardStatus::Allowed
            };
            (false, status)
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
    Ok(GuardedTurnContext::new(
        finalized, family, status, blocked, rejection,
    ))
}

/// Stage 6: Persist — governance log archiving.
pub fn persist_stage(
    state: &mut SystemState,
    guarded: GuardedTurnContext,
) -> Result<PersistedTurnContext, String> {
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
        let input = TurnInputContext::new(
            state.session_id.clone(),
            raw_text.clone(),
            PropositionParser::parse(&raw_text),
            false,
        );
        let prepared = prepare_stage(&mut state, input).unwrap();
        let routed = route_stage(&mut state, prepared).unwrap();
        let rendered = render_stage(&mut state, routed).unwrap();

        let graph_before = state.semantic.runtime_graph.edges.len();
        let essence_witnesses_before = state.semantic.essence.witnesses.len();
        let commitments_before = state
            .semantic
            .semantic_commitments
            .as_ref()
            .map(|store| store.active.len())
            .unwrap_or(0);

        let finalized = finalize_stage(&mut state, rendered).unwrap();
        let guarded = guard_stage(&mut state, finalized).unwrap();

        assert!(guarded.blocked(), "guard should block empty input");
        assert_eq!(guarded.family(), CanonicalMoveFamily::CMRepair);
        assert!(guarded.rejection().is_some());
        assert!(state.semantic.runtime_graph.edges.len() >= graph_before);
        assert!(state.semantic.essence.witnesses.len() >= essence_witnesses_before);
        assert!(
            state
                .semantic
                .semantic_commitments
                .as_ref()
                .map(|store| store.active.len())
                .unwrap_or(0)
                >= commitments_before
        );
    }
}

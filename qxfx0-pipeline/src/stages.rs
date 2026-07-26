//! Pipeline stages — synchronous, sequential processing.
//! Each stage takes &mut SystemState + &mut hints and returns Result<(), String>.

use crate::conversation_fsm::{
    fsm_state_discriminant, fsm_state_from_discriminant, initial_state, proposition_to_event,
    transition as fsm_transition,
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
    PropositionParser, SenseDecomposer, Verbosity,
};
use qxfx0_types::atom::AtomId;
use qxfx0_types::field::FieldProfile;
use qxfx0_types::system_state::*;
use qxfx0_types::*;
use std::collections::BTreeMap;

/// Shared hints passed between stages.
pub type Hints = BTreeMap<String, String>;

/// Hard bounds for persistent per-session graph growth. Seed data is far
/// below these limits; they protect long-running sessions with novel inputs.
pub const MAX_RUNTIME_ATOMS: usize = 10_000;
pub const MAX_RUNTIME_EDGES: usize = 20_000;

/// Stage 1: Prepare — Self Layer: Conatus, Salience, Deliberation.
pub fn prepare_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    // Subject/mode already parsed in process_turn and stashed in hints.
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

    hints.insert("conatus_energy".into(), conatus_energy.to_string());
    hints.insert("salience".into(), salience.to_string());
    hints.insert("holistic_dominant".into(), holistic_dominant.to_string());
    hints.insert("essence_strength".into(), essence_strength.to_string());
    hints.insert(
        "deliberation_family".into(),
        format!("{:?}", deliberation.plan.family),
    );
    hints.insert(
        "deliberation_rule".into(),
        format!("{:?}", deliberation.trace.rule),
    );

    // W3: Populate has_enough — true when the subject exists in the runtime graph,
    // meaning we have enough semantic context to reason about it.
    let subject = hints.get("subject").cloned().unwrap_or_default();
    let has_enough = state
        .semantic
        .runtime_graph
        .atoms
        .contains_key(&AtomId::new(subject));
    hints.insert("has_enough".into(), has_enough.to_string());

    Ok(())
}

/// Stage 2: Route — FSM-driven move family selection (persisted across turns).
pub fn route_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let mode_str = hints.get("raw_mode").cloned().unwrap_or_default();
    let has_info = hints
        .get("has_enough")
        .map(|s| s == "true")
        .unwrap_or(false);

    let short_mode = if mode_str.contains("Challenge") {
        "Challenge"
    } else if mode_str.contains("Define") {
        "Define"
    } else if mode_str.contains("Reflect") {
        "Reflect"
    } else if mode_str.contains("Assert") {
        "Assert"
    } else if mode_str.contains("Connect") {
        "Connect"
    } else if mode_str.contains("Greeting") {
        "Greeting"
    } else if mode_str.contains("Purpose") {
        "Purpose"
    } else if mode_str.contains("WorldCause") {
        "WorldCause"
    } else {
        "Other"
    };

    let event = proposition_to_event(short_mode, has_info);

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
    let family = match short_mode {
        "Challenge" => CanonicalMoveFamily::CMRepair,
        "Define" => CanonicalMoveFamily::CMDefine,
        "Connect" => CanonicalMoveFamily::CMConnect,
        "Assert" => CanonicalMoveFamily::CMGround,
        "Reflect" => CanonicalMoveFamily::CMReflect,
        "Greeting" => CanonicalMoveFamily::CMContact,
        "Purpose" => CanonicalMoveFamily::CMPurpose,
        "WorldCause" => CanonicalMoveFamily::CMHypothesis,
        _ => match &state.last_turn_decision {
            Some(decision) => decision.family,
            None => CanonicalMoveFamily::CMGround,
        },
    };

    hints.insert("family".into(), format!("{:?}", family));
    hints.insert("conversation_state".into(), format!("{:?}", next));
    Ok(())
}

/// Stage 3: Render — compose response from graph (2-level cascade: Conjugate → ContentSelector).
pub fn render_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let raw = hints.get("raw_text").cloned().unwrap_or_default();
    let subject = hints.get("subject").cloned().unwrap_or_default();
    let is_challenge = hints
        .get("is_challenge")
        .map(|s| s == "true")
        .unwrap_or(false);

    // Seed the runtime graph once if empty — persist it so subsequent turns
    // render against the full semantic graph, not a near-empty one.
    if state.semantic.runtime_graph.edges.is_empty() {
        state.semantic.runtime_graph = seed_graph();
    }
    let conatus_energy: f64 = hints
        .get("conatus_energy")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5);
    let salience: f64 = hints
        .get("salience")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5);
    let essence_strength: f64 = hints
        .get("essence_strength")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);

    let fp = FieldProfile::from_self(
        &state.semantic.field,
        conatus_energy,
        salience,
        essence_strength,
    );
    hints.insert("path_depth".into(), fp.path_depth().to_string());

    // Specialized intents must reach their typed frames directly. The
    // generic discourse composer intentionally emits an introduction even
    // with no predicates, so these frames cannot be implemented as a late
    // fallback.
    let mode = hints.get("raw_mode").cloned().unwrap_or_default();
    if mode.contains("Greeting") || mode.contains("Purpose") || mode.contains("WorldCause") {
        let mut prop = PropositionParser::parse(&raw);
        prop.subject = subject.clone();
        let frame = RenderEngine::frame_from_proposition(&prop);
        let response = RenderEngine::render_frame(&frame, &mut state.semantic, &fp, "");
        hints.insert("response".into(), normalize_punctuation(&response));
        return Ok(());
    }

    let sn = cached_semantic_network(&mut state.semantic);
    let graph = &state.semantic.runtime_graph;

    let sense_vectors = SenseDecomposer::decompose(&raw, graph);

    // Build style from Self Layer state
    let holistic_dominant = hints
        .get("holistic_dominant")
        .map(|s| s == "true")
        .unwrap_or(false);
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
    if let Some(ref last_topic) = state.dialogue.last_topic {
        if last_topic != &subject {
            let bridge = qxfx0_semantic::GraphEngagement::bfs_path(
                graph,
                &AtomId::new(last_topic.clone()),
                &AtomId::new(subject.clone()),
            );
            if !bridge.is_empty() {
                hints.insert("has_bridge".into(), "true".into());
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
        let raw_text = hints.get("raw_text").cloned().unwrap_or_default();
        let prop = PropositionParser::parse(&raw_text);
        let frame = RenderEngine::frame_from_proposition(&prop);
        response = RenderEngine::render_frame(&frame, &mut state.semantic, &fp, "");
    }
    if response.is_empty() {
        response =
            "Я не знаю этот смысл, но он вызывает определенный резонанс в моей системе.".into();
    }

    hints.insert("response".into(), normalize_punctuation(&response));
    Ok(())
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
pub fn finalize_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let edge_count_before = state.semantic.runtime_graph.edges.len();
    let response = hints.get("response").cloned().unwrap_or_default();
    let subject = hints.get("subject").cloned().unwrap_or_default();
    let mode_str = hints.get("raw_mode").cloned().unwrap_or_default();

    let essence_mode = if mode_str.contains("Challenge") || mode_str.contains("Assert") {
        EssenceMode::Defend
    } else if mode_str.contains("Define")
        || mode_str.contains("Reflect")
        || mode_str.contains("Purpose")
        || mode_str.contains("WorldCause")
    {
        EssenceMode::Define
    } else if mode_str.contains("Connect") {
        EssenceMode::Revise
    } else {
        EssenceMode::Commit
    };

    let turn = state.dialogue.turn_count + 1;
    let conatus_energy: f64 = hints
        .get("conatus_energy")
        .and_then(|v| v.parse().ok())
        .unwrap_or(1.0);
    let holistic_dominant = hints
        .get("holistic_dominant")
        .map(|v| v == "true")
        .unwrap_or(false);
    let salience: f64 = hints
        .get("salience")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.5);

    // Use the real deliberation trace from prepare_stage hints, not synthesized values.
    let driver = hints
        .get("deliberation_rule")
        .cloned()
        .unwrap_or_else(|| "RuleFormalAdvantage".into());
    let reconcile_rule = &driver;
    let agreement = hints
        .get("deliberation_rule")
        .map(|_| "PartialAgreement")
        .unwrap_or("NoAgreement");
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

    Ok(())
}

/// Stage 5: Guard — content quality + post-render safety.
pub fn guard_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let response = hints.get("response").cloned().unwrap_or_default();
    let topic = hints.get("subject").cloned().unwrap_or_default();
    let raw_input = hints.get("raw_text").cloned().unwrap_or_default();
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
        hints.insert("family".into(), "CMRepair".into());
        hints.insert("guard_status".into(), format!("{:?}", status));
        hints.insert("blocked".into(), "true".into());
        return Err(reason.into());
    }

    let safety_status = ContentQualityGate::post_render_safety(&response, history, &guard_config);
    if matches!(&safety_status, GuardStatus::InvariantBlock(_)) {
        let status_debug = format!("{:?}", safety_status);
        state.last_turn_decision = Some(TurnDecision {
            family: CanonicalMoveFamily::CMRepair,
            force: IllocutionaryForce::IFAssert,
            guard_status: safety_status,
            legitimacy: 0.0,
        });
        hints.insert("family".into(), "CMRepair".into());
        hints.insert("guard_status".into(), status_debug);
        hints.insert("blocked".into(), "true".into());
        return Err("Blocked by post-render safety".into());
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
        hints.insert("family".into(), "CMRepair".into());
        CanonicalMoveFamily::CMRepair
    } else {
        let family_str = hints.get("family").cloned().unwrap_or_default();
        CanonicalMoveFamily::from_hint(&family_str)
    };

    state.last_turn_decision = Some(TurnDecision {
        family,
        force: IllocutionaryForce::IFAssert,
        guard_status: status.clone(),
        legitimacy: if blocked { 0.0 } else { 1.0 },
    });

    hints.insert("guard_status".into(), format!("{:?}", status));
    hints.insert("blocked".into(), blocked.to_string());

    if blocked {
        Err("Blocked by content quality gate".into())
    } else {
        Ok(())
    }
}

/// Stage 6: Persist — governance log archiving.
pub fn persist_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let blocked = hints.get("blocked").map(|s| s == "true").unwrap_or(false);
    let family_str = hints.get("family").cloned().unwrap_or_default();
    let family = CanonicalMoveFamily::from_hint(&family_str);

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

    hints.insert("governance_events".into(), "1".into());
    Ok(())
}

//! Pipeline stages — synchronous, sequential processing.
//! Each stage takes &mut SystemState + &mut hints and returns Result<(), String>.

use std::collections::BTreeMap;
use qxfx0_types::system_state::*;
use qxfx0_types::field::FieldProfile;
use qxfx0_types::atom::AtomId;
use qxfx0_types::*;
use qxfx0_self::{
    Conatus, EssenceMode, EssenceModulation, Salience, SelfBlanket,
    collapse_essence, commit_essence, should_commit_essence, witness_essence,
    deliberation::{self, DeliberationModulation, Plan},
};
use qxfx0_semantic::{
    ContentSelector, DiscourseComposer, SenseDecomposer,
    DiscourseStyle, Verbosity,
    build_semantic_network, derive_atoms,
    network::activate as network_activate,
    seed_graph,
};
use qxfx0_commitment::{CommitmentOps, CommitResult};
use qxfx0_guard::ContentQualityGate;
use crate::conversation_fsm::{
    initial_state, proposition_to_event, transition as fsm_transition,
    fsm_state_discriminant, fsm_state_from_discriminant,
};

/// Shared hints passed between stages.
pub type Hints = BTreeMap<String, String>;

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
        &modln, &holistic_plan, &formal_plan,
        &field, conatus_energy, salience, holistic_dominant,
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
    hints.insert("deliberation_family".into(), format!("{:?}", deliberation.plan.family));
    hints.insert("deliberation_rule".into(), format!("{:?}", deliberation.trace.rule));

    // W3: Populate has_enough — true when the subject exists in the runtime graph,
    // meaning we have enough semantic context to reason about it.
    let subject = hints.get("subject").cloned().unwrap_or_default();
    let has_enough = state.semantic.runtime_graph.atoms.contains_key(&AtomId::new(subject));
    hints.insert("has_enough".into(), has_enough.to_string());

    Ok(())
}

/// Stage 2: Route — FSM-driven move family selection (persisted across turns).
pub fn route_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let mode_str = hints.get("raw_mode").cloned().unwrap_or_default();
    let has_info = hints.get("has_enough").map(|s| s == "true").unwrap_or(false);

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
    } else {
        "Other"
    };

    let event = proposition_to_event(short_mode, has_info);

    // Restore FSM state from discriminant (or use initial).
    let current = state.dialogue.conversation_state
        .and_then(fsm_state_from_discriminant)
        .unwrap_or_else(initial_state);

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
    let is_challenge = hints.get("is_challenge").map(|s| s == "true").unwrap_or(false);

    // Seed the runtime graph once if empty — persist it so subsequent turns
    // render against the full semantic graph, not a near-empty one.
    if state.semantic.runtime_graph.edges.is_empty() {
        state.semantic.runtime_graph = seed_graph();
    }
    let graph = &state.semantic.runtime_graph;

    let conatus_energy: f64 = hints.get("conatus_energy").and_then(|v| v.parse().ok()).unwrap_or(0.5);
    let salience: f64 = hints.get("salience").and_then(|v| v.parse().ok()).unwrap_or(0.5);
    let essence_strength: f64 = hints.get("essence_strength").and_then(|v| v.parse().ok()).unwrap_or(0.0);

    let fp = FieldProfile::from_self(&state.semantic.field, conatus_energy, salience, essence_strength);

    let sense_vectors = SenseDecomposer::decompose(&raw, &graph);

    // Build style from Self Layer state
    let holistic_dominant = hints.get("holistic_dominant").map(|s| s == "true").unwrap_or(false);
    let angst: f64 = state.semantic.essence.angst;
    let essence_committed = state.semantic.essence.commitment.is_some();
    let style = style_from_state(conatus_energy, angst, holistic_dominant, essence_committed);

    // Primary: DiscourseComposer (template-based, field-modulated)
    let sn = build_semantic_network(&graph);
    let cs = ContentSelector::build(&graph);
    let activated = network_activate(&AtomId::new(subject.clone()), &sn);
    let mut selected = cs.compose_from_activation(&fp, &subject, &activated);

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
            qxfx0_semantic::ConjugateComposer::compose_with_challenge(&graph, &sense_vectors, true)
        } else {
            qxfx0_semantic::ConjugateComposer::compose(&graph, &sense_vectors)
        };
        response = conjugate_surface.text;
    }

    hints.insert("response".into(), response);
    Ok(())
}

fn style_from_state(conatus: f64, angst: f64, holistic: bool, committed: bool) -> DiscourseStyle {
    DiscourseStyle {
        register: if committed { "philosophical".into() } else { "conversational".into() },
        complexity: if conatus > 0.8 { 3 } else if conatus > 0.4 { 2 } else { 1 },
        hedging: angst.min(1.0).max(0.0),
        verbosity: if holistic { Verbosity::Elaborate } else { Verbosity::Medium },
        use_transitions: conatus > 0.6,
    }
}

/// Stage 4: Finalize — witness + commitment + graph growth + derive_atoms.
pub fn finalize_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let response = hints.get("response").cloned().unwrap_or_default();
    let subject = hints.get("subject").cloned().unwrap_or_default();
    let mode_str = hints.get("raw_mode").cloned().unwrap_or_default();

    let essence_mode = if mode_str.contains("Challenge") {
        EssenceMode::Defend
    } else if mode_str.contains("Define") {
        EssenceMode::Define
    } else {
        EssenceMode::Commit
    };

    let turn = state.dialogue.turn_count + 1;
    let conatus_energy: f64 = hints.get("conatus_energy").and_then(|v| v.parse().ok()).unwrap_or(1.0);
    let holistic_dominant = hints.get("holistic_dominant").map(|v| v == "true").unwrap_or(false);
    let salience: f64 = hints.get("salience").and_then(|v| v.parse().ok()).unwrap_or(0.5);

    // Use the real deliberation trace from prepare_stage hints, not synthesized values.
    let driver = hints.get("deliberation_rule").cloned().unwrap_or_else(|| "RuleFormalAdvantage".into());
    let reconcile_rule = &driver;
    let agreement = hints.get("deliberation_rule").map(|_| "PartialAgreement").unwrap_or("NoAgreement");
    let divergence = if holistic_dominant { salience.abs() } else { 1.0 - salience };

    let em = EssenceModulation::default();
    witness_essence(&em, turn, conatus_energy, &mut state.semantic.essence,
        essence_mode, response.clone(), driver.as_str(), reconcile_rule, agreement, divergence);

    if let Some(trigger) = should_commit_essence(&em, &state.semantic.essence) {
        if state.semantic.essence.commitment.is_none() {
            let commitment = commit_essence(turn, trigger, &state.semantic.essence);
            state.semantic.essence.commitment = Some(commitment);
        }
    }

    // Derive atoms + enrich graph
    let subject_id = AtomId::new(subject.clone());
    let topic_in_graph = state.semantic.runtime_graph.atoms.contains_key(&subject_id);
    let tags = qxfx0_semantic::inference::classify_state_tags(
        topic_in_graph, state.semantic.field.confidence,
        state.semantic.field.counterfactual, state.semantic.field.resonance,
        conatus_energy, state.semantic.essence.angst,
    );
    let derived = derive_atoms(&tags);
    for da in &derived {
        let id = da.id.clone();
        if !state.semantic.runtime_graph.atoms.contains_key(&id) {
            state.semantic.runtime_graph.atoms.insert(id.clone(), qxfx0_types::atom::Atom {
                id: id.clone(),
                display: format!("{:?}", da.tag),
                category: qxfx0_types::atom::AtomCategory::CatConcept,
            });
            let rel = qxfx0_types::atom::Relation {
                from: id.clone(), to: subject_id.clone(),
                rel_type: RelationType::RelRelatedTo,
                object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
                object_text: subject.clone(),
                verb_override: None,
                ru_original: format!("производный атом ← {}", subject),
                en_original: format!("derived atom ← {}", subject),
                source: qxfx0_types::atom::RelationSource::SeedFromPredicate,
                topic: subject.clone(),
                rationale: Some(format!("derived via {:?}", da.rule)),
                counter: None, synthesis: None,
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
    if subject.chars().count() > 2 && !topic_in_graph {
        let atom = qxfx0_types::atom::Atom {
            id: subject_id.clone(),
            display: subject.clone(),
            category: qxfx0_types::atom::AtomCategory::CatTopic,
        };
        state.semantic.runtime_graph.atoms.insert(subject_id.clone(), atom);
        // Register the "мир" atom if not already present.
        let mir_id = AtomId::new("мир");
        state.semantic.runtime_graph.atoms.entry(mir_id.clone()).or_insert(qxfx0_types::atom::Atom {
            id: mir_id.clone(),
            display: "мир".into(),
            category: qxfx0_types::atom::AtomCategory::CatTopic,
        });
        let rel = qxfx0_types::atom::Relation {
            from: mir_id, to: subject_id,
            rel_type: RelationType::RelRelatedTo,
            object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
            object_text: subject.clone(),
            verb_override: None,
            ru_original: format!("мир включает {}", subject),
            en_original: format!("world includes {}", subject),
            source: qxfx0_types::atom::RelationSource::SeedFromPredicate,
            topic: subject.clone(),
            rationale: None, counter: None, synthesis: None,
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

    Ok(())
}

/// Stage 5: Guard — content quality + post-render safety.
pub fn guard_stage(state: &mut SystemState, hints: &mut Hints) -> Result<(), String> {
    let response = hints.get("response").cloned().unwrap_or_default();
    let topic = hints.get("subject").cloned().unwrap_or_default();
    let history: &[String] = &state.dialogue.history;

    let safety_status = ContentQualityGate::post_render_safety(&response, history);
    if matches!(&safety_status, GuardStatus::InvariantBlock(_)) {
        let status_debug = format!("{:?}", safety_status);
        state.last_turn_decision = Some(TurnDecision {
            family: CanonicalMoveFamily::CMRepair,
            force: IllocutionaryForce::IFAssert,
            guard_status: safety_status,
            legitimacy: 0.0,
        });
        hints.insert("guard_status".into(), status_debug);
        hints.insert("blocked".into(), "true".into());
        return Err("Blocked by post-render safety".into());
    }

    let verdict = ContentQualityGate::evaluate(&topic, &response);
    let blocked = matches!(verdict, qxfx0_guard::QualityVerdict::Block(_));
    let status = if blocked { GuardStatus::Blocked(response.clone()) } else { GuardStatus::Allowed };

    state.last_turn_decision = Some(TurnDecision {
        family: CanonicalMoveFamily::CMDefine,
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

    let event_type = if blocked {
        qxfx0_types::governance::GovernanceEventType::GuardBlocked
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

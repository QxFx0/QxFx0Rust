//! Integration tests — replay determinism, multi-turn persistence, end-to-end pipeline.

use qxfx0_pipeline::{process_turn, process_turn_with_trace, TurnInput};
use qxfx0_types::field::Atmosphere;
use qxfx0_types::system_state::SystemState;

fn test_state(session_id: &str) -> SystemState {
    SystemState {
        session_id: session_id.into(),
        ..SystemState::default()
    }
}

#[test]
fn test_replay_determinism_5_turns() {
    let inputs = [
        "что такое свобода?",
        "свобода это просто вседозволенность",
        "что ты думаешь об ответственности?",
        "как свобода связана с истиной?",
        "что такое память?",
    ];

    let mut outputs1 = Vec::new();
    let mut state1 = test_state("replay1");
    for text in &inputs {
        let input = TurnInput {
            session_id: "replay1".into(),
            raw_text: text.to_string(),
        };
        let out = process_turn(&input, &mut state1);
        outputs1.push(out.response);
    }

    let mut outputs2 = Vec::new();
    let mut state2 = test_state("replay2");
    for text in &inputs {
        let input = TurnInput {
            session_id: "replay2".into(),
            raw_text: text.to_string(),
        };
        let out = process_turn(&input, &mut state2);
        outputs2.push(out.response);
    }

    for (i, (a, b)) in outputs1.iter().zip(outputs2.iter()).enumerate() {
        assert_eq!(a, b, "turn {} response differs between runs", i);
    }
}

#[test]
fn test_pr1_typed_context_output_parity() {
    use qxfx0_types::CanonicalMoveFamily;

    let cases = [
        (
            "что такое свобода?",
            "Размышляя о свободе, можно сказать следующее. Нельзя говорить о свободе, не затрагивая выбора. Потому что без выбора действие не отличается от рефлекса. Более того, свобода нуждается в сознании. Однако трудно совместить свободу и истину. Именно поэтому свобода требует не только возможности, но и осознанности выбора. Что думаешь об этом? Например, смысл «свобода» можно проверить на конкретной ситуации и её последствиях.",
            CanonicalMoveFamily::CMDefine,
            "Reasoning",
        ),
        (
            "свобода существует",
            "Размышляя о свободе, можно сказать следующее. Нельзя говорить о свободе, не затрагивая выбора. Потому что без выбора действие не отличается от рефлекса. Более того, свобода нуждается в сознании. Однако трудно совместить свободу и истину. Именно поэтому свобода требует не только возможности, но и осознанности выбора. Что думаешь об этом? Например, смысл «свобода» можно проверить на конкретной ситуации и её последствиях.",
            CanonicalMoveFamily::CMGround,
            "Reasoning",
        ),
        (
            "свобода это просто отсутствие ограничений",
            "Размышляя о свободе, можно сказать следующее. Нельзя говорить о свободе, не затрагивая выбора. Потому что без выбора действие не отличается от рефлекса. Более того, свобода нуждается в сознании. Однако трудно совместить свободу и истину. Именно поэтому свобода требует не только возможности, но и осознанности выбора. Что думаешь об этом? Например, смысл «свобода» можно проверить на конкретной ситуации и её последствиях.",
            CanonicalMoveFamily::CMRepair,
            "Reasoning",
        ),
        (
            "как истина связана с красотой?",
            "Размышляя об истине, можно сказать следующее. Истина претендует на реальность. Более того, критерием истины служит воспроизводимость. Взгляни на это так: истина и добро переплетены. Что думаешь об этом? Например, смысл «истина» можно проверить на конкретной ситуации и её последствиях.",
            CanonicalMoveFamily::CMConnect,
            "Reasoning",
        ),
        (
            "что ты думаешь о памяти?",
            "Размышляя о памяти, можно сказать следующее. Более того, память нуждается в сознании. Взгляни на это так: память структурирует бытие. Что думаешь об этом? Например, смысл «память» можно проверить на конкретной ситуации и её последствиях.",
            CanonicalMoveFamily::CMReflect,
            "Reflecting",
        ),
        (
            "привет",
            "Привет. Я готов продолжить разговор и помочь с конкретной задачей.",
            CanonicalMoveFamily::CMContact,
            "Greeting",
        ),
        (
            "в чём функция стола?",
            "Функция стола определяется устойчивой ролью объекта и результатом его использования.",
            CanonicalMoveFamily::CMPurpose,
            "Greeting",
        ),
        (
            "почему небо голубое?",
            "Причину того, почему небо голубое, нужно проверять по внешним фактам; локальный граф может только обозначить рамку рассуждения.",
            CanonicalMoveFamily::CMHypothesis,
            "Greeting",
        ),
    ];

    for (index, (raw_text, expected_response, expected_family, expected_state)) in
        cases.into_iter().enumerate()
    {
        let session_id = format!("typed-context-parity-{index}");
        let mut state = test_state(&session_id);
        let output = process_turn(
            &TurnInput {
                session_id,
                raw_text: raw_text.into(),
            },
            &mut state,
        );

        assert_eq!(output.response, expected_response, "surface for {raw_text}");
        assert_eq!(output.family, expected_family, "family for {raw_text}");
        assert_eq!(
            output.conversation_state, expected_state,
            "FSM state for {raw_text}"
        );
        assert!(!output.blocked, "parity case was blocked: {raw_text}");
    }
}

#[test]
fn test_multi_turn_state_advances() {
    let mut state = test_state("advance");

    let input1 = TurnInput {
        session_id: "advance".into(),
        raw_text: "что такое свобода?".into(),
    };
    let out1 = process_turn(&input1, &mut state);
    assert_eq!(state.dialogue.turn_count, 1);
    assert!(!out1.response.is_empty());

    let input2 = TurnInput {
        session_id: "advance".into(),
        raw_text: "что ты думаешь об истине?".into(),
    };
    let out2 = process_turn(&input2, &mut state);
    assert_eq!(state.dialogue.turn_count, 2);
    assert!(!out2.response.is_empty());
    assert_ne!(
        out1.response, out2.response,
        "different topics should produce different responses"
    );
}

#[test]
fn test_persistence_round_trip_3_turns() {
    let db = qxfx0_persistence::Persistence::open_memory().unwrap();

    let mut state = test_state("persist-test");

    for text in &[
        "что такое свобода?",
        "что ты думаешь об ответственности?",
        "как истина связана с красотой?",
    ] {
        let input = TurnInput {
            session_id: "persist-test".into(),
            raw_text: text.to_string(),
        };
        process_turn(&input, &mut state);
    }

    db.save_state("persist-test", &state).unwrap();

    let loaded = db.load_state("persist-test").unwrap().unwrap();
    assert_eq!(loaded.dialogue.turn_count, 3);
    assert_eq!(loaded.dialogue.history.len(), 3);
    assert_eq!(loaded.session_id, "persist-test");
}

#[test]
fn test_graph_growth_across_turns() {
    let mut state = test_state("growth");

    let input = TurnInput {
        session_id: "growth".into(),
        raw_text: "что такое свобода?".into(),
    };
    process_turn(&input, &mut state);
    let initial_edges = state.semantic.runtime_graph.edges.len();

    // Use a known topic that will generate a response (not blocked by guard)
    let input = TurnInput {
        session_id: "growth".into(),
        raw_text: "что такое память?".into(),
    };
    process_turn(&input, &mut state);

    // Graph may grow from derived atoms + new topic registration
    // The seed graph already has all topics, so growth comes from
    // derive_atoms adding inferred atoms in finalize_stage.
    assert!(
        state.semantic.runtime_graph.edges.len() >= initial_edges,
        "graph should not shrink across turns (initial={}, now={})",
        initial_edges,
        state.semantic.runtime_graph.edges.len()
    );
}

#[test]
fn test_atmosphere_affects_output() {
    let mut state_warm = test_state("warm");
    state_warm.semantic.field.atmosphere = Atmosphere::new(0.8, 0.8);

    let mut state_terse = test_state("terse");
    state_terse.semantic.field.atmosphere = Atmosphere::new(0.0, 0.1);

    let warm_input = TurnInput {
        session_id: "warm".into(),
        raw_text: "что такое свобода?".into(),
    };
    let terse_input = TurnInput {
        session_id: "terse".into(),
        raw_text: "что такое свобода?".into(),
    };

    let out_warm = process_turn(&warm_input, &mut state_warm);
    let out_terse = process_turn(&terse_input, &mut state_terse);

    assert!(!out_warm.response.is_empty());
    assert!(!out_terse.response.is_empty());
    assert_ne!(
        out_warm.response, out_terse.response,
        "different atmosphere should produce different output"
    );
}

#[test]
fn test_governance_log_grows_per_turn() {
    let mut state = test_state("gov");

    for i in 0..5 {
        let input = TurnInput {
            session_id: "gov".into(),
            raw_text: format!("что такое тест{}", i),
        };
        process_turn(&input, &mut state);
    }

    assert_eq!(state.governance_log.len(), 5);
    assert!(state.governance_log.replay_check().is_empty());
}

#[test]
fn test_essence_trajectory_accumulates() {
    let mut state = test_state("essence");

    for text in &[
        "что такое свобода?",
        "что ты думаешь об ответственности?",
        "что такое истина?",
    ] {
        let input = TurnInput {
            session_id: "essence".into(),
            raw_text: text.to_string(),
        };
        process_turn(&input, &mut state);
    }

    assert!(state.semantic.essence.trajectory_committed);
    assert!(!state.semantic.essence.witnesses.is_empty());
}

#[test]
fn test_commitment_store_populated() {
    let mut state = test_state("commit");

    let input = TurnInput {
        session_id: "commit".into(),
        raw_text: "что такое свобода?".into(),
    };
    process_turn(&input, &mut state);

    assert!(
        state.semantic.semantic_commitments.is_some(),
        "commitment store should be initialised after first turn"
    );
    let store = state.semantic.semantic_commitments.as_ref().unwrap();
    assert!(
        !store.active.is_empty(),
        "at least one commitment should be active"
    );
}

#[test]
fn test_fsm_state_transitions_across_turns() {
    let mut state = test_state("fsm");

    let input = TurnInput {
        session_id: "fsm".into(),
        raw_text: "что такое свобода?".into(),
    };
    process_turn(&input, &mut state);
    assert!(state.dialogue.conversation_state.is_some());

    let input = TurnInput {
        session_id: "fsm".into(),
        raw_text: "что ты думаешь об истине?".into(),
    };
    process_turn(&input, &mut state);
    let fsm = state.dialogue.conversation_state.unwrap();
    // Reflect mode should have transitioned to Reflecting state (discriminant 6)
    assert_eq!(fsm, 6, "Reflect mode should reach Reflecting state");
}

#[test]
fn test_blocked_turn_preserves_state() {
    let mut state = test_state("block");

    let semantic_before = serde_json::to_value(&state.semantic).unwrap();

    let input = TurnInput {
        session_id: "block".into(),
        raw_text: "".into(),
    };
    let output = process_turn(&input, &mut state);

    assert!(output.blocked);
    assert_eq!(
        serde_json::to_value(&state.semantic).unwrap(),
        semantic_before,
        "blocked turn must roll back the complete persistent semantic state"
    );
    assert_eq!(state.dialogue.turn_count, 1);
    assert_eq!(state.dialogue.history.len(), 1);
    assert_eq!(state.governance_log.len(), 1);
    assert!(state.governance_log.has_blocks());
    assert!(matches!(
        state.last_turn_decision.as_ref().map(|d| &d.guard_status),
        Some(qxfx0_types::system_state::GuardStatus::InvariantBlock(_))
    ));
}

#[test]
fn test_reflect_pamyat_not_blocked() {
    let mut state = test_state("pamyat");

    let input = TurnInput {
        session_id: "pamyat".into(),
        raw_text: "что ты думаешь о памяти?".into(),
    };
    let output = process_turn(&input, &mut state);
    assert!(
        !output.blocked,
        "Reflect mode for 'память' should not be blocked. Response: '{}'",
        output.response
    );
    assert!(!output.response.is_empty());
}

#[test]
fn test_reflect_svoboda_not_blocked() {
    let mut state = test_state("svoboda");

    let input = TurnInput {
        session_id: "svoboda".into(),
        raw_text: "что ты думаешь о свободе?".into(),
    };
    let output = process_turn(&input, &mut state);
    assert!(
        !output.blocked,
        "Reflect mode for 'свобода' should not be blocked. Response: '{}'",
        output.response
    );
}

#[test]
fn test_fsm_rollback_on_blocked_turn() {
    let mut state = test_state("fsm-rollback");

    // First turn: advances FSM
    let input = TurnInput {
        session_id: "fsm-rollback".into(),
        raw_text: "что такое свобода?".into(),
    };
    process_turn(&input, &mut state);
    let fsm_after_turn1 = state.dialogue.conversation_state;

    // Second turn with empty input — should be blocked
    let input = TurnInput {
        session_id: "fsm-rollback".into(),
        raw_text: "".into(),
    };
    let output = process_turn(&input, &mut state);

    assert!(output.blocked);
    assert_eq!(
        state.dialogue.conversation_state, fsm_after_turn1,
        "FSM state should be rolled back on blocked turn"
    );
}

#[test]
fn test_language_acceptance_matrix() {
    let cases = [
        ("known", "что такое свобода?", "свобод"),
        ("unknown", "что такое квантобус?", "квантобус"),
        ("greeting", "привет", "привет"),
        ("assertion", "я купил дом", "дом"),
        ("purpose", "в чём функция стола?", "стола"),
        ("world-cause", "почему небо голубое?", "небо голубое"),
        (
            "challenge",
            "свобода — это просто вседозволенность",
            "свобод",
        ),
    ];

    for (name, text, expected_fragment) in cases {
        let mut state = SystemState {
            session_id: format!("acceptance-{name}"),
            ..SystemState::default()
        };
        let output = process_turn(
            &TurnInput {
                session_id: state.session_id.clone(),
                raw_text: text.into(),
            },
            &mut state,
        );

        assert!(!output.blocked, "{name} was blocked: {}", output.response);
        assert!(
            output.response.to_lowercase().contains(expected_fragment),
            "{name} lost its topic: {}",
            output.response
        );
        assert!(
            !output.response.contains("внутренняя ошибка"),
            "{name} reached recovery output"
        );
        assert!(
            !output.response.contains(".."),
            "{name} contains duplicated punctuation: {}",
            output.response
        );
        if name != "known" && name != "challenge" {
            assert!(
                !output
                    .response
                    .to_lowercase()
                    .contains("свобода в действии"),
                "{name} contains an unrelated hard-coded example"
            );
        }
    }
}

#[test]
fn test_derived_semantic_cache_is_not_persisted() {
    let db = qxfx0_persistence::Persistence::open_memory().unwrap();
    let mut state = SystemState {
        session_id: "cache-roundtrip".into(),
        ..SystemState::default()
    };
    let output = process_turn(
        &TurnInput {
            session_id: state.session_id.clone(),
            raw_text: "в чём функция стола?".into(),
        },
        &mut state,
    );
    assert!(!output.blocked);
    let _ = qxfx0_semantic::cached_semantic_network(&mut state.semantic);
    assert!(state.semantic.cached_network.is_some());

    db.save_state("cache-roundtrip", &state).unwrap();
    let loaded = db.load_state("cache-roundtrip").unwrap().unwrap();
    assert_eq!(loaded.dialogue.turn_count, 1);
    assert!(loaded.semantic.cached_network.is_none());
    assert_eq!(loaded.semantic.cached_edge_count, 0);
}

#[test]
fn test_stage_trace_is_replay_deterministic() {
    let input = TurnInput {
        session_id: "trace-replay".into(),
        raw_text: "что такое свобода?".into(),
    };
    let mut first = SystemState {
        session_id: input.session_id.clone(),
        ..SystemState::default()
    };
    let mut second = first.clone();

    let (first_output, first_trace) = process_turn_with_trace(&input, &mut first);
    let (second_output, second_trace) = process_turn_with_trace(&input, &mut second);

    assert_eq!(first_output.response, second_output.response);
    assert_eq!(first_trace.request_id, second_trace.request_id);
    assert_eq!(
        first_trace.replay_signature(),
        second_trace.replay_signature()
    );
    assert_eq!(
        first_trace
            .steps
            .iter()
            .map(|step| step.stage.as_str())
            .collect::<Vec<_>>(),
        [
            "prepare",
            "route",
            "plan_shadow",
            "render",
            "finalize",
            "guard",
            "persist",
            "turn_output",
        ]
    );
    let plan_step = first_trace
        .steps
        .iter()
        .find(|step| step.stage == "plan_shadow")
        .expect("shadow plan step must be replay-visible");
    assert_eq!(
        plan_step.metadata.get("plan_outcome").map(String::as_str),
        Some("ready")
    );
    assert_eq!(
        plan_step.metadata.get("response_goal").map(String::as_str),
        Some("define")
    );
    assert_eq!(
        plan_step.metadata.get("subject_kind").map(String::as_str),
        Some("topic")
    );
    assert_eq!(
        plan_step.metadata.get("plan_topic").map(String::as_str),
        Some("свобода")
    );
    assert_eq!(
        plan_step.metadata.get("plan_version").map(String::as_str),
        Some("content_v1")
    );
    assert_eq!(
        plan_step
            .metadata
            .get("plan_claim_count")
            .map(String::as_str),
        Some("3")
    );
    assert_eq!(
        plan_step
            .metadata
            .get("argued_topic_admitted")
            .map(String::as_str),
        Some("true")
    );
    assert!(plan_step
        .metadata
        .get("predicate_refs")
        .is_some_and(|refs| refs.contains("freedom_choice")));
}

#[test]
fn test_shadow_plan_trace_records_unknown_topic_recovery() {
    let input = TurnInput {
        session_id: "trace-unknown-topic".into(),
        raw_text: "что такое кванточайник?".into(),
    };
    let mut state = test_state(&input.session_id);
    let (output, trace) = process_turn_with_trace(&input, &mut state);
    let plan_step = trace
        .steps
        .iter()
        .find(|step| step.stage == "plan_shadow")
        .expect("shadow plan step must exist");

    assert!(!output.response.is_empty());
    assert_eq!(
        plan_step.metadata.get("plan_outcome").map(String::as_str),
        Some("fallback")
    );
    assert_eq!(
        plan_step
            .metadata
            .get("fallback_reason")
            .map(String::as_str),
        Some("unknown_topic")
    );
    assert_eq!(
        plan_step
            .metadata
            .get("recovery_strategy")
            .map(String::as_str),
        Some("ask_clarification")
    );
    assert_eq!(
        plan_step.metadata.get("recovery_cause").map(String::as_str),
        Some("unknown_topic")
    );
    assert_eq!(
        plan_step
            .metadata
            .get("recovery_evidence_count")
            .map(String::as_str),
        Some("1")
    );
    assert!(plan_step
        .metadata
        .get("recovery_evidence")
        .is_some_and(|evidence| evidence.contains("topic_lookup")));
}

#[test]
fn test_shadow_plan_refuses_unaudited_content_for_recognized_topic() {
    let input = TurnInput {
        session_id: "trace-unadmitted-topic".into(),
        raw_text: "что такое знание?".into(),
    };
    let mut state = test_state(&input.session_id);
    let (output, trace) = process_turn_with_trace(&input, &mut state);
    let plan_step = trace
        .steps
        .iter()
        .find(|step| step.stage == "plan_shadow")
        .expect("shadow plan step must exist");

    assert!(
        !output.response.is_empty(),
        "legacy renderer remains active"
    );
    assert_eq!(
        plan_step.metadata.get("plan_outcome").map(String::as_str),
        Some("fallback")
    );
    assert_eq!(
        plan_step
            .metadata
            .get("fallback_reason")
            .map(String::as_str),
        Some("no_admissible_predicate")
    );
    assert_eq!(
        plan_step.metadata.get("subject_kind").map(String::as_str),
        Some("known_topic")
    );
    assert_eq!(
        plan_step.metadata.get("plan_topic").map(String::as_str),
        Some("знание")
    );
}

#[test]
fn test_all_audited_topics_reach_content_plan_in_fresh_sessions() {
    let registry = qxfx0_semantic::argued_topic_registry().unwrap();

    for (index, topic) in registry.topics().enumerate() {
        let session_id = format!("audited-topic-{index}");
        let input = TurnInput {
            session_id: session_id.clone(),
            raw_text: format!("что такое {}?", topic.topic().as_str()),
        };
        let mut state = test_state(&session_id);
        let (_, trace) = process_turn_with_trace(&input, &mut state);
        let plan_step = trace
            .steps
            .iter()
            .find(|step| step.stage == "plan_shadow")
            .expect("shadow plan step must exist");

        assert_eq!(
            plan_step.metadata.get("plan_outcome").map(String::as_str),
            Some("ready"),
            "{} must be admitted",
            topic.topic().as_str()
        );
        assert_eq!(
            plan_step.metadata.get("plan_topic").map(String::as_str),
            Some(topic.topic().as_str())
        );
        assert_eq!(
            plan_step
                .metadata
                .get("argued_topic_admitted")
                .map(String::as_str),
            Some("true")
        );
    }
}

#[test]
fn test_guard_trace_uses_typed_quality_recovery() {
    let input = TurnInput {
        session_id: "trace-quality-recovery".into(),
        raw_text: String::new(),
    };
    let mut state = test_state(&input.session_id);
    let (output, trace) = process_turn_with_trace(&input, &mut state);
    let guard_step = trace
        .steps
        .iter()
        .find(|step| step.stage == "guard")
        .expect("guard step must exist");

    assert!(output.blocked);
    assert_eq!(
        guard_step
            .metadata
            .get("fallback_reason")
            .map(String::as_str),
        Some("quality_rejection")
    );
    assert_eq!(
        guard_step
            .metadata
            .get("recovery_strategy")
            .map(String::as_str),
        Some("reject_surface")
    );
    assert_eq!(
        guard_step
            .metadata
            .get("recovery_cause")
            .map(String::as_str),
        Some("quality_rejection")
    );
    assert_eq!(
        guard_step
            .metadata
            .get("recovery_evidence_count")
            .map(String::as_str),
        Some("1")
    );
    assert!(guard_step
        .metadata
        .get("recovery_evidence")
        .is_some_and(|evidence| evidence.contains("quality_gate")));
}

#[test]
fn test_session_mismatch_is_blocked_without_mutation() {
    let mut state = SystemState {
        session_id: "loaded-session".into(),
        ..SystemState::default()
    };
    let before = qxfx0_pipeline::execution_trace::calculate_stable_digest(&state).unwrap();
    let output = process_turn(
        &TurnInput {
            session_id: "other-session".into(),
            raw_text: "что такое свобода?".into(),
        },
        &mut state,
    );
    let after = qxfx0_pipeline::execution_trace::calculate_stable_digest(&state).unwrap();

    assert!(output.blocked);
    assert!(matches!(
        output.guard_status,
        qxfx0_types::system_state::GuardStatus::InvariantBlock(_)
    ));
    assert_eq!(before, after);
}

#[test]
fn test_rc_pilot_language_regressions() {
    let prompts = [
        "что такое свобода?",
        "что ты думаешь об ответственности?",
        "как истина связана с красотой?",
        "что такое память?",
        "что ты думаешь о сознании?",
        "как свобода связана с волей?",
        "что такое справедливость?",
        "что ты думаешь о смерти?",
        "как язык связан с мышлением?",
        "что такое время?",
    ];
    let forbidden = [
        "о ответственности",
        "о истине",
        "об языке",
        "бытиеа",
        "если нет последствии",
        "природа отсутствие",
        "мышление лишена",
        ":.",
    ];
    let mut state = SystemState {
        session_id: "rc-pilot-language".into(),
        semantic: qxfx0_types::system_state::SemanticState {
            runtime_graph: qxfx0_semantic::seed_graph(),
            ..Default::default()
        },
        ..Default::default()
    };

    for turn in 0..20 {
        let output = process_turn(
            &TurnInput {
                session_id: state.session_id.clone(),
                raw_text: prompts[turn % prompts.len()].into(),
            },
            &mut state,
        );
        let normalized = output.response.to_lowercase();
        for fragment in forbidden {
            assert!(
                !normalized.contains(fragment),
                "turn {turn} contains RC pilot regression '{fragment}': {}",
                output.response
            );
        }
    }
}

#[test]
fn test_soak_1000_turns_has_bounded_state() {
    let topics = [
        "что такое свобода?",
        "что ты думаешь об ответственности?",
        "как истина связана с красотой?",
        "что такое память?",
        "что ты думаешь о сознании?",
        "как свобода связана с волей?",
        "что такое справедливость?",
        "что ты думаешь о смерти?",
        "как язык связан с мышлением?",
        "что такое время?",
    ];
    let mut state = SystemState {
        session_id: "soak-1000".into(),
        ..SystemState::default()
    };
    let mut warmed_graph_size = None;

    for turn in 0..1_000 {
        let output = process_turn(
            &TurnInput {
                session_id: state.session_id.clone(),
                raw_text: topics[turn % topics.len()].into(),
            },
            &mut state,
        );
        assert!(!output.blocked, "soak turn {turn} was blocked");
        if turn == 99 {
            warmed_graph_size = Some((
                state.semantic.runtime_graph.atoms.len(),
                state.semantic.runtime_graph.edges.len(),
            ));
        }
    }

    assert_eq!(state.dialogue.turn_count, 1_000);
    assert_eq!(state.dialogue.history.len(), 1_000);
    assert_eq!(state.governance_log.len(), 1_000);
    assert_eq!(
        warmed_graph_size,
        Some((
            state.semantic.runtime_graph.atoms.len(),
            state.semantic.runtime_graph.edges.len(),
        )),
        "graph should stop growing after repeated topics warm up"
    );
    let commitments = state
        .semantic
        .semantic_commitments
        .as_ref()
        .map(|store| store.active.len() + store.quarantine.len())
        .unwrap_or(0);
    assert!(commitments <= qxfx0_commitment::MAX_COMMITMENTS);
    assert!(state.semantic.essence.witnesses.len() <= 32);
    assert!(state.validate().is_empty(), "final soak state is invalid");
}

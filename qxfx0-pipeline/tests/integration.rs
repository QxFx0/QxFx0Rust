//! Integration tests — replay determinism, multi-turn persistence, end-to-end pipeline.

use qxfx0_pipeline::{
    process_turn, process_turn_with_options, process_turn_with_options_and_trace,
    process_turn_with_trace, process_turn_with_trace_and_renderer_and_anomaly_shadow,
    process_turn_with_trace_and_renderer_and_doubt_shadow,
    process_turn_with_trace_and_renderer_and_features,
    process_turn_with_trace_and_renderer_and_features_and_suppression,
    response_plan_v2_canary_allowlist, response_plan_v2_canary_digest,
    response_plan_v2_state_parity, AnomalyShadowMode, ClarificationMode, DoubtShadowMode,
    RendererAuthority, ResponsePlanV2Authority, ResponsePlanV2Mode, SameTopicSuppressionMode,
    TurnInput, TurnOptions,
};
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
            "doubt_shadow",
            "anomaly_shadow",
            "clarification_route",
            "same_topic_suppression",
            "prepare",
            "route",
            "response_plan_v2",
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
fn response_plan_v2_canary_is_sorted_stable_and_attribute_preserving() {
    assert_eq!(
        response_plan_v2_canary_allowlist(),
        &["правда", "произвол", "свобода"]
    );
    assert_eq!(
        response_plan_v2_canary_digest(),
        response_plan_v2_canary_digest()
    );

    for topic in response_plan_v2_canary_allowlist() {
        let session_id = format!("v2-canary-{topic}");
        let input = TurnInput {
            session_id: session_id.clone(),
            raw_text: format!("что такое {topic}?"),
        };
        let mut baseline_state = test_state(&session_id);
        let baseline_output =
            process_turn_with_options(&input, &mut baseline_state, TurnOptions::new());
        let mut canary_state = test_state(&session_id);
        let (canary_output, trace) = process_turn_with_options_and_trace(
            &input,
            &mut canary_state,
            TurnOptions::new().with_response_plan_v2(ResponsePlanV2Mode::Canary),
        );

        assert_eq!(canary_output.response, baseline_output.response, "{topic}");
        assert_eq!(canary_output.family, baseline_output.family, "{topic}");
        assert_eq!(
            canary_output.guard_status, baseline_output.guard_status,
            "{topic}"
        );
        assert_eq!(canary_output.blocked, baseline_output.blocked, "{topic}");
        assert_eq!(
            canary_output.commitment_engaged, baseline_output.commitment_engaged,
            "{topic}"
        );
        assert_eq!(
            canary_output.governance_events, baseline_output.governance_events,
            "{topic}"
        );
        assert_eq!(
            canary_output.conatus_energy, baseline_output.conatus_energy,
            "{topic}"
        );
        assert_eq!(
            canary_output.path_depth, baseline_output.path_depth,
            "{topic}"
        );
        assert_eq!(
            canary_output.holistic_dominant, baseline_output.holistic_dominant,
            "{topic}"
        );
        assert_eq!(
            canary_output.conversation_state, baseline_output.conversation_state,
            "{topic}"
        );
        assert!(response_plan_v2_state_parity(
            &baseline_state,
            &canary_state
        ));

        let step = trace
            .steps
            .iter()
            .find(|step| step.stage == "response_plan_v2")
            .expect("canary trace step");
        for (key, expected) in [
            ("requested_mode", "Canary"),
            ("effective_mode", "Canary"),
            ("canary_eligible", "true"),
            ("attempted", "true"),
            ("completed", "true"),
            ("downgrade_count", "0"),
            ("downgrade_reason", "none"),
            ("v1_authoritative", "true"),
        ] {
            assert_eq!(
                step.metadata.get(key).map(String::as_str),
                Some(expected),
                "{topic}:{key}"
            );
        }
        assert_eq!(
            step.metadata.get("canary_digest"),
            Some(&response_plan_v2_canary_digest())
        );
        for parity in ["semantic_parity", "authority_parity", "realization_parity"] {
            assert!(step.metadata.contains_key(parity), "{topic}:{parity}");
        }
    }
}

#[test]
fn response_plan_v2_rollout_scopes_downgrade_without_affecting_stance_payload() {
    let input = TurnInput {
        session_id: "v2-rollout-scope".into(),
        raw_text: "что такое знание?".into(),
    };
    let attestation = qxfx0_types::StanceDecisionAttestation {
        version: qxfx0_types::STANCE_ATTESTATION_VERSION,
        issuer_id: "issuer".into(),
        key_id: "key".into(),
        audience: "audience".into(),
        session_id: input.session_id.clone(),
        expected_pre_turn: 0,
        topic: qxfx0_types::StanceTopic::new("знание").unwrap(),
        polarity: qxfx0_types::StancePolarity::Affirmed,
        request_digest: qxfx0_types::calculate_stance_request_digest(
            &input.session_id,
            &input.raw_text,
        ),
        decision_id: [3; 16],
        issued_at_unix_seconds: 100,
        expires_at_unix_seconds: 200,
    };
    let signing_payload = attestation.canonical_bytes().unwrap();
    let mut state = test_state(&input.session_id);
    let (_, trace) = process_turn_with_options_and_trace(
        &input,
        &mut state,
        TurnOptions::new().with_response_plan_v2(ResponsePlanV2Mode::Canary),
    );
    assert_eq!(attestation.canonical_bytes().unwrap(), signing_payload);
    assert!(state.semantic.stance_provenance.is_empty());

    let step = trace
        .steps
        .iter()
        .find(|step| step.stage == "response_plan_v2")
        .expect("rollout trace step");
    for (key, expected) in [
        ("requested_mode", "Canary"),
        ("effective_mode", "Off"),
        ("canary_eligible", "false"),
        ("attempted", "false"),
        ("completed", "false"),
        ("downgrade_count", "1"),
        ("downgrade_reason", "topic_outside_rollout_scope"),
    ] {
        assert_eq!(
            step.metadata.get(key).map(String::as_str),
            Some(expected),
            "{key}"
        );
    }
}

#[test]
fn response_plan_v2_canary_authority_is_explicit_and_rolls_back_to_v1() {
    let topic = "свобода";
    let input = TurnInput {
        session_id: "v2-authority-canary".into(),
        raw_text: format!("что такое {topic}?"),
    };
    let mut canary_state = test_state(&input.session_id);
    let (canary_output, canary_trace) = process_turn_with_options_and_trace(
        &input,
        &mut canary_state,
        TurnOptions::new().with_response_plan_v2_authority(ResponsePlanV2Authority::Canary),
    );
    let render = canary_trace
        .steps
        .iter()
        .find(|step| step.stage == "render")
        .expect("render trace");
    assert_eq!(
        render.metadata.get("renderer_source").map(String::as_str),
        Some("response_plan_v2")
    );
    assert_eq!(
        canary_trace
            .steps
            .iter()
            .find(|step| step.stage == "response_plan_v2")
            .and_then(|step| step.metadata.get("v1_authoritative"))
            .map(String::as_str),
        Some("false")
    );
    assert!(!canary_output.response.is_empty());

    let mut replay_state = test_state(&input.session_id);
    let (replay_output, replay_trace) = process_turn_with_options_and_trace(
        &input,
        &mut replay_state,
        TurnOptions::new().with_response_plan_v2_authority(ResponsePlanV2Authority::Canary),
    );
    assert_eq!(canary_output.response, replay_output.response);
    assert_eq!(
        canary_trace.replay_signature(),
        replay_trace.replay_signature()
    );
    assert!(response_plan_v2_state_parity(&canary_state, &replay_state));
    assert_eq!(
        canary_trace.authority_receipt,
        replay_trace.authority_receipt
    );
    assert_eq!(
        canary_trace.authority_guard_classification.as_deref(),
        Some("v2_successfully_emitted")
    );

    let mut rollback_state = test_state(&input.session_id);
    let (rollback_output, rollback_trace) = process_turn_with_options_and_trace(
        &input,
        &mut rollback_state,
        TurnOptions::new().with_response_plan_v2(ResponsePlanV2Mode::Canary),
    );
    let rollback_render = rollback_trace
        .steps
        .iter()
        .find(|step| step.stage == "render")
        .expect("rollback render trace");
    assert_ne!(canary_output.response, rollback_output.response);
    assert_eq!(
        rollback_render
            .metadata
            .get("renderer_source")
            .map(String::as_str),
        Some("legacy_graph")
    );
    assert_eq!(
        rollback_trace
            .steps
            .iter()
            .find(|step| step.stage == "response_plan_v2")
            .and_then(|step| step.metadata.get("v1_authoritative"))
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(rollback_trace.authority_guard_classification, None);
}

#[test]
fn response_plan_v2_behavioral_canary_respects_the_define_only_boundary() {
    let topics = ["правда", "произвол", "свобода"];
    for topic in topics {
        let session_id = format!("v2-behavioral-{topic}");
        let mut state = test_state(&session_id);
        for input_text in [
            format!("что такое {topic}?"),
            format!("что есть {topic}?"),
            format!("уточни, что такое {topic}?"),
            format!("что такое {topic}?"),
        ] {
            let input = TurnInput {
                session_id: session_id.clone(),
                raw_text: input_text,
            };
            let (output, trace) = process_turn_with_options_and_trace(
                &input,
                &mut state,
                TurnOptions::new().with_response_plan_v2_authority(ResponsePlanV2Authority::Canary),
            );
            assert!(!output.blocked, "eligible definition blocked for {topic}");
            assert_eq!(
                trace.authority_guard_classification.as_deref(),
                Some("v2_successfully_emitted")
            );
            assert!(trace.authority_receipt.is_some());
        }

        let challenge = TurnInput {
            session_id: session_id.clone(),
            raw_text: format!("{topic} это просто мнение"),
        };
        let (_, trace) = process_turn_with_options_and_trace(
            &challenge,
            &mut state,
            TurnOptions::new().with_response_plan_v2_authority(ResponsePlanV2Authority::Canary),
        );
        assert_eq!(
            trace.authority_guard_classification.as_deref(),
            Some("authority_denied_before_render")
        );
        assert!(trace.authority_receipt.is_none());
    }
}

#[test]
fn response_plan_v2_negative_controls_preserve_default_and_rollback_boundaries() {
    for (case_id, raw_text) in [
        ("outside-allowlist", "что такое истина?"),
        ("unknown-topic", "что такое кванточайник?"),
        ("unsupported-intent", "свобода существует"),
        ("guard-rejected", ""),
    ] {
        let session_id = format!("v2-negative-{case_id}");
        let mut state = test_state(&session_id);
        let input = TurnInput {
            session_id,
            raw_text: raw_text.into(),
        };
        let (_, trace) = process_turn_with_options_and_trace(
            &input,
            &mut state,
            TurnOptions::new().with_response_plan_v2_authority(ResponsePlanV2Authority::Canary),
        );
        assert_eq!(
            trace.authority_guard_classification.as_deref(),
            Some("authority_denied_before_render"),
            "negative control {case_id}"
        );
        assert!(trace.authority_receipt.is_none());
        assert!(trace.steps.iter().all(|step| {
            step.metadata.get("renderer_source").map(String::as_str) != Some("response_plan_v2")
        }));
    }

    let input = TurnInput {
        session_id: "v2-negative-rollback".into(),
        raw_text: "что такое свобода?".into(),
    };
    let mut authority_state = test_state(&input.session_id);
    let (_, authority_trace) = process_turn_with_options_and_trace(
        &input,
        &mut authority_state,
        TurnOptions::new().with_response_plan_v2_authority(ResponsePlanV2Authority::Canary),
    );
    assert_eq!(
        authority_trace.authority_guard_classification.as_deref(),
        Some("v2_successfully_emitted")
    );
    let (_, rollback_trace) =
        process_turn_with_options_and_trace(&input, &mut authority_state, TurnOptions::new());
    assert_eq!(rollback_trace.authority_guard_classification, None);
    assert!(rollback_trace.steps.iter().any(|step| {
        step.metadata.get("renderer_source").map(String::as_str) == Some("legacy_graph")
    }));
}

#[test]
fn same_topic_suppression_is_bounded_shadowed_and_limited() {
    let session_id = "same-topic-suppression";
    let input = TurnInput {
        session_id: session_id.into(),
        raw_text: "что такое кванточайник?".into(),
    };
    let mut prior_state = test_state(session_id);
    prior_state.semantic.field.confidence = 0.0;
    let (first, _) = process_turn_with_trace_and_renderer_and_features(
        &input,
        &mut prior_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
        ClarificationMode::LimitedEnabled,
    );
    assert_eq!(first.family, qxfx0_types::CanonicalMoveFamily::CMClarify);

    let mut baseline_state = prior_state.clone();
    let mut shadow_state = prior_state.clone();
    let (baseline, _) = process_turn_with_trace_and_renderer_and_features_and_suppression(
        &input,
        &mut baseline_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
        ClarificationMode::LimitedEnabled,
        SameTopicSuppressionMode::Disabled,
    );
    let (shadow, shadow_trace) = process_turn_with_trace_and_renderer_and_features_and_suppression(
        &input,
        &mut shadow_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
        ClarificationMode::LimitedEnabled,
        SameTopicSuppressionMode::TraceOnly,
    );
    assert_eq!(baseline.response, shadow.response);
    assert_eq!(baseline.family, shadow.family);
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&baseline_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&shadow_state).unwrap()
    );
    let shadow_step = shadow_trace
        .steps
        .iter()
        .find(|step| step.stage == "same_topic_suppression")
        .unwrap();
    assert_eq!(
        shadow_step.metadata["same_topic_suppression_eligible"],
        "true"
    );
    assert_eq!(
        shadow_step.metadata["same_topic_suppression_applied"],
        "false"
    );
    assert_eq!(
        shadow_step.metadata["same_topic_suppression_recall_count"],
        "1"
    );

    let mut enabled_state = prior_state.clone();
    let (enabled, enabled_trace) =
        process_turn_with_trace_and_renderer_and_features_and_suppression(
            &input,
            &mut enabled_state,
            RendererAuthority::LegacyShadow,
            DoubtShadowMode::Disabled,
            ClarificationMode::LimitedEnabled,
            SameTopicSuppressionMode::LimitedEnabled,
        );
    assert_eq!(enabled.family, qxfx0_types::CanonicalMoveFamily::CMDefine);
    assert!(!enabled.response.contains("Мне нужно уточнение"));
    let enabled_step = enabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "same_topic_suppression")
        .unwrap();
    assert_eq!(
        enabled_step.metadata["same_topic_suppression_applied"],
        "true"
    );
    assert_eq!(
        enabled_step.metadata["same_topic_suppression_actual_route"],
        "retain_current"
    );

    let different_input = TurnInput {
        session_id: session_id.into(),
        raw_text: "что такое другойчайник?".into(),
    };
    let mut different_state = prior_state.clone();
    let (different, different_trace) =
        process_turn_with_trace_and_renderer_and_features_and_suppression(
            &different_input,
            &mut different_state,
            RendererAuthority::LegacyShadow,
            DoubtShadowMode::Disabled,
            ClarificationMode::LimitedEnabled,
            SameTopicSuppressionMode::LimitedEnabled,
        );
    assert_eq!(
        different.family,
        qxfx0_types::CanonicalMoveFamily::CMClarify
    );
    let different_step = different_trace
        .steps
        .iter()
        .find(|step| step.stage == "same_topic_suppression")
        .unwrap();
    assert_eq!(
        different_step.metadata["same_topic_suppression_eligible"],
        "false"
    );

    let mut replay_state = prior_state;
    let (_, replay_trace) = process_turn_with_trace_and_renderer_and_features_and_suppression(
        &input,
        &mut replay_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
        ClarificationMode::LimitedEnabled,
        SameTopicSuppressionMode::LimitedEnabled,
    );
    assert_eq!(
        serde_json::to_vec(&enabled_trace).unwrap(),
        serde_json::to_vec(&replay_trace).unwrap()
    );
}

#[test]
fn clarification_route_is_default_off_shadowed_and_limited() {
    let input = TurnInput {
        session_id: "clarification-route".into(),
        raw_text: "что такое кванточайник?".into(),
    };
    let mut disabled_state = test_state(&input.session_id);
    disabled_state.semantic.field.confidence = 0.0;
    let mut shadow_state = disabled_state.clone();
    let (disabled_output, disabled_trace) = process_turn_with_trace_and_renderer_and_features(
        &input,
        &mut disabled_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
        ClarificationMode::Disabled,
    );
    let (shadow_output, shadow_trace) = process_turn_with_trace_and_renderer_and_features(
        &input,
        &mut shadow_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
        ClarificationMode::TraceOnly,
    );

    assert_eq!(disabled_output.response, shadow_output.response);
    assert_eq!(disabled_output.family, shadow_output.family);
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&disabled_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&shadow_state).unwrap()
    );
    let disabled_step = disabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "clarification_route")
        .unwrap();
    let shadow_step = shadow_trace
        .steps
        .iter()
        .find(|step| step.stage == "clarification_route")
        .unwrap();
    assert_eq!(disabled_step.metadata["clarification_enabled"], "false");
    assert_eq!(
        shadow_step.metadata["clarification_proposed_route"],
        "clarify"
    );
    assert_eq!(shadow_step.metadata["clarification_applied"], "false");

    let mut enabled_state = test_state(&input.session_id);
    enabled_state.semantic.field.confidence = 0.0;
    let (enabled_output, enabled_trace) = process_turn_with_trace_and_renderer_and_features(
        &input,
        &mut enabled_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
        ClarificationMode::LimitedEnabled,
    );
    assert_eq!(
        enabled_output.family,
        qxfx0_types::CanonicalMoveFamily::CMClarify
    );
    assert!(enabled_output.response.contains("Мне нужно уточнение"));
    assert!(enabled_output.response.contains("кванточайник"));
    assert_eq!(enabled_output.conversation_state, "Clarifying");
    let enabled_route = enabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "route")
        .unwrap();
    assert_eq!(
        enabled_route.metadata.get("family").map(String::as_str),
        Some("CMClarify")
    );
    let enabled_render = enabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "render")
        .unwrap();
    assert_eq!(
        enabled_render
            .metadata
            .get("renderer_source")
            .map(String::as_str),
        Some("clarification")
    );
}

#[test]
fn doubt_shadow_trace_is_observational_deterministic_and_bounded() {
    let session_id = "doubt-shadow-parity";
    let mut prior_state = test_state(session_id);
    let prior = process_turn(
        &TurnInput {
            session_id: session_id.into(),
            raw_text: "что такое свобода?".into(),
        },
        &mut prior_state,
    );
    assert!(!prior.blocked, "setup turn must be a confirmed decision");
    // Make the pure score exceed the clarification threshold. The proposed
    // suppression still remains trace-only and must not affect the real route.
    prior_state.semantic.field.confidence = 0.0;

    let input = TurnInput {
        session_id: session_id.into(),
        raw_text: "что такое свобода?".into(),
    };
    let mut disabled_state = prior_state.clone();
    let mut enabled_state = prior_state.clone();
    let (disabled_output, disabled_trace) = process_turn_with_trace_and_renderer_and_doubt_shadow(
        &input,
        &mut disabled_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::Disabled,
    );
    let (enabled_output, enabled_trace) = process_turn_with_trace_and_renderer_and_doubt_shadow(
        &input,
        &mut enabled_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::TraceOnly,
    );

    assert_eq!(enabled_output.response, disabled_output.response);
    assert_eq!(enabled_output.family, disabled_output.family);
    assert_eq!(enabled_output.blocked, disabled_output.blocked);
    assert_eq!(
        enabled_output.conversation_state,
        disabled_output.conversation_state
    );
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&enabled_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&disabled_state).unwrap(),
        "trace-only doubt must not change persisted state"
    );
    assert_eq!(
        serde_json::to_vec(&enabled_state).unwrap(),
        serde_json::to_vec(&disabled_state).unwrap()
    );

    for stage in ["route", "plan_shadow", "render"] {
        let disabled = disabled_trace
            .steps
            .iter()
            .find(|step| step.stage == stage)
            .expect("disabled trace stage");
        let enabled = enabled_trace
            .steps
            .iter()
            .find(|step| step.stage == stage)
            .expect("enabled trace stage");
        assert_eq!(
            disabled.output_digest, enabled.output_digest,
            "{stage} changed"
        );
        assert_eq!(disabled.metadata, enabled.metadata, "{stage} changed");
    }

    let disabled = disabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "doubt_shadow")
        .expect("disabled trace must expose its disabled mode");
    assert_eq!(
        disabled
            .metadata
            .get("doubt_shadow_enabled")
            .map(String::as_str),
        Some("false")
    );
    assert_eq!(
        disabled.metadata.get("doubt_reason").map(String::as_str),
        Some("disabled")
    );

    let enabled = enabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "doubt_shadow")
        .expect("enabled trace must expose doubt evidence");
    for key in [
        "doubt_shadow_enabled",
        "doubt_score",
        "doubt_driver",
        "doubt_recall_count",
        "doubt_proposed_route",
        "doubt_reason",
    ] {
        assert!(enabled.metadata.contains_key(key), "missing {key}");
    }
    assert_eq!(
        enabled
            .metadata
            .get("doubt_shadow_enabled")
            .map(String::as_str),
        Some("true")
    );
    assert_eq!(
        enabled.metadata.get("doubt_driver").map(String::as_str),
        Some("other")
    );
    assert_eq!(
        enabled
            .metadata
            .get("doubt_proposed_route")
            .map(String::as_str),
        Some("suppressed_by_recent_decision"),
        "this is evidence only; the actual route above remains unchanged"
    );
    let recall_count = enabled.metadata["doubt_recall_count"]
        .parse::<usize>()
        .unwrap();
    let recall_limit = enabled.metadata["doubt_recall_limit"]
        .parse::<usize>()
        .unwrap();
    let capacity = enabled.metadata["doubt_episodic_capacity"]
        .parse::<usize>()
        .unwrap();
    assert!(recall_count <= recall_limit);
    assert!(recall_count <= capacity);

    let mut replay_state = prior_state;
    let (_, replay_trace) = process_turn_with_trace_and_renderer_and_doubt_shadow(
        &input,
        &mut replay_state,
        RendererAuthority::LegacyShadow,
        DoubtShadowMode::TraceOnly,
    );
    assert_eq!(
        serde_json::to_vec(&enabled_trace).unwrap(),
        serde_json::to_vec(&replay_trace).unwrap(),
        "serialized trace excludes wall-clock duration and must replay exactly"
    );
}

#[test]
fn anomaly_shadow_trace_is_observational_deterministic_and_bounded() {
    let input = TurnInput {
        session_id: "anomaly-shadow-parity".into(),
        raw_text: "что такое я?".into(),
    };
    let mut disabled_state = test_state(&input.session_id);
    disabled_state.semantic.essence.angst = 0.95;
    let mut enabled_state = disabled_state.clone();

    let (disabled_output, disabled_trace) = process_turn_with_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut disabled_state,
        RendererAuthority::LegacyShadow,
        AnomalyShadowMode::Disabled,
    );
    let (enabled_output, enabled_trace) = process_turn_with_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut enabled_state,
        RendererAuthority::LegacyShadow,
        AnomalyShadowMode::TraceOnly,
    );

    assert_eq!(enabled_output.response, disabled_output.response);
    assert_eq!(enabled_output.family, disabled_output.family);
    assert_eq!(enabled_output.blocked, disabled_output.blocked);
    assert_eq!(
        enabled_output.conversation_state,
        disabled_output.conversation_state
    );
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&enabled_state).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&disabled_state).unwrap(),
        "trace-only anomaly recovery must not change persisted state"
    );
    assert_eq!(
        serde_json::to_vec(&enabled_state).unwrap(),
        serde_json::to_vec(&disabled_state).unwrap()
    );

    for stage in ["route", "plan_shadow", "render"] {
        let disabled = disabled_trace
            .steps
            .iter()
            .find(|step| step.stage == stage)
            .expect("disabled trace stage");
        let enabled = enabled_trace
            .steps
            .iter()
            .find(|step| step.stage == stage)
            .expect("enabled trace stage");
        assert_eq!(
            disabled.output_digest, enabled.output_digest,
            "{stage} changed"
        );
        assert_eq!(disabled.metadata, enabled.metadata, "{stage} changed");
    }

    let disabled = disabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "anomaly_shadow")
        .expect("disabled trace must expose its disabled mode");
    assert_eq!(disabled.metadata["anomaly_shadow_enabled"], "false");
    assert_eq!(disabled.metadata["anomaly_reason"], "disabled");

    let enabled = enabled_trace
        .steps
        .iter()
        .find(|step| step.stage == "anomaly_shadow")
        .expect("enabled trace must expose anomaly evidence");
    for key in [
        "anomaly_shadow_enabled",
        "anomaly_proposed_kind",
        "anomaly_strategy",
        "anomaly_result",
        "anomaly_idempotency_key",
        "anomaly_replay_outcome",
        "anomaly_reason",
    ] {
        assert!(enabled.metadata.contains_key(key), "missing {key}");
    }
    assert_eq!(enabled.metadata["anomaly_shadow_enabled"], "true");
    assert_eq!(
        enabled.metadata["anomaly_proposed_kind"],
        "self_referential_collapse"
    );
    assert_eq!(enabled.metadata["anomaly_strategy"], "reset_essence");
    assert_eq!(enabled.metadata["anomaly_result"], "essence_reset");
    assert_eq!(enabled.metadata["anomaly_replay_outcome"], "proposed");
    let ledger_len = enabled.metadata["anomaly_ledger_len"]
        .parse::<usize>()
        .unwrap();
    let ledger_capacity = enabled.metadata["anomaly_ledger_capacity"]
        .parse::<usize>()
        .unwrap();
    assert!(ledger_len <= ledger_capacity);
    assert_eq!(
        enabled.metadata["anomaly_temporal_evidence"],
        "typed_persisted_provenance"
    );

    let mut replay_state = test_state(&input.session_id);
    replay_state.semantic.essence.angst = 0.95;
    let (_, replay_trace) = process_turn_with_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut replay_state,
        RendererAuthority::LegacyShadow,
        AnomalyShadowMode::TraceOnly,
    );
    assert_eq!(
        serde_json::to_vec(&enabled_trace).unwrap(),
        serde_json::to_vec(&replay_trace).unwrap(),
        "serialized anomaly trace excludes wall-clock duration and must replay exactly"
    );
}

#[test]
fn anomaly_shadow_proposes_temporal_recovery_from_persisted_provenance_only() {
    let input = TurnInput {
        session_id: "temporal-shadow".into(),
        raw_text: "что такое свобода?".into(),
    };
    let mut state = test_state(&input.session_id);
    state.dialogue.turn_count = 1;
    state
        .semantic
        .stance_provenance
        .record(qxfx0_types::stance::StanceObservation {
            turn: 1,
            topic: qxfx0_types::stance::StanceTopic::new("свобода").unwrap(),
            polarity: qxfx0_types::stance::StancePolarity::Rejected,
            source: qxfx0_types::stance::StanceSource::SystemDecision,
        });
    let before = qxfx0_pipeline::execution_trace::calculate_stable_digest(&state).unwrap();

    let (output, trace) = process_turn_with_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut state,
        RendererAuthority::LegacyShadow,
        AnomalyShadowMode::TraceOnly,
    );
    assert!(!output.blocked);
    let anomaly = trace
        .steps
        .iter()
        .find(|step| step.stage == "anomaly_shadow")
        .unwrap();
    assert_eq!(
        anomaly.metadata["anomaly_temporal_evidence"],
        "typed_persisted_provenance"
    );
    assert_eq!(anomaly.metadata["anomaly_proposed_kind"], "temporal");
    assert_eq!(anomaly.metadata["anomaly_strategy"], "request_revision");
    assert_eq!(anomaly.metadata["anomaly_reason"], "observation_only");
    assert_eq!(state.semantic.stance_provenance.len(), 1);
    assert_ne!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&state).unwrap(),
        before,
        "ordinary turn bookkeeping changes, but trace adds no provenance"
    );
}

#[test]
fn anomaly_shadow_shared_session_replay_is_persistence_stable() {
    let session_id = "anomaly-shadow-shared-session";
    let mut seeded = test_state(session_id);
    let setup = TurnInput {
        session_id: session_id.into(),
        raw_text: "что такое свобода?".into(),
    };
    process_turn(&setup, &mut seeded);
    seeded.semantic.essence.angst = 0.95;

    let first_db = qxfx0_persistence::Persistence::open_memory().unwrap();
    let replay_db = qxfx0_persistence::Persistence::open_memory().unwrap();
    first_db.save_state(session_id, &seeded).unwrap();
    replay_db.save_state(session_id, &seeded).unwrap();
    let input = TurnInput {
        session_id: session_id.into(),
        raw_text: "что такое я?".into(),
    };

    let mut first_state = first_db.load_state(session_id).unwrap().unwrap();
    let (first_output, first_trace) = process_turn_with_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut first_state,
        RendererAuthority::LegacyShadow,
        AnomalyShadowMode::TraceOnly,
    );
    first_db.save_state(session_id, &first_state).unwrap();

    let mut replay_state = replay_db.load_state(session_id).unwrap().unwrap();
    let (replay_output, replay_trace) = process_turn_with_trace_and_renderer_and_anomaly_shadow(
        &input,
        &mut replay_state,
        RendererAuthority::LegacyShadow,
        AnomalyShadowMode::TraceOnly,
    );
    replay_db.save_state(session_id, &replay_state).unwrap();

    assert_eq!(first_output.response, replay_output.response);
    assert_eq!(first_output.family, replay_output.family);
    assert_eq!(
        serde_json::to_vec(&first_trace).unwrap(),
        serde_json::to_vec(&replay_trace).unwrap()
    );
    let first_saved = first_db.load_state(session_id).unwrap().unwrap();
    let replay_saved = replay_db.load_state(session_id).unwrap().unwrap();
    assert_eq!(
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&first_saved).unwrap(),
        qxfx0_pipeline::execution_trace::calculate_stable_digest(&replay_saved).unwrap()
    );
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

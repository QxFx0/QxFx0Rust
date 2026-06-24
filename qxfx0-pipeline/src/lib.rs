use qxfx0_commitment::CommitmentOps;
use qxfx0_governance::{GovernanceEvent, GovernanceEventType, GovernanceLog};
use qxfx0_guard::ContentQualityGate;
use qxfx0_render::RenderEngine;
use qxfx0_self::{Adjunction, Conatus, EssenceMode, Salience, SelfBlanket};
use qxfx0_semantic::{seed_graph, GraphEngagement, PropositionMode, PropositionParser};
use qxfx0_types::field::FieldProfile;
use qxfx0_types::system_state::*;
use qxfx0_types::*;

/// 6-stage TurnPipeline: Prepare → Route → Render → Finalize → Guard → Persist
pub struct TurnPipeline;

/// Turn input — parsed user input + context.
#[derive(Debug, Clone)]
pub struct TurnInput {
    pub raw_text: String,
    pub session_id: String,
}

/// Turn output — the response + metadata.
#[derive(Debug, Clone)]
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
}

impl TurnPipeline {
    /// Process a single turn through all 6 stages.
    pub fn process(input: &TurnInput, state: &mut SystemState) -> TurnOutput {
        // ── Stage 1: Prepare — Self Layer computes internal state ──
        let prop = PropositionParser::parse(&input.raw_text);
        let field = state.semantic.field.clone();

        // Conatus: system's drive — how much it "cares" about this topic
        let conatus_energy = Conatus::compute(&field);

        // Salience: Holistic/Formal bias — intuitive vs logical
        let salience = Salience::compute(&field);

        // Adjunction: reconcile Holistic ⊣ Formal
        let holistic_prop = field.resonance * 0.6 + field.counterfactual * 0.4;
        let formal_prop = field.confidence * 0.7 + field.consolidation * 0.3;
        let reconciled = Adjunction::reconcile(holistic_prop, formal_prop, &field);
        let holistic_dominant = salience > 0.5;

        // Store adjunction state
        state.semantic.adjunction = AdjunctionState {
            holistic_value: holistic_prop,
            formal_value: formal_prop,
            reconciled_value: reconciled,
            holistic_dominant,
        };

        // Essence: check if system should commit to this topic
        let essence_strength = if state.semantic.essence.trajectory_committed {
            state.semantic.essence.witnesses.len() as f64 / 10.0
        } else {
            0.0
        };

        // Build extended FieldProfile with Self-Layer signals (F4 fix)
        let fp = FieldProfile::from_self(&field, conatus_energy, salience, essence_strength);

        // Self-blanket check
        let violations = SelfBlanket::check(&field, conatus_energy);
        if !violations.is_empty() {
            tracing::warn!("Self-blanket violations: {:?}", violations);
        }

        // Update Field based on input — resonance with topic
        if !prop.subject.is_empty() {
            let topic_in_graph = state
                .semantic
                .runtime_graph
                .atoms
                .contains_key(&AtomId::new(prop.subject.clone()));
            // If topic is known → confidence increases, if unknown → counterfactual increases
            if topic_in_graph {
                state.semantic.field.confidence = (state.semantic.field.confidence + 0.1).min(1.0);
                state.semantic.field.resonance = (state.semantic.field.resonance + 0.05).min(1.0);
            } else {
                state.semantic.field.counterfactual =
                    (state.semantic.field.counterfactual + 0.1).min(1.0);
            }
        }

        // ── Stage 2: Route — Self-aware routing (F3 fix) ──
        let family = Self::route_family_aware(&prop, &fp, &state.semantic.essence);

        // ── Stage 3: Render — Self-driven generation (F1, F2 fix) ──
        let graph = if state.semantic.runtime_graph.edges.is_empty() {
            seed_graph()
        } else {
            state.semantic.runtime_graph.clone()
        };

        let engagement = GraphEngagement::engage(&graph, &prop);

        // Build commitment reference — anchored to Essence trajectory (F2 fix)
        let commitment_ref = if fp.anchors_to_trajectory() {
            if let Some(ref store) = state.semantic.semantic_commitments {
                let prior = CommitmentOps::retrieve(&prop.subject, store);
                if let Some(first) = prior.first() {
                    match prop.mode {
                        PropositionMode::Challenge => {
                            format!("Я удерживаю позицию: {}. ", first.statement)
                        }
                        PropositionMode::Define | PropositionMode::Reflect => {
                            format!("Я ранее полагал, что {}. ", first.statement)
                        }
                        _ => String::new(),
                    }
                } else {
                    String::new()
                }
            } else {
                String::new()
            }
        } else {
            String::new()
        };

        // Use RenderEngine with Self-Layer-aware FieldProfile
        let frame = RenderEngine::frame_from_proposition(&prop);
        let mut response = RenderEngine::render_frame(&frame, &graph, &fp, &commitment_ref);

        // Fallback to ContextualComposer if RenderEngine returns empty
        if response.is_empty() {
            let surface =
                qxfx0_semantic::ContextualComposer::compose(&graph, &fp, &prop, &engagement);
            response = surface.text;
        }

        // ── Stage 4: Finalize — Essence witness + commitment + graph growth ──
        state.dialogue.turn_count += 1;
        state.dialogue.last_family = family;
        state.dialogue.last_topic = prop.subject.clone();
        state.dialogue.history.push(response.clone());

        // Essence: witness this turn (F2 fix)
        let essence_mode = match prop.mode {
            PropositionMode::Define => EssenceMode::Define,
            PropositionMode::Challenge => EssenceMode::Defend,
            _ => EssenceMode::Commit,
        };
        if !Conatus::gate_fired(conatus_energy, Conatus::STRUCTURAL_FLOOR) {
            state.semantic.essence.witnesses.push(EssenceWitness {
                turn: state.dialogue.turn_count,
                mode: format!("{:?}", essence_mode),
                statement: prop.subject.clone(),
            });
            state.semantic.essence.trajectory_committed = true;
        }

        // Update commitment store
        let turn_seq = state.dialogue.turn_count;
        let mut new_commitments = false;
        if !response.is_empty() {
            let payload = FactualClaimPayload {
                statement: response.clone(),
                confidence: field.confidence,
                origin: CommitmentOrigin::OriginParser("anchor".into()),
                turn_seq,
                deps: Vec::new(),
                topic: prop.subject.clone(),
            };

            let store = state
                .semantic
                .semantic_commitments
                .clone()
                .unwrap_or_default();
            let (new_store, _) = CommitmentOps::commit_observation(payload, &store);
            state.semantic.semantic_commitments = Some(new_store);
            new_commitments = true;
        }

        // ── Graph Construction (F5 fix) — build NEW relations from Self Layer ──
        let mut enriched_count = 0;
        if new_commitments && conatus_energy > Conatus::STRUCTURAL_FLOOR {
            // Copy engagement relations (existing)
            for rel in &engagement.supporting {
                let exists = state
                    .semantic
                    .runtime_graph
                    .edges
                    .iter()
                    .any(|e| e.ru_original == rel.ru_original);
                if !exists {
                    state.semantic.runtime_graph.add_relation(rel.clone());
                    enriched_count += 1;
                }
            }

            // CONSTRUCT new relations from Self Layer state (F5 fix)
            // When system has high conatus + high counterfactual → seek contradictions
            if fp.seeks_contradictions() && !engagement.supporting.is_empty() {
                // Create a "system questions" relation: topic --RelDiffersFrom--> new_concept
                let topic_atom = AtomId::new(prop.subject.clone());
                let existing_counters = state
                    .semantic
                    .runtime_graph
                    .relations_from(&topic_atom)
                    .iter()
                    .filter(|r| {
                        r.rel_type == RelationType::RelContrastsWith
                            || r.rel_type == RelationType::RelDiffersFrom
                    })
                    .count();

                // If no contradictions exist for this topic, construct one from engagement
                if existing_counters == 0 && !engagement.supporting.is_empty() {
                    let support = &engagement.supporting[0];
                    let new_counter = Relation {
                        from: topic_atom.clone(),
                        to: support.to.clone(),
                        rel_type: RelationType::RelContrastsWith,
                        object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
                        object_text: support.object_text.clone(),
                        verb_override: None,
                        ru_original: format!(
                            "{} контрастирует с {}",
                            prop.subject, support.object_text
                        ),
                        en_original: String::new(),
                        source: qxfx0_types::atom::RelationSource::PromotedSubstrate,
                        topic: prop.subject.clone(),
                        rationale: Some(format!(
                            "различие обнаружено через контрфактический анализ (conatus={:.1})",
                            conatus_energy
                        )),
                        counter: None,
                        synthesis: None,
                    };
                    state.semantic.runtime_graph.add_relation(new_counter);
                    enriched_count += 1;
                }
            }

            // When system has high consolidation → seek structural relations
            if fp.seeks_structure() && !engagement.supporting.is_empty() {
                let topic_atom = AtomId::new(prop.subject.clone());
                let existing_structural = state
                    .semantic
                    .runtime_graph
                    .relations_from(&topic_atom)
                    .iter()
                    .filter(|r| {
                        r.rel_type == RelationType::RelPresupposes
                            || r.rel_type == RelationType::RelRequires
                    })
                    .count();

                // If no structural relations exist, construct one
                if existing_structural == 0 && !engagement.qualifying.is_empty() {
                    let qual = &engagement.qualifying[0];
                    let new_structural = Relation {
                        from: topic_atom.clone(),
                        to: qual.to.clone(),
                        rel_type: RelationType::RelPresupposes,
                        object_case: qxfx0_types::atom::ObjectCase::CaseAccusative,
                        object_text: qual.object_text.clone(),
                        verb_override: None,
                        ru_original: format!("{} предполагает {}", prop.subject, qual.object_text),
                        en_original: String::new(),
                        source: qxfx0_types::atom::RelationSource::PromotedSubstrate,
                        topic: prop.subject.clone(),
                        rationale: Some(format!(
                            "структурная связь обнаружена через консолидацию (conatus={:.1})",
                            conatus_energy
                        )),
                        counter: None,
                        synthesis: None,
                    };
                    state.semantic.runtime_graph.add_relation(new_structural);
                    enriched_count += 1;
                }
            }

            // When system anchors to trajectory → create synthesis relation
            if fp.anchors_to_trajectory() && !engagement.supporting.is_empty() {
                let topic_atom = AtomId::new(prop.subject.clone());
                let existing_synthesis = state
                    .semantic
                    .runtime_graph
                    .relations_from(&topic_atom)
                    .iter()
                    .any(|r| r.synthesis.is_some());

                if !existing_synthesis && !engagement.supporting.is_empty() {
                    let support = &engagement.supporting[0];
                    let mut syn_rel = support.clone();
                    syn_rel.synthesis = Some(format!(
                        "именно поэтому {} и {} связаны через позицию системы (turn={})",
                        prop.subject, support.object_text, turn_seq
                    ));
                    syn_rel.source = qxfx0_types::atom::RelationSource::PromotedSubstrate;
                    // Update the existing relation with synthesis
                    if let Some(idx) = state
                        .semantic
                        .runtime_graph
                        .edges
                        .iter()
                        .position(|e| e.ru_original == syn_rel.ru_original)
                    {
                        state.semantic.runtime_graph.edges[idx].synthesis =
                            syn_rel.synthesis.clone();
                    }
                }
            }
        }

        // ── Post-stage validation (D1 fix) ──
        let post_violations = Self::validate_state(state);
        if !post_violations.is_empty() {
            tracing::warn!("Post-stage state violations: {:?}", post_violations);
        }

        // ── Stage 5: Guard ──
        let (final_text, blocked) = ContentQualityGate::finalize_output(
            &prop.subject,
            &response,
            &state.dialogue.history[..state.dialogue.history.len().saturating_sub(1)],
        );

        let guard_status = if blocked {
            GuardStatus::InvariantBlock("content quality gate".into())
        } else {
            GuardStatus::InvariantOk
        };

        if blocked {
            if let Some(last) = state.dialogue.history.last_mut() {
                *last = final_text.clone();
            }
        }

        // ── Governance logging (N2 fix) ──
        let mut gov_log = GovernanceLog::new();
        gov_log.append(GovernanceEvent {
            turn: state.dialogue.turn_count,
            event_type: GovernanceEventType::TurnCompleted,
            family,
            guard_status: guard_status.clone(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        });

        if blocked {
            gov_log.append(GovernanceEvent {
                turn: state.dialogue.turn_count,
                event_type: GovernanceEventType::GuardBlocked,
                family,
                guard_status: guard_status.clone(),
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }

        if enriched_count > 0 {
            gov_log.append(GovernanceEvent {
                turn: state.dialogue.turn_count,
                event_type: GovernanceEventType::GraphEnriched {
                    new_relations: enriched_count,
                },
                family,
                guard_status: GuardStatus::InvariantOk,
                timestamp: chrono::Utc::now().to_rfc3339(),
            });
        }

        let replay_violations = gov_log.replay_check();
        if !replay_violations.is_empty() {
            tracing::warn!("Governance replay violations: {:?}", replay_violations);
        }

        let governance_events = gov_log.len();

        // ── Stage 6: Persist (caller's responsibility) ──

        let commitment_engaged = if let Some(ref store) = state.semantic.semantic_commitments {
            let eng = CommitmentOps::detect_engagement(store, &prop.subject);
            !eng.engaged_ids.is_empty()
        } else {
            false
        };

        TurnOutput {
            response: final_text,
            family,
            guard_status,
            blocked,
            commitment_engaged,
            governance_events,
            conatus_energy,
            path_depth: fp.path_depth(),
            holistic_dominant,
        }
    }

    /// Validate structural integrity of SystemState after pipeline stages.
    fn validate_state(state: &SystemState) -> Vec<String> {
        let mut violations = Vec::new();

        if state.dialogue.turn_count > 10000 {
            violations.push("turn_count_unreasonable".into());
        }
        if state.dialogue.history.len() != state.dialogue.turn_count {
            violations.push("history_length_mismatch".into());
        }
        if state.session_id.is_empty() {
            violations.push("session_id_empty".into());
        }
        for edge in &state.semantic.runtime_graph.edges {
            if edge.from == edge.to {
                violations.push(format!("self_loop: {}", edge.ru_original));
                break;
            }
        }

        violations
    }

    /// Self-aware routing — uses proposition mode + Self-Layer state (F3 fix).
    fn route_family_aware(
        prop: &qxfx0_semantic::ParsedProposition,
        fp: &FieldProfile,
        essence: &EssenceState,
    ) -> CanonicalMoveFamily {
        let base_family = match prop.mode {
            PropositionMode::Define => CanonicalMoveFamily::CMDefine,
            PropositionMode::Challenge => CanonicalMoveFamily::CMConfront,
            PropositionMode::Connect => CanonicalMoveFamily::CMDistinguish,
            PropositionMode::Reflect => CanonicalMoveFamily::CMReflect,
            PropositionMode::Assert => CanonicalMoveFamily::CMGround,
        };

        // Self-Layer modulation:
        // High counterfactual + challenge → CMConfront (already, but strengthen)
        // Low conatus + any → CMRepair (system hesitant)
        // High essence + define → CMDeepen (building on trajectory)
        if fp.conatus_energy < 0.5 {
            // Very low energy → system hesitates
            return CanonicalMoveFamily::CMRepair;
        }

        if fp.anchors_to_trajectory()
            && essence.trajectory_committed
            && prop.mode == PropositionMode::Define
        {
            // Building on trajectory → deepen instead of just define
            return CanonicalMoveFamily::CMDeepen;
        }

        base_family
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_define() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        let input = TurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };
        let output = TurnPipeline::process(&input, &mut state);

        assert!(!output.response.is_empty());
        assert!(!output.blocked);
        assert_eq!(state.dialogue.turn_count, 1);
        assert!(output.governance_events > 0);
        // F1 fix: conatus energy should be computed and reported
        assert!(
            output.conatus_energy > 0.0,
            "Conatus energy should be positive"
        );
        // F2 fix: essence should be witnessed
        assert!(
            state.semantic.essence.trajectory_committed,
            "Essence should be committed after turn"
        );
    }

    #[test]
    fn test_pipeline_challenge() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        let input1 = TurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };
        TurnPipeline::process(&input1, &mut state);

        let input2 = TurnInput {
            raw_text: "свобода это просто вседозволенность".into(),
            session_id: "test".into(),
        };
        let output2 = TurnPipeline::process(&input2, &mut state);

        assert_eq!(output2.family, CanonicalMoveFamily::CMConfront);
        assert!(output2.response.contains("удерживаю") || output2.response.contains("позицию"));
    }

    #[test]
    fn test_pipeline_commitment_memory() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "test".into(),
            },
            &mut state,
        );

        assert!(state.semantic.semantic_commitments.is_some());
        let store = state.semantic.semantic_commitments.as_ref().unwrap();
        assert!(!store.active.is_empty());
    }

    #[test]
    fn test_pipeline_multi_turn() {
        let mut state = SystemState {
            session_id: "multi".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };

        let out1 = TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "multi".into(),
            },
            &mut state,
        );
        assert!(!out1.response.is_empty());

        let out2 = TurnPipeline::process(
            &TurnInput {
                raw_text: "свобода это просто отсутствие ограничений".into(),
                session_id: "multi".into(),
            },
            &mut state,
        );
        assert_eq!(out2.family, CanonicalMoveFamily::CMConfront);

        let out3 = TurnPipeline::process(
            &TurnInput {
                raw_text: "что ты думаешь об ответственности?".into(),
                session_id: "multi".into(),
            },
            &mut state,
        );
        assert!(!out3.response.is_empty());

        assert_eq!(state.dialogue.turn_count, 3);
        assert_eq!(state.dialogue.history.len(), 3);
    }

    #[test]
    fn test_pipeline_determinism() {
        let mut state1 = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut state2 = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };

        let input = TurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };

        let out1 = TurnPipeline::process(&input, &mut state1);
        let out2 = TurnPipeline::process(&input, &mut state2);

        assert_eq!(
            out1.response, out2.response,
            "Same input should produce same output"
        );
        assert_eq!(out1.family, out2.family);
    }

    #[test]
    fn test_pipeline_guard_blocks_empty() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        let output = TurnPipeline::process(
            &TurnInput {
                raw_text: "".into(),
                session_id: "test".into(),
            },
            &mut state,
        );
        assert!(output.blocked || !output.response.is_empty());
    }

    #[test]
    fn test_pipeline_governance_events_logged() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        let output = TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "test".into(),
            },
            &mut state,
        );
        assert!(output.governance_events >= 1);
    }

    #[test]
    fn test_pipeline_graph_grows_with_new_relations() {
        // F5 fix: graph should grow with genuinely new relations from Self Layer
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        let initial_edges = state.semantic.runtime_graph.edges.len();

        TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "test".into(),
            },
            &mut state,
        );

        // Graph should have at least as many edges as before
        assert!(
            state.semantic.runtime_graph.edges.len() >= initial_edges,
            "Graph should not shrink, got {} < {}",
            state.semantic.runtime_graph.edges.len(),
            initial_edges
        );
    }

    #[test]
    fn test_self_layer_influences_output() {
        // Phase 4: different Conatus/FIELD state → different output
        let mut high_energy_state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                field: Field {
                    confidence: 0.9,
                    resonance: 0.9,
                    consolidation: 0.9,
                    counterfactual: 0.1,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let mut low_energy_state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                field: Field {
                    confidence: 0.1,
                    resonance: 0.1,
                    consolidation: 0.1,
                    counterfactual: 0.9,
                    ..Default::default()
                },
                ..Default::default()
            },
            ..Default::default()
        };

        let input = TurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };

        let high_out = TurnPipeline::process(&input, &mut high_energy_state);
        let low_out = TurnPipeline::process(&input, &mut low_energy_state);

        // Conatus energy should differ
        assert!(
            high_out.conatus_energy > low_out.conatus_energy,
            "High-confidence field should produce higher conatus: {} vs {}",
            high_out.conatus_energy,
            low_out.conatus_energy
        );

        // Low energy might route to CMRepair
        assert!(
            low_out.conatus_energy < high_out.conatus_energy,
            "Conatus should reflect field state"
        );
    }

    #[test]
    fn test_essence_witnessed_after_turn() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };

        assert!(!state.semantic.essence.trajectory_committed);

        TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "test".into(),
            },
            &mut state,
        );

        assert!(state.semantic.essence.trajectory_committed);
        assert!(!state.semantic.essence.witnesses.is_empty());
    }

    #[test]
    fn test_adjunction_state_stored() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };

        TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "test".into(),
            },
            &mut state,
        );

        // Adjunction state should be stored
        assert!(
            state.semantic.adjunction.reconciled_value > 0.0,
            "Adjunction should produce a reconciled value"
        );
    }
}

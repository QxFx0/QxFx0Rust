use qxfx0_commitment::CommitmentOps;
use qxfx0_guard::ContentQualityGate;
use qxfx0_self::{Conatus, SelfBlanket};
use qxfx0_semantic::{
    seed_graph, ContextualComposer, GraphEngagement, PropositionMode, PropositionParser,
};
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
}

impl TurnPipeline {
    /// Process a single turn through all 6 stages.
    pub fn process(input: &TurnInput, state: &mut SystemState) -> TurnOutput {
        // ── Stage 1: Prepare ──
        let prop = PropositionParser::parse(&input.raw_text);
        let field = &state.semantic.field;
        let conatus_energy = Conatus::compute(field);

        // Self-blanket check
        let violations = SelfBlanket::check(field, conatus_energy);
        if !violations.is_empty() {
            tracing::warn!("Self-blanket violations: {:?}", violations);
        }

        // ── Stage 2: Route ──
        let family = Self::route_family(&prop);
        let fp = FieldProfile {
            confidence: field.confidence,
            counterfactual: field.counterfactual,
            consolidation: field.consolidation,
            resonance: field.resonance,
        };

        // ── Stage 3: Render ──
        let graph = if state.semantic.runtime_graph.edges.is_empty() {
            seed_graph()
        } else {
            state.semantic.runtime_graph.clone()
        };

        let engagement = GraphEngagement::engage(&graph, &prop);
        let surface = ContextualComposer::compose(&graph, &fp, &prop, &engagement);

        let mut response = surface.text;

        // Add commitment reference if available
        if let Some(ref store) = state.semantic.semantic_commitments {
            let prior = CommitmentOps::retrieve(&prop.subject, store);
            if let Some(first) = prior.first() {
                let prefix = match prop.mode {
                    PropositionMode::Challenge => {
                        format!("Я удерживаю позицию: {}. ", first.statement)
                    }
                    PropositionMode::Define | PropositionMode::Reflect => {
                        format!("Я ранее полагал, что {}. ", first.statement)
                    }
                    _ => String::new(),
                };
                if !prefix.is_empty() && !response.is_empty() {
                    response = format!("{}{}", prefix, response);
                }
            }
        }

        // Add authority prefix for Define mode
        if prop.mode == PropositionMode::Define && !response.is_empty() {
            response = format!("Известно, что {}", response);
        }

        // ── Stage 4: Finalize ──
        state.dialogue.turn_count += 1;
        state.dialogue.last_family = family;
        state.dialogue.last_topic = prop.subject.clone();
        state.dialogue.history.push(response.clone());

        // Update commitment store
        let turn_seq = state.dialogue.turn_count;
        let mut new_commitments = false;
        if !response.is_empty() {
            let payload = FactualClaimPayload {
                statement: response.clone(),
                confidence: 0.5,
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

        // ── Graph Enrichment (F1 fix) ──
        // Write engagement-derived relations back to the runtime graph
        if new_commitments && !engagement.supporting.is_empty() {
            for rel in &engagement.supporting {
                // Only add if not already present (dedup by ru_original)
                let exists = state
                    .semantic
                    .runtime_graph
                    .edges
                    .iter()
                    .any(|e| e.ru_original == rel.ru_original);
                if !exists {
                    state.semantic.runtime_graph.add_relation(rel.clone());
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

        // Update response if blocked
        if blocked {
            if let Some(last) = state.dialogue.history.last_mut() {
                *last = final_text.clone();
            }
        }

        // ── Stage 6: Persist (caller's responsibility) ──
        // State is mutated in-place; caller should save to persistence.

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
        // Check graph integrity: no self-loops
        for edge in &state.semantic.runtime_graph.edges {
            if edge.from == edge.to {
                violations.push(format!("self_loop: {}", edge.ru_original));
                break;
            }
        }

        violations
    }

    /// Route to a canonical move family based on proposition mode.
    fn route_family(prop: &qxfx0_semantic::ParsedProposition) -> CanonicalMoveFamily {
        match prop.mode {
            PropositionMode::Define => CanonicalMoveFamily::CMDefine,
            PropositionMode::Challenge => CanonicalMoveFamily::CMConfront,
            PropositionMode::Connect => CanonicalMoveFamily::CMDistinguish,
            PropositionMode::Reflect => CanonicalMoveFamily::CMReflect,
            PropositionMode::Assert => CanonicalMoveFamily::CMGround,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pipeline_define() {
        let mut state = SystemState::default();
        let input = TurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };
        let output = TurnPipeline::process(&input, &mut state);

        assert!(!output.response.is_empty());
        assert_eq!(output.family, CanonicalMoveFamily::CMDefine);
        assert!(!output.blocked);
        assert_eq!(state.dialogue.turn_count, 1);
    }

    #[test]
    fn test_pipeline_challenge() {
        let mut state = SystemState::default();
        // First turn: define
        let input1 = TurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };
        TurnPipeline::process(&input1, &mut state);

        // Second turn: challenge
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
        let mut state = SystemState::default();

        // Turn 1: define свобода
        TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "test".into(),
            },
            &mut state,
        );

        // Verify commitment store was updated
        assert!(state.semantic.semantic_commitments.is_some());
        let store = state.semantic.semantic_commitments.as_ref().unwrap();
        assert!(
            !store.active.is_empty(),
            "Should have active commitments after turn 1"
        );
    }

    #[test]
    fn test_pipeline_multi_turn() {
        let mut state = SystemState::default();

        // Turn 1
        let out1 = TurnPipeline::process(
            &TurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "multi".into(),
            },
            &mut state,
        );
        assert!(!out1.response.is_empty());

        // Turn 2 — challenge
        let out2 = TurnPipeline::process(
            &TurnInput {
                raw_text: "свобода это просто отсутствие ограничений".into(),
                session_id: "multi".into(),
            },
            &mut state,
        );
        assert_eq!(out2.family, CanonicalMoveFamily::CMConfront);

        // Turn 3 — reflect
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
        // Same input + same state → same output
        let mut state1 = SystemState::default();
        let mut state2 = SystemState::default();

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
        let mut state = SystemState::default();
        // Empty input should be blocked
        let output = TurnPipeline::process(
            &TurnInput {
                raw_text: "".into(),
                session_id: "test".into(),
            },
            &mut state,
        );
        // Empty input → topic "неизвестный" → graph has no relations → empty response → blocked
        assert!(output.blocked || output.response.contains("не нахожу"));
    }
}

use qxfx0_semantic::{seed_graph, ConjugateComposer, SenseDecomposer};
use qxfx0_types::atom::{AtomGraph, SenseVector};
use qxfx0_types::system_state::*;

/// Vector-based turn pipeline: input -> SenseDecomposer -> ConjugateComposer -> output.
///
/// Responses are deterministic: same graph + same input → same output.
pub struct VectorTurnPipeline;

/// Vector turn input.
#[derive(Debug, Clone)]
pub struct VectorTurnInput {
    pub raw_text: String,
    pub session_id: String,
}

/// Vector turn output — conjugate response + metadata.
#[derive(Debug, Clone)]
pub struct VectorTurnOutput {
    pub response: String,
    pub sense_vectors: Vec<SenseVector>,
    pub resonance: f64,
    pub depth: usize,
    pub blocked: bool,
}

impl VectorTurnPipeline {
    /// Process a turn through the vector pipeline.
    /// Decomposes input into sense vectors, then composes a conjugate response.
    pub fn process(input: &VectorTurnInput, state: &mut SystemState) -> VectorTurnOutput {
        if state.session_id.is_empty() {
            state.session_id = input.session_id.clone();
        }
        let session_matches = state.session_id == input.session_id;
        let graph = if state.semantic.runtime_graph.edges.is_empty() {
            seed_graph()
        } else {
            state.semantic.runtime_graph.clone()
        };

        // Stage 1: Decompose input into sense vectors
        let sense_vectors = SenseDecomposer::decompose(&input.raw_text, &graph);

        // Detect challenge mode via centralized detection
        let is_challenge = crate::detect_challenge(&input.raw_text);

        // Stage 2: Compose conjugate response through vector algebra
        let surface = if is_challenge {
            ConjugateComposer::compose_with_challenge(&graph, &sense_vectors, true)
        } else {
            ConjugateComposer::compose(&graph, &sense_vectors)
        };

        // Stage 3: Finalize — update state
        state.dialogue.turn_count += 1;
        state.dialogue.last_topic = sense_vectors
            .first()
            .map(|v| v.atom_id.as_str().to_string());
        state.dialogue.history.push(surface.text.clone());

        // Stage 4: Guard — check for empty response
        let blocked = surface.text.is_empty() || !session_matches;

        VectorTurnOutput {
            response: surface.text,
            sense_vectors,
            resonance: surface.depth_score,
            depth: surface.paths.first().map(|p| p.edges.len()).unwrap_or(0),
            blocked,
        }
    }

    /// Process a turn with a custom graph (for testing).
    pub fn process_with_graph(input: &VectorTurnInput, graph: &AtomGraph) -> VectorTurnOutput {
        let sense_vectors = SenseDecomposer::decompose(&input.raw_text, graph);
        let surface = ConjugateComposer::compose(graph, &sense_vectors);
        let blocked = surface.text.is_empty();
        VectorTurnOutput {
            response: surface.text,
            sense_vectors,
            resonance: surface.depth_score,
            depth: surface.paths.first().map(|p| p.edges.len()).unwrap_or(0),
            blocked,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_vector_pipeline_basic() {
        let graph = seed_graph();
        let input = VectorTurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };
        let output = VectorTurnPipeline::process_with_graph(&input, &graph);
        assert!(!output.response.is_empty());
        assert!(!output.sense_vectors.is_empty());
        assert!(output.resonance >= 0.0);
        assert!(output.depth > 0);
        assert!(!output.blocked);
        // Deterministic output — no canned openings
        assert!(!output.response.contains("Когда я думаю о"));
        assert!(!output.response.contains("Я вижу это так:"));
    }

    #[test]
    fn test_vector_pipeline_challenge() {
        let mut state = SystemState::default();
        let input = VectorTurnInput {
            raw_text: "свобода это просто вседозволенность".into(),
            session_id: "test".into(),
        };
        let output = VectorTurnPipeline::process(&input, &mut state);
        assert!(!output.response.is_empty());
        // Challenge mode should include graph-derived defense content
        assert!(output.response.contains("свобода") || output.response.contains("предполагает"));
    }

    #[test]
    fn test_vector_pipeline_unique_outputs() {
        let graph = seed_graph();
        let input1 = VectorTurnInput {
            raw_text: "свобода".into(),
            session_id: "test".into(),
        };
        let input2 = VectorTurnInput {
            raw_text: "истина".into(),
            session_id: "test".into(),
        };
        let out1 = VectorTurnPipeline::process_with_graph(&input1, &graph);
        let out2 = VectorTurnPipeline::process_with_graph(&input2, &graph);
        assert_ne!(out1.response, out2.response);
    }

    #[test]
    fn test_vector_pipeline_no_template_phrases() {
        let graph = seed_graph();
        let input = VectorTurnInput {
            raw_text: "что ты думаешь об истине?".into(),
            session_id: "test".into(),
        };
        let output = VectorTurnPipeline::process_with_graph(&input, &graph);
        if !output.response.is_empty() {
            assert!(!output.response.contains("Когда я думаю о"));
            assert!(!output.response.contains("Я вижу это так:"));
            assert!(!output.response.contains("Я вижу это иначе"));
            assert!(!output.response.contains("Связь прослеживается:"));
        }
    }

    #[test]
    fn test_vector_pipeline_empty_input() {
        let graph = seed_graph();
        let input = VectorTurnInput {
            raw_text: "".into(),
            session_id: "test".into(),
        };
        let output = VectorTurnPipeline::process_with_graph(&input, &graph);
        // Empty input produces empty or minimal response
        assert!(output.response.is_empty() || output.sense_vectors.is_empty());
    }

    #[test]
    fn test_vector_pipeline_state_update() {
        let mut state = SystemState {
            session_id: "test".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };
        let input = VectorTurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };
        let output = VectorTurnPipeline::process(&input, &mut state);
        assert!(!output.response.is_empty());
        assert_eq!(state.dialogue.turn_count, 1);
        assert_eq!(state.dialogue.history.len(), 1);
        assert!(state.dialogue.last_topic.is_some());
    }

    #[test]
    fn test_vector_pipeline_multi_turn() {
        let mut state = SystemState {
            session_id: "multi".into(),
            semantic: qxfx0_types::system_state::SemanticState {
                runtime_graph: seed_graph(),
                ..Default::default()
            },
            ..Default::default()
        };

        let out1 = VectorTurnPipeline::process(
            &VectorTurnInput {
                raw_text: "что такое свобода?".into(),
                session_id: "multi".into(),
            },
            &mut state,
        );
        assert!(!out1.response.is_empty());

        let out2 = VectorTurnPipeline::process(
            &VectorTurnInput {
                raw_text: "свобода это просто вседозволенность".into(),
                session_id: "multi".into(),
            },
            &mut state,
        );
        assert!(!out2.response.is_empty());

        let out3 = VectorTurnPipeline::process(
            &VectorTurnInput {
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
    fn test_vector_pipeline_determinism() {
        let graph = seed_graph();
        let input = VectorTurnInput {
            raw_text: "что такое свобода?".into(),
            session_id: "test".into(),
        };
        let out1 = VectorTurnPipeline::process_with_graph(&input, &graph);
        let out2 = VectorTurnPipeline::process_with_graph(&input, &graph);
        assert_eq!(out1.response, out2.response);
    }

    #[test]
    fn test_vector_pipeline_sense_vectors_not_empty() {
        let graph = seed_graph();
        let input = VectorTurnInput {
            raw_text: "свобода и ответственность".into(),
            session_id: "test".into(),
        };
        let output = VectorTurnPipeline::process_with_graph(&input, &graph);
        assert!(!output.sense_vectors.is_empty());
        // Should find at least one known atom
        assert!(output.sense_vectors.iter().any(|v| {
            v.atom_id.as_str() == "свобода" || v.atom_id.as_str() == "ответственность"
        }));
    }
}

use qxfx0_semantic::{ConjugateComposer, SenseDecomposer};
use qxfx0_types::atom::AtomGraph;

/// ConjugatePipeline — vector-based turn processing.
///
/// Replaces the template-based ContextualComposer path with vector algebra:
/// input -> SenseDecomposer -> ConjugateComposer -> output.
/// No template phrases. Each response is unique because the vector path is unique.
pub struct ConjugatePipeline;

impl ConjugatePipeline {
    /// Process input text through vector decomposition and conjugate composition.
    /// Returns the generated response text.
    pub fn process(input: &str, graph: &AtomGraph) -> String {
        let sense_vectors = SenseDecomposer::decompose(input, graph);
        if sense_vectors.is_empty() {
            return String::new();
        }
        let is_challenge = Self::detect_challenge(input);
        let surface = ConjugateComposer::compose_with_challenge(graph, &sense_vectors, is_challenge);
        surface.text
    }

    /// Detect if the input is a challenge (reduction, disagreement, contradiction).
    fn detect_challenge(input: &str) -> bool {
        crate::detect_challenge(input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use qxfx0_semantic::seed_graph;

    #[test]
    fn test_conjugate_pipeline_basic() {
        let graph = seed_graph();
        let response = ConjugatePipeline::process("что такое свобода", &graph);
        assert!(!response.is_empty());
        assert!(!response.contains("Когда я думаю о"));
        assert!(!response.contains("Я вижу это так:"));
    }

    #[test]
    fn test_conjugate_pipeline_challenge() {
        let graph = seed_graph();
        let response = ConjugatePipeline::process("свобода это просто вседозволенность", &graph);
        assert!(!response.is_empty());
        assert!(response.contains("свобода") || response.contains("предполагает"));
    }

    #[test]
    fn test_conjugate_pipeline_empty() {
        let graph = seed_graph();
        let response = ConjugatePipeline::process("", &graph);
        assert!(response.is_empty());
    }

    #[test]
    fn test_conjugate_pipeline_unique() {
        let graph = seed_graph();
        let r1 = ConjugatePipeline::process("свобода", &graph);
        let r2 = ConjugatePipeline::process("истина", &graph);
        assert_ne!(r1, r2);
    }

    #[test]
    fn test_conjugate_pipeline_no_templates() {
        let graph = seed_graph();
        let response = ConjugatePipeline::process("что ты думаешь об ответственности", &graph);
        if !response.is_empty() {
            assert!(!response.contains("Когда я думаю о"));
            assert!(!response.contains("Я вижу это так:"));
            assert!(!response.contains("Я вижу это иначе"));
            assert!(!response.contains("Связь прослеживается:"));
        }
    }
}

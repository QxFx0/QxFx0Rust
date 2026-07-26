pub mod agent_bridge;
pub mod batch_loader;
pub mod code_renderer;
pub mod code_schema;
pub mod full_registry;
pub mod intent_parser;
pub mod registry;
pub mod type_edges;
pub mod type_info;

pub use agent_bridge::{FunctionLookup, FunctionResult, RegistryStats};
pub use batch_loader::{convert_atom, convert_relation, AtomInput};
pub use code_renderer::CodeRenderer;
pub use code_schema::{
    CallChain, CodeAtom, CodeAtomId, CodeAtomKind, CodeGraph, CodeLang, CodeRelation,
    CodeRelationType, TypedParam,
};
pub use full_registry::build_full_registry;
pub use intent_parser::{Intent, IntentModifier, IntentObject, IntentParser, IntentVerb};
pub use registry::build_rust_registry;
pub use type_edges::TypeEdgeBuilder;
pub use type_info::{types_compose, ComposeResult, PrimitiveType, TypeInfo};

use thiserror::Error;

#[derive(Error, Debug)]
pub enum CodeOrchError {
    #[error("no functions found for intent: {0}")]
    NoMatch(String),
    #[error("type mismatch: {producer} -> {consumer}")]
    TypeMismatch { producer: String, consumer: String },
}

/// The orchestrator — top-level API for code function selection.
///
/// Pipeline: Intent → TagMatch → TypeDirectedSearch → ChainComposition → Render
pub struct CodeOrchestrator {
    graph: CodeGraph,
}

impl CodeOrchestrator {
    pub fn new(graph: CodeGraph) -> Self {
        CodeOrchestrator { graph }
    }

    pub fn graph(&self) -> &CodeGraph {
        &self.graph
    }

    pub fn graph_mut(&mut self) -> &mut CodeGraph {
        &mut self.graph
    }

    /// Orchestrate: parse intent → find functions → compose chains → render
    pub fn orchestrate(&self, input: &str) -> Result<OrchestrationResult, CodeOrchError> {
        let intent = IntentParser::parse(input);
        let matches = IntentParser::match_atoms(&intent, &self.graph);

        if matches.is_empty() {
            return Err(CodeOrchError::NoMatch(intent.raw));
        }

        let mut chains = Vec::new();
        for (atom, score) in matches.iter().take(5) {
            let single = CallChain::single(atom.id.clone(), atom.lang);
            chains.push(OrchestratedChain {
                chain: single,
                tag_score: *score,
                type_score: 1.0,
            });

            let composed = TypeEdgeBuilder::compose_chain(&self.graph, &atom.id, 4);
            for c in composed.iter().take(3) {
                if c.steps.len() > 1 {
                    let ts = TypeEdgeBuilder::score_chain(&self.graph, c);
                    chains.push(OrchestratedChain {
                        chain: c.clone(),
                        tag_score: *score,
                        type_score: ts,
                    });
                }
            }
        }

        chains.sort_by(|a, b| {
            let a_total = a.tag_score * a.type_score;
            let b_total = b.tag_score * b.type_score;
            b_total.total_cmp(&a_total)
        });

        if chains.is_empty() {
            return Err(CodeOrchError::NoMatch(intent.raw));
        }

        let best = chains
            .first()
            .ok_or_else(|| CodeOrchError::NoMatch(intent.raw.clone()))?;
        let rendered = CodeRenderer::render_chain(&self.graph, &best.chain);

        let alternatives: Vec<String> = chains
            .iter()
            .skip(1)
            .take(4)
            .map(|c| {
                let first = c
                    .chain
                    .steps
                    .first()
                    .and_then(|id| self.graph.atoms.get(id));
                first.map(CodeRenderer::render_summary).unwrap_or_default()
            })
            .collect();

        Ok(OrchestrationResult {
            intent,
            rendered,
            alternatives,
            chain_count: chains.len(),
        })
    }

    /// Register a function atom + its relations
    pub fn register(&mut self, atom: CodeAtom, relations: Vec<CodeRelation>) {
        self.graph.add_atom(atom);
        for rel in relations {
            self.graph.add_relation(rel);
        }
    }

    /// Build type-directed edges for all registered atoms
    pub fn build_type_edges(&mut self) {
        TypeEdgeBuilder::build_type_edges(&mut self.graph);
    }
}

/// A candidate call chain with its associated scoring metrics.
#[derive(Debug, Clone)]
pub struct OrchestratedChain {
    pub chain: CallChain,
    pub tag_score: f64,
    pub type_score: f64,
}

/// The final result of the orchestration process, containing the primary
/// rendered code and potential alternatives.
#[derive(Debug, Clone)]
pub struct OrchestrationResult {
    pub intent: Intent,
    pub rendered: String,
    pub alternatives: Vec<String>,
    pub chain_count: usize,
}

impl OrchestrationResult {
    pub fn primary(&self) -> &str {
        &self.rendered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_atom(
        id: &str,
        name: &str,
        sig: &str,
        doc: &str,
        tags: Vec<&str>,
        params: Vec<TypedParam>,
        return_type: Option<TypeInfo>,
    ) -> CodeAtom {
        CodeAtom {
            id: CodeAtomId::new(id),
            lang: CodeLang::Rust,
            kind: CodeAtomKind::Function,
            name: name.into(),
            module: "test".into(),
            signature: sig.into(),
            doc: doc.into(),
            tags: tags.into_iter().map(String::from).collect(),
            params,
            return_type,
            complexity: None,
            requires_alloc: false,
            panics: false,
            async_fn: false,
        }
    }

    fn make_param(name: &str, ty: TypeInfo) -> TypedParam {
        TypedParam {
            name: name.into(),
            ty,
            is_self: false,
            is_mut: false,
        }
    }

    fn demo_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_atom(make_atom(
            "rust::Vec::sort_unstable",
            "Vec::sort_unstable",
            "pub fn sort_unstable(&mut self) where T: Ord",
            "Sorts in-place, unstable, O(n log n)",
            vec!["sort", "in-place", "unstable", "O(n log n)"],
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(TypeInfo::unit()),
        ));
        g.add_atom(make_atom(
            "rust::Vec::max",
            "Vec::max",
            "pub fn max(&self) -> Option<T> where T: Ord",
            "Returns the maximum element",
            vec!["max", "find", "Vec", "search"],
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(TypeInfo::option(TypeInfo::type_param("T"))),
        ));
        g.add_atom(make_atom(
            "rust::Vec::last",
            "Vec::last",
            "pub fn last(&self) -> Option<&T>",
            "Returns the last element",
            vec!["last", "find", "Vec"],
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(TypeInfo::option(TypeInfo::type_param("T"))),
        ));
        g.add_atom(make_atom(
            "rust::Vec::iter",
            "Vec::iter",
            "pub fn iter(&self) -> Iter<'_, T>",
            "Returns iterator over elements",
            vec!["iter", "loop", "Vec", "lazy"],
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(TypeInfo::Generic {
                base: "Iterator".into(),
                args: vec![TypeInfo::type_param("T")],
            }),
        ));
        g.add_atom(make_atom(
            "rust::Iterator::collect",
            "Iterator::collect",
            "pub fn collect<B: FromIterator<Self::Item>>(self) -> B",
            "Collect iterator into a collection type",
            vec!["collect", "eager", "materialize", "Vec"],
            vec![make_param(
                "self",
                TypeInfo::Generic {
                    base: "Iterator".into(),
                    args: vec![TypeInfo::type_param("T")],
                },
            )],
            Some(TypeInfo::vec(TypeInfo::type_param("T"))),
        ));
        g
    }

    #[test]
    fn test_orchestrate_find_max() {
        let g = demo_graph();
        let orch = CodeOrchestrator::new(g);
        let result = orch.orchestrate("найти максимум в массиве").unwrap();
        assert!(!result.rendered.is_empty());
        assert!(result.chain_count > 0);
    }

    #[test]
    fn test_orchestrate_sort() {
        let g = demo_graph();
        let orch = CodeOrchestrator::new(g);
        let result = orch.orchestrate("отсортировать массив").unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_orchestrate_no_match() {
        let g = demo_graph();
        let orch = CodeOrchestrator::new(g);
        let result = orch.orchestrate("сделать кофе");
        assert!(result.is_err());
    }

    #[test]
    fn test_orchestrate_has_alternatives() {
        let g = demo_graph();
        let orch = CodeOrchestrator::new(g);
        let result = orch.orchestrate("найти максимум в массиве").unwrap();
        assert!(!result.alternatives.is_empty() || result.chain_count > 1);
    }

    #[test]
    fn test_build_type_edges_and_orchestrate() {
        let mut g = demo_graph();
        TypeEdgeBuilder::build_type_edges(&mut g);
        let orch = CodeOrchestrator::new(g);
        let result = orch.orchestrate("найти максимум в массиве").unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_orchestrate_iter_collect_chain() {
        let mut g = demo_graph();
        TypeEdgeBuilder::build_type_edges(&mut g);
        let orch = CodeOrchestrator::new(g);
        let result = orch.orchestrate("собрать элементы в массив").unwrap();
        assert!(!result.rendered.is_empty());
        assert!(
            result.rendered.contains("iter"),
            "should contain iter, got: {}",
            result.rendered
        );
        assert!(result.chain_count > 0);
    }

    #[test]
    fn test_orchestrate_iter_max_chain() {
        let mut g = demo_graph();
        TypeEdgeBuilder::build_type_edges(&mut g);
        let orch = CodeOrchestrator::new(g);
        let result = orch.orchestrate("найти максимум в массиве").unwrap();
        assert!(!result.rendered.is_empty());
        let n = result.rendered.lines().count();
        assert!(
            n >= 2,
            "chain should have >=2 lines, got {}: {}",
            n,
            result.rendered
        );
    }
}

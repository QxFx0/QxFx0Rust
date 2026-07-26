//! Agent2048 integration bridge — provides CodeOrchestrator as a function lookup
//! service for the Writer agent.
//!
//! Usage from Agent2048:
//! ```no_run
//! use qxfx0_code::agent_bridge::FunctionLookup;
//! let lookup = FunctionLookup::new();
//! let result = lookup.find("sort array in rust");
//! ```

use crate::code_schema::CodeGraph;
use crate::{CodeOrchestrator, OrchestrationResult};

/// Bridge between QxFx0 CodeOrchestrator and Agent2048 Writer agent.
/// Provides a simple lookup API that the Writer can call before writing code
/// to find the right function instead of hallucinating.
pub struct FunctionLookup {
    orch: CodeOrchestrator,
}

impl FunctionLookup {
    /// Create a new lookup with the production typed Rust registry loaded.
    pub fn new() -> Self {
        let graph = crate::build_full_registry();
        FunctionLookup {
            orch: CodeOrchestrator::new(graph),
        }
    }

    /// Create with a custom graph (for testing or partial registries).
    pub fn with_graph(graph: CodeGraph) -> Self {
        FunctionLookup {
            orch: CodeOrchestrator::new(graph),
        }
    }

    /// Find functions matching a natural language description.
    /// Returns the primary match + alternatives.
    pub fn find(&self, query: &str) -> Option<FunctionResult> {
        match self.orch.orchestrate(query) {
            Ok(result) => Some(FunctionResult::from(result)),
            Err(_) => None,
        }
    }

    /// Find functions, returning only the primary signature.
    pub fn find_signature(&self, query: &str) -> Option<String> {
        self.find(query).map(|r| r.signature)
    }

    /// Find functions, returning the primary match + call chain.
    pub fn find_chain(&self, query: &str) -> Option<String> {
        self.find(query).map(|r| r.rendered)
    }

    /// Get registry statistics for diagnostics.
    pub fn stats(&self) -> RegistryStats {
        let graph = self.orch.graph();
        let rust = graph
            .atoms
            .values()
            .filter(|a| a.lang == crate::CodeLang::Rust)
            .count();
        let python = graph
            .atoms
            .values()
            .filter(|a| a.lang == crate::CodeLang::Python)
            .count();
        let haskell = graph
            .atoms
            .values()
            .filter(|a| a.lang == crate::CodeLang::Haskell)
            .count();
        let typescript = graph
            .atoms
            .values()
            .filter(|a| a.lang == crate::CodeLang::TypeScript)
            .count();
        RegistryStats {
            total_atoms: graph.atoms.len(),
            total_relations: graph.edges.len(),
            rust_atoms: rust,
            python_atoms: python,
            haskell_atoms: haskell,
            typescript_atoms: typescript,
        }
    }
}

impl Default for FunctionLookup {
    fn default() -> Self {
        Self::new()
    }
}

/// Result of a function lookup — simplified for Agent2048 consumption.
#[derive(Debug, Clone)]
pub struct FunctionResult {
    /// Primary function signature (e.g., "pub fn max(&self) -> Option<T> where T: Ord")
    pub signature: String,
    /// Full rendered output including call chain
    pub rendered: String,
    /// Alternative function names
    pub alternatives: Vec<String>,
    /// Number of composition chains found
    pub chain_count: usize,
    /// The parsed intent (verb + object)
    pub intent_verb: String,
    pub intent_object: String,
}

impl From<OrchestrationResult> for FunctionResult {
    fn from(r: OrchestrationResult) -> Self {
        // Extract primary signature from rendered output
        let signature = r
            .rendered
            .lines()
            .find(|l| {
                !l.starts_with("//") && !l.starts_with("--") && !l.starts_with("#") && !l.is_empty()
            })
            .unwrap_or(&r.rendered)
            .trim()
            .to_string();

        FunctionResult {
            signature,
            rendered: r.rendered,
            alternatives: r.alternatives,
            chain_count: r.chain_count,
            intent_verb: r.intent.verb.as_str().to_string(),
            intent_object: r.intent.object.as_str().to_string(),
        }
    }
}

/// Registry statistics for diagnostics.
#[derive(Debug, Clone)]
pub struct RegistryStats {
    pub total_atoms: usize,
    pub total_relations: usize,
    pub rust_atoms: usize,
    pub python_atoms: usize,
    pub haskell_atoms: usize,
    pub typescript_atoms: usize,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_schema::*;
    use crate::type_edges::TypeEdgeBuilder;
    use crate::TypeInfo;

    fn make_atom(
        id: &str,
        lang: CodeLang,
        name: &str,
        params: Vec<TypedParam>,
        return_type: Option<TypeInfo>,
        tags: Vec<&str>,
    ) -> CodeAtom {
        CodeAtom {
            id: CodeAtomId::new(id),
            lang,
            kind: CodeAtomKind::Function,
            name: name.into(),
            module: "test".into(),
            signature: format!("fn {}", name),
            doc: "test".into(),
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

    fn iter_collect_graph() -> CodeGraph {
        let mut g = CodeGraph::new();
        g.add_atom(make_atom(
            "rust::Vec::iter",
            CodeLang::Rust,
            "Vec::iter",
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(TypeInfo::Generic {
                base: "Iterator".into(),
                args: vec![TypeInfo::type_param("T")],
            }),
            vec!["iter", "loop", "lazy"],
        ));
        g.add_atom(make_atom(
            "rust::Iterator::collect",
            CodeLang::Rust,
            "Iterator::collect",
            vec![make_param(
                "self",
                TypeInfo::Generic {
                    base: "Iterator".into(),
                    args: vec![TypeInfo::type_param("T")],
                },
            )],
            Some(TypeInfo::vec(TypeInfo::type_param("T"))),
            vec!["collect", "eager", "materialize"],
        ));
        g.add_atom(make_atom(
            "rust::Iterator::max",
            CodeLang::Rust,
            "Iterator::max",
            vec![make_param(
                "self",
                TypeInfo::Generic {
                    base: "Iterator".into(),
                    args: vec![TypeInfo::type_param("T")],
                },
            )],
            Some(TypeInfo::option(TypeInfo::type_param("T"))),
            vec!["max", "find", "search"],
        ));
        TypeEdgeBuilder::build_type_edges(&mut g);
        g
    }

    #[test]
    fn test_function_lookup_new() {
        let lookup = FunctionLookup::new();
        let stats = lookup.stats();
        assert!(stats.total_atoms >= 80);
        assert!(stats.total_relations > 0);
        assert_eq!(stats.total_atoms, stats.rust_atoms);
    }

    #[test]
    fn test_find_max() {
        let lookup = FunctionLookup::new();
        let result = lookup.find("найти максимум в массиве").unwrap();
        assert!(!result.signature.is_empty());
        assert_eq!(result.intent_verb, "find");
    }

    #[test]
    fn test_find_signature() {
        let lookup = FunctionLookup::new();
        let sig = lookup.find_signature("sort array").unwrap();
        assert!(!sig.is_empty());
    }

    #[test]
    fn test_find_not_found() {
        let lookup = FunctionLookup::new();
        assert!(lookup.find("qqqqqqqq zzzzzzzz").is_none());
    }

    #[test]
    fn test_find_cross_lang() {
        let lookup = FunctionLookup::new();
        let result = lookup.find("how to handle errors in Rust").unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_collect_chain_through_bridge() {
        let graph = iter_collect_graph();
        let lookup = FunctionLookup::with_graph(graph);
        let result = lookup.find("collect iterator into vec").unwrap();
        assert!(!result.rendered.is_empty());
        assert!(result.chain_count > 0);
        assert!(result.rendered.contains("iter") || result.rendered.contains("collect"));
    }

    #[test]
    fn test_iter_max_chain_through_bridge() {
        let graph = iter_collect_graph();
        let lookup = FunctionLookup::with_graph(graph);
        let result = lookup.find("find max in array").unwrap();
        assert!(!result.rendered.is_empty());
        assert!(result.chain_count > 0);
    }

    #[test]
    fn test_chain_alternative_fn() {
        let lookup = FunctionLookup::new();
        let result = lookup.find("sort list").unwrap();
        assert!(
            !result.alternatives.is_empty() || result.chain_count > 1,
            "should have alternatives for sort, chains={}",
            result.chain_count
        );
    }

    #[test]
    fn test_find_chain_api() {
        let graph = iter_collect_graph();
        let lookup = FunctionLookup::with_graph(graph);
        let chain_rendered = lookup.find_chain("collect iterator").unwrap();
        assert!(!chain_rendered.is_empty());
    }
}

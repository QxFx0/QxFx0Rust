use crate::code_schema::{CallChain, CodeAtom, CodeAtomId, CodeGraph, CodeLang, CodeRelationType};
use crate::type_info::{types_compose, ComposeResult};
use std::collections::BTreeSet;

/// Type-directed edge construction.
/// This is the core innovation: instead of text-based spreading activation,
/// we match OUTPUT types to INPUT types to find composable function chains.
pub struct TypeEdgeBuilder;

impl TypeEdgeBuilder {
    /// Find all functions whose return type can feed into `consumer`'s first param.
    /// Returns (producer_atom, compose_result, confidence) tuples.
    pub fn find_producers<'g>(
        graph: &'g CodeGraph,
        consumer: &CodeAtom,
    ) -> Vec<(&'g CodeAtom, ComposeResult, f64)> {
        let mut results = Vec::new();
        let consumer_input = consumer.params.first().map(|p| &p.ty);

        for atom in graph.atoms.values() {
            if atom.id == consumer.id {
                continue;
            }
            if atom.lang != consumer.lang {
                continue;
            }
            if let Some(ret) = &atom.return_type {
                if let Some(input) = consumer_input {
                    let result = types_compose(ret, input);
                    if result.can_compose() {
                        results.push((atom, result, result.confidence()));
                    }
                }
            }
        }
        // Deterministic order: descending confidence, with CodeAtomId as
        // a stable tie-breaker. `confidence()` currently returns a small
        // set of fixed values (0.0–1.0), so ties are common when several
        // producers share a compose mode; AtomId is guaranteed unique and
        // Ord, giving bit-exact identical order across runs/platforms.
        results.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.id.cmp(&b.0.id)));
        results
    }

    /// Find all functions that can consume `producer`'s return type.
    pub fn find_consumers<'g>(
        graph: &'g CodeGraph,
        producer: &CodeAtom,
    ) -> Vec<(&'g CodeAtom, ComposeResult, f64)> {
        let mut results = Vec::new();
        let producer_output = match &producer.return_type {
            Some(t) => t,
            None => return results,
        };

        for atom in graph.atoms.values() {
            if atom.id == producer.id {
                continue;
            }
            if atom.lang != producer.lang {
                continue;
            }
            if let Some(first_param) = atom.params.first() {
                let result = types_compose(producer_output, &first_param.ty);
                if result.can_compose() {
                    results.push((atom, result, result.confidence()));
                }
            }
        }
        // Deterministic order: descending confidence, then CodeAtomId.
        // `confidence()` is a piecewise-constant f64, so identical scores
        // occur whenever multiple consumers match via the same compose
        // mode (e.g. several `GenericMatch` candidates); AtomId breaks
        // those ties stably across runs.
        results.sort_by(|a, b| b.2.total_cmp(&a.2).then_with(|| a.0.id.cmp(&b.0.id)));
        results
    }

    /// Build a call chain: start → ... → end, maximizing type compatibility.
    /// BFS over type-composable edges, bounded by max_depth.
    pub fn compose_chain(
        graph: &CodeGraph,
        start: &CodeAtomId,
        max_depth: usize,
    ) -> Vec<CallChain> {
        use std::collections::VecDeque;

        let mut chains = Vec::new();
        let start_atom = match graph.atoms.get(start) {
            Some(a) => a,
            None => return chains,
        };
        let lang = start_atom.lang;

        let mut queue: VecDeque<(CodeAtomId, Vec<CodeAtomId>, BTreeSet<CodeAtomId>, f64)> =
            VecDeque::new();
        let mut initial_visited = BTreeSet::new();
        initial_visited.insert(start.clone());
        queue.push_back((start.clone(), vec![start.clone()], initial_visited, 1.0));

        while let Some((current_id, path, visited, confidence)) = queue.pop_front() {
            // Every path is a valid chain (even single-step)
            chains.push(CallChain {
                steps: path.clone(),
                lang,
                total_complexity: None,
            });

            if path.len() >= max_depth {
                continue;
            }

            let current = match graph.atoms.get(&current_id) {
                Some(a) => a,
                None => continue,
            };

            let consumers = Self::find_consumers(graph, current);
            if consumers.is_empty() {
                continue;
            }

            for (consumer, _result, conf) in consumers {
                if visited.contains(&consumer.id) {
                    continue;
                }
                let mut new_visited = visited.clone();
                new_visited.insert(consumer.id.clone());
                let mut new_path = path.clone();
                new_path.push(consumer.id.clone());
                let new_conf = confidence * conf;
                queue.push_back((consumer.id.clone(), new_path, new_visited, new_conf));
            }
        }

        // Primary sort key is `steps.len()` (integer, no float comparison
        // here). The secondary tie-breaker on the first step's AtomId keeps
        // chain ordering deterministic when several chains of the same
        // length are produced from the same BFS frontier — Rust's standard
        // sort is otherwise free to permute equal-length chains.
        chains.sort_by(|a, b| {
            b.steps
                .len()
                .cmp(&a.steps.len())
                .then_with(|| a.steps.first().cmp(&b.steps.first()))
        });
        chains
    }

    /// Add type-directed RelComposes edges to the graph for all compatible pairs.
    pub fn build_type_edges(graph: &mut CodeGraph) {
        let atom_ids: Vec<CodeAtomId> = graph.atoms.keys().cloned().collect();

        for id in &atom_ids {
            let consumers: Vec<(CodeAtomId, String)> = {
                let atom = match graph.atoms.get(id) {
                    Some(a) => a,
                    None => continue,
                };
                let lang = atom.lang;
                let cs = Self::find_consumers(graph, atom);
                cs.into_iter()
                    .map(|(c, result, _)| {
                        (
                            c.id.clone(),
                            format!("type-match:{}:lang:{}", result.label(), lang.as_str()),
                        )
                    })
                    .collect()
            };
            for (consumer_id, note) in consumers {
                let lang = graph
                    .atoms
                    .get(id)
                    .map(|a| a.lang)
                    .unwrap_or(CodeLang::Rust);
                graph.add_relation(crate::code_schema::CodeRelation {
                    from: id.clone(),
                    to: consumer_id,
                    rel_type: CodeRelationType::RelComposes,
                    lang,
                    note,
                });
            }
        }
    }

    /// Score a call chain by average type compatibility.
    pub fn score_chain(graph: &CodeGraph, chain: &CallChain) -> f64 {
        if chain.steps.len() <= 1 {
            return 1.0;
        }
        let mut total = 0.0;
        for i in 0..chain.steps.len() - 1 {
            let producer = match graph.atoms.get(&chain.steps[i]) {
                Some(a) => a,
                None => continue,
            };
            let consumer = match graph.atoms.get(&chain.steps[i + 1]) {
                Some(a) => a,
                None => continue,
            };
            if let (Some(ret), Some(input)) = (&producer.return_type, consumer.params.first()) {
                total += types_compose(ret, &input.ty).confidence();
            }
        }
        total / (chain.steps.len() - 1) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_schema::*;
    use crate::type_info::*;

    fn make_atom(
        id: &str,
        lang: CodeLang,
        name: &str,
        params: Vec<TypedParam>,
        return_type: Option<TypeInfo>,
    ) -> CodeAtom {
        CodeAtom {
            id: CodeAtomId::new(id),
            lang,
            kind: CodeAtomKind::Function,
            name: name.into(),
            module: "test".into(),
            signature: format!("fn {}", name),
            doc: "test".into(),
            tags: vec![],
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

    #[test]
    fn test_find_consumers_exact() {
        let mut graph = CodeGraph::new();
        graph.add_atom(make_atom(
            "rust::iter",
            CodeLang::Rust,
            "iter",
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(TypeInfo::Generic {
                base: "Iterator".into(),
                args: vec![TypeInfo::type_param("T")],
            }),
        ));
        graph.add_atom(make_atom(
            "rust::collect",
            CodeLang::Rust,
            "collect",
            vec![make_param(
                "iter",
                TypeInfo::Generic {
                    base: "Iterator".into(),
                    args: vec![TypeInfo::type_param("T")],
                },
            )],
            Some(TypeInfo::vec(TypeInfo::type_param("T"))),
        ));

        let producers = TypeEdgeBuilder::find_consumers(
            &graph,
            graph.atoms.get(&CodeAtomId::new("rust::iter")).unwrap(),
        );
        assert!(producers.iter().any(|(a, _, _)| a.name == "collect"));
    }

    #[test]
    fn test_compose_chain() {
        let mut graph = CodeGraph::new();
        let iter_ret = TypeInfo::Generic {
            base: "Iterator".into(),
            args: vec![TypeInfo::type_param("T")],
        };
        let collect_input = TypeInfo::Generic {
            base: "Iterator".into(),
            args: vec![TypeInfo::type_param("T")],
        };

        // Verify types compose
        let compose_result = types_compose(&iter_ret, &collect_input);
        assert_eq!(
            compose_result,
            ComposeResult::Exact,
            "Iterator<T> should compose with Iterator<T>"
        );

        graph.add_atom(make_atom(
            "rust::iter",
            CodeLang::Rust,
            "iter",
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(iter_ret),
        ));
        graph.add_atom(make_atom(
            "rust::collect",
            CodeLang::Rust,
            "collect",
            vec![make_param("iter", collect_input)],
            Some(TypeInfo::vec(TypeInfo::type_param("T"))),
        ));

        // Verify find_consumers works
        let iter_atom = graph.atoms.get(&CodeAtomId::new("rust::iter")).unwrap();
        let consumers = TypeEdgeBuilder::find_consumers(&graph, iter_atom);
        assert!(
            !consumers.is_empty(),
            "find_consumers should find collect for iter"
        );

        let chains = TypeEdgeBuilder::compose_chain(&graph, &CodeAtomId::new("rust::iter"), 3);
        assert!(!chains.is_empty());
        let longest = chains.iter().max_by_key(|c| c.steps.len()).unwrap();
        assert!(
            longest.steps.len() >= 2,
            "should find iter→collect chain, got {} steps",
            longest.steps.len()
        );
    }

    #[test]
    fn test_build_type_edges() {
        let mut graph = CodeGraph::new();
        graph.add_atom(make_atom(
            "rust::a",
            CodeLang::Rust,
            "a",
            vec![],
            Some(TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32))),
        ));
        graph.add_atom(make_atom(
            "rust::b",
            CodeLang::Rust,
            "b",
            vec![make_param(
                "input",
                TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32)),
            )],
            Some(TypeInfo::bool()),
        ));

        TypeEdgeBuilder::build_type_edges(&mut graph);
        let edges: Vec<_> =
            graph.relations_by_type(&CodeAtomId::new("rust::a"), CodeRelationType::RelComposes);
        assert!(!edges.is_empty(), "should add RelComposes edge a→b");
    }

    #[test]
    fn test_score_chain() {
        let mut graph = CodeGraph::new();
        let a = make_atom(
            "rust::a",
            CodeLang::Rust,
            "a",
            vec![],
            Some(TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32))),
        );
        let b = make_atom(
            "rust::b",
            CodeLang::Rust,
            "b",
            vec![make_param(
                "input",
                TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32)),
            )],
            Some(TypeInfo::bool()),
        );
        graph.add_atom(a);
        graph.add_atom(b);

        let chain = CallChain {
            steps: vec![CodeAtomId::new("rust::a"), CodeAtomId::new("rust::b")],
            lang: CodeLang::Rust,
            total_complexity: None,
        };
        let score = TypeEdgeBuilder::score_chain(&graph, &chain);
        assert!(score > 0.99, "exact match should score ~1.0, got {}", score);
    }

    #[test]
    fn test_find_producers() {
        let mut graph = CodeGraph::new();
        graph.add_atom(make_atom(
            "rust::iter",
            CodeLang::Rust,
            "iter",
            vec![make_param("self", TypeInfo::vec(TypeInfo::type_param("T")))],
            Some(TypeInfo::Generic {
                base: "Iterator".into(),
                args: vec![TypeInfo::type_param("T")],
            }),
        ));
        graph.add_atom(make_atom(
            "rust::collect_target",
            CodeLang::Rust,
            "consume_iter",
            vec![make_param(
                "iter",
                TypeInfo::Generic {
                    base: "Iterator".into(),
                    args: vec![TypeInfo::type_param("T")],
                },
            )],
            Some(TypeInfo::vec(TypeInfo::type_param("T"))),
        ));

        let target = graph
            .atoms
            .get(&CodeAtomId::new("rust::collect_target"))
            .unwrap();
        let producers = TypeEdgeBuilder::find_producers(&graph, target);
        assert!(!producers.is_empty(), "should find iter as producer");
        assert_eq!(producers[0].0.name, "iter");
    }

    #[test]
    fn test_no_cross_lang_producers() {
        let mut graph = CodeGraph::new();
        graph.add_atom(make_atom(
            "rust::iter",
            CodeLang::Rust,
            "iter",
            vec![make_param("self", TypeInfo::vec(TypeInfo::string()))],
            Some(TypeInfo::Generic {
                base: "Iterator".into(),
                args: vec![TypeInfo::string()],
            }),
        ));
        graph.add_atom(make_atom(
            "py::consumer",
            CodeLang::Python,
            "consumer",
            vec![make_param(
                "it",
                TypeInfo::Generic {
                    base: "Iterator".into(),
                    args: vec![TypeInfo::string()],
                },
            )],
            Some(TypeInfo::unit()),
        ));

        let target = graph.atoms.get(&CodeAtomId::new("py::consumer")).unwrap();
        let producers = TypeEdgeBuilder::find_producers(&graph, target);
        assert!(producers.is_empty(), "should not find cross-lang producers");
    }
}

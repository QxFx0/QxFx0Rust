use crate::code_schema::{CallChain, CodeAtom, CodeGraph, CodeLang};

pub struct CodeRenderer;

impl CodeRenderer {
    pub fn render_signature(atom: &CodeAtom) -> String {
        let prefix = match atom.lang {
            CodeLang::Rust => format!("// {}\n", atom.doc),
            CodeLang::Python => format!("# {}\n", atom.doc),
            CodeLang::TypeScript => format!("// {}\n", atom.doc),
            CodeLang::Haskell => format!("-- {}\n", atom.doc),
        };
        format!("{}{}", prefix, atom.signature)
    }

    pub fn render_call(atom: &CodeAtom, args: &[&str]) -> String {
        let call_args = args.join(", ");
        match atom.lang {
            CodeLang::Rust => {
                if atom.signature.contains("&self") || atom.signature.contains("&mut self") {
                    format!(
                        "{}.{}({});",
                        args.first().unwrap_or(&"obj"),
                        atom.name.split("::").last().unwrap_or(&atom.name),
                        call_args
                    )
                } else {
                    format!("{}({});", atom.name, call_args)
                }
            }
            CodeLang::Python => {
                format!("{}({})", atom.name, call_args)
            }
            CodeLang::TypeScript => {
                format!("{}({});", atom.name, call_args)
            }
            CodeLang::Haskell => {
                format!("{} {}", atom.name, call_args)
            }
        }
    }

    pub fn render_chain(graph: &CodeGraph, chain: &CallChain) -> String {
        if chain.steps.is_empty() {
            return String::new();
        }

        let mut parts = Vec::new();
        for (i, step_id) in chain.steps.iter().enumerate() {
            if let Some(atom) = graph.atoms.get(step_id) {
                let step_str = if i == 0 {
                    Self::render_signature(atom)
                } else {
                    format!("  // step {}: {}\n  {}", i + 1, atom.doc, atom.signature)
                };
                parts.push(step_str);
            }
        }

        parts.join("\n")
    }

    pub fn render_example(atom: &CodeAtom) -> String {
        match atom.lang {
            CodeLang::Rust => format!("```rust\n{}\n```", atom.signature),
            CodeLang::Python => format!("```python\n{}\n```", atom.signature),
            CodeLang::TypeScript => format!("```typescript\n{}\n```", atom.signature),
            CodeLang::Haskell => format!("```haskell\n{}\n```", atom.signature),
        }
    }

    pub fn render_summary(atom: &CodeAtom) -> String {
        let mut s = format!("{} [{}]", atom.name, atom.lang.as_str());
        if let Some(c) = &atom.complexity {
            s.push_str(&format!(" O: {}", c));
        }
        if atom.requires_alloc {
            s.push_str(" [alloc]");
        }
        if atom.panics {
            s.push_str(" [panics]");
        }
        if atom.async_fn {
            s.push_str(" [async]");
        }
        s
    }

    pub fn render_comparison(graph: &CodeGraph, atom: &CodeAtom) -> String {
        let cross_lang = graph.cross_lang_mapping(&atom.id);
        if cross_lang.is_empty() {
            return String::new();
        }
        let mut s = format!("// Cross-language equivalents for {}:\n", atom.name);
        for other in cross_lang {
            s.push_str(&format!(
                "//   {} [{}]: {}\n",
                other.name,
                other.lang.as_str(),
                other.signature
            ));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::code_schema::*;

    fn make_atom(lang: CodeLang, name: &str, sig: &str, doc: &str) -> CodeAtom {
        CodeAtom {
            id: CodeAtomId::new(format!("{}::{}", lang.as_str(), name)),
            lang,
            kind: CodeAtomKind::Function,
            name: name.into(),
            module: "test".into(),
            signature: sig.into(),
            doc: doc.into(),
            tags: vec![],
            params: vec![],
            return_type: None,
            complexity: None,
            requires_alloc: false,
            panics: false,
            async_fn: false,
        }
    }

    #[test]
    fn test_render_signature_rust() {
        let atom = make_atom(
            CodeLang::Rust,
            "Vec::max",
            "pub fn max(&self) -> Option<T> where T: Ord",
            "Returns max element",
        );
        let s = CodeRenderer::render_signature(&atom);
        assert!(s.contains("// Returns max element"));
        assert!(s.contains("pub fn max"));
    }

    #[test]
    fn test_render_summary() {
        let atom = CodeAtom {
            complexity: Some("O(n)".into()),
            requires_alloc: true,
            panics: true,
            ..make_atom(CodeLang::Rust, "test", "fn test()", "test")
        };
        let s = CodeRenderer::render_summary(&atom);
        assert!(s.contains("[alloc]"));
        assert!(s.contains("[panics]"));
        assert!(s.contains("O(n)"));
    }

    #[test]
    fn test_render_example() {
        let atom = make_atom(
            CodeLang::Python,
            "sorted",
            "sorted(iterable, key=None)",
            "Sort iterable",
        );
        let s = CodeRenderer::render_example(&atom);
        assert!(s.contains("```python"));
    }
}

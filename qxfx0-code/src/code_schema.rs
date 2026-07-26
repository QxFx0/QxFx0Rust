use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::type_info::TypeInfo;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CodeAtomId(pub String);

impl CodeAtomId {
    pub fn new(s: impl Into<String>) -> Self {
        CodeAtomId(s.into())
    }
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeLang {
    Rust,
    Python,
    TypeScript,
    Haskell,
}

impl CodeLang {
    pub fn as_str(&self) -> &'static str {
        match self {
            CodeLang::Rust => "rust",
            CodeLang::Python => "python",
            CodeLang::TypeScript => "typescript",
            CodeLang::Haskell => "haskell",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeAtomKind {
    Function,
    Method,
    Trait,
    Struct,
    Enum,
    TypeAlias,
    Macro,
    Module,
    Concept,
    Pattern,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum CodeRelationType {
    RelCalls,
    RelReturns,
    RelRequires,
    RelImplements,
    RelAccepts,
    RelAlternative,
    RelComposes,
    RelTranslatesTo,
    RelDependsOn,
    RelVariant,
    RelBroader,
    RelNarrower,
}

impl CodeRelationType {
    pub fn is_structural(&self) -> bool {
        matches!(
            self,
            CodeRelationType::RelCalls
                | CodeRelationType::RelComposes
                | CodeRelationType::RelDependsOn
                | CodeRelationType::RelRequires
        )
    }

    pub fn is_cross_lang(&self) -> bool {
        matches!(self, CodeRelationType::RelTranslatesTo)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeAtom {
    pub id: CodeAtomId,
    pub lang: CodeLang,
    pub kind: CodeAtomKind,
    pub name: String,
    pub module: String,
    pub signature: String,
    pub doc: String,
    pub tags: Vec<String>,
    pub params: Vec<TypedParam>,
    pub return_type: Option<TypeInfo>,
    pub complexity: Option<String>,
    pub requires_alloc: bool,
    pub panics: bool,
    pub async_fn: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TypedParam {
    pub name: String,
    pub ty: TypeInfo,
    pub is_self: bool,
    pub is_mut: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CodeRelation {
    pub from: CodeAtomId,
    pub to: CodeAtomId,
    pub rel_type: CodeRelationType,
    pub lang: CodeLang,
    pub note: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct CodeGraph {
    pub atoms: BTreeMap<CodeAtomId, CodeAtom>,
    pub edges: Vec<CodeRelation>,
    pub edges_from: BTreeMap<CodeAtomId, Vec<usize>>,
    pub edges_to: BTreeMap<CodeAtomId, Vec<usize>>,
}

impl CodeGraph {
    pub fn new() -> Self {
        CodeGraph::default()
    }

    pub fn add_atom(&mut self, atom: CodeAtom) {
        self.atoms.insert(atom.id.clone(), atom);
    }

    pub fn add_relation(&mut self, rel: CodeRelation) {
        let idx = self.edges.len();
        self.edges_from
            .entry(rel.from.clone())
            .or_default()
            .push(idx);
        self.edges_to.entry(rel.to.clone()).or_default().push(idx);
        self.edges.push(rel);
    }

    pub fn relations_from(&self, id: &CodeAtomId) -> Vec<&CodeRelation> {
        self.edges_from
            .get(id)
            .map(|indices| indices.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    pub fn relations_to(&self, id: &CodeAtomId) -> Vec<&CodeRelation> {
        self.edges_to
            .get(id)
            .map(|indices| indices.iter().filter_map(|&i| self.edges.get(i)).collect())
            .unwrap_or_default()
    }

    pub fn relations_by_type(&self, id: &CodeAtomId, rt: CodeRelationType) -> Vec<&CodeRelation> {
        self.relations_from(id)
            .into_iter()
            .filter(|r| r.rel_type == rt)
            .collect()
    }

    pub fn find_by_name(&self, name: &str) -> Vec<&CodeAtom> {
        self.atoms.values().filter(|a| a.name == name).collect()
    }

    pub fn find_by_tag(&self, tag: &str) -> Vec<&CodeAtom> {
        self.atoms
            .values()
            .filter(|a| a.tags.contains(&tag.to_string()))
            .collect()
    }

    pub fn cross_lang_mapping(&self, id: &CodeAtomId) -> Vec<&CodeAtom> {
        let rels = self.relations_by_type(id, CodeRelationType::RelTranslatesTo);
        rels.iter().filter_map(|r| self.atoms.get(&r.to)).collect()
    }

    /// Validate code registry identity, endpoint and index invariants.
    pub fn validate(&self) -> Vec<String> {
        let mut violations = Vec::new();
        for (key, atom) in &self.atoms {
            if key != &atom.id {
                violations.push(format!(
                    "code atom key '{}' differs from id '{}'",
                    key.as_str(),
                    atom.id.as_str()
                ));
            }
            if atom.name.trim().is_empty() || atom.module.trim().is_empty() {
                violations.push(format!("code atom '{}' has empty metadata", key.as_str()));
            }
            if matches!(atom.kind, CodeAtomKind::Function | CodeAtomKind::Method)
                && atom.signature.trim().is_empty()
            {
                violations.push(format!("callable '{}' has no signature", key.as_str()));
            }
        }

        let mut expected_from: BTreeMap<CodeAtomId, Vec<usize>> = BTreeMap::new();
        let mut expected_to: BTreeMap<CodeAtomId, Vec<usize>> = BTreeMap::new();
        for (index, relation) in self.edges.iter().enumerate() {
            if !self.atoms.contains_key(&relation.from) {
                violations.push(format!(
                    "code edge {index} references missing source '{}'",
                    relation.from.as_str()
                ));
            }
            if !self.atoms.contains_key(&relation.to) {
                violations.push(format!(
                    "code edge {index} references missing target '{}'",
                    relation.to.as_str()
                ));
            }
            expected_from
                .entry(relation.from.clone())
                .or_default()
                .push(index);
            expected_to
                .entry(relation.to.clone())
                .or_default()
                .push(index);
        }
        if self.edges_from != expected_from {
            violations.push("code edges_from index does not match edges".into());
        }
        if self.edges_to != expected_to {
            violations.push("code edges_to index does not match edges".into());
        }
        violations
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CallChain {
    pub steps: Vec<CodeAtomId>,
    pub lang: CodeLang,
    pub total_complexity: Option<String>,
}

impl CallChain {
    pub fn single(id: CodeAtomId, lang: CodeLang) -> Self {
        CallChain {
            steps: vec![id],
            lang,
            total_complexity: None,
        }
    }

    pub fn len(&self) -> usize {
        self.steps.len()
    }

    pub fn is_empty(&self) -> bool {
        self.steps.is_empty()
    }
}

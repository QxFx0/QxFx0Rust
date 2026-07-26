use serde::{Deserialize, Serialize};

use crate::code_schema::{CodeAtom, CodeAtomKind, CodeGraph, CodeLang};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Intent {
    pub verb: IntentVerb,
    pub object: IntentObject,
    pub modifiers: Vec<IntentModifier>,
    pub lang_pref: Option<CodeLang>,
    pub raw: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IntentVerb {
    Find,
    Sort,
    Filter,
    Transform,
    Aggregate,
    Create,
    Remove,
    Check,
    Iterate,
    Combine,
    Convert,
    Handle,
    Store,
    Compare,
    Unknown,
}

impl IntentVerb {
    pub fn as_str(&self) -> &'static str {
        match self {
            IntentVerb::Find => "find",
            IntentVerb::Sort => "sort",
            IntentVerb::Filter => "filter",
            IntentVerb::Transform => "transform",
            IntentVerb::Aggregate => "aggregate",
            IntentVerb::Create => "create",
            IntentVerb::Remove => "remove",
            IntentVerb::Check => "check",
            IntentVerb::Iterate => "iterate",
            IntentVerb::Combine => "combine",
            IntentVerb::Convert => "convert",
            IntentVerb::Handle => "handle",
            IntentVerb::Store => "store",
            IntentVerb::Compare => "compare",
            IntentVerb::Unknown => "unknown",
        }
    }

    pub fn to_tags(&self) -> Vec<&'static str> {
        match self {
            IntentVerb::Find => vec![
                "find", "search", "lookup", "get", "max", "min", "first", "last", "locate",
                "retrieve",
            ],
            IntentVerb::Sort => vec!["sort", "ordering", "arrange", "ordered"],
            IntentVerb::Filter => vec!["filter", "where", "select", "retain", "keep"],
            IntentVerb::Transform => vec!["map", "transform", "convert", "apply", "change"],
            IntentVerb::Aggregate => vec![
                "sum",
                "product",
                "count",
                "fold",
                "reduce",
                "accumulate",
                "total",
                "aggregate",
            ],
            IntentVerb::Create => vec![
                "create",
                "new",
                "build",
                "construct",
                "make",
                "init",
                "empty",
            ],
            IntentVerb::Remove => vec![
                "remove", "delete", "drop", "clear", "drain", "free", "destroy",
            ],
            IntentVerb::Check => vec![
                "check", "contains", "any", "all", "exists", "validate", "verify", "test",
            ],
            IntentVerb::Iterate => vec![
                "iter",
                "iterator",
                "iteration",
                "loop",
                "each",
                "walk",
                "traverse",
                "next",
                "enumerate",
            ],
            IntentVerb::Combine => vec![
                "zip", "chain", "merge", "concat", "join", "combine", "union",
            ],
            IntentVerb::Convert => vec![
                "collect",
                "into",
                "from",
                "cast",
                "parse",
                "serialize",
                "deserialize",
            ],
            IntentVerb::Handle => vec![
                "error",
                "result",
                "option",
                "unwrap",
                "match",
                "handle",
                "catch",
                "recover",
                "propagate",
            ],
            IntentVerb::Store => vec![
                "insert", "push", "add", "set", "store", "save", "append", "put",
            ],
            IntentVerb::Compare => vec![
                "cmp", "compare", "eq", "partial", "ord", "order", "less", "greater",
            ],
            IntentVerb::Unknown => vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IntentObject {
    Array,
    String,
    Map,
    Set,
    Iterator,
    Number,
    File,
    Json,
    Custom(String),
}

impl IntentObject {
    pub fn as_str(&self) -> &str {
        match self {
            IntentObject::Array => "array",
            IntentObject::String => "string",
            IntentObject::Map => "map",
            IntentObject::Set => "set",
            IntentObject::Iterator => "iterator",
            IntentObject::Number => "number",
            IntentObject::File => "file",
            IntentObject::Json => "json",
            IntentObject::Custom(s) => s,
        }
    }

    pub fn to_type_tags(&self) -> Vec<&'static str> {
        match self {
            IntentObject::Array => vec!["Vec", "array", "list", "collection", "elements", "items"],
            IntentObject::String => vec!["String", "str", "string", "text"],
            IntentObject::Map => vec!["HashMap", "BTreeMap", "dict", "map"],
            IntentObject::Set => vec!["HashSet", "BTreeSet", "set"],
            IntentObject::Iterator => vec!["Iterator", "iter", "IntoIterator", "loop", "traverse"],
            IntentObject::Number => vec!["i32", "f64", "number", "numeric"],
            IntentObject::File => vec!["File", "Read", "Write", "file", "io"],
            IntentObject::Json => vec!["json", "serde", "serialize", "deserialize"],
            IntentObject::Custom(_) => vec![],
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IntentModifier {
    InPlace,
    Immutable,
    Sorted,
    Stable,
    Unstable,
    Lazy,
    Eager,
    Parallel,
    Async,
    ErrorSafe,
    Reverse,
    Unique,
}

impl IntentModifier {
    pub fn as_str(&self) -> &'static str {
        match self {
            IntentModifier::InPlace => "in-place",
            IntentModifier::Immutable => "immutable",
            IntentModifier::Sorted => "sorted",
            IntentModifier::Stable => "stable",
            IntentModifier::Unstable => "unstable",
            IntentModifier::Lazy => "lazy",
            IntentModifier::Eager => "eager",
            IntentModifier::Parallel => "parallel",
            IntentModifier::Async => "async",
            IntentModifier::ErrorSafe => "error-safe",
            IntentModifier::Reverse => "reverse",
            IntentModifier::Unique => "unique",
        }
    }

    pub fn to_tags(&self) -> Vec<&'static str> {
        match self {
            IntentModifier::InPlace => vec!["in-place", "mut"],
            IntentModifier::Immutable => vec!["immutable", "&self"],
            IntentModifier::Sorted => vec!["sorted", "ordered"],
            IntentModifier::Stable => vec!["stable"],
            IntentModifier::Unstable => vec!["unstable"],
            IntentModifier::Lazy => vec!["lazy", "iterator"],
            IntentModifier::Eager => vec!["eager", "collect"],
            IntentModifier::Parallel => vec!["parallel", "rayon"],
            IntentModifier::Async => vec!["async", "await"],
            IntentModifier::ErrorSafe => vec!["error-safe", "no-panic"],
            IntentModifier::Reverse => vec!["reverse", "rev"],
            IntentModifier::Unique => vec!["unique", "dedup"],
        }
    }
}

pub struct IntentParser;

impl IntentParser {
    pub fn parse(input: &str) -> Intent {
        let lower = input.to_lowercase();
        let trimmed = lower.trim();

        let verb = Self::detect_verb(trimmed);
        let object = Self::detect_object(trimmed);
        let modifiers = Self::detect_modifiers(trimmed);
        let lang_pref = Self::detect_lang(trimmed);

        Intent {
            verb,
            object,
            modifiers,
            lang_pref,
            raw: input.to_string(),
        }
    }

    fn detect_verb(text: &str) -> IntentVerb {
        // Strip common question prefixes that don't affect verb detection
        let text = text.strip_prefix("how to ").unwrap_or(text);
        let text = text.strip_prefix("how do i ").unwrap_or(text);
        let text = text.strip_prefix("how can i ").unwrap_or(text);
        let text = text.strip_prefix("i want to ").unwrap_or(text);
        let text = text.strip_prefix("i need to ").unwrap_or(text);

        let verb_map: &[(&[&str], IntentVerb)] = &[
            (
                &[
                    "найти",
                    "find",
                    "get",
                    "lookup",
                    "получить",
                    "максимум",
                    "минимум",
                    "max",
                    "min",
                    "first",
                    "последний",
                    "первый",
                    "last",
                    "locate",
                    "retrieve",
                    "fetch",
                ],
                IntentVerb::Find,
            ),
            (
                &[
                    "сортировать",
                    "sort",
                    "упорядочить",
                    "отсортировать",
                    "order",
                    "arrange",
                    "organize",
                ],
                IntentVerb::Sort,
            ),
            (
                &[
                    "фильтровать",
                    "filter",
                    "отфильтровать",
                    "where",
                    "select",
                    "retain",
                    "оставить",
                    "exclude",
                    "keep",
                ],
                IntentVerb::Filter,
            ),
            (
                &[
                    "преобразовать",
                    "transform",
                    "map",
                    "применить",
                    "apply",
                    "изменить",
                    "convert",
                    "change",
                ],
                IntentVerb::Transform,
            ),
            (
                &[
                    "сложить",
                    "sum",
                    "aggregate",
                    "посчитать",
                    "count",
                    "fold",
                    "reduce",
                    "сумма",
                    "количество",
                    "total",
                    "accumulate",
                ],
                IntentVerb::Aggregate,
            ),
            (
                &[
                    "создать",
                    "create",
                    "new",
                    "построить",
                    "build",
                    "construct",
                    "сделать",
                    "init",
                    "make",
                    "initialize",
                ],
                IntentVerb::Create,
            ),
            (
                &[
                    "удалить",
                    "remove",
                    "delete",
                    "drop",
                    "очистить",
                    "clear",
                    "убрать",
                    "destroy",
                    "free",
                ],
                IntentVerb::Remove,
            ),
            (
                &[
                    "проверить",
                    "check",
                    "contains",
                    "содержит",
                    "exists",
                    "validate",
                    "any",
                    "all",
                    "проверка",
                    "verify",
                    "test",
                ],
                IntentVerb::Check,
            ),
            (
                &[
                    "итерировать",
                    "iterate",
                    "перебрать",
                    "loop",
                    "for",
                    "each",
                    "пройти",
                    "traverse",
                    "обойти",
                    "walk",
                    "visit",
                    "enumerate",
                ],
                IntentVerb::Iterate,
            ),
            (
                &[
                    "объединить",
                    "combine",
                    "zip",
                    "merge",
                    "concat",
                    "join",
                    "соединить",
                    "слить",
                    "union",
                    "interleave",
                ],
                IntentVerb::Combine,
            ),
            (
                &[
                    "конвертировать",
                    "convert",
                    "collect",
                    "into",
                    "cast",
                    "parse",
                    "преобразовать в",
                    "serialize",
                    "deserialize",
                ],
                IntentVerb::Convert,
            ),
            (
                &[
                    "обработать",
                    "handle",
                    "error",
                    "ошибк",
                    "result",
                    "option",
                    "unwrap",
                    "match",
                    "catch",
                    "recover",
                    "propagate",
                    "panic",
                ],
                IntentVerb::Handle,
            ),
            (
                &[
                    "вставить",
                    "store",
                    "insert",
                    "push",
                    "add",
                    "добавить",
                    "сохранить",
                    "save",
                    "put",
                    "append",
                ],
                IntentVerb::Store,
            ),
            (
                &[
                    "сравнить",
                    "compare",
                    "cmp",
                    "eq",
                    "ord",
                    "порядок",
                    "order",
                    "less",
                    "greater",
                ],
                IntentVerb::Compare,
            ),
        ];

        for (keywords, verb) in verb_map {
            if keywords.iter().any(|kw| text.contains(kw)) {
                return *verb;
            }
        }
        IntentVerb::Unknown
    }

    fn detect_object(text: &str) -> IntentObject {
        // Strip "how to" prefix — it doesn't affect object detection
        let text = text.strip_prefix("how to ").unwrap_or(text);
        let text = text.strip_prefix("how to ").unwrap_or(text); // double strip for "how to how to"

        let object_map: &[(&[&str], IntentObject)] = &[
            (
                &[
                    "массив",
                    "array",
                    "vec",
                    "вектор",
                    "list",
                    "список",
                    "slice",
                    "срез",
                    "elements",
                    "items",
                    "values",
                    "data",
                    "collection",
                ],
                IntentObject::Array,
            ),
            (
                &["строк", "string", "str", "текст", "text"],
                IntentObject::String,
            ),
            (
                &["map", "hashmap", "dict", "словарь", "btreemap", "хэш"],
                IntentObject::Map,
            ),
            (
                &["set", "hashset", "множество", "btreeset"],
                IntentObject::Set,
            ),
            (
                &[
                    "iterator",
                    "iter",
                    "итератор",
                    "sequence",
                    "последовательность",
                    "stream",
                ],
                IntentObject::Iterator,
            ),
            (
                &[
                    "число",
                    "number",
                    "integer",
                    "float",
                    "int",
                    "numeric",
                    "i32",
                    "f64",
                ],
                IntentObject::Number,
            ),
            (
                &["файл", "file", "read", "write", "io", "чтение", "запись"],
                IntentObject::File,
            ),
            (
                &["json", "serde", "serialize", "deserialize", "сериализовать"],
                IntentObject::Json,
            ),
        ];

        for (keywords, obj) in object_map {
            if keywords.iter().any(|kw| text.contains(kw)) {
                return obj.clone();
            }
        }
        IntentObject::Custom("unknown".into())
    }

    fn detect_modifiers(text: &str) -> Vec<IntentModifier> {
        let mut mods = Vec::new();
        let mod_map: &[(&[&str], IntentModifier)] = &[
            (
                &[
                    "in-place",
                    "in place",
                    "на месте",
                    "мутабельно",
                    "mutable",
                    "&mut",
                ],
                IntentModifier::InPlace,
            ),
            (
                &["immutable", "неизменяемый", "&self", "immut"],
                IntentModifier::Immutable,
            ),
            (&["stable", "стабильный"], IntentModifier::Stable),
            (&["unstable", "нестабильный"], IntentModifier::Unstable),
            (
                &["lazy", "ленивый", "iterator", "lazy"],
                IntentModifier::Lazy,
            ),
            (
                &["eager", "жадный", "collect", "сразу"],
                IntentModifier::Eager,
            ),
            (
                &["parallel", "параллельн", "rayon"],
                IntentModifier::Parallel,
            ),
            (&["async", "асинхронн", "await"], IntentModifier::Async),
            (
                &["safe", "безопасн", "no-panic", "error-safe", "без паники"],
                IntentModifier::ErrorSafe,
            ),
            (
                &["reverse", "обратный", "наоборот", "rev"],
                IntentModifier::Reverse,
            ),
            (
                &["unique", "уникальный", "dedup", "без дубликатов"],
                IntentModifier::Unique,
            ),
            (
                &["sorted", "отсортированный", "упорядоченный"],
                IntentModifier::Sorted,
            ),
        ];

        for (keywords, m) in mod_map {
            if keywords.iter().any(|kw| text.contains(kw)) {
                mods.push(m.clone());
            }
        }
        mods
    }

    fn detect_lang(text: &str) -> Option<CodeLang> {
        // Use word-boundary matching to avoid false positives
        // (e.g., "elements" contains "ts" → TypeScript false positive)
        let words: Vec<&str> = text.split_whitespace().collect();
        let has_word = |kw: &str| {
            words
                .iter()
                .any(|w| *w == kw || (kw.len() >= 4 && w.contains(kw)))
        };

        if has_word("rust") || has_word("раст") || has_word("cargo") {
            Some(CodeLang::Rust)
        } else if has_word("python") || has_word("питон") {
            Some(CodeLang::Python)
        } else if has_word("typescript") {
            Some(CodeLang::TypeScript)
        } else if has_word("haskell") || has_word("хаскел") {
            Some(CodeLang::Haskell)
        } else {
            None
        }
    }

    pub fn to_search_tags(intent: &Intent) -> Vec<String> {
        let mut tags = Vec::new();
        tags.extend(intent.verb.to_tags().iter().map(|s| s.to_string()));
        tags.extend(intent.object.to_type_tags().iter().map(|s| s.to_string()));
        for m in &intent.modifiers {
            tags.extend(m.to_tags().iter().map(|s| s.to_string()));
        }
        tags
    }

    pub fn match_atoms<'g>(intent: &Intent, graph: &'g CodeGraph) -> Vec<(&'g CodeAtom, f64)> {
        let verb_tags: Vec<String> = intent
            .verb
            .to_tags()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let object_tags: Vec<String> = intent
            .object
            .to_type_tags()
            .iter()
            .map(|s| s.to_string())
            .collect();
        let modifier_tags: Vec<String> = intent
            .modifiers
            .iter()
            .flat_map(|m| {
                m.to_tags()
                    .iter()
                    .map(|s| s.to_string())
                    .collect::<Vec<_>>()
            })
            .collect();
        let _search_tags: Vec<String> = verb_tags
            .iter()
            .chain(object_tags.iter())
            .chain(modifier_tags.iter())
            .cloned()
            .collect();

        let mut scored: Vec<(&CodeAtom, f64)> = Vec::new();

        for atom in graph.atoms.values() {
            if let Some(lang) = intent.lang_pref {
                if atom.lang != lang {
                    continue;
                }
            }

            let mut score = 0.0;

            // Verb-specific tags get highest weight (3.0)
            for tag in &verb_tags {
                if atom.tags.iter().any(|t| t == tag) {
                    score += 3.0;
                }
            }
            // Object type tags get medium weight (1.0)
            for tag in &object_tags {
                if atom.tags.iter().any(|t| t == tag) {
                    score += 1.0;
                }
            }
            // Modifier tags get low weight (0.5)
            for tag in &modifier_tags {
                if atom.tags.iter().any(|t| t == tag) {
                    score += 0.5;
                }
            }

            // Name matching: verb keywords in atom name get boost (2.0)
            let name_lower = atom.name.to_lowercase();
            for tag in &verb_tags {
                if name_lower.contains(tag.as_str()) {
                    score += 2.0;
                }
            }
            // Object keywords in name get small boost (0.5)
            for tag in &object_tags {
                if name_lower.contains(tag.as_str()) {
                    score += 0.5;
                }
            }

            // Penalize Struct/Enum/Trait when looking for actions (verbs imply functions/methods)
            if matches!(
                intent.verb,
                IntentVerb::Find
                    | IntentVerb::Sort
                    | IntentVerb::Filter
                    | IntentVerb::Transform
                    | IntentVerb::Aggregate
                    | IntentVerb::Remove
                    | IntentVerb::Check
                    | IntentVerb::Iterate
                    | IntentVerb::Combine
                    | IntentVerb::Convert
                    | IntentVerb::Store
                    | IntentVerb::Compare
            ) {
                match atom.kind {
                    CodeAtomKind::Struct
                    | CodeAtomKind::Enum
                    | CodeAtomKind::Trait
                    | CodeAtomKind::TypeAlias
                    | CodeAtomKind::Module => {
                        score *= 0.3; // heavily penalize type definitions when looking for operations
                    }
                    CodeAtomKind::Concept => {
                        score *= 0.1; // concepts are even less relevant for specific operations
                    }
                    CodeAtomKind::Pattern => {
                        score *= 0.5;
                    }
                    CodeAtomKind::Function | CodeAtomKind::Method | CodeAtomKind::Macro => {
                        score *= 1.2; // boost actual functions/methods
                    }
                }
            }

            // Boost atoms with more specific tags (higher information content)
            let specific_tags = atom
                .tags
                .iter()
                .filter(|t| !matches!(t.as_str(), "alloc" | "collection" | "trait"))
                .count();
            score += specific_tags as f64 * 0.1;

            if score > 0.5 {
                scored.push((atom, score));
            }
        }

        // Sort by score descending, with deterministic tie-breaking:
        // 1. Higher score first
        // 2. Name contains verb keyword (more semantically relevant)
        // 3. Kind is Function/Method (more actionable than Struct/Trait)
        // 4. Lexicographic name (deterministic last resort)
        let verb_tags_lower: Vec<String> = verb_tags.iter().map(|t| t.to_lowercase()).collect();
        scored.sort_by(|a, b| {
            // Primary: score
            let score_cmp = b.1.total_cmp(&a.1);
            if !score_cmp.is_eq() {
                return score_cmp;
            }
            // Secondary: name contains verb keyword
            let a_name_match = verb_tags_lower
                .iter()
                .any(|t| a.0.name.to_lowercase().contains(t));
            let b_name_match = verb_tags_lower
                .iter()
                .any(|t| b.0.name.to_lowercase().contains(t));
            if b_name_match && !a_name_match {
                return std::cmp::Ordering::Greater;
            }
            if !b_name_match && a_name_match {
                return std::cmp::Ordering::Less;
            }
            // Tertiary: Function/Method > Struct/Trait > Concept
            let kind_priority = |k: CodeAtomKind| match k {
                CodeAtomKind::Function => 0,
                CodeAtomKind::Method => 1,
                CodeAtomKind::Macro => 2,
                CodeAtomKind::Pattern => 3,
                CodeAtomKind::Trait => 4,
                CodeAtomKind::Struct => 5,
                CodeAtomKind::Enum => 6,
                CodeAtomKind::TypeAlias => 7,
                CodeAtomKind::Module => 8,
                CodeAtomKind::Concept => 9,
            };
            let kind_cmp = kind_priority(a.0.kind).cmp(&kind_priority(b.0.kind));
            if !kind_cmp.is_eq() {
                return kind_cmp;
            }
            // Quaternary: lexicographic name (deterministic)
            a.0.name.cmp(&b.0.name)
        });
        scored
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_find_max() {
        let intent = IntentParser::parse("найти максимум в массиве");
        assert_eq!(intent.verb, IntentVerb::Find);
        assert_eq!(intent.object, IntentObject::Array);
    }

    #[test]
    fn test_parse_sort() {
        let intent = IntentParser::parse("отсортировать вектор");
        assert_eq!(intent.verb, IntentVerb::Sort);
        assert_eq!(intent.object, IntentObject::Array);
    }

    #[test]
    fn test_parse_english() {
        let intent = IntentParser::parse("sort array in-place");
        assert_eq!(intent.verb, IntentVerb::Sort);
        assert_eq!(intent.object, IntentObject::Array);
        assert!(intent.modifiers.contains(&IntentModifier::InPlace));
    }

    #[test]
    fn test_parse_iterate_elements() {
        let intent = IntentParser::parse("iterate elements");
        assert_eq!(intent.verb, IntentVerb::Iterate);
        assert_eq!(intent.object, IntentObject::Array);
    }

    #[test]
    fn test_parse_how_to() {
        let intent = IntentParser::parse("how to handle errors in Rust");
        assert_eq!(intent.verb, IntentVerb::Handle);
        assert_eq!(intent.lang_pref, Some(CodeLang::Rust));
    }

    #[test]
    fn test_parse_cross_lang_python() {
        let intent = IntentParser::parse("sort list python");
        assert_eq!(intent.verb, IntentVerb::Sort);
        assert_eq!(intent.lang_pref, Some(CodeLang::Python));
    }

    #[test]
    fn test_parse_error_handling() {
        let intent = IntentParser::parse("обработать ошибку в rust");
        assert_eq!(intent.verb, IntentVerb::Handle);
        assert_eq!(intent.lang_pref, Some(CodeLang::Rust));
    }

    #[test]
    fn test_parse_filter_string() {
        let intent = IntentParser::parse("filter string");
        assert_eq!(intent.verb, IntentVerb::Filter);
        assert_eq!(intent.object, IntentObject::String);
    }

    #[test]
    fn test_to_search_tags() {
        let intent = Intent {
            verb: IntentVerb::Find,
            object: IntentObject::Array,
            modifiers: vec![IntentModifier::ErrorSafe],
            lang_pref: None,
            raw: String::new(),
        };
        let tags = IntentParser::to_search_tags(&intent);
        assert!(tags.contains(&"max".to_string()));
        assert!(tags.contains(&"Vec".to_string()));
        assert!(tags.contains(&"no-panic".to_string()));
    }
}

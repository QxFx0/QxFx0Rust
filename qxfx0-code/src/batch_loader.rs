//! Batch loader — converts Kimi-generated batch data into CodeGraph atoms.
//!
//! Kimi batches use a simpler schema (no TypedParam/return_type).
//! This loader parses signatures to extract type information where possible
//! and creates CodeAtom entries compatible with our CodeGraph.

use crate::code_schema::{
    CodeAtom, CodeAtomId, CodeAtomKind, CodeLang, CodeRelation, CodeRelationType,
};
use crate::type_info::{PrimitiveType, TypeInfo};

/// Input for `convert_atom` — groups parameters to reduce argument count.
pub struct AtomInput<'a> {
    pub id: String,
    pub lang: CodeLang,
    pub kind: CodeAtomKind,
    pub name: &'a str,
    pub module: &'a str,
    pub signature: &'a str,
    pub doc: &'a str,
    pub tags: &'a [&'a str],
}

/// Convert a simple atom (from Kimi batch format) into our CodeAtom.
/// Parses the signature string to extract return type and params heuristically.
pub fn convert_atom(input: &AtomInput) -> CodeAtom {
    let (params, return_type) = parse_signature(input.signature, input.lang, input.name);

    CodeAtom {
        id: CodeAtomId::new(&input.id),
        lang: input.lang,
        kind: input.kind,
        name: input.name.to_string(),
        module: input.module.to_string(),
        signature: input.signature.to_string(),
        doc: input.doc.to_string(),
        tags: input.tags.iter().map(|s| s.to_string()).collect(),
        params,
        return_type,
        complexity: extract_complexity(input.tags),
        requires_alloc: input.tags.iter().any(|t| t == &"alloc"),
        panics: input.tags.iter().any(|t| t == &"panics"),
        async_fn: input.tags.iter().any(|t| t == &"async"),
    }
}

/// Convert a simple relation (from Kimi batch format) into our CodeRelation.
pub fn convert_relation(
    from: &str,
    to: &str,
    rel_type: CodeRelationType,
    lang: CodeLang,
    doc: &str,
) -> CodeRelation {
    CodeRelation {
        from: CodeAtomId::new(from),
        to: CodeAtomId::new(to),
        rel_type,
        lang,
        note: doc.to_string(),
    }
}

/// Heuristic signature parser — extracts params and return type from a Rust/Python signature string.
/// `method_name` is used to infer the self type for methods (e.g., "Vec::push" → self: Vec<T>).
fn parse_signature(
    sig: &str,
    lang: CodeLang,
    method_name: &str,
) -> (Vec<crate::code_schema::TypedParam>, Option<TypeInfo>) {
    let mut params = Vec::new();
    let mut return_type = None;

    match lang {
        CodeLang::Rust => {
            // Infer self type from method name (e.g., "Vec::push" → Vec<T>)
            let self_type = infer_self_type(method_name);
            // Try to find "-> ReturnType" in the signature
            if let Some(arrow_pos) = sig.find("-> ") {
                let ret_str = sig[arrow_pos + 3..].trim();
                // Cut at "where" clause, "{", ";", or end of string
                let ret_str = ret_str
                    .split(" where ")
                    .next()
                    .unwrap_or(ret_str)
                    .split(['{', ';'])
                    .next()
                    .unwrap_or("")
                    .trim();
                if !ret_str.is_empty() {
                    return_type = Some(parse_rust_type(ret_str));
                }
            }

            // Try to extract params from "fn name(params)" or "(params)"
            if let Some(open) = sig.find('(') {
                if let Some(close) = sig[open..].find(')') {
                    let param_str = &sig[open + 1..open + close];
                    for param in param_str.split(',') {
                        let param = param.trim();
                        if param.is_empty()
                            || param == "&self"
                            || param == "&mut self"
                            || param == "self"
                        {
                            if param == "&self" {
                                params.push(crate::code_schema::TypedParam {
                                    name: "self".into(),
                                    ty: TypeInfo::Reference {
                                        inner: Box::new(self_type.clone()),
                                        mutable: false,
                                        lifetime: None,
                                    },
                                    is_self: true,
                                    is_mut: false,
                                });
                            } else if param == "&mut self" {
                                params.push(crate::code_schema::TypedParam {
                                    name: "self".into(),
                                    ty: TypeInfo::Reference {
                                        inner: Box::new(self_type.clone()),
                                        mutable: true,
                                        lifetime: None,
                                    },
                                    is_self: true,
                                    is_mut: true,
                                });
                            } else if param == "self" {
                                params.push(crate::code_schema::TypedParam {
                                    name: "self".into(),
                                    ty: self_type.clone(),
                                    is_self: true,
                                    is_mut: false,
                                });
                            }
                            continue;
                        }
                        // Split "name: Type"
                        if let Some(colon) = param.find(':') {
                            let name = param[..colon].trim();
                            let ty_str = param[colon + 1..].trim();
                            params.push(crate::code_schema::TypedParam {
                                name: name.to_string(),
                                ty: parse_rust_type(ty_str),
                                is_self: false,
                                is_mut: ty_str.starts_with("&mut"),
                            });
                        }
                    }
                }
            }
        }
        CodeLang::Python => {
            // Python: "def name(params) -> ReturnType"
            if let Some(arrow) = sig.find("-> ") {
                let ret_str = sig[arrow + 3..].trim();
                if !ret_str.is_empty() {
                    return_type = Some(parse_python_type(ret_str));
                }
            }
            if let Some(open) = sig.find('(') {
                if let Some(close) = sig[open..].find(')') {
                    let param_str = &sig[open + 1..open + close];
                    for param in param_str.split(',') {
                        let param = param.trim();
                        if param.is_empty() || param == "self" {
                            continue;
                        }
                        if let Some(colon) = param.find(':') {
                            let name = param[..colon].trim();
                            let ty_str = param[colon + 1..].trim();
                            params.push(crate::code_schema::TypedParam {
                                name: name.to_string(),
                                ty: parse_python_type(ty_str),
                                is_self: false,
                                is_mut: false,
                            });
                        } else {
                            params.push(crate::code_schema::TypedParam {
                                name: param.to_string(),
                                ty: TypeInfo::Unknown,
                                is_self: false,
                                is_mut: false,
                            });
                        }
                    }
                }
            }
        }
        _ => {
            // For TypeScript/Haskell, leave params empty — will be filled by hand
        }
    }

    (params, return_type)
}

/// Infer the self type from a method name like "Vec::push" → Vec<T>, "HashMap::insert" → HashMap<K,V>.
fn infer_self_type(method_name: &str) -> TypeInfo {
    if let Some(pos) = method_name.rfind("::") {
        let type_name = &method_name[..pos];
        // Map known collection types to their generic forms
        match type_name {
            "Vec" | "VecDeque" | "LinkedList" => {
                return TypeInfo::Generic {
                    base: type_name.to_string(),
                    args: vec![TypeInfo::type_param("T")],
                };
            }
            "HashMap" => {
                return TypeInfo::Generic {
                    base: type_name.to_string(),
                    args: vec![TypeInfo::type_param("K"), TypeInfo::type_param("V")],
                };
            }
            "BTreeMap" => {
                return TypeInfo::Generic {
                    base: type_name.to_string(),
                    args: vec![TypeInfo::type_param("K"), TypeInfo::type_param("V")],
                };
            }
            "HashSet" | "BTreeSet" => {
                return TypeInfo::Generic {
                    base: type_name.to_string(),
                    args: vec![TypeInfo::type_param("T")],
                };
            }
            "String" => return TypeInfo::string(),
            "str" => return TypeInfo::Primitive(PrimitiveType::Str),
            _ => {
                // Generic type with a single type param
                return TypeInfo::Generic {
                    base: type_name.to_string(),
                    args: vec![TypeInfo::type_param("T")],
                };
            }
        }
    }
    TypeInfo::Unknown
}

fn parse_rust_type(s: &str) -> TypeInfo {
    let s = s.trim().trim_end_matches([',', ';', '{', '}']);

    if s.is_empty() || s == "_" {
        return TypeInfo::Unknown;
    }

    if s == "()" {
        return TypeInfo::Primitive(PrimitiveType::Unit);
    }

    // References
    if let Some(rest) = s.strip_prefix("&mut ") {
        return TypeInfo::Reference {
            inner: Box::new(parse_rust_type(rest)),
            mutable: true,
            lifetime: None,
        };
    }
    if let Some(rest) = s.strip_prefix('&') {
        let inner_str = if let Some(rest2) = rest.strip_prefix('\'') {
            // &'a T — skip lifetime
            if let Some(space) = rest2.find(' ') {
                &rest2[space + 1..]
            } else {
                rest
            }
        } else {
            rest
        };
        return TypeInfo::Reference {
            inner: Box::new(parse_rust_type(inner_str)),
            mutable: false,
            lifetime: None,
        };
    }

    // Slices: [T]
    if s.starts_with('[') && s.ends_with(']') {
        let inner = &s[1..s.len() - 1];
        // Check for [T; N]
        if let Some(semi) = inner.find(';') {
            let elem = inner[..semi].trim();
            let size = inner[semi + 1..].trim().parse::<usize>().ok();
            return TypeInfo::Array {
                element: Box::new(parse_rust_type(elem)),
                size,
            };
        }
        return TypeInfo::Slice(Box::new(parse_rust_type(inner)));
    }

    // Tuples: (A, B)
    if s.starts_with('(') && s.ends_with(')') {
        let inner = &s[1..s.len() - 1];
        let items: Vec<TypeInfo> = inner
            .split(',')
            .map(|t| parse_rust_type(t.trim()))
            .collect();
        return TypeInfo::Tuple(items);
    }

    // Generics: Type<Args>
    if let Some(open) = s.find('<') {
        if s.ends_with('>') {
            let base = s[..open].trim();
            let args_str = &s[open + 1..s.len() - 1];
            let args: Vec<TypeInfo> = split_generic_args(args_str)
                .into_iter()
                .map(|a| parse_rust_type(a.trim()))
                .collect();
            return TypeInfo::Generic {
                base: base.to_string(),
                args,
            };
        }
    }

    // Primitives
    match s {
        "i8" => return TypeInfo::Primitive(PrimitiveType::I8),
        "i16" => return TypeInfo::Primitive(PrimitiveType::I16),
        "i32" => return TypeInfo::Primitive(PrimitiveType::I32),
        "i64" => return TypeInfo::Primitive(PrimitiveType::I64),
        "i128" => return TypeInfo::Primitive(PrimitiveType::I128),
        "isize" => return TypeInfo::Primitive(PrimitiveType::ISize),
        "u8" => return TypeInfo::Primitive(PrimitiveType::U8),
        "u16" => return TypeInfo::Primitive(PrimitiveType::U16),
        "u32" => return TypeInfo::Primitive(PrimitiveType::U32),
        "u64" => return TypeInfo::Primitive(PrimitiveType::U64),
        "u128" => return TypeInfo::Primitive(PrimitiveType::U128),
        "usize" => return TypeInfo::Primitive(PrimitiveType::USize),
        "f32" => return TypeInfo::Primitive(PrimitiveType::F32),
        "f64" => return TypeInfo::Primitive(PrimitiveType::F64),
        "bool" => return TypeInfo::Primitive(PrimitiveType::Bool),
        "char" => return TypeInfo::Primitive(PrimitiveType::Char),
        "str" => return TypeInfo::Primitive(PrimitiveType::Str),
        _ => {}
    }

    // Type parameter heuristic: single uppercase letter or short name
    let chars: Vec<char> = s.chars().collect();
    if chars.len() <= 2 && chars.first().map(|c| c.is_uppercase()).unwrap_or(false) {
        return TypeInfo::TypeParam {
            name: s.to_string(),
            bounds: vec![],
        };
    }

    // Named type without args
    TypeInfo::Generic {
        base: s.to_string(),
        args: vec![],
    }
}

fn split_generic_args(s: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut depth = 0;
    let mut current = String::new();
    for c in s.chars() {
        match c {
            '<' => {
                depth += 1;
                current.push(c);
            }
            '>' => {
                depth -= 1;
                current.push(c);
            }
            ',' if depth == 0 => {
                result.push(current.trim().to_string());
                current.clear();
            }
            _ => current.push(c),
        }
    }
    if !current.trim().is_empty() {
        result.push(current.trim().to_string());
    }
    result
}

fn parse_python_type(s: &str) -> TypeInfo {
    let s = s.trim();
    if s.is_empty() || s == "None" {
        return TypeInfo::Primitive(PrimitiveType::Unit);
    }
    if s == "int" {
        return TypeInfo::Primitive(PrimitiveType::I64);
    }
    if s == "float" {
        return TypeInfo::Primitive(PrimitiveType::F64);
    }
    if s == "bool" {
        return TypeInfo::Primitive(PrimitiveType::Bool);
    }
    if s == "str" {
        return TypeInfo::Generic {
            base: "str".into(),
            args: vec![],
        };
    }
    if s.starts_with("list[") && s.ends_with(']') {
        let inner = &s[5..s.len() - 1];
        return TypeInfo::Generic {
            base: "list".into(),
            args: vec![parse_python_type(inner)],
        };
    }
    if s.starts_with("dict[") && s.ends_with(']') {
        let inner = &s[5..s.len() - 1];
        let parts: Vec<&str> = inner.splitn(2, ',').collect();
        if parts.len() == 2 {
            return TypeInfo::Generic {
                base: "dict".into(),
                args: vec![
                    parse_python_type(parts[0].trim()),
                    parse_python_type(parts[1].trim()),
                ],
            };
        }
    }
    if s.starts_with("Optional[") && s.ends_with(']') {
        let inner = &s[9..s.len() - 1];
        return TypeInfo::option(parse_python_type(inner));
    }
    TypeInfo::Generic {
        base: s.to_string(),
        args: vec![],
    }
}

fn extract_complexity(tags: &[&str]) -> Option<String> {
    for tag in tags {
        if tag.starts_with("O(") {
            return Some(tag.to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_rust_vec_max() {
        let (params, ret) = parse_signature(
            "pub fn max(&self) -> Option<T> where T: Ord",
            CodeLang::Rust,
            "Vec::max",
        );
        assert!(params.iter().any(|p| p.is_self));
        assert!(ret.is_some());
        let r = ret.unwrap();
        assert_eq!(r.base_name(), "Option");
    }

    #[test]
    fn test_parse_rust_sort() {
        let (params, ret) = parse_signature(
            "pub fn sort_unstable(&mut self) where T: Ord",
            CodeLang::Rust,
            "Vec::sort_unstable",
        );
        assert!(params.iter().any(|p| p.is_self && p.is_mut));
        // sort returns () (unit) — no explicit -> in signature
        assert!(ret.is_none() || ret == Some(TypeInfo::Primitive(PrimitiveType::Unit)));
    }

    #[test]
    fn test_parse_rust_iter() {
        let (params, ret) = parse_signature(
            "pub fn iter(&self) -> Iter<'_, T>",
            CodeLang::Rust,
            "Vec::iter",
        );
        assert!(params.iter().any(|p| p.is_self));
        assert!(ret.is_some());
    }

    #[test]
    fn test_parse_rust_collect() {
        let (params, ret) = parse_signature(
            "pub fn collect<B: FromIterator<Self::Item>>(self) -> B",
            CodeLang::Rust,
            "Iterator::collect",
        );
        assert!(
            params.iter().any(|p| p.is_self),
            "collect should have self param"
        );
        assert!(ret.is_some());
    }

    #[test]
    fn test_parse_python_sorted() {
        let (params, ret) = parse_signature(
            "sorted(iterable: Iterable[T], key: Callable | None = None) -> list[T]",
            CodeLang::Python,
            "sorted",
        );
        assert!(!params.is_empty());
        assert!(ret.is_some());
    }

    #[test]
    fn test_convert_atom() {
        let input = AtomInput {
            id: "rust::std::vec::Vec::max".into(),
            lang: CodeLang::Rust,
            kind: CodeAtomKind::Method,
            name: "Vec::max",
            module: "std::vec",
            signature: "pub fn max(&self) -> Option<T> where T: Ord",
            doc: "Returns the maximum element",
            tags: &["max", "find", "Vec", "O(n)"],
        };
        let atom = convert_atom(&input);
        assert_eq!(atom.id.as_str(), "rust::std::vec::Vec::max");
        assert_eq!(atom.complexity, Some("O(n)".to_string()));
        assert!(atom.params.iter().any(|p| p.is_self));
        // F8 fix: self param should now have a typed Reference, not Unknown
        let self_param = atom.params.iter().find(|p| p.is_self).unwrap();
        assert!(
            matches!(&self_param.ty, TypeInfo::Reference { mutable: false, .. }),
            "self param should be &Vec<T>, got: {}",
            self_param.ty.display()
        );
    }

    #[test]
    fn test_self_type_inference_vec() {
        let ty = infer_self_type("Vec::push");
        assert_eq!(ty.base_name(), "Vec");
    }

    #[test]
    fn test_self_type_inference_hashmap() {
        let ty = infer_self_type("HashMap::insert");
        assert_eq!(ty.base_name(), "HashMap");
        // HashMap has 2 type params
        if let TypeInfo::Generic { args, .. } = &ty {
            assert_eq!(args.len(), 2);
        }
    }

    #[test]
    fn test_self_type_inference_mut() {
        let input = AtomInput {
            id: "rust::std::vec::Vec::push".into(),
            lang: CodeLang::Rust,
            kind: CodeAtomKind::Method,
            name: "Vec::push",
            module: "std::vec",
            signature: "pub fn push(&mut self, value: T)",
            doc: "Appends element",
            tags: &["insert", "O(1)"],
        };
        let atom = convert_atom(&input);
        let self_param = atom.params.iter().find(|p| p.is_self).unwrap();
        assert!(
            matches!(&self_param.ty, TypeInfo::Reference { mutable: true, .. }),
            "push self should be &mut Vec<T>, got: {}",
            self_param.ty.display()
        );
    }

    #[test]
    fn test_parse_ref_type() {
        let ty = parse_rust_type("&Vec<T>");
        assert!(matches!(ty, TypeInfo::Reference { mutable: false, .. }));
    }

    #[test]
    fn test_parse_mut_ref_type() {
        let ty = parse_rust_type("&mut Vec<T>");
        assert!(matches!(ty, TypeInfo::Reference { mutable: true, .. }));
    }

    #[test]
    fn test_parse_slice_type() {
        let ty = parse_rust_type("[u8]");
        assert!(matches!(ty, TypeInfo::Slice(_)));
    }

    #[test]
    fn test_parse_tuple_type() {
        let ty = parse_rust_type("(String, i32)");
        assert!(matches!(ty, TypeInfo::Tuple(_)));
    }

    #[test]
    fn test_parse_type_param() {
        let ty = parse_rust_type("T");
        assert!(matches!(ty, TypeInfo::TypeParam { .. }));
    }
}

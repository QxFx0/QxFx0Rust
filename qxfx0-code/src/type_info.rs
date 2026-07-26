use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeInfo {
    Primitive(PrimitiveType),
    Generic {
        base: String,
        args: Vec<TypeInfo>,
    },
    Function {
        params: Vec<TypeInfo>,
        ret: Box<TypeInfo>,
    },
    TraitBound {
        name: String,
        bounds: Vec<TypeInfo>,
    },
    Lifetime(String),
    /// Reference: &T or &mut T
    Reference {
        inner: Box<TypeInfo>,
        mutable: bool,
        lifetime: Option<String>,
    },
    /// Tuple: (A, B, C)
    Tuple(Vec<TypeInfo>),
    /// Array: [T; N]
    Array {
        element: Box<TypeInfo>,
        size: Option<usize>,
    },
    /// Slice: &[T]
    Slice(Box<TypeInfo>),
    /// Associated type: Iterator::Item, IntoIterator::IntoIter
    AssociatedType {
        trait_name: String,
        type_name: String,
    },
    /// Trait object: dyn Trait
    TraitObject {
        trait_name: String,
        lifetime: Option<String>,
    },
    /// Type parameter: T, E, K, V (with optional bounds)
    TypeParam {
        name: String,
        bounds: Vec<TypeInfo>,
    },
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PrimitiveType {
    I8,
    I16,
    I32,
    I64,
    I128,
    ISize,
    U8,
    U16,
    U32,
    U64,
    U128,
    USize,
    F32,
    F64,
    Bool,
    Char,
    Str,
    Unit,
}

impl PrimitiveType {
    pub fn as_str(&self) -> &'static str {
        match self {
            PrimitiveType::I8 => "i8",
            PrimitiveType::I16 => "i16",
            PrimitiveType::I32 => "i32",
            PrimitiveType::I64 => "i64",
            PrimitiveType::I128 => "i128",
            PrimitiveType::ISize => "isize",
            PrimitiveType::U8 => "u8",
            PrimitiveType::U16 => "u16",
            PrimitiveType::U32 => "u32",
            PrimitiveType::U64 => "u64",
            PrimitiveType::U128 => "u128",
            PrimitiveType::USize => "usize",
            PrimitiveType::F32 => "f32",
            PrimitiveType::F64 => "f64",
            PrimitiveType::Bool => "bool",
            PrimitiveType::Char => "char",
            PrimitiveType::Str => "str",
            PrimitiveType::Unit => "()",
        }
    }
}

impl TypeInfo {
    pub fn vec(item: TypeInfo) -> Self {
        TypeInfo::Generic {
            base: "Vec".into(),
            args: vec![item],
        }
    }

    pub fn option(inner: TypeInfo) -> Self {
        TypeInfo::Generic {
            base: "Option".into(),
            args: vec![inner],
        }
    }

    pub fn result(ok: TypeInfo, err: TypeInfo) -> Self {
        TypeInfo::Generic {
            base: "Result".into(),
            args: vec![ok, err],
        }
    }

    pub fn hashmap(k: TypeInfo, v: TypeInfo) -> Self {
        TypeInfo::Generic {
            base: "HashMap".into(),
            args: vec![k, v],
        }
    }

    pub fn string() -> Self {
        TypeInfo::Generic {
            base: "String".into(),
            args: vec![],
        }
    }

    pub fn str_ref() -> Self {
        TypeInfo::Primitive(PrimitiveType::Str)
    }

    pub fn bool() -> Self {
        TypeInfo::Primitive(PrimitiveType::Bool)
    }

    pub fn unit() -> Self {
        TypeInfo::Primitive(PrimitiveType::Unit)
    }

    pub fn type_param(name: &str) -> Self {
        TypeInfo::TypeParam {
            name: name.into(),
            bounds: vec![],
        }
    }

    pub fn ref_to(inner: TypeInfo) -> Self {
        TypeInfo::Reference {
            inner: Box::new(inner),
            mutable: false,
            lifetime: None,
        }
    }

    pub fn mut_ref_to(inner: TypeInfo) -> Self {
        TypeInfo::Reference {
            inner: Box::new(inner),
            mutable: true,
            lifetime: None,
        }
    }

    pub fn slice_of(inner: TypeInfo) -> Self {
        TypeInfo::Slice(Box::new(inner))
    }

    pub fn tuple(items: Vec<TypeInfo>) -> Self {
        TypeInfo::Tuple(items)
    }

    pub fn display(&self) -> String {
        match self {
            TypeInfo::Primitive(p) => p.as_str().to_string(),
            TypeInfo::Generic { base, args } => {
                if args.is_empty() {
                    base.clone()
                } else {
                    format!(
                        "{}<{}>",
                        base,
                        args.iter()
                            .map(|a| a.display())
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            }
            TypeInfo::Function { params, ret } => {
                format!(
                    "fn({}) -> {}",
                    params
                        .iter()
                        .map(|p| p.display())
                        .collect::<Vec<_>>()
                        .join(", "),
                    ret.display()
                )
            }
            TypeInfo::TraitBound { name, bounds } => {
                if bounds.is_empty() {
                    name.clone()
                } else {
                    format!(
                        "{}: {}",
                        name,
                        bounds
                            .iter()
                            .map(|b| b.display())
                            .collect::<Vec<_>>()
                            .join(" + ")
                    )
                }
            }
            TypeInfo::Lifetime(s) => format!("'{s}"),
            TypeInfo::Reference {
                inner,
                mutable,
                lifetime,
            } => {
                let lt = lifetime
                    .as_deref()
                    .map(|l| format!("'{l} "))
                    .unwrap_or_default();
                if *mutable {
                    format!("&{lt}mut {}", inner.display())
                } else {
                    format!("&{lt}{}", inner.display())
                }
            }
            TypeInfo::Tuple(items) => {
                format!(
                    "({})",
                    items
                        .iter()
                        .map(|t| t.display())
                        .collect::<Vec<_>>()
                        .join(", ")
                )
            }
            TypeInfo::Array { element, size } => match size {
                Some(n) => format!("[{}; {}]", element.display(), n),
                None => format!("[{}]", element.display()),
            },
            TypeInfo::Slice(element) => format!("[{}]", element.display()),
            TypeInfo::AssociatedType {
                trait_name,
                type_name,
            } => {
                format!("<{}>::{}", trait_name, type_name)
            }
            TypeInfo::TraitObject {
                trait_name,
                lifetime,
            } => {
                let lt = lifetime
                    .as_deref()
                    .map(|l| format!(" + '{l}"))
                    .unwrap_or_default();
                format!("dyn {}{}", trait_name, lt)
            }
            TypeInfo::TypeParam { name, bounds } => {
                if bounds.is_empty() {
                    name.clone()
                } else {
                    format!(
                        "{}: {}",
                        name,
                        bounds
                            .iter()
                            .map(|b| b.display())
                            .collect::<Vec<_>>()
                            .join(" + ")
                    )
                }
            }
            TypeInfo::Unknown => "_".to_string(),
        }
    }

    pub fn is_numeric(&self) -> bool {
        matches!(
            self,
            TypeInfo::Primitive(
                PrimitiveType::I8
                    | PrimitiveType::I16
                    | PrimitiveType::I32
                    | PrimitiveType::I64
                    | PrimitiveType::I128
                    | PrimitiveType::ISize
                    | PrimitiveType::U8
                    | PrimitiveType::U16
                    | PrimitiveType::U32
                    | PrimitiveType::U64
                    | PrimitiveType::U128
                    | PrimitiveType::USize
                    | PrimitiveType::F32
                    | PrimitiveType::F64
            )
        )
    }

    pub fn is_collection(&self) -> bool {
        matches!(
            self,
            TypeInfo::Generic { base, .. } if matches!(
                base.as_str(),
                "Vec" | "HashMap" | "BTreeMap" | "HashSet" | "BTreeSet"
                    | "VecDeque" | "LinkedList" | "list" | "dict" | "set"
                    | "tuple" | "List" | "Map"
            )
        ) || matches!(self, TypeInfo::Slice(_) | TypeInfo::Array { .. })
    }

    pub fn inner_type(&self) -> Option<&TypeInfo> {
        match self {
            TypeInfo::Generic { args, .. } => args.first(),
            TypeInfo::Slice(elem) => Some(elem),
            TypeInfo::Array { element, .. } => Some(element),
            TypeInfo::Reference { inner, .. } => Some(inner),
            _ => None,
        }
    }

    pub fn base_name(&self) -> &str {
        match self {
            TypeInfo::Primitive(p) => p.as_str(),
            TypeInfo::Generic { base, .. } => base,
            TypeInfo::Function { .. } => "fn",
            TypeInfo::TraitBound { name, .. } => name,
            TypeInfo::Lifetime(_) => "lifetime",
            TypeInfo::Reference { .. } => "ref",
            TypeInfo::Tuple(_) => "tuple",
            TypeInfo::Array { .. } => "array",
            TypeInfo::Slice(_) => "slice",
            TypeInfo::AssociatedType { trait_name, .. } => trait_name,
            TypeInfo::TraitObject { trait_name, .. } => trait_name,
            TypeInfo::TypeParam { name, .. } => name,
            TypeInfo::Unknown => "_",
        }
    }

    /// Check if this type is a reference to the given base type.
    pub fn is_ref_to(&self, base: &str) -> bool {
        match self {
            TypeInfo::Reference { inner, .. } => inner.base_name() == base,
            _ => false,
        }
    }
}

/// Check if a TypeInfo is a type parameter (T, E, K, V).
fn is_type_param(ty: &TypeInfo) -> bool {
    match ty {
        TypeInfo::TypeParam { .. } => true,
        TypeInfo::Generic { base, args } if args.is_empty() => {
            let chars: Vec<char> = base.chars().collect();
            chars.len() <= 2 && chars.first().map(|c| c.is_uppercase()).unwrap_or(false)
        }
        _ => false,
    }
}

/// Check if output type of `producer` can feed into input type of `consumer`.
/// This is the CORE of type-directed edge construction.
///
/// Rules (in priority order):
/// 1. Exact match: Vec<i32> == Vec<i32> → Exact
/// 2. TypeParam match: T matches anything → GenericMatch
/// 3. Generic structural match: Vec<T> matches Vec<i32> → GenericMatch
/// 4. Reference unwrap: &T → T → RefUnwrap
/// 5. Vec → Iterator/IntoIterator → Unwrap (Vec<T> implements IntoIterator)
/// 6. Option unwrap: Option<T> → T → OptionUnwrap (with None risk)
/// 7. Unknown: _ matches anything → Wildcard
///
/// NOTE: Numeric widening is NOT included — Rust requires explicit .into().
pub fn types_compose(producer: &TypeInfo, consumer: &TypeInfo) -> ComposeResult {
    if producer == consumer {
        return ComposeResult::Exact;
    }

    if matches!(producer, TypeInfo::Unknown) || matches!(consumer, TypeInfo::Unknown) {
        return ComposeResult::Wildcard;
    }

    // TypeParam matches anything (unification)
    if is_type_param(consumer) {
        return ComposeResult::GenericMatch;
    }
    if is_type_param(producer) {
        return ComposeResult::GenericMatch;
    }

    // Reference → T: &Vec<T> can feed into Vec<T> (deref)
    if let TypeInfo::Reference { inner, .. } = producer {
        let inner_result = types_compose(inner, consumer);
        if inner_result.can_compose() {
            return ComposeResult::RefUnwrap;
        }
    }
    // T → &T: producing T, consuming &T (borrow)
    if let TypeInfo::Reference { inner, .. } = consumer {
        let inner_result = types_compose(producer, inner);
        if inner_result.can_compose() {
            return ComposeResult::RefBorrow;
        }
    }

    // Generic structural match with recursive type param unification
    if let (TypeInfo::Generic { base: pb, args: pa }, TypeInfo::Generic { base: cb, args: ca }) =
        (producer, consumer)
    {
        if pb == cb && pa.len() == ca.len() {
            let all_match = pa.iter().zip(ca.iter()).all(|(p, c)| {
                let r = types_compose(p, c);
                matches!(
                    r,
                    ComposeResult::Exact | ComposeResult::Wildcard | ComposeResult::GenericMatch
                )
            });
            if all_match {
                return ComposeResult::GenericMatch;
            }
        }

        // Vec<T> → Iterator<Item=T>: Vec implements IntoIterator
        if pb == "Vec" && (cb == "Iterator" || cb == "IntoIterator") {
            return ComposeResult::Unwrap;
        }
        // Vec<T> → &[T]: Vec derefs to slice
        if pb == "Vec" && matches!(consumer, TypeInfo::Slice(_)) {
            return ComposeResult::Unwrap;
        }
    }

    // Vec<T> → Slice<T>
    if let (TypeInfo::Generic { base: pb, args: pa }, TypeInfo::Slice(elem)) = (producer, consumer)
    {
        if pb == "Vec" && pa.len() == 1 {
            let r = types_compose(&pa[0], elem);
            if r.can_compose() {
                return ComposeResult::Unwrap;
            }
        }
    }

    // Iterator<T> → Vec<T> (collect pattern)
    if let (TypeInfo::Generic { base: pb, args: pa }, TypeInfo::Generic { base: cb, args: ca }) =
        (producer, consumer)
    {
        if (pb == "Iterator" || pb == "IntoIterator") && cb == "Vec" {
            return ComposeResult::Unwrap;
        }
        // Iterator<T> → &[T] (collect to slice is not direct, but as_ref works)
        if (pb == "Iterator" || pb == "IntoIterator") && cb == "Slice" {
            return ComposeResult::Unwrap;
        }
        // Vec<T> → VecDeque<T>, HashSet<T>, BTreeSet<T> (collection conversion via collect)
        if pb == "Vec"
            && matches!(
                cb.as_str(),
                "VecDeque" | "HashSet" | "BTreeSet" | "LinkedList"
            )
        {
            return ComposeResult::Unwrap;
        }
        // Result<T,E> → Option<T> (ok() method)
        if pb == "Result" && cb == "Option" && pa.len() == 2 && ca.len() == 1 {
            let r = types_compose(&pa[0], &ca[0]);
            if r.can_compose() {
                return ComposeResult::Unwrap;
            }
        }
    }

    // Option<T> → T (unwrap with None risk)
    if let TypeInfo::Generic { base, args } = producer {
        if base == "Option"
            && args.len() == 1
            && types_compose(&args[0], consumer) != ComposeResult::NoMatch
        {
            return ComposeResult::OptionUnwrap;
        }
    }

    // Result<T,E> → T (unwrap Ok, with Err risk)
    if let TypeInfo::Generic { base, args } = producer {
        if base == "Result"
            && !args.is_empty()
            && types_compose(&args[0], consumer) != ComposeResult::NoMatch
        {
            return ComposeResult::OptionUnwrap;
        }
    }

    // Tuple → element (accessing a tuple element)
    if let TypeInfo::Tuple(items) = producer {
        for item in items {
            if types_compose(item, consumer) != ComposeResult::NoMatch {
                return ComposeResult::Unwrap;
            }
        }
    }

    // Array [T; N] → Slice [T]
    if let (TypeInfo::Array { element, .. }, TypeInfo::Slice(elem)) = (producer, consumer) {
        let r = types_compose(element, elem);
        if r.can_compose() {
            return ComposeResult::Unwrap;
        }
    }
    // Array [T; N] → Vec<T>
    if let (TypeInfo::Array { element, .. }, TypeInfo::Generic { base, args }) =
        (producer, consumer)
    {
        if base == "Vec" && args.len() == 1 {
            let r = types_compose(element, &args[0]);
            if r.can_compose() {
                return ComposeResult::Unwrap;
            }
        }
    }

    // TraitBound matches if consumer type implements the trait (heuristic: name match)
    if let TypeInfo::TraitBound { name, .. } = consumer {
        if producer.base_name() == name {
            return ComposeResult::GenericMatch;
        }
    }

    // AssociatedType: <Iterator>::Item matches the inner type
    if let TypeInfo::AssociatedType {
        trait_name,
        type_name,
    } = consumer
    {
        if trait_name == "Iterator" && type_name == "Item" {
            // Iterator::Item matches any concrete type
            return ComposeResult::GenericMatch;
        }
    }

    // TraitObject: dyn Trait matches any type implementing Trait (heuristic)
    if let TypeInfo::TraitObject { trait_name, .. } = consumer {
        if producer.base_name() == trait_name {
            return ComposeResult::GenericMatch;
        }
    }

    ComposeResult::NoMatch
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComposeResult {
    Exact,
    GenericMatch,
    Wildcard,
    Unwrap,
    OptionUnwrap,
    RefUnwrap,
    RefBorrow,
    NoMatch,
}

impl ComposeResult {
    pub fn can_compose(&self) -> bool {
        !matches!(self, ComposeResult::NoMatch)
    }

    pub fn confidence(&self) -> f64 {
        match self {
            ComposeResult::Exact => 1.0,
            ComposeResult::GenericMatch => 0.9,
            ComposeResult::RefBorrow => 0.85,
            ComposeResult::Unwrap => 0.8,
            ComposeResult::RefUnwrap => 0.75,
            ComposeResult::OptionUnwrap => 0.5,
            ComposeResult::Wildcard => 0.3,
            ComposeResult::NoMatch => 0.0,
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            ComposeResult::Exact => "exact",
            ComposeResult::GenericMatch => "generic",
            ComposeResult::Unwrap => "unwrap",
            ComposeResult::OptionUnwrap => "option-unwrap",
            ComposeResult::RefUnwrap => "ref-unwrap",
            ComposeResult::RefBorrow => "ref-borrow",
            ComposeResult::Wildcard => "wildcard",
            ComposeResult::NoMatch => "no-match",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_exact_match() {
        let a = TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32));
        assert_eq!(types_compose(&a, &a), ComposeResult::Exact);
    }

    #[test]
    fn test_generic_match() {
        let concrete = TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32));
        let generic = TypeInfo::vec(TypeInfo::type_param("T"));
        assert_eq!(
            types_compose(&concrete, &generic),
            ComposeResult::GenericMatch
        );
    }

    #[test]
    fn test_option_unwrap() {
        let opt = TypeInfo::option(TypeInfo::Primitive(PrimitiveType::I32));
        let bare = TypeInfo::Primitive(PrimitiveType::I32);
        assert_eq!(types_compose(&opt, &bare), ComposeResult::OptionUnwrap);
    }

    #[test]
    fn test_no_match() {
        let a = TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32));
        let b = TypeInfo::string();
        assert_eq!(types_compose(&a, &b), ComposeResult::NoMatch);
    }

    #[test]
    fn test_numeric_no_implicit_widening() {
        let i32_t = TypeInfo::Primitive(PrimitiveType::I32);
        let f64_t = TypeInfo::Primitive(PrimitiveType::F64);
        assert_eq!(
            types_compose(&i32_t, &f64_t),
            ComposeResult::NoMatch,
            "Rust does not implicitly widen — i32 → f64 requires explicit .into()"
        );
    }

    #[test]
    fn test_ref_unwrap() {
        let ref_vec = TypeInfo::ref_to(TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32)));
        let vec_t = TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32));
        assert_eq!(types_compose(&ref_vec, &vec_t), ComposeResult::RefUnwrap);
    }

    #[test]
    fn test_ref_borrow() {
        let vec_t = TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32));
        let ref_vec = TypeInfo::ref_to(TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32)));
        assert_eq!(types_compose(&vec_t, &ref_vec), ComposeResult::RefBorrow);
    }

    #[test]
    fn test_vec_to_iterator() {
        let vec_t = TypeInfo::vec(TypeInfo::type_param("T"));
        let iter_t = TypeInfo::Generic {
            base: "Iterator".into(),
            args: vec![TypeInfo::type_param("T")],
        };
        assert_eq!(types_compose(&vec_t, &iter_t), ComposeResult::Unwrap);
    }

    #[test]
    fn test_vec_to_slice() {
        let vec_t = TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32));
        let slice_t = TypeInfo::Slice(Box::new(TypeInfo::Primitive(PrimitiveType::I32)));
        assert_eq!(types_compose(&vec_t, &slice_t), ComposeResult::Unwrap);
    }

    #[test]
    fn test_display_ref() {
        assert_eq!(TypeInfo::ref_to(TypeInfo::string()).display(), "&String");
        assert_eq!(
            TypeInfo::mut_ref_to(TypeInfo::string()).display(),
            "&mut String"
        );
    }

    #[test]
    fn test_display_tuple() {
        assert_eq!(
            TypeInfo::tuple(vec![
                TypeInfo::Primitive(PrimitiveType::I32),
                TypeInfo::string()
            ])
            .display(),
            "(i32, String)"
        );
    }

    #[test]
    fn test_display_slice() {
        assert_eq!(
            TypeInfo::slice_of(TypeInfo::Primitive(PrimitiveType::I32)).display(),
            "[i32]"
        );
    }

    #[test]
    fn test_wildcard() {
        let any = TypeInfo::Unknown;
        let concrete = TypeInfo::string();
        assert_eq!(types_compose(&any, &concrete), ComposeResult::Wildcard);
        assert_eq!(types_compose(&concrete, &any), ComposeResult::Wildcard);
    }

    #[test]
    fn test_display() {
        assert_eq!(
            TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32)).display(),
            "Vec<i32>"
        );
        assert_eq!(
            TypeInfo::option(TypeInfo::string()).display(),
            "Option<String>"
        );
        assert_eq!(
            TypeInfo::result(TypeInfo::string(), TypeInfo::type_param("E")).display(),
            "Result<String, E>"
        );
    }

    #[test]
    fn test_is_collection() {
        assert!(TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32)).is_collection());
        assert!(!TypeInfo::string().is_collection());
    }

    #[test]
    fn test_confidence_ordering() {
        assert!(ComposeResult::Exact.confidence() > ComposeResult::GenericMatch.confidence());
        assert!(ComposeResult::GenericMatch.confidence() > ComposeResult::Unwrap.confidence());
        assert!(ComposeResult::Unwrap.confidence() > ComposeResult::OptionUnwrap.confidence());
        assert!(ComposeResult::NoMatch.confidence() == 0.0);
    }

    #[test]
    fn test_iterator_to_vec() {
        let iter = TypeInfo::Generic {
            base: "Iterator".into(),
            args: vec![TypeInfo::type_param("T")],
        };
        let vec = TypeInfo::vec(TypeInfo::type_param("T"));
        assert_eq!(types_compose(&iter, &vec), ComposeResult::Unwrap);
    }

    #[test]
    fn test_result_to_option() {
        let result = TypeInfo::result(TypeInfo::string(), TypeInfo::type_param("E"));
        let option = TypeInfo::option(TypeInfo::string());
        assert_eq!(types_compose(&result, &option), ComposeResult::Unwrap);
    }

    #[test]
    fn test_result_to_t() {
        let result = TypeInfo::result(TypeInfo::string(), TypeInfo::type_param("E"));
        let bare = TypeInfo::string();
        assert_eq!(types_compose(&result, &bare), ComposeResult::OptionUnwrap);
    }

    #[test]
    fn test_tuple_element_access() {
        let tuple = TypeInfo::tuple(vec![
            TypeInfo::string(),
            TypeInfo::Primitive(PrimitiveType::I32),
        ]);
        let str_t = TypeInfo::string();
        assert_eq!(types_compose(&tuple, &str_t), ComposeResult::Unwrap);
    }

    #[test]
    fn test_array_to_slice() {
        let array = TypeInfo::Array {
            element: Box::new(TypeInfo::Primitive(PrimitiveType::I32)),
            size: Some(10),
        };
        let slice = TypeInfo::Slice(Box::new(TypeInfo::Primitive(PrimitiveType::I32)));
        assert_eq!(types_compose(&array, &slice), ComposeResult::Unwrap);
    }

    #[test]
    fn test_array_to_vec() {
        let array = TypeInfo::Array {
            element: Box::new(TypeInfo::Primitive(PrimitiveType::I32)),
            size: Some(10),
        };
        let vec = TypeInfo::vec(TypeInfo::Primitive(PrimitiveType::I32));
        assert_eq!(types_compose(&array, &vec), ComposeResult::Unwrap);
    }

    #[test]
    fn test_vec_to_vecdeque() {
        let vec = TypeInfo::vec(TypeInfo::type_param("T"));
        let deque = TypeInfo::Generic {
            base: "VecDeque".into(),
            args: vec![TypeInfo::type_param("T")],
        };
        assert_eq!(types_compose(&vec, &deque), ComposeResult::Unwrap);
    }
}

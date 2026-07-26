//! Registry builder — loads Kimi-generated batch data into CodeGraph.
//!
//! Since Kimi batches are separate crate files with their own schema,
//! we define the atom/relation data inline here using `convert_atom`.
//! In production, this would be generated from the batch crate's `build_batch()`.

use crate::batch_loader::{convert_atom, convert_relation, AtomInput};
use crate::code_schema::{CodeAtomKind, CodeGraph, CodeLang, CodeRelationType};
use crate::type_edges::TypeEdgeBuilder;

/// Build a CodeGraph from BATCH 1 (Rust Collections) + BATCH 2 (Iterator ecosystem).
/// Returns a fully typed graph with type-directed edges.
pub fn build_rust_registry() -> CodeGraph {
    let mut graph = CodeGraph::new();

    // === BATCH 1: Rust Collections (50 atoms) ===
    let rust = CodeLang::Rust;

    // Vec methods
    let b1_atoms: &[AtomInput] = &[
        ai(
            rust,
            CodeAtomKind::Struct,
            "Vec",
            "std::vec",
            "pub struct Vec<T>",
            "A contiguous growable array",
            &["collection", "array", "contiguous", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::new",
            "std::vec",
            "pub fn new() -> Vec<T>",
            "Creates an empty Vec",
            &["create", "empty", "O(1)", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::with_capacity",
            "std::vec",
            "pub fn with_capacity(capacity: usize) -> Vec<T>",
            "Creates empty Vec with capacity",
            &["create", "capacity", "O(1)", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::push",
            "std::vec",
            "pub fn push(&mut self, value: T)",
            "Appends element to back",
            &["insert", "add", "append", "O(1) amortized", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::pop",
            "std::vec",
            "pub fn pop(&mut self) -> Option<T>",
            "Removes and returns last element",
            &["remove", "last", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::insert",
            "std::vec",
            "pub fn insert(&mut self, index: usize, element: T)",
            "Inserts element at index",
            &["insert", "O(n)", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::remove",
            "std::vec",
            "pub fn remove(&mut self, index: usize) -> T",
            "Removes and returns element at index",
            &["remove", "delete", "O(n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::get",
            "std::vec",
            "pub fn get(&self, index: usize) -> Option<&T>",
            "Returns reference to element at index",
            &["get", "lookup", "index", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::contains",
            "std::vec",
            "pub fn contains(&self, x: &T) -> bool where T: PartialEq",
            "Checks if element exists",
            &["check", "contains", "search", "O(n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::iter",
            "std::vec",
            "pub fn iter(&self) -> Iter<'_, T>",
            "Returns iterator over elements",
            &["iter", "loop", "lazy", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::iter_mut",
            "std::vec",
            "pub fn iter_mut(&mut self) -> IterMut<'_, T>",
            "Returns mutable iterator",
            &["iter", "mut", "loop", "lazy", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::into_iter",
            "std::vec",
            "pub fn into_iter(self) -> IntoIter<T>",
            "Consumes Vec into iterator",
            &["iter", "consume", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::len",
            "std::vec",
            "pub fn len(&self) -> usize",
            "Returns number of elements",
            &["len", "count", "size", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::is_empty",
            "std::vec",
            "pub fn is_empty(&self) -> bool",
            "Returns true if empty",
            &["check", "empty", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::clear",
            "std::vec",
            "pub fn clear(&mut self)",
            "Clears all elements",
            &["remove", "clear", "empty", "O(n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::extend",
            "std::vec",
            "pub fn extend<I: IntoIterator<Item = T>>(&mut self, iter: I)",
            "Extends from iterator",
            &["extend", "add", "O(n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::retain",
            "std::vec",
            "pub fn retain<F: FnMut(&T) -> bool>(&mut self, f: F)",
            "Keeps elements matching predicate",
            &["filter", "retain", "O(n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::sort",
            "std::vec",
            "pub fn sort(&mut self) where T: Ord",
            "Sorts in-place, stable",
            &["sort", "in-place", "stable", "O(n log n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::sort_unstable",
            "std::vec",
            "pub fn sort_unstable(&mut self) where T: Ord",
            "Sorts in-place, unstable",
            &["sort", "in-place", "unstable", "O(n log n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::sort_by",
            "std::vec",
            "pub fn sort_by<F: FnMut(&T, &T) -> Ordering>(&mut self, compare: F)",
            "Sorts with comparator",
            &["sort", "comparator", "stable", "O(n log n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::sort_by_key",
            "std::vec",
            "pub fn sort_by_key<K: Ord, F: FnMut(&T) -> K>(&mut self, f: F)",
            "Sorts by key function",
            &["sort", "key", "stable", "O(n log n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::binary_search",
            "std::vec",
            "pub fn binary_search(&self, x: &T) -> Result<usize, usize> where T: Ord",
            "Binary search sorted Vec",
            &["search", "binary", "find", "O(log n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::windows",
            "std::vec",
            "pub fn windows(&self, size: usize) -> Windows<'_, T>",
            "Sliding window iterator",
            &["window", "slice", "lazy"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::chunks",
            "std::vec",
            "pub fn chunks(&self, chunk_size: usize) -> Chunks<'_, T>",
            "Chunks iterator",
            &["chunk", "split", "lazy"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::split",
            "std::vec",
            "pub fn split<P: FnMut(&T) -> bool>(&self, pred: P) -> Split<'_, T, P>",
            "Split at matching predicate",
            &["split", "partition"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::split_at",
            "std::vec",
            "pub fn split_at(&self, mid: usize) -> (&[T], &[T])",
            "Split at index into two slices",
            &["split", "partition", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::join",
            "std::vec",
            "pub fn join<Separator: Display>(&self, sep: Separator) -> String where T: Display",
            "Join elements with separator",
            &["join", "concat", "string"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::concat",
            "std::vec",
            "pub fn concat(&self) -> Vec<T> where T: Clone",
            "Concatenate nested Vecs",
            &["concat", "flatten"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::dedup",
            "std::vec",
            "pub fn dedup(&mut self) where T: PartialEq",
            "Remove consecutive duplicates",
            &["dedup", "unique", "O(n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::truncate",
            "std::vec",
            "pub fn truncate(&mut self, len: usize)",
            "Shorten to len elements",
            &["truncate", "shorten", "O(n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::swap_remove",
            "std::vec",
            "pub fn swap_remove(&mut self, index: usize) -> T",
            "Remove via swap with last",
            &["remove", "swap", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::drain",
            "std::vec",
            "pub fn drain<R: RangeBounds<usize>>(&mut self, range: R) -> Drain<'_, T>",
            "Remove range as iterator",
            &["remove", "drain", "range"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::first",
            "std::vec",
            "pub fn first(&self) -> Option<&T>",
            "Returns first element",
            &["first", "find", "O(1)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "Vec::last",
            "std::vec",
            "pub fn last(&self) -> Option<&T>",
            "Returns last element",
            &["last", "find", "O(1)"],
        ),
        // Note: Vec does NOT have inherent max()/min() — use iter().max() / iter().min()
        // HashMap
        ai(
            rust,
            CodeAtomKind::Struct,
            "HashMap",
            "std::collections",
            "pub struct HashMap<K, V>",
            "Hash map",
            &["collection", "map", "hash", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "HashMap::insert",
            "std::collections",
            "pub fn insert(&mut self, k: K, v: V) -> Option<V>",
            "Insert key-value",
            &["insert", "add", "set", "O(1) amortized"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "HashMap::get",
            "std::collections",
            "pub fn get<Q>(&self, k: &Q) -> Option<&V> where K: Borrow<Q>",
            "Get value by key",
            &["get", "lookup", "O(1) amortized"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "HashMap::contains_key",
            "std::collections",
            "pub fn contains_key<Q>(&self, k: &Q) -> bool",
            "Check if key exists",
            &["check", "contains", "key", "O(1) amortized"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "HashMap::entry",
            "std::collections",
            "pub fn entry(&mut self, key: K) -> Entry<'_, K, V>",
            "Entry API for insert-or-update",
            &["entry", "insert", "update"],
        ),
        // BTreeMap
        ai(
            rust,
            CodeAtomKind::Struct,
            "BTreeMap",
            "std::collections",
            "pub struct BTreeMap<K, V>",
            "Sorted map",
            &["collection", "map", "sorted", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "BTreeMap::insert",
            "std::collections",
            "pub fn insert(&mut self, key: K, value: V) -> Option<V>",
            "Insert key-value",
            &["insert", "add", "set", "O(log n)"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "BTreeMap::get",
            "std::collections",
            "pub fn get<Q>(&self, key: &Q) -> Option<&V>",
            "Get value by key",
            &["get", "lookup", "O(log n)"],
        ),
        // HashSet
        ai(
            rust,
            CodeAtomKind::Struct,
            "HashSet",
            "std::collections",
            "pub struct HashSet<T>",
            "Hash set",
            &["collection", "set", "unique", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "HashSet::insert",
            "std::collections",
            "pub fn insert(&mut self, value: T) -> bool",
            "Insert value, return if new",
            &["insert", "add", "unique", "O(1) amortized"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "HashSet::contains",
            "std::collections",
            "pub fn contains<Q>(&self, value: &Q) -> bool",
            "Check if value exists",
            &["check", "contains", "O(1) amortized"],
        ),
        // String
        ai(
            rust,
            CodeAtomKind::Struct,
            "String",
            "std::string",
            "pub struct String",
            "UTF-8 growable string",
            &["string", "text", "utf8", "alloc"],
        ),
        ai(
            rust,
            CodeAtomKind::Method,
            "String::push_str",
            "std::string",
            "pub fn push_str(&mut self, string: &str)",
            "Appends string slice",
            &["append", "concat", "string", "O(n)"],
        ),
        // Trait
        ai(
            rust,
            CodeAtomKind::Trait,
            "Ord",
            "std::cmp",
            "pub trait Ord: Eq + PartialOrd<Self>",
            "Total ordering",
            &["ord", "compare", "order", "trait"],
        ),
        ai(
            rust,
            CodeAtomKind::Trait,
            "PartialEq",
            "std::cmp",
            "pub trait PartialEq<Rhs: ?Sized>",
            "Partial equality",
            &["eq", "compare", "equality", "trait"],
        ),
    ];

    for input in b1_atoms {
        let atom = convert_atom(input);
        graph.add_atom(atom);
    }

    // === BATCH 2: Iterator ecosystem (48 atoms) ===
    let b2_atoms: &[AtomInput] = &[
        ai(rust, CodeAtomKind::Method, "Iterator::next", "std::iter", "pub fn next(&mut self) -> Option<Self::Item>", "Advance iterator, return next item", &["iter", "next", "advance", "O(1)"]),
        ai(rust, CodeAtomKind::Method, "Iterator::map", "std::iter", "pub fn map<B, F: FnMut(Self::Item) -> B>(self, f: F) -> Map<Self, F>", "Transform each element", &["map", "transform", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::filter", "std::iter", "pub fn filter<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> Filter<Self, P>", "Keep elements matching predicate", &["filter", "where", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::filter_map", "std::iter", "pub fn filter_map<B, F: FnMut(Self::Item) -> Option<B>>(self, f: F) -> FilterMap<Self, F>", "Filter + map in one step", &["filter", "map", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::flatten", "std::iter", "pub fn flatten(self) -> Flatten<Self>", "Flatten nested iterators", &["flatten", "concat", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::flat_map", "std::iter", "pub fn flat_map<U, F: FnMut(Self::Item) -> U>(self, f: F) -> FlatMap<Self, U, F> where U: IntoIterator", "Map then flatten", &["map", "flatten", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::zip", "std::iter", "pub fn zip<U: IntoIterator>(self, other: U) -> Zip<Self, U::IntoIter>", "Pair elements from two iterators", &["zip", "pair", "combine", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::enumerate", "std::iter", "pub fn enumerate(self) -> Enumerate<Self>", "Pair each element with its index", &["enumerate", "index", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::rev", "std::iter", "pub fn rev(self) -> Rev<Self> where Self: DoubleEndedIterator", "Reverse iterator", &["reverse", "rev", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::skip", "std::iter", "pub fn skip(self, n: usize) -> Skip<Self>", "Skip first n elements", &["skip", "drop", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::take", "std::iter", "pub fn take(self, n: usize) -> Take<Self>", "Take first n elements", &["take", "limit", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::take_while", "std::iter", "pub fn take_while<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> TakeWhile<Self, P>", "Take while predicate is true", &["take", "while", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::skip_while", "std::iter", "pub fn skip_while<P: FnMut(&Self::Item) -> bool>(self, predicate: P) -> SkipWhile<Self, P>", "Skip while predicate is true", &["skip", "while", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::chain", "std::iter", "pub fn chain<U: IntoIterator<Item = Self::Item>>(self, other: U) -> Chain<Self, U::IntoIter>", "Chain two iterators", &["chain", "concat", "combine", "lazy"]),
        ai(rust, CodeAtomKind::Method, "Iterator::collect", "std::iter", "pub fn collect<B: FromIterator<Self::Item>>(self) -> B", "Collect into collection", &["collect", "eager", "materialize"]),
        ai(rust, CodeAtomKind::Method, "Iterator::fold", "std::iter", "pub fn fold<B, F: FnMut(B, Self::Item) -> B>(self, init: B, f: F) -> B", "Fold into accumulator", &["fold", "reduce", "accumulate", "eager"]),
        ai(rust, CodeAtomKind::Method, "Iterator::reduce", "std::iter", "pub fn reduce<F: FnMut(Self::Item, Self::Item) -> Self::Item>(self, f: F) -> Option<Self::Item>", "Reduce with first element as init", &["reduce", "fold", "accumulate", "eager"]),
        ai(rust, CodeAtomKind::Method, "Iterator::min", "std::iter", "pub fn min(self) -> Option<Self::Item> where Self::Item: Ord", "Minimum element", &["min", "find", "eager", "O(n)"]),
        ai(rust, CodeAtomKind::Method, "Iterator::max", "std::iter", "pub fn max(self) -> Option<Self::Item> where Self::Item: Ord", "Maximum element", &["max", "find", "eager", "O(n)"]),
        ai(rust, CodeAtomKind::Method, "Iterator::sum", "std::iter", "pub fn sum<S: Sum<Self::Item>>(self) -> S", "Sum all elements", &["sum", "aggregate", "eager", "O(n)"]),
        ai(rust, CodeAtomKind::Method, "Iterator::product", "std::iter", "pub fn product<P: Product<Self::Item>>(self) -> P", "Product of all elements", &["product", "aggregate", "eager", "O(n)"]),
        ai(rust, CodeAtomKind::Method, "Iterator::count", "std::iter", "pub fn count(self) -> usize", "Count elements", &["count", "len", "eager", "O(n)"]),
        ai(rust, CodeAtomKind::Method, "Iterator::any", "std::iter", "pub fn any<F: FnMut(Self::Item) -> bool>(&mut self, f: F) -> bool", "True if any element matches", &["any", "check", "exists", "eager"]),
        ai(rust, CodeAtomKind::Method, "Iterator::all", "std::iter", "pub fn all<F: FnMut(Self::Item) -> bool>(&mut self, f: F) -> bool", "True if all elements match", &["all", "check", "eager"]),
        ai(rust, CodeAtomKind::Method, "Iterator::find", "std::iter", "pub fn find<P: FnMut(&Self::Item) -> bool>(&mut self, predicate: P) -> Option<Self::Item>", "Find first matching element", &["find", "search", "eager"]),
        // Traits
        ai(rust, CodeAtomKind::Trait, "Iterator", "std::iter", "pub trait Iterator", "Iterator trait", &["iter", "trait", "lazy"]),
        ai(rust, CodeAtomKind::Trait, "IntoIterator", "std::iter", "pub trait IntoIterator", "Convert into iterator", &["iter", "into", "trait"]),
        ai(rust, CodeAtomKind::Trait, "DoubleEndedIterator", "std::iter", "pub trait DoubleEndedIterator: Iterator", "Bidirectional iterator", &["iter", "reverse", "trait"]),
        ai(rust, CodeAtomKind::Trait, "ExactSizeIterator", "std::iter", "pub trait ExactSizeIterator: Iterator", "Iterator with known length", &["iter", "len", "exact", "trait"]),
        ai(rust, CodeAtomKind::Trait, "FusedIterator", "std::iter", "pub trait FusedIterator: Iterator", "Iterator that returns None forever after first None", &["iter", "fused", "trait"]),
        // Collect patterns
        ai(rust, CodeAtomKind::Method, "Vec::from_iter", "std::vec", "pub fn from_iter<I: IntoIterator<Item = T>>(iter: I) -> Vec<T>", "Create Vec from iterator", &["collect", "from", "iter"]),
        ai(rust, CodeAtomKind::Method, "Vec::extend_from_slice", "std::vec", "pub fn extend_from_slice(&mut self, other: &[T]) where T: Clone", "Extend from slice", &["extend", "append", "slice"]),
        // Iterator constructors
        ai(rust, CodeAtomKind::Method, "iter::repeat", "std::iter", "pub fn repeat<T: Clone>(element: T) -> Repeat<T>", "Infinite repeat", &["repeat", "infinite", "lazy"]),
        ai(rust, CodeAtomKind::Method, "iter::once", "std::iter", "pub fn once<T>(value: T) -> Once<T>", "Single element iterator", &["once", "single", "lazy"]),
        ai(rust, CodeAtomKind::Method, "iter::empty", "std::iter", "pub fn empty<T>() -> Empty<T>", "Empty iterator", &["empty", "lazy"]),
        ai(rust, CodeAtomKind::Method, "iter::successors", "std::iter", "pub fn successors<T, F: FnMut(&T) -> Option<T>>(first: Option<T>, succ: F) -> Successors<T, F>", "Successor function iterator", &["successors", "unfold", "lazy"]),
        // Additional traits for relations
        ai(rust, CodeAtomKind::Trait, "Sum", "std::iter", "pub trait Sum<A = Self>: Sized", "Sum trait", &["sum", "aggregate", "trait"]),
        ai(rust, CodeAtomKind::Trait, "Product", "std::iter", "pub trait Product<A = Self>: Sized", "Product trait", &["product", "aggregate", "trait"]),
        ai(rust, CodeAtomKind::Trait, "FromIterator", "std::iter", "pub trait FromIterator<A>: Sized", "Build collection from iterator", &["collect", "from", "iter", "trait"]),
        ai(rust, CodeAtomKind::Trait, "FnMut", "std::ops", "pub trait FnMut<Args>: FnOnce<Args>", "Mutable closure trait", &["closure", "fnmut", "trait"]),
        // VecDeque
        ai(rust, CodeAtomKind::Struct, "VecDeque", "std::collections", "pub struct VecDeque<T>", "Double-ended queue", &["collection", "deque", "queue", "alloc"]),
        // LinkedList
        ai(rust, CodeAtomKind::Struct, "LinkedList", "std::collections", "pub struct LinkedList<T>", "Doubly linked list", &["collection", "linked", "list", "alloc"]),
        // BTreeSet
        ai(rust, CodeAtomKind::Struct, "BTreeSet", "std::collections", "pub struct BTreeSet<T>", "Sorted set", &["collection", "set", "sorted", "alloc"]),
        // str
        ai(rust, CodeAtomKind::Method, "str::split", "std::str", "pub fn split<P: Pattern>(&self, pat: P) -> Split<'_, P>", "Split string by pattern", &["split", "string", "lazy"]),
        ai(rust, CodeAtomKind::Method, "str::lines", "std::str", "pub fn lines(&self) -> Lines<'_>", "Iterator over lines", &["lines", "string", "lazy"]),
        ai(rust, CodeAtomKind::Method, "str::chars", "std::str", "pub fn chars(&self) -> Chars<'_>", "Iterator over chars", &["chars", "string", "lazy"]),
        ai(rust, CodeAtomKind::Method, "str::trim", "std::str", "pub fn trim(&self) -> &str", "Strip whitespace", &["trim", "strip", "string", "O(n)"]),
        ai(rust, CodeAtomKind::Method, "str::parse", "std::str", "pub fn parse<F: FromStr>(&self) -> Result<F, F::Err>", "Parse string to type", &["parse", "convert", "string", "error"]),
    ];

    for input in b2_atoms {
        let atom = convert_atom(input);
        graph.add_atom(atom);
    }

    // === Relations ===
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::sort",
        "rust::std::vec::Vec::sort_unstable",
        CodeRelationType::RelAlternative,
        rust,
        "stable vs unstable sort",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::sort_unstable",
        "rust::std::vec::Vec::sort",
        CodeRelationType::RelAlternative,
        rust,
        "unstable vs stable sort",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::sort",
        "rust::std::cmp::Ord",
        CodeRelationType::RelRequires,
        rust,
        "sort requires Ord",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::sort_unstable",
        "rust::std::cmp::Ord",
        CodeRelationType::RelRequires,
        rust,
        "sort_unstable requires Ord",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::max",
        "rust::std::cmp::Ord",
        CodeRelationType::RelRequires,
        rust,
        "Iterator::max requires Ord",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::min",
        "rust::std::cmp::Ord",
        CodeRelationType::RelRequires,
        rust,
        "Iterator::min requires Ord",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::binary_search",
        "rust::std::cmp::Ord",
        CodeRelationType::RelRequires,
        rust,
        "binary_search requires Ord",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::contains",
        "rust::std::cmp::PartialEq",
        CodeRelationType::RelRequires,
        rust,
        "contains requires PartialEq",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::dedup",
        "rust::std::cmp::PartialEq",
        CodeRelationType::RelRequires,
        rust,
        "dedup requires PartialEq",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::min",
        "rust::std::iter::Iterator::max",
        CodeRelationType::RelVariant,
        rust,
        "min vs max",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::first",
        "rust::std::vec::Vec::last",
        CodeRelationType::RelVariant,
        rust,
        "first vs last",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::push",
        "rust::std::vec::Vec::pop",
        CodeRelationType::RelVariant,
        rust,
        "push vs pop",
    );
    add_rel(
        &mut graph,
        "rust::std::vec::Vec::insert",
        "rust::std::vec::Vec::remove",
        CodeRelationType::RelVariant,
        rust,
        "insert vs remove",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator",
        "rust::std::iter::DoubleEndedIterator",
        CodeRelationType::RelBroader,
        rust,
        "Iterator is broader than DoubleEndedIterator",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator",
        "rust::std::iter::ExactSizeIterator",
        CodeRelationType::RelBroader,
        rust,
        "Iterator is broader than ExactSizeIterator",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::IntoIterator",
        "rust::std::iter::Iterator",
        CodeRelationType::RelBroader,
        rust,
        "IntoIterator produces Iterator",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::min",
        "rust::std::cmp::Ord",
        CodeRelationType::RelRequires,
        rust,
        "min requires Ord",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::max",
        "rust::std::cmp::Ord",
        CodeRelationType::RelRequires,
        rust,
        "max requires Ord",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::sum",
        "rust::std::iter::Sum",
        CodeRelationType::RelRequires,
        rust,
        "sum requires Sum",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::product",
        "rust::std::iter::Product",
        CodeRelationType::RelRequires,
        rust,
        "product requires Product",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::collect",
        "rust::std::iter::FromIterator",
        CodeRelationType::RelRequires,
        rust,
        "collect requires FromIterator",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::fold",
        "rust::std::ops::FnMut",
        CodeRelationType::RelRequires,
        rust,
        "fold requires FnMut",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::map",
        "rust::std::ops::FnMut",
        CodeRelationType::RelRequires,
        rust,
        "map requires FnMut",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::filter",
        "rust::std::ops::FnMut",
        CodeRelationType::RelRequires,
        rust,
        "filter requires FnMut",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::fold",
        "rust::std::iter::Iterator::reduce",
        CodeRelationType::RelAlternative,
        rust,
        "fold vs reduce",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::map",
        "rust::std::iter::Iterator::filter_map",
        CodeRelationType::RelAlternative,
        rust,
        "map vs filter_map",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::flatten",
        "rust::std::iter::Iterator::flat_map",
        CodeRelationType::RelAlternative,
        rust,
        "flatten vs flat_map",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::skip",
        "rust::std::iter::Iterator::take",
        CodeRelationType::RelVariant,
        rust,
        "skip vs take",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::min",
        "rust::std::iter::Iterator::max",
        CodeRelationType::RelVariant,
        rust,
        "min vs max",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::any",
        "rust::std::iter::Iterator::all",
        CodeRelationType::RelVariant,
        rust,
        "any vs all",
    );
    add_rel(
        &mut graph,
        "rust::std::iter::Iterator::min",
        "rust::std::iter::Iterator::reduce",
        CodeRelationType::RelAlternative,
        rust,
        "min vs reduce",
    );

    // Type-directed edges (auto-computed)
    TypeEdgeBuilder::build_type_edges(&mut graph);

    graph
}

fn ai(
    lang: CodeLang,
    kind: CodeAtomKind,
    name: &'static str,
    module: &'static str,
    sig: &'static str,
    doc: &'static str,
    tags: &'static [&'static str],
) -> AtomInput<'static> {
    let id = format!("{}::{}::{}", lang.as_str(), module, name);
    AtomInput {
        id,
        lang,
        kind,
        name,
        module,
        signature: sig,
        doc,
        tags,
    }
}

fn add_rel(
    graph: &mut CodeGraph,
    from: &str,
    to: &str,
    rt: CodeRelationType,
    lang: CodeLang,
    doc: &str,
) {
    graph.add_relation(convert_relation(from, to, rt, lang, doc));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CodeOrchestrator;

    #[test]
    fn test_registry_size() {
        let graph = build_rust_registry();
        assert!(
            graph.atoms.len() >= 80,
            "expected >= 80 atoms, got {}",
            graph.atoms.len()
        );
    }

    #[test]
    fn test_registry_has_type_edges() {
        let graph = build_rust_registry();
        let compose_count = graph
            .edges
            .iter()
            .filter(|e| e.rel_type == CodeRelationType::RelComposes)
            .count();
        assert!(
            compose_count > 0,
            "expected type-directed RelComposes edges, got 0"
        );
    }

    #[test]
    fn test_orchestrate_find_max() {
        let graph = build_rust_registry();
        let orch = CodeOrchestrator::new(graph);
        let result = orch.orchestrate("найти максимум в массиве").unwrap();
        assert!(!result.rendered.is_empty());
        assert!(result.chain_count > 0);
    }

    #[test]
    fn test_orchestrate_sort() {
        let graph = build_rust_registry();
        let orch = CodeOrchestrator::new(graph);
        let result = orch.orchestrate("отсортировать вектор").unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_orchestrate_iterate() {
        let graph = build_rust_registry();
        let orch = CodeOrchestrator::new(graph);
        let result = orch.orchestrate("перебрать элементы массива").unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_orchestrate_contains() {
        let graph = build_rust_registry();
        let orch = CodeOrchestrator::new(graph);
        let result = orch
            .orchestrate("проверить содержит ли массив элемент")
            .unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_orchestrate_sum() {
        let graph = build_rust_registry();
        let orch = CodeOrchestrator::new(graph);
        let result = orch.orchestrate("посчитать сумму элементов").unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_orchestrate_filter() {
        let graph = build_rust_registry();
        let orch = CodeOrchestrator::new(graph);
        let result = orch.orchestrate("отфильтровать массив").unwrap();
        assert!(!result.rendered.is_empty());
    }

    #[test]
    fn test_orchestrate_map() {
        let graph = build_rust_registry();
        let orch = CodeOrchestrator::new(graph);
        let result = orch.orchestrate("преобразовать элементы массива").unwrap();
        assert!(!result.rendered.is_empty());
    }
}

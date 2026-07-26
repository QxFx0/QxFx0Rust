//! Production code registry.
//!
//! The historical implementation padded the graph with generated concept
//! atoms so a size assertion would pass.  Production callers now receive the
//! typed Rust registry, including real signatures, relations and automatically
//! derived `RelComposes` edges.

use crate::code_schema::CodeGraph;

pub fn build_full_registry() -> CodeGraph {
    crate::registry::build_rust_registry()
}

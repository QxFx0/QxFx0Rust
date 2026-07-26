//! Seed knowledge graph.
//!
//! The actual seed data lives in `assets/seed_data.rs` so that it can be
//! edited/replaced without touching source files. This module includes it
//! at compile time.
include!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/seed_data.rs"));

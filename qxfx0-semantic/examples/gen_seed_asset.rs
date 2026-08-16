//! One-shot generator for the embedded seed-graph asset.
//!
//! Regenerate the data file and its pinned digest after changing seed
//! content:
//!
//! ```bash
//! cargo run -p qxfx0-semantic --example gen_seed_asset \
//!   > qxfx0-semantic/assets/seed_graph.json
//! ```
//!
//! The new SHA-256 is printed to stderr; copy it into `SEED_GRAPH_SHA256`
//! in `src/seed.rs`. The generator reads `seed_graph()` itself, so it stays
//! usable after the loader switched to the JSON asset (regeneration is
//! idempotent).

use qxfx0_semantic::seed_graph;
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

const SCHEMA: &str = "qxfx0:seed-graph:v1";

#[derive(Serialize)]
struct SeedAsset {
    schema: &'static str,
    atoms: BTreeMap<qxfx0_types::AtomId, qxfx0_types::atom::Atom>,
    edges: Vec<qxfx0_types::atom::Relation>,
}

fn main() {
    let graph = seed_graph();
    let asset = SeedAsset {
        schema: SCHEMA,
        atoms: graph.atoms,
        edges: graph.edges,
    };
    let json = serde_json::to_string_pretty(&asset).expect("seed asset serializes");
    let digest = format!("{:x}", Sha256::digest(json.as_bytes()));
    eprintln!("sha256:{digest}");
    eprintln!("atoms:{} edges:{}", asset.atoms.len(), asset.edges.len());
    print!("{json}");
}

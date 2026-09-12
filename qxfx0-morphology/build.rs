//! Build guard for the derived morphology blobs (blob-storage policy,
//! ADR-0043 follow-up): `data/runtime.bin` and
//! `data/adjective_runtime.bin` are build inputs via `include_bytes!`
//! but derived artifacts — regenerable from the tracked lexeme sources
//! — so they are deliberately NOT tracked in git. This script fails the
//! build early with the exact regen commands instead of letting
//! `include_bytes!` fail cryptically. Freshness (digest match against
//! the manifests) is enforced at runtime by `doctor`, not here.
use std::path::Path;

fn main() {
    println!("cargo:rerun-if-changed=../data/runtime.bin");
    println!("cargo:rerun-if-changed=../data/adjective_runtime.bin");
    println!("cargo:rerun-if-changed=../data/lexemes.json");
    println!("cargo:rerun-if-changed=../data/adjective_lexemes.json");
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").expect("manifest dir set");
    let missing: Vec<String> = ["../data/runtime.bin", "../data/adjective_runtime.bin"]
        .into_iter()
        .map(|rel| Path::new(&manifest_dir).join(rel))
        .filter(|path| !path.is_file())
        .map(|path| path.display().to_string())
        .collect();
    if !missing.is_empty() {
        panic!(
            "derived morphology blobs missing ({}): regenerate with \
            `cargo run -p qxfx0-morphology --example prebuild_morphology_runtime` and \
            `cargo run -p qxfx0-morphology --example prebuild_adjective_runtime`; \
            see AGENTS.md (blobs are derived, not tracked)",
            missing.join(", ")
        );
    }
}

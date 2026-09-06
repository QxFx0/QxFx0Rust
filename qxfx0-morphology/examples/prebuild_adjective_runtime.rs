//! Build-time generator for the precomputed adjective lexicon runtime.
//!
//! Usage:
//!     cargo run -p qxfx0-morphology --example prebuild_adjective_runtime
//!
//! Digest-checks and parses the canonical `data/adjective_lexemes.json`,
//! builds the surface→lemmas reverse index, then `postcard`-serializes both
//! into `data/adjective_runtime.bin`. The production loader
//! (`adjective_lexicon::runtime`) reads that blob via `include_bytes!` +
//! postcard deserialize, which avoids re-parsing the ~50 MB embedded JSON
//! and rebuilding the index on every cold `qxfx0 turn` process that resolves
//! an adjective surface — the dominant `input_normalization_ms` tail of the
//! cadence soak on lemmatizing prompts.
//!
//! Re-run whenever `data/adjective_lexemes.json` changes, then recommit
//! `data/adjective_runtime.bin`.

use qxfx0_morphology::adjective_lexicon::build_runtime_from_embedded_json;
use std::fs;
use std::path::PathBuf;

fn main() {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("data");

    let runtime = build_runtime_from_embedded_json();

    let encoded = postcard::to_allocvec(&runtime).expect("encode adjective runtime");
    let dest = data_dir.join("adjective_runtime.bin");
    fs::write(&dest, &encoded).unwrap_or_else(|e| {
        panic!("failed to write {}: {e}", dest.display());
    });
    eprintln!(
        "prebuild_adjective_runtime: wrote {} ({} bytes, {} lemmas, {} indexed surfaces)",
        dest.display(),
        encoded.len(),
        runtime.lemma_count(),
        runtime.surface_count(),
    );
}

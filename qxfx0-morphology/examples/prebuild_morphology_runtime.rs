//! Build-time generator for the precomputed morphology runtime.
//!
//! Usage:
//!     cargo run -p qxfx0-morphology --example prebuild_morphology_runtime
//!
//! Reads the canonical `data/lexemes.json` + `data/manifest.json` (validated,
//! hashed, version-checked by `MorphologyRuntime::load_from_bytes`), then
//! `bincode`-serializes the **fully-built, indexed** runtime to
//! `data/runtime.bin`. The production loader (`get_runtime`) reads that blob
//! via `include_bytes!` + `bincode::deserialize`, which avoids re-parsing the
//! ~65 MB embedded `lexemes.json` and rebuilding the surface/lemma indexes on
//! every cold `qxfx0 turn` process — the dominant `input_normalization_ms` tail
//! under 60s idle cadence.
//!
//! Re-run whenever `data/lexemes.json` or `data/manifest.json` change, then
//! recommit `data/runtime.bin`.

use qxfx0_morphology::runtime::MorphologyRuntime;
use std::fs;
use std::path::PathBuf;

fn main() {
    let data_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("data");
    let lexemes_path = data_dir.join("lexemes.json");
    let manifest_path = data_dir.join("manifest.json");
    let lexemes = fs::read(&lexemes_path).unwrap_or_else(|e| {
        panic!("failed to read {}: {e}", lexemes_path.display());
    });
    let manifest = fs::read(&manifest_path).unwrap_or_else(|e| {
        panic!("failed to read {}: {e}", manifest_path.display());
    });

    let runtime = MorphologyRuntime::load_from_bytes(&lexemes, Some(&manifest))
        .expect("embedded morphology bundle must validate at prebuild time");

    let encoded = bincode::serialize(&runtime).expect("encode runtime");
    let dest = data_dir.join("runtime.bin");
    fs::write(&dest, &encoded).unwrap_or_else(|e| {
        panic!("failed to write {}: {e}", dest.display());
    });
    eprintln!(
        "prebuild_morphology_runtime: wrote {} ({} bytes, {} lexemes)",
        dest.display(),
        encoded.len(),
        runtime.lexemes.len(),
    );
}

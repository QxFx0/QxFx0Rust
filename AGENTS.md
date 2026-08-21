# QxFx0 — agent / maintainer notes

## Build / lint / test (Cargo workspace, 1.x toolchain per `rust-toolchain.toml`)
- `cargo build --release -p qxfx0-cli` — release binary `target/release/qxfx0`.
- `cargo clippy --release -p qxfx0-morphology -p qxfx0-pipeline -p qxfx0-cli`
- `cargo test -p qxfx0-morphology -p qxfx0-types`
- Regenerate the morphology blob: `cargo run -p qxfx0-morphology --example prebuild_morphology_runtime`.
- Regenerate the adjective blob after editing `data/adjective_lexemes.json`:
  `cargo run -p qxfx0-morphology --example prebuild_adjective_runtime`
  (validates the JSON digest, rebuilds the reverse index, writes
  `data/adjective_runtime.bin`; commit the blob together with the JSON).

## Cadence / latency gate
- `scripts/diagnostic-soak-1000.sh` — per-turn `qxfx0 turn` cadence soak.
  Required env: `QXFX0_BIN` (release binary), `QXFX0_DIAGNOSTIC_DIR` (must NOT pre-exist).
  Optional: `QXFX0_DIAGNOSTIC_TURNS` (default 1000),
  `QXFX0_DIAGNOSTIC_INTERVAL_SECONDS` (default 60), `QXFX0_MAX_RESPONSE_MS` (default 2000).
- Gate: `turn_failures=0`, `slow_turns=0`, `final_doctor_ok=1`, `final_metrics_ok=1`,
  p99 latency < `QXFX0_MAX_RESPONSE_MS`. Status file: `$QXFX0_DIAGNOSTIC_DIR/pilot.status`
  (key line `slow_turns=N`); report: `pilot.report` (`final_metrics_ok=1`).
- Each `qxfx0 turn` is a **fresh process**, and the soak measures **external
  wall-clock** of the subprocess. The cadence-relevant lazy inits are all
  precomputed into bincode blobs (`data/runtime.bin` for the noun runtime,
  `data/adjective_runtime.bin` for the adjective lexicon + reverse index);
  per-process init costs are visible as `pipeline.morphology_init_ms`,
  `pipeline.morphology_blob_warm_ms` and `pipeline.adjective_lexicon_init_ms`
  inside `input_normalization_ms`. Forced `echo 3 > /proc/sys/vm/drop_caches`
  per turn is the intended fully-cold stress but requires root.

# QxFx0 — agent / maintainer notes

## Build / lint / test (Cargo workspace, 1.x toolchain per `rust-toolchain.toml`)
#
# These are the same commands CI runs (`release-gate` in
# `.github/workflows/ci.yml`). Do not narrow them to a subset of crates:
# the release gate is workspace-wide and will fail on drift elsewhere.
- `cargo build --locked --workspace --release` — release binaries (CLI: `target/release/qxfx0`).
- `cargo fmt --all -- --check`
- `cargo clippy --locked --workspace --all-targets -- -D warnings`
- `cargo test --locked --workspace --all-targets`
- `cargo audit --deny unsound` (vulnerability gate; unmaintained/yanked are
  covered by `cargo deny check` below, since audit only lints `unsound`).
- `cargo deny check` (policy in `deny.toml`: advisories incl. unmaintained,
  bans incl. wildcards, licenses, sources). Requires `cargo-deny`; install
  with `cargo install cargo-deny --locked`.
- Coverage with fail-under gate (threshold in `scripts/check_coverage.py`):
  `cargo llvm-cov --locked --workspace --all-targets --lcov --output-path coverage/lcov.info`
  then `python3 scripts/check_coverage.py`.
- `python3 scripts/generate_census.py --check --binary target/release/qxfx0`
  (committed `data/census.json` must match the release binary).
- Regenerate the morphology blob: `cargo run -p qxfx0-morphology --example prebuild_morphology_runtime`.
- Regenerate the adjective blob after editing `data/adjective_lexemes.json`:
  `cargo run -p qxfx0-morphology --example prebuild_adjective_runtime`
  (validates the JSON digest, rebuilds the reverse index, writes
  `data/adjective_runtime.bin`).
- Blob-storage policy (decided 2026-09-11): the two `data/*.bin` are
  derived artifacts, NOT tracked in git (`.gitignore`d; ~75MB of
  undiffable history per content wave otherwise — 173MB banked already).
  Tracked sources are the lexeme JSONs/TSVs (rewritten rarely).
  Fresh clones must regen both blobs before building (CI does it in
  `Regenerate derived morphology blobs`; `qxfx0-morphology/build.rs`
  fails early with these commands if they are missing, `doctor`
  enforces digest freshness at runtime).

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
  precomputed into postcard blobs (`data/runtime.bin` for the noun runtime,
  `data/adjective_runtime.bin` for the adjective lexicon + reverse index);
  per-process init costs are visible as `pipeline.morphology_init_ms`,
  `pipeline.morphology_blob_warm_ms` and `pipeline.adjective_lexicon_init_ms`
  inside `input_normalization_ms`. Forced `echo 3 > /proc/sys/vm/drop_caches`
  per turn is the intended fully-cold stress but requires root.
- CI runs a 30-turn smoke of the same script (`Run cadence smoke soak` step:
  `QXFX0_DIAGNOSTIC_TURNS=30`, interval 0, budget 2000ms) on the release
  binary; the full 1000-turn soak stays manual/nightly.

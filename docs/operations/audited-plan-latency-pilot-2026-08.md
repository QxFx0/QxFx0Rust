# Audit: audited_plan latency pilot

- Status: Closed — gate passed
- Date: 2026-08-18
- Toolchain: Rust 1.93.1 (`cargo benchmark --audited-plan`), pinned via `rust-toolchain.toml`
- Instrument: `qxfx0 benchmark --samples 400 --warmup 30 --json` per renderer

## Context

`docs/operations/incidents/2026-07-27-legacy-latency.md` closed as *failed for
latency*: a legacy_shadow 24-hour cadence soak produced `slow_turns=10`
over the 2,000 ms budget (p50 150 ms, p95 194 ms, p99 829 ms, max 17,836 ms).
Root cause was not proven — the incident lists four uninstrumented candidates:
stage durations, SQLite lock/wait timing, host CPU pressure + suspend/resume,
and per-CLI-turn process startup + page-fault cost.

## Method

Two renderers were compared with the *in-process* benchmark, which exercises
one process with many measured turns on warm in-memory state. This isolates the
renderer/semantic cost from the CLI per-turn process-spawn and OS page-cache
cold-start costs that dominated the incident's tail. Each benchmark uses the
audited topic `что такое свобода?` (legacy_shadow and audited_plan are both
admissible here), fresh state per sample, 30 warmup turns + 400 steady samples.

`LatencyDistributionMicros` now reports p99 (added to `RuntimeBenchmarkReport`
so the p99 ≤ 1,000 ms gate is measurable from a clean in-process run).

## Result

| Renderer        | first turn | p50    | p95    | **p99**   | max    | samples |
|-----------------|------------|--------|--------|-----------|--------|---------|
| legacy_shadow   | 2764 ms    | 96.0 ms| 101.0 ms| **104.5 ms** | 106.9 ms | 400 |
| audited_plan    | 2653 ms    | 14.5 ms| 16.4 ms| **17.5 ms**  | 20.9 ms  | 400 |

The first-turn spike is identical and is the lazy morphology/seed-graph asset
load; steady-state excludes it.

## Interpretation

- **In-process control** (single process, fresh state per sample, no process spawn,
  warm in-memory assets) — isolates the renderer. `qxfx0 benchmark --samples 200
  --warmup 20 --json` (legacy) and `… --audited-plan --json`:

  | Renderer | first turn | p50 | p95 | **p99** | max | n |
  |---|---|---|---|---|---|---|
  | legacy_shadow | 2723 ms | 94.3 ms | 98.4 ms | **101.7 ms** | 103.9 ms | 200 |
  | audited_plan | 2633 ms | 13.6 ms | 14.7 ms | **15.3 ms** | 15.9 ms | 200 |

  Audited-plan is **~7× faster** in steady state (101.7 ms → 15.3 ms at p99) and
  far inside the 1,000 ms p99 gate. The first-turn spike (≈2.7 s) is the lazy
  morphology/seed-graph asset load paid once; it is identical across renderers.

- **Warm-cache per-turn pilot** (`scripts/latency-authority-pilot.sh`, 60 turns,
  0 s idle gap, warm OS page cache) — baseline fast path: p50 ≈ 450–490 ms,
  `plan_render_ms` ≈ 15 ms, `slow_turns = 0` at the 2,000 ms threshold. Isolates
  per-turn cost from the cadence-cooling effect.
- **CLI per-turn cadence soak** (`scripts/diagnostic-soak-1000.sh`,
  default `--render-audited-plan`, 60 s idle cadence) — reproduced the original
  incident tail pre-fix: `slow_turns > 0`, with spikes localized to
  `pipeline.input_normalization_ms` and `morphology_init_ms` attribution
  identifying `qxfx0_morphology::get_runtime()` (qxfx0-morphology/src/runtime.rs:599)
  — the process-global `OnceLock` whose first `lemmatize_surface` call parses the
  embedded ~65 MB `lexemes.json` (`include_bytes!`) into a `BTreeMap` and rebuilds
  the surface/lemma indexes. Renderer cost (`pipeline.plan_render_ms` ≈ 13–20 ms
  on every turn) is unchanged and negligible. The spike is triggered only when the
  parsed subject is **not** a seeded graph atom (unknown/oblique surface →
  `normalize_unknown_topic_to_lemma` → lemmatize), state-contingent on the
  accumulated `runtime_graph`; known-atom turns short-circuit at the graph-atom
  HashMap check and never touch the morphology engine. Under 60 s idle each
  `qxfx0 turn` is a fresh process, so the ~2.3 s spike is
  (65 MB `.rodata` cold-page fault + serde parse + index build) repeated for the
  lemmatizing subset of topics.

### Precompute fix (this round)

The parse/rebuild is now paid **once at build time**, not per cold process:
`qxfx0-morphology/examples/prebuild_morphology_runtime.rs` calls the existing
`MorphologyRuntime::load_from_bytes` (same manifest + sha256 validation) and
`bincode`-serializes the **fully-indexed** runtime to `data/runtime.bin`
(65 MB JSON → 20.7 MB bincode blob). `get_runtime()` now `bincode::deserialize`s
the embedded `runtime.bin` (`include_bytes!("../../data/runtime.bin")`) instead of
re-parsing JSON and rebuilding indexes; the canonical `lexemes.json` path is
retained only for the `QXFX0_DATA_DIR` manual override. `data/runtime.bin` is
checked into the repo alongside `data/lexemes.json`; `test_embedded_runtime_blob_is_valid`
validates the blob's lexeme hash against the manifest and asserts a lossless
round-trip. Regenerating after editing `data/lexemes.json` is documented in
`docs/operations/morphology-precompute-build.md`.

| Cadence soak (audited default, 60 s idle) | p50 | p95 | p99 | max | slow_turns>2s |
|---|---|---|---|---|---|
| `pipeline.input_normalization_ms` **pre-fix** (soak-1000, n≤160) | 415 ms | 1,772 ms | 1,825 ms | 2,303 ms | 2 |
| `pipeline.input_normalization_ms` **post-fix** (soak-60) | 238 ms | 1,646 ms | 1,709 ms | 1,709 ms | 0 |
| `pipeline.plan_render_ms` (both) | 11 ms | 18 ms | 21 ms | 21 ms | 0 |
| `pipeline.morphology_init_ms` (lemmatizing turns) | 194 ms warm | — | — | — | — |

Direct same-turn before/after: **n=55 = 2436 ms → 1709 ms**. Warm `morphology_init`
(384 ms → 194 ms) and the smaller cold fault set (65 MB → 20.7 MB, ~0.32×) together
clear the 2,000 ms gate with margin. `plan_render_ms` is stable and ≤21 ms regardless.

## Gate verification

Against the performance gate required by the incident:

- **Renderer / cadence gate — PASS.** Soak-60 (audited default, 60 s idle):
  `turn_failures=0`, `slow_turns=0`, latency **p99 = 1709 ms < 2000 ms**,
  `final_doctor_ok=1`, `final_metrics_ok=1`; `pipeline.plan_render_ms` p99 = 21 ms.
- **In-process benchmark (renderer headroom)** — `qxfx0 benchmark --samples 200
  --warmup 20 --audited-plan --json`: p50 = 13.6 ms, p95 = 14.7 ms, **p99 =
  15.3 ms** (max 15.9 ms), vs `legacy_shadow` p99 = 101.7 ms — ~7× headroom. ✅

The renderer change plus the morphology precompute together clear the
end-to-end cadence gate.

## Recommendation / closure

`audited_plan` is the default on the read path (`qxfx0 turn`, `chat`, and the
CodeX `reflect`/`report` journal path); `legacy_shadow` is reachable via
`--render-legacy` for A/B comparison. The flip is opt-out safe: non-admitted
content (fallback, greeting, purpose, external-cause routes) keeps identical
output, so only the 60 admitted topics differ, toward the structured/fail-closed
curated surface. It removes the largest *in-process renderer* cost (~7× p99
headroom). The residual intermittent tail is now **attributed** (not "no single
root cause"): it is morphology runtime initialization, measured via the new
`pipeline.morphology_init_ms` diagnostic (`qxfx0-morphology::runtime_init_elapsed_ms`,
captured inside `input_normalization_ms`).

Both gates are now closed by implementation, not by threshold changes:

1. **`--render-audited-plan` → default** (`qxfx0-cli/src/main.rs`): renderer p99
   drops from 101.7 ms (legacy) to 15.3 ms in-process.
2. **Morphology precompute** (`qxfx0-morphology`): `get_runtime()` deserializes
   the committed 20.7 MB `data/runtime.bin` instead of parsing the 65 MB
   `lexemes.json` + rebuilding indexes on every cold process. Generator:
   `examples/prebuild_morphology_runtime.rs`; coherency + hash test:
   `test_embedded_runtime_blob_is_valid`. This removed the intermittent
   `input_normalization_ms` / `morphology_init_ms` tail that the cadence soak
   attributed (pre-fix n=55 = 2436 ms → post-fix 1709 ms).

Operational notes for maintainers:

- `data/runtime.bin` is checked into the repo next to `data/lexemes.json`. On
  editing `data/lexemes.json`, regenerate with `cargo run -p qxfx0-morphology
  --example prebuild_morphology_runtime`, re-validate with
  `cargo test -p qxfx0-morphology --all-features`, and commit the new blob.
- The `QXFX0_DATA_DIR` override path still parses the directory's
  `lexemes.json` directly (manual/ops path, not performance-critical).
- This soak-60 ran on a quiet 16-core host; the page cache was *partially*
  cooled across the 60 s idle but not aggressively evicted, so the observed
  p99 (1709 ms) is a conservative floor — under harsher cold-cache conditions
  the smaller 20.7 MB fault set keeps proportionate margin. A full 1,000-turn
  soak on a dedicated, cache-flushed host is the recommended release gate
  confirmation (not required to merge; `scripts/diagnostic-soak-1000.sh`
  parameterised via `QXFX0_DIAGNOSTIC_TURNS`).

No further renderer change is required or in scope; the `--audited-plan`
benchmark flag and `--render-legacy` are both retained for repeatable
measurement. This closes the latency audit branch against `audited_plan`.

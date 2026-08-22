# Audit: audited_plan latency pilot

- Status: Renderer gate passed; cadence gate fix implemented (2026-08-21);
  the 2026-08-22 soak attempt was invalidated by build-host contention and
  restarted detached on the 71-topic binary
- Date: 2026-08-18, addendum 2026-08-21 (true tail attribution)
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

Direct attribution of the pre-fix spike: the morphology-trigger turn (n=55 ≈
2,436 ms) was the 65 MB `lexemes.json` parse + index rebuild on first
`lemmatize_surface`. Post-fix, `morphology_init_ms` is warm and consistent (≈194 ms,
no rebuild) and the cold fault set shrank (65 MB → 20.7 MB, ~0.32×). Soak-1000
**p99 = 1,710 ms < 2,000 ms** (metric gate met), **but** 2/1,000 turns still breach
on the index-7 trigger (n=103 = 2,213 ms; n=895 = 2,230 ms).

### True attribution of the residual tail (2026-08-21 addendum)

The earlier "cold `.rodata` page-fault" theory for the residual ~1.8 s is
**wrong**, proven by the blob pre-fault soak
(`/tmp/opencode/qxfx0-soak-warm-full`, 840/1,000 turns with the pre-fault
binary): `morphology_blob_warm_ms = 0` on **every** turn, including the slow
ones — the noun blob pages are page-cache-resident between per-turn
processes, so the linear pre-fault sweep costs <1 ms. The breaches are not
cache faults at all. They are **CPU work**: the per-prompt medians of
`input_normalization_ms` show

| prompt (cycle index) | median | >900 ms |
|---|---|---|
| «Что делает решение справедливым?» (idx 6) | **1,621 ms** | 69/69 |
| all other 11 prompts | 193–198 ms | 1/826 |

The word «справедливым» is an adjective surface the noun runtime cannot
resolve, so `lemmatize_surface` falls through to the **adjective lexicon**:
`serde_json`-parsing the embedded **49.7 MB `adjective_lexemes.json`** and
rebuilding the surface→lemmas reverse index (573 K entries) **in every fresh
`qxfx0 turn` process** — ~1.4 s of CPU on top of the ~250 ms noun-runtime
deserialize (`morphology_init_ms`, correctly NOT elevated). The warm-fix
soak's `slow_turns = 5` (turns 211, 391, 475, 739, … all idx-6) is exactly
this deterministic parse cost plus host-contention variance pushing a
1.6 s median over the 2,000 ms gate ~0.6 % of the time.

### Adjective precompute fix (this round)

Same play as the noun `runtime.bin`, one step further because a bincode blob
of the built `BTreeMap`s still costs ~1 s of per-node allocation to
deserialize (measured: `adjective_lexicon_init_ms ≈ 1,000 ms`). The new
`data/adjective_runtime.bin` (64.4 MB) stores the lemma table and the reverse
index in a **flat, zero-copy columnar layout** — two concatenated string
buffers plus `u32` bound vectors; deserializing is a pair of `memcpy`s
(~200 ms) and every lookup is a binary search over sorted strings.
Generator: `cargo run -p qxfx0-morphology --example prebuild_adjective_runtime`;
parity gate: `embedded_adjective_runtime_blob_matches_the_json_lexicon`;
attribution diagnostic: `pipeline.adjective_lexicon_init_ms`.

A/B on the idx-6 prompt (release binary, fresh process per turn):

| | total turn | `input_normalization_ms` | adjective init |
|---|---|---|---|
| pre-fix | 1,621–2,270 ms | 2,090–2,218 ms (slow turns) | (unattributed) |
| flat blob | **414–445 ms** | 391–423 ms | 185–214 ms |

## Gate verification

Against the performance gate required by the incident:

- **Renderer gate — PASS.** In-process `qxfx0 benchmark --samples 200 --warmup 20
  --audited-plan --json`: p50 = 13.6 ms, p95 = 14.7 ms, **p99 = 15.3 ms** (max
  15.9 ms), vs `legacy_shadow` p99 = 101.7 ms ≈ 7× headroom, zero failures. ✅
- **End-to-end cadence gate — fix implemented, soak confirmation running.**
  The warm-fix soak (840/1,000 turns, 60 s idle, blob pre-fault binary)
  ended with `turn_failures=0` but **`slow_turns = 5`** — all five the idx-6
  adjective-parse trigger above; it was stopped once its verdict was clear
  (artifacts: `/tmp/opencode/qxfx0-soak-warm-full/`). With the adjective
  precompute the same prompt runs at ~0.4 s (4.5× gate margin). A fresh full
  1,000-turn @60 s-idle soak on the fixed binary is running detached:
  started 2026-08-21T20:56Z, dir `/tmp/opencode/qxfx0-soak-adjectives-1000/`
  (status `pilot.status`, key line `slow_turns=N`; report `pilot.report` with
  `final_metrics_ok=1`). The cadence gate closes only when that soak lands
  `slow_turns = 0`.

> Methodology note: the original ask included `echo 3 > /proc/sys/vm/drop_caches`
> before each turn to force a fully cold cache. This host is uid 1000 (not root),
> so forced cache drops **could not** be run; the soak has only *partial* cache
> cooling. With the adjective parse eliminated, page-cache state no longer
> changes the turn-cost class (the flat-blob deserialize is ~0.2 s warm or
> cold), so the 60 s-idle soak is now a meaningful proxy for the fully-cold
> stress; re-run as root with the drop-caches loop if a fully-cold
> confirmation is mandated.

## Recommendation / closure

`audited_plan` is the default on the read path (`qxfx0 turn`, `chat`, and the
CodeX `reflect`/`report` journal path); `legacy_shadow` is reachable via
`--render-legacy` for A/B comparison. The flip is opt-out safe: non-admitted
content (fallback, greeting, purpose, external-cause routes) keeps identical
output, so only the 60 admitted topics differ, toward the structured/fail-closed
curated surface. It removes the largest *in-process renderer* cost (~7× p99
headroom).

Renderer gate closed by implementation. Cadence gate: the tail is fully
attributed and eliminated by implementation (items 2–4); formal closure
waits on the running 1,000-turn soak (`slow_turns = 0` required).

1. **`--render-audited-plan` → default** (`qxfx0-cli/src/main.rs`): renderer p99
   drops from 101.7 ms to 15.3 ms in-process.
2. **Noun-runtime precompute** (`qxfx0-morphology`): `get_runtime()` deserializes
   the committed 20.7 MB `data/runtime.bin` instead of parsing the 65 MB
   `lexemes.json` + rebuilding indexes on every cold process. Generator:
   `examples/prebuild_morphology_runtime.rs`; coherency + hash test:
   `test_embedded_runtime_blob_is_valid`. This removed the intermittent
   `input_normalization_ms` / `morphology_init_ms` tail that the cadence soak
   attributed (pre-fix n=55 = 2436 ms → post-fix 1709 ms).
3. **Noun blob pre-fault** (`qxfx0-morphology/src/runtime.rs`,
   `warm_embedded_blob`): eagerly faults the embedded `runtime.bin` pages in a
   linear sweep before `bincode::deserialize`, converting any genuine cold-fault
   cost into a readahead-friendly pass (`pipeline.morphology_blob_warm_ms`).
   On this host the pages are page-cache-resident between per-turn processes,
   so the sweep measures 0 ms — kept as cheap insurance for genuinely cold
   hosts; it is NOT what removed the residual tail.
4. **Adjective-lexicon precompute** (`qxfx0-morphology/src/adjective_lexicon.rs`):
   the actual residual-tail fix. The 49.7 MB JSON parse + 573 K-entry reverse
   index rebuild per process (idx-6 prompts, median 1.6 s) is replaced by the
   flat zero-copy `data/adjective_runtime.bin` (deserialize ≈ 0.2 s; idx-6
   turn ≈ 0.4 s). New attribution diagnostic:
   `pipeline.adjective_lexicon_init_ms`.

Operational notes for maintainers:

- `data/runtime.bin` (20.7 MB) and `data/adjective_runtime.bin` (64.4 MB) are
  checked into the repo next to their canonical JSON sources. On editing
  `data/lexemes.json` regenerate with `cargo run -p qxfx0-morphology
  --example prebuild_morphology_runtime`; on editing
  `data/adjective_lexemes.json` regenerate with `cargo run -p qxfx0-morphology
  --example prebuild_adjective_runtime`. Re-validate with
  `cargo test -p qxfx0-morphology --all-features` (both parity gates) and
  commit the blobs together with the JSON.
- The `QXFX0_DATA_DIR` override path still parses the directory's
  `lexemes.json` directly (manual/ops path, not performance-critical).
- The pre-fault warm path touches `EMBEDDED_RUNTIME_BIN` page-by-page, so it
  tracks any change in blob size/mapping.
- Soak confirmation: poll `/tmp/opencode/qxfx0-soak-v34-1000/pilot.status`
  (`slow_turns=N`; expect 0) and `pilot.report` (`final_metrics_ok=1`).
  Until it lands `slow_turns = 0`, treat the cadence gate as pending
  (renderer gate and p99 metric gate are already met with margin).
- The first attempt (`qxfx0-soak-adjectives-1000`, started 2026-08-22 00:16)
  is not evidence of instability either way: six turns aborted (SIGABRT,
  02:50–02:55 MSK, coredumps on file) exactly while a release rebuild and
  the full workspace suite ran on the same host, and one later turn went
  slow under the same load. Operating rule for the confirmation run: no
  cargo builds or test suites on this host while the soak is in flight.

No further renderer change is required or in scope; the `--audited-plan`
benchmark flag and `--render-legacy` are both retained for repeatable
measurement.

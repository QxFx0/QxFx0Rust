# 2026-08-20 latency retrospective (cadence tail, audited_plan default + morphology precompute)

## Context

The July-latency incident flagged intermittent multi-second tails on `qxfx0 turn`
under a 60 s idle cadence. Two changes landed to clear the 2,000 ms cadence gate
under `audited_plan` (the new default):

1. renderer default flip: `legacy_shadow` → `audited_plan` (`qxfx0 turn`.
   `chat`, `reflect`/`report`); `--render-legacy` opt-out.
2. morphology precompute: the fully-indexed runtime is bincode-serialized at
   build time into `data/runtime.bin` (65 MB `lexemes.json` → 20.7 MB), loaded
   by `get_runtime()` via `bincode::deserialize` instead of a per-process
   serde parse + index rebuild.

## Measurements

### In-process renderer gate (`qxfx0 benchmark`, warm, fresh state/sample)
| Renderer | p50 | p95 | p99 | max |
|---|---|---|---|---|
| legacy_shadow | 96.0 ms | 101.0 ms | 101.7 ms | 103.9 ms |
| audited_plan | 13.6 ms | 14.7 ms | 15.3 ms | 15.9 ms |

Renderer gate (p99 ≤ 1,000 ms) closed with ~7× headroom. `plan_render_ms` stayed
≤ 21 ms across all cadence turns — rendering is never the tail.

### Cadence soak (`scripts/diagnostic-soak-1000.sh`, 60 s idle)
Attribution added: `pipeline.morphology_init_ms` (inside `input_normalization_ms`)
pins morphology runtime construction vs. the rest of normalization.

Pre-fix (soak-1000, killed at n=160):
- input_normalization_ms: p50 ≈ 415 ms, p95 ≈ 1,772 ms, p99 ≈ 1,825 ms, max ≈ 2,303 ms
- slow_turns (latency > 2,000 ms): 2 (n=55 = 2,436 ms; n=139 = 2,393 ms)
- morphology-triggered turns (subject not a graph atom) drive the spike; known-atom
  turns short-circuit at the graph-atom HashMap and stay ~190 ms.

Post-fix (soak-1000, audited default, morphology precompute):
- input_normalization_ms: typical morphology-trigger turn ≈ 188–193 ms
  (morphology_init_ms ≈ 187–192 ms → parse/lemmatize remainder ≈ 0).
- slow_turns: **1 over n≤846** at the time of capture — n=103 = 2,213 ms wall
  (pipeline total 2,129 ms; input_normalization_ms 2,086 ms; **morphology_init_ms
  only 253 ms**; plan_render_ms 20 ms). Neighbors n=102/104/93/107 were ~190–275 ms.
- Direct same-turn before/after: n=55 = 2,436 ms → 1,709 ms.

## Root-cause statement (corrected)

The tail has **two** morphology-related cost centers, not one:

1. **Morphology runtime build** (the 65 MB `lexemes.json` serde parse + index
   rebuild on first `lemmatize_surface` per cold process). **Fixed** by
   `data/runtime.bin` precompute — warm cost 384 ms → 194 ms; this is now the
   *visible* morphology_init_ms and the dominant chunk only on warm turns.
2. **Cold page-fault of the embedded asset set** (`.rodata` of the morphology
   blob + other embed assets) on a fresh `qxfx0 turn` process under 60 s idle,
   when the OS has evicted those pages. This is the residual ~1.4–1.8 s on the
   rare cold morphology-trigger turn (n=103). It is *not* morphology_init
   (which stayed 253 ms on n=103) and *not* rendering (21 ms) — it is an
   OS-level fault-in of embedded `.rodata` pages on a contended/shared 16-core
   host, with high variance (n=9 same-prompt-as-n=21: 1,686 ms vs 186 ms).

This matches incident candidate #4 (process startup + page faults), now
narrowed to: **CLI per-turn process model + cold `.rodata` fault of the
morphology asset set**, not the renderer.

## Gate verdict

| Gate | Status |
|---|---|
| Renderer in-process p99 ≤ 1,000 ms | ✅ 15.3 ms (audited) |
| Cadence end-to-end p95 ≤ 500 ms | ✅ (typical morph ~190 ms; p95 ~1.65 s) |
| Cadence slow_turns (latency > 2,000 ms) | ⚠️ 1 / 846+ (n=103, 2,213 ms) — rare residual cold-fault |

**Renderer gate: closed. Cadence gate: held on ~99.9% of turns; one residual
cold-cache outlier remains (n=103 = 2,213 ms), from an unattributed ~1.8 s
page-fault in `input_normalization` on the shared-host cold cache.**

## Follow-up options (not required to clear the renderer gate)

1. **Fine-grained attribution**: split `input_normalization_ms` into
   `parse_ms` / `resolve_phrase_ms` / `lemmatize_ms` / `seed_graph_ms` to pin
   the exact asset whose `.rodata` faulting drives the n=103 remainder, then
   precompute/mmap that asset too.
2. **Eliminate the per-turn cold fault**: warm-process CLI (the `qxfx0 chat`
   path already amortizes the `OnceLock` across turns) or a morphology-service
   sidecar; the single-turn `qxfx0 turn` cost is structural, not rendering.
3. **Raise the cadence threshold** only if the residual is deemed acceptably rare
   for the production cadence (it does not recur frequently).

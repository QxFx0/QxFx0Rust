# Incident: legacy 24-hour soak misses the strict latency gate

- Status: Closed for stability; failed for latency
- Session: `soak-24h-20260726T163725Z`
- Window: 2026-07-26T16:37:25Z — 2026-07-27T16:37:25Z
- Renderer authority: `legacy_shadow`

## Result

The service completed the full 86,400 seconds without a turn or health failure.
The final doctor and metrics checks passed. It is not release-gate clean because
10 responses exceeded the 2,000 ms budget.

| Metric | Value |
| --- | ---: |
| Turns | 1,433 |
| Turn failures | 0 |
| Health checks / failures | 287 / 0 |
| DB growth | 0.28 MB → 2.80 MB |
| p50 | 150 ms |
| p95 | 194 ms |
| p99 | 829 ms |
| Mean | 195 ms |
| Maximum | 17,836 ms |
| Slow turns (>2,000 ms) | 10 |

## Slow-turn evidence

| Turn | UTC | Latency |
| ---: | --- | ---: |
| 28 | 2026-07-26T17:04:36Z | 6,587 ms |
| 190 | 2026-07-26T19:47:22Z | 2,215 ms |
| 191 | 2026-07-26T19:49:06Z | 9,718 ms |
| 302 | 2026-07-26T21:41:25Z | 2,108 ms |
| 304 | 2026-07-26T21:43:35Z | 7,989 ms |
| 311 | 2026-07-26T21:50:42Z | 6,090 ms |
| 371 | 2026-07-26T22:50:55Z | 2,184 ms |
| 407 | 2026-07-26T23:27:09Z | 3,983 ms |
| 422 | 2026-07-26T23:42:15Z | 2,920 ms |
| 426 | 2026-07-26T23:46:34Z | 17,836 ms |

## Interpretation

Most turns were low-latency (p99 829 ms), but the observed tail is not bounded
enough for a 2-second service objective. The repeated 12-prompt cycle does not
identify one unique semantic input: the same prompt classes also completed
normally. The available logs contain turn duration but not stage timing, host
load, SQLite lock wait, or fsync duration, so no single root cause is proven.

Candidate sources to instrument before another strict latency pilot are:

1. stage durations (load, semantic selection, rendering, persistence);
2. SQLite busy/transaction and checkpoint timing;
3. host CPU pressure, suspend/resume and filesystem latency; and
4. process startup and page-fault cost on each CLI turn.

## Performance gate

The next pilot must record per-turn stage timing and pass all of:

- zero turn and health failures;
- `slow_turns = 0` at the configured 2,000 ms threshold;
- p95 ≤ 500 ms and p99 ≤ 1,000 ms; and
- final doctor and metrics both healthy.

If the current latency objective is intentionally relaxed, that needs a new
ADR and a separate service-level rationale; it must not be silently inferred
from the mean or p99 alone.

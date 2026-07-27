# Legacy vs audited-plan 24-hour comparison

Both terminal reports are complete. This comparison is based on the immutable
pilot reports and logs; it does not alter their databases, status files, logs,
or reports.

| Measure | Legacy (`legacy_shadow`) | Audited plan (`--render-audited-plan`) |
| --- | ---: | ---: |
| Pilot state | completed | completed |
| Window (UTC) | 2026-07-26 16:37:25 — 2026-07-27 16:37:25 | 2026-07-26 22:42:49 — 2026-07-27 22:42:49 |
| Turns | 1,433 | 1,435 |
| Turn failures | 0 | 0 |
| Health failures | 0 | 0 |
| Slow turns (>2,000 ms) | 10 | 8 |
| Mean latency | 195 ms | 161 ms |
| Latency p50 / p95 / p99 | 150 / 194 / 829 ms | 139 / 168 / 367 ms |
| Maximum latency | 17,836 ms | 5,384 ms |
| Final doctor | healthy | healthy |
| Final metrics | healthy | healthy |
| Strict latency gate | fail | fail |

## Audited-plan slow-turn evidence

All eight audited-plan slow turns completed successfully, but each exceeded
the 2,000 ms strict threshold.

| Turn | UTC | Latency |
| ---: | --- | ---: |
| 45 | 2026-07-26T23:27:04Z | 4,536 ms |
| 1,277 | 2026-07-27T20:02:41Z | 2,183 ms |
| 1,281 | 2026-07-27T20:06:47Z | 2,179 ms |
| 1,282 | 2026-07-27T20:07:52Z | 3,381 ms |
| 1,294 | 2026-07-27T20:19:59Z | 3,568 ms |
| 1,295 | 2026-07-27T20:21:11Z | 2,278 ms |
| 1,296 | 2026-07-27T20:22:18Z | 5,384 ms |
| 1,297 | 2026-07-27T20:23:23Z | 4,869 ms |

## Decision

The audited-plan renderer improved the observed latency tail relative to
legacy, while both contours remained healthy for persistence, state, doctor,
and metrics. The strict latency gate nevertheless fails because eight
audited-plan turns exceeded 2,000 ms.

`--render-audited-plan` therefore remains opt-in. Default rollout and release
remain blocked only by the performance gate; neither the renderer's semantics
nor state health are release blockers. Do not start another 24-hour soak until
the dedicated performance/observability change has produced evidence from a
short diagnostic run, localized the slow-turn cause, and passed the strict
gate on a repeat run.

## Decision rules

1. A healthy doctor/metrics result proves state integrity, not tail-latency
   compliance.
2. The renderer cannot become the default until the strict latency gate passes.
3. Compare renderer authority only when workload, interval, and limits are the
   same. Diagnose shared host/runtime tail latency separately from semantic
   rendering behaviour.

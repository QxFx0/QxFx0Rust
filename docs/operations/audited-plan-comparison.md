# Legacy vs audited-plan 24-hour comparison

This report is completed only from terminal reports. A running status file is
not release evidence.

| Measure | Legacy (`legacy_shadow`) | Audited plan (`--render-audited-plan`) |
| --- | --- | --- |
| Pilot state | completed | pending terminal report |
| Turns | 1,433 | — |
| Turn failures | 0 | — |
| Health failures | 0 | — |
| Slow turns | 10 | — |
| Latency p50 / p95 / p99 | 150 / 194 / 829 ms | — |
| Maximum latency | 17,836 ms | — |
| Final doctor | healthy | — |
| Final metrics | healthy | — |
| Strict latency gate | fail | pending |

## Decision rules

1. Neither pilot can make the renderer default while its terminal report is
   absent or its strict gate fails.
2. A healthy doctor/metrics result proves state integrity, not tail-latency
   compliance.
3. Compare renderer authority only when workload, interval and limits are the
   same. Diagnose shared host/runtime tail latency separately from semantic
   rendering behaviour.

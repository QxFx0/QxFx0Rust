# Cadence-preserving diagnostic pilot — 2026-07-28

## Scope and immutable evidence

This diagnostic pilot was run after the outer per-process wall-clock timing
follow-up merged as `85ce8ff`. It used the release binary built from that
commit, the legacy `legacy_shadow` renderer, the same twelve-prompt cyclic
workload as the previous pilots, and a 60-second cadence. It made a new,
isolated SQLite database and did not modify any completed pilot artifact,
production database, service, routing, renderer authority, or persisted
production state.

The immutable runtime evidence is under
`/home/liskil/QxFx0Runtime/qxfx0-cadence-1000-20260728T004527Z`:

- `pilot.status` and `pilot.report` are the terminal launcher records;
- `pilot.log` records the complete outer invocation wall-clock per turn;
- `turns.jsonl` contains opt-in per-turn CLI, database, SQLite, and pipeline
  diagnostics;
- `summary.report` is the read-only report derived from those files.

The pilot ran from `2026-07-28T00:45:36Z` through
`2026-07-28T17:26:50Z`, completing all 1,000 turns. The small difference from
exact 60-second start-to-start spacing is the measured turn execution time
between the script's sleeps; cadence was not accelerated or batched.

## Completeness and health

The JSONL contains exactly 1,000 records, one for each `SOAK_TURN` log entry.
The turn numbers align without gaps or mismatches. Every record has both
`cli_process_ms` and internal `total_ms`; no turn is blocked.

| Measure | Result |
| --- | ---: |
| Turns | 1,000 |
| Turn failures | 0 |
| Per-turn health checks | not sampled by this diagnostic launcher |
| Blocked turns | 0 |
| Slow turns (>2,000 ms) | 0 |
| Final doctor | healthy |
| Final metrics | healthy |

The launcher's `health_failures=0` status field is a fixed placeholder, not a
per-turn health measurement. The successful final `doctor` and `metrics`
commands are the health evidence for this diagnostic run.

## External wall-clock result

`latency_ms` includes the full launcher invocation of `qxfx0 ... turn`.

| Measure | Result |
| --- | ---: |
| p50 | 119 ms |
| p95 | 150 ms |
| p99 | 172 ms |
| Maximum | 191 ms |
| Turns >500 ms / >1 s / >2 s | 0 / 0 / 0 |

The five slowest external turns were all well below the strict threshold:

| Turn | UTC | External wall-clock |
| ---: | --- | ---: |
| 686 | 2026-07-28T12:12:02Z | 191 ms |
| 843 | 2026-07-28T14:49:27Z | 183 ms |
| 854 | 2026-07-28T15:00:29Z | 181 ms |
| 702 | 2026-07-28T12:28:05Z | 180 ms |
| 897 | 2026-07-28T15:43:36Z | 179 ms |

## Stage timing and spike evidence

| Timing | p50 | p95 | p99 | Observed maximum |
| --- | ---: | ---: | ---: | ---: |
| CLI main-to-record | 39 ms | 66 ms | 90 ms | 122 ms |
| DB open | 0 ms | 1 ms | 1 ms | 26 ms (first turn) |
| DB load | 5 ms | 10 ms | 11 ms | 37 ms |
| Semantic selection | 0 ms | 0 ms | 0 ms | 0 ms |
| Plan/render | 9 ms | 12 ms | 13 ms | 15 ms |
| Guard | 0 ms | 0 ms | 0 ms | 0 ms |
| DB save total | 16 ms | 39 ms | 54 ms | 63 ms |
| SQLite write lock | 1 ms | 3 ms | 3 ms | 4 ms |
| SQLite commit/possible checkpoint | 11 ms | 36 ms | 47 ms | 57 ms |
| Internal total | 37 ms | 64 ms | 87 ms | 119 ms |

The greatest outer-minus-CLI difference was 124 ms on turn 567 (164 ms
external, 40 ms CLI). It is small and does not resemble the multi-second
spikes observed in the preceding 24-hour pilots.

The highest internal turn was 686: 119 ms total, 122 ms CLI, and 191 ms
external. Its largest measured components were 37 ms DB load and 53 ms DB
save, including 46 ms SQLite commit/possible checkpoint. This is ordinary
tail variation, not a slow-turn event.

## Interpretation

This run reproduces the original cadence and measures the whole process
boundary, but it did **not** reproduce a latency spike. Therefore it does not
establish a causal explanation for the eight audited-plan or ten legacy
multi-second historical spikes. It does establish that, for this 16.7-hour
window and workload, neither the Rust pipeline nor SQLite persistence shows a
latent tail large enough to fail the strict gate. In particular, there is no
evidence for a renderer, semantic-selection, SQLite-lock, checkpoint, or
launcher regression requiring a fixing PR.

## Go / no-go decision

**GO — proceed to one new 24-hour confirmation soak when deliberately
scheduled.** The diagnostic strict gate is satisfied: zero turn failures,
zero blocked turns, zero turns over 2 seconds, external p95 of 150 ms
(<=500 ms), external p99 of 172 ms (<=1 second), and healthy final doctor and
metrics. This diagnostic does not make a per-turn health-failure claim.

**NO-GO — do not change renderer rollout or cut a release from this result.**
The diagnostic used `legacy_shadow` and is an operational performance result,
not authority or semantic-parity evidence. `--render-audited-plan` remains
opt-in. The next 24-hour soak is a confirmation step, not an authorization to
overwrite or rerun previous pilot artifacts; it must use a new artifact
directory and retain the same outer/process diagnostics.

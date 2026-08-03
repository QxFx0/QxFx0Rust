# Morphology index optimization — 2026-08-01

This comparison uses the same machine, release profile, benchmark input and
sample counts as `2026-08-01-runtime-and-renderer-baseline.md`.

## Change

The production morphology runtime now owns each full `LexemeEntry` exactly
once. Surface indexes store compact stable lexeme ids, case/number and
confidence. A full `LexemeCandidate` is materialized only for the explicit
candidate API or an ambiguous result. Bundle validation also returns the
parsed entries and computed SHA-256, eliminating a second JSON parse and hash.

Lookup policy, source-tier ordering, ambiguity detection, manifest validation
and the serialized `MorphologyLookup` types are unchanged.

## Release in-memory turn

Command:

```bash
target/release/qxfx0 benchmark --samples 100 --warmup 10 --json
```

| Metric | Before | After | Change |
|---|---:|---:|---:|
| First turn | 946,378 us | 507,673 us | -46.4% |
| Steady p50 | 1,700 us | 1,622 us | -4.6% |
| Steady p95 | 2,361 us | 1,777 us | -24.7% |
| RSS after first turn | 452,083,712 B | 83,202,048 B | -81.6% |
| RSS after steady state | 452,091,904 B | 83,288,064 B | -81.6% |

The optimized release executable is 18,259,352 bytes. Its active pack
fingerprint remains
`deb023728e10a0ba2b3a475df7e303e3e7f0a617a97189d12104d64b2796166b`.

## Full cold process

Command:

```bash
python3 scripts/benchmark_runtime.py --samples 10
```

| Metric | Before | After | Change |
|---|---:|---:|---:|
| Process latency p50 | 1,027.246 ms | 539.360 ms | -47.5% |
| Process latency p95 | 1,041.340 ms | 543.574 ms | -47.8% |
| Peak RSS p50 | 464,818,176 B | 92,241,920 B | -80.2% |
| Peak RSS p95 | 465,469,440 B | 93,220,864 B | -80.0% |

Optimized binary SHA-256:
`a01d409dbb428c43f929b17a4b8e4486d84612939e946dbecfd31781f1e78520`.

## Decision

The clone amplification debt is closed. JSON/index initialization still
dominates a new process, but it no longer imposes a roughly 465 MB resident
set. A binary on-disk representation remains an optional future experiment;
it should be accepted only against this baseline and must retain manifest and
source-asset verification.

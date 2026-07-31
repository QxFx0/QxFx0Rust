# Runtime and renderer baseline — 2026-08-01

This baseline was captured on Linux 7.1.5 x86_64 with Rust 1.93.1 from the
measurement worktree based on commit `036f471`. It is a diagnostic reference,
not a release threshold. Filesystem page cache was not flushed.

## Release in-memory turn

Command:

```bash
target/release/qxfx0 benchmark --samples 100 --warmup 10 --json
```

Results:

- first turn, including fresh state and lazy indexes: 946,378 us;
- steady state: min 1,547 us, p50 1,700 us, p95 2,361 us, max 2,927 us;
- RSS: 4,292,608 bytes before initialization, 452,083,712 bytes after the
  first turn, 452,091,904 bytes after the steady-state run;
- executable: 18,262,328 bytes;
- embedded morphology bundle: 12,319,026 bytes, of which 12,318,729 bytes are
  `lexemes.json`;
- active pack fingerprint:
  `deb023728e10a0ba2b3a475df7e303e3e7f0a617a97189d12104d64b2796166b`.

## Full cold process

Command:

```bash
python3 scripts/benchmark_runtime.py --samples 10
```

Each sample used a new process, session and temporary SQLite database.

- latency: min 1,022.057 ms, p50 1,027.246 ms, p95/max 1,041.340 ms, mean
  1,028.425 ms;
- peak RSS: min 464,412,672 bytes, p50 464,818,176 bytes, p95/max 465,469,440
  bytes;
- release binary SHA-256:
  `5f9c2463af5bbca08eeb0c37a345af483b0c8b6fb9bcee35fe78f625965fb5e5`.

## Audited renderer diversity

Command:

```bash
target/release/qxfx0 renderer-audit --opening-words 3
```

- 30/30 audited topics produced a ready plan; none was blocked;
- 30/30 complete responses were unique;
- 70/99 normalized sentences were unique;
- one sentence kind accounted for all 29 repeated occurrences:
  `что думаешь об этом` appeared 30 times;
- 27/30 topic-normalized three-word openings were unique;
- the maximum repeated normalized opening appeared twice:
  `<topic> выражает отсутствие`.

## Decision

Warm turn latency is already small. Initialization and resident memory are the
dominant costs, so morphology representation is the next optimization target.
The current surface index stores cloned lexeme payloads in candidates; the next
change should replace those copies with stable lexeme references or compact
identifiers while preserving lookup, ambiguity, provenance and deterministic
ordering. Renderer expansion remains separate: the audit identifies the
universal agreement question as the first measurable repetition hotspot.

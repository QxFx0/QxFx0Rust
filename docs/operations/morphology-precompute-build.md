# Morphology runtime precompute

`qxfx0-morphology` loads a ~65 MB embedded `lexemes.json` on first lemmatizer use
per process. Under the 60 s idle cadence of `qxfx0 turn` (a fresh process each
turn, cold page cache on a contended host) this parse + `BTreeMap` index build
was the dominant `input_normalization_ms` latency tail (pre-fix p99 ≈ 1.8 s,
max ≈ 2.3 s, slow_turns > 0 at the 2,000 ms gate).

To remove it, the fully-built, indexed `MorphologyRuntime` is precomputed at
build time into a bincode blob and loaded via `bincode::deserialize` at runtime
(near-instant; 65 MB JSON → 20.7 MB blob; warm `morphology_init_ms` 384 ms →
194 ms; cadence-soak p99 2436 ms → 1709 ms at n=55).

## Files

- `data/lexemes.json`, `data/manifest.json` — canonical source assets (unchanged).
- `data/runtime.bin` — generated, NOT committed (`.gitignore`d derived
  artifact, blob-storage policy 2026-09-11), `include_bytes!`-ed by
  `qxfx0-morphology/src/runtime.rs`. A missing blob fails the build
  early via `qxfx0-morphology/build.rs` with the regen commands;
  freshness is enforced at runtime by `doctor` digests. There is no
  JSON fallback (fail-closed by decision).

## Regenerating the blob

Whenever `data/lexemes.json` or `data/manifest.json` change:

```sh
cargo run -p qxfx0-morphology --example prebuild_morphology_runtime
cargo test -p qxfx0-morphology --all-features
```

The test asserts the blob's `lexemes_sha256` matches the manifest-recorded hash
and that a fresh JSON parse yields the same lexeme count, so a stale `runtime.bin`
fails CI loudly.

## Release gate

See `docs/operations/audited-plan-latency-pilot-2026-08.md`. The cadence
soak (`scripts/diagnostic-soak-1000.sh`, `--max-response-ms 2000`) is the
end-to-end check; pre-fix slow_turns > 0, post-fix slow_turns = 0 with p99 ≈
1.7 s.

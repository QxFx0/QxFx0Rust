# QxFx0 Rust

Deterministic local dialogue runtime built around typed semantic graphs, Russian morphology and persistent multi-turn state. For the same input and the same starting state, QxFx0 produces the same response and the same persistent state.

The system is self-contained: it does not call an LLM or an external knowledge service. Its factual scope is therefore limited by the embedded graph. Unknown and external-world questions receive an explicit bounded response instead of fabricated knowledge.

## Current status

The CLI is the supported production surface. It includes:

- atomic SQLite persistence and automatic compatibility migration to schema v8;
- six-stage turn processing with guard rollback and governance events;
- 107 recognized topics, of which 30 have audited declarative content;
- 172 seed atoms, 276 semantic relations and 69 curated `FactRecord` values;
- bounded FactId-grounded positions and replay-stable semantic episodes;
- a manifest-validated active knowledge pack with a replay-visible SHA-256 fingerprint;
- 127 Russian surface templates and six-case morphology;
- a real Rust code registry with 97 typed atoms and type-directed composition edges;
- stable SHA-256 stage digests for deterministic replay diagnostics;
- bounded dialogue, governance, essence, commitment and runtime-graph state;
- a real `doctor` health gate and a strict CI release gate;
- verified online backups plus health, DB-size and response-latency metrics.

## Architecture

```text
qxfx0-cli          CLI: turn, chat, doctor, backup, metrics, sessions, code
       │
qxfx0-pipeline     Prepare → Route → Render → Finalize → Guard → Persist
       │
       ├── qxfx0-self         conatus, deliberation, Perspective, semantic episodes
       ├── qxfx0-semantic     parser, seed graph, activation, selection, composition
       ├── qxfx0-render       typed semantic-frame rendering
       ├── qxfx0-guard        input, quality and post-render safety gates
       ├── qxfx0-commitment   bounded semantic commitments and lineage
       └── qxfx0-governance   append-only replay-visible turn decisions
       │
qxfx0-persistence  SQLite sessions, graph and semantic state

qxfx0-code         independent typed Rust code registry and orchestrator
qxfx0-types        shared deterministic data model and state invariants
qxfx0-morphology   Russian case conversion and lemmatization
```

Persistent maps use ordered containers. Semantic-network caches are derived in memory, are invalidated when the graph changes and are deliberately excluded from persisted JSON.

Static knowledge packs are process-global and are never copied into
`SystemState`. A session stores only the active pack-set fingerprint so replay
cannot silently cross a semantic-authority change.

## Quick start

Build the CLI:

```bash
cargo build -p qxfx0-cli
```

Run a single turn in a named session:

```bash
cargo run -p qxfx0-cli -- \
  --db /tmp/qxfx0.db \
  --session-id demo \
  turn "что такое свобода?"
```

Continue the same session from another process:

```bash
cargo run -p qxfx0-cli -- \
  --db /tmp/qxfx0.db \
  --session-id demo \
  turn "я купил дом"
```

Interactive mode and other commands:

```bash
cargo run -p qxfx0-cli -- --db /tmp/qxfx0.db --session-id demo chat
cargo run -p qxfx0-cli -- --db /tmp/qxfx0.db sessions
cargo run -p qxfx0-cli -- --db /tmp/qxfx0.db doctor
cargo run -p qxfx0-cli -- --db /tmp/qxfx0.db doctor --json
cargo run -p qxfx0-cli -- --db /tmp/qxfx0.db metrics
cargo run -p qxfx0-cli -- benchmark --samples 100 --warmup 10
cargo run -p qxfx0-cli -- renderer-audit
cargo run -p qxfx0-cli -- --db /tmp/qxfx0.db backup /tmp/qxfx0-backup.db
cargo run -p qxfx0-cli -- discover свобода
cargo run -p qxfx0-cli -- code "посчитать сумму элементов"
cargo run -p qxfx0-cli -- code-stats
```

Example output:

```text
> я купил дом
Размышляя о доме, можно сказать следующее. Более того, дом и покой
переплетены. Взгляни на это так: эти вещи — дом и семья — идут рука об руку.
```

## Health check

`doctor` is an executable health gate, not an informational banner. It checks:

- SQLite `quick_check`, foreign keys, schema v8 and every stored session;
- seed-graph identities, endpoints, indexes and covered topics;
- concept, fact and active knowledge-pack manifests, hashes and conflicts;
- FactId-grounded Perspective capacity and curated counterpoint links;
- FactId-authorized stance rendering with fail-closed validation of persisted
  opinions;
- the non-promoting Haskell corpus pilot and its quarantine counts;
- embedded template syntax, weights and relation-type coverage;
- morphology manifest, hash, provenance, tier counts and ambiguity metrics;
- production code-registry identities, endpoints, indexes and `RelComposes` edges.

It exits non-zero if any check fails:

```text
QxFx0 Rust v0.1.1 health check:
  [OK] SQLite: schema v8, quick_check/foreign keys/session states valid
  [OK] Seed graph: 172 atoms, 276 relations, 107 covered topics
  [OK] Knowledge packs: active_packs=[philosophy-core-v1@1(...)], fact_conflicts=0, fingerprint=...
  [OK] Corpus import pilot: pilot_topics=300, already_active=5, quarantine=295, promotion_enabled=false
  [OK] Templates: 127 templates for 33 types; direct coverage 22/23 used relation types
  [OK] Morphology: seed dictionary and case conversion operational
  [OK] Code registry: 97 typed atoms, 1353 relations, 1322 RelComposes edges
  Status: OK
```

Use `doctor --json` for automation. The `metrics` command additionally emits
Prometheus gauges for doctor health, total DB/WAL/SHM bytes, doctor duration,
and the duration and health of an in-memory response probe.

## Performance and renderer baselines

The built-in benchmark separates the first lazy in-memory turn from a warmed
distribution. It reports min/p50/p95/max latency, resident memory before and
after initialization, executable size and the exact embedded morphology asset
size. Each measured turn uses a fresh state so session history does not skew
the result:

```bash
target/release/qxfx0 benchmark --samples 100 --warmup 10 --json
```

Full process startup is measured separately without requiring GNU `time`.
This runner starts a new process and temporary database per sample; it does
not flush the operating system's filesystem page cache:

```bash
python3 scripts/benchmark_runtime.py --samples 10
```

Renderer breadth is measured independently across all 30 audited topics. The
audit reports unique responses and sentences, repeated sentence counts, and
topic-normalized opening n-grams. It is diagnostic and does not change the
renderer or semantic state:

```bash
target/release/qxfx0 renderer-audit --opening-words 3 --json
```

## SQLite migration, backup and recovery

The database is upgraded automatically on open. Migration v8 is idempotent and transactional. It supports the historical `runtime_sessions` layout and deliberately leaves the legacy `schema_version` table untouched. File databases use WAL, foreign keys, a five-second busy timeout and `synchronous=NORMAL`.

Back up before upgrading a valuable database. The built-in command opens the
source read-only, uses SQLite's online backup API, verifies the partial copy,
and refuses to overwrite an existing destination:

```bash
cargo run -p qxfx0-cli -- --db qxfx0.db backup qxfx0-before-v7.db
cargo run -p qxfx0-cli -- --db qxfx0-before-v7.db doctor
```

If the migration or health check fails, keep the failed database for diagnosis and restore the backup while QxFx0 is stopped:

```bash
mv qxfx0.db qxfx0.failed.db
cp qxfx0-before-v7.db qxfx0.db
cargo run -p qxfx0-cli -- --db qxfx0.db doctor
```

Do not copy only the main database file while another process is writing in
WAL mode. Use the built-in `backup` command, or stop every writer and copy the
database together with any `-wal` and `-shm` files.

Session identifiers are part of the persistence boundary: a turn is rejected without mutation if its ID is empty, contains control characters, exceeds 128 characters or differs from the loaded state's ID.

## Determinism and observability

Determinism is verified both in-process and across fresh CLI processes. The pipeline exposes `process_turn_with_trace`, whose stage digests are SHA-256 over deterministic JSON. Replay comparison uses stage/input/output digests and excludes wall-clock durations.

The trace covers:

```text
prepare → route → plan_shadow → render → finalize → guard → persist → turn_output
```

Raw user text is not written to normal CLI tracing logs. Traces contain digests and bounded metadata.
The response-plan and turn-output steps include the active pack-set fingerprint.

## Knowledge packs and corpus audit

The active build embeds `data/packs/philosophy-core-v1`. Its manifest hashes
`concepts.json`, `facts.json` and `relations.json` before any record is
admitted. Duplicate IDs fail the complete active set, duplicate aliases remain
explicitly ambiguous, and conflicting facts fail closed.

Rebuild the current pack from audited Rust assets:

```bash
python3 scripts/build_core_pack.py
```

The Haskell corpus is not an active pack. The bounded importer is audit-only:

```bash
python3 scripts/import_haskell_corpus.py --limit 300
```

It writes a normalized inventory, an explicit quarantine and a hash-validated
metrics report under `data/imports/haskell-curated-pilot-v1`. Promotion is
disabled. The current source contains 6,239 rows and 12,478 surfaces but only
4,050 trimmed raw topic strings (4,040 after normalization); the 300-topic pilot admits
no new facts and quarantines 295 candidates for review. The source worktree was
dirty when the report was generated, which is exposed by `doctor` and must be
resolved before a production import pack can claim commit-exact provenance.

Production examples for daily backup retention, five-minute monitoring,
systemd timers and logrotate are in [`ops/`](ops/README.md). `metrics` exits
non-zero when doctor fails, DB storage exceeds its configured threshold, or
the response probe is invalid or too slow.

## State bounds

Long-running sessions enforce the following persistent limits:

- dialogue history: 10,000 responses;
- governance log: 10,000 events;
- essence witnesses: 32 by default;
- semantic commitments: 1,024;
- runtime graph: 10,000 atoms and 20,000 relations.

The integration suite includes a 1,000-turn full-pipeline soak test. It verifies that a repeated-topic workload stops growing the graph and remains valid at the end.

## Development and release gate

Run the same checks as CI:

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-targets
cargo build --workspace --release
target/release/qxfx0 --db /tmp/qxfx0-doctor.db doctor
```

CI and local release checks use the Rust 1.93.1 toolchain pinned in
`rust-toolchain.toml`, including the matching `clippy` and `rustfmt` components.

The exact test count is intentionally not hardcoded because it changes with
each semantic contract. The commands above are the authoritative release gate.

## Operational limits

- QxFx0 is a deterministic local semantic system, not a general-purpose factual assistant.
- Recognition covers 107 topics, but declarative rendering is currently admitted for only 30.
- There is no active autonomous learning or promotion loop; corpus expansion remains review-gated.
- External-world causal questions are explicitly marked as requiring external facts.
- The morphology engine combines a curated dictionary with heuristics; unusual names and unseen word forms can still be awkward.
- SQLite supports concurrent readers and serialized writers; it is not a distributed session store.

## License

MIT

# QxFx0 Rust

Deterministic local dialogue runtime built around typed semantic graphs, Russian morphology and persistent multi-turn state. For the same input and the same starting state, QxFx0 produces the same response and the same persistent state.

The system is self-contained: it does not call an LLM or an external knowledge service. Its factual scope is therefore limited by the embedded graph. Unknown and external-world questions receive an explicit bounded response instead of fabricated knowledge.

## Current status

The CLI is the supported production surface. It includes:

- atomic SQLite persistence and automatic compatibility migration to schema v7;
- six-stage turn processing with guard rollback and governance events;
- 107 curated topics, 142 seed atoms and 276 semantic relations;
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
       ├── qxfx0-self         conatus, salience, deliberation, essence trajectory
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

- SQLite `quick_check`, foreign keys, schema v7 and every stored session;
- seed-graph identities, endpoints, indexes and covered topics;
- embedded template syntax, weights and relation-type coverage;
- morphology probes;
- production code-registry identities, endpoints, indexes and `RelComposes` edges.

It exits non-zero if any check fails:

```text
QxFx0 Rust v0.1.1 health check:
  [OK] SQLite: schema v7, quick_check/foreign keys/session states valid
  [OK] Seed graph: 142 atoms, 276 relations, 107 covered topics
  [OK] Templates: 127 templates for 33 types; direct coverage 22/23 used relation types
  [OK] Morphology: seed dictionary and case conversion operational
  [OK] Code registry: 97 typed atoms, 1353 relations, 1322 RelComposes edges
  Status: OK
```

Use `doctor --json` for automation. The `metrics` command additionally emits
Prometheus gauges for doctor health, total DB/WAL/SHM bytes, doctor duration,
and the duration and health of an in-memory response probe.

## SQLite migration, backup and recovery

The database is upgraded automatically on open. Migration v7 is idempotent and transactional. It supports the historical `runtime_sessions` layout and deliberately leaves the legacy `schema_version` table untouched. File databases use WAL, foreign keys, a five-second busy timeout and `synchronous=NORMAL`.

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
prepare → route → render → finalize → guard → persist → turn_output
```

Raw user text is not written to normal CLI tracing logs. Traces contain digests and bounded metadata.

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

The workspace currently contains 358 Rust tests across unit, integration, migration, CLI replay and soak coverage. The count may increase; the commands above are the authoritative release gate.

The audited content-plan corpus is part of `cargo test --workspace --all-targets`.
Run it in isolation with:

```bash
cargo test -p qxfx0-pipeline --test structural_corpus
```

It validates all 30 admitted topics in fresh sessions and one shared 30-turn
session: topic and canonical slots, exact predicate set, claim roles,
derivation, provenance, no repeated claims, terminal punctuation, and explicit
fallback for recognized but unadmitted content. It observes `plan_shadow`; the
route-based renderer remains the default authority. The plan-renderer checks
in the same gate verify the exact curated surface for the same fresh and
long-session corpus.

To run the controlled audited renderer, enable the explicit CLI flag:

```bash
target/release/qxfx0 --db qxfx0.db --render-audited-plan turn 'что такое свобода?'
```

Without the flag, `legacy_shadow` remains authoritative and records a
plan-to-surface comparison in the render trace. With the flag,
`audited_plan` renders only admitted topic-backed `ReadyResponsePlan` values;
fallback, greeting, purpose and external-cause routes retain their existing
contracts.

## Operational limits

- QxFx0 is a deterministic local semantic system, not a general-purpose factual assistant.
- External-world causal questions are explicitly marked as requiring external facts.
- The morphology engine combines a curated dictionary with heuristics; unusual names and unseen word forms can still be awkward.
- SQLite supports concurrent readers and serialized writers; it is not a distributed session store.

## License

MIT

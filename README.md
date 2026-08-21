# QxFx0 Rust

Deterministic local dialogue runtime built around typed semantic graphs, Russian morphology and persistent multi-turn state. For the same input and the same starting state, QxFx0 produces the same response and the same persistent state.

The system is self-contained: it does not call an LLM or an external knowledge service. Its factual scope is therefore limited by the embedded graph. Unknown and external-world questions receive an explicit bounded response instead of fabricated knowledge.

## Current status

The CLI is the supported production surface. It includes:

- atomic SQLite persistence and automatic compatibility migration to schema v10;
- six-stage turn processing with guard rollback and governance events;
- 130 recognized topics, of which 60 have audited declarative content with
  129 typed claims;
- 20k-lemma noun morphology plus 30,809 digest-pinned verb paradigms
  (reflexive included), 42,239 adjective and 68 closed-class pronoun
  paradigms, rule-based out-of-vocabulary declension and preposition
  government;
- 207 seed atoms, 346 semantic relations and 129 curated `FactRecord` values;
- a 60-topic/129-claim audited ResponsePlan V2 corpus with manifest, replay,
  realization-parity and zero-downgrade gates;
- the «Кодекс» practice loop: deterministic topic revisits, prior-position
  callbacks, explicit contradiction events, practice-day reporting and a
  replay-verifiable diary export (`export` / `verify-diary`);
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
       ├── qxfx0-plan-v2      ResponsePlan V2 certificate chain (ADR-0034/0041)
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
cargo build --locked -p qxfx0-cli
```

Run a single turn in a named session:

```bash
cargo run --locked -p qxfx0-cli -- \
  --db /tmp/qxfx0.db \
  --session-id demo \
  turn "что такое свобода?"
```

Continue the same session from another process:

```bash
cargo run --locked -p qxfx0-cli -- \
  --db /tmp/qxfx0.db \
  --session-id demo \
  turn "я купил дом"
```

Interactive mode and other commands:

```bash
cargo run --locked -p qxfx0-cli -- --db /tmp/qxfx0.db --session-id demo chat
cargo run --locked -p qxfx0-cli -- --db /tmp/qxfx0.db sessions
cargo run --locked -p qxfx0-cli -- --db /tmp/qxfx0.db doctor
cargo run --locked -p qxfx0-cli -- --db /tmp/qxfx0.db doctor --json
cargo run --locked -p qxfx0-cli -- --db /tmp/qxfx0.db metrics
cargo run --locked -p qxfx0-cli -- benchmark --samples 100 --warmup 10
cargo run --locked -p qxfx0-cli -- renderer-audit
cargo run --locked -p qxfx0-cli -- --db /tmp/qxfx0.db backup /tmp/qxfx0-backup.db
cargo run --locked -p qxfx0-cli -- discover свобода
cargo run --locked -p qxfx0-cli -- code "посчитать сумму элементов"
cargo run --locked -p qxfx0-cli -- code-stats
cargo run --locked -p qxfx0-cli -- reflect
cargo run --locked -p qxfx0-cli -- --session-id demo report
```

Example output:

```text
> я купил дом
Размышляя о доме, можно сказать следующее. Более того, дом и покой
переплетены. Взгляни на это так: эти вещи — дом и семья — идут рука об руку.
```

## Кодекс — дневник размышлений (продуктовый режим)

`reflect` и `report` — первая продуктовая оболочка QxFx0: локальный дневник
размышлений с протоколом убеждений. Всё работает офлайн; приватность здесь —
архитектурный факт (в коде нет ни одного сетевого вызова), а не обещание.

Ежедневная петля:

```bash
# 1. Тема дня: детерминированный выбор из 60 аудированных тем (UTC-день),
#    тезис и контрпункт из проверенного корпуса, два вопроса для записи.
qxfx0 reflect
qxfx0 reflect свобода          # явная тема; неаудированная — отказ с ошибкой

# 2. Записать размышление в сессию-дневник.
qxfx0 --session-id diary turn "свобода для меня — это прежде всего..."

# 3. Протокол убеждений: какие темы возвращаются, какие позиции высказаны,
#    противоречия, состояние эссенции и governance-статистика.
qxfx0 --session-id diary report
qxfx0 --session-id diary report --markdown --out week-1.md

# 4. Верифицируемый дневник: Markdown с вложенным манифестом (входы, ответы,
#    дни, дайджесты состояний). Подпись парольной фразой — опционально.
qxfx0 --session-id diary export --out diary-2026-08.md
qxfx0 --session-id diary export --out diary-signed.md --passphrase "моя фраза"
qxfx0 verify-diary diary-signed.md --passphrase "моя фраза"
```

Подпись дневника — это сам детерминизм системы: `verify-diary` пересобирает
каждую запись этой же версией qxfx0 в чистой in-memory сессии и сверяет
ответы, дайджесты состояний каждого хода и итоговый дайджест сессии.
Изменённая хотя бы на букву запись не пройдёт проверку; HMAC-подпись
дополнительно доказывает авторство экспорта. Acceptance-тест
`codex_verifiable_export` проверяет: чистый экспорт верифицируется, правка
буквы в записи или ответе ломает верификацию, неверная парольная фраза
отклоняется, а правки прозы вне манифеста безвредны.

`reflect` не открывает базу вовсе; `report` только читает существующую базу
и сессию и отказывает (код выхода ≠ 0), если их нет — продукт не создаёт
файлы по опечатке в пути. Экспорт никогда не перезаписывает существующий
файл. Содержимое отчёта — чистая функция состояния: два отчёта по одному
состоянию совпадают байт-в-байт, включая SHA-256-отпечаток активного пака.

Политика возврата темы — детерминированная функция `(UTC-день, состояние)`:
сначала возвращаются темы, которым исполнилось минимум семь дней, затем ещё
не посещённые темы, а после обхода корпуса — самая старая позиция. Карточка
возврата показывает до двух прошлых позиций прямо в тексте и — вместо
корпусного контрпункта — бросает вызов сохранённой позиции: типизированное
оппозиционное ребро семантического графа («государство ограничивает
свободу») выбирается детерминированно из слов самой позиции. Если новая
запись противоречит сохранённой, это фиксируется как событие практики и
попадает в карточку и отчёт. Acceptance-тест `codex_thirty_day_practice` прогоняет 30
дней, проверяет непрерывность, возврат темы, динамику позиций и противоречие.

## Health check

`doctor` is an executable health gate, not an informational banner. It checks:

- SQLite `quick_check`, foreign keys, schema v10 and every stored session;
- seed-graph identities, endpoints, indexes and covered topics;
- concept, fact and active knowledge-pack manifests, hashes and conflicts;
- FactId-grounded Perspective capacity and curated counterpoint links;
- FactId-authorized stance rendering with fail-closed validation of persisted
  opinions;
- embedded template syntax, weights and relation-type coverage;
- morphology manifest, hash, provenance, tier counts and ambiguity metrics;
- production code-registry identities, endpoints, indexes and `RelComposes` edges.

It exits non-zero if any check fails:

```text
QxFx0 Rust v0.1.1 health check:
  [OK] SQLite: schema v10, quick_check/foreign keys/session states valid
  [OK] Seed graph: 207 atoms, 346 relations, 130 covered topics
  [OK] Content plan assets: recognition_topics_total=130, content_predicates_total=129, argued_topics_admitted=60, argued_predicates_admitted=60, profile_enabled=audited_v1
  [OK] Templates: 127 templates for 33 types; direct coverage 24/31 used relation types
  [OK] Morphology: seed dictionary and case conversion operational
  [OK] Verb lexicon: 30809 digest-pinned verb paradigms; conjugation probes operational
  [OK] Adjective lexicon: 42239 digest-pinned adjective paradigms; probes operational
  [OK] Pronoun lexicon: 68 digest-pinned closed-class paradigms
  [OK] Code registry: 97 typed atoms, 1353 relations, 1322 RelComposes edges
  [OK] Knowledge pack: active immutable pack fingerprint ..., 129 facts
  [OK] Curated FactRegistry: 129 curated FactId records re-resolve successfully
  [OK] Perspective boundary: bounded PerspectiveState valid; fact-grounded rollout default is Disabled
  [OK] Stance authority: signed attestation, bounded provenance, and temporal contract versions valid
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

Renderer breadth is measured independently across all 60 audited topics. The
audit reports unique responses and sentences, repeated sentence counts, and
topic-normalized opening n-grams. It is diagnostic and does not change the
renderer or semantic state:

```bash
target/release/qxfx0 renderer-audit --opening-words 3 --json
```

## SQLite migration, backup and recovery

The database is upgraded automatically on open. Migration v10 is idempotent and transactional. It supports the historical `runtime_sessions` layout and deliberately leaves the legacy `schema_version` table untouched. File databases use WAL, foreign keys, a five-second busy timeout and `synchronous=NORMAL`.

Back up before upgrading a valuable database. The built-in command opens the
source read-only, uses SQLite's online backup API, verifies the partial copy,
and refuses to overwrite an existing destination:

```bash
cargo run --locked -p qxfx0-cli -- --db qxfx0.db backup qxfx0-before-v7.db
cargo run --locked -p qxfx0-cli -- --db qxfx0-before-v7.db doctor
```

If the migration or health check fails, keep the failed database for diagnosis and restore the backup while QxFx0 is stopped:

```bash
mv qxfx0.db qxfx0.failed.db
cp qxfx0-before-v7.db qxfx0.db
cargo run --locked -p qxfx0-cli -- --db qxfx0.db doctor
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

The typed P4 catalog is `data/packs/catalog-v1`. It records four approved digest-pinned packs and five non-authoritative candidate/planned themes. Rebuild or verify all thematic assets deterministically:

```bash
python3 scripts/build_thematic_packs.py
python3 scripts/build_thematic_packs.py --check
```

Discovery does not grant authority; activation additionally requires approved lifecycle, the embedded digest allowlist, and complete pinned dependencies. Promotion gates are documented in ADR-0039.

The Haskell corpus is not an active pack. The bounded importer is audit-only:

```bash
python3 scripts/import_haskell_corpus.py --limit 300
```

It writes a normalized inventory, an explicit quarantine and a hash-validated
metrics report under `data/imports/haskell-curated-pilot-v1`. Promotion is
disabled. The pinned report records 6,239 source rows, 12,478 predicates,
4,050 trimmed raw topic strings (4,040 after normalization), and a clean source
worktree. The 300-topic pilot admits no new facts and quarantines 295 candidates
for review, including 271 candidates without typed slots. Promotion remains
disabled until those candidates pass the same audited admission boundary.

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
cargo audit --deny unsound
cargo fmt --all -- --check
cargo clippy --locked --workspace --all-targets -- -D warnings
cargo test --locked --workspace --all-targets
cargo install cargo-llvm-cov --version 0.8.4 --locked
mkdir -p coverage
cargo llvm-cov --locked --workspace --all-targets --lcov --output-path coverage/lcov.info
cargo llvm-cov --locked report --text > coverage/summary.txt
cargo build --locked --workspace --release
target/release/qxfx0 --db /tmp/qxfx0-doctor.db doctor
```

The release gate also runs all six ResponsePlan V2 contract gates, verifies
the explicit authority trace path and executes the positive/negative
behavioral authority matrix in
[`scripts/response-plan-v2-behavioral-canary.sh`](scripts/response-plan-v2-behavioral-canary.sh).

CI and local release checks use the Rust 1.93.1 toolchain pinned in
`rust-toolchain.toml`, including the matching `clippy` and `rustfmt` components.

The exact test count is intentionally not hardcoded because it changes with
each semantic contract. The commands above are the authoritative release gate.

### Acceptance tests

The regular workspace test command above includes the short black-box CLI
acceptance tests (`cli_backup_recovery`, `cli_fail_closed`, and the short test
in `cli_restart_soak`). They are non-ignored, so PR CI runs them without a
second acceptance-only test pass. Run just that short set locally with:

```bash
cargo test --locked -p qxfx0-cli --test cli_backup_recovery
cargo test --locked -p qxfx0-cli --test cli_fail_closed
cargo test --locked -p qxfx0-cli --test cli_restart_soak restart_load_acceptance_is_deterministic_across_twin_databases -- --exact
```

The longer operational checks are ignored by default and run only in the
scheduled/manual Operational Acceptance workflow. Run them locally by exact
test name (do not enable every ignored workspace test):

```bash
cargo test --locked -p qxfx0-cli --test cli_restart_soak restart_load_extended_soak_is_deterministic_across_twin_databases -- --ignored --exact
cargo test --locked -p qxfx0-cli --test cli_restart_soak concurrent_writer_lock_fails_cleanly_and_retry_succeeds -- --ignored --exact
```

The audited content-plan corpus is part of `cargo test --locked --workspace --all-targets`.
Run it in isolation with:

```bash
cargo test --locked -p qxfx0-pipeline --test structural_corpus
```

It validates all 60 admitted topics in fresh sessions and one shared 60-turn
session: topic and canonical slots, exact predicate set, claim roles,
derivation, provenance, no repeated claims, terminal punctuation, and explicit
fallback for recognized but unadmitted content. It observes `plan_shadow`; the
route-based renderer remains the default authority. The plan-renderer checks
in the same gate verify the exact curated surface for the same fresh and
long-session corpus.

To run the controlled audited renderer, it is now the default:

```bash
target/release/qxfx0 --db qxfx0.db turn 'что такое свобода?'
```

`audited_plan` renders only admitted topic-backed `ReadyResponsePlan` values;
for admitted topics it emits the curated Thesis/Контрпункт/Следствие surface,
for everything else (fallback, greeting, purpose, external-cause routes) it
retains the existing contracts. `--render-audited-plan` is kept as an explicit
alias; `--render-legacy` restores the legacy shadow renderer for A/B comparison.
`legacy_shadow` records a plan-to-surface comparison in the render trace.
See `docs/operations/audited-plan-latency-pilot-2026-08.md`.

The explicit `turn --response-plan-v2-authority` path is a separate three-topic
canary authority surface. It emits a verifiable external JSONL trace; the
default turn path remains unchanged unless an authority or renderer flag is
provided.

## Operational limits

- QxFx0 is a deterministic local semantic system, not a general-purpose factual assistant.
- Recognition covers 130 topics, with declarative rendering currently admitted for 60.
- The audited profile contains 129 typed claims across 60 admitted topics; each
  topic has a thesis and counterpoint, with an optional consequence where the
  corpus supplies one.
- The first product acceptance proof covers 30 deterministic journal days and
  requires visible position dynamics plus at least one contradiction.
- There is no active autonomous learning or promotion loop; corpus expansion remains review-gated.
- External-world causal questions are explicitly marked as requiring external facts.
- The morphology engine combines pinned paradigms with fail-closed rules: verbs outside the lexicon and nouns whose stem class is undecidable by ending are refused rather than guessed.
- SQLite supports concurrent readers and serialized writers; it is not a distributed session store.

## License

MIT

### Thesis projection rollout and schema v10

SQLite schema v10 additively reserves nullable `session_semantic.thesis_state_json`; v9 rows are not rewritten and NULL loads as an empty bounded projection. `ThesisProjectionRollout` is explicit and default-off: `Disabled` preserves the production path, while `Shadow` validates catalog-bound receipts without state mutation. There is deliberately no pipeline or CLI write mode: thesis lifecycle persistence requires a separate policy, retention/export/delete design, and evidence window. User or generated text never creates thesis authority.

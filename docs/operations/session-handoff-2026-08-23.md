# Session handoff — 2026-08-23 (branch `proposal/response-plan-v2-cohort-2026-08`)

Working backup of the 2026-08-23 session: what landed, in what state the
machine and pipelines are, and the ordered plan forward. Companion ADR:
`docs/adr/0043-subject-runtime-upgrade-u0-u6.md` (the upgrade concept this
session executes).

## 1. Session arc

1. **Audit** (three tracks: architecture / pipeline+latency / data+git).
   Key findings: one fail-open path (blob JSON fallback), dead
   `qxfx0-governance` crate, 21 `process_turn*` wrappers, ~5.1k lines of
   domain in the CLI crate, census literals across crates, README drift.
   Full reports were relayed in-session; the load-bearing items all became
   U0 below.
2. **Soak pipeline repair.** The v34 cadence-confirmation soak died at
   95/1,000 turns to a host OOM caused by a concurrent test suite
   (`docs/operations/audited-plan-latency-pilot-2026-08.md` has the
   postmortem). v35 relaunched behind a mechanical quiet-host gate.
3. **ADR-0043 written and adopted** as the upgrade concept: three laws
   (determinism behind the promotion boundary, fail-closed, incremental
   landing), phases U0–U6.
4. **U0 «Гигиена» — landed completely** (six commits).
5. **U1 «Субстрат» — landed** (`qxfx0 serve` daemon; components/ finding
   corrected: they are independently versioned nested repos).
6. **U2 «Ядро субъекта» — crate part landed** (`qxfx0-self-v2`: canonical
   Conatus + Essence with the B2 ablation hook). Pipeline wiring remains.

## 2. Commits of this session (e9d8481 → HEAD)

| Hash | What |
|---|---|
| `c9bd0bc` | soak v34 OOM postmortem, v35 quiet-gate relaunch |
| `480f83e` | ADR-0043 concept U0–U6 |
| `0b60e81` | U0.1 fail-closed blob init (noun + adjective + `QXFX0_DATA_DIR`) |
| `279ee95` | U0.2 dead `qxfx0-governance` removed; tests ported to types; `rusqlite`→dev-dep |
| `e49c8ce` | U0.3 README synced to live doctor (141/71/151, seven stages) |
| `0c9f0d3` | U0.4 census single source: gates bind to live corpus; `data/census.json` + `scripts/generate_census.py --check` in CI |
| `2ad2a9c` | U0.5 14 combinatorial `process_turn*` wrappers deprecated; prod code on the 4 canonical shapes |
| `0cd4f8c` | U0.6a `qxfx0-gates` extracted |
| `b09f5c8` | U0.6b `qxfx0-codex` extracted with the journal runtime (cycle broken) |
| `a7aff84` | U1 `qxfx0 serve` — unix-socket daemon, byte-parity + 50 ms warm gate |
| `ecc7339` | U2 crate `qxfx0-self-v2` — Conatus + Essence + ablation + doctor check |

Gate at HEAD: **867/867 workspace tests**, fmt clean, clippy
`--workspace --all-targets -D warnings` clean, all six V2 gates green,
doctor OK, census `--check` green. Workspace is now 18 crates
(+`qxfx0-gates`, +`qxfx0-codex`, +`qxfx0-serve`, +`qxfx0-self-v2`,
 +`qxfx0-bridge` in U4; −`qxfx0-governance`)
(+`qxfx0-gates`, +`qxfx0-codex`, +`qxfx0-serve`, +`qxfx0-self-v2`;
−`qxfx0-governance`).

## 3. Live pipelines and where their controls are

- **Soak v35** (cadence-gate confirmation): **LOST 2026-08-25** — the host
  rebooted (likely) and `/tmp/opencode/` was wiped together with
  `pilot.status`/`pilot.report`; no verdict, no soak process, watchdog gone.
  **CLOSED by v36** (`~/QxFx0Runtime/qxfx0-soak-v36-1000/`, launched
  2026-08-25 13:56Z against the release binary built 8 s after `ce59b14`
  landed — the shipping-tree requirement held): 1000/1000 turns,
  `turn_failures=0`, `slow_turns=0`, `final_doctor_ok=1`,
  `final_metrics_ok=1`, p50 352 ms / p95 557 ms / p99 697 ms. The owed
  closure edit in `docs/operations/audited-plan-latency-pilot-2026-08.md`
  is now done (2026-09-09 session): header + gate-verification + ops
  notes all record the v36 verdict.
  Driver script: `scripts/diagnostic-soak-1000.sh`; quiet-gate launcher
  recipe: wait for no cargo/cabal/test processes + ≥2 GiB `MemAvailable`
  before starting (the old `/tmp` launcher copy is gone; recreate before
  the next confirmation soak). Diagnostic dirs belong under
  `~/QxFx0Runtime/`, never `/tmp`.
- **Census**: after any content/morphology wave —
  `cargo build --release -p qxfx0-cli && python3 scripts/generate_census.py`
  and commit `data/census.json` together with the change; CI fails on
  drift.
- **Blobs**: regenerate per `AGENTS.md` (prebuild examples); parity gates
  live in `qxfx0-morphology` tests; blob corruption now panics fail-closed
  (ADR-0043 U0.1).

## 4. Environment gotchas (this host, this session)

- The rustup proxy layer is broken in the harness shell
  (`unknown proxy name`). Use the toolchain binaries directly:
  `PATH=/home/liskil/.rustup/toolchains/1.93.1-x86_64-unknown-linux-gnu/bin:$PATH cargo …`
- The ZCode AppImage intercepts some binaries (`pgrep` two-pattern forms,
  `ls` on removed paths); prefer anchored patterns and absolute paths.
- Another agent session runs long Haskell test suites on this host
  (`~/my-haskell-project/QxFx0`) — the exact load class that killed two
  soaks. Never build/test while a soak is in flight; the launcher's
  quiet-gate enforces it for the soak side.
- `ReadSessionContext` is overloaded for this session; read files directly.

## 5. Plan forward (ordered)

### U2 — finish «Ядро субъекта» (next)
1. Wire `qxfx0_self_v2::advance_essence` into the pipeline: build the
   `SelfBlanketSnapshot` from real state (morphology total = runtime
   lexeme count, identity claims = commitment store size, turn count),
   call in Finalize, record `EssenceAdvanceTrace` as nullable replay
   fields (pattern: thesis-projection Shadow fields).
2. **Shadow mode first**: compute + persist trace, change no routing; the
   plan-family guard (`validate_plan`) stays observational until a
   separate release flips it. Per ADR law 3.
3. Replay gate: before/after identical on the 71-topic corpus (assert in
   `structural_corpus`).
4. Ablation wiring: the B2 control arm reads `EssenceAblation::
   CommitDisabled` from an explicit test/CLI-only switch — never a runtime
   default.

**Status 2026-08-25**: items 1–4 are implemented, verified and committed.
The tree was written blind while soak v35 was in flight; after the soak
was lost with `/tmp` (§3), the full gate ran on a quiet host: fmt clean,
clippy `--workspace --all-targets -D warnings` clean, workspace tests
green (fixes during verification: four persistence schema-marker tests
and the `ops/README.md` marker updated 10→11), doctor healthy, census
`--check` green. What landed in the tree:

- `qxfx0-types`: `SemanticState.essence_v2` (nullable opaque JSON, the
  pipeline owns the typed decode, fail-closed) + the field rides the
  turn-rollback snapshot.
- `qxfx0-persistence`: schema **v11** (`essence_v2_json` nullable
  column, additive migration) + save/load.
- `qxfx0-pipeline`: `DeliberationTrace` carried through
  `PreparedTurnContext`; `advance_essence` called in `finalize_stage`
  from a real blanket snapshot (morphology runtime lexemes, pre-turn
  commitment count, turn ordinal; violations empty until the V2 blanket
  check ports in U3); `TurnOptions.essence_v2_ablation` (default
  `Enabled`); `PipelineTrace.essence_advance` nullable field, recorded
  only on non-blocked turns, outside replay digests; essence_v2 excluded
  from `response_plan_v2_state_parity` like `thesis_state`.
- `qxfx0-codex`: `run_journal_turn_with_essence_ablation`; journal
  `state_digest` and diary `session_digest` lift the shadow out before
  hashing, so **pre-U2 diaries still verify on post-U2 binaries**.
- `qxfx0-cli`: `turn --essence-v2-ablation enabled|commit-disabled`,
  threaded through the journal and doubt-shadow (incl. diagnostics)
  paths.
- `structural_corpus.rs`: B2 parity gate (enabled vs ablated arms over
  all 71 topics: response/family/blocked byte-equal, state parity, trace
  + persistence present) and the state-serialization round-trip gate.

Before committing run the full gate (§2 order): `cargo fmt`, workspace
clippy `-D warnings`, workspace tests, doctor, six V2 gates, census
`--check`. Suggested message: `feat(self-v2): wire advance_essence into
Finalize — shadow mode, B2 ablation switch, replay gate (ADR-0043 U2,
pipeline wiring)`.

### U3 — «Делиберация»
Port Field (five components) + Salience contributions + `reconcile`
replacing priority switching + doubt loop + episodic recall, each landing
in Shadow; calibration of constants stays deferred until a trace corpus
exists (discipline, not debt).

**Status 2026-09-09 (U3.1 landed)**: the canonical Salience controller is
ported into `qxfx0-self-v2::salience` (Haskell `QxFx0.Self.Salience`,
Phase 5 / ADR-0010): signed per-signal contributions, the uncontested
Conatus gate (bias 0 / confidence 1 / `conatus_gate`), sigmoid bias,
dispersion confidence, dead-band hemisphere dispatch (Tied → formal),
`is_holistic_family`, bounded Phase-B weight adaptation (lr 0.02, clamp
[0,2], anti-drift 1.0, zero-signal identity), and
`validate_salience_invariants` wired into the doctor self-layer check via
`validate_invariants`. Shadow integration: `advance_essence` computes the
verdict and carries it as `EssenceAdvanceTrace.self_verdict` (nullable,
serde-default — pre-U3 trace JSONs load unchanged; never enters the
witness hash, persisted state or replay digests). Locks: 12 salience unit
tests + the structural-corpus shadow gate asserts the verdict is
replay-visible on every turn with behaviour byte-identical to the ablated
arm. Content saliency stays 0.0 (spectral clustering deferred). Gate at
HEAD: 896/896 workspace tests, fmt/clippy clean, six V2 gates green,
doctor OK, census `--check` green. Remaining U3: reconcile replacing the
priority switch in route (shadow), doubt loop, episodic recall.

**Status 2026-09-09 (U3.2 blanket landed)**: the V2 structural self-blanket
is ported into `qxfx0-self-v2::blanket` (Haskell `QxFx0.Self.Invariants` +
`QxFx0.Self.Blanket`): `check_initial_blanket` (non-empty session identity,
strictly-positive morphology) and `check_blanket_transition` (adds session
stability, turn-count and identity-claim monotonicity). The transition check
needs the *previous* blanket across turns, which a fresh per-turn process has
nowhere to keep, so it persists as a new observational field
(`SemanticState.blanket_v2`, opaque JSON, the pipeline owns the fail-closed
decode; rides the rollback snapshot; outside `response_plan_v2_state_parity`
and lifted out of the journal/diary digests exactly like `essence_v2`, so
pre-U3 diaries verify). `finalize_stage` checks the blanket every turn:
the violation list now feeds `compute_conatus_energy` (−λ·|v| penalty) and
rides `EssenceAdvanceTrace.blanket_violations` (nullable, serde-default,
`skip_serializing_if = "Vec::is_empty"`) — the U2 comment "violation list is
empty by construction" is retired. Ruptures are fail-closed DATA (a
`tracing::warn`, never a turn-path panic), matching the Haskell
`IdentityRupture`-as-data reading. Persistence: schema **v12**
(`blanket_v2_json` nullable column, additive migration mirroring v10/v11;
the v9→current test asserts the new column is NULL on legacy rows).
On healthy sessions the list is empty by construction still — but that is now
a *checked* fact (the erosion trigger has teeth if it ever breaks), not an
assumption. Locks: 5 blanket unit tests + the corpus shadow gate; workspace
green.

**Status 2026-09-09 (U3.3 reconcile + doubt loop landed, shadow)**:
`qxfx0-self-v2::deliberation` ports the Haskell Phase-8 six-rule ladder
(`reconcile` keyed on the canonical `SelfVerdictV2`: Conatus override →
agreement → salience lead above the 0.7 escalation floor → single-axis
advantage → tied-fallback-formal; recovery causes merge by severity and are
never silenced; the Rust plan vocabulary carries family + recovery +
confidence, so the differ classification and divergence denominator are 4→2
in lockstep with the record). `proposal_pair_from_field` materialises both
hemispheric proposals (editorial content mirrors V1 prepare's proposal
shapes; the algebra is canonical). `deliberate_shadow` adds the doubt loop
(Haskell doubt law keyed on the canonical gate: complement of confidence,
gate floor 0.9, counterfactual/content ambiguity +0.2, threshold 0.75,
same-topic-confirmed suppression with V1 recall semantics) and the
applied-vs-reconciled comparison, attached in `finalize_stage` as
`EssenceAdvanceTrace.deliberation_shadow` (nullable, serde-default —
pre-U3.3 trace JSONs load unchanged; never feeds routing, the witness hash
or persisted state). Route is untouched: V1 `family_for_mode` stays the
authority; the flip reads the reconciled==applied statistic off the trace
corpus in a separate release (law 3). The suppression fact rides
`PreparedTurnContext` (`serde(skip)` so stage digests stay byte-identical).
Doctor: `validate_deliberation_invariants` joined the self-layer check.
Locks: 11 deliberation unit tests + the corpus shadow gate (replay-visible
on every turn, records the applied family, routing byte-equal to the
ablated arm). **U3 is now complete in shadow**; episodic recall V2-port is
intentionally left to the doubt-policy consumer at flip time (V1 recall
already runs in `record_doubt_shadow`/`clarification_decision` and the V2
shadow mirrors its suppression semantics exactly).

### U4 — «Мост обучения»
`qxfx0-bridge` behind a feature flag (no network in default builds),
runtime edge store with reinforce/decay/retire (Haskell
`RuntimeLLMFeedback` is the spec), corroboration queue, quarantine tables
in SQLite. Gate: zero visible behavior change.

**Status 2026-09-10 (U4.1 landed)**: `qxfx0-bridge` exists as the
workspace's 18th crate — the trust boundary's pure core, nothing wired
into the turn path.
- `runtime_edges` is the exact `RuntimeLLMFeedback` port: positive
  corroboration `+0.05` and co-occurrence `+1`, negative `−0.10` to a
  floor of 0.0, conflict retires, promotion flips the source tag at
  `confidence ≥ 0.75 ∧ co_occurrence ≥ 3` (Haskell
  `ProvenanceDialogueFeedback` ≙ our `BridgeEdgeSource::Promoted`);
  turn-boundary decay `×0.95` skips topic-touching edges, retires under
  0.3, and the runtime-bridge tier prunes to 500 by (confidence desc,
  key asc) so the map stays deterministic. The store is
  `BTreeMap<(AtomId, AtomId), BridgeEdge>` — surface-free, never
  renders.
- `corroboration` is a bounded FIFO that a between-turn worker drains
  into the store (insertion order, back-pressure reported via
  `push -> bool`, first positive sighting creates the edge, negative
  and conflict never fabricate one). Default queue capacity 1024.
- `candidates` is the source seam — `CandidateSource` trait, `Noop`
  (default) and `Scripted` (offline determinism) implementations,
  `CandidateError` as fail-closed hard stops. `HttpCandidateClient`
  exists only under the `llm-candidates` cargo feature and even there
  fails closed as `Transport`: adding a review-gated HTTP crate is
  its own supply-chain decision, deliberately deferred past landing
  the algebra.
- Doctor: `validate_bridge_invariants` rides the new `Learning bridge`
  check; the check also asserts the default build carries no network
  surface via a CLI forwarding feature (`llm-candidates` pulls in
  bridge's — enabling it honestly flips the doctor to fail on network
  capability). `checks.len()` lock moved 15 → 16. Census is unaffected
  (bridge has no content-bearing counts).
- 16 unit tests: full ladder (positive / negative / conflict /
  promotion / decay-skip / retire / prune tie-break), queue semantics
  (back-pressure, first-sighting, no fabrication, promote-through-
  drain), offline source determinism, invariant lock.

**Status 2026-09-10 (U4.2 landed — U4 complete)**: the worker, the
quarantine and the schema landed, and the ADR's "zero visible behavior
change" gate became executable.
- `qxfx0-bridge::worker` — `process_turn_boundary` is the pure between-turn
  cycle: admission first (`admission_reason` refuses unknown endpoints,
  empty identities and self-loops — every refusal is quarantined with a
  named reason, never silently dropped), then the reinforce/retire fold,
  then boundary decay + cap prune. The `BoundedCorroborationQueue` gained
  `into_events` for the pass; `encode_store`/`decode_store` persist the
  BTreeMap as a key-sorted edge list (non-string map keys do not serialize
  as JSON objects; a malformed blob is a decode error, never a silent
  empty store).
- `qxfx0-bridge::quarantine` — bounded insertion-ordered `QuarantineLedger`
  with `QuarantineReason` (`UnknownEndpoint` / `Degenerate` /
  `QueueOverflow` / `QuarantineFull`); this list is exactly the U5 review
  queue.
- Persistence schema **v13**: `session_bridge_edges` (one opaque
  serialized store per session) and `session_bridge_quarantine`
  (append-ordered `(session_id, seq, entry_json)`), both `ON DELETE
  CASCADE` beside the state, with SQL-edge bounds (4 MiB store, 64 KiB
  entry, 4096 rows; a full ledger errors rather than evicting evidence);
  `delete_session` cleans them; v12 databases migrate empty-table-clean.
  Bridge blobs stay opaque to persistence — the codec lives in the bridge.
- `qxfx0 bridge-maintain [--json]` (CLI): runs the decay/prune half per
  session that carries a store (topic from `dialogue.last_topic`, known
  atoms from the runtime graph; queue empty — candidate fetching is U5,
  the live queue is daemon-side). Idempotent; a drained-to-nothing store
  clears its row so a drained session is indistinguishable from a
  never-touched one.
- The U4 gate (`qxfx0-pipeline/tests/bridge_sleeps.rs`): manifest scan —
  only `qxfx0-cli` (runtime) and dev/test edges may name the bridge; and
  corpus equality — the twelve soak prompts run twice with a live,
  populated bridge store kept open, byte-identical on responses,
  families, guard verdicts and parity state, `SystemState` JSON still
  free of bridge fields. Gate at HEAD: fmt/clippy clean (default +
  `llm-candidates`), 947/947 workspace tests, six V2 gates green,
  doctor OK (16 checks incl. `Learning bridge`), census OK, coverage
  floor holds.

Remaining U4: none — the between-turn worker is reachable
(`bridge-maintain` + the pure cycle), candidate source wiring and
promotion are U5 proper. The serve-daemon queue host is an operator
wiring decision for when U5 needs it.

### U5 — «Промоушен»
Gates (informativeness threshold, versioned policy, draft overlay →
activate → human release → rollback), CLI `promotion …`, fingerprint in
the replay envelope, and the corpus bridge: promoted candidates must pass
the same admission bar as editors; `import_haskell_corpus.py` quarantine
becomes an input.

**Status 2026-09-10 (U5.1 landed)**: the promotion boundary is the bridge's
own pure module (`qxfx0-bridge::promotion`), gated and human-released, and
*by design a released overlay never mutates the graph or a session's pack
fingerprint on the turn path*. The operator admits a released, checksummed
artifact into the embedded pack through the editorial bar — the ADR-0043
law-1 fingerprint mechanism — exactly how a content wave or the Haskell
importer's quarantine becomes input.

- `promotion` (pure, in the bridge): versioned+SHA-checksummed
  `GatePolicy`; `evaluate_candidate_informativeness` is the exact
  Haskell port — normalizeAtom, stop-word-filtered >3-char Jaccard vs the
  curated topic surfaces, semantic-gain ≥ 0.75, the constraint-relation
  novelty disjunct, and the tautology/topic-paraphrase refusals; the
  immutable lifecycle `create_draft → Overlay::activate → Overlay::release`
  with `rollback` moving only the active pointer. `verify_integrity`
  re-checks the content address; a Released overlay refuses re-release and
  re-activation (release is permanent, enforced by the transition table).
- persistence schema **v14**: `promotion_overlays` (status CHECK
  Draft/Activated/Released, snapshot id, optional parent, SHA checksum,
  opaque `overlay_json`) + a singleton `promotion_active` that may only
  point at a Released version. `save_promotion_overlay` inserts idempotently
  by version and errors on a checksum conflict;
  `replace_promotion_overlay_if_matches` is a pinned CAS that refuses a
  lost update and a mid-lifecycle content-address change.
- CLI `promotion list/draft/approve/release/rollback [--json]` (the
  `PromotionSurface` helper in `qxfx0-cli`). `draft` enumerates the
  Promoted tier across every session, deduplicated by canonical triple
  (snake-cased `RelationType` slug), with a content-derived snapshot id
  (never the clock), so a retry yields the same candidate set and the same
  overlay version; baseline = argued-corpus thesis/counterpoint/consequence.
  `now_unix_seconds()` is sampled only by CLI commands, never on the turn
  path. `validate_promotion_invariants` rides the new doctor `Promotion
  boundary` check (checks 16 → 17).
- gate: `a_released_active_overlay_leaves_the_corpus_byte_identical` (in
  `bridge_sleeps.rs`) drives a real draft→approve→release lifecycle, points
  the active singleton, runs the twelve soak prompts, and asserts
  byte-identical responses/families/guard + `response_plan_v2_state_parity`
  + `SystemState` JSON free of promotion/overlay fields. The bridge
  link-closure scan still forbids a runtime edge to the bridge outside the
  CLI. 14 promotion unit tests (bridge) + 5 store tests + a CLI lifecycle
  unit test (draft→approve→release→idempotent re-draft→rollback, with the
  Draft-cannot-release and Released-is-immutable refusals) + a CLI
  integration test.

Gate at HEAD: fmt/clippy clean (default + `llm-candidates`); workspace
tests green; six V2 gates green; doctor OK (17 checks); census `--check`
green; coverage ≥ floor.

**Status 2026-09-11 (U5.2 landed)**: the three remaining promotions
landed as one coherent increment. (1) The import feed:
`promotion draft --quarantine PATH` mixes an offline quarantine JSONL
into the same snapshot digest, admits resolvable rows through the
ordinary ladder, and surfaces per-line refusals — a corrupt byte stream
fails closed before touching the store. (2) Admission is enforced at
draft via `create_draft_with_admission` (seed-atom bar against the
union of session graphs, counterpoint bar against the argued registry).
(3) Empirical pre-activation: `promotion evaluate` runs the structural
corpus precheck (overlay predicate topics ∪ fixed 12-topic set), persists
a content-addressed trial row to the new v15 `promotion_evaluations`,
and `promotion approve` binds to the latest trial that passed for this
exact content (checksum-pinned; a re-draft cannot inherit an old verdict).
`promotion revalidate` reports policy/baseline drift without touching a
row. Remaining U5: the *runtime* AB leg of the corpus evaluation (a
separate method row for when an overlay can actually render); the
machine for it already exists in the row shape.

### U6 — «Свидетельства»
Dual journal (subject positions symmetric with practitioner positions),
verifiable export as the FELT measurement instrument (B2 rubrics get
reproducible artifacts).

**U6.1 LANDED 2026-09-11** (997 tests green, `cargo build --locked
--workspace --release` + fmt + clippy + full tests + doctor 18/18): no new
crate, no schema change, no turn-path touch. `qxfx0-codex::dual_journal`
zips turns 1..=turn_count with the subject's persisted position (V1
witness driver/rule/agreement/divergence/conatus, commitment coverage,
reset flags; V2-shadow presence as a session flag; pre-journal gaps
flagged, never filled; per-turn family honestly absent). `qxfx0-codex::felt`
is the six mechanical M6 gates (10-turn floor, 2-turn definition, 2-topic
distinction, contradiction-with-retention, full id accounting; empty fails
all six) plus the `felt-export`/`felt-verify` artifact pair — the manifest
embeds the dual evidence, verify re-evaluates the same pure core with no
DB, `session_digest` cross-checks against the diary export. `felt-export`
never refuses a thin session («not proven» is the testimony).
`validate_felt_invariants` rides doctor `Felt evidence` (17 → 18 checks).
Remaining: U6.2 flip criteria + B2 rubric bindings.

**U6.2 LANDED 2026-09-11** (full gate green, doctor 19/19): the B2
verdict procedure is a deterministic library (`b2_report::run_b2_report`
in pipeline; probe example now a thin printer with byte-identical
output), and `qxfx0-codex::flip` drafts the machine-checkable flip
proposal — five rubrics (felt-sustained ≥5 distinct ≥10-turn sessions;
b2-enabled-commits; b2-violations-bounded below the hysteresis window;
b2-ablation-control; b2-guard-stable) over verified FELT exports plus a
live B2 re-run. CLI `flip-draft`/`flip-verify`; doctor `Flip readiness`
(18 → 19). The draft never flips: migration stays a human-reviewed
change. U6 complete; the flip awaits sustained practice + a blessed v2
tuning.

### Deferred / operational
- **U1.5**: noun blob → columnar mmap (with the daemon, paid once per
  process; do it when CLI-mode cold start matters again).
- **Soak closure edit** in the ops doc when v35 completes (see §3).
- **Blob storage policy** (LFS or release artifacts) before promotion
  starts multiplying content — decide before U5, not after.
- **Cross-twin conformance**: shared golden corpus with the Haskell
  reference (fixture parity tests first; `qxfx0-self-v2` is the natural
  first parity target against `QxFx0.Self.Conatus/Essence`).
- `components/` services may re-target the canonical `qxfx0-serve` layer
  (their git-rev pins predate all of U0–U2).

## 6. Where the knowledge lives

- Concept & laws: `docs/adr/0043-subject-runtime-upgrade-u0-u6.md`
- Latency saga & soak ops: `docs/operations/audited-plan-latency-pilot-2026-08.md`
- Census workflow: README «The content census…» + `scripts/generate_census.py`
- Serve protocol: README «Long-lived daemon (ADR-0043 U1)»
- Haskell reference (readiness map): `~/my-haskell-project/QxFx0`
  (`AGENTS.md` fact-checks, `ROADMAP.md`, `src/QxFx0/Self/*`)

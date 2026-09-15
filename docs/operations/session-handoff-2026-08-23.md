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
row. Remaining U5: the Haskell-corpus promotion output feeding the
editorial bar (the runtime-AB leg landed as U5.3 below).

**U5.3 LANDED 2026-09-11** (full gate green): the runtime-A/B leg —
`promotion-runtime-ab-01` as the second evaluation method. `promotion
evaluate-runtime` snapshots the operator DB twice into temp (never
mutated), carries the overlay as held positions (turn-0 commitments) in
every candidate scratch session, and renders each case topic once per
arm (`Что такое {topic}?`, audited-plan, day derived from
`completed_at`): overlay topics plus the fixed 12-set, regression =
fixed minus overlay's. Pass = regression byte-identical AND
guard-stable across arms; overlay-topic divergence recorded, never
gating. Trial rows persist in `promotion_evaluations` (content-
addressed id binds the measured outputs); `approve` now binds to the
latest passing row of EACH method for the exact checksum. Empirical
proof the mechanism is real: the lifecycle test's admitted свобода
triple quotes on its own topic while all 11 regression topics render
identically. Remaining U5: only the Haskell-corpus feed.

**U5.4 LANDED 2026-09-11** (full gate green): the editorial feed closes
the loop back toward the Haskell corpus — `promotion export-pack`
writes a Released overlay's predicates as versioned machine JSON
(`promotion-export-pack-01`: surfaces, confidence/support/gain, both
bound evaluation ids, policy pin, overlay checksum) for the human merge
into the pack sources. Refuses anything but Released; never creates a
DB on a mistyped path; never overwrites. Automation stops at the file:
graph effect still comes only from editorial admission, pack gates
validate the merged result. U5 complete: draft → admission →
revalidate → structural precheck → runtime A/B → bound approve →
permanent release → editorial feed.

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

### First practice + calibration + ready proposal (2026-09-11)
Five sustained CLI sessions (12 turns each, two topics, challenged
positions) on a scratch DB, all `felt-export`ed and verified clean:
4/5 proven, s4 honestly not-proven (no retained contradiction). The
practice caught a real miscalibration: `governed-evidence` required
pack-binding, but the fact-grounded rollout is `Disabled` by default
(asserted in code) — no default session is ever pack-bound, so the
gate was unpassable law. Recalibrated the same day: governed =
validates + non-vacuous (≥1 turn); fingerprint stays embedded as
cross-reference, not verdict; regression-locked by unit test.
`flip-draft` over the 5 exports + full 141-prompt corpus: all five
rubrics PASS (felt-sustained 5/5, commits 1, max_run 4 ≤ 7, control
43/0, guard 0/0) — the first ready flip proposal, verified clean.
The migration itself stays a human-reviewed change: the proposal is
on the table, the decision is not taken.

### Cross-twin parity (post-U6, pre-tuning)
First target landed 2026-09-11: Conatus golden parity —
`qxfx0-self-v2/tests/fixtures/haskell_conatus_golden.tsv` holds 12 real
Haskell vectors (`QxFx0.Self.Conatus` @ `~/my-haskell-project/QxFx0`
7cbc0ba, generator in `/tmp`, tree untouched), pinned by
`conatus_haskell_parity.rs` at 1e-12 relative. The port is bit-faithful;
v2 tuning now has a floor it cannot silently redefine. Next targets, in
order: Essence advance/commitment/violation dynamics, then Salience,
then Deliberation.

Second target landed 2026-09-11: Essence golden trajectories —
`qxfx0-self-v2/tests/fixtures/haskell_essence_golden.tsv` holds 31 real
Haskell rows over two scripted scenarios (A: angst accrual → 0.75-fire,
override-hold, decay, sub-floor hold, band edges, erosion-window miss;
B: full 8-window conatus erosion fire, window-break recovery), replayed
natively step-for-step (`witness` → angst/floor/count/stored bands →
`shouldCommit` → `extractMode`) at 1e-12. Two honest boundaries
documented in the fixture: witness hashes are per-twin encodings
(aeson-generic `ew-` names vs serde snake_case, never compared across
twins) and `validatePlan` needs family/tone/style mapping (later
target). Remaining parity: Salience, then Deliberation.

Third target landed 2026-09-11: Salience golden cases —
`qxfx0-self-v2/tests/fixtures/haskell_salience_golden.tsv` holds 14 real
Haskell rows (`computeSelfVerdict` under builtin weights: gate fire and
boundary, all-zero, dominance, magnitude ties, both extremes, content
signal, dead band both sides, saturation), replayed natively at 1e-12
(bias, confidence, driver tag, dispatch verdict + margin). Weight
calibration and `adaptSalienceWeights` stay out of scope (Phase-7
territory). Remaining parity: Deliberation — then the tuning floor is
complete.

Fourth target landed 2026-09-11: Deliberation golden cases —
`qxfx0-self-v2/tests/fixtures/haskell_deliberation_golden.tsv` holds 8
real Haskell rows over the shared family×recovery subset (style/tone
pinned equal, recovery None/Gate, courtesy None): all five rule paths
(Agreement, Holistic/ FormalAdvantage, SalienceLead both sides,
TiedFallback, ConatusOverride) with agreement, reconciled
family/confidence/recovery all matching. The port's vocabulary
reduction is now recorded truth, not suspicion: divergence divisor
count/4 vs count/2 (values compared only at 0.0), Style/Tone agreement
classes unreachable in Rust, recovery ladder 10 rungs vs 3 buckets
(picks agree on None/Gate), ConatusOverride reconciled plan
structurally diverged (rule/agreement compare, plan doesn't). The
tuning floor is complete: Conatus, Essence, Salience bit-faithful;
Deliberation rule skeleton pinned with known boundaries. Next: the v2
tuning itself (hysteresis per ADR-0044), then the probe re-run.

### V2 tuning (ADR-0044 hysteresis, landed 2026-09-11)

`qxfx0-self-v2::advance_essence` policy + topic threading in pipeline:
violation decay (admissible same-topic turn decays, not zeroes),
per-topic commitments (counter moves only on the commitment's topic;
pre-tuning `None` stays universal), lifetime budget (default 3, crossings
recorded but suppressed past it). Unit-locked (decay cadence,
cross-topic neutrality, unscoped compat, budget testimony); parity
green; probe re-run identical numbers (non-degenerate dynamics
preserved). Recorded in ADR-0044 (tuning + re-run sections).

### Migration M1 (ADR-0044, landed 2026-09-11)

Flip proposal reviewed: МИГРИРУЕМ. `TurnOptions.subject_authority`
(`V1Authority` default, `--subject-authority-v2` opt-in); Prepare
reads the V2 energy scalar + bias under V2, `f64` plumbing unchanged
downstream; journal records carry the per-turn authority label and
diary replay honors it per entry (pre-migration defaults V1, unknown
fails closed). Zero drift under the default. Next: M2 Deliberation
(ladder + documented mapping), M3 witness+commitment,
M4 default flip with re-baselining + soak.

### Migration M2 (ADR-0044, landed 2026-09-11)

Prepare runs the canonical ladder under V2 (`proposal_pair_from_field`
+ `reconcile` over the canonical verdict) and maps the result onto the
working-layer shape: rules 1:1, agreement Agree-only, divergence
passthrough (V1 angst keys on the same boundaries), V1 driver
convention. Witness/commitment stay V1; the mapping retires with them
in M3. Unit-locked (full rule×agreement mapping, override shape,
ladder-derived prepare output). Next: M3, then M4.

### Migration M4 (ADR-0044, landed 2026-09-11)

Default flipped to V2. B2 and the runtime-AB harness pin V1
explicitly; re-baselined to the live layer: essence-practice angst,
anomaly crafting + replay, same-authority digest comparisons,
trajectory accumulation, V1-default test. Every drift triaged as
legitimate V2-sourced behavior or pin-needed comparison, zero
unexpected. V1 modules retire one by one from here; the flip soak
re-runs below.

### Density doctrine, first instruments (landed 2026-09-11)

Strategic goal recorded: meaningful dialogue — understand, don't
guess; fewer words, more meaning per word. First two instruments:
`content_novelty` (top-down saliency: uncovered share of prompt
content words; bare≈0, substantive mid, unknown=1.0 — measured, not
assumed; spectral clustering explicitly deferred: no embedding graph
exists to cluster over) feeding the V2 salience controller on the
live path, and `concentrate` (arousal > 0.6 → densest sentence,
smoothed density so one-word sentences can't win; audited path
never concentrates). Haskell `decompressForReceiver` analog with
one honest difference: density measured, not positional. The soak
caught one real defect pre-commit: an overlap-blind concentrate
tripped the guard's zero-topic block — the winner must now share a
lemma with the topic or the full text stands. Next under
the doctrine: input frame semantics (biggest gap), compositional
inference, calibrated saliency weights.

### Input frames v1 (landed 2026-09-11)
Haskell `input_semantic_contract` analog, v1 scope: `raw_text →
WordUnit[] → InputFrame → route_hint → route/family`, legacy
detectors as fallback. Frame carries units (lemma + confidence,
every token exactly one), clause/speech act, single-negation
polarity, prepositional topic, focus, agent/target; hint fires only
at ≥0.8 (greeting shape + second-person mental-verb questions —
the latter is genuinely new signal: the cascade has no shape for
`ты помнишь меня?`). Route consults the hint, everything else keeps
the parser mode: zero drift outside pinned patterns.

### Frame consumers (landed 2026-09-11)Units carry morphology POS; focus accepts open-class content only
(verbs/adverbs/closed classes never emphasize; mental verbs name
the act, not its object). First consumer: clarification names a
focus distinct from the routed subject, otherwise the historical
wording stands byte-identically. Agent/target ride as tested API
awaiting response consumers (same pattern as polarity). Next:
compositional inference, calibrated saliency weights.

### Compositional inference (landed 2026-09-11)
Haskell `Logic/Inference` analog, bounded: transitivity over 10
carried types (5 Haskell types have no Rust vocabulary — recorded,
not silently dropped) and symmetry over 5, fixpoint ≤5 rounds,
≤128 new edges per call, deterministic order, existing triples win,
no self-loops. Derived edges carry `Inferred` source + `rationale`
path and validate; they enrich activation/paths in finalize growth
(under the same edge bound) but never promotion evidence
(generated text must not self-promote). Unit-locked (chain,
two-hop fixpoint, symmetry, determinism) + integration through a
real turn.

### Belief revision (landed 2026-09-11)

Haskell `Logic/BeliefRevision` analog, bounded: a caught
contradiction weakens the weaker live side (halved; ties weaken the
challenger — held positions stand), quarter-steps direct dependents
(one level), quarantines below 0.3 with `ParserContradiction`
lineage (never deleted; FELT id-accounting holds). Wired into
Finalize next to the angst accrual. Unit-locked (weaken, tie,
quarantine+propagation, retired-id no-op). Doctrine loop closed:
positions now move instead of standing forever.

### Calibrated saliency weights (landed 2026-09-11)

Haskell corpus-tuning promotion adopted with provenance
(best-non-regressing grid candidate, uniformly +0.003, thresholds
untouched): `CALIBRATED_SALIENCE_WEIGHTS` + `calibrated()`, wired
into the three production sites (Prepare live path, hemisphere
dispatch, essence self-verdict). `Default` stays builtin
(parity-pinned); doctor enforces the nudge bound (±0.05, thresholds
identical). Re-baseline: zero drift — all 1062 green unchanged,
consistent with the non-regressing provenance. An in-house grid
search remains future work.

Flip soak (M4): 30-turn smoke green on the flipped binary
(`turn_failures=0`, `slow_turns=0`); full 1000-turn soak launched
2026-09-12 ~21:00 UTC (`QXFX0_DIAGNOSTIC_DIR=/tmp/opencode/flip-soak-1000`,
log `/tmp/opencode/flip-soak-1000.log`) — converges ~14:00 UTC next
day; close the gate on its `pilot.status` before declaring M4 done.

Flip soak CLOSED 2026-09-13: 1000/1000, `turn_failures=0`,
`slow_turns=0`, `health_failures=0`, `final_doctor_ok=1`,
`final_metrics_ok=1`. M4 done: the flipped binary holds cadence.

### Migration M3 (ADR-0044, landed 2026-09-11)

The V2 advance is the authority under V2: V1 witness/commit writes
skipped, every reader (strength, style, tags, collapse, bump,
anomaly, reports) routed through `essence_view`, collapse journaled
into the authority-agnostic `reset_events`. B2 stays V1 by design.
Unit-locked (V1 untouched under V2, collapse both paths, bump +
report on the live layer). Next: M4 — default flip with
re-baselining + soak.

### Deferred / operational
- **U1.5**: noun blob → columnar mmap (with the daemon, paid once per
  process; do it when CLI-mode cold start matters again).
- **Soak closure edit** in the ops doc when v35 completes (see §3).
- **Blob storage policy — DECIDED 2026-09-11**: evidence measured
  (173MB of `*.bin` history banked over 10 content waves, ~75MB per
  wave; bins byte-identical on regen, ~45s for both). Derived bins
  untracked (`.gitignore`), sources stay tracked; `build.rs` guard
  fails early with regen commands, CI regens before clippy, `doctor`
  enforces digest freshness. No LFS, no release artifacts — the
  cheapest option that satisfies offline fresh-clone builds.
- **Cross-twin conformance — DONE 2026-09-11**: golden fixtures for
  Conatus, Essence, Salience (bit-faithful) and Deliberation
  (shared-subset + recorded divergences); v2 tuning landed on top.
- `components/` services may re-target the canonical `qxfx0-serve` layer
  (their git-rev pins predate all of U0–U2).

## 6. Where the knowledge lives

- Concept & laws: `docs/adr/0043-subject-runtime-upgrade-u0-u6.md`
- Latency saga & soak ops: `docs/operations/audited-plan-latency-pilot-2026-08.md`
- Census workflow: README «The content census…» + `scripts/generate_census.py`
- Serve protocol: README «Long-lived daemon (ADR-0043 U1)»
- Haskell reference (readiness map): `~/my-haskell-project/QxFx0`
  (`AGENTS.md` fact-checks, `ROADMAP.md`, `src/QxFx0/Self/*`)

### Skeptical audit (landed 2026-09-13, closed)

Three probes (dead code/TODO/panics; test health; law/doc drift).
Code debt: ~zero (no TODOs, no blanket allows, panics fail-closed
only). Fixed in `da8ad12`: flag help default, reflect DB wording,
ADR-0044 accepted, V1-law wording, Law-1 correction, essence_view
exception, two-defaults comments, network scope, stale comments.
Ritual verdict (`1b373df` + this note): help-dedup done; shadow
tables, XorShift ×4, prose pins, trace keys, counts retained
deliberately — determinism pins, not ritual. Audit closed.

### Skeptical audit 2 (landed 2026-09-13, closed)

Probes: perf (clones/allocs/scans), API/types (stringly, errors),
floats/serde/bounds. Fixed in `3e36092`: NaN floor guards (both
twins + tests), journal/contradictions/lineage caps
(10k/10k/256, drain-oldest, validate backstops, gap honesty),
builder side-effect removed (explicit mode at 10 call sites).
Confirmed healthy: BPS conversions, deny_unknown_fields placement,
all recent serde defaults, typed library errors, V2 float hygiene.
Deferred with reasons: V1 NaN guards beyond the floor, path_depth
recalibration, recovery_cause enum, typed revise errors,
response_plan field merge, in-house grid search. Full gate green
(1071 tests). Audit closed.

### Skeptical audit 2 (landed 2026-09-13, closed)

Probes: perf (clones/allocs/scans), API/types (stringly, errors),
floats/serde/bounds. Findings fixed: NaN floor poisoning (both
twins, finite-guard + tests), journal/contradictions/lineage caps
(10k/10k/256, drain-oldest, validate backstops, gap honesty),
builder side-effect removed (explicit mode at call sites).
Confirmed healthy: BPS conversions, deny_unknown_fields placement,
all recent serde defaults, error-type discipline, float hygiene in
V2. Deferred with reasons: V1 NaN guards beyond the floor (V1 is
pinned-comparison surface), path_depth recalibration, recovery_cause
enum, typed revise errors, response_plan field merge, in-house grid.

### Frame tail + operation mode (landed 2026-09-13)Frame tail closed: polarity/agent/target ride the dual journal as
observational evidence (read off stored inputs, defaults for gaps,
old exports deserialize). Memory card deliberately untouched
(topic-centric by design, input-agnostic). Operation loop
rehearsed live (6 mixed turns incl. challenge + guard-block):
report/doctor green, felt-verify clean, facets correct per turn
(negative+agent+target on the challenge, recovery flagged on the
block), verdict honestly not-proven (recovery turn + <10 turns).

### A1 NP chunker (landed 2026-09-14)

`[Adj* Noun (Gen-Noun)*]` over morphology POS with closed-class
surfaces independent of the dictionary, verb-suffix detection for
unknowns (incl. past tense — bare verbs never head), hyphens kept
whole, lemma-repetition and nominative-agreement breaks, genitive
surface fallback for syncretic readings. Integrated as post-pass
over parser subjects/objects (None → byte-identical). 45 golden
rows; 400-row school corpus green after triaging two real defects
(verb-headed spans, hyphenated particles). Documented limits:
unknown adverbs chain, syncretic plurals attach by ending.

### A2 ComposedPair (landed 2026-09-14)

`chunk_all` lists every NP of a span (shared core with the head
chunker); `comparison_pair` composes the distinction pair from the
frame's normalized text (first two phrases — a three-way tail never
glues onto the second). The Distinction branch prefers the composed
pair, legacy `и`-split as fallback. School corpus (400 rows) green
with zero drift; golden pair tests pin the triple case.

### A3 inference deepening (landed 2026-09-14)

Five carried relation types (Enables/Causes/Influences/PartOf/
Opposes + русские verb forms + pathfinder biases + ALL 47→52):
transitive/symmetric sets now match Haskell exactly. V2 energy
scaled ÷10 at the Prepare boundary (monotonic — all downstream
consumers compare against V1-calibrated thresholds; unscaled scalar
still feeds floors/erosion). Inferred edges carry chain-decayed
confidence (0.7/hop, floor 0.25 enforced — bites at 9+ spans,
proven by test). Zero re-baseline drift beyond the intended scale
change.

### C1 provisional lexicon (landed 2026-09-14)

ADR-0029 refined (not lifted): unknown content words accumulate in
a bounded TTL quarantine (`provisional_atoms`: 3 sightings spanning
2 turns, 50-turn TTL, 256 cap) and promote into marked
`CatProvisional` atoms on threshold without canonical collision.
Finalize observes/evicts/promotes; admission universe excludes the
category by construction (tested). Canonical-collision drops
curated duplicates. Unit-locked (turn-counting, thresholds, TTL,
caps) + integration through real turns + admission blindness.

### Phase C verdict (2026-09-14): C1 landed, C2 deferred, C3 specified

C1 provisional lexicon above. C2 (V1-pin removal) APPROVED IN
PRINCIPLE but DEFERRED by engineering judgment: V1 paths are load-
bearing for B2 comparison, the runtime-AB baseline, and a dozen
pinned tests — removing them now burns the evidence base that
validates the M4 flip days after landing. Trigger for removal:
30 days of V2-default operation or 2 content waves without a
V1-caught regression, whichever first; then delete (never flag)
in one commit with B2 retired alongside. C3 (in-house grid) is
blocked on labeled ground truth, not machinery: the grid needs
30–50 prompts with editorially-judged expected hemispheres (human
task, format: prompt<TAB>holistic|formal<TAB>reason); `adapt_`
functions and the ±0.05 doctor bound already constrain the search
space. No theater built.

### Phase C verdict (2026-09-15): C1 landed, C2 deferred, C3 specified

C2 (V1-pin removal) APPROVED IN PRINCIPLE but DEFERRED by
engineering judgment: V1 paths are load-bearing for B2 comparison,
the runtime-AB baseline, and a dozen pinned tests — removing them
now burns the evidence base that validates the M4 flip days after
landing. Trigger for removal: 30 days of V2-default operation or 2
content waves without a V1-caught regression, whichever first; then
delete (never flag) in one commit with B2 retired alongside. C3
(in-house grid) is blocked on labeled ground truth, not machinery:
the grid needs 30–50 prompts with editorially-judged expected
hemispheres (human task, format: prompt<TAB>holistic|formal<TAB>
reason); `adapt_` functions and the ±0.05 doctor bound already
constrain the search space. No theater built.

### C3 validator (landed 2026-09-14)

`scripts/validate_label_corpus.py` encodes the mechanical contract
(format/LF/BOM, 3 columns, closed labels, no dups, no A∩B overlap)
and reports stats (balance, borderline, length strata as
informational). v1 passes; re-label/kappa and the grid itself wait
on the human labeling pass.

### Memory M1 recall scoring (landed 2026-09-14)

`qxfx0-codex::recall`: pure ranking over persisted commitments +
journal — `status × recency + contradiction bonus`, same-topic
only, total deterministic order, caller takes top N. Unit-locked
(hand-computed scores, scoping, limits, determinism). No consumers
yet by design (M2 next).

### Memory M2 recall surface (landed 2026-09-14)

`ReflectionReport.recalled` (top-3 per last topic) rendered in
console (`вспомнилось:`) and markdown (`Вспомнилось:`) with
standing (held/stale) and contradiction marks. Read-only: no turn
rendering changes. Next: M3 recall events in FELT, M4 forgetting.

### Memory M3 recall evidence in FELT (landed 2026-09-14)

`FeltManifest.recalls`: per discussed topic, shown (top-5) vs
suppressed with scores; new `Воспоминания` markdown section;
verify checks disjoint ids + shown score-desc. Next: M4 forgetting.

### Memory M4 governed forgetting (landed 2026-09-14)

`CommitmentOps::forget_stale` in Finalize, every turn: retires
positions untouched 50+ turns, confidence < 0.5, uncontested, with
no live dependents (max 8/turn, id order). Lineage keeps
`Retracted(Forgotten)` — visible, never the silent eviction the
capacity path refuses. Recall/FELT read `active`, so forgotten
positions simply stop surfacing. Memory program M1–M4 complete.

### Practice-2 + contested-expiry fix (landed 2026-09-14)

Практика-2 (`docs/operations/second-practice-2026-09-14/`, 66 ходов)
поймала баг дизайна M4: вечное освобождение оспоренных делало
забвение недостижимым вживую (все sub-0.5 позиции — из revision).
Фикс: спор держит живым, пока жив сам (внутри TTL); древний спор —
история. Подтверждено вживую: Forgotten на ходах 55 и 65 ровно по
touched+50, verify чист. Урок: практики только на свежем release.

### Engagement-fix: «нет» как сигнал (landed 2026-09-14)

Разбор практики-2: `contains("не ")` не видел ведущее «нет» (ход 8
engaged, но не contradicted). Фикс — токенные сигналы + «нет» только
в начале предложения + слитные формы («неверно», «несогласен»…).
Чистая `has_contradiction_signal`, матрица 17 входов, регрессия хода 8.
Живая проверка (8 ходов): противоречий 1 → 2 (7 vs 6 на ходу 8).

### Practice-3: contradiction-rich session (landed 2026-09-14)

`docs/operations/third-practice-2026-09-14/` (60 ходов): 15
противоречий новыми сигналами, забывания ид 3/7 на ходах 54/58,
пара 11/12 разрешилась карантином проигравшего (победитель стоит).
Все гейты пройдены. Константы памяти 50/0.5/8 — оставляем.

### Phase D1: recovery_cause в enum (landed 2026-09-14)

8 сайтов recovery в `process_turn` типизированы: `RecoveryCause`
(Prepare/Route/PlanShadow/Render/Finalize/Guard, 1:1 с именами
`execute_stage`); guard-сообщение — `stage error: {stage}` вместо
немого «stage error». Тест покрывает все 6 вариантов и блокировку.

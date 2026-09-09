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
doctor OK, census `--check` green. Workspace is now 17 crates
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

### U4 — «Мост обучения»
`qxfx0-bridge` behind a feature flag (no network in default builds),
runtime edge store with reinforce/decay/retire (Haskell
`RuntimeLLMFeedback` is the spec), corroboration queue, quarantine tables
in SQLite. Gate: zero visible behavior change.

### U5 — «Промоушен»
Gates (informativeness threshold, versioned policy, draft overlay →
activate → human release → rollback), CLI `promotion …`, fingerprint in
the replay envelope, and the corpus bridge: promoted candidates must pass
the same admission bar as editors; `import_haskell_corpus.py` quarantine
becomes an input.

### U6 — «Свидетельства»
Dual journal (subject positions symmetric with practitioner positions),
verifiable export as the FELT measurement instrument (B2 rubrics get
reproducible artifacts).

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

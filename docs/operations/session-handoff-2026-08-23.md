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

- **Soak v35** (cadence-gate confirmation): `/tmp/opencode/qxfx0-soak-v35-1000/`
  (`pilot.status`, `pilot.log`, `turns.jsonl`). Started 2026-08-23 19:58Z
  on the release binary built at the U0 tree (behavior-identical to HEAD
  for the soak's purposes; the formal gate should close on the binary that
  ships — if HEAD is released first, rerun the soak once against it).
  Driver script: `scripts/diagnostic-soak-1000.sh`; quiet-gate launcher:
  `/tmp/opencode/qxfx0-soak-v35-launcher.sh` (waits for no
  cargo/cabal/test processes + ≥2 GiB `MemAvailable`).
  **Watchdog**: cron automation «soak-конвейер: статус каждые 30 минут»
  (id in `CronList`) — reports status, restarts a dead driver as v36+ on a
  quiet host, verifies the verdict and self-deletes on completion.
  Closure criterion: `slow_turns=0`, `final_doctor_ok=1`,
  `final_metrics_ok=1` in `pilot.report`; then update
  `docs/operations/audited-plan-latency-pilot-2026-08.md` (close the
  cadence gate) — that edit is still owed.
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

### U3 — «Делиберация»
Port Field (five components) + Salience contributions + `reconcile`
replacing priority switching + doubt loop + episodic recall, each landing
in Shadow; calibration of constants stays deferred until a trace corpus
exists (discipline, not debt).

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

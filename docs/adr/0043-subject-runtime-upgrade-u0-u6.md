# ADR 0043: Subject-runtime upgrade U0–U6 — from deterministic dialogue runtime to verifiable subject

Status: accepted — U0 complete, U1 complete (serve daemon), U2 complete
(crate + pipeline wiring + hysteresis landed; ADR-0044 verdict recorded) —
U3 complete in shadow (salience + blanket + canonical reconcile/doubt-loop
landed 2026-09-09, all replay-visible; the dispatch flip landed as
ADR-0044 Migration M1–M4 2026-09-11/13 with re-baselining and a closed
1000-turn soak) — U4 complete
(U4.1 + U4.2 landed 2026-09-10: `qxfx0-bridge` algebra + worker +
quarantine + schema v13 + `bridge-maintain`, zero visible behavior change
gated by corpus equality) — U5 in flight (U5.1 + U5.2 landed 2026-09-10/11: pure
promotion boundary + v14/v15 store + CLI `promotion` with import source,
admission bar, revalidation, structural corpus precheck and an
evaluation-bound `approve`; the Haskell-corpus
promotion-output feed remains) — see
`docs/operations/session-handoff-2026-08-23.md` for the running state

## Frame

The Haskell twin (per its ROADMAP North Star) is building a *subject*, not a
tool; the Rust twin is the deterministic, verifiable kernel with the shipped
product surface («Кодекс»). This upgrade is **not a port** of the Haskell
system. It transfers only landed, fact-checked phases of Haskell's
architecture into Rust while preserving Rust's own law — everything that
renders and persists is a pure function of `(input, state, active knowledge
set digest)`. The Haskell side builds the subject; the Rust side makes it
**provable**: replayable, byte-exact, doctor-gated.

## The three laws of the upgrade

1. **Determinism.** No LLM call ever happens on the turn path. Everything
   nondeterministic lives *between* turns and enters a turn only as data —
   via a versioned, digest-fingerprinted promotion overlay. (Recorded
   correction 2026-09-13: the turn path never reads promotion tables —
   overlays act only through editorial admission into the embedded pack
   and offline measurement harnesses. The law's overlay clause describes
   the admission/review boundary, not a turn-time read.) The existing
   pack-set fingerprint mechanism (a session cannot silently cross a
   semantic-authority change) extends to overlays.
2. **Fail-closed.** No hypothesis reaches the user without gates and an
   explicit human release. A broken blob or overlay is a hard turn error,
   never a silent JSON-parse degradation.
3. **Incrementality.** Every phase lands with a doctor check, a contract
   gate and replay-visible trace fields — the way thesis projection landed
   in Shadow mode without mutation.

## Phases

- **U0 «Гигиена»** — behavior-neutral engineering from the 2026-08 audit:
  fail-closed blob paths; builder over `TurnOptions` deprecating the 21
  `process_turn*` wrappers; extraction of `codex`/`gates` out of the CLI
  crate; removal of the dead `qxfx0-governance` crate; generated census
  manifest as the single source of truth (CI: regeneration = no diff);
  README/doctor number sync; blob storage policy (LFS or artifacts) before
  the corpus starts growing again.
- **U1 «Субстрат»** — `qxfx0 serve`: a canonical long-lived process in the
  main workspace — same turn code over the journal runtime, a unix socket
  with a one-JSON-per-line protocol, single-threaded accept loop so
  same-session turns stay serialized. Determinism needs state isolation,
  not process death: the daemon pays the blob/seed-graph init once instead
  of per turn. The `components/` services (QxFx0TurnService et al.) are
  independently versioned nested repositories, not unversioned code as the
  audit first read — they keep their HTTP/TLS/Postgres authority-
  infrastructure shape and may later re-target the canonical serve layer.
  The noun blob then moves to a columnar mmap format mirroring the
  adjective blob (~200 ms deserialize → ~0).
  Gates: CLI cadence no worse than today; daemon warm-turn gate p99 < 50 ms
  with byte-identical responses vs the CLI path; soak extended with a serve
  mode.
- **U2 «Ядро субъекта»** — port `Conatus` (drive/energy functional) and
  `Essence` (witness / shouldCommit / commit; unconditional law, not a
  runtime flag; `EssenceRupture`; ablation hook only for the B2 control
  arm, present from day one) into a new `qxfx0-self-v2` crate (V1
  `qxfx0-self` untouched). Prepare/Finalize integration, nullable trace
  fields, doctor invariants, replay contract gate.
- **U3 «Делиберация»** — Field (five components), Salience (contributions;
  spectral clustering deferred), `reconcile` replacing priority switching
  in route, doubt loop (threshold → CMClarify), episodic recall. Shadow
  mode first — new routing computed and logged, not applied; flip in a
  separate release after shadow comparison on the soak corpus.
  Landed so far: the canonical Salience controller (`qxfx0-self-v2::
  salience`, Haskell Phase-5 port — contributions, uncontested Conatus
  gate, dead-band hemisphere dispatch, bounded weight adaptation) rides
  the U2 shadow advance as `EssenceAdvanceTrace.self_verdict`: replay-
  visible, outside the witness hash and every persisted field, locked by
  the structural-corpus shadow gate and the doctor self-layer check.
  Field already exists in `qxfx0-types`; the structural self-blanket
  (`QxFx0.Self.Invariants` port — session stability, morphology presence,
  turn / identity-claim monotonicity) landed as U3.2: it is checked every
  turn in finalize, its violations feed the conatus penalty and ride the
  shadow trace (fail-closed data, never a turn-path panic), and the previous
  blanket persists through the new additive schema-v12 `blanket_v2_json`
  column so a fresh per-turn process can run the transition check.
  Reconcile / doubt loop / episodic recall landed as U3.3:
  `qxfx0-self-v2::deliberation` ports the Haskell Phase-8 six-rule ladder
  keyed on the canonical salience verdict (Conatus override → agreement →
  salience lead → single-axis advantage → tied-fallback-formal, recovery
  merged by severity and never silenced), and `deliberate_shadow` attaches
  the reconciled-vs-applied comparison plus the doubt-loop escalation
  (V2 Conatus-gate floor 0.9, counterfactual ambiguity +0.2,
  same-topic-confirmed suppression) to the same replay-visible
  `EssenceAdvanceTrace` — V1 routed until the ADR-0044 M4 flip
  (2026-09-11); the flip reads the agreement statistic off the trace
  corpus.
- **U4 «Мост обучения»** — `qxfx0-bridge` behind a feature flag (default
  build has no network — privacy stays an architectural fact). Runtime
  edge store with reinforce/decay/retire (Haskell `RuntimeLLMFeedback` is
  the spec): associative evidence only, never rendered. Bounded queue,
  between-turn worker, quarantine tables in SQLite. Gate: zero visible
  behavior change (corpus-equality asserts in tests).
  Landed so far (U4.1, 2026-09-10): the crate exists as the trust
  boundary's pure core — `runtime_edges` is the exact `RuntimeLLMFeedback`
  port (reinforce +0.05/−0.10, conflict retires, promote at
  confidence ≥ 0.75 ∧ co-occurrence ≥ 3, multiplicative decay 0.95 with a
  0.3 retire floor and a 500-edge cap, tie-broken by key), `corroboration`
  the bounded FIFO that a between-turn worker drains (back-pressure
  reported, negative/conflict evidence never fabricates an edge), and
  `candidates` the source seam — a total offline `Noop`/`Scripted` source
  with `HttpCandidateClient` present only under the `llm-candidates`
  feature, which still fails closed (no transport is linked; wiring a
  review-gated HTTP dependency is its own supply-chain decision). Nothing
  is wired into the pipeline yet; `validate_bridge_invariants` rides the
  doctor `Learning bridge` check, which also asserts the default build
  carries no network surface. U4.2 landed the rest: `worker`
  (`process_turn_boundary`) is the pure between-turn cycle — admit
  (quarantining refusals with named reasons, never silently), fold the
  ladder, decay/prune; `quarantine` is the bounded, insertion-ordered
  review ledger the U5 queue reads; schema **v13** adds the
  `session_bridge_edges` / `session_bridge_quarantine` tables beside the
  session state, and the CLI `bridge-maintain` command runs the decay
  half of the cycle per session (the queue is daemon-side, candidate
  fetching is U5). The U4 gate is executable, not intended:
  `bridge_is_not_linked_into_the_turn_path` scans every workspace manifest
  (only `qxfx0-cli`, and dev/test edges, may name the crate) and
  `a_live_untouched_bridge_store_leaves_the_corpus_byte_identical` runs the
  twelve soak prompts twice with a live, populated bridge store open —
  responses, routing, guard verdicts and parity state are byte-equal and
  `SystemState` still has no bridge fields.
- **U5 «Промоушен»** — gates per the Haskell design: informativeness with
  a semantic-gain threshold, versioned gate policy, draft overlay →
  activate → human release → rollback. CLI: `promotion list/approve/
  release/rollback`. The bridge to the corpus machine: a promoted
  candidate must pass the same admission bar as an editor (seed atom,
  counterpoint, clause grammar, bijective morphology). `import_haskell_
  corpus.py` quarantine becomes an input; Haskell promotion output becomes
  admission input.
  Landed so far (U5.1, 2026-09-10): the pure boundary in
  `qxfx0-bridge::promotion` ports the Haskell `QxFx0.Learning.Promotion`
  core — a versioned + SHA-checksummed `GatePolicy`, the exact
  `evaluateCandidateInformativeness` decision (not-tautological,
  not-topic-paraphrase, novel-against-the-curated-baseline,
  semantic-gain ≥ 0.75 via Jaccard of stop-word-filtered >3-char atoms,
  constraint-relation novelty), and the immutable lifecycle machine
  (`create_draft` → `activate` → `release` → `rollback`), with a
  content-addressed `overlay-<checksum>` version that pins its parent. The
  CLI `promotion list/draft/approve/release/rollback` drives it: `draft`
  enumerates candidates from the bridge's Promoted tier (deduplicated by
  canonical triple, so a retry over unchanged evidence is the same
  candidate set and the same overlay version), against the argued-corpus
  baseline (the same surfaces an editor passes). Schema **v14** stores the
  overlay journal (`promotion_overlays`, status CHECK-constrained, CAS
  transitions that refuse a lost update and reject a mid-lifecycle
  checksum move) and a one-row `promotion_active` singleton that may only
  point at a Released version. A released overlay is a reviewable artifact
  for editorial admission into the embedded pack — the fingerprint
  mechanism of law 1 — *not* a live graph edit: the turn path never reads
  either v14 table, and the U5 corpus-equality gate
  (`a_released_active_overlay_leaves_the_corpus_byte_identical`) runs the
  soak prompts against a live Released+active overlay and demands
  byte-identical responses/routing/guard/state, plus `SystemState` JSON
  free of promotion fields. `validate_promotion_invariants` rides the
  doctor `Promotion boundary` check (checks 16 → 17). Landed 2026-09-11
  as U5.2 on top: (1) the import-quarantine candidate source (offline
  quarantine JSONL parses into canonical triples resolved against the
  installation's session-graph atoms, mixed into the same snapshot
  digest; unresolvable rows are per-line refusals, never silent);
  (2) admission-bar enforcement at draft (seed atoms from the union of
  session graphs, counterpoint presence from the argued registry) via
  `create_draft_with_admission`; (3) `revalidate` as report-only
  policy/baseline drift detection; (4) the structural corpus precheck
  (content/coverage no-regression over the fixed 12-topic set plus the
  overlay's own topics, persisted as content-addressed v15
  `promotion_evaluations` rows), and `approve` binding to the latest
  passing evaluation pinned to the overlay's exact checksum — the Haskell
  boundary's `prior_runtime_evaluation_id` precondition, operationalized.
  The loop closes back toward the Haskell corpus as U5.4: a Released
  overlay exports its predicates as versioned machine JSON
  (`promotion-export-pack-01`: topic/subject/relation/object surfaces,
  confidence/support/gain, both bound evaluation ids, policy pin,
  overlay checksum) for the human editor to merge into the pack sources
  — the export refuses anything but Released, and automation stops
  there: graph effect still comes only from editorial admission, pack
  gates validate the merged result.
  Remaining U5: the runtime-AB leg of the corpus evaluation (a separate
  method row for when an overlay can actually render) and the
  Haskell-corpus promotion output feeding the editorial bar. Landed
  2026-09-11 as U5.3: `promotion-runtime-ab-01` renders every case topic
  once per arm — pristine snapshot vs snapshot carrying the overlay as
  held positions (the as-if-admitted operationalization; the operator DB
  is never mutated) — over the overlay's topics plus the fixed 12-topic
  set minus the overlay's as the regression baseline. Pass = regression
  byte-identical AND guard-stable; overlay-topic divergence is recorded
  signal, never gating. `approve` binds to the latest passing row of
  EACH method for the exact checksum.
- **U6 «Свидетельства»** — «Кодекс» grows from the practitioner's diary
  into a dual journal: the subject's positions tracked symmetrically with
  the practitioner's (contradictions, stability, refusals, memory shaping
  positions). The verifiable diary export becomes the measurement
  instrument for the FELT evidence line (B2 rubrics get reproducible
  artifacts). Landed 2026-09-11 as U6.1 inside `qxfx0-codex` (no new
  crate, no schema change, no turn-path touch): `dual_journal` zips each
  turn 1..=turn_count with the subject's persisted position (V1 witness
  attributes, commitment coverage, reset flags; V2-shadow presence as a
  session flag; pre-journal gaps flagged, never filled; per-turn family
  honestly absent — a routing-time projection, not a persisted position),
  and `felt` evaluates the six mechanical M6 gates ported from
  `QxFx0.Core.M6FeltGate` (governed-evidence, non-fallback-dialogue,
  definition-of-subject ≥2 contentful turns, distinction-of-positions ≥2
  topics, repair-of-contradiction with live retention, commitment-of-memory
  ≥10 turns with full id accounting; empty session fails all six).
  `felt-export`/`felt-verify` are the artifact pair: the manifest embeds
  the dual journal plus exactly the facts the gates read, so verification
  re-evaluates the same pure core with no database (diary stays the replay
  truth, FELT the gate truth, `session_digest` cross-checks the two);
  `felt-export` never refuses a thin session — testifying «not proven» is
  the tool's job.   `validate_felt_invariants` rides the doctor `Felt evidence` check
  (checks 17 → 18). Landed 2026-09-11 as U6.2 on top: the flip criteria
  and the B2 rubric bindings. The B2 verdict procedure
  (`b2_ablation_probe`: corpus legs in fresh sessions plus the 8×8
  challenged свобода long leg, both arms) is now a deterministic library
  function (`qxfx0-pipeline::b2_report::run_b2_report`, corpus pinned by
  SHA-256, means rounded to 4dp) with the probe example as a thin
  printer — the human re-run and the machine proposal can never diverge.
  `qxfx0-codex::flip` drafts the machine-checkable flip proposal: five
  rubrics over verified FELT exports (≥5 distinct sustained sessions
  with ≥10 turns each; pass counts recorded, not gating) plus a live B2
  re-run (enabled arm commits ≥1, violation max-run below the hysteresis
  window — derived as one below the v2 default, coupling locked by a
  pipeline test — ablated control suppresses without violating, guard
  blocks match across arms). `flip-draft`/`flip-verify` are the artifact
  pair (verify recomputes with no files, no DB, no re-run);
  `validate_flip_invariants` rides the doctor `Flip readiness` check
  (checks 18 → 19). The draft never flips anything: the migration itself
  stays a human-reviewed code change, and human release is permanent.
  U6 is now complete; the flip landed as ADR-0044 Migration M1–M4
  (practice archived, probe re-run, default flipped with re-baselining,
  soak closed 2026-09-13).

## Anti-goals

- No wholesale port; no commit-level sync with Haskell. Rust takes only
  landed, fact-checked phases (Haskell AGENTS.md fact-checks are the
  readiness map).
- No network in the default build; no LLM on the turn path; no
  auto-promotion — human release is permanent.
- The «Кодекс» surface and all six V2 gates stay green through every U
  phase.
- Constants (Salience/doubt) stay default until a trace corpus exists —
  deferred calibration is discipline, not debt.

## Risks and answers

| Risk | Answer |
|---|---|
| Replay breaks on nondeterminism | Nondeterminism confined behind the promotion boundary; overlay digest in the replay envelope; per-phase gate: replay before/after identical on the 71-topic corpus |
| Daemon is a new drift surface | Same turn code; per-session state isolation; soak in both modes; CLI remains the reference |
| Blob history bloat as the corpus grows | U0 settles the LFS/artifact policy before promotion starts multiplying content |
| Self-logic duplication across languages | Haskell is the reference; each Rust port is fixed by an ADR linking the Haskell ADR; fixture parity tests now, shared golden corpus later |
| B2 evidence without a control arm | The Essence ablation hook lands in U2, not retroactively |

## Sequencing note

The v35 cadence-confirmation soak (see
`docs/operations/audited-plan-latency-pilot-2026-08.md`) was paused while U0
landed, then relaunched on the shipping tree — the formal gate should close on
the binary that actually ships. **CLOSED by v36** (2026-08-26, 1000/1000
turns @60 s idle, `slow_turns=0`, p99 697 ms) against the post-U2 release
binary. Future U-phase landings that touch the turn path re-run the cadence
soak as before (manual/nightly; CI keeps the 30-turn smoke).

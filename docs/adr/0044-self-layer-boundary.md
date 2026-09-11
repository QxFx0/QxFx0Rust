# ADR 0044: Self-layer boundary — `qxfx0-self` vs `qxfx0-self-v2`

Status: proposed

## Context

The pipeline carries two subject crates with overlapping vocabulary:

- `qxfx0-self` (v1, ~3.4k lines): the working layer. Prepare/Finalize call
  `Conatus`/`Salience`/`Deliberation`/`witness_essence`/`commit_essence`
  directly; doubt/anomaly episodic stores and `fact_perspective` serve the
  shadow recorders. Everything user-visible flows through it.
- `qxfx0-self-v2` (~1.5k lines): the canonical core from ADR-0043 U2 —
  `Conatus` functional plus `Essence`, advanced in shadow with a B2
  ablation arm (`EssenceAblation`). It never renders and never persists;
  its advance is trace evidence only.

They are not accidental duplicates: v1 is authority, v2 is the experiment
that may one day replace it. Merging them now would either bless unproven
semantics (v2) or churn the hot path for aesthetics.

## Verdict experiment (run 2026-09-08, probe
`qxfx0-pipeline/examples/b2_ablation_probe.rs`)

Both arms over the full admitted corpus (fresh sessions: angst 0.05,
zero commitments/violations/suppressions in either arm — v2 sleeps on
single turns) and over a 64-turn challenged свобода session:

- long-enabled: v1 commits, v2 commits once, then 25 violations;
- long-ablated: v1 commits, 43 suppressions, 0 violations;
- visible behaviour identical in both arms (already gated).
Reading (corrected after measuring runs, not just totals): the 25
violations are intermittent (max run 4/8) — disagreement inside a held
position, which a commitment should survive. The hysteresis backstop
(release after 8 *consecutive* violations, angst halved, witnesses
kept) covers the sustained case the probe never reaches; unit tests
lock all three behaviors (release, counter reset, no immediate
recommit on the angst path) plus the documented erosion re-fire.
The ablation arm suppressing 43/57 turns is informative, not broken:
angst pins high in challenged sessions, so nearly every turn would
commit — the control records exactly that.
Marginal value of v2 today = violation-sensitivity + replay-visible
trajectory, not better commitment. No merge until a product decision
needs v2 as turn authority.

## Tuning (landed 2026-09-11, `qxfx0-self-v2` only + topic threading)

The hysteresis item of the Decision is implemented, all three legs:

- **violation decay**: an admissible turn on the commitment's topic
  decays the counter by `violation_decay_step` (default 1) instead of
  zeroing it — a mostly-violating trajectory drifts toward release
  instead of starting over;
- **per-topic commitment**: `EssenceCommitment.topic` (threaded from the
  turn subject by the pipeline); the counter moves only on turns scoped
  to the commitment's topic. Cross-topic violations are recorded, never
  counted. Pre-tuning commitments (`topic: None`) stay universal, so old
  snapshots behave as before;
- **commitment budget**: `max_lifetime_commits` (default 3) caps commits
  per trajectory; further crossings are recorded (`trigger`) but
  suppressed (`budget_suppressed` in the trace).

All three are unit-locked (decay cadence, cross-topic neutrality,
unscoped compatibility, budget suppression testimony). Parity fixtures
(Conatus/Essence/Salience/Deliberation) stay green: the tuning lives in
`advance_essence` policy, the pinned laws (`witness`, `should_commit`,
`extract_mode`, `field_signature`, controller, reconcile skeleton) are
untouched. New modulation fields ride `doctor` via
`validate_invariants`.

## Verdict re-run (2026-09-11, release probe, post-tuning)

- corpus, both arms: 141 turns, angst 0.05, zero
  commitments/violations/suppressions — identical to 09-08. Single-turn
  dynamics untouched, as designed.
- long-enabled: v1 committed, v2 committed once, 25 violations,
  max_run 4, releases 0 — identical to 09-08.
- long-ablated: 43 suppressions, 0 violations — identical to 09-08.

Reading: the tuning is invisible at trace level on this script (the
challenged prompts vary in subject, so per-topic scoping keeps the
counter low; the release threshold is reached in neither version).
That is preservation, not absence of effect — the new mechanics fire
on sustained same-topic counter-evidence and lifetime churn, regimes
the script never enters and the units cover directly. Non-degenerate
dynamics confirmed: commits once, disagrees intermittently inside the
held position, control suppresses, guard identical across arms.
Unification still not decided: no product need for v2 as turn
authority yet.

## Decision (proposed)

1. Keep both crates (v1 authority, v2 shadow). No unification until v2
   gains hysteresis (commitment budget / per-topic commitment /
   violation decay) and this probe shows non-degenerate dynamics.
2. Re-run the probe as the acceptance gate for that tuning.
3. When that verdict lands, migrate Prepare/Finalize call sites behind
   the existing `TurnOptions.essence_v2_ablation` switch, retire v1
   modules one by one — never a flag-day rewrite.

## Consequences

- `essence_v2: Value` in persisted state stays opaque until the verdict;
  typing it now would freeze an experimental schema.
- Hygiene PRs must not "simplify" the self layer by merging crates.

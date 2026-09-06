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

## Decision (proposed)

1. Keep both crates. Document the boundary instead of merging:
   v1 = turn authority, v2 = shadow canonical core under ablation.
2. No unification work until the B2 ablation experiment yields a verdict
   (visible-behaviour parity + a product decision on which semantics win).
3. When that verdict lands, the migration is: move Prepare/Finalize call
   sites to v2 behind the existing `TurnOptions.essence_v2_ablation`
   switch, retire v1 modules one by one, then delete the crate — never a
   flag-day rewrite.

## Consequences

- `essence_v2: Value` in persisted state stays opaque until the verdict;
  typing it now would freeze an experimental schema.
- Hygiene PRs must not "simplify" the self layer by merging crates.

# ADR 0042: Plan type unification — shared law descends to qxfx0-types

Status: accepted — step 1 of the ADR-0041 roadmap

ADR-0041 split the V2 certificate chain into `qxfx0-plan-v2` but left the
plan vocabulary in place: V2 imported `SemanticId`, `NonEmptyVec`,
`Confidence` and `ClaimRole` from `qxfx0-semantic`'s V1 `response_plan`
module. That made the V2→V1 dependency heavier than it needs to be — those
four types are not renderer-era V1 detail, they are the shared law of both
strata: every plan addresses semantic content through `SemanticId`, carries
at least one claim through `NonEmptyVec`, ranks evidence through
basis-point `Confidence`, and labels discourse function through
`ClaimRole`.

Step 1 moves them to `qxfx0-types/src/plan.rs` verbatim (no serialized-form
change; the wire names and validation rules are byte-identical).
`qxfx0-semantic` re-exports them from `response_plan` so downstream paths
are unchanged, and `qxfx0-plan-v2` now imports them from `qxfx0-types`
directly. The plan-v2 → semantic dependency is thereby reduced to what it
always claimed to be: the authority registries (`argued_topics`,
`fact_model`, `knowledge_pack`) and, in tests only, the V1 proposition
algebra.

Deliberately NOT unified, with reasons:

* Same-named identity types. V1's `ClaimId` is a plan-local index; V2's
  `ClaimId` is a content-addressed certificate digest. Merging them would
  let a plan-local index masquerade as a content address — identity types
  unify only when their construction semantics unify.
* `SemanticProposition` (V1) vs the V2 proposition DAG. The V1 enum mixes
  canonical predicates with renderer-facing counterpoint/consequence leaves
  (`PredicateRef`-carrying); the V2 DAG is a certificate stratum with its
  own invariants. Unifying them requires deciding which invariants are
  shared law, and that decision is not smuggled through a type move.

Next steps, each separately reviewed: (2) split the registry surface —
certificate-facing APIs of `fact_model`/`argued_topics` move behind
`qxfx0-plan-v2` once V1's renderer-facing half is disentangled; (3) retire
`qxfx0-pipeline`'s public re-exports of V2 types so downstream consumers
depend on `qxfx0-plan-v2` directly.

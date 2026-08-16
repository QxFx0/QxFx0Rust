# ADR 0041: Split response_plan_v2 out of qxfx0-semantic

Status: accepted

`qxfx0-semantic` held both the V1 core (parser, seed graph, registries, V1
plans, surface generation) and the ResponsePlan V2 certificate chain, ~17.5k
lines in 21+14 modules. The V2 chain imports V1 types (`response_plan::{
SemanticId, NonEmptyVec, Confidence, ClaimRole }`), the audited registries
(`argued_topics`, `fact_model`) and `knowledge_pack`. Inside one crate this
coupling is invisible: nothing prevents a V1 module from reaching into V2, and
any future "unify V1/V2 types" migration would have to happen inside a crate
that is already the largest in the workspace.

The split moves `src/response_plan_v2/` (15 modules, inline tests included)
into a new crate `qxfx0-plan-v2`, together with its two embedded assets
(`valency_frames.tsv`, `preposition_allomorphs.tsv`). The dependency is now
explicit and one-directional: `qxfx0-plan-v2 → qxfx0-semantic → morphology`.
There were no reverse dependencies (V1 never imported V2), so the move is
mechanical: `crate::response_plan_v2::` became `crate::`, V1 imports became
`qxfx0_semantic::` root re-exports, and only `qxfx0-pipeline` (public
re-exports of `ResponsePlanV2Mode` and the replay snapshot types) and
`qxfx0-cli` (the contract gates and the fixture generator) referenced V2 from
outside. No behavior, digest domain, or public item name changed; the full
test suite passes unchanged after the move.

The extraction is the enabling step for the remaining unification work, which
stays a separately reviewed migration:

1. V2 keeps its own proposition/derivation/discourse types next to V1's
   `response_plan` types. Unifying them means deciding which invariants are
   shared law and which are stratum-specific certificates; that decision must
   not be smuggled in through a refactor.
2. `argued_topics`, `fact_model` and `knowledge_pack` are the shared authority
   surface. Once V2 is the only consumer of their certificate-facing APIs,
   those APIs can move to `qxfx0-plan-v2` and leave V1 with the renderer-facing
   half, shrinking the dependency to morphology-level primitives.
3. `qxfx0-pipeline` re-exports of V2 types should become direct dependencies of
   downstream consumers, so the pipeline stops being a type bus.

Rule going forward: V1 modules must not depend on `qxfx0-plan-v2`, and
`qxfx0-plan-v2` must not grow renderer-side surface generation — that belongs
to V1 until a certificate chain replaces it.

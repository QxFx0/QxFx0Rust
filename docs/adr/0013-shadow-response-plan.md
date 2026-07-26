# ADR-0013: Shadow response outcome and typed recovery

- Status: Accepted
- Date: 2026-07-27

## Context

ADR-0012 removed string routing, but the renderer still follows a route without
an observable semantic planning boundary. Introducing the final claim model in
one cutover would mix contract design, content migration, and surface changes.

## Decision

Insert `PlannedTurnContext` between route and render. It carries
`PlanOutcome<ShadowResponsePlan>`:

- `Ready` records the typed goal, subject, source mode, and routed family;
- `Fallback` records a typed recovery trace;
- the two outcomes cannot coexist;
- fallback reason is derived from recovery cause, so they cannot diverge;
- recovery evidence is structurally non-empty.

The shadow plan is observational. Renderer continues to use the routed context
embedded in `PlannedTurnContext`; responses and persistent state must remain
identical. `plan_shadow` is a replay-visible pipeline step with stable metadata.
Guard rejection also emits the same recovery taxonomy in trace metadata.

## Deferred

`ShadowResponsePlan` deliberately has no claims. The next content PR replaces
it with `ReadyResponsePlan`, non-empty claims, proposition IDs, predicate refs,
and provenance after the audited-topic admission registry exists.

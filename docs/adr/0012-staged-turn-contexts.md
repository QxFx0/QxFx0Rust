# ADR-0012: Staged turn contexts and typed routing

- Status: Accepted
- Date: 2026-07-27

## Context

The turn pipeline passes semantic decisions through `BTreeMap<String, String>`.
Proposition modes and move families are formatted with `Debug`, then recovered
with substring matching or string decoders. This admits incomplete contexts,
silently maps unknown values to defaults, and will become unsafe as routing
modes grow.

## Decision

Turn processing uses owned, stage-specific contexts. Each successful stage
consumes its predecessor and produces the only context accepted by the next
stage:

```text
TurnInputContext -> PreparedTurnContext -> RoutedTurnContext
  -> RenderedTurnContext -> FinalizedTurnContext
  -> GuardedTurnContext -> PersistedTurnContext
```

Routing and Essence mode selection match directly on `PropositionMode`.
`CanonicalMoveFamily` remains an enum throughout the pipeline. Expected guard
rejection is represented in `GuardedTurnContext`; `Err` is reserved for stage
faults.

PR1 does not introduce a semantic response plan and does not change rendering.
Its compatibility gate requires byte-for-byte response parity for all current
proposition modes, unchanged `TurnOutput` routing fields, and unchanged
persistent state behavior. Internal trace digests may change because their
serialized inputs become typed contexts; stage order and replay determinism
remain stable.

## Follow-up

The next change inserts `PlannedTurnContext` and a shadow `PlanOutcome`
between routing and rendering. `PlanOutcome::Ready` and
`PlanOutcome::Fallback` must be disjoint, ready claims must be non-empty, and
propositions must have one canonical storage location.

## Consequences

- Adding a proposition mode now requires exhaustive compiler-checked routing.
- Renderer and later stages cannot receive an unprepared or unrouted turn.
- String hints and fallback decoding are removed from the production pipeline.
- Context fields are duplicated only through explicit owned stage nesting;
  no mutable cross-stage string registry remains.

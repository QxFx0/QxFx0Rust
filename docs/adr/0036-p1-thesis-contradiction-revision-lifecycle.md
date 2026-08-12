# ADR-0036: P1 explicit contradiction and append-only thesis lifecycle (draft)

- Status: Draft
- Date: 2026-08-04

## Context

P0 introduced stable typed thesis identity and a bounded relation graph without changing
persistence or pipeline behavior. P1 needs replay-safe position changes while preserving the
legacy commitment APIs and preventing text similarity from becoming authority.

## Decision

Add a data-only `ThesisLifecycle` contract to `qxfx0-types`. A stable `ThesisId` owns immutable
canonical-digest revisions, one current head, a `BTreeMap` event history, and a `BTreeSet` of
typed relation triggers. Status is `Draft`, `Active`, `Contested`, `Superseded`, or `Retracted`.
Every appended event has a strictly increasing contiguous sequence and strictly increasing
logical turn. All collections are explicitly bounded; there is no wall-clock or float field.
New compatibility fields use serde defaults.

Add pure `ThesisLifecycleOps` beside, not inside, legacy APIs in `qxfx0-commitment`. Operations
clone-and-return state. Exact event replay returns unchanged state; conflicting replay, stale
head, unknown digest, stale turn/sequence, illegal transition, capacity overflow, unstable id,
and rollback to any historical digest fail closed. Revision preserves the old digest and marks
its revision superseded before installing a new active digest. Supersession and retraction are
terminal.

`Counters` and `Contradicts` are distinct relation kinds. Counterarguments can contest a thesis
but never prove contradiction. Contradiction requires an explicit `Contradicts` trigger or one
of the closed typed contradiction-rule variants. No keyword, surface-text, token, or word-overlap
logic is used by the P1 API.

Expose a minimal additive `project_thesis_lifecycle` adapter in `qxfx0-self`. It is read-only,
deterministic, and separate from existing `PerspectiveRegistry` DTOs and behavior. Shadow tests
assert projection leaves the existing registry replay digest unchanged.

## Compatibility and scope

P0 canonical digest v1 is unchanged. Legacy commitment and Perspective APIs remain intact.
P1 does not add persistence migrations, persistence mutation, or pipeline mutation. Promotion
into either path requires a later ADR and versioned compatibility plan.

## Consequences

Lifecycle transitions are auditable and deterministic, at the cost of bounded retained history
and explicit typed receipts at call sites. Matrix tests cover replay, relation semantics, stale
receipts, ordering, revision rollback/cycles, terminal states, bounds, and serialization.

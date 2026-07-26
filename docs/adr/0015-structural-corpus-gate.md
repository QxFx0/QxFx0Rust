# ADR-0015: Structural corpus gate before renderer authority

- Status: Accepted
- Date: 2026-07-27

## Context

`audited_v1` has 30 content-admitted topics, but the current renderer still
uses the legacy route-and-graph path. Switching renderer authority without
checking the complete plan structure in isolated and long sessions would hide
topic leakage or predicate substitution behind superficially fluent text.

## Decision

Freeze the 30 governed definition prompts as a Rust test fixture and run them
through the full pipeline in two modes: one fresh session per topic and one
shared 30-turn session. The `plan_shadow` trace must prove for every admitted
topic:

- the exact topic and canonical subject/relation/object slots;
- the exact admitted predicate set, claim roles and derivation sequence;
- curated provenance, no duplicate claims, and the planned sentence budget;
- a single terminal mark on the still-legacy response.

The same gate requires a recognized but unadmitted topic to record
`NoAdmissiblePredicate` rather than silently claiming content authority.

## Consequences

The gate is in the normal workspace test suite. It validates the semantic plan
and the renderer-independent surface invariant, not semantic fidelity of the
legacy renderer. The next renderer PR must preserve this gate and add
plan-to-surface checks for grounded leaves and morphology before it receives
authority.

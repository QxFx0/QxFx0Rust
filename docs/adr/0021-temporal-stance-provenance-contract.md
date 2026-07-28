# ADR-0021: Temporal anomaly evidence requires typed stance provenance

- Status: Proposed
- Date: 2026-07-29

## Decision

Temporal anomaly evidence is derived only from two typed `SystemDecision`
stance observations with the same normalized topic, strictly increasing turns,
and opposite `StancePolarity`. `UserInput` and `ExternalReference` are evidence
sources, not adopted system stances, and cannot trigger a temporal anomaly.

The pure contract uses a bounded `VecDeque`, explicit duplicate outcome, and a
single bridge to the existing `AnomalyEvidence::Temporal` labels. Its reference
vectors prove contradiction selection, source exclusion, bounded eviction and
deterministic serialization.

## Non-decision

This ADR does not add a field to `SystemState`, change persistence schema,
modify the pipeline, or enable temporal anomaly shadow tracing. It therefore
does not define durable idempotency, migration, retention, rollback, or user
surface semantics.

## Promotion gate

Any runtime integration must first define versioned persisted provenance and
rollback semantics, then pass feature-flagged shadow trace, replay and corpus
gates. Recovery strategies remain disabled until a separate limited-enablement
decision.

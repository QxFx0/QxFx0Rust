# ADR-0020: Anomaly recovery is typed, bounded and trace-visible

- Status: Proposed
- Date: 2026-07-27

## Decision

Anomaly recovery will use a closed `AnomalyKind` and typed recovery outcome,
not string labels or implicit renderer fallback. The first scope is bounded:
anti-conatus, self-referential collapse and explicit user anomaly acts.

## Required contract

- detection has an explicit evidence set and threshold;
- recovery is idempotent for the same event identity;
- retry/counter state is bounded;
- output changes only through an approved route/plan outcome; and
- trace records kind, evidence, selected strategy, result and state digest.

Production integration follows Essence parity and requires replay vectors plus
guard rollback tests before a feature flag can be enabled.

## Observation-only shadow integration

The first runtime bridge is deliberately not recovery integration. An explicit
`--anomaly-shadow-trace-jsonl PATH` creates a new external JSONL artifact and
records a deterministic `anomaly_shadow` pipeline step. It proposes typed
self-reference or anti-conatus recovery only; it cannot change the actual
family, plan, renderer, routing, or persisted `SystemState`.

Temporal evidence is reported as unavailable until the runtime has typed,
replay-visible stance provenance. It must not be inferred from topic strings
or free-form dialogue history. The per-turn ledger is ephemeral and bounded;
promotion to durable replay/idempotency semantics requires a separate
versioned state-contract decision.

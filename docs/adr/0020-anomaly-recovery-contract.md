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

# ADR-0022: Persisted temporal stance provenance

- Status: Proposed
- Date: 2026-07-29

## Decision

Schema v8 adds nullable `session_semantic.stance_provenance_json`. It stores
the V1 bounded typed provenance contract, including an explicit version and
retention capacity. Migration adds only the column and does not rewrite
`runtime_sessions.state_json`, normalized rows, or session data. A legacy row
with a NULL column loads as an empty V1 provenance store.

Recording is default-off. `qxfx0 turn --record-stance-provenance` records an
allowed turn's normalized subject as an explicitly affirmed `SystemDecision`.
This policy is intentionally narrow and is not inferred from free-form user
history. It does not consult provenance for routing, family, plan, renderer,
or recovery.

## Rollback semantics

Recording occurs after the normal guarded pipeline completes. A stage failure
uses the existing pre-turn rollback; a guard-rejected turn records no stance
observation. Persistence remains the existing single SQLite transaction, so a
failed save cannot commit only provenance.

## Non-decision

This does not enable temporal anomaly shadow tracing or any recovery strategy.
Trace and diagnostics combinations fail fast until their joint evidence API is
defined. A temporal shadow PR must still add replay/corpus evidence before any
separate limited-enablement decision.

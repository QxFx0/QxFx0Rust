# ADR-0019: Essence parity is a law-and-replay contract

- Status: Proposed
- Date: 2026-07-27

## Decision

The existing Rust `EssenceState` will be aligned with the Haskell reference by
laws, not by copying module structure. The integration PR must introduce typed
inputs/results for witness, `should_commit`, commit validation and collapse.

## Required laws

- witnesses are bounded and ordered by turn;
- `should_commit` is pure and deterministic from the bounded trajectory;
- a commit is emitted only after post-commit validation;
- collapse clears the live commitment and creates a replay-visible recovery
  record; and
- identical vectors produce the same trajectory digest and decision.

The PR must contain cross-language vectors, property tests for the finite
state transitions and no hidden I/O/global mutable state in the self layer.

## Compatibility and migration decision (2026-07-28)

The first parity implementation is deliberately a compatibility proof, not a
state migration. Rust sessions already persist `EssenceState` both inside the
legacy `runtime_sessions.state_json` blob and in normalized
`session_semantic.essence_json`. The current Rust reset value
`conatus_floor = f64::MAX` is therefore a persisted, replay-observable
contract even though the Haskell reference resets the corresponding field to
`1.0`.

Until an explicit migration is accepted:

- missing `conatus_floor` continues to deserialize through the existing Rust
  default (`f64::MAX`);
- an existing numeric value is retained unchanged through load/save;
- `collapse_essence` continues to reset to `f64::MAX`;
- no database schema migration, data rewrite, or implicit normalization is
  performed; and
- legacy and normalized state must replay through load → collapse → save →
  load without changing the reset event, commitment clearing, witness clearing
  or floor value.

A future change to `1.0` requires a separately approved, versioned migration
plan with vectors for both old and new serialized values, atomic rollback
behaviour, and corpus/replay evidence. It cannot be folded into a law or
renderer change.

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

# ADR-0018: Bounded doubt and episodic recall remain pure until route admission

- Status: Accepted
- Date: 2026-07-27

## Context

The Haskell reference has a living doubt loop and episodic consumer. A direct
production port would change family selection and could reintroduce repeated
clarifications or untraceable memory reads.

## Decision

Create a pure, unconnected Rust conformance layer with:

- `DoubtScore = clamp(1 - confidence)`, plus `0.2` for counterfactual
  ambiguity and a `0.9` floor for a conatus gate;
- a bounded, topic-scoped episodic store (64 retained, 50-turn recall window,
  at most 8 recalled events by default);
- `DoubtRoute`: retain current route, clarify, or suppress clarification after
  a recent same-topic `SystemDecision`;
- language-neutral JSON vectors and tests for the producer and consumer laws.

No pipeline route, renderer, `SystemState` persistence or feature flag reads
this layer in this PR.

## Admission gate for a later route connection

1. All current output-parity and structural corpus tests remain unchanged with
   the feature disabled.
2. Trace records score, bounded recall IDs/count, threshold and chosen route.
3. Parser-locked, safety and explicit fallback routes retain authority.
4. Shared-session corpus proves that a recent decision suppresses only a
   same-topic redundant clarification.
5. Replay produces identical score, recalled IDs and route.

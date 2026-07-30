# ADR-0026: Pure stance issuer request context

- Status: Proposed
- Date: 2026-07-30

## Decision

Expose a pure typed `prepare_stance_request_context` library boundary for an
integrating service. It returns only a version, session id, pre-turn count,
the exact pipeline-normalized `StanceTopic`, and the existing versioned stance
request digest. It never returns raw input and never mutates `SystemState`.

Topic parsing and normalization are shared with the authoritative pipeline
path. For a fresh state, preparation uses an ephemeral seed graph while the
actual pipeline retains its existing persisted seed behavior. Session and
state invariants are checked before any context is returned.

## Consequences

An external issuer can receive the exact topic binding without receiving raw
user input or maintaining a second normalizer. The resulting attestation is
still independently checked by the signed stance boundary for audience,
session, pre-turn, request digest, time, signature and post-turn topic match.

This change adds no network, clock, key management, persistence, stance
recording, routing, renderer, recovery, rollout or CLI surface.


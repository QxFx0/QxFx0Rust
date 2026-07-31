# ADR-0027: Narrow temporal recovery enablement contract

- Status: Proposed
- Date: 2026-08-01

## Context

The runtime can verify an authoritative signed stance, persist typed rejected
provenance and emit a deterministic temporal anomaly shadow proposal. None of
those contracts authorizes a recovery strategy. In particular, persisted V1
stance provenance does not itself retain signed-issuer identity, decision ID
or an attestation fingerprint. A future integrating boundary must therefore
supply separate typed authority evidence; it must not infer authority from
user text or from provenance history alone.

## Decision

Add a pure `TemporalRecoveryEligibility` contract in `qxfx0-self`. It admits
only the existing typed decision:

```text
Temporal -> RequestRevision -> RevisionRequested
```

Eligibility requires all of the following:

- explicit `LimitedNonProduction` mode;
- a non-production environment and an allowlisted session;
- exact equality between the anomaly decision evidence and the evidence derived
  from the supplied typed `TemporalStanceContradiction`;
- an affirmed current and rejected historical observation, both originating
  from `SystemDecision`;
- exact equality of the current and historical typed topics;
- non-empty issuer and key identities, a 16-byte decision ID encoded as lower
  hex, and a 32-byte signed-payload fingerprint encoded as lower hex;
- an unblocked turn and fresh provenance;
- replay consistency and a ready durable audit boundary;
- zero earlier revision requests for that topic in the caller-defined bounded
  window;
- a valid session/window scope.

Production, disabled and shadow-only modes are denied. Self-reference,
anti-conatus, retries and every strategy other than `RequestRevision` are
denied. The result is a capability-shaped value for future design work only.
No runtime path consumes it.

## Deterministic denial precedence

The first failed predicate wins in this order: mode, environment, session
allowlist, authority proof, strategy, evidence/lineage binding, topic equality,
blocked status, freshness, replay, audit, prior request and scope validity.
This makes replay and audit interpretation stable.

## Evidence

`docs/reference-vectors/temporal-recovery-enablement-v1.json` defines the
eligible state and every fail-closed branch. The executable conformance test
proves deterministic serialization, decision/contradiction/context
immutability, strict typed evidence binding, a maximum of one request per
topic/window and absence of raw signatures, attestations and request digests
from the permit.

## Non-goals and next gate

This ADR does not add a feature flag, pipeline consumer, state transition,
clarification surface, persistence schema, Turn Service integration or audit
event. Recovery remains disabled.

Any implementation PR must first define the authoritative later-turn linkage
between issuer audit evidence and persisted stance provenance, the bounded
window owner, a durable per-topic request counter, rollback semantics and an
external audit schema. It then requires shadow/replay/corpus evidence and a
separate explicit enablement decision.

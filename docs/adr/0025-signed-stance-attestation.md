# ADR-0025: Signed external system-stance attestation

- Status: Proposed
- Date: 2026-07-29

## Decision

An external authorized issuer may bind an explicit `SystemStanceDecision` to
one QxFx0 turn with a versioned Ed25519 signed attestation. The Rust runtime
does not fetch it, call a network service, generate keys, refresh key material,
or read its own wall clock. An integrating service supplies the signed payload,
configured public-key verifier, expected audience, and explicit verification
time plus maximum validity window.

The canonical payload binds `issuer_id`, `key_id`, audience, session id,
pre-turn number, normalized topic, polarity, SHA-256 request digest, opaque
decision id, and validity window. The runtime rejects an invalid signature or
any mismatched/expired binding, then processes the ordinary turn without a
provenance write. Only a verified payload whose topic exactly equals the
pipeline-normalized topic may call the existing explicit stance recorder.

## Consequences

The default pipeline and CLI remain unchanged. There is no HTTP/gRPC transport,
key management, nonce persistence, recovery strategy, routing, renderer, or
default rollout in this change. The `(session_id, expected_pre_turn,
request_digest)` binding prevents a successfully persisted attestation from
being accepted for a later state; durable replay-consumption tracking is a
separate persisted-semantics decision if later required.

The contract is tested against an executable canonical payload/signature vector
and deterministic verification time. An in-process caller with unrestricted
access to the trusted explicit-decision API is outside this signature boundary;
the integrating service must not expose that API directly to untrusted plugins.

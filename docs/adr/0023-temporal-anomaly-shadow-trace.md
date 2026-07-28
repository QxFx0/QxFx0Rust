# ADR-0023: Temporal anomaly shadow trace

- Status: Proposed
- Date: 2026-07-29

## Decision

The existing opt-in `--anomaly-shadow-trace-jsonl` reads persisted V1 stance
provenance and creates one in-memory, affirmed `SystemDecision` candidate for
the normalized current topic. A temporal proposal is admitted only when the
typed contract finds an earlier rejected system decision for that same topic.

The candidate, recovery ledger, and proposal remain trace-only. They do not
write provenance, invoke a recovery strategy, or affect routing, family, plan,
renderer, response, or persisted state. The JSONL sink remains external and
new-file-only.

## Evidence gate

The integration test seeds a prior typed rejected decision, proves that the
trace proposes `Temporal -> RequestRevision`, and proves ordinary output and
persisted provenance remain unchanged. Shared-session replay and corpus gates
remain required before any separate enablement decision.

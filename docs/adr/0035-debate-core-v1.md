# ADR-0035: Debate Core v1 observation boundary

- Status: Proposed
- Date: 2026-08-04

## Decision

Debate Core v1 is a deterministic, typed observer that runs after
`plan_shadow`. It projects immutable route and response-plan evidence into a
per-turn argument graph, debate move, append-only position-ledger projection,
and evidence-backed rubric. It is disabled by default and can be enabled only
as `DebateCoreMode::TraceOnly` or through
`turn --debate-core-trace-jsonl PATH`.

The receipt is attached only to `PipelineTrace`. The CLI writes a receipt-only
record rather than the generic pipeline envelope, and its sink is external and
new-file-only. Debate Core does not change routing, planning, rendering,
guarding, governance, commitments, response output, `SystemState`, or SQLite.
Persistent cross-turn positions are outside this decision and require a
separate schema, correction/export/delete policy, migration, and promotion
review.

## Evidence contract

The v1 receipt contains canonical topic IDs or categorical subject labels,
typed move/node/edge/ledger/rubric values, claim IDs, `FactId` references, and
a domain-separated SHA-256 digest over a length-prefixed binary contract. It
does not contain session IDs, raw user input, external subject labels,
proposition text, rendered responses, or wall-clock values.

Validation bounds graph and ledger sizes, rejects duplicate or dangling
nodes, self-edges, non-contiguous ledger entries, duplicate rubric dimensions,
rubrics without typed evidence, unsupported versions, invalid IDs, score
overflow, and digest mismatch.

## Promotion boundary

Tests must prove ordinary output and final-state parity, deterministic receipt
digests, create-new sink behavior, and absence of raw prompt and response text
from JSONL evidence. Observation results cannot grant renderer or policy
authority. Any persistence or user-visible behavior is a separate ADR and
rollout gate.

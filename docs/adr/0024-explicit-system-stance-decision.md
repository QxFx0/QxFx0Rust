# ADR-0024: Explicit system stance decision boundary

- Status: Proposed
- Date: 2026-07-29

`SystemStanceDecision` is a caller-authorized typed API boundary for an
affirmed or rejected system stance. The pipeline accepts it only after an
allowed turn whose normalized topic exactly equals the typed decision topic.
It never derives polarity from user text, history, a guard result, or response
surface. The ordinary CLI remains unable to create rejected stance provenance.

This contract does not enable recovery, routing, planning, rendering, or a
default rollout. A future authoritative producer must independently pass its
replay/corpus gate before it can call this API.

# ADR-0032: Fact-grounded stance rendering

- Status: Accepted
- Date: 2026-08-01

## Context

ADR-0018 stores a typed Perspective, but the renderer previously ignored it.
Using a free-form stance string would create a second declarative path and
could make a forged or stale opinion sound authoritative. The renderer needs a
small, deterministic adapter that changes discourse only after the existing
FactId leaf has been authorized.

## Decision

`qxfx0-self::resolve_render_stance` returns one of three typed decisions:

- `Neutral` when the current topic has no persisted opinion;
- `Affirmed` when the opinion cites the rendered curated thesis and has no
  curated counterpoint;
- `Qualified` when the opinion cites that thesis and at least one curated
  `Counters` fact targeting it.

The adapter re-selects the thesis and every grounding FactId through the active
`FactRegistry`, checks the subject identity, and rejects unsupported `Opposed`
polarity. It returns no surface text. `qxfx0-pipeline` maps the decision to a
fixed discourse prefix, while the sentence body still comes exclusively from
the topic's audited renderer leaf for that same FactId.

The first response for a topic is therefore neutral: Perspective is updated in
finalize after rendering. A later response can expose the qualified stance,
and replaying a plan remains idempotent.

## Consequences

- A stance cannot introduce a new fact, relation, or user-text claim.
- Forged, stale, non-curated, cross-topic, or unsupported opinion state fails
  closed before rendering.
- Existing first-turn audited surfaces remain stable; subsequent turns expose
  only the typed state already persisted by the FactId operator.

## Contract metadata

- Relates to: ADR-0024, ADR-0025, ADR-0031, ADR-0033.
- Supersedes: feature branch ADR-0019 numbering only.
- Main contract: ADR-0024 explicit system stance and ADR-0025 signed attestation.
- Contract version: Fact-grounded renderer adapter v1.
- Reference vectors: `qxfx0-self` fact perspective and signed stance tests.

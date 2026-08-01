# ADR-0033: Fact-grounded state and authoritative stance composition

- Status: Accepted for default-off integration
- Date: 2026-08-01

## Decision

QxFx0 has two independent authorization results. The immutable curated
`FactRegistry` authorizes factual renderer leaves and the signed Ed25519
attestation authorizes external system stance provenance. Neither result can
silently substitute for the other.

`PerspectiveRegistry` remains the main authoritative projection registry and
its immutable `PerspectiveProjection` remains the renderer boundary.
`PerspectiveState` is bounded per-session semantic evidence containing only
`ConceptId`, `FactId`, typed polarity, and revision metadata. It does not
replace the registry and it is not the main bounded doubt/episodic trace.

`PerspectiveEpisode` is typed semantic memory. A future connection to the
main `BoundedEpisodicStore` may use only an explicit typed observation event;
observations never promote into facts and memory is never an implicit
authority source.

The composed path preserves the exact optional `SystemStanceDecision`
alongside the fact-grounded discourse modifier. `Rejected` is never converted
to `Opposed`; that policy requires a new ADR and reference vectors. A missing
attestation does not erase a valid local grounding decision, and an
attestation does not authorize a factual sentence absent from the curated
FactId leaf.

The rollout enum is `Disabled`, `Shadow`, `TraceOnly`,
`LimitedNonProduction`, or `Enabled`; the default is `Disabled`. The legacy
route, renderer, CLI behavior, production database, pilot artifacts, recovery
and soak behavior remain unchanged until explicit enablement.

Perspective JSON persistence is additive in SQLite schema v9. State save/load
validates JSON, bounded semantic state, active pack fingerprint and every
referenced FactId. A mismatch fails closed before semantic stages. All
semantic updates remain inside the caller's rollback snapshot; static facts
are never copied into session state.

## Contract metadata

- Relates to: ADR-0017, ADR-0018, ADR-0024, ADR-0025, ADR-0027, ADR-0030–0032.
- Supersedes: none; this is the integration boundary ADR.
- Contract version: Fact-grounded authority composition v1.
- Reference vectors: `qxfx0-pipeline/src/fact_grounded.rs` tests,
  `qxfx0-self` fact perspective tests, and existing signed/temporal stance vectors.

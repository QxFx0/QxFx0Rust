# ADR-0031: Fact-grounded Perspective and semantic episodes v1

- Status: Accepted
- Date: 2026-08-01

## Context

Fact Model v1 made declarative output auditable, but the session still had no
typed position derived from those facts. The legacy commitment store contains
generated dialogue surfaces and is explicitly not factual authority. Reusing
it for Perspective would restore the boundary that ADR-0015 removed.

ADR-0016 requires the first Perspective slice to cite `FactId`, preserve
polarity, record deterministic revision reasons, use bounded per-session
episodes, and never convert observations into facts.

## Decision

`FactId` identity moves to `qxfx0-types`; immutable `FactRecord` authority and
selection remain in `qxfx0-semantic`. This lets persisted session types hold
typed references without creating a crate dependency cycle or copying static
facts into `SystemState`.

`PerspectiveState` contains:

- at most 1,024 `OpinionCore` records keyed by `ConceptId`;
- at most 2,048 `PerspectiveEpisode` records with monotonic per-session ids;
- a primary FactId, polarity, confidence and a deterministic set of grounding
  FactIds for each opinion.

The operator accepts only `(ClaimRole, FactId)` pairs from an already built
`ReadyResponsePlan`. Every id is selected again through the active
`FactRegistry`, so draft, deprecated, retracted or missing facts fail closed.
There is no string parameter for observed or generated text.

For an admitted plan, the curated thesis establishes an `Affirmed` position.
A curated fact whose `FactCondition::Counters` targets that thesis changes the
position to `Qualified` and records both ids as the revision cause. A curated
`FollowsFrom` consequence adds a reinforcement episode. Replaying the same
plan is idempotent and creates no duplicate episodes.

Perspective updates occur during finalize and are part of the existing
semantic rollback snapshot. Unknown, ambiguous and blocked turns therefore
 cannot change the store. SQLite schema v9 persists Perspective in its own
normalized JSON column. Persistence validates every referenced fact against
the active pack when fingerprints match; pack-mismatched sessions retain the
existing typed block before semantic execution.

## Consequences

- A position and every revision cause can be audited back to curated FactIds.
- Episode records have no field capable of storing raw user or generated text.
- Static FactRecords remain process-global and are not copied into sessions.
- Deterministic trace output exposes opinion and episode counts.
- Doctor reports Perspective limits and available curated counterpoint links.
- Renderer wording remains governed by FactId-authorized leaves. ADR-0032 adds
  a stance decision on top of those leaves without introducing another
  declarative surface path.

## Contract metadata

- Relates to: ADR-0017, ADR-0018, ADR-0030, ADR-0032.
- Supersedes: feature branch ADR-0018 numbering only.
- Main contract: ADR-0017 PerspectiveProjection and ADR-0018 bounded doubt/episodic.
- Contract version: Fact-grounded Perspective v1.
- Reference vectors: `qxfx0-self` fact perspective and persistence tests.

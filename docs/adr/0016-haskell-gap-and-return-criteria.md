# ADR-0016: Haskell capability gap and return criteria

- Status: Accepted
- Date: 2026-08-01

## Context

The Rust runtime deliberately ported the deterministic turn boundary, typed
semantic graph, morphology, persistence, governance and audited renderer before
the wider Haskell learning and subjectivity layers. Without an explicit gap
record, absent capabilities can be mistaken either for implemented behavior or
for permanently rejected design.

Rust currently recognizes 107 topics but admits declarative content for 30.
There is no production learning or automatic promotion path. The Haskell corpus
is useful source material, not runtime authority: its 6,239 rows contain 4,050
trimmed raw topic strings and free-form bilingual surfaces rather than typed
`FactRecord` values.

## Decision

The following subsystems remain deferred and return only through bounded,
independently reviewable vertical slices:

- **Self/Perspective**: return after a typed belief can cite `FactId`, preserve
  polarity and record a deterministic revision reason. First validate on the
  30 audited topics; broad corpus coverage is not a prerequisite.
- **Episodic memory**: return with explicit retention bounds, per-session
  ownership, replay-stable event identities and no conversion of observations
  into facts. It must demonstrate a position change caused by cited episodes.
- **Learning/promotion**: return only after immutable pack manifests,
  quarantine, conflict handling, review evidence, activation fingerprint and
  rollback are production contracts. User or generated text must never
  self-promote.
- **Topic drift and analogy**: return after property tests prove unknown or
  ambiguous surfaces cannot create graph state and every declarative result
  still resolves to a curated `FactId`.
- **GF**: return when measured renderer repetition or morphology failures show
  a concrete quality gain that justifies its runtime and build cost.
- **Agda/Datalog**: return for stable, high-value invariants that property tests
  cannot express economically. Initial candidates are legitimacy and
  sovereignty; no broad proof-toolchain port is implied.

The historical Haskell implementation is evidence and test-corpus input, not a
specification to copy wholesale. Each returning subsystem requires a Rust ADR,
typed boundary, deterministic replay evidence and a rollback story.

## Consequences

- Product claims describe an auditable dialogue runtime; “digital subject” is
  an internal direction until Perspective and episodic slices meet the criteria.
- Corpus breadth, renderer diversity and subjectivity can progress as separate
  measured tracks without weakening fact admission.
- Deferred code volume is not itself a reason to port a subsystem.

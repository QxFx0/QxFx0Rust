# ADR-0035: P0 typed thesis identity and authority graph (draft)

- Status: Draft
- Date: 2026-08-04

## Context

`FactRecord` is the current curated authority contract, while response-plan and
pack fingerprints depend on its existing `FactId`, `SemanticId`, and JSON
shape. P0 needs a reusable typed thesis identity and relation graph without
moving persistence or changing pipeline behavior.

## Decision

Add the dependency-neutral thesis algebra to `qxfx0-types`, reusing
`ConceptId`. `RelationId` is introduced there rather than moving response-plan
`SemanticId`; the semantic crate owns the explicit adapter.

Canonical encoding v1 is binary and domain-separated. It consists of a u32
big-endian byte length plus `qxfx0:thesis:canonical:v1`, version byte `1`, then
u32 big-endian length-prefixed UTF-8 subject, predicate, and object, a stable
one-byte `ThesisKind` tag, a u32 qualifier count, and sorted key/value pairs,
each with u32 big-endian byte lengths. SHA-256 of those bytes is serialized as
exactly 64 lowercase hexadecimal characters.

The thesis id, surface text, confidence, provenance, and validity timestamps do
not participate in identity. All fields are nevertheless strictly bounded and
validated before canonicalization. Qualifiers use `BTreeMap`.

`ThesisGraph` uses `BTreeMap`/`BTreeSet`, rejects missing endpoints and all
self-edges, makes exact duplicate insertion idempotent, and rejects cycles over
the combined `Revises`/`Supersedes` subgraph. Its limits are explicit.

`FactRecord::canonical_thesis()` maps current fact slots and metadata without
changing `FactRecord`. `FactRegistry::canonical_thesis_graph()` maps
`Counters` and `FollowsFrom` in dependent-to-referenced direction. The current
69 facts and 39 conditions are covered by a census test.

## Compatibility

`FactId`, response-plan APIs, knowledge-pack JSON, and fingerprint v1 remain
unchanged. No persistence schema/migration or pipeline behavior is changed in
P0. A future version may promote the thesis graph only under a separate ADR and
versioned migration.

## Consequences

Canonical bytes are now a public compatibility contract and require a new
version/domain for any change. Reference vectors, order invariance, semantic
sensitivity, excluded metadata, bounds, graph integrity, cycle, and complete
legacy mapping tests guard the contract.

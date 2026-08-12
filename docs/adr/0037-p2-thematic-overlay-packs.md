# ADR-0037: P2 thematic knowledge overlay packs

- Status: Accepted
- Date: 2026-08-01

## Context

P0 introduced canonical thesis identity and a bounded authority graph. P1 added pure thesis lifecycle transitions. The curated `philosophy-core-v1` pack remains the owner of all existing ConceptId and FactId records. P2 needs deeper thematic organization without copying those authority records or changing v1 canonical/fingerprint semantics.

## Decision

Knowledge pack schema dispatch is explicit. Schema v1 retains exactly `concepts.json`, `facts.json`, and `relations.json`, with its existing manifest fingerprint inputs unchanged. Schema v2 is an overlay containing `theses.json`, `relations.json`, and `lifecycle.json`; it must depend exactly on `philosophy-core-v1` and must not contain concepts or facts.

An overlay thesis is metadata keyed by an existing authority FactId and its recomputed P0 ThesisDigest. The digest is validated against the canonical v1 adapter, so metadata cannot alter semantic identity. Explicit overlay edges use the closed set Supports, Counters, Contradicts, Qualifies, Entails, and DependsOn. Lifecycle scenarios name existing local digests and require an explicit trigger edge. They are declarative fixtures only and are not session persistence.

The three active P2 overlays are `agency-responsibility-v1`, `epistemology-truth-v1`, and `mind-memory-language-v1`. Every overlay has zero authority ownership, all endpoints resolve through the core authority registry, and build output is deterministic.

## Validation and ownership

The loader rejects unsupported schemas, unknown fields, file/hash census drift, missing dependencies, bounds violations, duplicate pack/thesis/digest/scenario ownership, authority digest tampering, dangling/self/duplicate edges, incomplete relation-kind coverage, and invalid lifecycle triggers. Pack-set summaries and fingerprints are sorted independently of input order.

## Consequences

- `philosophy-core-v1` remains the sole owner of the referenced concepts and facts.
- V1 canonical thesis and core asset fingerprint semantics are unchanged.
- The active pack-set fingerprint changes because reviewed overlays are activated.
- No lifecycle or overlay state is persisted to sessions in P2.

## Contract metadata

- Relates to: ADR-0030, ADR-0035, ADR-0036.
- Reference implementation: `qxfx0-semantic/src/knowledge_pack.rs`.
- Deterministic builder: `scripts/build_thematic_packs.py`.

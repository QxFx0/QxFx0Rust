# ADR-0039: P4 typed thematic catalog and activation policy

- Status: Accepted
- Date: 2026-08-01

## Decision

P4 adds a bounded catalog DTO in `qxfx0-types`, keeping semantic activation dependent on types rather than creating a dependency cycle. Catalog and manifest identities use deterministic canonical SHA-256 digests. Validation fails closed on duplicate IDs, invalid digest pins, missing or cyclic dependencies, ownership collisions, invalid overlays, unbounded metadata, and authority claims in non-approved or discovery-only entries.

The immutable semantic registry separates discovery from authority. Activation requires an exact `(pack_id, version, manifest_digest)` allowlist match, `approved` lifecycle, `curated_embedded` trust, and the complete digest-pinned dependency closure. Candidate/planned entries are discoverable metadata only and contain no authority facts. Persistence is intentionally unchanged.

The catalog includes the core and three approved deep packs plus candidate/planned entries for ethics/social order, existential, affect/action, aesthetics, and temporality/history.

## Promotion gates

An operator may promote a future entry only in a reviewed release after all gates pass:

1. **License:** compatible license is explicit for every payload and source.
2. **Provenance/evidence:** commit-exact provenance, content hashes, evidence links, and confidence assessments validate.
3. **Relation integrity:** all thesis/fact endpoints resolve, dependency pins form a DAG, and ownership/overlay declarations have no undeclared collision.
4. **Morphology/renderer coverage:** every admitted concept and thesis passes morphology lookup and audited renderer/corpus coverage.
5. Regenerate the catalog, review the digest diff, add the exact pin to the activation allowlist, and run the full release gates. Discovery or a catalog edit alone never promotes authority.

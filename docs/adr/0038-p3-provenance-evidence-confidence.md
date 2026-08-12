# ADR-0038: P3 provenance, evidence, and confidence

- Status: Accepted
- Date: 2026-08-01

## Decision

P3 introduces bounded typed evidence/source/assessment identifiers, closed evidence/trust/link-role enums, immutable evidence records, thesis links, and confidence assessments. Canonical SHA-256 digests are versioned and domain-separated. Evidence-set identity sorts record digests. Confidence is basis points only; aggregation uses checked integers and no wall clock.

Authority policy v1 admits only curated embedded and verified signed external evidence. User/generated observations and quarantine remain auditable in the evidence-set digest but cannot increase authority confidence. Unknown policy versions, dangling references, duplicate ownership, digest mismatch, invalid basis points, and assessment mismatch fail closed.

Schema-v2 thematic overlays add `evidence.json`, `evidence-links.json`, and `assessments.json`, all manifest-hashed. The core schema-v1 assets and canonical thesis semantics are unchanged. P3 state is not connected to session persistence.

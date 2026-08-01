# ADR-0028: Fact Model v1 and curated renderer authority

- Status: Accepted
- Date: 2026-07-31

## Context

Graph relations, observed user text, generated dialogue outcomes, and curated
facts previously had no hard semantic boundary. The audited response plan was
structural, but the route-based renderer could still emit graph-selected
declarative text that had no `FactId`.

The audited corpus also used 30 canonical object slots that were not registered
concept identities. Representing those slots as self-referential fact objects
would validate structurally while changing their meaning.

## Decision

Introduce immutable `FactRecord` and `FactRegistry` types. Registry loading
fails on missing provenance, duplicate `FactId`, unknown subject/object
concepts, untyped relations, invalid confidence, or missing fact dependencies.
Only `FactStatus::Curated` is selectable.

Register the 30 canonical fact-object slots as auxiliary versioned concepts in
`core-concepts-v1`. Their graph atoms are `CatConcept` values and are not added
to the 107 covered dialogue topics. Audited facts now reference those
`ConceptId` values directly instead of self-referential placeholders.

`ReadyResponsePlan` is renderer-authoritative for declarative content. Before a
surface is emitted, every declarative claim is checked against the immutable
registry, its fact subject is checked against the plan topic, and its FactId is
bound to the audited renderer leaf. A fallback plan produces a bounded typed
response and never enters the graph content composer. Dialogue and external
system contracts remain non-factual typed frames.

Generated output is recorded as `DialogueObservation`. Historical
`FactualClaimPayload` state is legacy-only and is never consulted by fact
selection or the curated renderer.

The trace step name `plan_shadow` is retained solely for replay compatibility;
its `ContentV1` result is no longer observational.

## Consequences

- User input and generated output cannot mutate the fact registry.
- Deprecated, retracted, and draft facts fail closed before rendering.
- All declarative surfaces for the 30 audited topics resolve through FactId.
- Known but unadmitted topics receive typed fallback without declarative graph
  claims.
- Adding factual content requires a reviewed concept, typed relation,
  provenance, and curated renderer leaf.

## Contract metadata

- Relates to: ADR-0014, ADR-0029, ADR-0030.
- Supersedes: feature branch ADR-0015 numbering only.
- Main contract: ADR-0014 audited content plan.
- Contract version: Fact Model v1.
- Reference vectors: `qxfx0-semantic` fact/knowledge-pack tests.

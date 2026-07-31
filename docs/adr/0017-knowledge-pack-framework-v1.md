# ADR-0017: Knowledge Pack Framework v1

- Status: Accepted
- Date: 2026-08-01

## Context

Concept Registry and Fact Model v1 established stable identities and curated
declarative authority, but their production records still needed one immutable
activation boundary. Directly importing thousands of Haskell surfaces would
bypass typed relations, provenance, conflict detection and rollback.

## Decision

An active knowledge pack contains `manifest.json`, `concepts.json`,
`facts.json` and `relations.json`. The manifest fixes pack/schema versions,
source repository, full source commit, MIT license and the SHA-256 of every data
file. Validation completes before JSON records are admitted.

`KnowledgePackSet` merges only the build-fixed source list. It rejects duplicate
pack, concept and fact IDs. Equal aliases are preserved as explicit ambiguity.
Facts must use a relation declared by their pack and known to the typed relation
model. Multiple objects for an equal subject/relation key are treated as an
unresolved conflict and fail the whole active set.

The active set is process-global. `SystemState` stores only its fingerprint;
existing empty legacy fingerprints migrate on the next turn, while a non-empty
mismatch blocks the turn before semantic stages. Deterministic traces and
`doctor` expose pack IDs, versions and fingerprint.

The offline Haskell importer is intentionally non-promoting. It emits inventory,
quarantine and metrics files with a separate hash manifest. A candidate requires
explicit ConceptId, typed relation/object slots, morphology coverage, provenance
and review before a later tool may construct a new pack.

Rollback means rebuilding or deploying with a previously reviewed immutable
pack set. Sessions carrying the newer fingerprint fail closed rather than being
silently replayed against older semantic authority; an explicit migration tool
is required to authorize such a replay.

## Consequences

- Static pack data is not copied into session state.
- Duplicate aliases never become last-write-wins behavior.
- Corpus size can grow without widening production authority automatically.
- The current pilot is evidence about quarantine work, not additional runtime
  knowledge.

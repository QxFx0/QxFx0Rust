# ADR-0017: Immutable, replayable PerspectiveProjection boundary

- Status: Accepted
- Date: 2026-07-27

## Context

The Haskell reference model separates a bounded, versioned perspective registry
from the public object used for rendering and replay. Allowing a Rust renderer
to read a mutable registry or arbitrary raw state would make surface behaviour
depend on non-replayable implementation details.

## Decision

Introduce a pure `PerspectiveRegistry` in `qxfx0-self` and public immutable
DTOs in `qxfx0-types`:

- mutations are explicit `PerspectiveMutation` values with a typed decision;
- scopes, identity and version are typed;
- registry threads and their raw evidence remain private;
- `PerspectiveProjection` is built only from an active endorsed version;
- active projections, revision history, inactive versions, evidence and
  counterarguments are bounded deterministically;
- a canonical SHA-256 replay digest and reference-vector tests provide
  language-neutral conformance evidence (`perspective_projection_v1.json`).

This PR intentionally does not persist the registry, invoke promotion logic,
or connect a renderer. Production rendering therefore has unchanged authority.

## Consequences

Future perspective integration must pass immutable projections to rendering and
must retain explicit mutation/replay records. It may not expose registry maps
or use them as a renderer input. Persistence, bounded doubt/episodic selection
and operational promotion remain separate reviewed steps.

# ADR-0016: Controlled authority for audited plan rendering

- Status: Accepted
- Date: 2026-07-27

## Context

The `audited_v1` response plan can identify exact admitted claims, but legacy
rendering still reads the broad runtime graph. Its output may therefore differ
from the plan even when the plan itself is correct.

## Decision

Introduce explicit `RendererAuthority` with `legacy_shadow` as the default
and `audited_plan` as an opt-in CLI flag. In both modes, renderer trace records
whether a topic-backed plan surface was available and whether it matched the
returned output.

`audited_plan` resolves surfaces only by `PredicateRef` through the admitted
topic registry. It validates topic, canonical proposition, role-specific
predicate references, discourse structure and dialogue obligation before
rendering. It has no runtime graph input. Any non-topic ready plan or fallback
continues through the established route contract.

## Consequences

The 30-topic corpus gate now checks exact curated plan surfaces in fresh and
shared sessions, including punctuation and morphology embodied by the reviewed
leaves. The default surface remains unchanged until rollout explicitly enables
the flag. A new soak pilot must run with `--render-audited-plan` before the
flag can become the default.

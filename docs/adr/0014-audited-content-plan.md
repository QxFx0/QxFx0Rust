# ADR-0014: Audited content-bearing response plans

- Status: Superseded in part by ADR-0015
- Date: 2026-07-27

## Context

The shadow plan records routing but cannot prove which semantic content may be
stated. Rust recognizes 107 topics, while only 30 topics have a governed
Haskell release corpus with canonical predicate slots, counterpoints, and
reviewed consequences. Treating recognition as content authority would admit
semantic substitution and cross-topic leakage.

## Decision

Introduce the `audited_v1` admission registry for those 30 topics. Each entry
has a stable predicate ID, canonical subject/relation/object IDs, a
counterpoint, optional consequence, and curated provenance.

`PlanOutcome::Ready` now contains `ReadyResponsePlan` with structurally
non-empty claims and predicate references. Propositions live only inside their
claim. Plans carry predicate IDs, roles, evidence, bounded confidence,
discourse, dialogue obligation, and derivation steps; grounded surface strings
remain in the audited asset and are not copied into the plan.

A recognized topic outside `audited_v1` produces
`FallbackReason::NoAdmissiblePredicate`. Dialogue and external-question routes
use explicit system/user-input contracts. At the time of this decision the
renderer remained on its route-based path; ADR-0015 completes that cutover.

Doctor exposes `recognition_topics_total`, `content_predicates_total`,
`argued_topics_admitted`, `argued_predicates_admitted`, and `profile_enabled`.

## Deferred

Wider recognition assets remain non-argued until explicitly admitted by a
reviewed profile. The deferred renderer authority was completed by ADR-0015.

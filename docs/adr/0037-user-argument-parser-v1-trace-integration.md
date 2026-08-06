# ADR-0037: User Argument Parser v1 trace integration

- Status: Accepted for default-off observation integration
- Date: 2026-08-07

## Context

ADR-0036 defined privacy-bounded user-argument contracts and required a
reviewed gold corpus before runtime integration. The checked-in v1 corpus now
covers 17 curated-synthetic categories and all nine relation kinds, with no raw
production logs and no authority or persistence change.

The first runtime parser must establish deterministic integration, privacy,
and parity evidence without widening its semantic claims beyond reviewed
examples.

## Decision

Add `UserArgumentParserMode::{Disabled, TraceOnly}` to `TurnOptions`.
`Disabled` remains the default. In `TraceOnly`, the parser runs after
`plan_shadow`, reads the current input ephemerally, and attaches a validated
`UserArgumentParseReceipt` only to `PipelineTrace::user_argument_receipt`.

The v1 rule registry recognizes exactly the 17 reviewed-synthetic gold
formulations after deterministic trim and lowercase normalization. Every rule
has a stable identifier and explicit version. Inputs outside that reviewed set
produce `ParseDisposition::Abstained` with
`ParseOmissionReason::InsufficientEvidence`; they do not receive guessed nodes
or relations.

The parser emits no `ArgumentSpanDigest`. Unknown and external subjects use
the categorical `unresolved_topic` and `external_subject` variants. Raw input,
surface spans, session identity, response text, user-derived labels, offsets,
and wall-clock values never enter the receipt.

## External artifact

The CLI opt-in is:

```text
qxfx0 turn "..." --user-argument-trace-jsonl PATH
```

The sink is opened with create-new semantics before SQLite. It contains
exactly one receipt-only record with schema
`qxfx0.user-argument-parse-trace.v1`; the generic pipeline trace is not
exported. `verify-user-argument-trace PATH` checks the closed JSON schema,
single-record bound, one-megabyte file bound, receipt structure, and receipt
digest.

## Authority and persistence boundary

Parser output does not affect routing, planning, rendering, guard behavior,
stance, governance, commitments, response output, `SystemState`, or SQLite.
It does not create a position ledger, feedback, or parser authority. A future
rule expansion requires relation-level gold evidence and a separate review;
successful observation does not promote unmatched inputs from abstention.

## Required evidence

Tests bind the parser to every checked-in gold case and verify:

- all node, proposition, source, polarity, confidence, relation, disposition,
  and omission expectations;
- deterministic receipt replay;
- output and final-state parity with the ordinary path;
- default trace-schema compatibility;
- absence of reviewed formulations, privacy needles, response text, and
  session identity from artifacts;
- repeated unknown-topic privacy;
- receipt-only schema and verifier behavior;
- create-new sink failure before database creation.

## Consequences

This integration provides an honest exact-match baseline for the first
observation window. It intentionally has high abstention outside the reviewed
corpus. Broader compositional parsing, feedback rendering, and cross-turn
position persistence remain separate promotion boundaries.

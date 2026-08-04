# ADR-0036: User Argument Parsing v1 observation contract

- Status: Proposed
- Date: 2026-08-04

## Context

Debate Core v1 projects the typed system response plan into an argument graph.
It does not identify separate premises, conclusions, qualifiers, or
counterclaims in the user's message. Cross-turn positions, feedback, and
argument-quality evaluation cannot safely rely on the current receipt because
it describes system output rather than the user's full argument.

The first Debate Core observation window completed with zero validation,
replay, privacy, output-parity, and state-parity failures. This evidence permits
designing the next observation layer; it does not grant parser, feedback, or
ledger authority.

## Decision

User Argument Parsing v1 will be a deterministic, bounded, default-off observer.
It may read the current input ephemerally after `plan_shadow`, together with the
immutable prepare and route contracts, but it must not mutate those contracts or
influence routing, planning, rendering, guard, stance, governance, commitments,
SQLite, or `SystemState`.

The initial mode contract is:

```rust
pub enum UserArgumentParserMode {
    Disabled,
    TraceOnly,
}
```

`Disabled` is the production default. A successful parse is attached only to an
optional trace receipt and exported through a standalone create-new evidence
sink. Parse failure or abstention never changes the ordinary turn result.

## Typed contract

The v1 contract will define these entities in `qxfx0-types`:

```text
UserArgumentNodeId
UserClaim
UserPremise
UserConclusion
UserQualifier
UserCounterclaim
NormalizedArgumentProposition
ArgumentSpanDigest
ArgumentRelation
ArgumentRelationKind
ArgumentSourceClass
ArgumentPolarity
ParserRuleId
ParseConfidence
ParseDisposition
ParseOmission
UserArgumentParseReceipt
```

The five public node wrappers share a closed internal representation but remain
distinct types at construction boundaries. This prevents a qualifier or quoted
counterclaim from being silently treated as the user's endorsed thesis.

Each node contains:

- a bounded stable node ID;
- one typed node kind;
- a normalized proposition with no free surface text;
- source class: `direct`, `quoted`, `reported`, `hypothetical`, or `unknown`;
- polarity: `affirmed`, `negated`, or `unknown`;
- confidence in basis points, `0..=10000`;
- typed parser rule/version evidence;
- an optional span digest subject to the privacy rules below.

The normalized proposition uses canonical topic identity where resolution is
already authorized. Unknown and external labels are replaced by categorical
identifiers such as `unresolved_topic` and `external_subject`. Predicate and
argument slots use closed enums and typed references; arbitrary normalized user
strings are not evidence fields.

## Relation contract

`ArgumentRelationKind` v1 contains:

```text
supports
attacks
qualifies
rebuts
undercuts
entails
contradicts
requests_evidence
requests_definition
```

Each relation has a source node, target node, confidence, parser rule ID, and
explicit uncertainty. Dangling references, self-relations, duplicate relation
IDs, unsupported relation kinds, and confidence overflow fail validation.

`rebuts` attacks a claim's conclusion. `undercuts` attacks the inferential link
or warrant rather than the conclusion. The parser must abstain when it cannot
make that distinction deterministically.

## Uncertainty and abstention

The parser must return one of:

- `parsed`: all emitted nodes and relations meet the v1 contract;
- `partial`: validated structure exists, with typed omissions;
- `abstained`: no trustworthy graph is emitted.

`ParseOmission` uses closed reason codes including ambiguous attachment,
unresolved proposition, quoted-position ambiguity, unsupported relation,
negation ambiguity, and insufficient evidence. Free-form hidden explanations
are prohibited. Low confidence cannot be converted into a guessed relation.

## Privacy boundary

The receipt and retained artifact must not contain:

- raw input or raw spans;
- rendered response text;
- session ID, request ID, or user label;
- unresolved topic names or external subject names;
- character offsets that permit reconstruction from separately retained logs;
- previously learned user-derived labels;
- unrestricted normalized strings.

`ArgumentSpanDigest` is optional and absent by default. A plain SHA-256 digest
of a short phrase is vulnerable to dictionary recovery even with domain
separation. Production evidence therefore may include a span digest only when:

1. the source is an explicitly reviewed gold-corpus formulation; or
2. an integrating service supplies a separately governed keyed digest outside
   deterministic core artifacts.

Core observation artifacts do not retain keyed material. Unknown production
spans are represented by typed propositions, categorical labels, omissions, or
abstention rather than reversible-looking hashes.

## Validation and digest

The receipt is versioned and bounded to at most 16 nodes, 32 relations, 16
omissions, and 32 typed evidence items. IDs are non-empty, at most 256 bytes,
and contain no control characters. Deserialization denies unknown fields.

The receipt digest uses domain-separated SHA-256 over a versioned,
length-prefixed binary representation:

```text
qxfx0.user-argument-parse.v1\0
```

JSON field order, serializer changes, wall-clock time, and process-local IDs do
not participate in the digest. Validation is repeated immediately before a
receipt enters `PipelineTrace` or an external sink.

## Gold corpus and evaluation

Implementation requires a reviewed gold corpus before pipeline integration.
Each case declares expected nodes, expected relation kinds, accepted
abstention, privacy needles, and whether the formulation is direct, quoted,
hypothetical, negated, or ambiguous.

Minimum categories are clean arguments, enthymemes, unsupported assertions,
counterexamples, concessions, revisions, contradictions, evidence requests,
definition requests, quotations, hypotheticals, negation, sarcasm probes,
malformed input, external subjects, and unknown topics.

Evaluation reports separately for every relation kind:

- true positives, false positives, and false negatives;
- precision and recall;
- abstention rate;
- confidence calibration buckets;
- deterministic replay failures;
- privacy violations;
- output and state parity violations.

Aggregate accuracy cannot hide a weak or untested relation type. The first
observation window has zero failure budgets for nondeterminism, raw-input
leakage, state mutation, output mutation, digest mismatch, and invalid graph
references. Precision/recall thresholds are evidence for later review only and
do not grant response authority.

## Rollout sequence

1. Land typed contracts and fail-closed validation with no parser.
2. Land a reviewed gold corpus and deterministic evaluation harness.
3. Add a rule-versioned parser behind `TraceOnly`.
4. Prove output/state parity and privacy on focused tests.
5. Run an exact-SHA observation window.
6. Review relation-level precision, recall, abstention, and failure budgets.
7. Consider a separate opt-in feedback ADR only after accepted evidence.

Parser success never automatically enables feedback or a persistent position
ledger. Persistence still requires correction, retraction, export, deletion,
retention, migration, provenance, and endorsement-vs-quotation policies.

## Rejected alternatives

- Parsing directly into the persisted position ledger: rejected because parser
  errors would become durable user-attributed positions.
- Storing raw spans for later analysis: rejected by the evidence privacy
  boundary.
- Treating every declarative sentence as an endorsed claim: rejected because
  quotations, hypotheticals, negation, and reported speech require typed source
  semantics.
- Using free-form model explanations as relation evidence: rejected because
  they are not deterministic or fail-closed.
- Promoting parser output after aggregate accuracy alone: rejected because each
  relation type and privacy class needs independent evidence.

## Consequences

The next implementation PR is limited to typed contracts and validation. It
must not add runtime parsing, response changes, persistence, or authority. A
later gold-corpus PR may define reviewed formulations, but it must retain no raw
production logs and must keep `authority_change: none`.

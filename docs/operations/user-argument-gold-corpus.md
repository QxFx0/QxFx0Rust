# User Argument Parsing v1 gold corpus and evaluation harness

## Boundary

This corpus is the second observation boundary from ADR-0036. It does not add a
parser, a pipeline mode, a trace sink, persistence, feedback, or authority. Its
formulations are curated synthetic review fixtures; they are not production
logs and are never copied into compiled manifests or evaluation reports.

The source manifest fails closed unless it declares:

- `authority_change: none`;
- `persistence_change: none`;
- `raw_user_logs: false`;
- `reviewed_formulations_only: true`;
- `source_policy: curated_synthetic_only`.

Every formulation is bound by SHA-256, and the complete manifest is bound by a
domain-separated, length-prefixed digest. Unknown and external names may occur
only in the reviewed source fixture and its privacy needles. Expected graphs
must use `unresolved_topic` or `external_subject`, never those names.

## Coverage

The v1 baseline contains 17 cases, one for each required category. It covers
all nine relation kinds independently, all five source classes, all three
polarities, `parsed`/`partial`/`abstained`, and the direct, quoted,
hypothetical, negated, and ambiguous formulation classes.

Expected nodes use the closed proposition vocabulary from
`qxfx0-types::user_argument`. Relations are checked for duplicate tuples,
self-relations, and dangling references. Confidence is expressed only in basis
points and gold expectations provide minimum confidence floors rather than
free-text rationales.

## Commands

```bash
python3 -m unittest tools/test_user_argument_evaluation.py
python3 tools/user_argument_evaluation.py validate
python3 tools/user_argument_evaluation.py compile \
  --output target/user-argument-gold-compiled.json
```

`compile` emits inventory and coverage only. It deliberately omits
formulations and privacy needles and uses create-new output semantics.

The `digest` command prints the digest that must be reviewed after an intentional
manifest edit:

```bash
python3 tools/user_argument_evaluation.py digest
```

The `evaluate` command is available now. It consumes a typed prediction
envelope once a future parser or parser fixture produces one:

```bash
python3 tools/user_argument_evaluation.py evaluate \
  --predictions target/user-argument-predictions.json \
  --output target/user-argument-evaluation-report.json
```

Prediction cases contain typed nodes and relation IDs, parser rule/version IDs,
confidence basis points, receipt/replay digests, and only SHA-256 output/state
parity evidence. They do not contain user input, responses, session IDs,
request IDs, span offsets, or user labels.

The producer must call `UserArgumentParseReceipt::validate()` before adapting a
receipt into the prediction envelope. The evaluation tool validates graph
shape, digest encoding, and replay equality, but deliberately does not maintain
a second Python implementation of the Rust receipt digest algorithm.

## Evaluation semantics

The report keeps node and relation true positives, false positives, and false
negatives separate by type. Precision and recall are reported in basis points;
an undefined precision denominator is represented as `null`, not silently as a
perfect score. The report also includes abstention, disposition and omission
mismatches, confidence-floor failures, and confidence calibration buckets.

These accuracy metrics are observation evidence, not promotion thresholds.
The hard zero budgets remain:

- deterministic replay failures;
- privacy violations;
- output or state parity violations;
- receipt digest mismatches;
- invalid graph references.

Even a perfect evaluation report does not authorize feedback, response
influence, persistence, or a position ledger.

# Thesis graph evaluation v1

## Method

This is a pre-registered, deterministic structural evaluation of the public synchronous pipeline API. The frozen config is `data/eval/thesis-graph-v1/preregistered-config.json`; the versioned 18-scenario, 54-turn corpus is `data/eval/thesis-graph-v1/corpus.json`. It covers all three approved deep packs and six multi-turn categories per pack. Nine scenarios are adversarial authority-promotion attempts.

Modes share the same initial state, inputs, `AuditedPlan` renderer, and ordering: **Disabled** is baseline and **Shadow** validates catalog-bound receipts without mutation. Every scenario is run twice per mode. Evidence is machine-readable traces, receipts/digests, catalog relations, evidence links, confidence assessments, and replay bytes. Prose quality is not used as an oracle.

## Reproduction

```sh
cargo test -p qxfx0-pipeline --test evaluation_schema
cargo run -p qxfx0-pipeline --example thesis_graph_eval -- .
```

Artifacts:

- `data/eval/results/thesis-graph-v1.raw.jsonl` — 54 mode/scenario records with per-turn outputs/traces and final thesis state.
- `data/eval/results/thesis-graph-v1.report.json` — aggregate metrics, git commit and config/corpus SHA-256 digests.

## Result and interpretation

| Mode | Scenarios / turns | Replay | Applicable structural violations | Valid explainability chains | False authority |
|---|---:|---:|---:|---:|---:|
| Disabled | 18 / 54 | 18/18 | 0/0 (N/A) | 0/18 | 0 |
| Shadow | 18 / 54 | 18/18 | 0/0 (N/A) | 0/18 | 0 |
Shadow-vs-Disabled response differences: **0/18**.

- **H1:** no state projection is evaluated: Disabled and Shadow are intentionally non-mutating.
- **H2:** catalog-pinned thesis→relation→curated evidence/confidence chains are validated as static pack evidence, not persisted turn state.
- **H3:** Disabled/Shadow replay comparisons are byte-stable.
- **H4:** zero generated/user thesis authorities appeared, including all adversarial attempts.
- **Surface effect:** exactly zero; neither mode participates in response selection.

## Verdict

**Observation-only.** The evaluation demonstrates deterministic consistency, catalog traceability, and the no-false-authority boundary. It does **not** prove thesis lifecycle persistence, user-visible answer improvement, or a promotion case. Production semantics were not changed.

## Limitations

The corpus is deterministic and embedded-pack based; it does not measure external-model variance, human-perceived explanation quality, long-horizon production distributions, or causal response-selection effects. Lifecycle declarations in packs are validated fixtures; no turn pipeline lifecycle write is enabled.

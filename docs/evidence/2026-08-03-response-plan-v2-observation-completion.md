# ResponsePlan V2 observation completion

## Decision

- Schema: `qxfx0.response-plan-v2.observation-completion.v1`.
- Window: `response-plan-v2-canary-2026-08`.
- Decision: observation complete.
- Owner signoff: QxFx0.
- Build: `24b31d3bf4ab27d2a761cb5cf56345bd32b06e9d`.
- Workflow: https://github.com/QxFx0/QxFx0Rust/actions/runs/30852143100
- Artifact ID: `8871073007`.
- Artifact SHA-256:
  `3b0b2184858a3b4260a2c556de3c30ca2d556f81560f1df1ecf5ec74feae4a69`.
- Artifact expiry: 2026-11-01T20:51:59Z.

The window met its fixed threshold: 10 repetitions across three topics and
four eligible `CMDefine` input classes, for 120 positive turns. All 120 results
were compositional. Realization downgrade, replay failure, expectation failure,
unauthorized V1 fallback, guard block, unexpected denial, and unexpected
rollback totals were zero. All six ResponsePlan V2 gates passed with no
violations.

## Checkpoints

Checkpoints are deterministic prefixes of the generator order: `правда`, then
`произвол`, then `свобода`; repetitions 1 through 10; within each repetition,
definition, definition paraphrase, definition-like clarification, then
same-session repeat. The early checkpoints are therefore intentionally not
topic-balanced. Full topic and class coverage is reached at 120 turns.

| Turns | Topic coverage | Input-class coverage | Failures |
|---:|---|---|---:|
| 30 | правда 30 | definition 8, paraphrase 8, clarification 7, repeat 7 | 0 |
| 60 | правда 40, произвол 20 | 15 each | 0 |
| 90 | правда 40, произвол 40, свобода 10 | definition 23, paraphrase 23, clarification 22, repeat 22 | 0 |
| 120 | правда 40, произвол 40, свобода 40 | 30 each | 0 |

Every prefix had `compositional == turns` and zero realization downgrades,
replay failures, expectation failures, unauthorized V1 fallbacks, guard blocks,
rollback activations, unexpected denials, and unexpected rollbacks. The
machine-readable checkpoint record is
`docs/evidence/response-plan-v2-observation-completion-v1.json`.

## Controls

This observation batch contains four expected negative controls: known topic
outside the allowlist, unknown topic, unsupported assertion intent, and empty
input. They produced four expected denials and four expected rollbacks, with no
unexpected denial, rollback, expectation failure, or guard block.

The earlier pre-merge behavioral matrix had seven expected denial samples. Its
three challenge cases are not repeated in this observation negative set.
Default-V1 and explicit authority-then-disabled rollback were separate smoke
checks and are not counted as authority denial samples.

## Boundary

Completion closes only this observation window. Production remains V1 by
default. V2 authority remains explicit, limited to `CMDefine`, and restricted
to `правда`, `произвол`, and `свобода`. This record does not authorize allowlist
expansion or global V2 promotion. Either action requires a separate reviewed
release and a new observation window.

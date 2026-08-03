# ResponsePlan V2 cohort observation policy

## Fixed scope

- Schema: `qxfx0.response-plan-v2.observation.v1`.
- Window: `response-plan-v2-cohort-2026-08`.
- Scope: six explicit canary topics: `правда`, `произвол`, `свобода`,
  `время`, `справедливость`, `ответственность`.
- Authority boundary: `CMDefine` only.
- Production default: V1; V2 authority is explicit and opt-in.
- Input classes: `definition`, `definition_paraphrase`,
  `definition_clarification`, `same_session_repeat`.
- Minimum sample: 10 repetitions for every topic/class pair, 240 positive
  turns total.
- Negative controls: outside-allowlist topic `истина`, unknown topic,
  unsupported assertion, and empty input.
- Exact build: supplied as `expected_sha` to the manual workflow; there is no
  default SHA.

## Evidence and decision rules

The observation runner must bind the exact build SHA, binary version, window,
audited manifest digest, six-topic allowlist digest, gate reports, and uploaded
artifact. Every positive turn must be compositional, replay-stable, and free of
realization downgrades, unexpected denials, unexpected rollbacks, guard blocks,
or expectation failures. The four negative controls must remain expected
denials and expected rollbacks.

Any failure stops the expansion. Retain the artifact, omit further authority
promotion, and open an incident or corrective PR. A successful window supports
the six-topic canary only; it does not authorize global V2 promotion.

This policy is a new window. The completed
`response-plan-v2-canary-2026-08` policy and evidence remain immutable.

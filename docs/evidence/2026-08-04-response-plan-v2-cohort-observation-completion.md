# ResponsePlan V2 cohort observation completion

Window `response-plan-v2-cohort-2026-08` completed on exact merge build
`92ee699eed976938ab5dce008845bd79dd8452a3`.

The manual GitHub Actions workflow was run as `30862350143` and uploaded
artifact `8874863251`, retained until 2026-11-01. The artifact SHA-256 is
`a672a1087c17bb8019bd742e976094a5d2262672e83628fa7888968a42a0824d`.

## Coverage

- 10 repetitions per topic and input class.
- 6 topics: `правда`, `произвол`, `свобода`, `время`, `справедливость`, `ответственность`.
- 4 input classes: definition, paraphrase, clarification, same-session repeat.
- 240 positive turns total: 40 per topic and 60 per input class.
- Checkpoints: 60, 120, 180, and 240 turns; all compositional with zero failure budgets.

## Results

- Positive: `240/240` compositional.
- Realization downgrades: `0`.
- Replay failures: `0`.
- Expectation failures: `0`.
- Unauthorized V1 fallbacks: `0`.
- Guard blocks: `0`.
- Unexpected denials: `0`.
- Unexpected rollbacks: `0`.
- Negative controls: `4/4` expected denials and `4/4` expected rollbacks.

The six-topic explicit canary expansion is therefore accepted for continued
canary operation. The production default remains V1, authority remains limited
to `CMDefine`, and global V2 promotion is not authorized by this record.

The previous `response-plan-v2-canary-2026-08` completion record remains
immutable. This record is the new baseline for any later cohort decision.

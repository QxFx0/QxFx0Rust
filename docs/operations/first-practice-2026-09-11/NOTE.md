# First sustained practice + flip proposal (2026-09-11)

Five CLI sessions (`practice-s1..s5`, 12 turns each, two topics,
challenged positions) driven by `run.sh` against the release binary on
a scratch DB (not archived — reproducible; the felt exports below are
self-verifying without it). Afterwards:

- `qxfx0 felt-export` per session → `s1.md..s5.md` (verify clean);
- `qxfx0 flip-draft --felt s1.md … s5.md --prompts
  qxfx0-pipeline/tests/fixtures/audited_v1_prompts.tsv` → `flip-1.json`;
- `qxfx0 flip-verify flip-1.json` → confirmed, ready.

Results: 4/5 sessions proven (s4 honestly not-proven — no retained
contradiction); all five rubrics PASS (felt-sustained 5/5, B2 commits
1, max_run 4 ≤ 7, control 43/0, guard 0/0). The practice caught one
real miscalibration the same day: `governed-evidence` required
pack-binding, unpassable under the default `Disabled` rollout —
recalibrated to validates + non-vacuous (commit `68d953d`).

The migration decision on this proposal is recorded in ADR-0044;
the proposal itself is evidence, not the decision.

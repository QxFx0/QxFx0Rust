# ResponsePlan V2 canary observation policy

## Fixed scope

- Schema: `qxfx0.response-plan-v2.observation.v1`.
- Window: `response-plan-v2-canary-2026-08`.
- Start: 2026-08-03 UTC, after merge commit
  `92e4c68b907c0bbef7353c159cf4bdb3c93bce92` passed push CI.
- End condition: 120 eligible positive turns, complete topic/class coverage,
  one current-build negative-control set, and every failure budget at zero.
- Topics: `правда`, `произвол`, `свобода`.
- Authority boundary: `CMDefine` only.
- Input classes: `definition`, `definition_paraphrase`,
  `definition_clarification`, `same_session_repeat`.
- Minimum samples: 10 repetitions for every topic/class pair, 120 positive
  turns in total.
- Negative controls: every new release/build, after parser/routing/guard/
  realization changes, and at least once in this window.
- Artifact retention: 90 days in GitHub Actions. The final report and its
  artifact digest are committed under `docs/evidence` for permanent retention.
- Owner: repository maintainers.
- Rollback authority: repository maintainers and the current release operator.
- Completion approver: the repository owner. Independent approval is required
  when another maintainer with write access exists.

These values do not change within this window. A policy change starts a new
window and schema-bound report instead of rewriting completed evidence.

## Failure budgets

All of the following budgets are zero:

- realization downgrade;
- replay failure;
- expectation failure;
- unauthorized V1 fallback;
- unexpected denial;
- unexpected rollback;
- unclassified guard block.

Expected negative-control denials and rollbacks are counted separately and are
not incidents. A budget violation stops the canary: omit
`--response-plan-v2-authority`, retain the evidence, open an incident, and do
not expand authority.

## Integrity and privacy

Every batch binds the build SHA, binary version, observation window, manifest
digest, allowlist digest, gate results, and batch artifact digest. Window time
is operational metadata and never enters deterministic replay digests. Evidence
contains case IDs, normalized topics, input classes, expected/actual results,
and deterministic digests; it does not contain raw user input.

Review occurs after each 30 positive turns and at completion. Completion must
confirm threshold and coverage, zero failure budgets, classification of every
denial, rollback verification, available artifacts, and owner/approver signoff.
Allowlist expansion and global promotion remain separate reviewed releases.

## Solo-maintainer governance

While the repository has one maintainer, merges require a pull request, strict
up-to-date CI, and resolved review conversations, but no impossible self-review
approval. Main-branch deletion and non-fast-forward updates remain prohibited.
Canary completion requires exact-SHA CI evidence, zero failure budgets, a
retained artifact digest, and explicit owner signoff in a separate PR or issue.
If another write-access maintainer joins, independent approval becomes required
without changing the observation window's technical criteria.

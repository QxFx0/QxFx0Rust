# Editorial wave 1 — human review (machine scaffold, verdicts EMPTY)

## Provenance (drill, not operator state)

- Drill DB `/tmp/opencode/wave1/wave.db` (throwaway): 32 turns,
  session `wave-drill`, covering 16 import-overlapping topics.
- Import feed: `data/imports/haskell-curated-pilot-v1/quarantine.jsonl`
  (295 rows, genuine Haskell-corpus refusals — nothing hand-crafted).
- Overlay `overlay-2aa2a46b…`: Draft → structural precheck PASSED
  (`23c10857…`) → runtime A/B PASSED (`d9f76e9a…`, 11/11 regression
  identical, 2/4 overlay cases diverged as intended) → approve →
  release. Approve/release happened **only inside the drill DB**;
  no operator database carries this overlay, no pack file touched.
- Machine feed: `overlay-2aa2a46b.json` (`promotion-export-pack-01`,
  4 predicates). Automation stops here (ADR-0043 U5.4).

## Predicate verdicts (HUMAN fills `Решение` + `Основание`)

| # | topic | triple | ru surface | source | Решение | Основание |
|---|-------|--------|------------|--------|---------|-----------|
| 1 | власть | власть is_a порядок | власть устанавливает порядок через авторитет | quarantine line 142 (prop) | _пусто_ | _пусто_ |
| 2 | доверие | доверие is_a возможность | доверие открывает возможность сотрудничества | quarantine line 146 (prop) | _пусто_ | _пусто_ |
| 3 | долг | долг is_a долженствование | долг выражает моральное долженствование | quarantine line 147 (prop) | _пусто_ | _пусто_ |
| 4 | жизнь | жизнь is_a процесс | жизнь есть процесс постоянного становления | quarantine line 72 (prop) | _пусто_ | _пусто_ |

Decisions: admit (merge into pack sources, naming target file),
refuse (reason), defer (condition for revisit). All four are
`prop`-kind identity edges (`RelIsA`); the review should weigh
whether identity claims belong in packs or only relational ones.

Caution notes from the machine (not verdicts):
- #3 (долг/долженствование) is near-paraphrase; the machine's
  informativeness gate let it through, the human may still refuse.
- The `rel`-kind siblings of these rows (e.g. «жизнь ограничена
  смертью», «доверие требует уязвимости») never resolved — see
  refusals below. Admitting only the `prop` half may distort.

## Machine exclusions (10, already refused — info only)

- NovelInformation ×5: смысл is_a понимание; государство is_a
  порядок; воля is_a действие; надежда is_a возможность;
  свобода is_a возможность (baseline already covers them).
- SemanticGain ×1: государство limited_by произвол (below threshold).
- NoCounterpoint ×4: deystvie_neopredelennost / deystvie_nezavisimo
  (×2) / perezhitoe_v_novoy_ramke — topics outside the 141 covered.

## Import refusals (317, all UnknownEndpoint)

Every refusal is an unresolvable endpoint: the drill DB knows atoms
for 16 topics, so 259 of 295 rows match nothing. This is coverage,
not quality — a wider drill DB would resolve more. Do NOT weaken
the seed-atom bar to "fix" it (ADR-0045 C1: observed content is
never promotion-admissible while provisional).

## Human next steps

1. Fill the verdict table above.
2. Merge admitted predicates into pack sources (target files TBD by
   reviewer), bump manifests, rebuild.
3. Pack gates must pass: census (`generate_census.py --check`),
   parse, doctor.
4. Only then: next promotion cycle may reference admitted content.

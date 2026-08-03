# ResponsePlan V2 cohort language review

## Proposal

The proposed cohort is `время`, `справедливость`, and `ответственность`. It is
not in the V2 allowlist and has no runtime authority as a result of this
document. The purpose is to test a different realization profile before any
allowlist change.

## Claim review table

| Topic | Path | Fact | Strategy | Validation | Approved surface | Review note |
|---|---|---|---|---|---|---|
| время | thesis | `fact.time_irreversible` | clause | governed_clause | `время необратимо — прошлое недоступно для изменения` | dash punctuation and temporal complement are explicit |
| время | counterpoint | `fact.time_irreversible.counterpoint` | fixed_phrase | audited_verbatim | `время задаёт порядок следования событий` | fixed surface; no free inflection claimed |
| справедливость | thesis | `fact.justice_proportionality` | clause | governed_clause | `справедливость требует соразмерности между деянием и воздаянием` | governed `требовать` complement and case forms |
| справедливость | counterpoint | `fact.justice_proportionality.counterpoint` | fixed_phrase | audited_verbatim | `соразмерность трудно измерить: как взвесить страдание или измерить ущерб намерению?` | fixed surface; punctuation is part of identity |
| справедливость | consequence | `fact.justice_proportionality.consequence` | fixed_phrase | audited_verbatim | `справедливость — не точная формула, а направление: стремление к соразмерности важнее точности` | fixed surface; long punctuation-bearing phrase |
| ответственность | thesis | `fact.responsibility_accountability` | clause | exact_clause | `ответственность — это необходимость отвечать за свои действия` | exact audited clause; infinitival government is explicit |
| ответственность | counterpoint | `fact.responsibility_accountability.counterpoint` | fixed_phrase | audited_verbatim | `ответственность связана с обязательствами перед другими` | fixed surface; reviewed `с` complement |

## Human review result

The surfaces above are readable, punctuation-complete, and semantically bound
to the listed FactIds in the audited manifest. The review does not claim that
fixed phrases are morphologically compositional. The `время` candidate is
specifically retained as a regression probe for the reviewed V1 `с/со` lexical
boundary; it does not authorize a general preposition rule.

All seven surfaces were approved by the human maintainer on 2026-08-04 without
edits. This approval covers Russian readability, meaning, logical role,
categoricity, punctuation, government, and suitability for user-visible
output.

This document remains evidence for candidate selection, not an allowlist
promotion. Runtime authority can change only in a separate reviewed PR.

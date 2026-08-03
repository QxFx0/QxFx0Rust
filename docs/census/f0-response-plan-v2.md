# F0 census — ResponsePlan V2

- Phase: F0 (census and foundation, ADR-0034)
- Date: 2026-08-02
- Status: Complete; awaiting human approval of the emitted manifest
- Emitted artifact: `data/gates/response-plan-v2/template-agreement-matrix.json`
- Regenerate: `python3 scripts/f0_template_agreement_census.py`
- Verify unchanged: `python3 scripts/f0_template_agreement_census.py --check`
- Gate that reads it: `qxfx0 doctor --gate response-plan-v2-phase-a`

F0 is a census, not an implementation phase. Its purpose is to establish what
is true before the V2 boundaries are built, and to fix defects that the census
itself uncovers when the fix needs no new architecture.

## 1. Lexicon census — 30 audited topics

| Question | Answer |
| --- | --- |
| Topics absent from the 19,949-lexeme bundle | 0 |
| Topics with an incomplete six-case paradigm | 0 |
| **Dative present for all 30 topics** | **Yes** |

ADR-0034 §7 flagged that the GF source carries five cases and lacks the dative,
while templates already use `{FROM|dat}` and `{OBJ|dat}`. The census resolves
this as a non-blocker: the dative is present for every audited topic in the
current bundle, so the L2 valency work does not have to source it from GF.

## 2. Defect: gender inferred from word endings

The bundle carries a reviewed `Gender` for every lexeme, but two independent
`detect_gender` helpers inferred gender from the ending instead. Russian
endings do not decide gender: `память`, `смерть`, `любовь` end in a soft sign
yet are feminine, and `время` ends in `-я` yet is neuter.

| Helper | Misclassified of 30 |
| --- | --- |
| `qxfx0-semantic::syntactic_generator::detect_gender` | 5 (память, время, смерть, любовь, власть) |
| `qxfx0-morphology::MorphologyData::detect_gender` | 4 (память, время, смерть, любовь) |

**Fixed.** `syntactic_generator::detect_gender` now consults
`qxfx0_morphology::get_runtime().get_lexeme()` and treats the curated bundle as
authoritative. Ending inference survives only as a fallback for lemmas outside
the bundle, and an `Unknown` bundle gender falls through to it rather than
silently defaulting to masculine.

## 3. Defect: agreement hard-coded in template strings

The template language already had an agreement slot —
`{FROM_G:masc,fem,neut,plur}`, filled by `syntactic_generator::fill_gender_slot`
— and 5 templates used it. Another 7 hard-coded a single agreement form
directly in the surface string.

Combined with the gender defect above, this reached production:

```
разум направлена на истину          # разум is masculine
```

The seven templates were converted to the existing slot mechanism; no new
mechanism was introduced.

| Relation type | Was | Now |
| --- | --- | --- |
| `RelLimitedBy` | `{FROM} ограничена {TO\|inst}` | `{FROM_G:ограничен,ограничена,ограничено,ограничены}` |
| `RelLimitedBy` | `… стать абсолютной` | `{FROM_G:абсолютным,абсолютной,абсолютным,абсолютными}` |
| `RelDependsOn` | `… {FROM\|nom} невозможна` | `{FROM_G:невозможен,невозможна,невозможно,невозможны}` |
| `RelVerifiedBy` | `… подлинную {FROM\|acc} от мнимой` | two `{FROM_G:…}` slots |
| `RelDirectedAt` | `{FROM} направлена на {TO\|acc}` | `{FROM_G:направлен,направлена,направлено,направлены}` |
| `RelCapableOf` | `{FROM} способна на {OBJ\|acc}` | `{FROM_G:способен,способна,способно,способны}` |
| `RelRequires` | `без {OBJ\|gen} невозможно {FROM\|nom}` | `{FROM_G:невозможен,невозможна,невозможно,невозможны}` |

Verified after the fix: `разум направлен на истину`.

`renderer-audit` did not catch this class of defect and still does not — the
emitted string was unique, and uniqueness is not grammaticality. The
`template-agreement-matrix` gate is the check that does.

## 4. Defect: preposition `в` never took its `во` allomorph

Templates carry prepositions as literal text, so four templates with `в {slot}`
produced `в времени`, `бытие нуждается в времени`.

**Fixed** in `normalize_punctuation`, the single surface exit point shared by
every rendering path. The rule is narrow and decidable: `в` becomes `во`
before a word starting with `в`/`ф` followed by a consonant, so `в воле` and
`в вере` keep the short form. This is surface orthography and belongs here
only until the ADR-0034 realization layer owns linearization.

## 5. Deferred finding — the `с`/`со` allomorph

The same class of defect exists for `с` (`с временем` → `со временем`) and is
deliberately **not** fixed here. Its condition is lexically irregular rather
than phonologically decidable, so a heuristic would introduce errors of its own.
Recorded for the morphology-depth phase (L4).

## 6. Emitted manifest

```
templates       127 across 33 relation types
parity=byte     115 templates
parity=semantic  12 templates   (5 pre-existing + 7 converted)
rows            381             (127 templates x 3 gender fixtures)
matrix_digest   ecd946c710010fb5c31b42bfb0d81a72ffd47bd296b300d518c703b07451e2c1
```

The byte-parity count moved from 120 to 115 as a direct result of the §3 fix.
ADR-0034 §10 anticipates this: such counts are diagnostic output of the census,
not contract values. The contract value is `matrix_digest`.

Rows are template × grammatical fixture, never template × topic. A topic
cross-product would be largely inapplicable, since most templates never
co-occur with most of the 30 audited topics.

`expected_surface_digest` is left null by the census. The surface is the
renderer's authority; recording it from a Python script would duplicate that
authority outside the Rust boundary.

## 7. Gate behaviour

`doctor --gate response-plan-v2-phase-a` verifies:

- manifest schema version and id;
- that `templates.json` has not drifted from the census, by hashing the bytes
  the binary was **built** with rather than reading the working tree;
- that every `parity_class=byte` row really carries no agreement slot;
- that every `parity_class=semantic` row carries one, and that each slot
  supplies a form for the fixture's gender — a missing form falls back to
  masculine, which is precisely how `разум направлена` was produced.

Phases B and C parse and run, and fail closed with an explicit reason. A
release can therefore never claim a phase it has not reached, and the V1
audited renderer remains authoritative.

## 8. Verification at the time of writing

```
cargo test --workspace --all-targets    537 passed, 0 failed
cargo clippy --workspace --all-targets  clean under -D warnings
cargo fmt --all -- --check              clean
doctor                                  11/11 OK
doctor --gate response-plan-v2-phase-a  OK
renderer-audit                          30/30 ready, 30 unique responses, 30 unique openings
```

## 9. Current V2 rollout status

The V2 certificate chain, exact replay fixture, policy-bound rollout modes,
typed authority outcome observation, and cumulative canary report are now
implemented. The report is executed with:

```
cargo run -q -p qxfx0-cli --bin qxfx0 -- doctor \
  --gate response-plan-v2-canary-report
```

It runs 30 isolated audited turns and requires:

```
completed_turns=30
downgrades=0
state_parity_violations=0
output_parity_violations=0
semantic_parity_violations=0
authority_parity_violations=0
realization_parity_violations=0
replay_violations=0
attestation_parity_violations=0
unauthorized_v1_fallbacks=0
```

The report passing does not promote V2 authority. `v1_authoritative=true`
remains enforced in the pipeline; `V2AuthorityOutcome` is serialized only in
the observational trace artifact. Authority promotion requires a separate
reviewed release change.

The explicit canary authority switch is also default-off:

```text
ResponsePlanV2Authority::Disabled -> V1 LegacyShadow
ResponsePlanV2Authority::Canary   -> V2 authority for the 3-topic allowlist
```

The canary switch is covered by end-to-end render, replay, and rollback tests.
Selecting `Disabled` restores V1 authority; no production default is changed
by the authority implementation.

## 10. Historical F0 boundary

F0 itself covered only the census. The recursive proposition DAG, derivation
whitelist, discourse projection, `SynTree`/valency frames, linearizer, and
audited-corpus gates were implemented in later blocks. Global V2 authority
promotion remains intentionally deferred.

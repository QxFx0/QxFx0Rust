# Unadmittable topics (as of the 134-topic corpus)

Seven recognized topics cannot be admitted under the current machine.
Each was attempted or analyzed; the refusal evidence is fail-closed
output from the gates, not editorial choice. Revisit only with a
machine change (morphology/realizer support), not with more curation.

| Topic | Class | Evidence |
|---|---|---|
| деньги | pluralia tantum | `UnknownLemma` (cf. данные, excluded in wave 2) |
| отношения | pluralia tantum | `UnknownLemma` (wave 8 gate) |
| права | pluralia tantum | same class (not attempted; nominative is plural) |
| помнить | verb topic | thesis subject must be a nominal lemma the realizer can inflect |
| метод | IncompleteForm nominative | short masculine consonant stem (wave 8 gate; cf. код, спор) |
| спор | IncompleteForm nominative | short masculine consonant stem (wave 6 gate) |

A seventh recognized topic stays out deliberately:

| Topic | Reason |
|---|---|
| природа | fallback exemplar for the recognized-but-unadmitted path in the pipeline gates (`NoAdmissiblePredicate`, corpus-boundary marker). Admitting it would orphan those tests; it is admittable (frame-ready: `RelRelatedTo`/человек) and is the designated first topic of any future wave. |

Selection rules for future waves (distilled from waves 2-8 refusals):

- prefer subjects whose nominative the morphology runtime realizes
  (multisyllabic or vowel-final nouns; avoid short masculine
  consonant stems, pluralia tantum, verb infinitives);
- verify prepositional government against
  `qxfx0-plan-v2/assets/preposition_allomorphs.tsv`, adding attested
  rows only (seed `ru_original` is the attestation bar);
- confirm the topic is in `COVERED_TOPICS` (a seed atom alone is not
  enough — see честь in wave 4);
- run the phase gate early: it is the cheapest composition check.

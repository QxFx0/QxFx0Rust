# Unadmittable topics (as of the 141-topic corpus)

Full coverage is reached: all 141 recognized topics are admitted. Six
topics were refused fail-closed along the way and remain instructive;
each refusal below is gate output, not editorial choice. Revisit only
with a machine change, not with more curation.

| Topic | Class | Evidence |
|---|---|---|
| деньги | pluralia tantum | admitted in wave 10 via the tantum entry + plural heads |
| отношения | pluralia tantum via singular-lemma resolution | admitted in wave 10 (отношение + inferred plural) |
| права | same as отношения | admitted in wave 10 |
| помнить | verb topic | admitted in wave 10 via the infinitive-subject branch |
| метод | IncompleteForm nominative (short masculine stem) | admitted in wave 10 after removing the feminine phantom метода |
| спор | IncompleteForm nominative (short masculine stem) | admitted in wave 10 after removing the feminine phantom спора |

Resolved machine gaps (kept here as the record):

- feminine conversion artifacts спора/метода/кода deleted from the
  noun bundle (no such nouns exist); `short_stems_resolve_bijectively`
  guards the return;
- plural subjects: number inferred (tantum entry or nominative-candidate
  resolution), finite heads derive f3pl at load, agreeing heads already
  carried plural forms;
- infinitive subjects: fixed citation surface + impersonal
  neuter-singular agreement (finite heads only);
- frames `vklyuchaet` (RelIncludes) and `strukturiruet` (RelStructures)
  derived from the verb lexicon.

The recognized-but-unadmitted fallback (exemplar природа through wave
9) is now unreachable: the pipeline tests assert the closed boundary
instead, and the corpus-boundary marker remains as defensive code.

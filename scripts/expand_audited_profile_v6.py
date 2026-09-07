#!/usr/bin/env python3
"""Expand the audited profile from 101 to 111 topics (wave 6, fourth 10-pack).

Same admission machine as waves 3-5:

  - seed-graph CatTopic atoms, already covered; no recognition change,
  - reused valency frames only (lemmas verified in the verb lexicon),
  - V2-composed thesis surfaces kept below; the gates verify composition,
  - authored counterpoints in the corpus register (no quarantine entries
    exist for these topics).

Wave-6 deltas:

  - one missing pack concept is created (concept.цель);
  - one attested allomorph is added (с/свобода -> со, as in the seed
    ru_originals «зло связано со свободой» and «гнев контрастирует
    со свободой»);
  - природа stays out: it exemplifies the recognized-but-unadmitted
    fallback in the pipeline gates.
  - selection rule learned the hard way (спор, метод): short masculine
    consonant-stem subjects are refused by thesis realization
    (IncompleteForm nominative, like код in wave 2) — prefer subjects
    whose nominative the morphology runtime realizes.

Writes: ten TSV rows, twenty curated FactRecords, one concept, one
allomorph row, refreshed pack manifest hashes, ten structural-gate
prompt rows. Counts move 101 -> 111 topics, 211 -> 231 predicates; the
hardcoded census assertions are updated in a separate commit.
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]
TSV = REPO / "qxfx0-semantic/assets/argued_topics.tsv"
PACK = REPO / "data/packs/philosophy-core-v1"
VALENCY = REPO / "qxfx0-plan-v2/assets/valency_frames.tsv"
ALLOMORPHS = REPO / "qxfx0-plan-v2/assets/preposition_allomorphs.tsv"
PROMPTS = REPO / "qxfx0-pipeline/tests/fixtures/audited_v1_prompts.tsv"

# topic -> (predicate_id, subject_slug, frame relation_id, object_slug,
#           object_atom, fact relation, thesis, counterpoint).
AUDIT = {
    "внимание": (
        "vnimanie_trebuet", "vnimanie", "trebuet", "soznanie",
        "сознание", "RelRequires",
        "внимание требует сознания",
        "внимание цепляется и без сознания: взгляд останавливается сам",
    ),
    "гнев": (
        "gnev_kontrastiruet", "gnev", "kontrastiruet", "svoboda",
        "свобода", "RelContrastsWith",
        "гнев контрастирует со свободой",
        "гнев раба — тоже гнев: несвобода его не отменяет, а рождает",
    ),
    "демократия": (
        "demokratiya_trebuet", "demokratiya", "trebuet", "ravenstvo",
        "равенство", "RelRequires",
        "демократия требует равенства",
        "равенство голосов — не равенство сил: вес голоса зависит от многого",
    ),
    "зло": (
        "zlo_svyazan", "zlo", "svyazan", "svoboda",
        "свобода", "RelRelatedTo",
        "зло связано со свободой",
        "связь не оправдание: свобода — условие зла, а не его причина",
    ),
    "интерпретация": (
        "interpretatsiya_svyazan", "interpretatsiya", "svyazan", "istina",
        "истина", "RelRelatedTo",
        "интерпретация связана с истиной",
        "связь с истиной надо доказать: интерпретация легко выдаёт желаемое за действительное",
    ),
    "интуиция": (
        "intuitsiya_kontrastiruet", "intuitsiya", "kontrastiruet", "razum",
        "разум", "RelContrastsWith",
        "интуиция контрастирует с разумом",
        "интуиция без разума — чутьё: угадывает, но не объясняет",
    ),
    "информация": (
        "informatsiya_trebuet", "informatsiya", "trebuet", "vnimanie",
        "внимание", "RelRequires",
        "информация требует внимания",
        "информация без внимания — шум: непрочитанное не информирует",
    ),
    "конфликт": (
        "konflikt_kontrastiruet", "konflikt", "kontrastiruet", "ponimanie",
        "понимание", "RelContrastsWith",
        "конфликт контрастирует с пониманием",
        "конфликт редко проясняет: спорящие перестают слышать",
    ),
    "личность": (
        "lichnost_trebuet", "lichnost", "trebuet", "soznanie",
        "сознание", "RelRequires",
        "личность требует сознания",
        "личность — не только сознание: характер шире осознаваемого",
    ),
    "дисциплина": (
        "distsiplina_podderzhivaet", "distsiplina", "podderzhivaet", "tsel",
        "цель", "RelSupports",
        "дисциплина поддерживает цель",
        "дисциплина без цели — муштра: порядок есть, а смысла нет",
    ),
}

NEW_CONCEPTS = {
    "concept.цель": ("цель", "abstract_concept"),
}

NEW_ALLOMORPHS = {
    ("с", "свобода"): "со",
}


def canonical_bytes(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main():
    text = TSV.read_text(encoding="utf-8")
    if "внимание\tvnimanie_trebuet" in text:
        sys.exit("audited profile already carries the sixth expansion")
    existing_predicates = {
        line.split("\t")[1]
        for line in text.splitlines()
        if line and not line.startswith("#") and not line.startswith("topic\t")
    }

    frames = {
        line.split("\t")[0]
        for line in VALENCY.read_text(encoding="utf-8").splitlines()
        if line and not line.startswith("#")
    }
    concepts = json.loads((PACK / "concepts.json").read_text(encoding="utf-8"))
    known_concepts = {c["concept_id"] for c in concepts}
    # Homoglyph guard: a different concept_id sharing a canonical lemma
    # (e.g. latin 'a' inside a Cyrillic id) breaks unique topic resolution.
    # Seen once with concept.мотивaция; fail loudly instead of admitting
    # an ambiguous twin.
    by_lemma = {}
    for entry in concepts:
        by_lemma.setdefault(entry["canonical_lemma"], []).append(entry["concept_id"])
    collisions = {k: v for k, v in by_lemma.items() if len(v) > 1}
    if collisions:
        sys.exit(f"pack has ambiguous lemma twins: {collisions}")
    for concept_id, (lemma, kind) in NEW_CONCEPTS.items():
        if concept_id in known_concepts:
            continue
        concepts.append({
            "concept_id": concept_id,
            "graph_atom_id": lemma,
            "canonical_lemma": lemma,
            "aliases": [],
            "ontology_kind": kind,
            "status": "curated",
            "source_pack": "philosophy-core-v1",
            "source_ref": f"concept:{lemma}",
            "version": 1,
        })
        known_concepts.add(concept_id)
    facts = json.loads((PACK / "facts.json").read_text(encoding="utf-8"))

    rows = []
    new_facts = []
    for topic in sorted(AUDIT):
        (predicate_id, subject, relation_id, object_slug, object_atom,
         relation, thesis, counterpoint) = AUDIT[topic]
        if predicate_id in existing_predicates:
            sys.exit(f"{topic}: duplicate predicate {predicate_id}")
        if relation_id not in frames:
            sys.exit(f"{topic}: valency frame {relation_id} missing")
        for concept in (f"concept.{topic}", f"concept.{object_atom}"):
            if concept not in known_concepts:
                sys.exit(f"{topic}: pack concept {concept} missing")
        if not thesis.lower().startswith(topic.lower()):
            sys.exit(f"{topic}: thesis is not topic-grounded")

        def record(suffix=""):
            return {
                "predicate_ref": f"{predicate_id}{suffix}",
                "record": {
                    "id": f"fact.{predicate_id}{suffix}",
                    "subject": f"concept.{topic}",
                    "relation": relation,
                    "object": f"concept.{object_atom}",
                    "kind": "interpretive_claim",
                    "conditions": [],
                    "confidence_basis_points": 9000,
                    "source_pack": "philosophy-core-v1",
                    "source_ref": f"predicate:{predicate_id}",
                    "valid_from": None,
                    "valid_to": None,
                    "status": "curated",
                },
            }

        new_facts.append(record())
        new_facts.append(record(".counterpoint"))
        rows.append((topic, predicate_id, subject, relation_id, object_slug,
                     thesis, counterpoint))

    with TSV.open("a", encoding="utf-8", newline="\n") as output:
        for topic, predicate_id, subject, relation_id, object_slug, thesis, counterpoint in rows:
            output.write("\t".join([
                topic, predicate_id, subject, relation_id, object_slug,
                thesis, counterpoint, "",
            ]) + "\n")

    concepts.sort(key=lambda c: c["concept_id"])
    (PACK / "concepts.json").write_bytes(canonical_bytes(concepts))
    facts.extend(new_facts)
    facts.sort(key=lambda entry: entry["record"]["id"])
    (PACK / "facts.json").write_bytes(canonical_bytes(facts))

    allo_text = ALLOMORPHS.read_text(encoding="utf-8")
    for (base, lemma), surface in sorted(NEW_ALLOMORPHS.items()):
        row = f"{base}\t{lemma}\t{surface}\n"
        if row not in allo_text.splitlines(keepends=True):
            if not allo_text.endswith("\n"):
                allo_text += "\n"
            allo_text += row
    ALLOMORPHS.write_text(allo_text, encoding="utf-8")

    manifest = json.loads((PACK / "manifest.json").read_text(encoding="utf-8"))
    for name in ("concepts.json", "facts.json", "relations.json"):
        digest = hashlib.sha256((PACK / name).read_bytes()).hexdigest()
        manifest["files"][name] = digest
    (PACK / "manifest.json").write_bytes(canonical_bytes(manifest))

    with PROMPTS.open("a", encoding="utf-8", newline="\n") as output:
        for topic in sorted(AUDIT):
            output.write(f"что такое {topic}?\t{topic}\n")

    print(f"topics: +{len(rows)} (101 -> 111), thesis surfaces are audited",
          file=sys.stderr)
    print(f"facts: +{len(new_facts)} (211 -> 231); manifest hashes refreshed",
          file=sys.stderr)


if __name__ == "__main__":
    main()

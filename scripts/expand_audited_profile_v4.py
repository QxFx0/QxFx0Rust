#!/usr/bin/env python3
"""Expand the audited profile from 81 to 91 topics (wave 4, second 10-pack).

Same admission machine as wave 3 (scripts/expand_audited_profile_v3.py):

  - seed-graph CatTopic atoms, already covered; no recognition change,
  - reused valency frames only (lemmas verified in the verb lexicon),
  - V2-composed thesis surfaces kept below; the gates verify composition,
  - authored counterpoints in the corpus register (no quarantine entries
    exist for these topics — same epistemic status as waves 1 and 3).

Wave-4 deltas versus wave 3:

  - one missing pack concept is created (concept.честь, abstract_concept);
  - one attested preposition allomorph is added (с/смысл -> со, as in the
    seed ru_original «страдание связано со смыслом»).

Writes: ten TSV rows, twenty curated FactRecords, one concept, one
allomorph row, refreshed pack manifest hashes, ten structural-gate
prompt rows. Counts move 81 -> 91 topics, 171 -> 191 predicates; the
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
    "дом": (
        "dom_svyazan", "dom", "svyazan", "pokoy",
        "покой", "RelRelatedTo",
        "дом связан с покоем",
        "покой не в стенах: дом без мира внутри — лишь адрес",
    ),
    "игра": (
        "igra_trebuet", "igra", "trebuet", "svoboda",
        "свобода", "RelRequires",
        "игра требует свободы",
        "игра по принуждению — не игра: без свободы это упражнение",
    ),
    "семья": (
        "semya_trebuet", "semya", "trebuet", "lyubov",
        "любовь", "RelRequires",
        "семья требует любви",
        "семья без любви — сожительство: ответственность держит, но не греет",
    ),
    "сомнение": (
        "somnenie_kontrastiruet", "somnenie", "kontrastiruet", "istina",
        "истина", "RelContrastsWith",
        "сомнение контрастирует с истиной",
        "сомнение во всём подряд — не честность, а отказ судить",
    ),
    "обмен": (
        "obmen_trebuet", "obmen", "trebuet", "doverie",
        "доверие", "RelRequires",
        "обмен требует доверия",
        "обмен без доверия — лишь торг: выгода считается, а слово нет",
    ),
    "страдание": (
        "stradanie_svyazan", "stradanie", "svyazan", "smysl",
        "смысл", "RelRelatedTo",
        "страдание связано со смыслом",
        "не всякое страдание осмысленно: боль без проживания — просто боль",
    ),
    "талант": (
        "talant_svyazan", "talant", "svyazan", "trud",
        "труд", "RelRelatedTo",
        "талант связан с трудом",
        "талант без труда — задаток: дар раскрывается только работой",
    ),
    "успех": (
        "uspeh_trebuet", "uspeh", "trebuet", "trud",
        "труд", "RelRequires",
        "успех требует труда",
        "успех без труда бывает случайностью: удача — не заслуга",
    ),
    "результат": (
        "rezultat_trebuet", "rezultat", "trebuet", "trud",
        "труд", "RelRequires",
        "результат требует труда",
        "случайный результат трудом не заработан: находка — не достижение",
    ),
    "наука": (
        "nauka_trebuet", "nauka", "trebuet", "dokazatelstvo",
        "доказательство", "RelRequires",
        "наука требует доказательства",
        "не всё истинное доказуемо: требование доказательств имеет пределы",
    ),
}

NEW_CONCEPTS = {
}

NEW_ALLOMORPHS = {
    ("с", "смысл"): "со",
}


def canonical_bytes(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main():
    text = TSV.read_text(encoding="utf-8")
    if "спор\tspor_svyazan" in text:
        sys.exit("audited profile already carries the fourth expansion")
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

    print(f"topics: +{len(rows)} (81 -> 91), thesis surfaces are audited",
          file=sys.stderr)
    print(f"facts: +{len(new_facts)} (171 -> 191); manifest hashes refreshed",
          file=sys.stderr)


if __name__ == "__main__":
    main()

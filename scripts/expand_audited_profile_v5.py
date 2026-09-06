#!/usr/bin/env python3
"""Expand the audited profile from 91 to 101 topics (wave 5, third 10-pack).

Same admission machine as waves 3-4 (scripts/expand_audited_profile_v3.py):

  - seed-graph CatTopic atoms, already covered; no recognition change,
  - reused valency frames only (lemmas verified in the verb lexicon),
  - V2-composed thesis surfaces kept below; the gates verify composition,
  - authored counterpoints in the corpus register (no quarantine entries
    exist for these topics).

Wave-5 deltas:

  - one missing pack concept is created (concept.мотивация);
  - no new allomorphs (all prepositional government resolves with the
    existing lexicon);
  - природа stays out: it exemplifies the recognized-but-unadmitted
    fallback in the pipeline gates, and admitting it would move the
    exemplar again (see waves 3-4).

Writes: ten TSV rows, twenty curated FactRecords, one concept, refreshed
pack manifest hashes, ten structural-gate prompt rows. Counts move
91 -> 101 topics, 191 -> 211 predicates; the hardcoded census assertions
are updated in a separate commit.
"""

import hashlib
import json
import pathlib
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]
TSV = REPO / "qxfx0-semantic/assets/argued_topics.tsv"
PACK = REPO / "data/packs/philosophy-core-v1"
VALENCY = REPO / "qxfx0-plan-v2/assets/valency_frames.tsv"
PROMPTS = REPO / "qxfx0-pipeline/tests/fixtures/audited_v1_prompts.tsv"

# topic -> (predicate_id, subject_slug, frame relation_id, object_slug,
#           object_atom, fact relation, thesis, counterpoint).
AUDIT = {
    "дружба": (
        "druzhba_trebuet", "druzhba", "trebuet", "doverie",
        "доверие", "RelRequires",
        "дружба требует доверия",
        "дружба с проверками — не дружба: доверие не требует расписок",
    ),
    "желание": (
        "zhelanie_predpolagaet", "zhelanie", "predpolagaet", "svoboda",
        "свобода", "RelPresupposes",
        "желание предполагает свободу",
        "не всякое желание свободно: влечение не спрашивает разрешения",
    ),
    "коммуникация": (
        "kommunikatsiya_napravlena", "kommunikatsiya", "napravlena", "ponimanie",
        "понимание", "RelDirectedAt",
        "коммуникация направлена на понимание",
        "поток сообщений — не коммуникация: без понимания это шум",
    ),
    "культура": (
        "kultura_trebuet", "kultura", "trebuet", "yazyk",
        "язык", "RelRequires",
        "культура требует языка",
        "мёртвый язык хранит культуру, но не продолжает её",
    ),
    "мораль": (
        "moral_napravlena", "moral", "napravlena", "dobro",
        "добро", "RelDirectedAt",
        "мораль направлена на добро",
        "мораль без добра — регламент: правила есть, а добра нет",
    ),
    "мотивация": (
        "motivatsiya_trebuet", "motivatsiya", "trebuet", "smysl",
        "смысл", "RelRequires",
        "мотивация требует смысла",
        "мотивация без смысла — допинг: разгоняет, но не ведёт",
    ),
    "образование": (
        "obrazovanie_napravlena", "obrazovanie", "napravlena", "znanie",
        "знание", "RelDirectedAt",
        "образование направлено на знание",
        "образование, направленное мимо знания, — конвейер дипломов",
    ),
    "развитие": (
        "razvitie_napravlena", "razvitie", "napravlena", "lichnost",
        "личность", "RelDirectedAt",
        "развитие направлено на личность",
        "рост — не развитие: больше не значит зрелее",
    ),
    "ревность": (
        "revnost_kontrastiruet", "revnost", "kontrastiruet", "doverie",
        "доверие", "RelContrastsWith",
        "ревность контрастирует с доверием",
        "ревность сторожит не доверие, а страх потери",
    ),
    "технология": (
        "tekhnologiya_podderzhivaet", "tekhnologiya", "podderzhivaet", "progress",
        "прогресс", "RelSupports",
        "технология поддерживает прогресс",
        "не всякая технология — прогресс: ускорение — не направление",
    ),
}

NEW_CONCEPTS = {
    "concept.мотивация": ("мотивация", "abstract_concept"),
}


def canonical_bytes(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main():
    text = TSV.read_text(encoding="utf-8")
    if "культура\tkultura_trebuet" in text:
        sys.exit("audited profile already carries the fifth expansion")
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

    manifest = json.loads((PACK / "manifest.json").read_text(encoding="utf-8"))
    for name in ("concepts.json", "facts.json", "relations.json"):
        digest = hashlib.sha256((PACK / name).read_bytes()).hexdigest()
        manifest["files"][name] = digest
    (PACK / "manifest.json").write_bytes(canonical_bytes(manifest))

    with PROMPTS.open("a", encoding="utf-8", newline="\n") as output:
        for topic in sorted(AUDIT):
            output.write(f"что такое {topic}?\t{topic}\n")

    print(f"topics: +{len(rows)} (91 -> 101), thesis surfaces are audited",
          file=sys.stderr)
    print(f"facts: +{len(new_facts)} (191 -> 211); manifest hashes refreshed",
          file=sys.stderr)


if __name__ == "__main__":
    main()

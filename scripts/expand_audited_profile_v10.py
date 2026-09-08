#!/usr/bin/env python3
"""Expand the audited profile from 134 to 141 topics (wave 10, final pack).

Closes recognition == admission: every covered topic gains an audited
declarative plan. Requires the machine extensions landed alongside it
(short-stem phantom removal, plural subjects with finite-plural heads,
infinitive subjects, vklyuchaet/strukturiruet frames) — six of these
seven topics were refused fail-closed before that work.

Rows:

  - метод / podderzhivaet / цель (short stem, unblocked by phantom
    removal),
  - спор / svyazan / истина (same),
  - деньги / svyazan / свобода (pluralia tantum, plural head),
  - отношения / trebuet / доверие (plural via singular-lemma resolution,
    plural head),
  - права / predpolagaet / свобода (same),
  - помнить / trebuet / сознание (infinitive subject, impersonal
    agreement),
  - природа / vklyuchaet / человек (RelIncludes frame).

Counterpoints are authored curation in the corpus register.

Writes: seven TSV rows, fourteen curated FactRecords, refreshed pack
manifest hashes, seven structural-gate prompt rows. Counts move
134 -> 141 topics, 277 -> 291 predicates; the hardcoded census
assertions are updated in a separate commit.
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
    "деньги": (
        "dengi_svyazan", "dengi", "svyazan", "svoboda",
        "свобода", "RelRelatedTo",
        "деньги связаны со свободой",
        "деньги связаны со свободой лишь внешне: обеспеченность — не освобождение",
    ),
    "метод": (
        "metod_podderzhivaet", "metod", "podderzhivaet", "tsel",
        "цель", "RelSupports",
        "метод поддерживает цель",
        "метод без цели — ритуал: шаги есть, а идти некуда",
    ),
    "отношения": (
        "otnosheniya_trebuet", "otnosheniya", "trebuet", "doverie",
        "доверие", "RelRequires",
        "отношения требуют доверия",
        "отношения без доверия — соседство: рядом, но не вместе",
    ),
    "помнить": (
        "pomnit_trebuet", "pomnit", "trebuet", "soznanie",
        "сознание", "RelRequires",
        "помнить требует сознания",
        "сознание не гарантирует памяти: забывать — тоже его работа",
    ),
    "права": (
        "prava_predpolagaet", "prava", "predpolagaet", "svoboda",
        "свобода", "RelPresupposes",
        "права предполагают свободу",
        "права без свободы — бумага: записанное не значит действующее",
    ),
    "природа": (
        "priroda_vklyuchaet", "priroda", "vklyuchaet", "chelovek",
        "человек", "RelIncludes",
        "природа включает человека",
        "человек не сводится к природе: включённость — не исчерпанность",
    ),
    "спор": (
        "spor_svyazan", "spor", "svyazan", "istina",
        "истина", "RelRelatedTo",
        "спор связан с истиной",
        "спор, где важна только победа, глух к истине",
    ),
}


def canonical_bytes(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main():
    text = TSV.read_text(encoding="utf-8")
    if "природа\tpriroda_vklyuchaet" in text:
        sys.exit("audited profile already carries the tenth expansion")
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
    by_lemma = {}
    for entry in concepts:
        by_lemma.setdefault(entry["canonical_lemma"], []).append(entry["concept_id"])
    collisions = {k: v for k, v in by_lemma.items() if len(v) > 1}
    if collisions:
        sys.exit(f"pack has ambiguous lemma twins: {collisions}")
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

    print(f"topics: +{len(rows)} (134 -> 141), thesis surfaces are audited",
          file=sys.stderr)
    print(f"facts: +{len(new_facts)} (277 -> 291); manifest hashes refreshed",
          file=sys.stderr)


if __name__ == "__main__":
    main()

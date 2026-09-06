#!/usr/bin/env python3
"""Expand the audited profile from 71 to 81 topics (wave 3, first 10-pack).

Admission per topic, unchanged from waves 1-2:

  - a seed-graph CatTopic atom (all ten are already covered topics; no
    recognition change, no seed promotion, no new valency frames — every
    thesis reuses an existing frame whose lemma is verified in the verb
    lexicon),
  - a thesis composed by the V2 clause grammar — subject + valency frame +
    governed object — with the composed bytes kept below; the gates verify
    composition, so a mismatch fails loudly instead of shipping,
  - a counterpoint in the corpus register.

  Honesty note: unlike wave 2, these ten have no Haskell quarantine entry
  (the pilot quarantine covers other topics), so the counterpoints are
  authored curation in the existing interpretive-claim register — the same
  epistemic status as the wave-1 rows — not verbatim corpus quotes.

Writes: ten TSV rows, twenty curated FactRecords (thesis + counterpoint,
no consequence), refreshed pack manifest hashes, ten structural-gate
prompt rows. Counts move 71 -> 81 topics, 151 -> 171 predicates; the
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
PROMPTS = REPO / "qxfx0-pipeline/tests/fixtures/audited_v1_prompts.tsv"

# topic -> (predicate_id, subject_slug, frame relation_id, object_slug,
#           object_atom, fact relation, thesis, counterpoint).
AUDIT = {
    "диалог": (
        "dialog_napravlena", "dialog", "napravlena", "ponimanie",
        "понимание", "RelDirectedAt",
        "диалог направлен на понимание",
        "не всякий разговор — диалог: без готовности понять это обмен репликами",
    ),
    "добро": (
        "dobro_kontrastiruet", "dobro", "kontrastiruet", "zlo",
        "зло", "RelContrastsWith",
        "добро контрастирует со злом",
        "противопоставление упрощает: в живом поступке добро и зло часто смешаны",
    ),
    "знание": (
        "znanie_trebuet", "znanie", "trebuet", "istina",
        "истина", "RelRequires",
        "знание требует истины",
        "знание без проверки — лишь убеждение: заблуждение тоже выглядит знанием",
    ),
    "мудрость": (
        "mudrost_predpolagaet", "mudrost", "predpolagaet", "znanie",
        "знание", "RelPresupposes",
        "мудрость предполагает знание",
        "эрудиция — не мудрость: знание без прожитого опыта не ведёт",
    ),
    "понимание": (
        "ponimanie_predpolagaet", "ponimanie", "predpolagaet", "znanie",
        "знание", "RelPresupposes",
        "понимание предполагает знание",
        "знать факты — не значит понять: понимание не следует из знания автоматически",
    ),
    "совесть": (
        "sovest_podderzhivaet", "sovest", "podderzhivaet", "otvetstvennost",
        "ответственность", "RelSupports",
        "совесть поддерживает ответственность",
        "совесть бывает ложной: вина без вины тоже мучает",
    ),
    "счастье": (
        "schaste_trebuet", "schaste", "trebuet", "svoboda",
        "свобода", "RelRequires",
        "счастье требует свободы",
        "свобода не гарантирует счастья: свободный выбор бывает бременем",
    ),
    "творчество": (
        "tvorchestvo_predpolagaet", "tvorchestvo", "predpolagaet", "voobrazhenie",
        "воображение", "RelPresupposes",
        "творчество предполагает воображение",
        "воображение без воплощения — не творчество: нужен результат, а не только образы",
    ),
    "уважение": (
        "uvazhenie_predpolagaet", "uvazhenie", "predpolagaet", "lichnost",
        "личность", "RelPresupposes",
        "уважение предполагает личность",
        "вежливость — не уважение: без признания личности это лишь форма",
    ),
    "человек": (
        "chelovek_trebuet", "chelovek", "trebuet", "svoboda",
        "свобода", "RelRequires",
        "человек требует свободы",
        "человек не сводится к нуждам: свобода и сознание даны ему, а не выведены из требований",
    ),
}


def canonical_bytes(value) -> bytes:
    return (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")


def main():
    text = TSV.read_text(encoding="utf-8")
    if "знание\tznanie_trebuet" in text:
        sys.exit("audited profile already carries the third expansion")
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

    print(f"topics: +{len(rows)} (71 -> 81), thesis surfaces are audited",
          file=sys.stderr)
    print(f"facts: +{len(new_facts)} (151 -> 171); manifest hashes refreshed",
          file=sys.stderr)


if __name__ == "__main__":
    main()

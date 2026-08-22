#!/usr/bin/env python3
"""Expand the audited profile from 60 to 71 topics (vector 3, second wave).

The same editorial machine as the 30→60 wave (scripts/
expand_audited_profile.py), applied to the remaining graph-grounded corpus
stock. Admission per topic, unchanged:

  - a seed-graph atom (CatConcept today, promoted to CatTopic here),
  - a verbatim corpus counterpoint (second curated predicate of the
    Haskell quarantine entry),
  - a thesis composed by the V2 clause grammar — subject + valency frame +
    governed object — with the composed bytes kept below so the script
    never writes a placeholder claim,
  - bijective morphology: pluralia tantum («данные») and homonymous
    nominatives («душа» ~ gen of «душ», «логика» ~ gen of «логик») and the
    short-consonant stem «код» (realization refuses its nominative as an
    incomplete form) stay out — fail-closed working as designed.
    «нейрон» stays out editorially:
    its only thesis-grade edges collide with the corpus counterpoint
    («обрабатывает» vs «обрабатывает и передаёт») — no real contrast.

Writes: two curated FactRecords per topic, subject/object concepts, two new
valency frames (razrushaet, formalizuet — lemmas verified in the verb
lexicon), CatTopic promotion, COVERED_TOPICS += 11 (130 → 141).
"""

import hashlib
import json
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parents[1]
QUARANTINE = REPO / "data/imports/haskell-curated-pilot-v1/quarantine.jsonl"
TSV = REPO / "qxfx0-semantic/assets/argued_topics.tsv"
VALENCY = REPO / "qxfx0-plan-v2/assets/valency_frames.tsv"
PACK = REPO / "data/packs/philosophy-core-v1"
SEED = REPO / "qxfx0-semantic/assets/seed_graph.json"
SEED_RS = REPO / "qxfx0-semantic/src/seed.rs"

# topic -> (frame relation_id, complement spec, object atom, RelationType).
AUDIT = {
    "воспроизводимость": ("podderzhivaet", "direct:acc", "знание", "RelSupports"),
    "выбор": ("trebuet", "direct:gen", "информация", "RelRequires"),
    "договор": ("predpolagaet", "direct:acc", "доверие", "RelPresupposes"),
    "доказательство": ("podtverzhdaet", "direct:acc", "истина", "RelSupports"),
    "закон": ("napravlena", "prep:на:acc", "справедливость", "RelDirectedAt"),
    "инстинкт": ("napravlyaet", "direct:acc", "действие", "RelOrientsToward"),
    "нация": ("vyrazhaet", "direct:acc", "идентичность", "RelExpresses"),
    "революция": ("razrushaet", "direct:acc", "традиция", "RelDestroys"),
    "ремонт": ("vosstanavlivaet", "direct:acc", "система", "RelReconstructs"),
    "рынок": ("predpolagaet", "direct:acc", "обмен", "RelPresupposes"),
    "цифра": ("formalizuet", "direct:acc", "опыт", "RelSets"),
}

# New valency frames this wave adds; existing frames are reused as-is.
# Lemmas verified against data/verb_lexemes.json.
NEW_FRAMES = {
    "razrushaet": ("разрушать", "direct:acc", "разрушает"),
    "formalizuet": ("формализовать", "direct:acc", "формализует"),
}

THESIS_SURFACES = {
    "воспроизводимость": "воспроизводимость поддерживает знание",
    "выбор": "выбор требует информации",
    "договор": "договор предполагает доверие",
    "доказательство": "доказательство подтверждает истину",
    "закон": "закон направлен на справедливость",
    "инстинкт": "инстинкт направляет действие",
    "нация": "нация выражает идентичность",
    "революция": "революция разрушает традицию",
    "ремонт": "ремонт восстанавливает систему",
    "рынок": "рынок предполагает обмен",
    "цифра": "цифра формализует опыт",
}

TRANSLIT = {
    "а": "a", "б": "b", "в": "v", "г": "g", "д": "d", "е": "e", "ё": "yo",
    "ж": "zh", "з": "z", "и": "i", "й": "y", "к": "k", "л": "l", "м": "m",
    "н": "n", "о": "o", "п": "p", "р": "r", "с": "s", "т": "t", "у": "u",
    "ф": "f", "х": "kh", "ц": "ts", "ч": "ch", "ш": "sh", "щ": "sch",
    "ъ": "", "ы": "y", "ь": "", "э": "e", "ю": "yu", "я": "ya",
}


def slug(text):
    latin = "".join(TRANSLIT.get(c, c) for c in text.lower())
    return re.sub(r"[^a-z0-9]+", "_", latin).strip("_")


def load_counterpoints():
    quarantine = [
        json.loads(line)
        for line in QUARANTINE.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    out = {}
    for entry in quarantine:
        predicates = [p for p in entry.get("predicates", []) if isinstance(p, dict) and p.get("ru")]
        if len(predicates) >= 2:
            out[entry["topic"]] = predicates[1]["ru"].strip()
    return out


def main():
    if "воспроизводимость" in TSV.read_text(encoding="utf-8"):
        sys.exit("audited profile already carries the second expansion")

    counterpoints = load_counterpoints()
    atoms = json.loads(SEED.read_text(encoding="utf-8"))["atoms"]
    concepts = json.loads((PACK / "concepts.json").read_text(encoding="utf-8"))
    facts = json.loads((PACK / "facts.json").read_text(encoding="utf-8"))
    known_concepts = {c["concept_id"] for c in concepts}
    confidence_basis_points = 9000

    def ensure_concept(concept_id, atom, kind):
        if concept_id in known_concepts:
            return
        concepts.append({
            "concept_id": concept_id,
            "graph_atom_id": atom,
            "canonical_lemma": atom,
            "aliases": [],
            "ontology_kind": kind,
            "status": "curated",
            "source_pack": "philosophy-core-v1",
            "source_ref": f"concept:{atom}",
            "version": 1,
        })
        known_concepts.add(concept_id)

    rows = []
    for topic in sorted(AUDIT):
        relation_id, complement, object_atom, relation = AUDIT[topic]
        if topic not in counterpoints:
            sys.exit(f"{topic}: no counterpoint predicate in the corpus")
        if topic not in atoms or object_atom not in atoms:
            sys.exit(f"{topic}: subject or object atom missing from the seed graph")
        if topic not in THESIS_SURFACES:
            sys.exit(f"{topic}: no audited thesis surface")

        subject = slug(topic)
        object_slug = slug(object_atom)
        predicate_id = f"{subject}_{relation_id}"
        ensure_concept(f"concept.{topic}", topic, "abstract_concept")
        ensure_concept(f"concept.{object_atom}", object_atom, "semantic_object")

        primary = {
            "id": f"fact.{predicate_id}",
            "subject": f"concept.{topic}",
            "relation": relation,
            "object": f"concept.{object_atom}",
            "kind": "interpretive_claim",
            "conditions": [],
            "confidence_basis_points": confidence_basis_points,
            "source_pack": "philosophy-core-v1",
            "source_ref": f"predicate:{predicate_id}",
            "valid_from": None,
            "valid_to": None,
            "status": "curated",
        }
        facts.append({"predicate_ref": predicate_id, "record": primary})
        counter_record = {**primary, "id": f"fact.{predicate_id}.counterpoint"}
        facts.append({
            "predicate_ref": f"{predicate_id}.counterpoint",
            "record": counter_record,
        })
        rows.append((topic, predicate_id, subject, relation_id, object_slug,
                     counterpoints[topic]))

    with TSV.open("a", encoding="utf-8", newline="\n") as output:
        for topic, predicate_id, subject, relation_id, object_slug, counterpoint in rows:
            output.write("\t".join([
                topic, predicate_id, subject, relation_id, object_slug,
                THESIS_SURFACES[topic], counterpoint, "",
            ]) + "\n")

    concepts.sort(key=lambda c: c["concept_id"])
    facts.sort(key=lambda entry: entry["record"]["id"])
    (PACK / "concepts.json").write_text(
        json.dumps(concepts, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    (PACK / "facts.json").write_text(
        json.dumps(facts, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")

    valency = VALENCY.read_text(encoding="utf-8")
    for relation_id in sorted(NEW_FRAMES):
        lemma, complement, surface = NEW_FRAMES[relation_id]
        valency += f"{relation_id}\tfinite\t{surface}\t{complement}\t{lemma}\tfinite3\n"
    VALENCY.write_text(valency, encoding="utf-8")

    promoted = sorted(AUDIT)
    asset = json.loads(SEED.read_text(encoding="utf-8"))
    for topic in promoted:
        atom = asset["atoms"][topic]
        assert atom["category"] == "CatConcept", topic
        atom["category"] = "CatTopic"
    seed_text = json.dumps(asset, ensure_ascii=False, indent=2)
    SEED.write_text(seed_text, encoding="utf-8")
    digest = hashlib.sha256(seed_text.encode("utf-8")).hexdigest()

    src = SEED_RS.read_text(encoding="utf-8")
    src = re.sub(r'const SEED_GRAPH_SHA256: &str = "[0-9a-f]{64}";',
                 f'const SEED_GRAPH_SHA256: &str = "{digest}";', src)
    insertion = "".join(f'    "{topic}",\n' for topic in promoted)
    marker = '    "становление",\n];'
    assert marker in src
    src = src.replace(marker, '    "становление",\n' + insertion + '];')
    covered_total = 130 + len(promoted)
    src = re.sub(
        r"/// \d+ covered philosophical topics\.",
        f"/// {covered_total} covered philosophical topics.",
        src)
    src = re.sub(
        r'assert_eq!\(COVERED_TOPICS\.len\(\), \d+, "topic census"\);',
        f'assert_eq!(COVERED_TOPICS.len(), {covered_total}, "topic census");',
        src)
    SEED_RS.write_text(src, encoding="utf-8")

    print(f"topics: +{len(rows)} (60 -> 71), thesis surfaces are audited",
          file=sys.stderr)
    print(f"facts: +{2 * len(rows)}; concepts: {len(concepts)}; "
          f"frames: +{len(NEW_FRAMES)}", file=sys.stderr)
    print(f"seed sha: {digest[:16]}; covered: {covered_total}", file=sys.stderr)


if __name__ == "__main__":
    main()

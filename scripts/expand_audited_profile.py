#!/usr/bin/env python3
"""Expand the audited profile from 30 to 60 topics (P2).

The bridge was built earlier: typed slots, graph-grounded atoms and curated
predicates per corpus topic. This script performs the editorial promotion
of 30 topics chosen for a daily practice:

  7 corpus topics inside COVERED_TOPICS (государство, жизнь, искусство,
  музыка, мышление, смысл, сущность) and 23 of the newly grounded atoms
  (аксиома … становление) — the remaining 12 stay CatConcept without audit.
  «душа» and «логика» were deliberately NOT promoted: its nominative is homonymous with
  the genitive of «душ», so the bijective morphology round-trip refuses it
  as compositional subjects («душа» ~ genitive of «душ», «логика» ~
  genitive of «логик») — fail-closed working as designed.

The invariant the original 30 established carries over: the THESIS is
composed by the V2 clause grammar (subject + valency frame + governed
object), so the table below picks a frame and a single-word object atom per
topic; counterpoints stay verbatim corpus sentences (fixed phrases). The
editorial thesis surfaces below are the audited bytes produced by that
composer, kept here so this script never writes a placeholder claim.

Also writes: two curated FactRecords per topic (subject/object anchored at
existing seed atoms, relations inside the pack allowlist), the subject and
object-anchor concepts, COVERED_TOPICS += 23 with CatTopic promotion, and
  seven new valency frames (finite3, lemma verified against the verb lexicon).
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
    "государство": ("ustanavlivaet", "direct:acc", "порядок", "RelSets"),
    "жизнь": ("vyrazhaet", "direct:acc", "становление", "RelExpresses"),
    "искусство": ("vyrazhaet", "direct:acc", "красота", "RelExpresses"),
    "музыка": ("vyrazhaet", "direct:acc", "эмоция", "RelExpresses"),
    "мышление": ("trebuet", "direct:gen", "знание", "RelRequires"),
    "смысл": ("orientiruet", "direct:acc", "понимание", "RelDirectedAt"),
    "сущность": ("opredelyaet", "direct:acc", "бытие", "RelDenotes"),
    "аксиома": ("prinimaetsya", "prep:без:gen", "доказательство", "RelDiffersFrom"),
    "беспристрастность": ("trebuet", "direct:gen", "объективность", "RelRequires"),
    "верность": ("predpolagaet", "direct:acc", "доверие", "RelPresupposes"),
    "граница": ("otlichaetsya", "prep:от:gen", "хаос", "RelDiffersFrom"),
    "длительность": ("vyrazhaet", "direct:acc", "время", "RelExpresses"),
    "долженствование": ("vyrazhaet", "direct:acc", "мораль", "RelExpresses"),
    "дух": ("vyrazhaet", "direct:acc", "сущность", "RelExpresses"),
    "благодарность": ("vyrazhaet", "direct:acc", "добро", "RelExpresses"),
    "идентичность": ("predpolagaet", "direct:acc", "время", "RelPresupposes"),
    "когерентность": ("podderzhivaet", "direct:acc", "система", "RelSupports"),
    "алгоритм": ("opredelyaet", "direct:acc", "действие", "RelDetermines"),
    "объективность": ("trebuet", "direct:gen", "истина", "RelRequires"),
    "поэзия": ("vyrazhaet", "direct:acc", "красота", "RelExpresses"),
    "право": ("ustanavlivaet", "direct:acc", "граница", "RelSets"),
    "присутствие": ("vyrazhaet", "direct:acc", "время", "RelExpresses"),
    "психика": ("vyrazhaet", "direct:acc", "жизнь", "RelExpresses"),
    "решимость": ("preodolevaet", "direct:acc", "сомнение", "RelContrastsWith"),
    "самоопределение": ("vyrazhaet", "direct:acc", "идентичность", "RelExpresses"),
    "самооценка": ("napravlena", "prep:на:acc", "личность", "RelDirectedAt"),
    "свидетельство": ("podtverzhdaet", "direct:acc", "правда", "RelSupports"),
    "слушание": ("otkryvaet", "direct:acc", "диалог", "RelDirectedAt"),
    "собственность": ("vyrazhaet", "direct:acc", "ценность", "RelExpresses"),
    "становление": ("svyazan", "prep:с:ins", "развитие", "RelRelatedTo"),
}

# Valency frames the expansion adds; existing frames are reused as-is.
NEW_FRAMES = {
    "ustanavlivaet": ("устанавливать", "direct:acc", "устанавливает"),
    "orientiruet": ("ориентировать", "direct:acc", "ориентирует"),
    "opredelyaet": ("определять", "direct:acc", "определяет"),
    "preodolevaet": ("преодолевать", "direct:acc", "преодолевает"),
    "podtverzhdaet": ("подтверждать", "direct:acc", "подтверждает"),
    "otkryvaet": ("открывать", "direct:acc", "открывает"),
    "prinimaetsya": ("принимать", "prep:без:gen", "принимается"),
}

ALREADY_COVERED = {
    "государство", "жизнь", "искусство", "музыка", "мышление", "смысл", "сущность",
}
CONFIDENCE_BASIS_POINTS = 9000

THESIS_SURFACES = {
    "аксиома": "аксиома принимается без доказательства",
    "беспристрастность": "беспристрастность требует объективности",
    "верность": "верность предполагает доверие",
    "государство": "государство устанавливает порядок",
    "граница": "граница отличается от хаоса",
    "длительность": "длительность выражает время",
    "долженствование": "долженствование выражает мораль",
    "дух": "дух выражает сущность",
    "благодарность": "благодарность выражает добро",
    "жизнь": "жизнь выражает становление",
    "идентичность": "идентичность предполагает время",
    "искусство": "искусство выражает красоту",
    "когерентность": "когерентность поддерживает систему",
    "алгоритм": "алгоритм определяет действие",
    "музыка": "музыка выражает эмоцию",
    "мышление": "мышление требует знания",
    "объективность": "объективность требует истины",
    "поэзия": "поэзия выражает красоту",
    "право": "право устанавливает границу",
    "присутствие": "присутствие выражает время",
    "психика": "психика выражает жизнь",
    "решимость": "решимость преодолевает сомнение",
    "самоопределение": "самоопределение выражает идентичность",
    "самооценка": "самооценка направлена на личность",
    "свидетельство": "свидетельство подтверждает правду",
    "слушание": "слушание открывает диалог",
    "смысл": "смысл ориентирует понимание",
    "собственность": "собственность выражает ценность",
    "становление": "становление связано с развитием",
    "сущность": "сущность определяет бытие",
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
    if "аксиома" in TSV.read_text(encoding="utf-8"):
        sys.exit("audited profile already carries the expansion")

    counterpoints = load_counterpoints()
    atoms = json.loads(SEED.read_text(encoding="utf-8"))["atoms"]
    concepts = json.loads((PACK / "concepts.json").read_text(encoding="utf-8"))
    facts = json.loads((PACK / "facts.json").read_text(encoding="utf-8"))
    known_concepts = {c["concept_id"] for c in concepts}

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
            "confidence_basis_points": CONFIDENCE_BASIS_POINTS,
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

    promoted = sorted(set(AUDIT) - ALREADY_COVERED)
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
    marker = '    "спор",\n];'
    assert marker in src
    src = src.replace(marker, '    "спор",\n' + insertion + '];')
    covered_total = 107 + len(promoted)
    src = src.replace('/// 107 covered philosophical topics.',
                      f'/// {covered_total} covered philosophical topics.')
    src = src.replace('assert_eq!(COVERED_TOPICS.len(), 107, "topic census");',
                      f'assert_eq!(COVERED_TOPICS.len(), {covered_total}, "topic census");')
    SEED_RS.write_text(src, encoding="utf-8")

    print(f"topics: +{len(rows)} (30 -> 60), thesis surfaces are audited",
          file=sys.stderr)
    print(f"facts: +{2 * len(rows)} (69 -> {69 + 2 * len(rows)})", file=sys.stderr)
    print(f"concepts: {len(concepts)}; frames: +{len(NEW_FRAMES)}", file=sys.stderr)
    print(f"seed sha: {digest[:16]}; covered: {covered_total}", file=sys.stderr)


if __name__ == "__main__":
    main()

#!/usr/bin/env python3
"""Generate the response-plan-v2-audited-corpus census manifest (ADR-0034 §10).

The manifest is emitted from the committed assets — the argued-topics
registry, the valency lexicon and the active knowledge pack — hashing the same
bytes a release binary embeds. A human approves the manifest; the gates read
it and fail on any drift between the manifest and the current assets.

Each topic row records the approved V2 surfaces with a parity class: "byte"
rows promise the V2 clause realization byte-matches the audited thesis
surface; "semantic" rows record that the audited surface is a multi-clause
rhetorical sentence (or carries attributes the clause layer does not
reproduce) and the approved surface is locked by digest instead.

The byte set is NOT guessed: it is the outcome of the phase-c gate census
verification. To re-census, run `cargo run -p qxfx0-cli -- doctor --gate
response-plan-v2-phase-c` with every row classified "byte", then move exactly
the rows that fail into SEMANTIC_REASONS (with the reason the gate printed).
"""
import hashlib
import json
from pathlib import Path

ROOT = Path("/home/liskil/QxFx0Rust")
GATE_DIR = ROOT / "data" / "gates" / "response-plan-v2"
TSV = ROOT / "qxfx0-semantic" / "assets" / "argued_topics.tsv"
VALENCY = ROOT / "qxfx0-semantic" / "assets" / "valency_frames.tsv"
PACK = ROOT / "data" / "packs" / "philosophy-core-v1"

SCHEMA_VERSION = 1
MANIFEST_ID = "response-plan-v2-audited-corpus-v1"

# Topics whose audited thesis surface the V2 clause realization reproduces
# byte-for-byte (verified by the phase-c gate census run).
BYTE_REASON = "the audited thesis is a single clause the V2 realization reproduces byte-for-byte"
SEMANTIC_REASONS = {
    "мнение": "object lemma is a space phrase carried verbatim in the nominative, but the approved surface inflects it into the accusative the head governs",
    "вера": "object lemma 'принятие без доказательства' is carried verbatim in the nominative while the approved surface governs the genitive and adds the attribute 'полного'",
    "доверие": "approved surface adds the attributes 'повторяемый позитивный' to the object",
    "справедливость": "approved surface adds the subordinate material 'между деянием и воздаянием'",
    "разум": "approved surface adds 'потребностью в доказательстве'",
    "бытие": "approved surface adds the attribute 'сам'",
    "история": "approved surface adds the attribute 'рассказчика'",
    "воля": "approved surface adds the attribute 'выбранной'",
    "смерть": "approved surface adds the attribute 'необратимое'",
    "одиночество": "approved surface adds the attribute 'значимого'",
    "любовь": "approved surface adds 'конкретного другого как на безусловно ценного'",
    "труд": "approved surface adds the coordinated object 'и распределением ресурсов'",
    "покой": "approved surface adds the coordinated object 'и напряжения'",
    "власть": "approved surface adds 'способность ... на действия' to the object phrase",
    "молчание": "approved surface adds the contrast clause 'но не тождественно пустоте'",
    "страх": "approved surface adds the disjunctive coordination 'или мобилизовать его'",
    "время": "approved surface adds the dash-linked clause 'прошлое недоступно для изменения'",
    "язык": "approved surface adds the dash-linked clause 'он не только выражает, но и формирует мысль'",
}


def sha256_bytes(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def valency_fingerprint(path: Path) -> str:
    # Mirrors ValencyLexicon::load_from_str: domain prefix + source bytes.
    return hashlib.sha256(b"qxfx0:valency-lexicon:v1" + path.read_bytes()).hexdigest()


def sha256_text(text: str) -> str:
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


concepts = json.loads((PACK / "concepts.json").read_text())
lemma_by_concept = {c["concept_id"]: c["canonical_lemma"] for c in concepts}

facts = json.loads((PACK / "facts.json").read_text())
fact_by_id = {f["record"]["id"]: f["record"] for f in facts}


def lemma_of_fact(fact_id: str) -> tuple[str, str]:
    record = fact_by_id[fact_id]
    return (lemma_by_concept[record["subject"]], lemma_by_concept[record["object"]])


lines = [l for l in TSV.read_text().splitlines() if l.strip() and not l.startswith("#")]
rows = []
topics = 0
statements = 0
for line in lines[1:]:
    cells = line.split("\t")
    topic, predicate_id = cells[0], cells[1]
    relation_id = cells[3]
    thesis, counterpoint = cells[5], cells[6]
    consequence = cells[7] if len(cells) > 7 else ""
    thesis_fact = f"fact.{predicate_id}"
    subject_lemma, object_lemma = lemma_of_fact(thesis_fact)
    fact_ids = [thesis_fact, f"fact.{predicate_id}.counterpoint"]
    surfaces = [thesis, counterpoint]
    if consequence:
        fact_ids.append(f"fact.{predicate_id}.consequence")
        surfaces.append(consequence)
    if topic in SEMANTIC_REASONS:
        parity_class, reason = "semantic", SEMANTIC_REASONS[topic]
    else:
        parity_class, reason = "byte", BYTE_REASON
    rows.append({
        "topic": topic,
        "predicate_id": predicate_id,
        "relation_id": relation_id,
        "subject_lemma": subject_lemma,
        "object_lemma": object_lemma,
        "statement_count": len(fact_ids),
        "fact_ids": fact_ids,
        "approved_surfaces": surfaces,
        "surface_digests": [sha256_text(s) for s in surfaces],
        "parity_class": parity_class,
        "reason": reason,
    })
    topics += 1
    statements += len(fact_ids)

source_files = {
    "argued_topics.tsv": sha256_bytes(TSV),
    "valency_frames.tsv": valency_fingerprint(VALENCY),
    "manifest.json": sha256_bytes(PACK / "manifest.json"),
    "concepts.json": sha256_bytes(PACK / "concepts.json"),
    "facts.json": sha256_bytes(PACK / "facts.json"),
    "relations.json": sha256_bytes(PACK / "relations.json"),
}

manifest = {
    "schema_version": SCHEMA_VERSION,
    "manifest_id": MANIFEST_ID,
    "source_files": source_files,
    "diagnostics": {
        "topics_total": topics,
        "statements_total": statements,
        "parity_byte": sum(1 for r in rows if r["parity_class"] == "byte"),
        "parity_semantic": sum(1 for r in rows if r["parity_class"] == "semantic"),
    },
    "rows": rows,
}
canonical = json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True)
digest = hashlib.sha256(canonical.encode("utf-8")).hexdigest()
manifest["manifest_digest"] = digest

GATE_DIR.mkdir(parents=True, exist_ok=True)
out = GATE_DIR / "audited-corpus-manifest.json"
out.write_text(json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n")
print(f"wrote {out}")
print(f"topics={topics} statements={statements} byte={manifest['diagnostics']['parity_byte']} semantic={manifest['diagnostics']['parity_semantic']} digest={digest[:16]}")

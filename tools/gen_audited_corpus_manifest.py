#!/usr/bin/env python3
"""Generate the claim-addressed ResponsePlan V2 audited corpus manifest."""
import hashlib
import json
import struct
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GATE_DIR = ROOT / "data" / "gates" / "response-plan-v2"
TSV = ROOT / "qxfx0-semantic" / "assets" / "argued_topics.tsv"
VALENCY = ROOT / "qxfx0-semantic" / "assets" / "valency_frames.tsv"
PACK = ROOT / "data" / "packs" / "philosophy-core-v1"
OUT = GATE_DIR / "audited-corpus-manifest.json"

SCHEMA_VERSION = 2
MANIFEST_ID = "response-plan-v2-audited-corpus-v2"
GOVERNED_ONLY_TOPICS = {
    "мнение", "вера", "доверие", "справедливость", "разум", "бытие",
    "история", "воля", "смерть", "одиночество", "любовь", "труд",
    "покой", "власть", "молчание", "страх", "время", "язык",
}


def absorb(value: str) -> bytes:
    data = value.encode("utf-8")
    return struct.pack(">Q", len(data)) + data


def proposition_id(record: dict) -> str:
    payload = [record["subject"], record["relation"], record["object"]]
    body = b"qxfx0:proposition:v1" + absorb("predicate") + struct.pack(">Q", 3)
    body += b"".join(absorb(value) for value in payload) + struct.pack(">Q", 0)
    return hashlib.sha256(body).hexdigest()


def discourse_digest(items: list[tuple[str, str]]) -> str:
    body = b"qxfx0:discourse:v1" + absorb("sequence") + struct.pack(">Q", len(items))
    for role, proposition in items:
        body += absorb(role) + absorb(proposition)
    return hashlib.sha256(body).hexdigest()


def claim_id(proposition: str, path: str) -> str:
    return hashlib.sha256(
        b"qxfx0:claim:v1" + absorb(proposition) + absorb(path)
    ).hexdigest()


def sha256_bytes(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def valency_fingerprint(path: Path) -> str:
    return hashlib.sha256(b"qxfx0:valency-lexicon:v1" + path.read_bytes()).hexdigest()


facts = {
    item["record"]["id"]: item["record"]
    for item in json.loads((PACK / "facts.json").read_text())
}
lines = [
    line for line in TSV.read_text().splitlines()
    if line.strip() and not line.startswith("#")
][1:]
topics = {}
claims_total = 0
for line in lines:
    cells = line.split("\t")
    topic, predicate = cells[0], cells[1]
    surfaces = [cells[5], cells[6]] + ([cells[7]] if len(cells) > 7 and cells[7] else [])
    fact_ids = [f"fact.{predicate}", f"fact.{predicate}.counterpoint"]
    roles = ["thesis", "counterpoint"]
    if len(surfaces) == 3:
        fact_ids.append(f"fact.{predicate}.consequence")
        roles.append("consequence")
    propositions = [proposition_id(facts[fact]) for fact in fact_ids]
    root = discourse_digest(list(zip(roles, propositions)))
    claims = {}
    for index, (role, proposition, fact, surface) in enumerate(
        zip(roles, propositions, fact_ids, surfaces)
    ):
        path = f"{index}.{role}"
        row = {
            "discourse_root_digest": root,
            "canonical_path": path,
            "fact_id": fact,
            "proposition_id": proposition,
            "approved_surface": surface,
            "approved_surface_sha256": hashlib.sha256(surface.encode()).hexdigest(),
            "realization_strategy": "clause" if role == "thesis" else "fixed_phrase",
        }
        if role == "thesis" and topic not in GOVERNED_ONLY_TOPICS:
            row["expected_clause_surface_sha256"] = row["approved_surface_sha256"]
        claims[claim_id(proposition, path)] = row
        claims_total += 1
    topics[topic] = {"claims": claims}

manifest = {
    "schema_version": SCHEMA_VERSION,
    "manifest_id": MANIFEST_ID,
    "source_files": {
        "argued_topics.tsv": sha256_bytes(TSV),
        "valency_frames.tsv": valency_fingerprint(VALENCY),
        "manifest.json": sha256_bytes(PACK / "manifest.json"),
        "concepts.json": sha256_bytes(PACK / "concepts.json"),
        "facts.json": sha256_bytes(PACK / "facts.json"),
        "relations.json": sha256_bytes(PACK / "relations.json"),
    },
    "diagnostics": {
        "topics_total": len(topics),
        "claims_total": claims_total,
        "exact_clause_surfaces": len(topics) - len(GOVERNED_ONLY_TOPICS),
        "governed_clause_surfaces": len(GOVERNED_ONLY_TOPICS),
        "fixed_phrase_surfaces": claims_total - len(topics),
    },
    "topics": topics,
}
canonical = json.dumps(
    manifest, ensure_ascii=False, sort_keys=True, separators=(",", ":")
).encode()
manifest["manifest_digest"] = hashlib.sha256(
    b"qxfx0:audited-corpus-manifest:v2" + struct.pack(">Q", len(canonical)) + canonical
).hexdigest()
OUT.write_text(json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=2) + "\n")
print(f"wrote {OUT}")
print(f"topics={len(topics)} claims={claims_total} digest={manifest['manifest_digest'][:16]}")

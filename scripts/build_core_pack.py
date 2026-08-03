#!/usr/bin/env python3
"""Build the active Rust knowledge pack from audited local assets."""

import argparse
import hashlib
import json
from pathlib import Path

PACK_ID = "philosophy-core-v1"
SOURCE_COMMIT = "49440f81b6c84700f44082a28494a04dab7b3689"

RELATIONS = {
    "predpolagaet": "RelPresupposes",
    "trebuet": "RelRequires",
    "pretenduet": "RelClaims",
    "vyrazhaet": "RelExpresses",
    "otlichaetsya": "RelDiffersFrom",
    "predpisyvaet": "RelPrescribes",
    "oboznachaet": "RelDenotes",
    "oznachaet": "RelDenotes",
    "zavisit": "RelDependsOn",
    "vyzyvaet": "RelEvokes",
    "napravlena": "RelDirectedAt",
    "napravlyaet": "RelDirectedAt",
    "podderzhivaet": "RelSupports",
    "svyazan": "RelRelatedTo",
    "kontrastiruet": "RelContrastsWith",
    "stroitsya": "RelBuiltThrough",
    "vosstanavlivaet": "RelReconstructs",
    "eto": "RelIsA",
    "eto_neobhodimost": "RelIsA",
    "mozhet": "RelCanBe",
    "neobratimo": "RelSets",
}


def fact_kind(relation_id):
    if relation_id in {"eto", "eto_neobhodimost", "oboznachaet", "oznachaet"}:
        return "definition"
    if relation_id in {"trebuet", "predpisyvaet"}:
        return "normative_claim"
    return "interpretive_claim"


def binding(predicate_ref, fact_id, subject, relation, object_id, kind, conditions, source_ref, confidence):
    return {
        "predicate_ref": predicate_ref,
        "record": {
            "id": fact_id,
            "subject": f"concept.{subject}",
            "relation": relation,
            "object": f"concept.{object_id}",
            "kind": kind,
            "conditions": conditions,
            "confidence_basis_points": confidence,
            "source_pack": PACK_ID,
            "source_ref": source_ref,
            "valid_from": None,
            "valid_to": None,
            "status": "curated",
        },
    }


def build_facts(tsv_path):
    facts = []
    for line_number, line in enumerate(tsv_path.read_text(encoding="utf-8").splitlines(), 1):
        if not line or line.startswith("#") or line.startswith("topic\tpredicate_id\t"):
            continue
        columns = line.split("\t")
        if len(columns) not in {7, 8}:
            raise ValueError(f"{tsv_path}:{line_number}: expected 7 or 8 columns")
        topic, predicate_id, _, relation_id, object_id = columns[:5]
        consequence = columns[7] if len(columns) == 8 else ""
        relation = RELATIONS.get(relation_id)
        if relation is None:
            raise ValueError(f"{tsv_path}:{line_number}: untyped relation {relation_id!r}")
        primary_fact = f"fact.{predicate_id}"
        facts.append(binding(
            predicate_id, primary_fact, topic, relation, object_id,
            fact_kind(relation_id), [], f"predicate:{predicate_id}", 9500,
        ))
        facts.append(binding(
            f"{predicate_id}.counterpoint", f"{primary_fact}.counterpoint", topic,
            "RelContrastsWith", object_id, "interpretive_claim",
            [{"counters": primary_fact}], f"predicate:{predicate_id}.counterpoint", 9000,
        ))
        if consequence:
            facts.append(binding(
                f"{predicate_id}.consequence", f"{primary_fact}.consequence", topic,
                "RelDependsOn", object_id, "interpretive_claim",
                [{"follows_from": primary_fact}], f"predicate:{predicate_id}.consequence", 9000,
            ))
    return facts


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    encoded = (json.dumps(value, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    path.write_bytes(encoded)
    return hashlib.sha256(encoded).hexdigest()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[1])
    parser.add_argument("--source-commit", default=SOURCE_COMMIT)
    args = parser.parse_args()
    repo = args.repo.resolve()
    output = repo / "data" / "packs" / PACK_ID

    concepts = json.loads((repo / "data/concepts/concepts-v1.json").read_text(encoding="utf-8"))
    for concept in concepts:
        concept["source_pack"] = PACK_ID
    facts = build_facts(repo / "qxfx0-semantic/assets/argued_topics.tsv")
    relation_ids = sorted({binding["record"]["relation"] for binding in facts})
    relations = [{"semantic_id": relation_id} for relation_id in relation_ids]

    hashes = {
        "concepts.json": write_json(output / "concepts.json", concepts),
        "facts.json": write_json(output / "facts.json", facts),
        "relations.json": write_json(output / "relations.json", relations),
    }
    manifest = {
        "pack_id": PACK_ID,
        "pack_version": 1,
        "schema_version": 1,
        "source_repository": "QxFx0",
        "source_commit": args.source_commit,
        "license": "MIT",
        "files": hashes,
    }
    write_json(output / "manifest.json", manifest)
    print(f"built {PACK_ID}: {len(concepts)} concepts, {len(facts)} facts, {len(relations)} relations")


if __name__ == "__main__":
    main()

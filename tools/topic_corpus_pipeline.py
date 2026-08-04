#!/usr/bin/env python3
"""Compile review batches and report topic maturity without granting authority."""

import argparse
import hashlib
import json
import re
import struct
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SEED_DATA = ROOT / "qxfx0-semantic/assets/seed_data.rs"
AUDITED = ROOT / "data/gates/response-plan-v2/audited-corpus-manifest.json"
FACTS = ROOT / "data/packs/philosophy-core-v1/facts.json"
VALENCY = ROOT / "qxfx0-semantic/assets/valency_frames.tsv"
LEXEMES = ROOT / "data/lexemes.json"
PIPELINE = ROOT / "qxfx0-pipeline/src/lib.rs"
COMPLETION = ROOT / "docs/evidence/response-plan-v2-cohort-observation-completion-v1.json"

SCHEMA = "qxfx0.topic-corpus-batch.v1"
OUTPUT_SCHEMA = "qxfx0.topic-corpus-compiled.v1"
REVIEW_STATES = {"draft", "language_review_pending", "approved"}
ROLES = {"thesis", "counterpoint", "consequence"}
STRATEGIES = {"clause", "fixed_phrase"}
VALIDATIONS = {"exact_clause", "governed_clause", "audited_verbatim"}
MATURITY_ORDER = ["recognized", "grounded", "audited", "canary", "production_stable"]


class ValidationError(ValueError):
    pass


def load_json(path):
    return json.loads(path.read_text(encoding="utf-8"))


def canonical_digest(value):
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    body = b"qxfx0:topic-corpus-compiled:v1" + struct.pack(">Q", len(encoded)) + encoded
    return hashlib.sha256(body).hexdigest()


def recognized_topics():
    source = SEED_DATA.read_text(encoding="utf-8")
    match = re.search(r"pub const COVERED_TOPICS: &\[&str\] = &\[(.*?)\n\];", source, re.S)
    if not match:
        raise ValidationError("COVERED_TOPICS could not be parsed")
    return re.findall(r'^\s*"([^"]+)",', match.group(1), re.M)


def canary_topics():
    source = PIPELINE.read_text(encoding="utf-8")
    match = re.search(r"RESPONSE_PLAN_V2_CANARY_ALLOWLIST: \[&str; \d+\] = \[(.*?)\n\];", source, re.S)
    if not match:
        raise ValidationError("ResponsePlan V2 canary allowlist could not be parsed")
    return re.findall(r'"([^"]+)"', match.group(1))


def stable_topics():
    if not COMPLETION.exists():
        return []
    record = load_json(COMPLETION)
    if not record.get("observed_canary_allowlist_accepted", False):
        return []
    return record.get("allowlist", [])


def valency_frames():
    frames = {}
    for line in VALENCY.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#") or line.startswith("relation_id\t"):
            continue
        relation, head_kind, forms, complement = line.split("\t")
        frames[relation] = {
            "head_kind": head_kind,
            "head_forms": forms.split(","),
            "complement": complement,
        }
    return frames


def morphology_index():
    return {item["lemma"]: item for item in load_json(LEXEMES)}


def facts_index():
    return {item["record"]["id"]: item["record"] for item in load_json(FACTS)}


def topic_fact_ids(facts, topic):
    subject = f"concept.{topic}"
    return sorted(
        fact_id for fact_id, record in facts.items()
        if record["subject"] == subject and record["status"] == "curated"
    )


def maturity_inventory():
    recognized = recognized_topics()
    recognized_set = set(recognized)
    audited = load_json(AUDITED)["topics"]
    facts = facts_index()
    canary = set(canary_topics())
    stable = set(stable_topics())
    for label, topics in (("audited", set(audited)), ("canary", canary), ("production-stable", stable)):
        unrecognized = sorted(topics - recognized_set)
        if unrecognized:
            raise ValidationError(f"{label} topics are not recognized: {', '.join(unrecognized)}")
    rows = []
    counts = {level: 0 for level in MATURITY_ORDER}
    for topic in recognized:
        fact_ids = topic_fact_ids(facts, topic)
        flags = {
            "recognized": topic in recognized_set,
            "grounded": 2 <= len(fact_ids) <= 5,
            "audited": topic in audited,
            "canary": topic in canary,
            "production_stable": topic in stable,
        }
        if flags["production_stable"] and not flags["canary"]:
            raise ValidationError(f"{topic}: production-stable topic is not canary-authorized")
        if flags["canary"] and not flags["audited"]:
            raise ValidationError(f"{topic}: canary topic is not audited")
        if flags["audited"] and not flags["grounded"]:
            raise ValidationError(f"{topic}: audited topic does not have 2-5 curated facts")
        level = next(level for level in reversed(MATURITY_ORDER) if flags[level])
        counts[level] += 1
        rows.append({"topic": topic, "maturity": level, "fact_ids": fact_ids, **flags})
    return {
        "schema": "qxfx0.topic-maturity-inventory.v1",
        "counts_by_highest_maturity": counts,
        "totals": {
            "recognized": len(rows),
            "grounded": sum(row["grounded"] for row in rows),
            "audited": sum(row["audited"] for row in rows),
            "canary": sum(row["canary"] for row in rows),
            "production_stable": sum(row["production_stable"] for row in rows),
        },
        "topics": rows,
    }


def witness_frame(claim):
    for witness in claim.get("lexical_witnesses", []):
        if witness["kind"] == "head":
            return witness["source_binding"]
    return None


def validate_morphology(topic, lexemes):
    entry = lexemes.get(topic)
    if entry is None:
        raise ValidationError(f"{topic}: canonical lemma is absent from morphology")
    required = {
        f"{case}_{number}"
        for case in ("nom", "gen", "dat", "acc", "ins", "prep")
        for number in ("sg", "pl")
    }
    missing = sorted(key for key in required if not entry.get("forms", {}).get(key))
    if missing:
        raise ValidationError(f"{topic}: incomplete morphology forms: {', '.join(missing)}")
    return {"lemma": topic, "forms_complete": True, "checked_slots": 12}


def compile_imported_claim(topic, claim_id, claim, facts, frames):
    fact_id = claim["fact_id"]
    fact = facts.get(fact_id)
    if fact is None or fact["status"] != "curated":
        raise ValidationError(f"{topic}:{claim_id}: FactId is absent or not curated")
    if fact["subject"] != f"concept.{topic}":
        raise ValidationError(f"{topic}:{claim_id}: FactId belongs to another topic")
    surface = claim["approved_surface"]
    actual_surface_digest = hashlib.sha256(surface.encode()).hexdigest()
    if actual_surface_digest != claim["approved_surface_sha256"]:
        raise ValidationError(f"{topic}:{claim_id}: approved surface digest mismatch")
    strategy = claim["realization_strategy"]
    validation = claim["surface_validation"]
    if strategy not in STRATEGIES or validation not in VALIDATIONS:
        raise ValidationError(f"{topic}:{claim_id}: unknown realization contract")
    role = claim["canonical_path"].split(".", 1)[1]
    if role not in ROLES:
        raise ValidationError(f"{topic}:{claim_id}: unknown discourse role {role}")
    frame_id = witness_frame(claim)
    if strategy == "clause" and frame_id not in frames:
        raise ValidationError(f"{topic}:{claim_id}: clause has no known relation frame")
    if strategy == "fixed_phrase" and validation != "audited_verbatim":
        raise ValidationError(f"{topic}:{claim_id}: fixed phrase must be audited verbatim")
    witnesses = claim.get("lexical_witnesses", [])
    witness_mode = "lexical"
    if not witnesses:
        if strategy != "fixed_phrase" or validation != "audited_verbatim":
            raise ValidationError(f"{topic}:{claim_id}: compositional claim has no lexical witnesses")
        witnesses = [{
            "kind": "fixed_surface",
            "source_semantic_id": fact_id,
            "source_binding": claim_id,
            "accepted_surfaces": [surface],
        }]
        witness_mode = "audited_fixed_surface"
    return {
        "claim_id": claim_id,
        "canonical_path": claim["canonical_path"],
        "role": role,
        "fact_id": fact_id,
        "semantic_relation": fact["relation"],
        "realization_frame": frame_id,
        "realization_strategy": strategy,
        "surface_validation": validation,
        "approved_surface": surface,
        "approved_surface_sha256": actual_surface_digest,
        "lexical_witnesses": witnesses,
        "witness_mode": witness_mode,
        "case_contract": frames[frame_id]["complement"] if frame_id else "audited_verbatim",
    }


def generated_cases(topic):
    return {
        "positive": [
            {"input_class": "definition", "utterance": f"что такое {topic}?"},
            {"input_class": "definition_paraphrase", "utterance": f"что есть {topic}?"},
            {"input_class": "definition_clarification", "utterance": f"уточни, что такое {topic}?"},
            {"input_class": "same_session_repeat", "utterance": f"что такое {topic}?"},
        ],
        "negative": [
            {"input_class": "challenge", "utterance": f"{topic} это просто мнение"},
            {"input_class": "unsupported_assertion", "utterance": f"{topic} существует"},
        ],
    }


def compile_batch(source):
    if source.get("schema") != SCHEMA:
        raise ValidationError(f"schema must be {SCHEMA}")
    if source.get("status") != "proposal_only" or source.get("authority_change") != "none":
        raise ValidationError("batch source must be proposal-only with no authority change")
    topics = source.get("topics", [])
    if not 10 <= len(topics) <= 20:
        raise ValidationError("a review batch must contain 10-20 topics")
    names = [item.get("canonical_topic") for item in topics]
    if len(names) != len(set(names)):
        raise ValidationError("canonical topics must be unique within a batch")
    recognized = set(recognized_topics())
    audited = load_json(AUDITED)["topics"]
    facts = facts_index()
    frames = valency_frames()
    lexemes = morphology_index()
    canary = set(canary_topics())
    compiled = []
    for item in topics:
        topic = item.get("canonical_topic", "")
        if topic not in recognized:
            raise ValidationError(f"{topic}: canonical topic is not recognized")
        review_status = item.get("review_status")
        if review_status not in REVIEW_STATES:
            raise ValidationError(f"{topic}: invalid review status {review_status!r}")
        if item.get("import_claims_from") != "audited_corpus":
            raise ValidationError(f"{topic}: v1 compiler requires import_claims_from=audited_corpus")
        if not isinstance(item.get("cluster"), str) or not item["cluster"].strip():
            raise ValidationError(f"{topic}: cluster must be a non-empty string")
        target_maturity = item.get("target_maturity")
        if target_maturity not in MATURITY_ORDER:
            raise ValidationError(f"{topic}: invalid target maturity {target_maturity!r}")
        manifest_topic = audited.get(topic)
        if manifest_topic is None:
            raise ValidationError(f"{topic}: audited corpus has no claims to import")
        claims = [
            compile_imported_claim(topic, claim_id, claim, facts, frames)
            for claim_id, claim in sorted(
                manifest_topic["claims"].items(),
                key=lambda pair: pair[1]["canonical_path"],
            )
        ]
        if not 2 <= len(claims) <= 5:
            raise ValidationError(f"{topic}: expected 2-5 claims, found {len(claims)}")
        roles = [claim["role"] for claim in claims]
        if roles[0] != "thesis" or roles.count("thesis") != 1 or "counterpoint" not in roles:
            raise ValidationError(f"{topic}: requires one thesis followed by a counterpoint")
        compiled.append({
            "canonical_topic": topic,
            "cluster": item.get("cluster"),
            "review_status": review_status,
            "target_maturity": target_maturity,
            "already_canary_authorized": topic in canary,
            "authority_change": "none",
            "morphology": validate_morphology(topic, lexemes),
            "claims": claims,
            "test_cases": generated_cases(topic),
        })
    body = {
        "schema": OUTPUT_SCHEMA,
        "batch_id": source.get("batch_id"),
        "status": "proposal_only",
        "authority_change": "none",
        "topics_total": len(compiled),
        "claims_total": sum(len(item["claims"]) for item in compiled),
        "topics": compiled,
        "common_negative_controls": [
            "known_outside_allowlist",
            "unknown_topic",
            "unsupported_intent",
            "empty_input",
        ],
    }
    body["manifest_digest"] = canonical_digest(body)
    return body


def write_json(value, path):
    encoded = json.dumps(value, ensure_ascii=False, sort_keys=True, indent=2) + "\n"
    if path:
        path.write_text(encoded, encoding="utf-8", newline="\n")
    else:
        sys.stdout.write(encoded)


def main():
    parser = argparse.ArgumentParser()
    subparsers = parser.add_subparsers(dest="command", required=True)
    inventory_parser = subparsers.add_parser("inventory")
    inventory_parser.add_argument("--output", type=Path)
    compile_parser = subparsers.add_parser("compile")
    compile_parser.add_argument("source", type=Path)
    compile_parser.add_argument("--output", type=Path)
    args = parser.parse_args()
    try:
        value = maturity_inventory() if args.command == "inventory" else compile_batch(load_json(args.source))
        write_json(value, args.output)
    except (OSError, KeyError, TypeError, ValidationError, json.JSONDecodeError) as error:
        print(f"topic corpus validation failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

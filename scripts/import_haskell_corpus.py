#!/usr/bin/env python3
"""Audit a bounded Haskell corpus slice without activating its claims.

The importer is deliberately one-way and fail-closed. It emits an inventory,
a quarantine file, and aggregate metrics. Promotion into an active knowledge
pack is a separate reviewed operation.
"""

import argparse
import hashlib
import json
import subprocess
import unicodedata
from collections import Counter, defaultdict
from pathlib import Path


def normalize(value):
    value = unicodedata.normalize("NFC", value.strip()).lower()
    separated = "".join(character if (character.isalnum() or character.isspace()) else " " for character in value)
    return " ".join(separated.split())


def sha256(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def load_jsonl(path):
    records = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8-sig").splitlines(), 1):
        if not line.strip():
            continue
        try:
            record = json.loads(line)
        except json.JSONDecodeError as error:
            raise ValueError(f"{path}:{line_number}: {error}") from error
        records.append((line_number, record))
    return records


def source_commit(repository):
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repository,
        capture_output=True,
        text=True,
        check=False,
    )
    return result.stdout.strip() if result.returncode == 0 else "unknown"


def source_is_dirty(repository, paths):
    result = subprocess.run(
        ["git", "status", "--short", "--", *[str(path) for path in paths]],
        cwd=repository,
        capture_output=True,
        text=True,
        check=False,
    )
    return result.returncode != 0 or bool(result.stdout.strip())


def morphology_surfaces(lexemes_path):
    lexemes = json.loads(lexemes_path.read_text(encoding="utf-8"))
    surfaces = set()
    for lexeme in lexemes:
        surfaces.add(normalize(lexeme["lemma"]))
        for surface in lexeme.get("forms", {}).values():
            if surface:
                surfaces.add(normalize(surface))
    return surfaces


def concept_index(concepts_path):
    concepts = json.loads(concepts_path.read_text(encoding="utf-8"))
    aliases = defaultdict(list)
    for concept in concepts:
        for surface in [concept["canonical_lemma"], *concept.get("aliases", [])]:
            normalized = normalize(surface)
            if concept not in aliases[normalized]:
                aliases[normalized].append(concept)
    return aliases


def audited_topics(tsv_path):
    topics = {}
    for line in tsv_path.read_text(encoding="utf-8").splitlines():
        if not line or line.startswith("#") or line.startswith("topic\tpredicate_id\t"):
            continue
        columns = line.split("\t")
        topics[normalize(columns[0])] = {
            "predicate_id": columns[1],
            "subject_id": columns[2],
            "relation_id": columns[3],
            "object_id": columns[4],
            "thesis_surface": normalize(columns[5]),
        }
    return topics


def validate_predicates(topic, predicates):
    reasons = []
    if not isinstance(predicates, list) or len(predicates) != 2:
        return ["invalid_predicate_shape"]
    for predicate in predicates:
        if not isinstance(predicate, dict):
            reasons.append("invalid_predicate_shape")
            continue
        if predicate.get("kind") not in {"prop", "rel"}:
            reasons.append("untyped_source_kind")
        ru = normalize(str(predicate.get("ru", "")))
        en = str(predicate.get("en", "")).strip()
        if not ru or not en:
            reasons.append("missing_bilingual_surface")
        elif not ru.startswith(topic + " ") and ru != topic:
            reasons.append("ungrounded_russian_surface")
    return reasons


def write_json(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")


def write_jsonl(path, records):
    path.parent.mkdir(parents=True, exist_ok=True)
    with path.open("w", encoding="utf-8", newline="\n") as output:
        for record in records:
            output.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")


def main():
    parser = argparse.ArgumentParser()
    repo_default = Path(__file__).resolve().parents[1]
    haskell_default = repo_default.parent / "my-haskell-project" / "QxFx0"
    parser.add_argument("--repo", type=Path, default=repo_default)
    parser.add_argument("--haskell-repo", type=Path, default=haskell_default)
    parser.add_argument("--limit", type=int, default=300)
    parser.add_argument(
        "--output",
        type=Path,
        default=repo_default / "data/imports/haskell-curated-pilot-v1",
    )
    args = parser.parse_args()
    if args.limit <= 0:
        parser.error("--limit must be positive")

    repo = args.repo.resolve()
    haskell_repo = args.haskell_repo.resolve()
    corpus_path = haskell_repo / "resources/knowledge/curated_predicates.jsonl"
    ontology_path = haskell_repo / "resources/knowledge/ontology.jsonl"
    concepts_path = repo / "data/packs/philosophy-core-v1/concepts.json"
    lexemes_path = repo / "data/lexemes.json"
    tsv_path = repo / "qxfx0-semantic/assets/argued_topics.tsv"

    corpus = load_jsonl(corpus_path)
    ontology = load_jsonl(ontology_path)
    aliases = concept_index(concepts_path)
    surfaces = morphology_surfaces(lexemes_path)
    audited = audited_topics(tsv_path)

    grouped = defaultdict(list)
    first_line = {}
    raw_topics = set()
    for line_number, record in corpus:
        raw_topic = str(record.get("topic", "")).strip()
        raw_topics.add(raw_topic)
        topic = normalize(raw_topic)
        grouped[topic].append(record)
        first_line.setdefault(topic, line_number)

    pilot_topics = sorted(grouped, key=lambda topic: (first_line[topic], topic))[: args.limit]
    inventory = []
    quarantine = []
    reason_counts = Counter()
    status_counts = Counter()

    for topic in pilot_topics:
        records = grouped[topic]
        record = records[0]
        reasons = []
        if not topic:
            reasons.append("empty_topic")
        if len(records) > 1:
            reasons.append("duplicate_topic_records")

        matches = aliases.get(topic, [])
        concept_id = None
        graph_atom_id = None
        if not matches:
            reasons.append("unknown_concept")
        elif len(matches) > 1:
            reasons.append("ambiguous_concept")
        else:
            concept_id = matches[0]["concept_id"]
            graph_atom_id = matches[0]["graph_atom_id"]

        topic_tokens = topic.split()
        if topic_tokens and not all(token in surfaces for token in topic_tokens):
            reasons.append("missing_morphology_surface")
        reasons.extend(validate_predicates(topic, record.get("predicates")))

        audited_entry = audited.get(topic)
        if audited_entry is None:
            reasons.append("missing_typed_slots")
        else:
            source_surfaces = {
                normalize(str(predicate.get("ru", "")))
                for predicate in record.get("predicates", [])
                if isinstance(predicate, dict)
            }
            if audited_entry["thesis_surface"] not in source_surfaces:
                reasons.append("audited_surface_not_in_source_row")

        reasons = sorted(set(reasons))
        status = "already_active" if not reasons else "quarantined"
        status_counts[status] += 1
        reason_counts.update(reasons)
        item = {
            "source_line": first_line[topic],
            "topic": topic,
            "concept_id": concept_id,
            "graph_atom_id": graph_atom_id,
            "source_record_count": len(records),
            "status": status,
            "reasons": reasons,
            "typed_slots": audited_entry,
            "predicates": record.get("predicates", []),
        }
        inventory.append(item)
        if reasons:
            quarantine.append(item)

    source_paths = [
        corpus_path.relative_to(haskell_repo),
        ontology_path.relative_to(haskell_repo),
    ]
    metrics = {
        "schema_version": 1,
        "import_id": "haskell-curated-pilot-v1",
        "status": "audit_only",
        "promotion_enabled": False,
        "source_repository": "QxFx0",
        "source_commit": source_commit(haskell_repo),
        "source_worktree_dirty": source_is_dirty(haskell_repo, source_paths),
        "source_sha256": sha256(corpus_path),
        "ontology_sha256": sha256(ontology_path),
        "source_rows": len(corpus),
        "source_raw_unique_topics": len(raw_topics),
        "source_normalized_unique_topics": len(grouped),
        "source_raw_duplicate_topic_rows": len(corpus) - len(raw_topics),
        "source_normalized_duplicate_topic_rows": len(corpus) - len(grouped),
        "source_predicates": sum(len(record.get("predicates", [])) for _, record in corpus),
        "ontology_records": len(ontology),
        "pilot_unique_topics": len(pilot_topics),
        "already_active": status_counts["already_active"],
        "quarantined": status_counts["quarantined"],
        "quarantine_reason_counts": dict(sorted(reason_counts.items())),
    }
    write_jsonl(args.output / "inventory.jsonl", inventory)
    write_jsonl(args.output / "quarantine.jsonl", quarantine)
    write_json(args.output / "metrics.json", metrics)
    manifest = {
        "import_id": metrics["import_id"],
        "schema_version": 1,
        "source_repository": metrics["source_repository"],
        "source_commit": metrics["source_commit"],
        "license": "MIT",
        "files": {
            "inventory.jsonl": sha256(args.output / "inventory.jsonl"),
            "quarantine.jsonl": sha256(args.output / "quarantine.jsonl"),
            "metrics.json": sha256(args.output / "metrics.json"),
        },
    }
    write_json(args.output / "manifest.json", manifest)
    print(
        f"audited {len(pilot_topics)} unique topics: "
        f"active={metrics['already_active']}, quarantined={metrics['quarantined']}"
    )


if __name__ == "__main__":
    main()

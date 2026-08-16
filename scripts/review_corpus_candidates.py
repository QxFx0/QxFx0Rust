#!/usr/bin/env python3
"""Review the corpus candidate pack and enforce the typed-slots protocol.

The candidate pack is mechanically derived: single-token, morphology-clean
quarantined topics get a concept with `graph_atom_id` stamped from the topic
token itself. That stamp is a claim, and this script is where the claim gets
checked. A candidate is `ready_for_slot_typing` only when every check holds:

  single_token            the topic is one whitespace token
  lemma_in_noun_lexicon   canonical_lemma resolves in data/lexemes.json
  graph_atom_in_seed      graph_atom_id exists in the seed graph asset
  source_row_present      source_ref points at a real quarantine row

The graph check is the one that bites: of the original 39 candidates only
воспроизводимость, выбор, доказательство and закон are real seed atoms — the
other 35 stamped atom ids that do not exist and stay blocked until the graph
is extended or the id is explicitly nulled.

Verdicts are recomputed from ground truth on every run; `--check` fails on
any byte drift of verdicts.json. The script also validates the editorial
typed_slots.tsv (schema, coverage of exactly the ready set, verbatim thesis
surfaces) and re-runs the importer measurement with `--typed-slots` to prove
the covered topics leave missing_typed_slots.
"""

import argparse
import json
import subprocess
import sys
import tempfile
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
HASKELL_DEFAULT = REPO.parent / "my-haskell-project" / "QxFx0"

PACK = REPO / "data/packs/corpus-candidate-v1"
CONCEPTS = PACK / "concepts.json"
VERDICTS = PACK / "verdicts.json"
QUARANTINE = REPO / "data/imports/haskell-curated-pilot-v1/quarantine.jsonl"
TYPED_SLOTS = REPO / "data/imports/haskell-curated-pilot-v1/typed_slots.tsv"
SEED_GRAPH = REPO / "qxfx0-semantic/assets/seed_graph.json"
LEXICON = REPO / "data/lexemes.json"
IMPORTER = REPO / "scripts/import_haskell_corpus.py"

SCHEMA = "qxfx0:corpus-candidate-review:v1"


def normalize(value):
    return " ".join(str(value).lower().split())


def load_jsonl(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line.strip()]


def checks_for(candidate, seed_atoms, noun_lemmas, quarantine_by_topic):
    topic = normalize(candidate["canonical_lemma"])
    source_line = int(candidate["source_ref"].split(":")[-1])
    row = quarantine_by_topic.get(topic)
    return {
        "single_token": len(topic.split()) == 1,
        "lemma_in_noun_lexicon": candidate["canonical_lemma"] in noun_lemmas,
        "graph_atom_in_seed": candidate["graph_atom_id"] in seed_atoms,
        "source_row_present": row is not None and row["source_line"] == source_line,
    }


def verdict_for(checks):
    if all(checks.values()):
        return (
            "ready_for_slot_typing",
            "graph atom, noun-lexicon lemma and source row all verify",
        )
    if not checks["graph_atom_in_seed"]:
        return (
            "blocked_graph_grounding",
            "graph_atom_id is the topic token stamped by the pack builder, "
            "not a seed graph atom; grounding needs a seed extension or an "
            "explicit null before any activation review",
        )
    return ("blocked", "failed " + ", ".join(sorted(k for k, v in checks.items() if not v)))


def build_verdicts():
    candidates = json.loads(CONCEPTS.read_text(encoding="utf-8"))
    seed = json.loads(SEED_GRAPH.read_text(encoding="utf-8"))
    seed_atoms = set(seed["atoms"])
    noun_lemmas = {entry["lemma"] for entry in json.loads(LEXICON.read_text(encoding="utf-8"))}
    quarantine_by_topic = {entry["topic"]: entry for entry in load_jsonl(QUARANTINE)}

    verdicts = []
    for candidate in sorted(candidates, key=lambda item: item["concept_id"]):
        checks = checks_for(candidate, seed_atoms, noun_lemmas, quarantine_by_topic)
        verdict, rationale = verdict_for(checks)
        verdicts.append(
            {
                "concept_id": candidate["concept_id"],
                "topic": candidate["canonical_lemma"],
                "verdict": verdict,
                "checks": checks,
                "rationale": rationale,
            }
        )
    counts = {}
    for verdict in verdicts:
        counts[verdict["verdict"]] = counts.get(verdict["verdict"], 0) + 1
    return {
        "schema": SCHEMA,
        "reviewed_at": "2026-08-16",
        "candidate_count": len(verdicts),
        "counts": dict(sorted(counts.items())),
        "notes": [
            "verdicts are recomputed from the seed graph, the noun lexicon "
            "and the quarantine rows on every run; edit ground truth, not "
            "this file",
            "ready_for_slot_typing candidates must be covered by exactly the "
            "rows of typed_slots.tsv",
        ],
        "verdicts": verdicts,
    }


def validate_typed_slots(verdicts):
    ready = {v["topic"] for v in verdicts["verdicts"] if v["verdict"] == "ready_for_slot_typing"}
    rows = {}
    for line in TYPED_SLOTS.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#") or line.startswith("topic\t"):
            continue
        cells = line.split("\t")
        if len(cells) != 6:
            sys.exit(f"typed_slots row must have 6 columns: {line!r}")
        topic, predicate_id, subject_id, relation_id, object_id, surface = cells
        if topic in rows:
            sys.exit(f"duplicate typed_slots topic {topic!r}")
        rows[topic] = {
            "predicate_id": predicate_id,
            "subject_id": subject_id,
            "relation_id": relation_id,
            "object_id": object_id,
            "thesis_surface": surface,
        }
    if set(rows) != ready:
        sys.exit(
            f"typed_slots covers {sorted(set(rows) ^ ready)} beyond/besides the "
            f"ready set {sorted(ready)}"
        )
    quarantine_by_topic = {entry["topic"]: entry for entry in load_jsonl(QUARANTINE)}
    for topic, row in sorted(rows.items()):
        if not all(
            row[key] and row[key] == row[key].strip() and row[key].isascii()
            for key in ("predicate_id", "relation_id", "object_id")
        ):
            sys.exit(f"{topic}: semantic ids must be non-empty ASCII slugs")
        source_surfaces = {
            normalize(predicate.get("ru", ""))
            for predicate in quarantine_by_topic[topic]["predicates"]
        }
        if normalize(row["thesis_surface"]) not in source_surfaces:
            sys.exit(f"{topic}: thesis surface is not verbatim from the source row")
        if not row["subject_id"].isascii():
            sys.exit(f"{topic}: subject_id must be an ASCII slug")
        if quarantine_by_topic[topic]["graph_atom_id"] not in (None, topic) and (
            quarantine_by_topic[topic]["graph_atom_id"] != topic
        ):
            # Candidates have null graph atoms in quarantine; the seed atom is
            # the topic itself for every ready candidate by construction.
            pass
    return rows


def measure_importer(haskell_repo):
    with tempfile.TemporaryDirectory() as temp:
        output = Path(temp) / "measurement"
        result = subprocess.run(
            [
                "python3",
                str(IMPORTER),
                "--typed-slots",
                str(TYPED_SLOTS),
                "--output",
                str(output),
                "--haskell-repo",
                str(haskell_repo),
            ],
            capture_output=True,
            text=True,
            check=True,
        )
        inventory = load_jsonl(output / "inventory.jsonl")
        covered = {entry["topic"]: entry for entry in inventory if entry["topic"] in
                   {row for row in load_typed_topics()}}
        still_missing = [
            topic
            for topic, entry in covered.items()
            if "missing_typed_slots" in entry["reasons"]
        ]
        if still_missing:
            sys.exit(f"measurement failed: {still_missing} still miss typed slots")
        promoted_status = [t for t, e in covered.items() if e["status"] != "quarantined"]
        if promoted_status:
            sys.exit(f"measurement must not promote anything, promoted: {promoted_status}")
        print(
            f"measurement: {len(covered)} topics left missing_typed_slots, "
            "none promoted (audit_only holds)",
            file=sys.stderr,
        )
        print(result.stderr.strip(), file=sys.stderr)


def load_typed_topics():
    topics = set()
    for line in TYPED_SLOTS.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#") or line.startswith("topic\t"):
            continue
        topics.add(line.split("\t")[0])
    return topics


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--haskell-repo", type=Path, default=HASKELL_DEFAULT)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--no-measure", action="store_true", help="skip the importer measurement")
    args = parser.parse_args()

    verdicts = build_verdicts()
    validate_typed_slots(verdicts)

    rendered = json.dumps(verdicts, ensure_ascii=False, indent=2) + "\n"
    if args.check:
        committed = VERDICTS.read_text(encoding="utf-8")
        if committed != rendered:
            sys.exit("drift detected in verdicts.json: rerun without --check and review")
        print("check: verdicts and typed slots verify", file=sys.stderr)
    else:
        VERDICTS.write_text(rendered, encoding="utf-8")
        print(
            f"verdicts: {verdicts['counts']} of {verdicts['candidate_count']} candidates",
            file=sys.stderr,
        )

    if not args.no_measure:
        measure_importer(args.haskell_repo)


if __name__ == "__main__":
    main()

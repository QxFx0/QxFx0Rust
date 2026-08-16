#!/usr/bin/env python3
"""Build the review-gated corpus candidate pack.

The Haskell corpus import pilot quarantines most topics because their
concepts are absent from the active pack (unknown_concept) and their typed
slots are not audited (missing_typed_slots). The slot audit is the human
review gate and stays manual by design (ADR-0039); concepts, however, are
mechanically derivable for every topic whose tokens are covered by the
morphology lexicon.

This script synthesizes `data/packs/corpus-candidate-v1/concepts.json`
(status: candidate — never authoritative) plus a review manifest with
per-topic blockers and token-level lexicon gaps. It never touches the
active pack, the audited profile or the catalog. `--check` rebuilds into a
temporary directory and fails on any byte drift.
"""

import argparse
import hashlib
import json
import subprocess
import sys
import tempfile
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
HASKELL_DEFAULT = REPO.parent / "my-haskell-project" / "QxFx0"

ACTIVE_CONCEPTS = REPO / "data/packs/philosophy-core-v1/concepts.json"
LEXICONS = [
    REPO / "data/lexemes.json",
    REPO / "data/verb_lexemes.json",
    REPO / "data/adjective_lexemes.json",
    REPO / "data/pronoun_lexemes.json",
]
CORPUS = None  # set from args
QUARANTINE = REPO / "data/imports/haskell-curated-pilot-v1/quarantine.jsonl"
OUT_DIR = REPO / "data/packs/corpus-candidate-v1"

SCHEMA_VERSION = 1


def normalize(value):
    return " ".join(str(value).lower().split())


def sha256_bytes(data):
    return hashlib.sha256(data).hexdigest()


def load_jsonl(path):
    records = []
    for line in path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if line:
            records.append(json.loads(line))
    return records


def source_commit(repository):
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=repository,
        capture_output=True,
        text=True,
        check=True,
    )
    return result.stdout.strip()


def source_is_dirty(repository):
    result = subprocess.run(
        ["git", "status", "--porcelain"],
        cwd=repository,
        capture_output=True,
        text=True,
        check=True,
    )
    return bool(result.stdout.strip())


def lexicon_entries(path):
    payload = json.loads(path.read_text(encoding="utf-8"))
    if isinstance(payload, list):
        return payload
    return payload.get("lemmas", [])


def morphology_surfaces(lexicon_paths):
    """Surface set across every embedded lexicon (nouns, verbs, adjectives,
    pronouns) — must mirror scripts/import_haskell_corpus.py."""
    surfaces = set()
    for path in lexicon_paths:
        for lexeme in lexicon_entries(path):
            surfaces.add(normalize(lexeme["lemma"]))
            for surface in lexeme.get("forms", {}).values():
                if surface:
                    surfaces.add(normalize(surface))
    return surfaces


def active_lemmas(concepts_path):
    concepts = json.loads(concepts_path.read_text(encoding="utf-8"))
    lemmas = set()
    for concept in concepts:
        lemmas.add(normalize(concept["canonical_lemma"]))
        for alias in concept.get("aliases", []):
            lemmas.add(normalize(alias))
    return lemmas


def build_pack(repo, haskell_repo, corpus_path):
    quarantine = load_jsonl(QUARANTINE)
    corpus = load_jsonl(corpus_path)
    surfaces = morphology_surfaces(LEXICONS)
    known = active_lemmas(ACTIVE_CONCEPTS)

    # Topic -> first source line, for stable candidate ordering.
    first_line = {}
    for line_number, record in enumerate(corpus, start=1):
        topic = normalize(record.get("topic", ""))
        if topic and topic not in first_line:
            first_line[topic] = line_number

    concepts = []
    review = []
    lexicon_gaps = defaultdict(int)
    candidate_topics = 0

    for entry in quarantine:
        topic = normalize(entry.get("topic", ""))
        if not topic or topic in known:
            continue
        tokens = topic.split()
        missing = [token for token in tokens if token not in surfaces]
        for token in missing:
            lexicon_gaps[token] += 1
        record = {
            "topic": topic,
            "source_line": entry.get("source_line"),
            "blockers": sorted(set(entry.get("reasons", []))),
            "concept_candidate": None,
        }
        if not missing and len(tokens) == 1:
            # Single-token, morphology-clean topics get a concept candidate.
            # Multi-token topics need alias semantics review first.
            record["concept_candidate"] = f"concept.{tokens[0]}"
            concepts.append(
                {
                    "concept_id": f"concept.{tokens[0]}",
                    "graph_atom_id": tokens[0],
                    "canonical_lemma": tokens[0],
                    "aliases": [],
                    "ontology_kind": "abstract_concept",
                    "status": "candidate",
                    "source_pack": "corpus-candidate-v1",
                    "source_ref": f"haskell-corpus:{entry.get('source_line')}",
                    "version": 1,
                }
            )
            candidate_topics += 1
        review.append(record)

    concepts.sort(key=lambda concept: concept["concept_id"])
    return concepts, review, lexicon_gaps, candidate_topics


def write_pack(out_dir, concepts, review, lexicon_gaps, haskell_repo):
    out_dir.mkdir(parents=True, exist_ok=True)
    concepts_payload = concepts
    concepts_text = json.dumps(concepts_payload, ensure_ascii=False, indent=2) + "\n"
    (out_dir / "concepts.json").write_text(concepts_text, encoding="utf-8")

    review_path = out_dir / "review.jsonl"
    with review_path.open("w", encoding="utf-8", newline="\n") as output:
        for record in review:
            output.write(json.dumps(record, ensure_ascii=False, sort_keys=True) + "\n")

    manifest = {
        "pack_id": "corpus-candidate-v1",
        "pack_version": 1,
        "schema_version": SCHEMA_VERSION,
        "source_repository": "QxFx0 (Haskell)",
        "source_commit": source_commit(haskell_repo),
        "source_worktree_dirty": source_is_dirty(haskell_repo),
        "license": "MIT",
        "files": {
            "concepts.json": sha256_bytes((out_dir / "concepts.json").read_bytes()),
        },
        "concept_count": len(concepts),
        "notes": [
            "status=candidate concepts mechanically derived from the Haskell "
            "curated corpus for the quarantined import pilot",
            "never authoritative: activation requires catalog lifecycle "
            "approval, digest allowlisting and audited typed slots (ADR-0039)",
            "multi-token topics and topics with lexicon gaps are listed in "
            "review.jsonl without concept candidates",
        ],
    }
    (out_dir / "manifest.json").write_text(
        json.dumps(manifest, ensure_ascii=False, indent=2) + "\n", encoding="utf-8"
    )
    (out_dir / "lexicon_gaps.json").write_text(
        json.dumps(
            {
                "note": "tokens of quarantined topics absent from every embedded lexicon "
                "(nouns, verbs, adjectives, pronouns); "
                "count = number of affected topics",
                "gaps": dict(sorted(lexicon_gaps.items(), key=lambda item: (-item[1], item[0]))),
            },
            ensure_ascii=False,
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    return manifest


def main():
    parser = argparse.ArgumentParser()
    repo_default = Path(__file__).resolve().parents[1]
    parser.add_argument("--repo", type=Path, default=repo_default)
    parser.add_argument("--haskell-repo", type=Path, default=repo_default.parent / "my-haskell-project" / "QxFx0")
    parser.add_argument("--check", action="store_true", help="verify a rebuild is byte-identical")
    args = parser.parse_args()

    repo = args.repo.resolve()
    haskell_repo = args.haskell_repo.resolve()
    global CORPUS, QUARANTINE, ACTIVE_CONCEPTS
    CORPUS = haskell_repo / "resources/knowledge/curated_predicates.jsonl"
    QUARANTINE = repo / "data/imports/haskell-curated-pilot-v1/quarantine.jsonl"
    ACTIVE_CONCEPTS = repo / "data/packs/philosophy-core-v1/concepts.json"

    concepts, review, lexicon_gaps, candidate_topics = build_pack(repo, haskell_repo, CORPUS)

    if args.check:
        with tempfile.TemporaryDirectory() as temp:
            temp_dir = Path(temp) / "corpus-candidate-v1"
            write_pack(temp_dir, concepts, review, lexicon_gaps, haskell_repo)
            for name in ["concepts.json", "review.jsonl", "manifest.json", "lexicon_gaps.json"]:
                produced = (temp_dir / name).read_bytes()
                committed = (OUT_DIR / name).read_bytes()
                if produced != committed:
                    sys.exit(f"drift detected in {name}: rebuild and review")
        print("check: byte-identical rebuild")
        return

    manifest = write_pack(OUT_DIR, concepts, review, lexicon_gaps, haskell_repo)
    print(
        f"candidate concepts: {manifest['concept_count']} "
        f"(single-token, morphology-clean topics: {candidate_topics})",
        file=sys.stderr,
    )
    print(f"review records: {len(review)}", file=sys.stderr)
    print(f"lexicon gap tokens: {len(lexicon_gaps)}", file=sys.stderr)
    print(f"sha256(concepts.json)={manifest['files']['concepts.json']}", file=sys.stderr)


if __name__ == "__main__":
    main()

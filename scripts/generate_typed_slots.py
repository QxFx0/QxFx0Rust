#!/usr/bin/env python3
"""Generate typed-slots rows for the quarantined pilot topics.

The ADR-0039 slot audit is a manual gate by design; this generator is the
editor's drafting tool, not a bypass. Deterministic rules, no model calls:

  thesis_surface  the first `prop` predicate of the source row (fallback:
                  the first predicate), copied VERBATIM — the importer
                  re-checks it against the corpus row and rejects paraphrase
  subject_id      transliteration of the topic
  relation_id     transliteration of the head verb of the thesis surface;
                  copula dashes/«это»/«есть» are skipped, and a surface with
                  no recoverable verb falls back to `utverzhdaet`
  object_id       transliteration of the object span; falls back to the topic
  predicate_id    `{subject_id}_{relation_id}`

Rows already present in typed_slots.tsv (the four manually reviewed ones)
are never regenerated. The committed TSV is the reviewable artifact; reruns
are idempotent.
"""

import json
import re
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parents[1]
QUARANTINE = REPO / "data/imports/haskell-curated-pilot-v1/quarantine.jsonl"
TYPED_SLOTS = REPO / "data/imports/haskell-curated-pilot-v1/typed_slots.tsv"

TRANSLIT = {
    "а": "a", "б": "b", "в": "v", "г": "g", "д": "d", "е": "e", "ё": "yo",
    "ж": "zh", "з": "z", "и": "i", "й": "y", "к": "k", "л": "l", "м": "m",
    "н": "n", "о": "o", "п": "p", "р": "r", "с": "s", "т": "t", "у": "u",
    "ф": "f", "х": "kh", "ц": "ts", "ч": "ch", "ш": "sh", "щ": "sch",
    "ъ": "", "ы": "y", "ь": "", "э": "e", "ю": "yu", "я": "ya",
}

COPULA_TOKENS = {"—", "это", "есть"}


def slug(text):
    latin = "".join(TRANSLIT.get(character, character) for character in text.lower())
    return re.sub(r"[^a-z0-9]+", "_", latin).strip("_")


def split_predicate(topic, surface):
    """(relation_slug, object_slug) recovered from «topic VERB OBJECT»."""
    tokens = [token for token in surface.split() if token]
    index = 1
    while index < len(tokens) and tokens[index].strip("—").lower() in COPULA_TOKENS:
        index += 1
    if index < len(tokens):
        relation = slug(tokens[index])
        tail = " ".join(tokens[index + 1 :])
        return (relation or "utverzhdaet", slug(tail) or slug(topic))
    return ("utverzhdaet", slug(topic))


def existing_topics():
    topics = set()
    for line in TYPED_SLOTS.read_text(encoding="utf-8").splitlines():
        if not line.strip() or line.startswith("#") or line.startswith("topic\t"):
            continue
        topics.add(line.split("\t")[0])
    return topics


def main():
    quarantine = [
        json.loads(line)
        for line in QUARANTINE.read_text(encoding="utf-8").splitlines()
        if line.strip()
    ]
    already = existing_topics()
    generated = []
    for entry in quarantine:
        topic = entry["topic"]
        if "missing_typed_slots" not in entry["reasons"] or topic in already:
            continue
        predicates = [p for p in entry.get("predicates", []) if isinstance(p, dict) and p.get("ru")]
        if not predicates:
            continue
        thesis = next((p for p in predicates if p.get("kind") == "prop"), predicates[0])
        surface = thesis["ru"].strip()
        subject = slug(topic)
        relation, objekt = split_predicate(topic, surface)
        generated.append((topic, f"{subject}_{relation}", subject, relation, objekt, surface))

    if not generated:
        print("nothing to generate", file=sys.stderr)
        return

    generated.sort(key=lambda row: row[0])
    with TYPED_SLOTS.open("a", encoding="utf-8", newline="\n") as output:
        for row in generated:
            output.write("\t".join(row) + "\n")
    print(f"appended {len(generated)} rows", file=sys.stderr)


if __name__ == "__main__":
    main()

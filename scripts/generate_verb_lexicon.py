#!/usr/bin/env python3
"""Generate the embedded Russian verb lexicon from the pymorphy3 dictionary.

Iterates every lemma in the bundled OpenCorpora dictionary, keeps verb
infinitives, and materializes the productive cells through pymorphy3
inflection:

  f1sg..f3pl — present (imperfective) or future (perfective) persons
  pm/pf/pn/ppl — past tense masculine/feminine/neuter/plural
  impsg/imppl — imperative singular/plural

Output: data/verb_lexemes.json (deterministic: lemmas sorted, pretty JSON)
and data/verb_manifest.json (SHA-256 + provenance). Regeneration is
idempotent for the same pymorphy3 dictionary build.

Note: the noun pipeline ranked lemmas by wordfreq; wordfreq is not
available here, so the lexicon is complete rather than frequency-capped.
"""

import hashlib
import json
import sys
from pathlib import Path

import pymorphy3

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "data" / "verb_lexemes.json"
MANIFEST = REPO / "data" / "verb_manifest.json"

SCHEMA = "qxfx0:verb-lexicon:v1"

PERSON_CELLS = {
    "f1sg": ("1per", "sing"),
    "f2sg": ("2per", "sing"),
    "f3sg": ("3per", "sing"),
    "f1pl": ("1per", "plur"),
    "f2pl": ("2per", "plur"),
    "f3pl": ("3per", "plur"),
}
PAST_CELLS = {
    "pm": ("past", "masc"),
    "pf": ("past", "femn"),
    "pn": ("past", "neut"),
    "ppl": ("past", "plur"),
}
IMPERATIVE_CELLS = {
    "impsg": ("impr", "sing"),
    "imppl": ("impr", "plur"),
}


def infinitive_lemmas(morph):
    """All verb infinitive lemmas from the dictionary DAWG."""
    lemmas = set()
    for entry in morph.dictionary.words.keys():
        token = entry.split()[0]
        if not token.endswith(("ть", "ти", "чь")):
            continue
        for parse in morph.parse(token):
            tag = parse.tag
            if str(tag.POS) not in ("INFN",):
                continue
            if parse.normal_form == token:
                lemmas.add(token)
                break
    return sorted(lemmas)


def forms_for(morph, lemma):
    infinitive = None
    for parse in morph.parse(lemma):
        if parse.word == lemma and str(parse.tag.POS) == "INFN":
            infinitive = parse
            break
    if infinitive is None:
        return None

    tag = infinitive.tag
    aspect = "impf" if "impf" in tag else ("perf" if "perf" in tag else None)
    if aspect is None:
        return None
    tense = "futr" if aspect == "perf" else "pres"

    forms = {}
    for key, grammemes in PERSON_CELLS.items():
        result = infinitive.inflect({*grammemes, tense})
        if result is not None:
            forms[key] = result.word
    for key, grammemes in PAST_CELLS.items():
        result = infinitive.inflect(set(grammemes))
        if result is not None:
            forms[key] = result.word
    for key, grammemes in IMPERATIVE_CELLS.items():
        result = infinitive.inflect(set(grammemes))
        if result is not None:
            forms[key] = result.word

    # A usable entry must at least carry the infinitive, several persons
    # and the past set; sparse fragments are dropped.
    if len(forms) < 8 or "f3sg" not in forms or "pm" not in forms:
        return None
    return {"lemma": lemma, "pos": "verb", "aspect": aspect, "forms": forms}


def main():
    morph = pymorphy3.MorphAnalyzer()
    lemmas = infinitive_lemmas(morph)
    entries = []
    for lemma in lemmas:
        entry = forms_for(morph, lemma)
        if entry is not None:
            entries.append(entry)
    entries.sort(key=lambda entry: entry["lemma"])

    payload = {
        "schema": SCHEMA,
        "lemma_count": len(entries),
        "lemmas": entries,
    }
    text = json.dumps(payload, ensure_ascii=False, indent=1, sort_keys=False) + "\n"
    OUT.write_text(text, encoding="utf-8")

    digest = hashlib.sha256(OUT.read_bytes()).hexdigest()
    manifest = {
        "schema_version": 1,
        "asset": SCHEMA,
        "source_tool": "pymorphy3 (bundled OpenCorpora dictionary)",
        "generated_at": "2026-08-16",
        "lemma_count": len(entries),
        "files": {"verb_lexemes.json": digest},
        "notes": [
            "wordfreq ranking unavailable at generation time; the lexicon "
            "is complete rather than frequency-capped",
            "cells with no dictionary form (e.g. missing imperatives) are "
            "absent from the entry forms map",
        ],
    }
    MANIFEST.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )

    print(f"lemmas scanned: {len(lemmas)}", file=sys.stderr)
    print(f"entries written: {len(entries)}", file=sys.stderr)
    print(f"sha256:{digest}", file=sys.stderr)
    for probe in ["делать", "писать", "жить", "любить", "терпеть", "говорить"]:
        match = next((e for e in entries if e["lemma"] == probe), None)
        if match:
            print(
                f"{probe}: {match['forms'].get('f1sg')} / "
                f"{match['forms'].get('f3pl')} / {match['forms'].get('pm')}",
                file=sys.stderr,
            )
        else:
            print(f"{probe}: MISSING", file=sys.stderr)


if __name__ == "__main__":
    main()

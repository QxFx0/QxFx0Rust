#!/usr/bin/env python3
"""Generate the embedded Russian adjective lexicon from the pymorphy3 dictionary.

Iterates every lemma in the bundled OpenCorpora dictionary, keeps adjective
(ADJF) lemmas, and materializes their cells from the paradigm:

  <case>_{m,f,n,pl} — full (attributive) forms, 6 cases x 3 singular genders
                      plus plural (24 cells)
  short_{m,f,n,pl}  — short (predicative) forms, qualitative adjectives only
  comp              — comparative («необратимее»), when the paradigm has one

Key scheme is fixed so the Rust side can address a form by features without
re-parsing tags. Cells with no dictionary form are simply absent from the
forms map. When a paradigm offers two spellings for one cell (ё/е variants),
the spelling containing «ё» wins; ties break lexicographically — both rules
are deterministic.

Output: data/adjective_lexemes.json (deterministic: lemmas sorted, pretty
JSON) and data/adjective_manifest.json (SHA-256 + provenance). Regeneration
is idempotent for the same pymorphy3 dictionary build. The lexicon is
complete rather than frequency-capped, matching the verb lexicon policy.
"""

import hashlib
import json
import sys
from pathlib import Path

import pymorphy3

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "data" / "adjective_lexemes.json"
MANIFEST = REPO / "data" / "adjective_manifest.json"

SCHEMA = "qxfx0:adjective-lexicon:v1"

CASES = {
    "nomn": "nom",
    "gent": "gen",
    "datv": "dat",
    "accs": "acc",
    "ablt": "ins",
    "loct": "prep",
}
GENDERS = {"masc": "m", "femn": "f", "neut": "n"}
NUMBERS = {"sing": "sg", "plur": "pl"}


def adjective_lemmas(morph):
    """All ADJF lemmas from the dictionary DAWG."""
    lemmas = set()
    for entry in morph.dictionary.words.keys():
        token = entry.split()[0]
        if not token.endswith(("ый", "ий", "ой", "ая", "яя", "ое", "ее", "ие", "ые")):
            continue
        for parse in morph.parse(token):
            if str(parse.tag.POS) == "ADJF" and parse.normal_form == token:
                lemmas.add(token)
                break
    return sorted(lemmas)


def better(candidate, incumbent, lemma=None):
    """Deterministic winner for two spellings of the same cell.

    The dictionary lexeme mixes paradigms: the superlative («свободнейший»)
    collides with the base form on every full cell, and prefixed
    comparatives («посвязаннее») pollute the comparative. Rules, in order:
    the lemma itself always wins `nom_m`; otherwise the shortest form wins
    (base forms are always shorter than their superlative/prefixed
    variants); remaining ties break lexicographically.
    """
    if incumbent is None:
        return candidate
    if lemma is not None and candidate == lemma:
        return candidate
    if lemma is not None and incumbent == lemma:
        return incumbent
    if len(candidate) != len(incumbent):
        return candidate if len(candidate) < len(incumbent) else incumbent
    return min(candidate, incumbent)


def forms_for(morph, lemma):
    adjective = None
    for parse in morph.parse(lemma):
        if parse.word == lemma and str(parse.tag.POS) == "ADJF":
            adjective = parse
            break
    if adjective is None:
        return None

    forms = {}
    for form in adjective.lexeme:
        tag = form.tag
        pos = str(tag.POS)
        case = CASES.get(str(tag.case) if tag.case else "")
        number = NUMBERS.get(str(tag.number) if tag.number else "")
        gender = GENDERS.get(str(tag.gender) if tag.gender else "")
        if pos == "ADJS" and number in ("sg", "pl"):
            if number == "pl":
                key = "short_pl"
            elif gender:
                key = f"short_{gender}"
            else:
                continue
        elif pos == "ADJF" and case is not None and number:
            if number == "pl":
                key = f"{case}_pl"
            elif gender:
                key = f"{case}_{gender}"
            else:
                continue
        else:
            continue
        forms[key] = better(form.word, forms.get(key), lemma=lemma if key == "nom_m" else None)

    # A usable entry carries the masculine nominative (= the lemma) and most
    # of the full-form grid; sparse fragments are dropped.
    full_cells = sum(1 for key in forms if not key.startswith(("short", "comp")))
    if forms.get("nom_m") != lemma or full_cells < 20:
        return None
    return {"lemma": lemma, "pos": "adj", "forms": forms}


def main():
    morph = pymorphy3.MorphAnalyzer()
    lemmas = adjective_lemmas(morph)
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
        "files": {"adjective_lexemes.json": digest},
        "notes": [
            "wordfreq ranking unavailable at generation time; the lexicon "
            "is complete rather than frequency-capped",
            "qualitative adjectives additionally carry short_{m,f,n,pl} and "
            "comp cells; relational adjectives carry the full-form grid only",
            "when one cell has two spellings (ё/е), the ё spelling wins and "
            "remaining ties break lexicographically",
        ],
    }
    MANIFEST.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )

    print(f"lemmas scanned: {len(lemmas)}", file=sys.stderr)
    print(f"entries written: {len(entries)}", file=sys.stderr)
    print(f"sha256:{digest}", file=sys.stderr)
    for probe in ["внутренний", "необратимый", "свободный", "культурный", "моральный"]:
        match = next((e for e in entries if e["lemma"] == probe), None)
        if match:
            print(
                f"{probe}: short_m={match['forms'].get('short_m')} "
                f"prep_f={match['forms'].get('prep_f')} "
                f"nom_m={match['forms'].get('nom_m')}",
                file=sys.stderr,
            )
        else:
            print(f"{probe}: MISSING", file=sys.stderr)


if __name__ == "__main__":
    main()

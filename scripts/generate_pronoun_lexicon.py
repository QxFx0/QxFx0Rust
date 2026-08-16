#!/usr/bin/env python3
"""Generate the embedded Russian pronoun lexicon from the pymorphy3 dictionary.

Pronouns are a closed class, but the class still declines: personal pronouns
carry a six-case paradigm, adjective-type pronouns (possessive,
demonstrative, determinative, relative) carry gendered full forms. Rather
than hand-maintaining the tables — and risking a drift between the Rust and
Python views of the same class — this generator materializes every NPRO
lemma straight from the dictionary.

Cell keys follow the adjective scheme where gender is distinguished:

  {case}_{sg,pl}          ungendered paradigm (я, ты, мы, вы, себя, кто, что…)
  {case}_{sg}_{m,f,n}     gendered forms of adjective-type pronouns
  {case}_pl               plural shared by all genders

Output: data/pronoun_lexemes.json (deterministic) and
data/pronoun_manifest.json (SHA-256 + provenance).
"""

import hashlib
import json
import sys
from pathlib import Path

import pymorphy3

REPO = Path(__file__).resolve().parents[1]
OUT = REPO / "data" / "pronoun_lexemes.json"
MANIFEST = REPO / "data" / "pronoun_manifest.json"

SCHEMA = "qxfx0:pronoun-lexicon:v1"

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

# Pronoun lemmas whose parse is dominated by a homonymous POS (её/его are
# also ADJF possessives, сам is also ADJF). They are pronouns in the closed
# class sense and must stay in this lexicon regardless of parse ordering.
FORCE_LEMMAS = frozenset(
    """я ты он она оно мы вы они себя мой твой свой наш ваш его её их
    этот тот такой этак столько весь всякий каждый сам самый кто что
    какой который чей сколько никто ничто никакой ничей кое-кто кое-что
    кто-то что-то кто-либо что-либо кто-нибудь что-нибудь какой-то
    какая-то какое-то какие-то какой-либо какой-нибудь чей-то чей-либо
    чей-нибудь""".split()
)


def pronoun_lemmas(morph):
    """All NPRO lemmas from the dictionary DAWG, plus forced closed-class ones."""
    lemmas = set()
    for entry in morph.dictionary.words.keys():
        token = entry.split()[0]
        for parse in morph.parse(token):
            if str(parse.tag.POS) == "NPRO" and parse.normal_form == token:
                lemmas.add(token)
                break
    lemmas.update(FORCE_LEMMAS)
    return sorted(lemmas)


# Invariable possessive pronouns: one surface serves every case cell.
INVARIABLE_LEMMAS = frozenset("его её их".split())


def forms_for(morph, lemma):
    if lemma in INVARIABLE_LEMMAS:
        forms = {}
        for case in CASES.values():
            for number in ("sg", "pl"):
                forms[f"{case}_{number}"] = lemma
        return {"lemma": lemma, "pos": "pron", "forms": forms}

    pronoun = None
    for parse in morph.parse(lemma):
        if parse.word == lemma and str(parse.tag.POS) in ("NPRO", "ADJF"):
            if parse.normal_form == lemma or lemma in FORCE_LEMMAS:
                pronoun = parse
                break
    if pronoun is None:
        return None

    forms = {}
    for form in pronoun.lexeme:
        tag = form.tag
        case = CASES.get(str(tag.case) if tag.case else "")
        number = NUMBERS.get(str(tag.number) if tag.number else "")
        gender = GENDERS.get(str(tag.gender) if tag.gender else "")
        if case is None or number is None:
            continue
        if gender:
            key = f"{case}_{number}_{gender}"
        else:
            key = f"{case}_{number}"
        incumbent = forms.get(key)
        if incumbent is None or form.word < incumbent:
            forms[key] = form.word

    # «себя» has no nominative; a real paradigm carries several cells.
    if len(forms) < 4:
        return None
    return {"lemma": lemma, "pos": "pron", "forms": forms}


def main():
    morph = pymorphy3.MorphAnalyzer()
    lemmas = pronoun_lemmas(morph)
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
        "files": {"pronoun_lexemes.json": digest},
        "notes": [
            "closed class materialized from the dictionary instead of "
            "hand-maintained tables, so Rust and Python share one source",
            "possessive его/её/их are invariable and carry a single form",
        ],
    }
    MANIFEST.write_text(
        json.dumps(manifest, ensure_ascii=False, indent=1) + "\n", encoding="utf-8"
    )

    print(f"lemmas scanned: {len(lemmas)}", file=sys.stderr)
    print(f"entries written: {len(entries)}", file=sys.stderr)
    print(f"sha256:{digest}", file=sys.stderr)
    for probe in ["я", "себя", "весь", "сам", "который", "его"]:
        match = next((e for e in entries if e["lemma"] == probe), None)
        if match:
            keys = sorted(match["forms"])
            print(f"{probe}: {len(keys)} cells, e.g. "
                  f"{keys[0]}={match['forms'][keys[0]]}", file=sys.stderr)
        else:
            print(f"{probe}: MISSING", file=sys.stderr)


if __name__ == "__main__":
    main()

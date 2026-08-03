#!/usr/bin/env python3
"""
Convert Haskell morphology data to Rust format.

Input: Haskell paradigms.json and exceptions.json
Output: Rust-compatible lexemes.json and manifest.json
"""

import argparse
import json
import hashlib
import subprocess
from pathlib import Path

REPO_DIR = Path(__file__).resolve().parents[1]
DEFAULT_HASKELL_REPO = REPO_DIR.parent / "my-haskell-project" / "QxFx0"

DEFAULT_SOURCE_TIER = "curated"
DEFAULT_QUALITY = 1.0


def convert_pos(pos_str):
    if pos_str is None:
        return "noun"
    mapping = {
        "Noun": "noun",
        "Adj": "adjective",
        "Verb": "verb",
        "Adv": "adverb",
        "Pron": "pronoun",
        "Prep": "preposition",
        "Conj": "conjunction",
        "Interj": "interjection",
        "Part": "particle",
        "Num": "numeral",
    }
    return mapping.get(pos_str, "other")


def convert_gender(gender_str):
    if gender_str is None:
        return "unknown"
    mapping = {
        "masc": "masculine",
        "femn": "feminine",
        "neut": "neuter",
    }
    return mapping.get(gender_str, "unknown")


def convert_animacy(animacy_str):
    if animacy_str is None:
        return "unknown"
    mapping = {
        "anim": "animate",
        "inan": "inanimate",
    }
    return mapping.get(animacy_str, "unknown")


def convert_forms(haskell_forms):
    case_map = {
        "NomSg": "nom_sg",
        "NomPl": "nom_pl",
        "GenSg": "gen_sg",
        "GenPl": "gen_pl",
        "DatSg": "dat_sg",
        "DatPl": "dat_pl",
        "AccSg": "acc_sg",
        "AccPl": "acc_pl",
        "InsSg": "ins_sg",
        "InsPl": "ins_pl",
        "LocSg": "prep_sg",
        "LocPl": "prep_pl",
    }
    rust_forms = {}
    for h_key, h_value in haskell_forms.items():
        rust_key = case_map.get(h_key)
        if rust_key:
            rust_forms[rust_key] = h_value
    return rust_forms


def create_lexeme_entry(lemma, paradigm_data, source_tier=DEFAULT_SOURCE_TIER, quality=DEFAULT_QUALITY):
    haskell_forms = paradigm_data.get("forms", {})
    rust_forms = convert_forms(haskell_forms)
    nom_sg = haskell_forms.get("NomSg", lemma)
    return {
        "lemma": nom_sg,
        "pos": convert_pos(paradigm_data.get("pos")),
        "gender": convert_gender(paradigm_data.get("gender")),
        "animacy": convert_animacy(paradigm_data.get("animacy")),
        "source_tier": source_tier,
        "quality": quality,
        "forms": rust_forms
    }


def load_haskell_data(haskell_dir):
    paradigms_path = haskell_dir / "paradigms.json"
    exceptions_path = haskell_dir / "exceptions.json"
    with open(paradigms_path, 'r', encoding='utf-8') as f:
        paradigms = json.load(f)
    with open(exceptions_path, 'r', encoding='utf-8') as f:
        exceptions = json.load(f)
    return paradigms, exceptions


def compute_sha256(filepath):
    sha256 = hashlib.sha256()
    with open(filepath, 'rb') as f:
        while True:
            data = f.read(8192)
            if not data:
                break
            sha256.update(data)
    return sha256.hexdigest()


def get_haskell_commit(haskell_repo):
    result = subprocess.run(
        ["git", "rev-parse", "HEAD"],
        cwd=haskell_repo,
        capture_output=True,
        text=True
    )
    if result.returncode == 0:
        return result.stdout.strip()
    return "unknown"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--haskell-repo", type=Path, default=DEFAULT_HASKELL_REPO)
    parser.add_argument("--output-dir", type=Path, default=REPO_DIR / "data")
    args = parser.parse_args()
    haskell_repo = args.haskell_repo.resolve()
    haskell_dir = haskell_repo / "resources" / "morphology"
    output_dir = args.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)

    print("Loading Haskell morphology data...")
    paradigms, exceptions = load_haskell_data(haskell_dir)

    print(f"Converting {len(paradigms)} paradigms...")
    lexemes = []
    for lemma, paradigm_data in paradigms.items():
        entry = create_lexeme_entry(lemma, paradigm_data)
        lexemes.append(entry)

    print(f"Converting {len(exceptions)} exceptions...")
    for lemma, exception_data in exceptions.items():
        entry = create_lexeme_entry(lemma, exception_data, source_tier="reviewed", quality=0.9)
        lexemes.append(entry)

    lexemes.sort(key=lambda x: x["lemma"])

    seen_lemmas = set()
    unique_lexemes = []
    for entry in lexemes:
        lemma = entry["lemma"]
        if lemma not in seen_lemmas:
            seen_lemmas.add(lemma)
            unique_lexemes.append(entry)
        else:
            print(f"Warning: duplicate lemma '{lemma}', skipping")

    print(f"Total unique lexemes: {len(unique_lexemes)}")

    lexemes_path = output_dir / "lexemes.json"
    with open(lexemes_path, 'w', encoding='utf-8') as f:
        json.dump(unique_lexemes, f, ensure_ascii=False, indent=2)
    print(f"Saved {len(unique_lexemes)} lexemes to {lexemes_path}")

    haskell_commit = get_haskell_commit(haskell_repo)
    lexemes_sha = compute_sha256(lexemes_path)

    manifest = {
        "bundle_version": 1,
        "source_repository": "QxFx0",
        "source_commit": haskell_commit,
        "license": "MIT",
        "created_at": "2026-07-29T16:55:56.251430+00:00",
        "files": {
            "lexemes.json": lexemes_sha
        }
    }

    manifest_path = output_dir / "manifest.json"
    with open(manifest_path, 'w', encoding='utf-8') as f:
        json.dump(manifest, f, ensure_ascii=False, indent=2)

    print(f"Saved manifest to {manifest_path}")
    print(f"Source commit: {haskell_commit}")
    print(f"Lexemes SHA-256: {lexemes_sha}")

    print("\nStatistics:")
    print(f"  Total lexemes: {len(unique_lexemes)}")

    pos_counts = {}
    for entry in unique_lexemes:
        pos = entry["pos"]
        pos_counts[pos] = pos_counts.get(pos, 0) + 1
    print(f"  By POS: {pos_counts}")

    gender_counts = {}
    for entry in unique_lexemes:
        gender = entry["gender"]
        gender_counts[gender] = gender_counts.get(gender, 0) + 1
    print(f"  By gender: {gender_counts}")

    animacy_counts = {}
    for entry in unique_lexemes:
        animacy = entry["animacy"]
        animacy_counts[animacy] = animacy_counts.get(animacy, 0) + 1
    print(f"  By animacy: {animacy_counts}")

    tier_counts = {}
    for entry in unique_lexemes:
        tier = entry["source_tier"]
        tier_counts[tier] = tier_counts.get(tier, 0) + 1
    print(f"  By source tier: {tier_counts}")


if __name__ == "__main__":
    main()

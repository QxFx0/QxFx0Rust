#!/usr/bin/env python3
"""F0 census: emit the fingerprinted template-agreement-matrix (ADR-0034 §10).

The matrix is one of two independent Phase A gate inputs. It is never merged
with `response-plan-v2-audited-corpus`: this one certifies that a *template*
agrees correctly with a grammatical feature bundle, the other certifies that a
*topic* renders with the right semantics and authority.

Rows are template x compatible fixture, not template x topic. A doctor
cross-product over the 30 audited topics would be largely inapplicable, since
most templates never co-occur with most topics.

`parity_class` decides what the Phase A gate demands of each row:

  byte      the template carries no agreement feature, so the V2 realization
            must reproduce the V1 surface byte for byte;
  semantic  the template agrees with its subject, so a principled generator
            may legitimately produce a different (correct) string; the gate
            checks semantics plus an approved golden surface.

Counts such as "120/127" are diagnostic output of this census, not contract
values (ADR-0034 §10).

Usage:
    python3 scripts/f0_template_agreement_census.py            # write manifest
    python3 scripts/f0_template_agreement_census.py --check    # verify only
"""

from __future__ import annotations

import argparse
import hashlib
import json
import pathlib
import re
import sys

REPO = pathlib.Path(__file__).resolve().parent.parent
TEMPLATES = REPO / "data" / "semantic" / "templates" / "templates.json"
LEXEMES = REPO / "data" / "lexemes.json"
OUT_DIR = REPO / "data" / "gates" / "response-plan-v2"
MATRIX = OUT_DIR / "template-agreement-matrix.json"

SCHEMA_VERSION = 1
MATRIX_ID = "template-agreement-matrix-v1"

# Explicit agreement slot: {FROM_G:masc,fem,neut,plur}. Index order is fixed by
# syntactic_generator::fill_gender_slot and is part of this contract.
GENDER_SLOT = re.compile(r"\{(FROM|TO|OBJ)_G:([^}]*)\}")

# One fixture per grammatical gender. Lemmas are drawn from the curated bundle
# so the census never invents morphology.
FIXTURES = [
    ("fixture.masc", "m", "разум"),
    ("fixture.fem", "f", "свобода"),
    ("fixture.neut", "n", "бытие"),
]


def sha256_hex(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def canonical_digest(payload: object) -> str:
    blob = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return sha256_hex(blob.encode("utf-8"))


def load_gender_map() -> dict[str, str]:
    short = {"masculine": "m", "feminine": "f", "neuter": "n", "unknown": "?"}
    return {
        entry["lemma"]: short.get(entry["gender"], "?")
        for entry in json.loads(LEXEMES.read_text(encoding="utf-8"))
    }


def classify(pattern: str) -> tuple[str, str]:
    """Return (parity_class, reason) for one template."""
    slots = GENDER_SLOT.findall(pattern)
    if slots:
        arities = {len(forms.split(",")) for _, forms in slots}
        return (
            "semantic",
            f"explicit agreement slot; arities={sorted(arities)}",
        )
    return ("byte", "no agreement feature in surface")


def build() -> dict:
    templates = json.loads(TEMPLATES.read_text(encoding="utf-8"))
    genders = load_gender_map()

    rows: list[dict] = []
    for relation_type in sorted(templates):
        for template_index, template in enumerate(templates[relation_type]):
            pattern = template["pattern"]
            parity_class, reason = classify(pattern)
            for fixture_id, fixture_gender, lemma in FIXTURES:
                # A fixture is only meaningful when the bundle agrees that the
                # lemma really has that gender; otherwise the row would test
                # the fixture, not the template.
                if genders.get(lemma) != fixture_gender:
                    raise SystemExit(
                        f"fixture {fixture_id} expects {fixture_gender} for "
                        f"{lemma}, bundle says {genders.get(lemma)}"
                    )
                rows.append(
                    {
                        "relation_type": relation_type,
                        "template_index": template_index,
                        "fixture_id": fixture_id,
                        "fixture_gender": fixture_gender,
                        "fixture_lemma": lemma,
                        "parity_class": parity_class,
                        "reason": reason,
                        # Filled by the Rust gate, which owns realization.
                        # The census fixes *what* must be checked, not the
                        # surface itself: a surface digest recorded here would
                        # duplicate renderer authority in a Python script.
                        "expected_surface_digest": None,
                    }
                )

    by_class: dict[str, int] = {}
    for row in rows:
        by_class[row["parity_class"]] = by_class.get(row["parity_class"], 0) + 1

    template_count = sum(len(v) for v in templates.values())
    byte_templates = sum(
        1
        for v in templates.values()
        for t in v
        if classify(t["pattern"])[0] == "byte"
    )

    manifest = {
        "schema_version": SCHEMA_VERSION,
        "matrix_id": MATRIX_ID,
        "source_files": {
            "templates.json": sha256_hex(TEMPLATES.read_bytes()),
        },
        "fixtures": [
            {"fixture_id": f, "gender": g, "lemma": lemma} for f, g, lemma in FIXTURES
        ],
        "diagnostics": {
            "templates_total": template_count,
            "relation_types": len(templates),
            "templates_parity_byte": byte_templates,
            "templates_parity_semantic": template_count - byte_templates,
            "rows_total": len(rows),
            "rows_by_parity_class": by_class,
        },
        "rows": rows,
    }
    manifest["matrix_digest"] = canonical_digest(
        {"rows": rows, "fixtures": manifest["fixtures"]}
    )
    return manifest


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="verify, do not write")
    args = parser.parse_args()

    manifest = build()
    blob = json.dumps(manifest, ensure_ascii=False, indent=2, sort_keys=True) + "\n"

    if args.check:
        if not MATRIX.exists():
            print(f"missing {MATRIX}", file=sys.stderr)
            return 1
        current = MATRIX.read_text(encoding="utf-8")
        if current != blob:
            print("template-agreement-matrix is stale; re-run without --check", file=sys.stderr)
            return 1
        print(f"matrix current: {manifest['matrix_digest']}")
        return 0

    OUT_DIR.mkdir(parents=True, exist_ok=True)
    MATRIX.write_text(blob, encoding="utf-8")
    diag = manifest["diagnostics"]
    print(f"wrote {MATRIX.relative_to(REPO)}")
    print(f"  templates      {diag['templates_total']} across {diag['relation_types']} relation types")
    print(f"  parity=byte    {diag['templates_parity_byte']} templates")
    print(f"  parity=semantic{diag['templates_parity_semantic']:>4} templates")
    print(f"  rows           {diag['rows_total']}")
    print(f"  matrix_digest  {manifest['matrix_digest']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

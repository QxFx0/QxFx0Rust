#!/usr/bin/env python3
"""Generate the committed census artifact `data/census.json` from `qxfx0 doctor --json`.

The census is the single committed record of every content-bearing count the
binary carries (seed graph, content plan, templates, lexicons, code registry,
knowledge pack, fact registry). A content or morphology wave that changes any
of these numbers must regenerate the artifact and commit it together with the
change — `--check` makes forgotten regeneration fail CI.

Usage:
    python3 scripts/generate_census.py [--binary target/release/qxfx0]
    python3 scripts/generate_census.py --check

The binary must be built from the current tree (`cargo build --release
-p qxfx0-cli`); doctor output is a pure function of the embedded assets.
"""

from __future__ import annotations

import argparse
import json
import pathlib
import subprocess
import sys
import tempfile

REPO_ROOT = pathlib.Path(__file__).resolve().parent.parent
ARTIFACT_PATH = REPO_ROOT / "data" / "census.json"
DEFAULT_BINARY = REPO_ROOT / "target" / "release" / "qxfx0"

# Doctor checks whose details carry content census numbers. Prose-only checks
# (SQLite, diagnostics channel, perspective/stance contracts) are excluded on
# purpose: they must not block a content wave, and they carry no counts.
CENSUS_CHECKS = [
    "Seed graph",
    "Content plan assets",
    "Templates",
    "Verb lexicon",
    "Adjective lexicon",
    "Pronoun lexicon",
    "Code registry",
    "Knowledge pack",
    "Curated FactRegistry",
]


def collect_census(binary: pathlib.Path) -> dict:
    with tempfile.TemporaryDirectory(prefix="qxfx0-census-") as tmp:
        db = pathlib.Path(tmp) / "census.db"
        result = subprocess.run(
            [str(binary), "--db", str(db), "doctor", "--json"],
            capture_output=True,
            text=True,
        )
    if result.returncode != 0:
        sys.exit(f"doctor failed ({result.returncode}): {result.stderr.strip()}")
    report = json.loads(result.stdout)
    checks = {check["name"]: check for check in report.get("checks", [])}
    census = {}
    for name in CENSUS_CHECKS:
        check = checks.get(name)
        if check is None:
            sys.exit(f"doctor report is missing the census check '{name}'")
        if not check.get("passed"):
            sys.exit(f"doctor check '{name}' did not pass; census refuses to record a red gate")
        census[name] = check["details"]
    return {
        "schema": "qxfx0.census.v1",
        "source": "qxfx0 doctor --json (regenerate after any content/morphology wave)",
        "checks": census,
    }


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--binary",
        type=pathlib.Path,
        default=DEFAULT_BINARY,
        help="qxfx0 release binary built from the current tree",
    )
    parser.add_argument(
        "--check",
        action="store_true",
        help="verify the committed artifact matches the binary; exit 1 on drift",
    )
    args = parser.parse_args()

    if not args.binary.is_file():
        sys.exit(f"binary not found: {args.binary} (build with cargo build --release -p qxfx0-cli)")

    census = collect_census(args.binary)
    encoded = json.dumps(census, ensure_ascii=False, indent=2, sort_keys=True) + "\n"

    if args.check:
        committed = ARTIFACT_PATH.read_text(encoding="utf-8")
        if committed == encoded:
            print("census OK: data/census.json matches the binary")
            return 0
        print("census DRIFT: data/census.json does not match the binary.")
        print("Regenerate and commit together with the content change:")
        print("  cargo build --release -p qxfx0-cli && python3 scripts/generate_census.py")
        committed_census = json.loads(committed)
        for name in CENSUS_CHECKS:
            old = committed_census.get("checks", {}).get(name)
            new = census["checks"][name]
            if old != new:
                print(f"  {name}:\n    committed: {old}\n    binary:    {new}")
        return 1

    ARTIFACT_PATH.write_text(encoded, encoding="utf-8")
    print(f"census written: {ARTIFACT_PATH}")
    for name, details in census["checks"].items():
        print(f"  {name}: {details}")
    return 0


if __name__ == "__main__":
    sys.exit(main())

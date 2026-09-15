#!/usr/bin/env python3
"""Validate a salience-hemisphere label corpus pair (ADR-0045 C3).

Checks the mechanical contract; human judgments (label correctness,
reason quality) stay human. Exit nonzero with the first violation
class found; prints measured stats either way.

Usage:
    python3 scripts/validate_label_corpus.py \
        data/eval/salience-hemisphere-v1/train.tsv \
        data/eval/salience-hemisphere-v1/dev.tsv
"""
import sys

CLASSES = ("holistic", "formal")
BORDERLINE_MARK = "пограничн"


def load(path):
    rows = []
    with open(path, encoding="utf-8") as handle:
        content = handle.read()
    if "\r" in content:
        return None, [f"{path}: CRLF line endings (need LF)"]
    if content.startswith("\ufeff"):
        return None, [f"{path}: BOM found"]
    errors = []
    for number, line in enumerate(content.split("\n"), 1):
        if not line.strip():
            continue
        columns = line.split("\t")
        if len(columns) != 3:
            errors.append(f"{path}:{number}: want 3 tab columns, got {len(columns)}")
            continue
        prompt, label, reason = (column.strip() for column in columns)
        if not prompt:
            errors.append(f"{path}:{number}: empty prompt")
        if label not in CLASSES:
            errors.append(f"{path}:{number}: label {label!r} not in {CLASSES}")
        if not reason:
            errors.append(f"{path}:{number}: empty reason")
        rows.append((prompt, label, reason))
    return rows, errors


def words(prompt):
    return len(prompt.split())


def main(train_path, dev_path):
    failures = []
    train, errors = load(train_path)
    failures.extend(errors)
    dev, errors = load(dev_path)
    failures.extend(errors)
    if failures:
        print("\n".join(failures))
        return 1
    for name, rows in (("train", train), ("dev", dev)):
        counts = {label: sum(1 for row in rows if row[1] == label) for label in CLASSES}
        print(f"{name}: rows={len(rows)} balance={counts}")
        borderline = sum(1 for row in rows if BORDERLINE_MARK in row[2])
        print(f"{name}: borderline={borderline}")
        strata = {"short": 0, "mid": 0, "long": 0}
        for prompt, _, _ in rows:
            count = words(prompt)
            if count <= 4:
                strata["short"] += 1
            elif count <= 15:
                strata["mid"] += 1
            else:
                strata["long"] += 1
        print(f"{name}: length={strata} (informational: dialogue turns skew short)")
    for name, rows in (("train", train), ("dev", dev)):
        prompts = [row[0] for row in rows]
        if len(set(prompts)) != len(prompts):
            failures.append(f"{name}: duplicate prompts inside the set")
    overlap = set(row[0] for row in train) & set(row[0] for row in dev)
    if overlap:
        failures.append(f"train/dev prompt overlap: {sorted(overlap)[:5]}")
    if failures:
        print("\n".join(failures))
        return 1
    print("OK: format, balance data present, no dups, no overlap")
    return 0


if __name__ == "__main__":
    if len(sys.argv) != 3:
        print(__doc__)
        sys.exit(2)
    sys.exit(main(sys.argv[1], sys.argv[2]))

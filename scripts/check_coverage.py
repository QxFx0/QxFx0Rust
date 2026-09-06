#!/usr/bin/env python3
"""Fail-under gate for line coverage.

Parses the `lcov.info` produced by
`cargo llvm-cov --locked --workspace --all-targets --lcov
--output-path coverage/lcov.info` and fails when the workspace line
coverage drops below the floor. The floor lives here (not in CI yaml)
so the policy and its parser change in one commit.

Override for local experiments: QXFX0_MIN_COVERAGE=80 python3
scripts/check_coverage.py (CI never sets the override).
"""

import os
import re
import sys

LCOV_PATH = os.path.join(
    os.path.dirname(os.path.dirname(os.path.abspath(__file__))),
    "coverage",
    "lcov.info",
)

# Calibrated 2026-09: workspace baseline ~87.75% (waves 3-4 + robustness
# gates included), floor 86% leaves room for line-count churn while
# catching a real coverage drop.
MIN_LINE_COVERAGE = 86.0


def line_coverage(path: str) -> tuple[int, int]:
    found = 0
    hit = 0
    with open(path, encoding="utf-8") as handle:
        for line in handle:
            match = re.match(r"^DA:(\d+),(\d+)$", line.strip())
            if match:
                found += 1
                if int(match.group(2)) > 0:
                    hit += 1
    return found, hit


def main() -> int:
    floor = float(os.environ.get("QXFX0_MIN_COVERAGE", str(MIN_LINE_COVERAGE)))
    try:
        found, hit = line_coverage(LCOV_PATH)
    except FileNotFoundError:
        print(f"check_coverage: missing {LCOV_PATH}; run cargo llvm-cov first")
        return 1
    if found == 0:
        print("check_coverage: no executable lines recorded; refusing empty report")
        return 1
    pct = 100.0 * hit / found
    print(f"check_coverage: lines {hit}/{found} = {pct:.2f}% (floor {floor:.2f}%)")
    if pct < floor:
        print(f"check_coverage: FAIL: coverage {pct:.2f}% below floor {floor:.2f}%")
        return 1
    print("check_coverage: OK")
    return 0


if __name__ == "__main__":
    sys.exit(main())

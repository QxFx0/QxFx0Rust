#!/usr/bin/env python3
"""Measure full cold-process latency and peak RSS without GNU time."""

import argparse
import hashlib
import json
import math
import platform
import subprocess
import tempfile
import time
from pathlib import Path


def parse_args():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", default="target/release/qxfx0")
    parser.add_argument("--samples", type=int, default=10)
    parser.add_argument("--text", default="что такое свобода?")
    parser.add_argument("--poll-ms", type=float, default=1.0)
    return parser.parse_args()


def nearest_rank(values, percentile):
    ordered = sorted(values)
    rank = max(1, math.ceil(percentile * len(ordered) / 100))
    return ordered[rank - 1]


def distribution(values):
    return {
        "samples": len(values),
        "min": min(values),
        "p50": nearest_rank(values, 50),
        "p95": nearest_rank(values, 95),
        "max": max(values),
        "mean": sum(values) / len(values),
    }


def process_rss_bytes(pid):
    try:
        status = Path(f"/proc/{pid}/status").read_text(encoding="utf-8")
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        return None
    for line in status.splitlines():
        if line.startswith("VmRSS:"):
            return int(line.split()[1]) * 1024
    return None


def sha256_file(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for chunk in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def run_sample(binary, db_path, session_id, text, poll_seconds):
    command = [
        str(binary),
        "--db",
        str(db_path),
        "--session-id",
        session_id,
        "turn",
        text,
    ]
    started = time.perf_counter_ns()
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    peak_rss = 0
    while process.poll() is None:
        rss = process_rss_bytes(process.pid)
        if rss is not None:
            peak_rss = max(peak_rss, rss)
        time.sleep(poll_seconds)
    stdout, stderr = process.communicate()
    elapsed_ms = (time.perf_counter_ns() - started) / 1_000_000
    if process.returncode != 0:
        raise RuntimeError(
            f"sample process exited {process.returncode}: {stderr.strip()}"
        )
    if not stdout.strip():
        raise RuntimeError("sample process returned an empty response")
    return elapsed_ms, peak_rss or None


def main():
    args = parse_args()
    if not 1 <= args.samples <= 1000:
        raise SystemExit("--samples must be between 1 and 1000")
    if not 0.1 <= args.poll_ms <= 1000:
        raise SystemExit("--poll-ms must be between 0.1 and 1000")
    repository = Path(__file__).resolve().parent.parent
    binary = Path(args.binary)
    if not binary.is_absolute():
        binary = repository / binary
    binary = binary.resolve()
    if not binary.is_file():
        raise SystemExit(f"release binary not found: {binary}")

    latencies = []
    peak_rss_values = []
    sample_reports = []
    with tempfile.TemporaryDirectory(prefix="qxfx0-cold-benchmark-") as temp_dir:
        for index in range(args.samples):
            elapsed_ms, peak_rss = run_sample(
                binary,
                Path(temp_dir) / f"sample-{index}.db",
                f"cold-process-{index}",
                args.text,
                args.poll_ms / 1000,
            )
            latencies.append(elapsed_ms)
            if peak_rss is not None:
                peak_rss_values.append(peak_rss)
            sample_reports.append(
                {
                    "index": index,
                    "latency_ms": elapsed_ms,
                    "peak_rss_bytes": peak_rss,
                }
            )

    report = {
        "schema_version": 1,
        "benchmark": "qxfx0-cold-process",
        "note": "Each sample is a new process; filesystem page cache is not flushed.",
        "platform": platform.platform(),
        "binary": str(binary),
        "binary_bytes": binary.stat().st_size,
        "binary_sha256": sha256_file(binary),
        "morphology_lexemes_bytes": (
            repository / "data/lexemes.json"
        ).stat().st_size,
        "latency_ms": distribution(latencies),
        "peak_rss_bytes": distribution(peak_rss_values)
        if peak_rss_values
        else None,
        "samples": sample_reports,
    }
    print(json.dumps(report, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

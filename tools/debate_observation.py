#!/usr/bin/env python3

import argparse
import collections
from contextlib import closing
import hashlib
import json
import os
from pathlib import Path
import re
import sqlite3
import subprocess
import tempfile

ROOT = Path(__file__).resolve().parents[1]
DEFAULT_MANIFEST = ROOT / "data/gates/debate-core/observation-corpus-v1.json"
ID_PATTERN = re.compile(r"^[a-z0-9][a-z0-9_-]{0,63}$")
REQUIRED_INPUT_CLASSES = {
    "definition",
    "assertion",
    "challenge",
    "distinction",
    "grounding_request",
    "counterargument",
    "consequence",
    "topic_connection",
    "reflection",
    "clarification",
    "greeting",
    "unknown_topic",
    "repeated_unknown_topic",
    "external_subject",
    "guard_blocked",
    "fallback_plan",
}
FORBIDDEN_RECEIPT_FIELDS = {"raw_text", "response", "session_id", "request_id"}


class ValidationError(Exception):
    pass


def load_json(path):
    try:
        with Path(path).open(encoding="utf-8") as source:
            return json.load(source)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"invalid JSON in {path}: {error}") from error


def validate_id(field, value):
    if not isinstance(value, str) or not ID_PATTERN.fullmatch(value):
        raise ValidationError(f"{field} is not a bounded stable identifier")


def reject_unknown_fields(label, value, allowed):
    unknown = sorted(set(value) - set(allowed))
    if unknown:
        raise ValidationError(f"{label} contains unknown fields: {unknown}")


def validate_manifest(manifest):
    if not isinstance(manifest, dict):
        raise ValidationError("observation corpus must be an object")
    reject_unknown_fields(
        "manifest",
        manifest,
        {
            "schema",
            "corpus_id",
            "authority_change",
            "raw_user_logs",
            "reviewed_formulations_only",
            "required_input_classes",
            "scenarios",
        },
    )
    if manifest.get("schema") != "qxfx0.debate-core-observation-corpus.v1":
        raise ValidationError("unsupported observation corpus schema")
    validate_id("corpus_id", manifest.get("corpus_id"))
    if manifest.get("authority_change") != "none":
        raise ValidationError("observation corpus cannot change authority")
    if manifest.get("raw_user_logs") is not False:
        raise ValidationError("raw_user_logs must be false")
    if manifest.get("reviewed_formulations_only") is not True:
        raise ValidationError("reviewed_formulations_only must be true")
    declared_classes = manifest.get("required_input_classes")
    if not isinstance(declared_classes, list) or set(declared_classes) != REQUIRED_INPUT_CLASSES:
        raise ValidationError("required_input_classes does not match the v1 contract")
    if len(declared_classes) != len(set(declared_classes)):
        raise ValidationError("required_input_classes contains duplicates")

    scenarios = manifest.get("scenarios")
    if not isinstance(scenarios, list) or not 10 <= len(scenarios) <= 32:
        raise ValidationError("observation corpus must contain 10-32 scenarios")
    scenario_ids = set()
    turn_ids = set()
    covered_classes = set()
    turns_total = 0
    for scenario in scenarios:
        if not isinstance(scenario, dict):
            raise ValidationError("scenario must be an object")
        reject_unknown_fields(
            "scenario",
            scenario,
            {"scenario_id", "input_class", "privacy_needles", "turns"},
        )
        scenario_id = scenario.get("scenario_id")
        validate_id("scenario_id", scenario_id)
        if scenario_id in scenario_ids:
            raise ValidationError(f"duplicate scenario_id '{scenario_id}'")
        scenario_ids.add(scenario_id)
        input_class = scenario.get("input_class")
        if input_class not in REQUIRED_INPUT_CLASSES:
            raise ValidationError(f"{scenario_id}: unknown input_class")
        covered_classes.add(input_class)
        needles = scenario.get("privacy_needles", [])
        if not isinstance(needles, list) or any(
            not isinstance(needle, str) or not needle.strip() for needle in needles
        ):
            raise ValidationError(f"{scenario_id}: invalid privacy_needles")
        turns = scenario.get("turns")
        if not isinstance(turns, list) or not 1 <= len(turns) <= 4:
            raise ValidationError(f"{scenario_id}: scenarios require 1-4 turns")
        turns_total += len(turns)
        for turn in turns:
            if not isinstance(turn, dict):
                raise ValidationError(f"{scenario_id}: turn must be an object")
            reject_unknown_fields(
                "turn",
                turn,
                {
                    "turn_id",
                    "utterance",
                    "expected_topic_id",
                    "expected_move",
                    "expected_guard_blocked",
                },
            )
            turn_id = turn.get("turn_id")
            validate_id("turn_id", turn_id)
            if turn_id in turn_ids:
                raise ValidationError(f"duplicate turn_id '{turn_id}'")
            turn_ids.add(turn_id)
            utterance = turn.get("utterance")
            if not isinstance(utterance, str) or len(utterance) > 512:
                raise ValidationError(f"{turn_id}: utterance must contain at most 512 characters")
            if not utterance.strip() and input_class != "guard_blocked":
                raise ValidationError(f"{turn_id}: only guard_blocked may use empty input")
            expected_guard_blocked = turn.get("expected_guard_blocked", False)
            if not isinstance(expected_guard_blocked, bool):
                raise ValidationError(f"{turn_id}: expected_guard_blocked must be boolean")
            if input_class == "guard_blocked" and not expected_guard_blocked:
                raise ValidationError(f"{turn_id}: guard_blocked must assert its guard result")
            topic_id = turn.get("expected_topic_id")
            move = turn.get("expected_move")
            if not isinstance(topic_id, str) or not topic_id.strip() or len(topic_id) > 256:
                raise ValidationError(f"{turn_id}: invalid expected_topic_id")
            if move not in {
                "define",
                "assert",
                "challenge",
                "distinguish",
                "ground",
                "counter",
                "infer_consequence",
                "clarify",
                "reflect",
                "connect",
                "contact",
                "other",
            }:
                raise ValidationError(f"{turn_id}: invalid expected_move")
    if covered_classes != REQUIRED_INPUT_CLASSES:
        missing = sorted(REQUIRED_INPUT_CLASSES - covered_classes)
        raise ValidationError(f"observation corpus misses input classes: {missing}")
    return {
        "scenarios_total": len(scenarios),
        "turns_total": turns_total,
    }


def sha256_bytes(value):
    return hashlib.sha256(value).hexdigest()


def sequence_digest(values):
    digest = hashlib.sha256(b"qxfx0.debate-observation-sequence.v1\0")
    for value in values:
        digest.update(len(value).to_bytes(8, "big"))
        digest.update(value)
    return digest.hexdigest()


def run_command(arguments, *, timeout=90):
    environment = dict(os.environ)
    environment.setdefault("RUST_LOG", "error")
    result = subprocess.run(
        [str(argument) for argument in arguments],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
        timeout=timeout,
        env=environment,
        cwd=ROOT,
    )
    if result.returncode != 0:
        diagnostic = result.stderr.decode("utf-8", errors="replace").strip()
        raise ValidationError(f"command failed ({result.returncode}): {diagnostic}")
    return result.stdout


def run_turn(binary, database, session_id, utterance, trace_path=None):
    arguments = [
        binary,
        "--db",
        database,
        "--session-id",
        session_id,
        "turn",
        utterance,
    ]
    if trace_path is not None:
        arguments.extend(["--debate-core-trace-jsonl", trace_path])
    return run_command(arguments)


def persisted_state(database, session_id):
    with closing(sqlite3.connect(database)) as connection:
        row = connection.execute(
            "SELECT state_json FROM runtime_sessions WHERE id = ?", (session_id,)
        ).fetchone()
    if row is None or not isinstance(row[0], str):
        raise ValidationError(f"session '{session_id}' has no persisted state")
    return row[0].encode("utf-8")


def forbidden_keys(value):
    found = set()
    if isinstance(value, dict):
        found.update(FORBIDDEN_RECEIPT_FIELDS.intersection(value))
        for child in value.values():
            found.update(forbidden_keys(child))
    elif isinstance(value, list):
        for child in value:
            found.update(forbidden_keys(child))
    return found


def verify_and_load_trace(binary, trace_path):
    run_command([binary, "verify-debate-trace", trace_path])
    encoded = Path(trace_path).read_bytes()
    try:
        record = json.loads(encoded)
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValidationError(f"invalid trace JSON: {error}") from error
    if record.get("schema") != "qxfx0.debate-core-trace.v1":
        raise ValidationError("trace schema mismatch")
    hidden = forbidden_keys(record)
    if hidden:
        raise ValidationError(f"trace contains forbidden fields: {sorted(hidden)}")
    return encoded, record["receipt"]


def receipt_digest(receipt):
    digest = receipt.get("digest")
    if not isinstance(digest, list) or len(digest) != 32 or any(
        not isinstance(byte, int) or not 0 <= byte <= 255 for byte in digest
    ):
        raise ValidationError("receipt digest is not a 32-byte array")
    return bytes(digest).hex()


def evidence_digest(root):
    digest = hashlib.sha256(b"qxfx0.debate-observation-evidence.v1\0")
    for path in sorted(Path(root).rglob("*.jsonl")):
        relative = path.relative_to(root).as_posix().encode("utf-8")
        payload = path.read_bytes()
        digest.update(len(relative).to_bytes(8, "big"))
        digest.update(relative)
        digest.update(len(payload).to_bytes(8, "big"))
        digest.update(payload)
    return digest.hexdigest()


def observe(binary, manifest_path, output_path, expected_sha=None):
    binary = Path(binary).resolve()
    manifest_path = Path(manifest_path).resolve()
    output_path = Path(output_path).resolve()
    manifest = load_json(manifest_path)
    inventory = validate_manifest(manifest)
    if not binary.is_file():
        raise ValidationError(f"binary does not exist: {binary}")
    if output_path.exists():
        raise ValidationError(f"output path already exists: {output_path}")
    output_path.parent.mkdir(parents=True, exist_ok=True)
    build_sha = run_command(["git", "rev-parse", "HEAD"]).decode().strip()
    if expected_sha is not None and build_sha != expected_sha:
        raise ValidationError(f"build SHA mismatch: expected {expected_sha}, got {build_sha}")
    version = run_command([binary, "version"]).decode("utf-8").strip()

    move_distribution = collections.Counter()
    topic_distribution = collections.Counter()
    graph_shapes = collections.Counter()
    input_classes = collections.Counter()
    rubric_scores = collections.defaultdict(collections.Counter)
    scenario_results = []
    receipts_generated = 0

    with tempfile.TemporaryDirectory(prefix="qxfx0-debate-observation-db-") as database_root:
        with tempfile.TemporaryDirectory(
            prefix=".qxfx0-debate-observation-", dir=output_path.parent
        ) as staging_root:
            staging = Path(staging_root)
            traces_root = staging / "traces"
            traces_root.mkdir()
            for scenario in manifest["scenarios"]:
                scenario_id = scenario["scenario_id"]
                session_id = f"debate-observation-{scenario_id}"
                baseline_db = Path(database_root) / f"{scenario_id}-baseline.db"
                observed_db = Path(database_root) / f"{scenario_id}-observed.db"
                replay_db = Path(database_root) / f"{scenario_id}-replay.db"
                baseline_outputs = []
                observed_outputs = []
                replay_outputs = []
                observed_traces = []
                replay_traces = []
                receipt_digests = []
                scenario_trace_root = traces_root / scenario_id
                scenario_trace_root.mkdir()

                for turn in scenario["turns"]:
                    baseline_outputs.append(
                        run_turn(
                            binary,
                            baseline_db,
                            session_id,
                            turn["utterance"],
                        )
                    )
                baseline_state = persisted_state(baseline_db, session_id)

                for turn in scenario["turns"]:
                    trace_path = scenario_trace_root / f"{turn['turn_id']}.jsonl"
                    output = run_turn(
                        binary,
                        observed_db,
                        session_id,
                        turn["utterance"],
                        trace_path,
                    )
                    encoded, receipt = verify_and_load_trace(binary, trace_path)
                    if receipt.get("topic_id") != turn["expected_topic_id"]:
                        raise ValidationError(f"{turn['turn_id']}: topic expectation failed")
                    if receipt.get("move_type") != turn["expected_move"]:
                        raise ValidationError(f"{turn['turn_id']}: move expectation failed")
                    if turn.get("expected_guard_blocked"):
                        current_state = json.loads(persisted_state(observed_db, session_id))
                        guard_status = current_state.get("last_turn_decision", {}).get(
                            "guard_status"
                        )
                        if not (
                            isinstance(guard_status, dict)
                            and "InvariantBlock" in guard_status
                        ):
                            raise ValidationError(
                                f"{turn['turn_id']}: expected a typed guard block"
                            )
                    privacy_values = [turn["utterance"], session_id, output.decode("utf-8").strip()]
                    privacy_values.extend(scenario.get("privacy_needles", []))
                    for value in privacy_values:
                        if value and value.encode("utf-8") in encoded:
                            raise ValidationError(f"{turn['turn_id']}: privacy boundary violation")
                    observed_outputs.append(output)
                    observed_traces.append(encoded)
                    receipt_digests.append(receipt_digest(receipt))
                    receipts_generated += 1
                    input_classes[scenario["input_class"]] += 1
                    move_distribution[receipt["move_type"]] += 1
                    topic_distribution[receipt["topic_id"]] += 1
                    graph_shapes[f"nodes_{len(receipt['nodes'])}_edges_{len(receipt['edges'])}"] += 1
                    for assessment in receipt["rubric"]:
                        rubric_scores[assessment["dimension"]][str(assessment["score"])] += 1
                observed_state = persisted_state(observed_db, session_id)

                for turn in scenario["turns"]:
                    replay_trace = Path(database_root) / f"{turn['turn_id']}-replay.jsonl"
                    replay_outputs.append(
                        run_turn(
                            binary,
                            replay_db,
                            session_id,
                            turn["utterance"],
                            replay_trace,
                        )
                    )
                    encoded, _ = verify_and_load_trace(binary, replay_trace)
                    replay_traces.append(encoded)
                replay_state = persisted_state(replay_db, session_id)

                if baseline_outputs != observed_outputs or observed_outputs != replay_outputs:
                    raise ValidationError(f"{scenario_id}: output parity violation")
                if baseline_state != observed_state or observed_state != replay_state:
                    raise ValidationError(f"{scenario_id}: state parity violation")
                if observed_traces != replay_traces:
                    raise ValidationError(f"{scenario_id}: deterministic replay violation")

                scenario_results.append(
                    {
                        "scenario_id": scenario_id,
                        "input_class": scenario["input_class"],
                        "turns": len(scenario["turns"]),
                        "output_digest": sequence_digest(observed_outputs),
                        "state_digest": sha256_bytes(observed_state),
                        "receipt_digests": receipt_digests,
                    }
                )

            report = {
                "schema": "qxfx0.debate-core-observation-report.v1",
                "corpus_id": manifest["corpus_id"],
                "build_sha": build_sha,
                "binary_version": version,
                "manifest_digest": sha256_bytes(manifest_path.read_bytes()),
                "evidence_digest": evidence_digest(staging),
                "authority_change": "none",
                "raw_user_logs": False,
                "reviewed_formulations_only": True,
                "scenarios_total": inventory["scenarios_total"],
                "turns_total": inventory["turns_total"],
                "receipts_generated": receipts_generated,
                "receipt_generation_rate_basis_points": (
                    receipts_generated * 10_000 // inventory["turns_total"]
                ),
                "validation_failures": 0,
                "replay_failures": 0,
                "privacy_violations": 0,
                "output_parity_violations": 0,
                "state_parity_violations": 0,
                "digest_mismatches": 0,
                "invalid_graph_references": 0,
                "input_class_distribution": dict(sorted(input_classes.items())),
                "move_distribution": dict(sorted(move_distribution.items())),
                "topic_distribution": dict(sorted(topic_distribution.items())),
                "graph_shape_distribution": dict(sorted(graph_shapes.items())),
                "rubric_score_distribution": {
                    dimension: dict(sorted(scores.items()))
                    for dimension, scores in sorted(rubric_scores.items())
                },
                "scenario_results": scenario_results,
            }
            (staging / "report.json").write_text(
                json.dumps(report, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
                encoding="utf-8",
            )
            Path(staging_root).replace(output_path)
            return report


def main():
    parser = argparse.ArgumentParser(description="Debate Core observation corpus harness")
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate = subparsers.add_parser("validate")
    validate.add_argument("manifest", nargs="?", default=DEFAULT_MANIFEST)
    run = subparsers.add_parser("run")
    run.add_argument("--binary", default=ROOT / "target/release/qxfx0")
    run.add_argument("--manifest", default=DEFAULT_MANIFEST)
    run.add_argument("--output", required=True)
    run.add_argument("--expected-sha")
    arguments = parser.parse_args()
    try:
        if arguments.command == "validate":
            result = validate_manifest(load_json(arguments.manifest))
        else:
            result = observe(
                arguments.binary,
                arguments.manifest,
                arguments.output,
                arguments.expected_sha,
            )
        print(json.dumps(result, ensure_ascii=False, sort_keys=True))
    except (OSError, sqlite3.Error, subprocess.SubprocessError, ValidationError) as error:
        parser.exit(1, f"debate observation failed: {error}\n")


if __name__ == "__main__":
    main()

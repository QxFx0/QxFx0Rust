#!/usr/bin/env python3

import copy
import importlib.util
import tempfile
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("debate_observation.py")
SPEC = importlib.util.spec_from_file_location("debate_observation", MODULE_PATH)
OBSERVATION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(OBSERVATION)


class DebateObservationManifestTests(unittest.TestCase):
    def manifest(self):
        return OBSERVATION.load_json(OBSERVATION.DEFAULT_MANIFEST)

    def test_manifest_covers_all_v1_input_classes(self):
        inventory = OBSERVATION.validate_manifest(self.manifest())
        self.assertEqual(inventory["scenarios_total"], 16)
        self.assertEqual(inventory["turns_total"], 17)

    def test_privacy_policy_fails_closed(self):
        for field, value in (
            ("raw_user_logs", True),
            ("reviewed_formulations_only", False),
            ("authority_change", "expand"),
        ):
            manifest = self.manifest()
            manifest[field] = value
            with self.assertRaises(OBSERVATION.ValidationError):
                OBSERVATION.validate_manifest(manifest)

    def test_missing_input_class_fails_closed(self):
        manifest = self.manifest()
        manifest["scenarios"] = manifest["scenarios"][:-1]
        with self.assertRaises(OBSERVATION.ValidationError):
            OBSERVATION.validate_manifest(manifest)

    def test_duplicate_turn_id_fails_closed(self):
        manifest = copy.deepcopy(self.manifest())
        manifest["scenarios"][1]["turns"][0]["turn_id"] = manifest["scenarios"][0][
            "turns"
        ][0]["turn_id"]
        with self.assertRaises(OBSERVATION.ValidationError):
            OBSERVATION.validate_manifest(manifest)

    def test_unknown_manifest_fields_fail_closed(self):
        manifest = self.manifest()
        manifest["raw_user_log_path"] = "/tmp/raw.log"
        with self.assertRaises(OBSERVATION.ValidationError):
            OBSERVATION.validate_manifest(manifest)

    def test_malformed_manifest_json_is_a_validation_error(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "manifest.json"
            path.write_text("{", encoding="utf-8")
            with self.assertRaises(OBSERVATION.ValidationError):
                OBSERVATION.load_json(path)

    def test_empty_input_is_reserved_for_typed_guard_case(self):
        manifest = self.manifest()
        manifest["scenarios"][0]["turns"][0]["utterance"] = ""
        with self.assertRaises(OBSERVATION.ValidationError):
            OBSERVATION.validate_manifest(manifest)

    def test_forbidden_receipt_fields_are_recursive(self):
        record = {"receipt": {"nodes": [{"raw_text": "secret"}]}}
        self.assertEqual(OBSERVATION.forbidden_keys(record), {"raw_text"})

    def test_sequence_digest_is_deterministic_and_length_prefixed(self):
        self.assertEqual(
            OBSERVATION.sequence_digest([b"a", b"bc"]),
            OBSERVATION.sequence_digest([b"a", b"bc"]),
        )
        self.assertNotEqual(
            OBSERVATION.sequence_digest([b"a", b"bc"]),
            OBSERVATION.sequence_digest([b"ab", b"c"]),
        )

    def test_receipt_digest_requires_a_32_byte_array(self):
        self.assertEqual(
            OBSERVATION.receipt_digest({"digest": [0] * 32}),
            "00" * 32,
        )
        invalid_values = (
            [0] * 31,
            [0] * 33,
            [256] + [0] * 31,
            [-1] + [0] * 31,
            "0" * 64,
            None,
        )
        for invalid in invalid_values:
            with self.assertRaises(OBSERVATION.ValidationError):
                OBSERVATION.receipt_digest({"digest": invalid})


if __name__ == "__main__":
    unittest.main()

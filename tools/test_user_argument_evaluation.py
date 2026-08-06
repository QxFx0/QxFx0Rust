#!/usr/bin/env python3

import copy
import hashlib
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path


MODULE_PATH = Path(__file__).with_name("user_argument_evaluation.py")
SPEC = importlib.util.spec_from_file_location("user_argument_evaluation", MODULE_PATH)
EVALUATION = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(EVALUATION)


class UserArgumentEvaluationTests(unittest.TestCase):
    """Exercise the reviewed corpus validator and deterministic evaluator."""

    def manifest(self):
        """Load a fresh copy of the checked-in gold manifest."""
        return EVALUATION.load_json(EVALUATION.DEFAULT_MANIFEST)

    @staticmethod
    def digest(value):
        """Return the lowercase SHA-256 digest of one UTF-8 test string."""
        return hashlib.sha256(value.encode("utf-8")).hexdigest()

    def perfect_predictions(self, manifest=None):
        """Build prediction fixtures that exactly reproduce the gold graph."""
        manifest = manifest or self.manifest()
        cases = []
        for gold in manifest["cases"]:
            nodes = []
            for expected in gold["expected_nodes"]:
                node = {
                    "node_id": expected["node_id"],
                    "kind": expected["kind"],
                    "source": expected["source"],
                    "polarity": expected["polarity"],
                    "proposition": copy.deepcopy(expected["proposition"]),
                    "confidence_basis_points": expected["confidence_min_basis_points"],
                    "parser_rule_id": "gold.fixture.node",
                    "parser_rule_version": 1,
                }
                nodes.append(node)
            relations = []
            for expected in gold["expected_relations"]:
                relation = {
                    "relation_id": f"{gold['case_id']}.relation.{len(relations)}",
                    "from": expected["from"],
                    "to": expected["to"],
                    "kind": expected["kind"],
                    "confidence_basis_points": expected["confidence_min_basis_points"],
                    "parser_rule_id": "gold.fixture.relation",
                    "parser_rule_version": 1,
                }
                relations.append(relation)
            receipt_digest = self.digest(f"{gold['case_id']}:receipt")
            output_digest = self.digest(f"{gold['case_id']}:output")
            state_digest = self.digest(f"{gold['case_id']}:state")
            cases.append(
                {
                    "case_id": gold["case_id"],
                    "disposition": gold["accepted_dispositions"][0],
                    "nodes": nodes,
                    "relations": relations,
                    "omissions": list(gold["expected_omissions"]),
                    "receipt_digest": receipt_digest,
                    "replay_receipt_digest": receipt_digest,
                    "baseline_output_digest": output_digest,
                    "observed_output_digest": output_digest,
                    "replay_output_digest": output_digest,
                    "baseline_state_digest": state_digest,
                    "observed_state_digest": state_digest,
                    "replay_state_digest": state_digest,
                    "artifact": {
                        "schema": "qxfx0.user-argument-parse-trace.v1",
                        "receipt_digest": receipt_digest,
                    },
                }
            )
        return {
            "schema": EVALUATION.PREDICTIONS_SCHEMA,
            "corpus_id": manifest["corpus_id"],
            "build_sha": "a" * 40,
            "authority_change": "none",
            "raw_user_logs": False,
            "cases": cases,
        }

    def test_manifest_is_reviewed_bounded_and_covers_every_v1_axis(self):
        """Require the reviewed corpus to cover every declared v1 taxonomy axis."""
        inventory = EVALUATION.validate_manifest(self.manifest())
        self.assertEqual(inventory["cases_total"], 17)
        self.assertEqual(set(inventory["relation_coverage"]), EVALUATION.RELATION_KINDS)
        self.assertEqual(
            set(inventory["formulation_class_coverage"]),
            EVALUATION.REQUIRED_FORMULATION_CLASSES,
        )
        self.assertEqual(set(inventory["source_class_coverage"]), EVALUATION.SOURCE_CLASSES)
        self.assertEqual(set(inventory["polarity_coverage"]), EVALUATION.POLARITIES)
        self.assertEqual(set(inventory["accepted_disposition_coverage"]), EVALUATION.DISPOSITIONS)

    def test_compiled_manifest_is_deterministic_and_contains_no_formulations(self):
        """Keep compiled inventory deterministic and free of reviewed text."""
        first = EVALUATION.compile_manifest(self.manifest())
        second = EVALUATION.compile_manifest(self.manifest())
        self.assertEqual(first, second)
        self.assertNotIn("formulation", first)
        self.assertNotIn("privacy_needles", first)
        encoded = json.dumps(first, ensure_ascii=False, sort_keys=True)
        self.assertNotIn('"privacy_needles"', encoded)
        for case in self.manifest()["cases"]:
            self.assertNotIn(case["formulation"], encoded)
            for needle in case["privacy_needles"]:
                self.assertNotIn(needle, encoded)
        self.assertFalse(first["parser_implementation"])
        self.assertFalse(first["runtime_integration"])
        self.assertEqual(first["authority_change"], "none")

    def test_manifest_privacy_and_authority_policy_fails_closed(self):
        """Reject any manifest that weakens privacy or changes authority."""
        for field, value in (
            ("raw_user_logs", True),
            ("reviewed_formulations_only", False),
            ("source_policy", "production_logs"),
            ("authority_change", "expand"),
            ("persistence_change", "write"),
        ):
            manifest = self.manifest()
            manifest[field] = value
            with self.assertRaises(EVALUATION.ValidationError):
                EVALUATION.validate_manifest(manifest, verify_digest=False)

    def test_manifest_digest_detects_formulation_and_expectation_tampering(self):
        """Bind reviewed formulations and expectations into the manifest digest."""
        manifest = self.manifest()
        manifest["cases"][0]["formulation"] += " подмена"
        manifest["cases"][0]["formulation_sha256"] = self.digest(
            manifest["cases"][0]["formulation"]
        )
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest)

        manifest = self.manifest()
        manifest["cases"][0]["expected_relations"][0]["confidence_min_basis_points"] -= 1
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest)

    def test_missing_category_or_relation_kind_fails_closed(self):
        """Reject incomplete required category and relation coverage."""
        manifest = self.manifest()
        manifest["cases"] = manifest["cases"][:-1]
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

        manifest = self.manifest()
        manifest["cases"][0]["expected_relations"] = []
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

    def test_unknown_fields_and_categorical_label_smuggling_fail_closed(self):
        """Reject schema expansion and unknown-topic label disclosure."""
        manifest = self.manifest()
        manifest["raw_log_path"] = "/tmp/user.log"
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

        manifest = self.manifest()
        unknown = next(case for case in manifest["cases"] if case["category"] == "unknown_topic")
        unknown["expected_nodes"][0]["proposition"]["subject"] = {
            "kind": "canonical_topic",
            "id": "Кванточайник",
        }
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

    def test_validator_enforces_source_polarity_and_disposition_coverage(self):
        """Require complete source, polarity, and disposition coverage."""
        manifest = self.manifest()
        hypothetical = next(case for case in manifest["cases"] if case["category"] == "hypothetical")
        hypothetical["expected_nodes"][0]["source"] = "direct"
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

        manifest = self.manifest()
        for case in manifest["cases"]:
            for node in case["expected_nodes"]:
                if node["polarity"] == "unknown":
                    node["polarity"] = "affirmed"
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

        manifest = self.manifest()
        unknown = next(case for case in manifest["cases"] if case["category"] == "unknown_topic")
        unknown["accepted_dispositions"] = ["abstained"]
        unknown["expected_nodes"] = []
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

        manifest = self.manifest()
        external = next(case for case in manifest["cases"] if case["category"] == "external_subject")
        external["expected_nodes"][0]["proposition"]["subject"]["id"] = "Господин Икс"
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

    def test_surrogates_fail_with_validation_errors_before_utf8_encoding(self):
        """Reject unpaired surrogates in every free or contract string boundary."""
        manifest = self.manifest()
        manifest["cases"][0]["formulation"] = "\ud800"
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

        manifest = self.manifest()
        manifest["cases"][0]["privacy_needles"] = ["\ud800"]
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

        manifest = self.manifest()
        manifest["cases"][0]["expected_nodes"][0]["proposition"]["subject"]["id"] = "\ud800"
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.validate_manifest(manifest, verify_digest=False)

    def test_perfect_predictions_score_each_relation_kind_separately(self):
        """Score perfect predictions independently for every node and relation kind."""
        report = EVALUATION.evaluate(self.manifest(), self.perfect_predictions())
        self.assertTrue(report["zero_failure_budgets_met"])
        self.assertEqual(report["confidence_floor_failures"], 0)
        self.assertEqual(report["unexpected_abstentions"], 0)
        for metrics in report["relation_metrics"].values():
            self.assertEqual(metrics["precision_basis_points"], 10_000)
            self.assertEqual(metrics["recall_basis_points"], 10_000)
        for metrics in report["node_metrics"].values():
            self.assertEqual(metrics["precision_basis_points"], 10_000)
            self.assertEqual(metrics["recall_basis_points"], 10_000)

    def test_report_is_deterministic_and_digest_binds_metrics(self):
        """Produce stable reports whose digest binds every reported metric."""
        manifest = self.manifest()
        predictions = self.perfect_predictions(manifest)
        first = EVALUATION.evaluate(manifest, predictions)
        second = EVALUATION.evaluate(manifest, predictions)
        self.assertEqual(first, second)
        recorded = first.pop("report_digest")
        self.assertEqual(recorded, EVALUATION.canonical_digest(first, EVALUATION.REPORT_DOMAIN))

    def test_missed_and_wrong_relations_are_not_hidden_by_aggregate_accuracy(self):
        """Expose relation-kind false positives and false negatives separately."""
        predictions = self.perfect_predictions()
        case = next(case for case in predictions["cases"] if case["case_id"] == "clean-support")
        case["relations"][0]["kind"] = "attacks"
        report = EVALUATION.evaluate(self.manifest(), predictions)
        self.assertEqual(report["relation_metrics"]["supports"]["false_negative"], 1)
        self.assertEqual(report["relation_metrics"]["supports"]["recall_basis_points"], 0)
        self.assertEqual(report["relation_metrics"]["attacks"]["false_positive"], 1)
        self.assertLess(report["relation_metrics"]["attacks"]["precision_basis_points"], 10_000)

    def test_duplicate_semantic_relation_is_incorrect_for_metrics_and_calibration(self):
        """Treat excess parallel semantic relations as false positives in all views."""
        predictions = self.perfect_predictions()
        case = next(case for case in predictions["cases"] if case["case_id"] == "clean-support")
        duplicate_node = copy.deepcopy(case["nodes"][0])
        duplicate_node["node_id"] = "clean.premise.duplicate"
        case["nodes"].append(duplicate_node)
        duplicate_relation = copy.deepcopy(case["relations"][0])
        duplicate_relation["relation_id"] = "clean-support.relation.duplicate"
        duplicate_relation["from"] = duplicate_node["node_id"]
        case["relations"].append(duplicate_relation)

        report = EVALUATION.evaluate(self.manifest(), predictions)
        supports = report["relation_metrics"]["supports"]
        self.assertEqual(supports["true_positive"], 1)
        self.assertEqual(supports["false_positive"], 1)
        bucket = report["confidence_calibration"]["supports"]["7500_10000"]
        self.assertEqual(bucket["correct"], 1)
        self.assertEqual(bucket["incorrect"], 1)

    def test_replay_privacy_and_parity_failures_are_reported(self):
        """Report replay, privacy, output, and state parity failures explicitly."""
        predictions = self.perfect_predictions()
        unknown = next(
            case for case in predictions["cases"] if case["case_id"] == "unknown-topic-categorical"
        )
        unknown["artifact"]["topic_label"] = "Кванточайник"
        unknown["replay_receipt_digest"] = "b" * 64
        unknown["observed_output_digest"] = "c" * 64
        unknown["observed_state_digest"] = "d" * 64
        report = EVALUATION.evaluate(self.manifest(), predictions)
        self.assertFalse(report["zero_failure_budgets_met"])
        self.assertEqual(report["privacy_violations"], 1)
        self.assertEqual(report["replay_failures"], 1)
        self.assertEqual(report["digest_mismatches"], 1)
        self.assertEqual(report["output_parity_violations"], 1)
        self.assertEqual(report["state_parity_violations"], 1)

        predictions = self.perfect_predictions()
        unknown = next(
            case for case in predictions["cases"] if case["case_id"] == "unknown-topic-categorical"
        )
        unknown["nodes"][0]["proposition"]["subject"] = {
            "kind": "canonical_topic",
            "id": "Кванточайник",
        }
        report = EVALUATION.evaluate(self.manifest(), predictions)
        self.assertEqual(report["privacy_violations"], 1)

    def test_prediction_validation_rejects_dangling_graphs_and_missing_cases(self):
        """Reject dangling graphs, incomplete envelopes, and duplicate relations."""
        predictions = self.perfect_predictions()
        first_relation = next(case for case in predictions["cases"] if case["relations"])
        first_relation["relations"][0]["from"] = "missing.node"
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.evaluate(self.manifest(), predictions)

        predictions = self.perfect_predictions()
        predictions["cases"].pop()
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.evaluate(self.manifest(), predictions)

        predictions = self.perfect_predictions()
        first_relation = next(case for case in predictions["cases"] if case["relations"])
        duplicate = copy.deepcopy(first_relation["relations"][0])
        duplicate["relation_id"] = f"{duplicate['relation_id']}.duplicate"
        first_relation["relations"].append(duplicate)
        with self.assertRaises(EVALUATION.ValidationError):
            EVALUATION.evaluate(self.manifest(), predictions)

    def test_create_new_output_refuses_overwrite(self):
        """Preserve evidence artifacts with create-new output semantics."""
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "report.json"
            EVALUATION.write_new_json(path, {"status": "first"})
            with self.assertRaises(FileExistsError):
                EVALUATION.write_new_json(path, {"status": "second"})


if __name__ == "__main__":
    unittest.main()

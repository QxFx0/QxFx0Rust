#!/usr/bin/env python3

import copy
import importlib.util
import unittest
from pathlib import Path

MODULE_PATH = Path(__file__).with_name("topic_corpus_pipeline.py")
SPEC = importlib.util.spec_from_file_location("topic_corpus_pipeline", MODULE_PATH)
PIPELINE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PIPELINE)


class TopicCorpusPipelineTests(unittest.TestCase):
    def source(self):
        return PIPELINE.load_json(
            PIPELINE.ROOT / "data/corpus-batches/2026-08-audited-clusters-v1.json"
        )

    def test_inventory_preserves_maturity_boundaries(self):
        inventory = PIPELINE.maturity_inventory()
        self.assertEqual(inventory["totals"]["recognized"], 107)
        self.assertEqual(inventory["totals"]["grounded"], 30)
        self.assertEqual(inventory["totals"]["audited"], 30)
        self.assertEqual(inventory["totals"]["canary"], 6)
        self.assertEqual(inventory["totals"]["production_stable"], 6)

    def test_batch_compiles_deterministically_without_authority(self):
        first = PIPELINE.compile_batch(self.source())
        second = PIPELINE.compile_batch(self.source())
        self.assertEqual(first, second)
        self.assertEqual(first["topics_total"], 10)
        self.assertEqual(first["claims_total"], 25)
        self.assertEqual(first["authority_change"], "none")
        self.assertTrue(all(not topic["already_canary_authorized"] for topic in first["topics"]))

    def test_batch_size_fails_closed(self):
        source = self.source()
        source["topics"] = source["topics"][:9]
        with self.assertRaises(PIPELINE.ValidationError):
            PIPELINE.compile_batch(source)

    def test_unrecognized_topic_fails_closed(self):
        source = copy.deepcopy(self.source())
        source["topics"][0]["canonical_topic"] = "кванточайник"
        with self.assertRaises(PIPELINE.ValidationError):
            PIPELINE.compile_batch(source)

    def test_authority_change_fails_closed(self):
        source = self.source()
        source["authority_change"] = "expand"
        with self.assertRaises(PIPELINE.ValidationError):
            PIPELINE.compile_batch(source)

    def test_cross_topic_fact_fails_closed(self):
        topic = "истина"
        claim_id, claim = next(iter(PIPELINE.load_json(PIPELINE.AUDITED)["topics"][topic]["claims"].items()))
        facts = PIPELINE.facts_index()
        facts[claim["fact_id"]] = dict(facts[claim["fact_id"]], subject="concept.мнение")
        with self.assertRaises(PIPELINE.ValidationError):
            PIPELINE.compile_imported_claim(
                topic,
                claim_id,
                claim,
                facts,
                PIPELINE.valency_frames(),
            )


if __name__ == "__main__":
    unittest.main()

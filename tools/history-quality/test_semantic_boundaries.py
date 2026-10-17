"""Preset boundary judgments calibrate review calculation, not model behavior."""
import unittest

import evaluate


SUITE = evaluate.DEFAULT.with_name("semantic-boundary-cases.json")


class SemanticBoundaryTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.suite = evaluate.read_json(SUITE, 512 * 1024)
        cls.report = evaluate.evaluate(SUITE)
        cls.cases = {case["case_id"]: case for case in cls.suite["cases"]}

    def test_six_preset_boundary_cases_keep_their_counts_and_provenance(self):
        expected = {
            "conditional-preserved": ("supported_in_review", 1, 0, 2, 0),
            "conditional-reversed": ("issues_found", 0, 1, 0, 2),
            "quotation-preserved": ("supported_in_review", 1, 0, 2, 0),
            "quotation-reversed": ("issues_found", 0, 1, 0, 2),
            "subjects-preserved": ("supported_in_review", 1, 0, 2, 0),
            "subjects-reversed": ("issues_found", 0, 1, 0, 2),
        }
        self.assertEqual(set(self.cases), set(expected))
        self.assertEqual(self.report["native_validation"], "not_rerun")
        self.assertIsNone(self.report["overall_model_quality"])
        for result in self.report["cases"]:
            with self.subTest(case=result["case_id"]):
                self.assertEqual((result["review_status"], result["items"]["supported"],
                                  result["items"]["contradicted"], result["required_units"]["observed"],
                                  result["required_units"]["missing"]), expected[result["case_id"]])
                self.assertEqual(result["items"]["uncertain"] + result["items"]["unsupported"]
                                 + result["items"]["unreviewed"] + result["required_units"]["unknown"], 0)
                self.assertEqual(result["reviewer"], "synthetic_fixture")
                self.assertEqual(result["evidence_mode"], "synthetic")
                self.assertIsNone(result["native_task_input_sha256"])

    def test_same_exact_quote_can_have_opposite_preset_verdicts(self):
        for name in ("conditional", "quotation", "subjects"):
            with self.subTest(scenario=name):
                good, bad = (self.cases[f"{name}-{suffix}"] for suffix in ("preserved", "reversed"))
                self.assertEqual(good["scope_value_sha256"], bad["scope_value_sha256"])
                self.assertEqual(good["item_judgments"][0]["evidence"], bad["item_judgments"][0]["evidence"])
                self.assertNotEqual(good["response_value_sha256"], bad["response_value_sha256"])
                for case in (good, bad):
                    scope = evaluate.scope_for(case, SUITE.parent)
                    text = scope[0]["node"]["message"]["content"]["parts"][0]
                    self.assertIsNone(scope[0]["node"]["message"]["create_time"])
                    for evidence in case["response"]["summary"][0]["evidence"]:
                        self.assertEqual(evidence["source_index"], 0)
                        self.assertIn(evidence["quote"], text)
                    self.assertEqual(case["response"]["summary"][0]["evidence"],
                                     case["item_judgments"][0]["evidence"])


if __name__ == "__main__":
    unittest.main()

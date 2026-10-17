"""Preset synthetic judgments calibrate metrics; they never grade a model."""
import unittest

import evaluate


SUITE = evaluate.DEFAULT.with_name("semantic-calibration-cases.json")


class SemanticCalibrationTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.report = evaluate.evaluate(SUITE)
        cls.cases = {case["case_id"]: case for case in cls.report["cases"]}

    def test_named_semantic_scenarios_keep_expected_judgments_and_coverage(self):
        # Expected counts are independently preset review outcomes, not an
        # inference about a real model or native semantic validation.
        # Fields: status, supported, contradicted, unsupported, uncertain,
        # observed units, missing units, unknown units.
        expected = {
            "preference-history-faithful": ("supported_in_review", 2, 0, 0, 0, 3, 0, 0),
            "preference-history-reversed": ("issues_found", 0, 1, 0, 0, 0, 3, 0),
            "branch-decisions-kept-separate": ("supported_in_review", 1, 0, 0, 0, 3, 0, 0),
            "branch-decisions-flattened": ("issues_found", 0, 1, 0, 0, 0, 3, 0),
            "assistant-proposal-kept-as-proposal": ("supported_in_review", 1, 0, 0, 0, 3, 0, 0),
            "assistant-proposal-promoted-to-owner-decision": ("issues_found", 0, 1, 0, 0, 0, 3, 0),
            "unknown-source-date-preserved": ("supported_in_review", 1, 0, 0, 0, 1, 0, 0),
            "unknown-source-date-invented": ("issues_found", 0, 0, 1, 0, 0, 1, 0),
            "faithful-summary-omits-cancellation": ("issues_found", 1, 0, 0, 0, 1, 1, 0),
            "branch-current-state-unresolved": ("review_required", 1, 0, 0, 1, 2, 0, 1),
        }
        self.assertEqual(set(self.cases), set(expected))
        self.assertEqual(len(self.report["cases"]), len(expected))
        self.assertEqual(self.report["native_validation"], "not_rerun")
        self.assertIsNone(self.report["overall_model_quality"])
        for identity, counts in expected.items():
            with self.subTest(case=identity):
                case = self.cases[identity]
                actual = (case["review_status"], *(case["items"][key] for key in
                          ("supported", "contradicted", "unsupported", "uncertain")),
                          *(case["required_units"][key] for key in ("observed", "missing", "unknown")))
                self.assertEqual(actual, counts)
                self.assertEqual(case["items"]["unreviewed"], 0)
                self.assertEqual(case["evidence_mode"], "synthetic")
                self.assertEqual(case["reviewer"], "synthetic_fixture")
                self.assertIsNone(case["native_task_input_sha256"])

    def test_full_supported_fraction_does_not_hide_an_omission(self):
        case = self.cases["faithful-summary-omits-cancellation"]
        self.assertEqual(case["supported_fraction_of_adjudicated_items"], 1)
        self.assertEqual(case["observed_fraction_of_required_units"], 0.5)
        self.assertTrue(case["all_required_units_reviewed"])
        self.assertEqual(case["review_status"], "issues_found")

    def test_uncertainty_stays_outside_the_adjudicated_denominator(self):
        case = self.cases["branch-current-state-unresolved"]
        self.assertEqual(case["adjudicated_item_count"], 1)
        self.assertEqual(case["supported_fraction_of_adjudicated_items"], 1)
        self.assertEqual(case["items"]["uncertain"], 1)
        self.assertEqual(case["required_units"]["unknown"], 1)
        self.assertFalse(case["all_required_units_reviewed"])
        self.assertEqual(case["review_status"], "review_required")


if __name__ == "__main__":
    unittest.main()

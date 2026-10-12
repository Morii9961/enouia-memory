import copy
import json
from io import BytesIO
import unittest
from unittest.mock import patch
import evaluate


class AssessmentTests(unittest.TestCase):
    def setUp(self):
        self.suite = json.loads(evaluate.DEFAULT.read_text(encoding="utf-8"))
        self.case = self.suite["cases"][0]
        self.scope = evaluate.scope_for(self.case, evaluate.DEFAULT.parent)

    def test_calibration_separates_supported_contradicted_and_unreviewed(self):
        result = evaluate.evaluate(evaluate.DEFAULT)
        self.assertEqual([c["review_status"] for c in result["cases"]],
                         ["supported_in_review", "issues_found", "review_required"])
        self.assertEqual(result["cases"][1]["items"]["contradicted"], 1)
        self.assertEqual(result["cases"][1]["required_units"]["missing"], 2)
        self.assertIsNone(result["cases"][2]["supported_fraction_of_adjudicated_items"])
        self.assertEqual(result["cases"][2]["required_units"]["unknown"], 2)
        self.assertIsNone(result["overall_model_quality"])

    def test_changed_response_invalidates_judgments(self):
        self.case["response"]["changes"][0]["statement"] = "改写后尚未审核。"
        with self.assertRaisesRegex(ValueError, "response judgment binding"):
            evaluate.assess(self.case, self.scope)

    def test_changed_source_invalidates_judgments(self):
        changed = copy.deepcopy(self.scope)
        changed[0]["node"]["message"]["content"]["parts"] = ["改写的来源。"]
        with self.assertRaisesRegex(ValueError, "scope judgment binding"):
            evaluate.assess(self.case, changed)

    def test_observed_unit_cannot_use_contradicted_path(self):
        self.case["item_judgments"][0]["verdict"] = "contradicted"
        with self.assertRaisesRegex(ValueError, "observed unit"):
            evaluate.assess(self.case, self.scope)

    def test_invalid_or_duplicate_paths_are_refused(self):
        original = copy.deepcopy(self.case)
        self.case["item_judgments"][0]["path"] = "/summary/99/statement"
        with self.assertRaisesRegex(ValueError, "reviewed path"):
            evaluate.assess(self.case, self.scope)
        original["item_judgments"].append(copy.deepcopy(original["item_judgments"][0]))
        with self.assertRaisesRegex(ValueError, "reviewed path"):
            evaluate.assess(original, self.scope)

    def test_bad_review_quote_is_refused(self):
        self.case["item_judgments"][0]["evidence"][0]["quote"] = "原文没有这句话。"
        with self.assertRaisesRegex(ValueError, "quote not in source"):
            evaluate.assess(self.case, self.scope)

    def test_unjudged_units_stay_unknown(self):
        self.case["unit_judgments"].pop()
        result = evaluate.assess(self.case, self.scope)
        self.assertEqual(result["required_units"]["unknown"], 1)
        self.assertFalse(result["all_required_units_reviewed"])
        self.assertEqual(result["review_status"], "review_required")

    def test_source_escape_and_unbound_owner_sample_are_refused(self):
        self.case["source_file"] = "../outside.json"
        with self.assertRaisesRegex(ValueError, "outside suite"):
            evaluate.scope_for(self.case, evaluate.DEFAULT.parent)
        self.case["evidence_mode"] = "owner_selected"
        with self.assertRaisesRegex(ValueError, "native sample binding"):
            evaluate.assess(self.case, self.scope)

    def test_duplicate_json_keys_are_refused(self):
        with patch("pathlib.Path.open", return_value=BytesIO(b'{"case_id":"first","case_id":"second"}')):
            from pathlib import Path
            with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
                evaluate.read_json(Path("synthetic-duplicate.json"), 1000)


if __name__ == "__main__":
    unittest.main()

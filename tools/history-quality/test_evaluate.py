import copy
import json
import hashlib
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

    def native_case(self):
        text="合成的选定片段。\n保留条件。"
        self.case.pop("source_file")
        self.case.pop("messages")
        self.case.update(scope_kind="native_selected_spans_v1",source_scope=[{
            "source":{"source_id":"synthetic-source","source_revision":1},
            "selection":{"start":4,"end":4+len(text.encode())},"text":text,
            "snippet_hash":hashlib.sha256(text.encode()).hexdigest(),
        }],reviewer="pending",item_judgments=[],unit_judgments=[])
        self.case["scope_value_sha256"]=evaluate.digest(self.case["source_scope"])
        return self.case

    def test_native_scope_keeps_pending_review_and_unknown_units(self):
        case=self.native_case()
        scope=evaluate.scope_for(case,evaluate.DEFAULT.parent)
        result=evaluate.assess(case,scope)
        self.assertEqual(result["review_status"],"review_required")
        self.assertEqual(result["required_units"]["unknown"],len(case["required_units"]))
        self.assertIsNone(result["supported_fraction_of_adjudicated_items"])

    def test_native_scope_refuses_tamper_ambiguity_duplicates_and_false_review(self):
        case=self.native_case()
        for field,value in [("source_file","unrequested.json"),("messages",[]),("scope_kind","other")]:
            bad=copy.deepcopy(case);bad[field]=value
            with self.assertRaisesRegex(ValueError,"ambiguous"):
                evaluate.scope_for(bad,evaluate.DEFAULT.parent)
        bad=copy.deepcopy(case);bad["source_scope"][0]["snippet_hash"]="0"*64
        with self.assertRaisesRegex(ValueError,"snippet binding"):
            evaluate.scope_for(bad,evaluate.DEFAULT.parent)
        bad=copy.deepcopy(case);bad["source_scope"]*=2
        with self.assertRaisesRegex(ValueError,"duplicate native"):
            evaluate.scope_for(bad,evaluate.DEFAULT.parent)
        bad=copy.deepcopy(case);bad["item_judgments"]=[{"path":"/changes/0/statement"}]
        with self.assertRaisesRegex(ValueError,"pending reviewer"):
            evaluate.assess(bad,bad["source_scope"])

    def test_native_review_quotes_use_only_selected_span(self):
        case=self.native_case();case["reviewer"]="synthetic_fixture"
        case["item_judgments"]=[{"path":"/changes/0/statement","verdict":"uncertain","reason":"合成测试判定。","evidence":[{"source_index":0,"quote":"保留条件"}]}]
        self.assertEqual(evaluate.assess(case,case["source_scope"])["items"]["uncertain"],1)
        case["item_judgments"][0]["evidence"][0]["quote"]="片段以外的内容"
        with self.assertRaisesRegex(ValueError,"quote not in source"):
            evaluate.assess(case,case["source_scope"])


if __name__ == "__main__":
    unittest.main()

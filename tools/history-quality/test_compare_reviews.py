import copy
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import compare_reviews
import evaluate
import test_evaluate as fixtures


class ComparisonTests(unittest.TestCase):
    def setUp(self):
        fixture = fixtures.AssessmentTests()
        fixture.setUp()
        self.case = fixture.native_case()
        self.case["case_id"] = "private-case-marker🌱"
        self.case["required_units"] = ["private-unit-marker", "second-unit"]
        self.temp = tempfile.TemporaryDirectory(prefix="synthetic-review-comparison-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def write(self, name, cases, **metadata):
        path = self.root / name
        path.write_text(json.dumps({"schema_version": evaluate.VERSION, "cases": cases, **metadata},
                                   ensure_ascii=False), encoding="utf-8")
        return path

    def partial(self, case=None):
        case = copy.deepcopy(self.case if case is None else case)
        case["reviewer"] = "synthetic_fixture"
        case["item_judgments"] = [{"path": "/changes/0/statement", "verdict": "supported",
                                   "reason": "private-reason-marker", "evidence": [
                                       {"source_index": 0, "quote": "保留条件"}]}]
        case["unit_judgments"] = [{"unit_id": case["required_units"][0], "status": "observed",
                                   "paths": ["/changes/0/statement"], "reason": "private-unit-reason"}]
        return case

    def test_partial_review_delta_preserves_unknowns_and_omits_original_text(self):
        before = self.write("before.json", [self.case])
        after = self.write("after.json", [self.partial()])
        original = (before.read_bytes(), after.read_bytes())
        result = compare_reviews.compare(before, after)
        case = result["cases"][0]
        self.assertEqual(result["changed_case_count"], 1)
        self.assertEqual(case["item_paths"], {"added": ["/changes/0/statement"], "edited": [], "removed": []})
        self.assertEqual(case["unit_indices"], {"added": [0], "edited": [], "removed": []})
        self.assertEqual(case["after"]["items"]["unreviewed"], 1)
        self.assertEqual(case["after"]["required_units"]["unknown"], 1)
        self.assertEqual(case["after"]["review_status"], "review_required")
        self.assertIsNone(result["overall_model_quality"])
        self.assertEqual(result["native_validation"], "not_rerun")
        data = compare_reviews.report_bytes(result).decode("utf-8")
        for private in (self.case["case_id"], self.case["required_units"][0], "private-reason-marker",
                        self.case["source_scope"][0]["text"], self.case["response"]["changes"][0]["statement"]):
            self.assertNotIn(private, data)
        self.assertEqual((before.read_bytes(), after.read_bytes()), original)

    def test_edited_and_removed_judgments_are_not_called_an_improvement(self):
        old = self.partial()
        new = copy.deepcopy(old)
        new["item_judgments"][0]["verdict"] = "uncertain"
        new["unit_judgments"] = []
        result = compare_reviews.compare(self.write("before.json", [old]), self.write("after.json", [new]))
        delta = result["cases"][0]
        self.assertEqual(delta["item_paths"]["edited"], ["/changes/0/statement"])
        self.assertEqual(delta["unit_indices"]["removed"], [0])
        self.assertEqual(delta["after"]["items"]["uncertain"], 1)
        self.assertEqual(delta["after"]["required_units"]["unknown"], 2)
        reverse = compare_reviews.compare(self.root / "after.json", self.root / "before.json")
        self.assertEqual(reverse["cases"][0]["unit_indices"]["added"], [0])
        self.assertIsNone(reverse["overall_model_quality"])

    def test_changed_inputs_are_refused_even_with_valid_updated_hashes(self):
        original = copy.deepcopy(self.case)
        original["extra_metadata"] = 1
        before = self.write("before.json", [original])
        alterations = []
        for key, value in (("required_units", ["different-unit"]), ("extra_metadata", True),
                           ("native_binding", {"session_id": "changed-synthetic-session"})):
            altered = copy.deepcopy(original)
            altered[key] = value
            alterations.append(altered)
        altered = copy.deepcopy(original)
        altered["response"]["changes"][0]["statement"] += " changed"
        altered["response_value_sha256"] = evaluate.digest(altered["response"])
        alterations.append(altered)
        altered = copy.deepcopy(original)
        selected = altered["source_scope"][0]
        selected["text"] += " changed"
        selected["selection"]["end"] = selected["selection"]["start"] + len(selected["text"].encode())
        selected["snippet_hash"] = hashlib.sha256(selected["text"].encode()).hexdigest()
        altered["scope_value_sha256"] = evaluate.digest(altered["source_scope"])
        alterations.append(altered)
        for index, altered in enumerate(alterations):
            with self.subTest(index=index):
                after = self.write("after.json", [altered])
                evaluate.evaluate(after)  # Each altered suite is independently valid.
                with self.assertRaisesRegex(ValueError, "case input changed"):
                    compare_reviews.compare(before, after)
        with self.assertRaisesRegex(ValueError, "suite metadata changed"):
            compare_reviews.compare(before, self.write("after.json", [original], extra=True))
        renamed = copy.deepcopy(original)
        renamed["case_id"] = "different-case"
        with self.assertRaisesRegex(ValueError, "identities changed"):
            compare_reviews.compare(before, self.write("after.json", [renamed]))

    def test_case_and_judgment_order_do_not_create_edits(self):
        first = self.partial()
        second = copy.deepcopy(first)
        second["case_id"] = "second-case"
        second["item_judgments"].append({"path": "/changes/0/explanation", "verdict": "uncertain",
                                         "reason": "still uncertain", "evidence": [
                                             {"source_index": 0, "quote": "保留条件"}]})
        second["unit_judgments"].append({"unit_id": second["required_units"][1], "status": "unknown",
                                         "paths": [], "reason": "still unknown"})
        reversed_second = copy.deepcopy(second)
        reversed_second["unit_judgments"].reverse()
        reversed_second["item_judgments"].reverse()
        result = compare_reviews.compare(self.write("before.json", [first, second]),
                                         self.write("after.json", [reversed_second, first]))
        self.assertEqual(result["changed_case_count"], 0)
        self.assertEqual([case["after_index"] for case in result["cases"]], [1, 0])
        reviewer_only = copy.deepcopy(self.case)
        reviewer_only["reviewer"] = "synthetic_fixture"
        result = compare_reviews.compare(self.write("before.json", [self.case]),
                                         self.write("after.json", [reviewer_only]))
        self.assertTrue(result["cases"][0]["review_changed"])
        self.assertEqual(result["cases"][0]["item_paths"]["added"], [])

    def test_changed_snapshot_is_refused(self):
        path = self.write("before.json", [self.case])
        original_evaluate = evaluate.evaluate

        def changed(selected):
            altered = copy.deepcopy(self.case)
            altered["case_id"] += "-changed"
            self.write("before.json", [altered])
            return original_evaluate(selected)

        with patch("compare_reviews.evaluate", side_effect=changed):
            with self.assertRaisesRegex(ValueError, "snapshot changed"):
                compare_reviews.compare(path, path)

    def test_cli_restricted_encoding_and_whole_comparison_refusal(self):
        before = self.write("before.json", [self.case])
        after = self.write("after.json", [self.partial()])
        command = [sys.executable, compare_reviews.__file__, "--before", str(before), "--after", str(after)]
        result = subprocess.run(command, env=dict(os.environ, PYTHONIOENCODING="ascii:strict"),
                                capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout.decode("utf-8")), compare_reviews.compare(before, after))
        bad = copy.deepcopy(self.case)
        bad["case_id"] = "invalid-case"
        bad["scope_value_sha256"] = "0" * 64
        self.write("after.json", [self.partial(), bad])
        result = subprocess.run(command, capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"")
        self.assertNotIn(b"Traceback", result.stderr)

    def test_maximum_case_and_judgment_counts_fit_output_budget(self):
        case = copy.deepcopy(self.case)
        change = case["response"]["changes"][0]
        case["response"]["changes"] = [copy.deepcopy(change) for _ in range(16)]
        case["response"]["summary"] = [{"statement": "synthetic summary"} for _ in range(8)]
        case["response_value_sha256"] = evaluate.digest(case["response"])
        case["required_units"] = [f"synthetic-unit-{index}" for index in range(32)]
        after_case = copy.deepcopy(case)
        after_case["reviewer"] = "synthetic_fixture"
        after_case["item_judgments"] = [{"path": path, "verdict": "supported", "reason": "synthetic",
                                          "evidence": [{"source_index": 0, "quote": "保留条件"}]}
                                         for path in evaluate.item_paths(case["response"])]
        after_case["unit_judgments"] = [{"unit_id": unit, "status": "observed", "reason": "synthetic",
                                          "paths": ["/changes/0/statement"]} for unit in case["required_units"]]
        before_cases, after_cases = [], []
        for index in range(32):
            old, new = copy.deepcopy(case), copy.deepcopy(after_case)
            old["case_id"] = new["case_id"] = f"synthetic-{index}"
            before_cases.append(old)
            after_cases.append(new)
        report = compare_reviews.compare(self.write("before.json", before_cases),
                                         self.write("after.json", after_cases))
        self.assertEqual(report["changed_case_count"], 32)
        self.assertLessEqual(len(compare_reviews.report_bytes(report)), compare_reviews.MAX_REPORT_BYTES)
        with patch("compare_reviews.MAX_REPORT_BYTES", 1):
            with self.assertRaisesRegex(ValueError, "output budget"):
                compare_reviews.report_bytes(report)


if __name__ == "__main__":
    unittest.main()

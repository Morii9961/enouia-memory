import copy
import hashlib
import json
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import evaluate
import review
import test_evaluate as fixtures


def blocks(text):
    return [json.loads(value) for value in re.findall(r"(?ms)^```json\n(.*?)^```\n", text)]


class ReviewTests(unittest.TestCase):
    def native_case(self, complete=False):
        fixture = fixtures.AssessmentTests()
        fixture.setUp()
        return fixture.native_analysis_case() if complete else fixture.native_case()

    def write_suite(self, root, case):
        path = root / "synthetic-review.json"
        path.write_text(json.dumps({"schema_version": evaluate.VERSION, "cases": [case]},
                                   ensure_ascii=False), encoding="utf-8")
        return path

    def test_default_review_preserves_sources_paths_judgments_and_unknowns(self):
        before = evaluate.DEFAULT.read_bytes()
        text = review.render(evaluate.DEFAULT)
        data = blocks(text)
        result = evaluate.evaluate(evaluate.DEFAULT)
        self.assertEqual(data[0]["suite_value_sha256"], result["suite_value_sha256"])
        self.assertEqual(data[0]["case_count"], 3)
        self.assertEqual(data[0]["native_validation"], "not_rerun")
        self.assertIsNone(data[0]["overall_model_quality"])
        self.assertEqual(len(data), 10)
        for index, assessment in enumerate(result["cases"]):
            binding, scope, response = data[1 + index * 3:4 + index * 3]
            self.assertEqual(binding["assessment"], assessment)
            self.assertEqual(evaluate.digest(scope), assessment["scope_value_sha256"])
            self.assertEqual(evaluate.digest(response["response"]), assessment["response_value_sha256"])
        self.assertEqual(data[3]["item_reviews"][0]["path"], "/changes/0/statement")
        self.assertEqual(data[3]["item_reviews"][0]["judgment"]["verdict"], "supported")
        self.assertTrue(all(unit["judgment"] is None for unit in data[9]["required_unit_reviews"]))
        self.assertEqual(data[7]["assessment"]["required_units"]["unknown"], 2)
        self.assertIn("关键单元：出现 0、漏提 2、未知 0", text)
        self.assertIn("按已有判断统计：仍需评审", text)
        self.assertEqual(evaluate.DEFAULT.read_bytes(), before)

    def test_native_input_structured_view_keeps_exact_declared_hash_and_pending(self):
        case = self.native_case(complete=True)
        with tempfile.TemporaryDirectory(prefix="synthetic-review-input-") as directory:
            path = self.write_suite(Path(directory), case)
            data = blocks(review.render(path))
        self.assertEqual(data[1]["assessment"]["reviewer"], "pending")
        self.assertEqual(data[3]["native_task_input_sha256"], case["native_task_input_sha256"])
        self.assertEqual(data[3]["input_value"], json.loads(case["native_task_input_text"]))
        self.assertEqual(data[4]["response"], case["response"])
        self.assertTrue(all(item["judgment"] is None for item in data[4]["item_reviews"]))

    def test_untrusted_markdown_and_html_stay_inside_json_data(self):
        case = self.native_case()
        injection = '```\n# Imported heading\n<script>alert("synthetic")</script>\n|row|value|'
        case["case_id"] = injection
        case["response"]["changes"][0]["statement"] = injection
        scope = case["source_scope"]
        scope[0]["text"] = injection
        scope[0]["selection"]["end"] = scope[0]["selection"]["start"] + len(injection.encode())
        scope[0]["snippet_hash"] = hashlib.sha256(injection.encode()).hexdigest()
        case["scope_value_sha256"] = evaluate.digest(scope)
        case["response_value_sha256"] = evaluate.digest(case["response"])
        with tempfile.TemporaryDirectory(prefix="synthetic-review-escaping-") as directory:
            text = review.render(self.write_suite(Path(directory), case))
        data = blocks(text)
        self.assertEqual(len(data), 4)
        self.assertEqual(data[1]["assessment"]["case_id"], injection)
        self.assertEqual(data[2][0]["text"], injection)
        self.assertEqual(data[3]["response"]["changes"][0]["statement"], injection)
        outside = re.sub(r"(?ms)^```json\n.*?^```\n", "", text)
        self.assertNotIn("Imported heading", outside)
        self.assertNotIn("<script>", outside)
        self.assertEqual(outside.count("## 案例 "), 1)

    def test_utf8_budget_is_exact_and_cli_emits_nothing_on_input_error(self):
        text = review.render(evaluate.DEFAULT)
        size = len(text.encode("utf-8"))
        self.assertGreater(size, len(text))
        self.assertEqual(review.render(evaluate.DEFAULT, size), text)
        with self.assertRaisesRegex(ValueError, "output budget"):
            review.render(evaluate.DEFAULT, size - 1)
        for limit in (0, True, review.MAX_OUTPUT_BYTES + 1):
            with self.subTest(limit=limit), patch("review.read_json") as read:
                with self.assertRaisesRegex(ValueError, "output limit"):
                    review.render(evaluate.DEFAULT, limit)
                read.assert_not_called()
        result = subprocess.run([sys.executable, review.__file__, "--suite", str(evaluate.DEFAULT),
                                 "--max-bytes", str(size - 1)], capture_output=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, b"")
        self.assertIn(b"review output budget", result.stderr)
        self.assertNotIn(b"Traceback", result.stderr)

    def test_changed_suite_between_reads_is_refused(self):
        case = self.native_case()
        original_evaluate = evaluate.evaluate
        with tempfile.TemporaryDirectory(prefix="synthetic-review-suite-change-") as directory:
            path = self.write_suite(Path(directory), case)

            def changed(selected):
                altered = copy.deepcopy(case)
                altered["case_id"] += "-changed"
                self.write_suite(Path(directory), altered)
                return original_evaluate(selected)

            with patch("review.evaluate", side_effect=changed):
                with self.assertRaisesRegex(ValueError, "suite changed"):
                    review.render(path)

    def test_changed_source_during_rendering_is_refused(self):
        original_scope_for = evaluate.scope_for

        def changed(case, root):
            scope = copy.deepcopy(original_scope_for(case, root))
            scope[0]["node"]["message"]["content"]["parts"] = ["合成已变更原文。"]
            return scope

        with patch("review.scope_for", side_effect=changed):
            with self.assertRaisesRegex(ValueError, "source changed"):
                review.render(evaluate.DEFAULT)


if __name__ == "__main__":
    unittest.main()

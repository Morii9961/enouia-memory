import copy
import hashlib
import itertools
import json
import tempfile
from pathlib import Path
import unittest

import check_native_matrix
import evaluate
import test_evaluate as fixtures


class NativeMatrixTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="synthetic-native-matrix-")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.rows = []
        for api, lineage in itertools.product(("openai", "anthropic"), (False, True)):
            builder = fixtures.AssessmentTests()
            builder.setUp()
            full = builder.native_analysis_case()
            body = json.loads(full["native_task_input_text"])
            body["binding"]["provider"] = api
            if not lineage:
                body.pop("selected_lineage")
            self.input_text(full, body)
            legacy = copy.deepcopy(full)
            legacy.pop("native_task_input_text")
            for complete, case in ((False, legacy), (True, full)):
                name = f'{api}-{"lineage" if lineage else "plain"}-{"complete" if complete else "legacy"}.json'
                suite = {"schema_version": evaluate.VERSION, "cases": [case]}
                row = {"file": name, "api": api, "lineage": lineage, "include_input": complete}
                self.write_suite(row, suite)
                self.rows.append(row)
        self.manifest()

    @staticmethod
    def input_text(case, body):
        case["native_task_input_text"] = json.dumps(body, ensure_ascii=False, separators=(",", ":"))
        case["native_task_input_sha256"] = hashlib.sha256(case["native_task_input_text"].encode()).hexdigest()

    def write_suite(self, row, suite):
        (self.root / row["file"]).write_text(json.dumps(suite, ensure_ascii=False), encoding="utf-8")
        row["suite_value_sha256"] = evaluate.digest(suite)

    def manifest(self):
        (self.root / "manifest.json").write_text(json.dumps({
            "version": "native-quality-matrix-1", "cases": self.rows}), encoding="utf-8")

    def test_complete_matrix_keeps_pending_and_no_quality_claim(self):
        report = check_native_matrix.check(self.root)
        self.assertEqual(report["fixtures_checked"], 8)
        self.assertIsNone(report["overall_model_quality"])
        self.assertEqual(report["native_validation"], "not_rerun")

    def test_partial_and_duplicate_combinations_are_refused(self):
        original = copy.deepcopy(self.rows)
        for rows, error in [(original[:-1], "coverage"), ([*original[:-1], original[0]], "identity")]:
            with self.subTest(error=error):
                self.rows = rows
                self.manifest()
                with self.assertRaisesRegex(ValueError, error):
                    check_native_matrix.check(self.root)

    def test_changed_fixture_digest_is_refused(self):
        self.rows[0]["suite_value_sha256"] = "0" * 64
        self.manifest()
        with self.assertRaisesRegex(ValueError, "digest mismatch"):
            check_native_matrix.check(self.root)

    def test_individually_valid_but_incompatible_pair_is_refused(self):
        row = self.rows[0]
        suite = evaluate.read_json(self.root / row["file"], 512 * 1024)
        suite["cases"][0]["case_id"] += "-changed"
        self.write_suite(row, suite)
        self.manifest()
        with self.assertRaisesRegex(ValueError, "builder compatibility"):
            check_native_matrix.check(self.root)

    def test_rehashed_input_cannot_hide_lineage_mode(self):
        row = next(r for r in self.rows if r["lineage"] and r["include_input"])
        suite = evaluate.read_json(self.root / row["file"], 512 * 1024)
        body = json.loads(suite["cases"][0]["native_task_input_text"])
        body.pop("selected_lineage")
        self.input_text(suite["cases"][0], body)
        self.write_suite(row, suite)
        self.manifest()
        with self.assertRaisesRegex(ValueError, "lineage visibility"):
            check_native_matrix.check(self.root)

    def test_pair_json_types_are_not_coerced(self):
        for row, value in ((self.rows[0], 1), (self.rows[1], True)):
            suite = evaluate.read_json(self.root / row["file"], 512 * 1024)
            suite["synthetic_marker"] = value
            self.write_suite(row, suite)
        self.manifest()
        with self.assertRaisesRegex(ValueError, "builder compatibility"):
            check_native_matrix.check(self.root)

    def test_fixture_provider_label_must_match_task_binding(self):
        complete = next(r for r in self.rows if r["api"] == "anthropic" and not r["lineage"] and r["include_input"])
        legacy = next(r for r in self.rows if r["api"] == "anthropic" and not r["lineage"] and not r["include_input"])
        suite = evaluate.read_json(self.root / complete["file"], 512 * 1024)
        body = json.loads(suite["cases"][0]["native_task_input_text"])
        body["binding"]["provider"] = "openai"
        self.input_text(suite["cases"][0], body)
        self.write_suite(complete, suite)
        reduced = copy.deepcopy(suite)
        reduced["cases"][0].pop("native_task_input_text")
        self.write_suite(legacy, reduced)
        self.manifest()
        with self.assertRaisesRegex(ValueError, "provider binding"):
            check_native_matrix.check(self.root)

    def test_path_escape_and_boolean_coercion_are_refused(self):
        original = copy.deepcopy(self.rows)
        for field, value, error in [("file", "../outside.json", "filename"), ("lineage", 0, "matrix row")]:
            with self.subTest(field=field):
                self.rows = copy.deepcopy(original)
                self.rows[0][field] = value
                self.manifest()
                with self.assertRaisesRegex(ValueError, error):
                    check_native_matrix.check(self.root)

    def replace_lineage(self, value):
        full = next(r for r in self.rows if r["lineage"] and r["include_input"])
        legacy = next(r for r in self.rows if r["api"] == full["api"] and r["lineage"] and not r["include_input"])
        suite = evaluate.read_json(self.root / full["file"], 512 * 1024)
        body = json.loads(suite["cases"][0]["native_task_input_text"])
        body["selected_lineage"] = value
        self.input_text(suite["cases"][0], body)
        self.write_suite(full, suite)
        reduced = copy.deepcopy(suite)
        reduced["cases"][0].pop("native_task_input_text")
        self.write_suite(legacy, reduced)
        self.manifest()

    def test_lineage_marker_requires_complete_row_array(self):
        for value in (None, "", {}, []):
            with self.subTest(value=value):
                self.replace_lineage(value)
                with self.assertRaisesRegex(ValueError, "lineage shape"):
                    check_native_matrix.check(self.root)

    def test_lineage_rows_require_strict_indices_and_parent_metadata(self):
        base = {"source_index": 0, "parent_source_indices": [], "unselected_parent_count": None}
        examples = [([base, base], "lineage shape")]
        for field, value in [("source_index", True), ("source_index", 1),
                             ("parent_source_indices", [True]), ("parent_source_indices", [1]),
                             ("parent_source_indices", [0, 0]), ("unselected_parent_count", True),
                             ("unselected_parent_count", -1), ("extra", None)]:
            row = copy.deepcopy(base)
            row[field] = value
            examples.append(([row], "lineage shape"))
        for value, error in examples:
            with self.subTest(value=value):
                self.replace_lineage(value)
                with self.assertRaisesRegex(ValueError, error):
                    check_native_matrix.check(self.root)

    def test_unknown_and_unselected_parent_counts_remain_valid(self):
        for count in (None, 0, 2):
            with self.subTest(count=count):
                self.replace_lineage([{"source_index": 0, "parent_source_indices": [],
                                      "unselected_parent_count": count}])
                report = check_native_matrix.check(self.root)
                self.assertIsNone(report["overall_model_quality"])
                self.assertEqual(report["native_validation"], "not_rerun")

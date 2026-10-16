import copy
import json
import hashlib
import subprocess
import sys
import tempfile
from pathlib import Path
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

    def test_response_collections_must_be_arrays_even_when_empty(self):
        original = self.native_case()
        for field in ("changes", "summary"):
            for value in ({}, ""):
                with self.subTest(field=field, value=value):
                    case = copy.deepcopy(original)
                    case["response"][field] = value
                    case["response_value_sha256"] = evaluate.digest(case["response"])
                    with self.assertRaisesRegex(ValueError, "response collections"):
                        evaluate.assess(case, case["source_scope"])

    def test_review_collections_must_be_arrays_even_when_empty(self):
        for field in ("item_judgments", "unit_judgments"):
            for value in ({}, ""):
                with self.subTest(field=field, value=value):
                    case = copy.deepcopy(self.case)
                    case[field] = value
                    with self.assertRaisesRegex(ValueError, "review collections"):
                        evaluate.assess(case, self.scope)

    def test_unit_paths_must_be_arrays_for_every_status(self):
        for status, paths in [("observed", {"/changes/0/statement": None}),
                              ("missing", {}), ("unknown", False), ("unknown", "")]:
            with self.subTest(status=status, paths=paths):
                case = copy.deepcopy(self.case)
                case["unit_judgments"][0].update(status=status, paths=paths)
                with self.assertRaisesRegex(ValueError, "unit paths"):
                    evaluate.assess(case, self.scope)

    def test_suite_cases_and_identity_have_explicit_shapes(self):
        for identity in ("", " ", True, 1, None, [], {}):
            with self.subTest(identity=identity):
                suite = copy.deepcopy(self.suite)
                suite["cases"][0]["case_id"] = identity
                with patch("evaluate.read_json", return_value=suite):
                    with self.assertRaisesRegex(ValueError, "case identity"):
                        evaluate.evaluate(evaluate.DEFAULT)
        for cases in ({}, "", [None]):
            with self.subTest(cases=cases), patch("evaluate.read_json", return_value={
                    "schema_version": evaluate.VERSION, "cases": cases}):
                with self.assertRaisesRegex(ValueError, "cases shape"):
                    evaluate.evaluate(evaluate.DEFAULT)

    def test_malformed_response_collection_cli_emits_no_metrics(self):
        case = self.native_case()
        valid = copy.deepcopy(case)
        case["case_id"] += "-invalid"
        case["response"]["changes"] = {}
        case["response_value_sha256"] = evaluate.digest(case["response"])
        with tempfile.TemporaryDirectory(prefix="synthetic-quality-collections-") as directory:
            suite = Path(directory) / "synthetic-suite.json"
            suite.write_text(json.dumps({"schema_version": evaluate.VERSION, "cases": [valid, case]},
                                        ensure_ascii=False), encoding="utf-8")
            result = subprocess.run([sys.executable, str(Path(evaluate.__file__)), "--suite", str(suite)],
                                    capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("invalid evaluation input: response collections", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

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

    def test_legacy_source_resolution_loops_fail_before_reading(self):
        root = evaluate.DEFAULT.parent
        target = root / self.case["source_file"]
        original_resolve = Path.resolve
        for blocked in (root, target):
            def resolve(path, *args, **kwargs):
                if path == blocked:
                    raise RuntimeError("synthetic source resolution loop")
                return original_resolve(path, *args, **kwargs)

            with self.subTest(path=blocked.name), patch.object(Path, "resolve", resolve), patch(
                    "evaluate.read_json") as read:
                with self.assertRaisesRegex(ValueError, "source path resolution"):
                    evaluate.scope_for(self.case, root)
                read.assert_not_called()

    def test_duplicate_json_keys_are_refused(self):
        with patch("pathlib.Path.open", return_value=BytesIO(b'{"case_id":"first","case_id":"second"}')):
            from pathlib import Path
            with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
                evaluate.read_json(Path("synthetic-duplicate.json"), 1000)

    def test_nonfinite_and_overflowing_json_numbers_are_refused(self):
        for number in ("NaN", "Infinity", "-Infinity", "1e400", "-1e400"):
            with self.subTest(number=number):
                with self.assertRaisesRegex(ValueError, "non-finite JSON number"):
                    evaluate.parse_json('{"number":' + number + '}')
        self.assertEqual(evaluate.parse_json('{"number":1.25,"zero":-0.0,"unknown":null}'),
                         {"number": 1.25, "zero": -0.0, "unknown": None})

    def test_json_depth_budget_ignores_quoted_and_escaped_brackets(self):
        limit = 128
        self.assertIsInstance(evaluate.parse_json("[" * limit + "0" + "]" * limit), list)
        with self.assertRaisesRegex(ValueError, "JSON nesting budget"):
            evaluate.parse_json("[" * (limit + 1) + "0" + "]" * (limit + 1))
        text = 'Synthetic quoted " text: ' + "[{]}" * 1000 + "\\"
        encoded = json.dumps({"text": text}, ensure_ascii=False)
        for data in (encoded, encoded.encode("utf-8"), encoded.encode("utf-16"), encoded.encode("utf-32")):
            self.assertEqual(evaluate.parse_json(data), {"text": text})
        with self.assertRaisesRegex(ValueError, "duplicate JSON key"):
            evaluate.parse_json(r'{"value":1,"\u0076alue":2}')

    def test_embedded_nonfinite_analysis_input_is_not_structurally_accepted(self):
        case = self.native_analysis_case()
        body = json.loads(case["native_task_input_text"])
        body["extra_number"] = float("nan")
        case["native_task_input_text"] = json.dumps(body, ensure_ascii=False)
        case["native_task_input_sha256"] = hashlib.sha256(case["native_task_input_text"].encode()).hexdigest()
        with self.assertRaisesRegex(ValueError, "non-finite JSON number"):
            evaluate.assess(case, case["source_scope"])

    def test_deep_embedded_input_cli_refuses_without_metrics_or_traceback(self):
        case = self.native_analysis_case()
        text = case["native_task_input_text"]
        case["native_task_input_text"] = text[:-1] + ',"extra_deep":' + "[" * 4096 + "0" + "]" * 4096 + "}"
        case["native_task_input_sha256"] = hashlib.sha256(case["native_task_input_text"].encode()).hexdigest()
        with tempfile.TemporaryDirectory(prefix="synthetic-quality-json-") as directory:
            suite = Path(directory) / "synthetic-suite.json"
            suite.write_text(json.dumps({"schema_version": evaluate.VERSION, "cases": [case]}, ensure_ascii=False),
                             encoding="utf-8")
            result = subprocess.run([sys.executable, str(Path(evaluate.__file__)), "--suite", str(suite)],
                                    capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("invalid evaluation input: JSON nesting budget", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

    def test_legacy_source_scope_refuses_malformed_shapes(self):
        ref = self.case["messages"][0]
        examples = [({"malformed": True}, "source archive shape"),
                    ([1], "source conversation shape"),
                    ([{"id": ref["conversation_id"], "mapping": []}], "source mapping shape"),
                    ([{"id": ref["conversation_id"], "mapping": {ref["message_id"]: True}}], "selected message shape"),
                    ([{"id": ref["conversation_id"], "mapping": {ref["message_id"]:
                      {"message": {"content": {"parts": "not an array"}}}}}], "selected message shape")]
        for raw, error in examples:
            with self.subTest(raw=raw), patch("evaluate.read_json", return_value=raw):
                with self.assertRaisesRegex(ValueError, error):
                    evaluate.scope_for(self.case, evaluate.DEFAULT.parent)

    def test_legacy_source_scope_preserves_attachment_metadata(self):
        conversations = {}
        for original in self.scope:
            node = copy.deepcopy(original["node"])
            node["message"]["content"]["parts"].append({"synthetic_attachment": {"missing": True}})
            conversation = conversations.setdefault(original["conversation_id"],
                                                     {"id": original["conversation_id"], "mapping": {}})
            conversation["mapping"][original["message_id"]] = node
        with patch("evaluate.read_json", return_value=list(conversations.values())):
            scope = evaluate.scope_for(self.case, evaluate.DEFAULT.parent)
        self.assertEqual(scope[0]["node"]["message"]["content"]["parts"][-1],
                         {"synthetic_attachment": {"missing": True}})
        self.case["scope_value_sha256"] = evaluate.digest(scope)
        self.assertEqual(evaluate.assess(self.case, scope)["review_status"], "supported_in_review")

    def test_malformed_legacy_source_cli_refuses_without_traceback(self):
        case = copy.deepcopy(self.case)
        ref = case["messages"][0]
        case["source_file"] = "synthetic-source.json"
        with tempfile.TemporaryDirectory(prefix="synthetic-quality-shape-") as directory:
            root = Path(directory)
            (root / case["source_file"]).write_text(json.dumps([{"id": ref["conversation_id"], "mapping": []}]),
                                                   encoding="utf-8")
            suite = root / "synthetic-suite.json"
            suite.write_text(json.dumps({"schema_version": evaluate.VERSION, "cases": [case]}, ensure_ascii=False),
                             encoding="utf-8")
            result = subprocess.run([sys.executable, str(Path(evaluate.__file__)), "--suite", str(suite)],
                                    capture_output=True, text=True, timeout=10)
        self.assertEqual(result.returncode, 1)
        self.assertEqual(result.stdout, "")
        self.assertIn("invalid evaluation input: source mapping shape", result.stderr)
        self.assertNotIn("Traceback", result.stderr)

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

    def native_analysis_case(self):
        case = self.native_case()
        selected = case["source_scope"][0]
        selected["content_hash"] = selected["snippet_hash"]
        case["scope_value_sha256"] = evaluate.digest(case["source_scope"])
        body = {"version": "history-semantic-review-1", "task": "Synthetic analysis instructions.",
                "sources": [{"source": selected["source"], "start": selected["selection"]["start"],
                             "end": selected["selection"]["end"], "content_hash": selected["content_hash"],
                             "snippet_hash": selected["snippet_hash"]}],
                "contexts": [{"source_index": 0, "text": selected["text"]}],
                "suggestions": [], "runs": [], "binding": {}, "topic": {},
                "selected_lineage": [{"source_index": 0, "parent_source_indices": [],
                                      "unselected_parent_count": None}]}
        text = json.dumps(body, ensure_ascii=False, separators=(",", ":"))
        case["native_task_input_text"] = text
        case["native_task_input_sha256"] = hashlib.sha256(text.encode()).hexdigest()
        return case

    def test_analysis_input_is_exact_hash_bound_but_does_not_judge(self):
        case = self.native_analysis_case()
        result = evaluate.assess(case, case["source_scope"])
        self.assertEqual(result["analysis_input_inspection"], "included_hash_and_scope_matched")
        self.assertEqual(result["review_status"], "review_required")
        self.assertIsNone(result["supported_fraction_of_adjudicated_items"])
        self.assertEqual(result["required_units"]["unknown"], len(case["required_units"]))
        case["native_task_input_text"] += " "
        with self.assertRaisesRegex(ValueError, "analysis input binding"):
            evaluate.assess(case, case["source_scope"])

    def test_analysis_input_refuses_duplicate_keys_wrong_shape_and_non_native_scope(self):
        case = self.native_analysis_case()
        original = case["native_task_input_text"]
        duplicate = original.replace('"selected_lineage":', '"version":"other","selected_lineage":')
        for text, error in [(duplicate, "duplicate JSON key"), ("[]", "analysis input shape"),
                            ('{"version":"other"}', "analysis input shape")]:
            with self.subTest(text=text):
                bad = copy.deepcopy(case)
                bad["native_task_input_text"] = text
                bad["native_task_input_sha256"] = hashlib.sha256(text.encode()).hexdigest()
                with self.assertRaisesRegex(ValueError, error):
                    evaluate.assess(bad, bad["source_scope"])
        case["scope_kind"] = "other"
        with self.assertRaisesRegex(ValueError, "needs native scope"):
            evaluate.assess(case, case["source_scope"])

    def test_analysis_input_cannot_disagree_with_selected_assessment_scope(self):
        case = self.native_analysis_case()
        original = json.loads(case["native_task_input_text"])
        for field, value in [("text", "Different synthetic input."), ("source_index", True),
                             ("speaker_role", "assistant")]:
            with self.subTest(field=field):
                body = copy.deepcopy(original)
                body["contexts"][0][field] = value
                bad = copy.deepcopy(case)
                bad["native_task_input_text"] = json.dumps(body, ensure_ascii=False)
                bad["native_task_input_sha256"] = hashlib.sha256(bad["native_task_input_text"].encode()).hexdigest()
                with self.assertRaisesRegex(ValueError, "analysis input scope"):
                    evaluate.assess(bad, bad["source_scope"])
        body = copy.deepcopy(original)
        body["sources"][0]["source"]["source_revision"] = True
        bad = copy.deepcopy(case)
        bad["native_task_input_text"] = json.dumps(body, ensure_ascii=False)
        bad["native_task_input_sha256"] = hashlib.sha256(bad["native_task_input_text"].encode()).hexdigest()
        with self.assertRaisesRegex(ValueError, "analysis input scope"):
            evaluate.assess(bad, bad["source_scope"])

    def test_analysis_input_budget_counts_utf8_bytes_and_legacy_suites_stay_readable(self):
        case = self.native_analysis_case()
        legacy = copy.deepcopy(case)
        legacy.pop("native_task_input_text")
        case["native_task_input_text"] = "合成" * (evaluate.MAX_ANALYSIS_INPUT_BYTES // 4)
        case["native_task_input_sha256"] = hashlib.sha256(case["native_task_input_text"].encode()).hexdigest()
        with self.assertRaisesRegex(ValueError, "analysis input budget"):
            evaluate.assess(case, case["source_scope"])
        self.assertEqual(evaluate.assess(legacy, legacy["source_scope"])["analysis_input_inspection"],
                         "not_included")


if __name__ == "__main__":
    unittest.main()

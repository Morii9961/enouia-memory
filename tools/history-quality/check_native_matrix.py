"""Cross-check explicitly generated synthetic native quality fixtures."""
import argparse
import copy
import itertools
import json
from pathlib import Path

from evaluate import digest, evaluate, lineage_shape, parse_json, read_json, require


def contained_path(root, filename, kind):
    try:
        path = (root / filename).resolve()
        inside = path.is_relative_to(root.resolve())
    except RuntimeError as exc:
        raise ValueError("matrix path resolution failed") from exc
    require(inside, f"matrix {kind} outside root")
    return path


def check(root):
    root = Path(root)
    manifest_path = contained_path(root, "manifest.json", "manifest")
    manifest = read_json(manifest_path, 16 * 1024)
    require(isinstance(manifest, dict) and manifest.get("version") == "native-quality-matrix-1",
            "matrix version")
    rows = manifest.get("cases")
    require(isinstance(rows, list) and len(rows) == 8, "matrix coverage")
    expected = set(itertools.product(("openai", "anthropic"), (False, True), (False, True)))
    suites = {}
    for row in rows:
        require(isinstance(row, dict) and isinstance(row.get("api"), str)
                and type(row.get("lineage")) is bool and type(row.get("include_input")) is bool,
                "matrix row")
        key = (row["api"], row["lineage"], row["include_input"])
        require(key in expected and key not in suites, "matrix identity")
        filename = f'{key[0]}-{"lineage" if key[1] else "plain"}-{"complete" if key[2] else "legacy"}.json'
        require(row.get("file") == filename, "matrix filename")
        path = contained_path(root, filename, "file")
        suite = read_json(path, 512 * 1024)
        report = evaluate(path)
        require(digest(suite) == report["suite_value_sha256"], "matrix fixture changed during reading")
        require(report["suite_value_sha256"] == row.get("suite_value_sha256"), "native suite digest mismatch")
        require(report["native_validation"] == "not_rerun" and report["overall_model_quality"] is None,
                "matrix cannot authenticate or grade")
        require(len(report["cases"]) == 1, "matrix case count")
        case, result = suite["cases"][0], report["cases"][0]
        require(case["evidence_mode"] == "synthetic" and result["reviewer"] == "pending"
                and case["item_judgments"] == case["unit_judgments"] == []
                and result["review_status"] == "review_required"
                and result["required_units"]["unknown"] == len(case["required_units"]), "matrix pending states")
        require(result["analysis_input_inspection"] == (
            "included_hash_and_scope_matched" if key[2] else "not_included"), "matrix input status")
        if key[2]:
            body = parse_json(case["native_task_input_text"])
            require(body["binding"].get("provider") == key[0], "matrix provider binding")
            require(("selected_lineage" in body) == key[1], "matrix lineage visibility")
            if key[1]:
                lineage_shape(body)
        else:
            require("native_task_input_text" not in case, "legacy matrix input")
        suites[key] = suite
    require(set(suites) == expected, "matrix coverage")
    for api, lineage in itertools.product(("openai", "anthropic"), (False, True)):
        reduced = copy.deepcopy(suites[(api, lineage, True)])
        reduced["cases"][0].pop("native_task_input_text")
        require(digest(reduced) == digest(suites[(api, lineage, False)]), "matrix builder compatibility")
    for api in ("openai", "anthropic"):
        require(suites[(api, False, True)]["cases"][0]["native_task_input_sha256"]
                != suites[(api, True, True)]["cases"][0]["native_task_input_sha256"], "matrix lineage task binding")
    return {"version": "native-quality-matrix-1", "fixtures_checked": 8,
            "native_validation": "not_rerun", "overall_model_quality": None,
            "review_status": "review_required"}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", type=Path, required=True)
    args = parser.parse_args()
    try:
        print(json.dumps(check(args.root), indent=2))
    except (ValueError, KeyError, TypeError, OSError) as exc:
        parser.exit(1, f"invalid synthetic matrix: {exc}\n")

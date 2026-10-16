"""Compare explicit review revisions without modifying or grading them."""
import argparse
import json
from pathlib import Path
import sys

from evaluate import digest, evaluate, read_json, require

REVIEW_FIELDS = {"reviewer", "item_judgments", "unit_judgments"}
SUMMARY_FIELDS = ("reviewer", "review_status", "items", "required_units",
                  "adjudicated_item_count", "supported_fraction_of_adjudicated_items",
                  "observed_fraction_of_required_units", "all_required_units_reviewed")
MAX_REPORT_BYTES = 128 * 1024


def snapshot(path):
    path = Path(path)
    suite = read_json(path, 512 * 1024)
    report = evaluate(path)
    require(digest(suite) == report["suite_value_sha256"], "review snapshot changed during reading")
    return suite, report


def changes(before, after, locations):
    return {"added": [location for key, location in locations if key not in before and key in after],
            "edited": [location for key, location in locations if key in before and key in after
                       and digest(before[key]) != digest(after[key])],
            "removed": [location for key, location in locations if key in before and key not in after]}


def compare(before_path, after_path):
    before, before_report = snapshot(before_path)
    after, after_report = snapshot(after_path)
    require(digest({key: value for key, value in before.items() if key != "cases"})
            == digest({key: value for key, value in after.items() if key != "cases"}),
            "review suite metadata changed")
    right = {case["case_id"]: (index, case, assessment) for index, (case, assessment) in enumerate(
        zip(after["cases"], after_report["cases"]))}
    require(set(right) == {case["case_id"] for case in before["cases"]}, "review case identities changed")
    cases = []
    for index, (old, old_assessment) in enumerate(zip(before["cases"], before_report["cases"])):
        after_index, new, new_assessment = right[old["case_id"]]
        immutable = {key: value for key, value in old.items() if key not in REVIEW_FIELDS}
        require(digest(immutable) == digest({key: value for key, value in new.items()
                                            if key not in REVIEW_FIELDS}), "review case input changed")
        old_items = {item["path"]: item for item in old["item_judgments"]}
        new_items = {item["path"]: item for item in new["item_judgments"]}
        item_locations = [(path, path) for path in sorted(set(old_items) | set(new_items))]
        old_units = {unit["unit_id"]: unit for unit in old["unit_judgments"]}
        new_units = {unit["unit_id"]: unit for unit in new["unit_judgments"]}
        unit_locations = [(unit, position) for position, unit in enumerate(old["required_units"])]
        item_delta = changes(old_items, new_items, item_locations)
        unit_delta = changes(old_units, new_units, unit_locations)
        changed = old["reviewer"] != new["reviewer"] or any(item_delta.values()) or any(unit_delta.values())
        cases.append({"before_index": index, "after_index": after_index,
                      "case_identity_sha256": digest(old["case_id"]), "input_value_sha256": digest(immutable),
                      "review_changed": bool(changed), "item_paths": item_delta, "unit_indices": unit_delta,
                      "before": {key: old_assessment[key] for key in SUMMARY_FIELDS},
                      "after": {key: new_assessment[key] for key in SUMMARY_FIELDS}})
    return {"format": "history-quality-review-comparison-1",
            "before_suite_value_sha256": before_report["suite_value_sha256"],
            "after_suite_value_sha256": after_report["suite_value_sha256"],
            "case_count": len(cases), "changed_case_count": sum(case["review_changed"] for case in cases),
            "native_validation": "not_rerun", "overall_model_quality": None, "cases": cases}


def report_bytes(report):
    data = (json.dumps(report, ensure_ascii=False, indent=2) + "\n").encode("utf-8")
    require(len(data) <= MAX_REPORT_BYTES, "review comparison output budget")
    return data


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--before", type=Path, required=True)
    parser.add_argument("--after", type=Path, required=True)
    args = parser.parse_args()
    try:
        output = report_bytes(compare(args.before, args.after))
    except (ValueError, KeyError, TypeError, OSError) as exc:
        parser.exit(1, f"invalid review comparison: {exc}\n")
    sys.stdout.buffer.write(output)

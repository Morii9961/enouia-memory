"""Calculate review metrics from explicit judgments; no semantic auto-grading."""
import argparse
import hashlib
import json
import math
from pathlib import Path

VERSION = "history-quality-1"
DEFAULT = Path(__file__).resolve().parents[2] / "tests/fixtures/history/quality-cases.json"
MAX_ANALYSIS_INPUT_BYTES = 64 * 1024
MAX_JSON_DEPTH = 128


def digest(value):
    return hashlib.sha256(json.dumps(value, ensure_ascii=False, sort_keys=True,
                                     separators=(",", ":")).encode()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def parse_json(data):
    if isinstance(data, (bytes, bytearray)):
        data = data.decode(json.detect_encoding(data), "surrogatepass")
    depth, quoted, escaped = 0, False, False
    for char in data:
        if quoted:
            if escaped:
                escaped = False
            elif char == "\\":
                escaped = True
            elif char == '"':
                quoted = False
        elif char == '"':
            quoted = True
        elif char in "[{":
            depth += 1
            require(depth <= MAX_JSON_DEPTH, "JSON nesting budget")
        elif char in "]}":
            depth -= 1

    def unique_object(pairs):
        value = {}
        for key, item in pairs:
            require(key not in value, "duplicate JSON key")
            value[key] = item
        return value
    def finite_float(text):
        value = float(text)
        require(math.isfinite(value), "non-finite JSON number")
        return value

    def reject_constant(_text):
        raise ValueError("non-finite JSON number")

    try:
        return json.loads(data, object_pairs_hook=unique_object,
                          parse_float=finite_float, parse_constant=reject_constant)
    except RecursionError as exc:
        raise ValueError("JSON nesting budget") from exc


def read_json(path, limit):
    with path.open("rb") as stream:
        data = stream.read(limit + 1)
    require(len(data) <= limit, "input budget")
    return parse_json(data)


def analysis_input_status(case, scope):
    if "native_task_input_text" not in case:
        return "not_included"
    require(case.get("scope_kind") == "native_selected_spans_v1", "analysis input needs native scope")
    text = case["native_task_input_text"]
    require(isinstance(text, str), "analysis input text")
    raw = text.encode("utf-8")
    require(0 < len(raw) <= MAX_ANALYSIS_INPUT_BYTES, "analysis input budget")
    require(hashlib.sha256(raw).hexdigest() == case.get("native_task_input_sha256"),
            "analysis input binding changed")
    body = parse_json(text)
    require(isinstance(body, dict) and body.get("version") == "history-semantic-review-1"
            and isinstance(body.get("task"), str) and body["task"].strip()
            and all(isinstance(body.get(field), list) for field in ("sources", "contexts", "suggestions", "runs"))
            and all(isinstance(body.get(field), dict) for field in ("binding", "topic")), "analysis input shape")
    require(len(body["sources"]) == len(body["contexts"]) == len(scope), "analysis input scope size")
    for index, (source, context, selected) in enumerate(zip(body["sources"], body["contexts"], scope)):
        require(isinstance(source, dict) and isinstance(context, dict)
                and digest(source.get("source")) == digest(selected["source"])
                and type(source.get("start")) is int and source["start"] == selected["selection"]["start"]
                and type(source.get("end")) is int and source["end"] == selected["selection"]["end"]
                and source.get("content_hash") == selected.get("content_hash")
                and source.get("snippet_hash") == selected["snippet_hash"]
                and type(context.get("source_index")) is int and context["source_index"] == index
                and context.get("text") == selected["text"]
                and all(digest(context.get(field)) == digest(selected.get(field))
                        for field in ("speaker_role", "evidence_class", "source_context")), "analysis input scope changed")
    return "included_hash_and_scope_matched"


def scope_for(case, root):
    if "source_scope" in case:
        require(case.get("scope_kind") == "native_selected_spans_v1"
                and "source_file" not in case and "messages" not in case, "ambiguous native scope")
        selected = case["source_scope"]
        require(isinstance(selected, list) and 0 < len(selected) <= 16, "source selection budget")
        require(len(json.dumps(selected, ensure_ascii=False).encode()) <= 128 * 1024, "native scope budget")
        seen = set()
        for item in selected:
            ref, span, text = item["source"], item["selection"], item["text"]
            start, end = span["start"], span["end"]
            require(isinstance(text, str) and type(start) is int and type(end) is int
                    and 0 <= start < end and end - start == len(text.encode()), "native span")
            require(item["snippet_hash"] == hashlib.sha256(text.encode()).hexdigest(), "native snippet binding")
            require(isinstance(ref["source_id"], str) and type(ref["source_revision"]) is int
                    and ref["source_revision"] > 0, "native source ref")
            key = (ref["source_id"], ref["source_revision"], start, end)
            require(key not in seen, "duplicate native span")
            seen.add(key)
        return selected
    target = (root / case["source_file"]).resolve()
    require(target.is_relative_to(root.resolve()), "source outside suite directory")
    raw = read_json(target, 2 * 1024 * 1024)
    require(isinstance(raw, list), "source archive shape")
    require(all(isinstance(conversation, dict) for conversation in raw), "source conversation shape")
    selected = []
    require(0 < len(case["messages"]) <= 16, "source selection budget")
    seen = set()
    for ref in case["messages"]:
        require(isinstance(ref, dict)
                and all(isinstance(ref.get(field), str) and ref[field]
                        for field in ("conversation_id", "message_id")), "source message ref")
        key = (ref["conversation_id"], ref["message_id"])
        require(key not in seen, "duplicate source message")
        seen.add(key)
        conversations = [c for c in raw if c.get("id") == key[0]]
        require(len(conversations) == 1, "unknown/ambiguous conversation")
        mapping = conversations[0].get("mapping")
        require(isinstance(mapping, dict), "source mapping shape")
        node = mapping.get(key[1])
        require(node is not None, "unknown source message")
        require(isinstance(node, dict) and isinstance(node.get("message"), dict)
                and isinstance(node["message"].get("content"), dict)
                and isinstance(node["message"]["content"].get("parts"), list), "selected message shape")
        selected.append({"conversation_id": key[0], "message_id": key[1], "node": node})
    return selected


def item_paths(response):
    require(response["version"] == "history-semantic-review-1", "response version")
    require(len(response["changes"]) <= 16 and len(response["summary"]) <= 8, "response budget")
    result = {}
    for section, fields in [("changes", ("statement", "explanation")), ("summary", ("statement",))]:
        for i, item in enumerate(response[section]):
            for field in fields:
                text = item[field]
                require(isinstance(text, str) and 0 < len(text.encode()) <= 4096, "statement budget")
                result[f"/{section}/{i}/{field}"] = text
    return result


def assess(case, scope):
    require(case["scope_value_sha256"] == digest(scope), "scope judgment binding changed")
    require(case["response_value_sha256"] == digest(case["response"]), "response judgment binding changed")
    input_status = analysis_input_status(case, scope)
    require(case["evidence_mode"] in ("synthetic", "owner_selected"), "evidence mode")
    require(case["reviewer"] in ("synthetic_fixture", "owner", "pending"), "reviewer")
    if case["reviewer"] == "pending":
        require(case["item_judgments"] == [] and case["unit_judgments"] == [], "pending reviewer cannot judge")
    if case["evidence_mode"] == "synthetic":
        require(case["reviewer"] in ("synthetic_fixture", "pending"), "synthetic fixture is not owner acceptance")
    if case["evidence_mode"] == "owner_selected":
        require(case["reviewer"] in ("owner", "pending") and isinstance(case.get("native_task_input_sha256"), str)
                and len(case["native_task_input_sha256"]) == 64
                and all(c in "0123456789abcdef" for c in case["native_task_input_sha256"]), "native sample binding required")
    paths = item_paths(case["response"])
    verdicts = {}
    for review in case["item_judgments"]:
        path, verdict = review["path"], review["verdict"]
        require(path in paths and path not in verdicts, "invalid/duplicate reviewed path")
        require(verdict in ("supported", "contradicted", "unsupported", "uncertain"), "item verdict")
        require(isinstance(review["reason"], str) and review["reason"].strip(), "review reason required")
        require(0 < len(review["evidence"]) <= 4, "review evidence required")
        for evidence in review["evidence"]:
            i, quote = evidence["source_index"], evidence["quote"]
            require(type(i) is int and 0 <= i < len(scope), "review evidence index")
            text = scope[i]["text"] if case.get("scope_kind") == "native_selected_spans_v1" else "\n".join(p for p in scope[i]["node"]["message"]["content"]["parts"] if isinstance(p, str))
            require(isinstance(quote, str) and quote.strip() and quote in text, "review quote not in source")
        verdicts[path] = verdict
    units = case["required_units"]
    require(isinstance(units, list) and 0 < len(units) <= 32 and all(isinstance(u, str) and u.strip() for u in units)
            and len(set(units)) == len(units), "required units")
    unit_states = {}
    for judgment in case["unit_judgments"]:
        unit, status = judgment["unit_id"], judgment["status"]
        require(unit in units and unit not in unit_states, "invalid/duplicate unit")
        require(status in ("observed", "missing", "unknown"), "unit status")
        require(isinstance(judgment["reason"], str) and judgment["reason"].strip(), "unit reason required")
        if status == "observed":
            require(judgment["paths"] and all(verdicts.get(p) == "supported" for p in judgment["paths"]), "observed unit needs supported paths")
        else:
            require(not judgment["paths"], "missing/unknown unit cannot claim response paths")
        unit_states[unit] = status
    counts = {v: sum(x == v for x in verdicts.values()) for v in ("supported", "contradicted", "unsupported", "uncertain")}
    counts["unreviewed"] = len(paths) - len(verdicts)
    coverage = {v: sum(unit_states.get(u, "unknown") == v for u in units) for v in ("observed", "missing", "unknown")}
    adjudicated = counts["supported"] + counts["contradicted"] + counts["unsupported"]
    unresolved = counts["unreviewed"] + counts["uncertain"] + coverage["unknown"]
    issues = counts["contradicted"] + counts["unsupported"] + coverage["missing"]
    status = "issues_found" if issues else "review_required" if unresolved else "supported_in_review"
    return {"case_id": case["case_id"], "evidence_mode": case["evidence_mode"], "reviewer": case["reviewer"],
            "scope_value_sha256": digest(scope), "response_value_sha256": digest(case["response"]),
            "native_task_input_sha256": case.get("native_task_input_sha256"), "analysis_input_inspection": input_status,
            "review_status": status,
            "items": counts, "required_units": coverage, "adjudicated_item_count": adjudicated,
            "supported_fraction_of_adjudicated_items": counts["supported"] / adjudicated if adjudicated else None,
            "observed_fraction_of_required_units": coverage["observed"] / len(units),
            "all_required_units_reviewed": coverage["unknown"] == 0}


def evaluate(path):
    path = Path(path)
    suite = read_json(path, 512 * 1024)
    require(suite["schema_version"] == VERSION, "suite version")
    cases = suite["cases"]
    require(0 < len(cases) <= 32 and len({c["case_id"] for c in cases}) == len(cases), "case budget/identity")
    return {"schema_version": VERSION, "suite_value_sha256": digest(suite), "native_validation": "not_rerun",
            "overall_model_quality": None, "cases": [assess(c, scope_for(c, path.parent)) for c in cases]}


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--suite", type=Path, default=DEFAULT)
    args = parser.parse_args()
    try:
        print(json.dumps(evaluate(args.suite), ensure_ascii=False, indent=2))
    except (ValueError, KeyError, TypeError, OSError) as exc:
        parser.exit(1, f"invalid evaluation input: {exc}\n")

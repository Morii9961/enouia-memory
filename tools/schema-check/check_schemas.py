"""Independent JSON Schema cross-check (python-jsonschema, Draft 2020-12).

The Rust test suite validates fixtures with an in-repository subset validator.
This script re-checks the same schemas and fixture manifests with a mature,
independently written implementation, so both validators must agree on every
positive and negative case. It also validates every schema against the 2020-12
metaschema. `format` is an annotation here, as in the Rust harness.

Usage (from the repository root, with the pinned requirements installed):
    python tools/schema-check/check_schemas.py
"""
import copy
import importlib.metadata
import json
import pathlib
import sys

from jsonschema import Draft202012Validator
from referencing import Registry, Resource
from referencing.jsonschema import DRAFT202012

ROOT = pathlib.Path(__file__).resolve().parents[2]
CONTRACTS = ROOT / "contracts"
FIXTURES = ROOT / "tests" / "fixtures" / "memory"
EXPECTED_VERSION = "4.26.0"


def load(path):
    return json.loads(path.read_text(encoding="utf-8"))


def build():
    schemas, resources = {}, []
    for path in sorted(CONTRACTS.glob("*/*.json")):
        schema = load(path)
        Draft202012Validator.check_schema(schema)
        rel = path.relative_to(CONTRACTS).as_posix()
        schemas[rel] = schema
        resources.append((schema["$id"], Resource.from_contents(schema, default_specification=DRAFT202012)))
    registry = Registry().with_resources(resources)
    validators = {rel: Draft202012Validator(s, registry=registry) for rel, s in schemas.items()}
    return validators


def apply_ops(root, ops):
    def parent_key(path):
        parent, key = path.rsplit("/", 1)
        return resolve(root, parent), key

    for op in ops:
        kind, path = op["op"], op["path"]
        if kind == "set":
            container, key = parent_key(path)
            if isinstance(container, list):
                container[int(key)] = copy.deepcopy(op["value"])
            else:
                container[key] = copy.deepcopy(op["value"])
        elif kind == "remove":
            container, key = parent_key(path)
            if isinstance(container, list):
                del container[int(key)]
            else:
                del container[key]
        elif kind == "append":
            value = copy.deepcopy(resolve(root, op["value_from"]) if "value_from" in op else op["value"])
            if "then" in op:
                wrapper = {"v": value}
                apply_ops(wrapper, [dict(t, path="/v" + t["path"]) for t in op["then"]])
                value = wrapper["v"]
            resolve(root, path).append(value)
        elif kind == "repeat":
            items = resolve(root, path)
            while len(items) < op["count"]:
                items.append(copy.deepcopy(items[0]))
        else:
            raise ValueError(kind)


def resolve(root, pointer):
    node = root
    for part in [p for p in pointer.split("/") if p != ""]:
        node = node[int(part)] if isinstance(node, list) else node[part]
    return node


SET_SCHEMAS = {
    "sources": "memory/source-v1.schema.json", "attachments": "memory/attachment-v1.schema.json",
    "projects": "memory/project-v1.schema.json", "memories": "memory/memory-v1.schema.json",
    "candidates": "memory/candidate-v1.schema.json", "reviews": "memory/review-v1.schema.json",
    "identities": "memory/identity-v1.schema.json", "sessions": "memory/session-v1.schema.json",
    "session_events": "memory/session-event-v1.schema.json", "checkpoints": "memory/checkpoint-v1.schema.json",
    "commits": "memory/commit-v1.schema.json", "tombstones": "memory/tombstone-v1.schema.json",
    "purge_receipts": "memory/purge-receipt-v1.schema.json", "audit_events": "memory/audit-event-v1.schema.json",
    "capsules": "context/capsule-v1.schema.json", "inspections": "context/inspection-v1.schema.json",
    "dispatches": "context/dispatch-v1.schema.json", "approvals": "memory/approval-v1.schema.json",
    "policies": "memory/policy-v1.schema.json", "imports": "memory/import-v1.schema.json",
}


def main():
    version = importlib.metadata.version("jsonschema")
    if version != EXPECTED_VERSION:
        sys.exit(f"jsonschema {version} installed; pinned {EXPECTED_VERSION}")
    validators = build()
    failures, counts = [], {"schemas": len(validators)}

    def valid(schema, instance):
        return validators[schema].is_valid(instance)

    records = load(FIXTURES / "records-manifest.json")
    for entry in records["valid"]:
        if not valid(entry["schema"], load(FIXTURES / entry["file"])):
            err = next(validators[entry["schema"]].iter_errors(load(FIXTURES / entry["file"])))
            failures.append(f"valid record rejected: {entry['file']}: {err.message[:160]}")
    counts["valid_records"] = len(records["valid"])
    schema_of = {e["file"]: e["schema"] for e in records["valid"]}
    for case in records["invalid"]:
        value = load(FIXTURES / case["base"])
        apply_ops(value, case["ops"])
        accepted = valid(schema_of[case["base"]], value)
        if accepted != (case["schema"] == "accept"):
            failures.append(f"record case {case['id']}: independent validator accepted={accepted}, manifest says {case['schema']}")
    counts["record_cases"] = len(records["invalid"])

    sets = load(FIXTURES / "sets-manifest.json")
    checked = 0
    set_docs = [(f, load(FIXTURES / f)) for f in sets["valid"]]
    for case in sets["invalid"]:
        doc = load(FIXTURES / case["base"])
        apply_ops(doc, case["ops"])
        set_docs.append((case["id"], doc))
    for name, doc in set_docs:
        for key, schema in SET_SCHEMAS.items():
            for index, record in enumerate(doc.get(key, [])):
                checked += 1
                if not valid(schema, record):
                    err = next(validators[schema].iter_errors(record))
                    failures.append(f"set {name} {key}/{index}: {err.message[:160]}")
    counts["set_records"] = checked

    ipc = load(FIXTURES / "ipc-manifest.json")
    schema = "ipc/memory-v1.schema.json"
    for request in ipc["valid_requests"]:
        if not valid(schema, request):
            failures.append(f"valid request rejected: {request['operation']}")
    for response in ipc["valid_responses"]:
        if not valid(schema, response["message"]):
            failures.append(f"valid response rejected: {response['message']['kind']}")
    for group, base_key, pick in (("invalid_requests", "valid_requests", lambda v: v),
                                  ("invalid_responses", "valid_responses", lambda v: v["message"])):
        for case in ipc[group]:
            value = copy.deepcopy(ipc[base_key][case["base"]])
            apply_ops(value, case["ops"])
            accepted = valid(schema, pick(value))
            if accepted != (case["schema"] == "accept"):
                failures.append(f"ipc case {case['id']}: independent validator accepted={accepted}, manifest says {case['schema']}")
    counts["ipc_messages"] = sum(len(ipc[k]) for k in ("valid_requests", "valid_responses", "invalid_requests", "invalid_responses"))

    store_dir = ROOT / "tests" / "fixtures" / "store"
    store = load(store_dir / "store-manifest.json")
    for entry in store["valid"]:
        if not valid(entry["schema"], load(store_dir / entry["file"])):
            err = next(validators[entry["schema"]].iter_errors(load(store_dir / entry["file"])))
            failures.append(f"valid store document rejected: {entry['file']}: {err.message[:160]}")
    store_schema_of = {e["file"]: e["schema"] for e in store["valid"]}
    for case in store["invalid"]:
        value = load(store_dir / case["base"])
        apply_ops(value, case["ops"])
        accepted = valid(store_schema_of[case["base"]], value)
        if accepted != (case["schema"] == "accept"):
            failures.append(f"store case {case['id']}: independent validator accepted={accepted}, manifest says {case['schema']}")
    counts["store_documents"] = len(store["valid"]) + len(store["invalid"])

    print(f"python-jsonschema {version}: " + ", ".join(f"{k}={v}" for k, v in counts.items()))
    for failure in failures:
        print("FAIL", failure)
    print("OK" if not failures else f"{len(failures)} disagreement(s)")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()

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

    ws = load(FIXTURES / "workspace-manifest.json")
    schema = "ipc/workspace-v1.schema.json"
    for request in ws["valid_requests"]:
        if not valid(schema, request):
            failures.append(f"valid workspace request rejected: {request['command']}")
    for response in ws["valid_responses"]:
        if not valid(schema, response["message"]):
            failures.append(f"valid workspace response rejected: {response['message']['kind']}")
    for group, base_key, pick in (("invalid_requests", "valid_requests", lambda v: v),
                                  ("invalid_responses", "valid_responses", lambda v: v["message"])):
        for case in ws[group]:
            value = copy.deepcopy(ws[base_key][case["base"]])
            apply_ops(value, case["ops"])
            accepted = valid(schema, pick(value))
            if accepted != (case["schema"] == "accept"):
                failures.append(f"workspace case {case['id']}: independent validator accepted={accepted}, manifest says {case['schema']}")
    counts["workspace_messages"] = sum(len(ws[k]) for k in ("valid_requests", "valid_responses", "invalid_requests", "invalid_responses"))

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

    host_dir = ROOT / "tests" / "fixtures" / "host"
    host = load(host_dir / "manifest.json")
    host_schema_of = {entry["file"]: entry["schema"] for entry in host["valid"]}
    for entry in host["valid"]:
        if not valid(entry["schema"], load(host_dir / entry["file"])):
            failures.append(f"valid host document rejected: {entry['file']}")
    for case in host["invalid"]:
        value = load(host_dir / case["base"])
        apply_ops(value, case["ops"])
        accepted = valid(host_schema_of[case["base"]], value)
        if accepted != (case["schema"] == "accept"):
            failures.append(f"host case {case['id']}: independent validator accepted={accepted}")
    counts["host_documents"] = len(host["valid"]) + len(host["invalid"])

    next_dir = ROOT / "tests" / "fixtures" / "next"
    next_manifest = load(next_dir / "manifest.json")
    for entry in next_manifest["valid"]:
        if not valid(entry["schema"], load(next_dir / entry["file"])):
            failures.append(f"valid next document rejected: {entry['file']}")
    for case in next_manifest["invalid"]:
        value = load(next_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid("next/automation-v2.schema.json", value) != (case["schema"] == "accept"):
            failures.append(f"next case {case['id']}: disagreement")
    counts["next_documents"] = len(next_manifest["valid"]) + len(next_manifest["invalid"])

    jobs_dir = ROOT / "tests" / "fixtures" / "automation"
    jobs = load(jobs_dir / "manifest.json")
    base = load(jobs_dir / jobs["valid"])
    if not valid(jobs["schema"], base):
        failures.append("valid automation journal rejected")
    if not valid(jobs["schema"], load(jobs_dir / jobs["generated"])):
        failures.append("valid generated automation journal rejected")
    for case in jobs["invalid"]:
        value = copy.deepcopy(base)
        apply_ops(value, case["ops"])
        if valid(jobs["schema"], value) != (case["schema"] == "accept"):
            failures.append(f"automation case {case['id']}: disagreement")
    counts["automation_documents"] = 2 + len(jobs["invalid"])

    automation_store_dir = ROOT / "tests" / "fixtures" / "automation-vault"
    automation_store = load(automation_store_dir / "manifest.json")
    for entry in automation_store["valid"]:
        if not valid(automation_store["schema"], load(automation_store_dir / entry["file"])):
            failures.append(f"valid automation store document rejected: {entry['file']}")
    for case in automation_store["invalid"]:
        value = load(automation_store_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(automation_store["schema"], value) != (case["schema"] == "accept"):
            failures.append(f"automation store case {case['id']}: disagreement")
    counts["automation_store_documents"] = len(automation_store["valid"]) + len(automation_store["invalid"])

    continuation_dir = ROOT / "tests" / "fixtures" / "continuation"
    continuation = load(continuation_dir / "manifest.json")
    for entry in continuation["valid"]:
        if not valid(continuation["schema"], load(continuation_dir / entry["file"])):
            failures.append(f"valid continuation document rejected: {entry['file']}")
    for case in continuation["invalid"]:
        value = load(continuation_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(continuation["schema"], value) != (case["schema"] == "accept"):
            failures.append(f"continuation case {case['id']}: disagreement")
    counts["continuation_documents"] = len(continuation["valid"]) + len(continuation["invalid"])

    formation_dir = ROOT / "tests" / "fixtures" / "formation"
    formation = load(formation_dir / "manifest.json")
    base = load(formation_dir / formation["valid"])
    if not valid(formation["schema"], base):
        failures.append("valid formation dimensions rejected")
    if not valid("next/automation-v2.schema.json", load(formation_dir / formation["policy"])):
        failures.append("valid dimensional policy rejected")
    for case in formation["invalid"]:
        value = copy.deepcopy(base)
        apply_ops(value, case["ops"])
        if valid(formation["schema"], value) != (case["schema"] == "accept"):
            failures.append(f"formation case {case['id']}: disagreement")
    counts["formation_documents"] = 2 + len(formation["invalid"])

    attention_dir = ROOT / "tests" / "fixtures" / "attention"
    attention = load(attention_dir / "manifest.json")
    for entry in attention["valid"]:
        if not valid(attention["schema"], load(attention_dir / entry["file"])):
            failures.append(f"valid attention document rejected: {entry['file']}")
    for case in attention["invalid"]:
        value = load(attention_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(attention["schema"], value) != (case["schema"] == "accept"):
            failures.append(f"attention case {case['id']}: disagreement")
    counts["attention_documents"] = len(attention["valid"]) + len(attention["invalid"])

    read_model_dir = ROOT / "tests" / "fixtures" / "read-model"
    read_model = load(read_model_dir / "manifest.json")
    for entry in read_model["valid"]:
        if not valid(read_model["schema"], load(read_model_dir / entry["file"])):
            failures.append(f"valid read-model document rejected: {entry['file']}")
    for case in read_model["invalid"]:
        value = load(read_model_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(read_model["schema"], value) != (case["schema"] == "accept"):
            failures.append(f"read-model case {case['id']}: disagreement")
    counts["read_model_documents"] = len(read_model["valid"]) + len(read_model["invalid"])

    agent = load(ROOT / "tests" / "fixtures" / "agent-read" / "manifest.json")
    for value in agent["valid"]:
        if not valid(agent["schema"], value):
            failures.append("valid Agent read request rejected")
    for case in agent["invalid"]:
        if valid(agent["schema"], case["value"]):
            failures.append(f"invalid Agent read request accepted: {case['name']}")
    counts["agent_read_documents"] = len(agent["valid"]) + len(agent["invalid"])

    records = load(ROOT / "tests" / "fixtures" / "agent-record-read" / "manifest.json")
    for value in records["valid"]:
        if not valid(records["schema"], value):
            failures.append("valid Agent record request rejected")
    for case in records["invalid"]:
        if valid(records["schema"], case["value"]):
            failures.append(f"invalid Agent record request accepted: {case['name']}")
    counts["agent_record_documents"] = len(records["valid"]) + len(records["invalid"])

    catalog_agent = load(ROOT / "tests" / "fixtures" / "agent-catalog-read" / "manifest.json")
    for value in catalog_agent["valid"]:
        if not valid(catalog_agent["schema"], value):
            failures.append("valid segmented Agent request rejected")
    for case in catalog_agent["invalid"]:
        if valid(catalog_agent["schema"], case["value"]):
            failures.append(f"invalid segmented Agent request accepted: {case['name']}")
    counts["agent_catalog_documents"] = len(catalog_agent["valid"]) + len(catalog_agent["invalid"])

    qualified = load(ROOT / "tests" / "fixtures" / "agent-qualified-read" / "manifest.json")
    for value in qualified["valid"]:
        if not valid(qualified["schema"], value):
            failures.append("valid qualified Agent request rejected")
    for case in qualified["invalid"]:
        if valid(qualified["schema"], case["value"]):
            failures.append(f"invalid qualified Agent request accepted: {case['name']}")
    counts["agent_qualified_documents"] = len(qualified["valid"]) + len(qualified["invalid"])
    initialized = load(ROOT / "tests" / "fixtures" / "agent-initialized-read" / "manifest.json")
    for value in initialized["valid"]:
        if not valid(initialized["schema"], value):
            failures.append("valid initialized Agent request rejected")
    for case in initialized["invalid"]:
        if valid(initialized["schema"], case["value"]):
            failures.append(f"invalid initialized Agent request accepted: {case['name']}")
    counts["agent_initialized_documents"] = len(initialized["valid"]) + len(initialized["invalid"])

    if len(sys.argv) == 3 and sys.argv[1] == "--runtime-capture-transcript":
        transcript = load(pathlib.Path(sys.argv[2]))
        for exchange in transcript["exchanges"]:
            for key in ("call", "reply"):
                if not valid("next/runtime-capture-pipe-v2.schema.json", exchange[key]):
                    failures.append(f"Runtime capture live {key}: rejected")
        for value in transcript.get("negative_replies", []):
            if valid("next/runtime-capture-pipe-v2.schema.json", value):
                failures.append("Runtime capture live invalid reply accepted")
        counts["runtime_capture_live_exchanges"] = len(transcript["exchanges"])
        counts["runtime_capture_invalid_replies"] = len(transcript.get("negative_replies", []))
    elif len(sys.argv) != 1:
        if len(sys.argv) != 3 or sys.argv[1] not in ("--mcp2-transcript", "--qualified-transcript", "--initialized-transcript"):
            raise SystemExit("usage: check_schemas.py [--mcp2-transcript FILE | --qualified-transcript FILE | --initialized-transcript FILE | --runtime-capture-transcript FILE]")
        transcript = load(pathlib.Path(sys.argv[2]))
        tools = {tool["name"]: tool for tool in transcript["catalog"]}
        for tool in tools.values():
            for key in ("inputSchema", "outputSchema"):
                Draft202012Validator.check_schema(tool[key])
        for exchange in transcript["exchanges"]:
            tool = tools[exchange["tool"]]
            for key, value in (("inputSchema", exchange["arguments"]),
                               ("outputSchema", exchange["response"]["result"]["structuredContent"])):
                if not Draft202012Validator(tool[key]).is_valid(value):
                    failures.append(f"MCP-2 live {exchange['tool']} {key}: rejected")
        for case in transcript.get("negative_outputs", []):
            tool = tools[case["tool"]]
            if Draft202012Validator(tool["outputSchema"]).is_valid(case["value"]):
                failures.append(f"MCP live invalid output accepted: {case['tool']}")
        counts["initialized_live_exchanges" if sys.argv[1] == "--initialized-transcript" else "qualified_live_exchanges" if sys.argv[1] == "--qualified-transcript" else "mcp2_live_exchanges"] = len(transcript["exchanges"])
        counts["mcp_live_invalid_outputs"] = len(transcript.get("negative_outputs", []))
    ui_dir = ROOT / "tests" / "fixtures" / "ui-records"
    ui = load(ui_dir / "manifest.json")
    ui_schemas = {e["file"]: e["schema"] for e in ui["valid"]}
    for entry in ui["valid"]:
        if not valid(entry["schema"], load(ui_dir / entry["file"])):
            failures.append(f"valid UI record rejected: {entry['file']}")
    for case in ui["invalid"]:
        value = load(ui_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(ui_schemas[case["base"]], value) != (case["schema"] == "accept"):
            failures.append(f"UI record case {case['id']}: disagreement")
    counts["ui_record_documents"] = len(ui["valid"]) + len(ui["invalid"])

    catalog_dir = ROOT / "tests" / "fixtures" / "catalog"
    catalog = load(catalog_dir / "manifest.json")
    catalog_schemas = {e["file"]: e["schema"] for e in catalog["valid"]}
    for entry in catalog["valid"]:
        if not valid(entry["schema"], load(catalog_dir / entry["file"])):
            failures.append(f"valid catalog document rejected: {entry['file']}")
    for case in catalog["invalid"]:
        value = load(catalog_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(catalog_schemas[case["base"]], value) != (case["schema"] == "accept"):
            failures.append(f"catalog case {case['id']}: disagreement")
    counts["catalog_documents"] = len(catalog["valid"]) + len(catalog["invalid"])

    capture_cases = load(ROOT / "tests/fixtures/runtime-capture/cases.json")
    for case in capture_cases["cases"]:
        value = copy.deepcopy(capture_cases["base"])
        apply_ops(value, case["ops"])
        if valid("next/runtime-capture-v2.schema.json", value) != case["schema"]:
            failures.append(f"Runtime capture case {case['name']}: disagreement")
    counts["runtime_capture_cases"] = len(capture_cases["cases"])
    owner_capture_cases = load(ROOT / "tests/fixtures/runtime-owner-capture/cases.json")
    for case in owner_capture_cases["cases"]:
        value = copy.deepcopy(owner_capture_cases["base"])
        apply_ops(value, case["ops"])
        if valid("next/runtime-owner-capture-v2.schema.json", value) != case["schema"]:
            failures.append(f"Owner Runtime capture case {case['name']}: disagreement")
    counts["owner_runtime_capture_cases"] = len(owner_capture_cases["cases"])

    wire_cases = load(ROOT / "tests/fixtures/runtime-capture/wire-cases.json")
    for case in wire_cases["cases"]:
        value = {"mode": "write", "request": copy.deepcopy(capture_cases["base"])}
        apply_ops(value, case["ops"])
        if valid("next/runtime-capture-pipe-v2.schema.json", value) != case["accept"]:
            failures.append(f"Runtime capture wire case {case['name']}: disagreement")
    counts["runtime_capture_wire_cases"] = len(wire_cases["cases"])

    projection_dir = ROOT / "tests" / "fixtures" / "projections"
    projection = load(projection_dir / "manifest.json")
    projection_schemas = {e["file"]: e["schema"] for e in projection["valid"]}
    for entry in projection["valid"]:
        if not valid(entry["schema"], load(projection_dir / entry["file"])):
            failures.append(f"valid projection document rejected: {entry['file']}")
    for case in projection["invalid"]:
        value = load(projection_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(projection_schemas[case["base"]], value) != (case["schema"] == "accept"):
            failures.append(f"projection case {case['id']}: disagreement")
    counts["projection_documents"] = len(projection["valid"]) + len(projection["invalid"])

    process_dir = ROOT / "tests" / "fixtures" / "process"
    process = load(process_dir / "manifest.json")
    process_schemas = {e["file"]: e["schema"] for e in process["valid"]}
    for entry in process["valid"]:
        if not valid(entry["schema"], load(process_dir / entry["file"])):
            failures.append(f"valid process document rejected: {entry['file']}")
    for case in process["invalid"]:
        value = load(process_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(process_schemas[case["base"]], value) != (case["schema"] == "accept"):
            failures.append(f"process case {case['id']}: disagreement")
    counts["process_documents"] = len(process["valid"]) + len(process["invalid"])

    owner_dir = ROOT / "tests" / "fixtures" / "owner"
    owner = load(owner_dir / "manifest.json")
    owner_schemas = {e["file"]: e["schema"] for e in owner["valid"]}
    for entry in owner["valid"]:
        if not valid(entry["schema"], load(owner_dir / entry["file"])):
            failures.append(f"valid owner document rejected: {entry['file']}")
    for case in owner["invalid"]:
        value = load(owner_dir / case["base"])
        apply_ops(value, case["ops"])
        if valid(owner_schemas[case["base"]], value) != (case["schema"] == "accept"):
            failures.append(f"owner case {case['id']}: disagreement")
    counts["owner_documents"] = len(owner["valid"]) + len(owner["invalid"])

    invocation_cases = load(FIXTURES / "provider-invocation-cases.json")
    for case in invocation_cases["cases"]:
        value = copy.deepcopy(invocation_cases["base"])
        value.update(case["set"])
        if valid("provider/invocation-v1.schema.json", value) != case["accept"]:
            failures.append(f"provider invocation case {case['name']}: disagreement")
    counts["provider_invocation_cases"] = len(invocation_cases["cases"])

    extraction_cases = load(FIXTURES / "extraction-job-cases.json")
    for case in extraction_cases["cases"]:
        value = copy.deepcopy(extraction_cases["base"])
        apply_ops(value, case["ops"])
        if valid("provider/extraction-job-v1.schema.json", value) != case["schema"]:
            failures.append(f"extraction job case {case['name']}: disagreement")
    counts["extraction_job_cases"] = len(extraction_cases["cases"])

    print(f"python-jsonschema {version}: " + ", ".join(f"{k}={v}" for k, v in counts.items()))
    for failure in failures:
        print("FAIL", failure)
    print("OK" if not failures else f"{len(failures)} disagreement(s)")
    sys.exit(1 if failures else 0)


if __name__ == "__main__":
    main()

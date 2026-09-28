import json, pathlib
ROOT = pathlib.Path(__file__).resolve().parents[2] / "contracts" / "ipc"
D = "https://json-schema.org/draft/2020-12/schema"
MAXSAFE = 9007199254740991
MC = "../memory/common-v1.schema.json"
def m(n): return {"$ref": f"{MC}#/$defs/{n}"}
def r(n): return {"$ref": f"#/$defs/{n}"}
def nul(s): return {"oneOf": [{"type": "null"}, s]}
def obj(props, required=None):
    return {"type": "object", "additionalProperties": False,
            "required": list(props) if required is None else required, "properties": props}
def arr(i, **kw):
    a = {"type": "array", "items": i}; a.update(kw); return a
def enum(*v): return {"enum": list(v)}
STR = {"type": "string", "minLength": 1}
TEXT = {"type": "string"}
BOOL = {"type": "boolean"}
COUNT = {"type": "integer", "minimum": 0, "maximum": MAXSAFE}
QUERY = {"type": "string", "minLength": 1, "maxLength": 2000}
LIMIT = nul({"type": "integer", "minimum": 1, "maximum": 100})
IDEM = {"type": "string", "pattern": "^[A-Za-z0-9_-]{16,128}$"}
NONCE = {"type": "string", "pattern": "^[A-Za-z0-9_-]{22,128}$"}

time_mode = obj({"asOf": m("timestampOrNull"), "knownAtCommitId": nul(m("commitId"))})
byte_range = obj({"start": COUNT, "end": COUNT})
proposed_item = obj({"claim": STR, "stateKind": m("stateKind"), "eventIds": arr(m("eventId"), minItems=1)})
args = {
    "context_get": obj({"query": QUERY, "sessionId": nul(m("sessionId")), "branchId": nul(m("branchId")),
                        "projectHints": arr(m("projectId")), "timeMode": time_mode,
                        "destination": obj({"kind": m("destinationKind"), "providerBindingName": nul(STR)}),
                        "purpose": m("purpose"), "maxTokens": {"type": "integer", "minimum": 1, "maximum": MAXSAFE}}),
    "memory_search": obj({"query": QUERY, "types": arr(m("memoryType"), uniqueItems=True),
                          "projectIds": arr(m("projectId")), "includeHistorical": BOOL, "timeMode": time_mode,
                          "cursor": nul(STR), "limit": LIMIT}),
    "memory_read": obj({"memoryId": m("memoryId"), "revision": nul(m("revision"))}),
    "memory_source": obj({"sourceId": m("sourceId"), "sourceRevision": nul(m("revision")),
                          "range": nul(byte_range), "maxBytes": {"type": "integer", "minimum": 1, "maximum": 8192}}),
    "memory_propose": obj({"proposalKind": enum("create", "revise", "supersede", "archive", "identity_change"),
                           "proposedType": enum("fact", "preference", "episode", "project_state", "session_checkpoint", "identity"),
                           "proposedContent": STR, "proposedDetails": nul({"type": "object"}),
                           "evidence": arr(obj({"sourceId": m("sourceId"), "sourceRevision": m("revision"),
                                                "range": nul(byte_range),
                                                "supports": {"oneOf": [{"const": "content"}, m("itemId")]}}), minItems=1),
                           "targetMemoryId": nul(m("memoryId")), "expectedRevision": nul(m("revision")),
                           "proposedEffectiveFrom": nul(m("businessTime")), "reason": STR}),
    "session_checkpoint": obj({"sessionId": m("sessionId"), "branchId": m("branchId"),
                               "fromSequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
                               "toSequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
                               "summary": STR, "decisions": arr(proposed_item), "openLoops": arr(proposed_item)}),
    "candidate_list": obj({"status": enum("pending", "accepted", "rejected", "merged", "withdrawn"),
                           "projectId": nul(m("projectId")), "cursor": nul(STR), "limit": LIMIT}),
    "candidate_review": obj({"candidateId": m("candidateId"), "candidateRevision": m("revision"),
                             "action": enum("accept", "edit_accept", "reject", "merge", "supersede", "identity_accept"),
                             "finalContentHash": m("sha256"), "approvedDiffHash": m("sha256"),
                             "targetExpectedRevisions": arr(obj({"memoryId": m("memoryId"), "revision": m("revision")})),
                             "approvalNonce": NONCE, "editedContent": nul(STR),
                             "effectiveFrom": nul(m("businessTime")),
                             "mergeTarget": nul(obj({"kind": enum("candidate", "memory"),
                                                     "id": {"type": "string", "pattern": "^(cand|mem)_[0-9a-f-]{36}$"}}))}),
    "identity_review": obj({"identityId": m("identityId"), "expectedRevision": nul(m("revision")),
                            "contentHash": m("sha256"), "approvedDiffHash": m("sha256"), "approvalNonce": NONCE}),
    "operation_get": obj({"operationId": m("operationId")}),
    "operation_cancel": obj({"operationId": m("operationId")}),
}
args["memory_update"] = json.loads(json.dumps(args["memory_propose"]))
args["memory_update"]["properties"]["proposalKind"] = enum("revise", "supersede")
args["memory_update"]["properties"]["targetMemoryId"] = m("memoryId")
args["memory_update"]["properties"]["expectedRevision"] = m("revision")

WRITES = {"memory_propose", "memory_update", "session_checkpoint", "candidate_review", "identity_review"}
defs = {}
requests = []
for op, a in args.items():
    name = "request_" + op
    defs[name] = obj({"schemaVersion": {"const": 1}, "requestId": m("requestId"), "operation": {"const": op},
                      "idempotencyKey": IDEM if op in WRITES else {"type": "null"}, "arguments": a})
    requests.append(r(name))

search_hit = obj({"memoryId": m("memoryId"), "revision": m("revision"), "type": m("memoryType"),
                  "snippet": TEXT, "currency": {"$ref": "../context/common-v1.schema.json#/$defs/currency"},
                  "evidenceCount": {"type": "integer", "minimum": 0, "maximum": 4294967295},
                  "updatedAt": m("timestamp")})
results = {
    "context_capsule": obj({"capsule": {"$ref": "../context/capsule-v1.schema.json"}, "inspectionId": m("inspectionId")}),
    "memory_search_result": obj({"items": arr(search_hit, maxItems=100), "nextCursor": nul(STR), "stale": BOOL, "partial": BOOL}),
    "memory_record": obj({"record": {"$ref": "../memory/memory-v1.schema.json"}, "supersededBy": arr(m("memoryId"))}),
    "source_excerpt": obj({"sourceId": m("sourceId"), "sourceRevision": m("revision"),
                           "excerpt": {"type": "string", "maxLength": 8192}, "byteStart": COUNT, "byteEnd": COUNT,
                           "truncated": BOOL,
                           "attachments": arr(obj({"attachmentId": m("attachmentId"),
                                                   "availability": enum("present", "missing", "external_reference", "quarantined", "unsupported")}))}),
    "candidate_proposed": obj({"candidateId": m("candidateId"), "revision": m("revision"), "state": enum("pending", "duplicate")}),
    "checkpoint_proposed": obj({"checkpointId": m("checkpointId"), "revision": m("revision"), "status": {"const": "provisional"}}),
    "candidate_page": obj({"items": arr(obj({"candidateId": m("candidateId"), "revision": m("revision"),
                                             "proposalKind": enum("create", "revise", "supersede", "archive", "identity_change"),
                                             "proposedType": enum("fact", "preference", "episode", "project_state", "session_checkpoint", "identity"),
                                             "status": enum("pending", "accepted", "rejected", "merged", "withdrawn"),
                                             "sensitivity": m("sensitivity"), "createdAt": m("timestamp")}), maxItems=100),
                           "nextCursor": nul(STR)}),
    "review_committed": obj({"reviewId": m("reviewId"), "commitId": m("commitId"), "records": arr(m("recordRef"), minItems=1)}),
    "operation_status": obj({"operationId": m("operationId"),
                             "state": enum("queued", "running", "succeeded", "failed", "cancelled"),
                             "progress": nul(obj({"done": COUNT, "total": COUNT})),
                             "errorCode": nul(m("memoryErrorCode"))}),
}
component = m("componentId")
header = {"schemaVersion": {"const": 1}, "requestId": m("requestId"), "vaultCommitId": nul(m("commitId")),
          "policyEpoch": nul(COUNT), "operationId": nul(m("operationId"))}
responses = []
for kind, result in results.items():
    props = dict(header); props["kind"] = {"const": kind}; props["result"] = result; props["error"] = {"type": "null"}
    if kind != "operation_status":
        props["vaultCommitId"] = m("commitId")
    if kind in ("candidate_proposed", "checkpoint_proposed", "review_committed"):
        props["operationId"] = m("operationId")
    defs["response_" + kind] = obj(props)
    responses.append(r("response_" + kind))
err = dict(header); err["kind"] = {"const": "memory_error"}; err["result"] = {"type": "null"}
err["error"] = obj({"code": m("memoryErrorCode"), "component": component, "retryable": BOOL})
defs["response_memory_error"] = obj(err)
responses.append(r("response_memory_error"))
schema = {"$schema": D, "$id": "https://contracts.enouia-memory.invalid/ipc/memory-v1.schema.json", "title": "Enouia local Memory IPC v1",
          "description": "Typed local requests and responses for Memory/Context/Session operations. camelCase DTO fields; embedded stored records keep snake_case. Logical IDs only: no paths, SQL, shell text, or caller-asserted identity. The request/response pairing per operation and write idempotency rules are also enforced in Rust. import/backup/restore/delete/attachment operations are reserved for MV-1..MV-3.",
          "oneOf": requests + responses, "$defs": defs}
(ROOT / "memory-v1.schema.json").write_bytes((json.dumps(schema, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))
print("ok")

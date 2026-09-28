import json, hashlib, pathlib
OUT = pathlib.Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "memory"
def uid(p, n): return f"{p}_{n:08x}-0000-4000-8000-{n:012x}"
def sha(t): return hashlib.sha256(t.encode()).hexdigest()
def rec(n): return json.loads((OUT / "records" / n).read_text(encoding="utf-8"))
HEAD = uid("cmt", 9)
def req(op, args, n, idem=None):
    return {"schemaVersion": 1, "requestId": uid("req", 500 + n), "operation": op, "idempotencyKey": idem, "arguments": args}
TM = {"asOf": None, "knownAtCommitId": None}
EV = [{"sourceId": uid("src", 2), "sourceRevision": 1, "range": None, "supports": "content"}]
valid_requests = [
    req("context_get", {"query": "我们之前 MoriMeta 的设计最后选了什么？", "sessionId": None, "branchId": None,
        "projectHints": [uid("prj", 1)], "timeMode": TM, "destination": {"kind": "local_mock", "providerBindingName": None},
        "purpose": "answer", "maxTokens": 8192}, 1),
    req("memory_search", {"query": "函馆", "types": ["fact"], "projectIds": [], "includeHistorical": False,
        "timeMode": TM, "cursor": None, "limit": None}, 2),
    req("memory_read", {"memoryId": uid("mem", 2), "revision": None}, 3),
    req("memory_source", {"sourceId": uid("src", 2), "sourceRevision": 1, "range": {"start": 0, "end": 512}, "maxBytes": 8192}, 4),
    req("memory_propose", {"proposalKind": "create", "proposedType": "fact", "proposedContent": "（合成）喜欢深色界面。",
        "proposedDetails": None, "evidence": EV, "targetMemoryId": None, "expectedRevision": None,
        "proposedEffectiveFrom": None, "reason": "agent suggestion"}, 5, "idem-propose-0000000001"),
    req("memory_update", {"proposalKind": "revise", "proposedType": "project_state", "proposedContent": "（合成）修订。",
        "proposedDetails": None, "evidence": EV, "targetMemoryId": uid("mem", 2), "expectedRevision": 1,
        "proposedEffectiveFrom": None, "reason": "legacy alias"}, 6, "idem-update-00000000001"),
    req("session_checkpoint", {"sessionId": uid("ses", 1), "branchId": uid("br", 1), "fromSequence": 1, "toSequence": 4,
        "summary": "（合成）摘要", "decisions": [],
        "openLoops": [{"claim": "实现 Darkroom", "stateKind": "planned", "eventIds": [uid("evt", 3)]}]}, 7,
        "idem-checkpoint-000000001"),
    req("candidate_list", {"status": "pending", "projectId": None, "cursor": None, "limit": 20}, 8),
    req("candidate_review", {"candidateId": uid("cand", 1), "candidateRevision": 1, "action": "reject",
        "finalContentHash": sha("x"), "approvedDiffHash": sha("d"), "targetExpectedRevisions": [],
        "approvalNonce": "nonce-ipc-review-0001-abcdef", "editedContent": None, "effectiveFrom": None,
        "mergeTarget": None}, 9, "idem-review-00000000001"),
    req("identity_review", {"identityId": uid("idn", 1), "expectedRevision": 1, "contentHash": sha("id"),
        "approvedDiffHash": sha("idd"), "approvalNonce": "nonce-ipc-identity-0001-abc"}, 10, "idem-identity-000000001"),
    req("operation_get", {"operationId": uid("op", 9)}, 11),
    req("operation_cancel", {"operationId": uid("op", 9)}, 12),
]
def resp(op, kind, result, n, op_id=None, error=None, commit=HEAD):
    return {"operation": op, "message": {"schemaVersion": 1, "requestId": uid("req", 500 + n), "kind": kind,
            "vaultCommitId": commit, "policyEpoch": 1, "operationId": op_id, "result": result, "error": error}}
valid_responses = [
    resp("context_get", "context_capsule", {"capsule": rec("capsule.json"), "inspectionId": uid("insp", 1)}, 1),
    resp("memory_search", "memory_search_result", {"items": [{"memoryId": uid("mem", 105), "revision": 2, "type": "fact",
        "snippet": "住在函馆。", "currency": "conflicted", "evidenceCount": 1, "updatedAt": "2026-09-21T10:02:00.500Z"}],
        "nextCursor": None, "stale": False, "partial": False}, 2),
    resp("memory_read", "memory_record", {"record": rec("memory-project-state.json"), "supersededBy": []}, 3),
    resp("memory_source", "source_excerpt", {"sourceId": uid("src", 2), "sourceRevision": 1,
        "excerpt": "（合成）就选 Professional Darkroom。", "byteStart": 0, "byteEnd": 48, "truncated": False,
        "attachments": [{"attachmentId": uid("att", 1), "availability": "present"}]}, 4),
    resp("memory_propose", "candidate_proposed", {"candidateId": uid("cand", 50), "revision": 1, "state": "pending"}, 5, uid("op", 50)),
    resp("memory_update", "candidate_proposed", {"candidateId": uid("cand", 51), "revision": 1, "state": "pending"}, 6, uid("op", 51)),
    resp("session_checkpoint", "checkpoint_proposed", {"checkpointId": uid("ckp", 50), "revision": 1, "status": "provisional"}, 7, uid("op", 52)),
    resp("candidate_review", "review_committed", {"reviewId": uid("rvw", 50), "commitId": HEAD,
        "records": [{"record_kind": "candidate", "record_id": uid("cand", 1), "revision": 2}]}, 9, uid("op", 53)),
    resp("operation_get", "operation_status", {"operationId": uid("op", 9), "state": "running",
        "progress": {"done": 10, "total": 40}, "errorCode": None}, 11, None, None, None),
    resp("memory_source", "memory_error", None, 4, None,
         {"code": "permission_denied", "component": "vault", "retryable": False}, None),
]
def S(p, v): return {"op": "set", "path": p, "value": v}
def bad(cid, base, ops, schema, rule): return {"id": cid, "base": base, "ops": ops, "schema": schema, "rust_rule": rule}
invalid_requests = [
    bad("request-major-2", 1, [S("/schemaVersion", 2)], "reject", "unsupported_schema"),
    bad("request-unknown-operation", 1, [S("/operation", "memory_delete_all")], "reject", "shape"),
    bad("request-reserved-operation", 1, [S("/operation", "delete_commit")], "reject", "shape"),
    bad("request-path-argument", 2, [S("/arguments/path", "C:/vault/records")], "reject", "shape"),
    bad("request-sql-argument", 1, [S("/arguments/sql", "SELECT * FROM memory")], "reject", "shape"),
    bad("request-caller-claims-owner", 8, [S("/arguments/actor", {"actor_type": "owner"})], "reject", "shape"),
    bad("search-limit-over-max", 1, [S("/arguments/limit", 101)], "reject", "ipc.limit"),
    bad("search-empty-query", 1, [S("/arguments/query", "")], "reject", "ipc.query_length"),
    bad("source-over-8k", 3, [S("/arguments/maxBytes", 65536)], "reject", "ipc.source_max_bytes"),
    bad("source-inverted-range", 3, [S("/arguments/range", {"start": 10, "end": 5})], "accept", "ipc.source_range"),
    bad("write-without-idempotency", 4, [S("/idempotencyKey", None)], "reject", "ipc.idempotency_key"),
    bad("read-with-idempotency", 2, [S("/idempotencyKey", "idem-read-00000000001")], "reject", "ipc.idempotency_key"),
    bad("update-alias-create", 5, [S("/arguments/proposalKind", "create")], "reject", "ipc.update_alias"),
    bad("propose-delete", 4, [S("/arguments/proposalKind", "delete"), S("/arguments/targetMemoryId", uid("mem", 2)),
                              S("/arguments/expectedRevision", 1)], "reject", "ipc.delete_not_proposable"),
    bad("propose-without-evidence", 4, [S("/arguments/evidence", [])], "reject", "ipc.evidence_required"),
    bad("revise-without-expected-revision", 5, [S("/arguments/expectedRevision", None)], "reject", "ipc.target_revision"),
    bad("review-short-nonce", 8, [S("/arguments/approvalNonce", "x")], "reject", "review.nonce_format"),
    bad("review-merge-wrong-target", 8, [S("/arguments/action", "merge"),
        S("/arguments/mergeTarget", {"kind": "memory", "id": uid("cand", 3)})], "accept", "ref.kind_prefix"),
    bad("checkpoint-inverted-range", 6, [S("/arguments/fromSequence", 5)], "accept", "ipc.checkpoint_range"),
    bad("context-client-asserts-unknown-destination", 0, [S("/arguments/destination/kind", "trusted_local")], "reject", "shape"),
]
invalid_responses = [
    bad("update-returns-committed", 5, [S("/message/kind", "review_committed")], "reject", "ipc.kind_for_operation"),
    bad("result-and-error", 4, [S("/message/error", {"code": "busy", "component": "vault", "retryable": True})], "reject", "ipc.result_or_error"),
    bad("error-with-stack", 9, [S("/message/error/stack", "at vault.rs:10")], "reject", "shape"),
    bad("write-without-operation-id", 4, [S("/message/operationId", None)], "reject", "ipc.write_operation_id"),
    bad("read-without-snapshot", 2, [S("/message/vaultCommitId", None)], "reject", "ipc.snapshot_header"),
    bad("search-over-100", 1, [{"op": "repeat", "path": "/message/result/items", "count": 101}], "reject", "ipc.limit"),
    bad("excerpt-over-8k", 3, [S("/message/result/excerpt", "x" * 9000)], "reject", "ipc.source_max_bytes"),
    bad("review-commit-header-mismatch", 7, [S("/message/vaultCommitId", uid("cmt", 8))], "accept", "ipc.commit_header"),
    bad("checkpoint-proposed-as-reviewed", 6, [S("/message/result/status", "reviewed")], "reject", "shape"),
]
doc = {"description": "Synthetic local Memory IPC v1 messages. 'base' indexes valid_requests or valid_responses; responses carry the producing operation.",
       "valid_requests": valid_requests, "valid_responses": valid_responses,
       "invalid_requests": invalid_requests, "invalid_responses": invalid_responses}
(OUT / "ipc-manifest.json").write_bytes((json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n").encode("utf-8"))
print(len(valid_requests), len(valid_responses), len(invalid_requests), len(invalid_responses))

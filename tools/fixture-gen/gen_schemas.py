"""One-off generator for MV-0 JSON Schemas. Output files are the committed,
hand-maintainable source of truth; this script is not part of the repository."""
import json, pathlib, copy

ROOT = pathlib.Path(__file__).resolve().parents[2] / "contracts"
D = "https://json-schema.org/draft/2020-12/schema"
C = "common-v1.schema.json"  # within contracts/memory
MAXSAFE = 9007199254740991
BASE = "https://contracts.enouia-memory.invalid/"  # never resolvable (.invalid); identity only

PREFIXES = [
    ("memoryId", "mem"), ("sourceId", "src"), ("attachmentId", "att"), ("importId", "imp"),
    ("projectId", "prj"), ("subjectId", "sub"), ("candidateId", "cand"), ("reviewId", "rvw"),
    ("identityId", "idn"), ("sessionId", "ses"), ("branchId", "br"), ("eventId", "evt"),
    ("turnId", "turn"), ("checkpointId", "ckp"), ("commitId", "cmt"), ("vaultId", "vlt"),
    ("deviceId", "dev"), ("principalId", "prn"), ("operationId", "op"), ("requestId", "req"),
    ("policyId", "pol"), ("deleteId", "del"), ("purgeReceiptId", "prg"), ("auditId", "aud"),
    ("capsuleId", "cap"), ("inspectionId", "insp"), ("dispatchId", "dsp"),
    ("extractionRunId", "ext"), ("conflictGroupId", "cfl"), ("itemId", "itm"),
]
UUID = "[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}"

def ref(name, file=None):
    if file is None:
        return {"$ref": f"#/$defs/{name}"}
    return {"$ref": f"{file}#/$defs/{name}"}

def nul(schema):
    return {"oneOf": [{"type": "null"}, schema]}

def obj(props, required=None, extra=None):
    o = {"type": "object", "additionalProperties": False,
         "required": list(props.keys()) if required is None else required,
         "properties": props}
    if extra:
        o.update(extra)
    return o

def enum(*values):
    return {"enum": list(values)}

def arr(items, **kw):
    a = {"type": "array", "items": items}
    a.update(kw)
    return a

STR = {"type": "string", "minLength": 1}
TEXT = {"type": "string"}
BOOL = {"type": "boolean"}
COUNT = {"type": "integer", "minimum": 0, "maximum": MAXSAFE}
U32 = {"type": "integer", "minimum": 0, "maximum": 4294967295}

def write(rel, schema):
    if isinstance(schema.get("$id"), str) and not schema["$id"].startswith("http"):
        schema["$id"] = BASE + rel
    path = ROOT / rel
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes((json.dumps(schema, ensure_ascii=False, indent=2) + "\n").encode("utf-8"))

# ---------------- memory/common-v1 ----------------
defs = {}
for name, prefix in PREFIXES:
    defs[name] = {"type": "string", "pattern": f"^{prefix}_{UUID}$"}
defs.update({
    "schemaVersion": {"const": 1},
    "timestamp": {"type": "string",
                  "pattern": "^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}\\.[0-9]{3}Z$",
                  "$comment": "UTC with milliseconds. Real calendar dates are checked by the Rust validator."},
    "timestampOrNull": nul(ref("timestamp")),
    "businessTime": {"oneOf": [ref("timestamp"), {"const": "unknown"}]},
    "sha256": {"type": "string", "pattern": "^[0-9a-f]{64}$"},
    "revision": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
    "count": COUNT,
    "label": {"type": "string", "pattern": "^[a-z0-9][a-z0-9_.:-]{0,127}$"},
    "code": {"type": "string", "pattern": "^[a-z][a-z0-9_]{0,63}$"},
    "sensitivity": enum("public", "normal", "private", "highly_sensitive"),
    "priority": enum("P0", "P1", "P2"),
    "volatility": enum("stable", "changing", "live"),
    "evidenceClass": enum("user_statement", "user_confirmation", "external_observation",
                          "model_claim", "summary", "unknown"),
    "speakerRole": enum("user", "assistant", "tool", "system", "unknown"),
    "timePrecision": enum("millisecond", "second", "minute", "hour", "day", "month", "year", "unknown"),
    "actorType": enum("owner", "device", "client", "agent", "provider", "system"),
    "actorRef": obj({"actor_id": ref("principalId"), "actor_type": ref("actorType")}),
    "ownerRef": obj({"actor_id": ref("principalId"), "actor_type": {"const": "owner"}}),
    "trustedSurface": enum("trusted_windows_app", "trusted_local_cli"),
    "stateKind": enum("planned", "decided", "implemented", "tested", "released", "unknown"),
    "locator": {"oneOf": [
        obj({"kind": {"const": "json_pointer"}, "pointer": {"type": "string", "pattern": "^(/.*)?$"}}),
        obj({"kind": {"const": "byte_range"}, "start": COUNT, "end": COUNT},
            extra={"$comment": "start < end is checked by the Rust validator."}),
        obj({"kind": {"const": "runtime_event"}, "event_id": ref("eventId")}),
        obj({"kind": {"const": "manual_input"}}),
        obj({"kind": {"const": "archive_member"},
             "member_name": {"type": "string", "minLength": 1, "maxLength": 512,
                             "pattern": "^[^/\\\\:][^\\\\:]*$"},
             "member_hash": ref("sha256"), "inner": ref("locator")},
            extra={"$comment": "'.' and '..' segments are rejected by the Rust validator."}),
    ]},
    "evidenceRef": obj({"source_id": ref("sourceId"), "source_revision": ref("revision"),
                        "locator": ref("locator"), "object_hash": ref("sha256"),
                        "evidence_class": ref("evidenceClass"),
                        "supports": {"oneOf": [{"const": "content"}, ref("itemId")]}}),
    "sourceRevisionRef": obj({"source_id": ref("sourceId"), "source_revision": ref("revision")}),
    "contentRef": obj({"object_hash": ref("sha256"), "size_bytes": COUNT, "media_type": STR}),
    "warning": obj({"code": ref("code"), "pointer": nul(TEXT)}),
    "extensions": {"type": "object",
                   "propertyNames": {"pattern": "^[a-z][a-z0-9-]*(\\.[a-z][a-z0-9-]*)+$"},
                   "$comment": "Namespaced, preserved on round trip, never used for policy. 'enouia.*' is reserved and currently rejected by the Rust validator."},
    "recordKind": enum("source", "attachment", "project", "memory", "candidate", "review",
                       "identity", "session", "session_event", "checkpoint", "commit",
                       "tombstone", "purge_receipt", "audit_event", "capsule", "inspection",
                       "dispatch", "provider_capabilities"),
    "recordRef": obj({"record_kind": ref("recordKind"),
                      "record_id": {"type": "string", "pattern": f"^[a-z]+_{UUID}$"},
                      "revision": ref("revision")},
                     extra={"$comment": "The ID prefix must match record_kind (Rust validator)."}),
    "providerBinding": obj({"provider": STR, "model": STR, "adapter_version": STR}),
    "componentId": enum("core", "vault", "memory_index", "context", "session", "provider",
                         "importer", "backup", "audit", "policy"),
    "memoryType": enum("fact", "preference", "episode", "project_state", "session_checkpoint"),
    "destinationKind": enum("local_mock", "local_model", "external_provider"),
    "purpose": enum("answer", "continue_session", "checkpoint", "extraction", "inspection_preview"),
    "clientSurface": enum("windows_app", "local_cli", "mcp_client", "import", "test"),
    "memoryErrorCode": enum("unauthenticated", "permission_denied", "not_found", "vault_locked",
                            "vault_recovering", "busy", "revision_conflict", "idempotency_conflict",
                            "invalid_request", "invalid_source", "broken_provenance",
                            "unsupported_schema", "index_not_ready", "budget_exceeded",
                            "unsupported_budget", "storage_full", "storage_failed",
                            "audit_unavailable", "provider_unavailable", "offline", "cancelled",
                            "clock_regression", "intentionally_purged"),
    "operation": enum("context_get", "memory_search", "memory_read", "memory_source",
                      "memory_propose", "memory_update", "session_checkpoint", "candidate_list",
                      "candidate_review", "identity_review", "operation_get", "operation_cancel"),
})
write("memory/common-v1.schema.json", {
    "$schema": D, "$id": BASE + "memory/common-v1.schema.json",
    "title": "Enouia Memory shared definitions v1",
    "description": "Shared definitions for Memory-domain records. Storage fields are snake_case. Constraints marked in $comment are enforced by enouia-memory-contract, not by this schema.",
    "$defs": defs})

def c(name):
    return ref(name, C)

def cn(name):
    return nul(c(name))

def record(title, desc, props, extra=None, file_id=None):
    s = {"$schema": D, "$id": file_id, "title": title, "description": desc}
    s.update(obj(props))
    if extra:
        s.update(extra)
    return s

# ---------------- source ----------------
source_props = {
    "schema_version": c("schemaVersion"), "source_id": c("sourceId"), "revision": c("revision"),
    "source_kind": enum("export_message", "runtime_event", "imported_document", "manual_assertion",
                        "external_event", "agent_submission"),
    "provider": nul(STR),
    "account_scope": nul({"type": "string", "pattern": "^[a-z0-9][a-z0-9_-]{0,63}$"}),
    "import_id": cn("importId"), "raw_object_hash": cn("sha256"), "content_hash": c("sha256"),
    "original_conversation_id": nul(TEXT), "original_message_id": nul(TEXT),
    "parent_source_ids": {"oneOf": [{"const": "unknown"}, arr(c("sourceId"))]},
    "branch_id": {"oneOf": [{"const": "unknown"}, {"type": "null"}, c("branchId")]},
    "locator": c("locator"),
    "original_time": nul(TEXT), "original_timezone": nul(TEXT),
    "occurred_at": c("timestampOrNull"), "captured_at": c("timestamp"),
    "time_precision": c("timePrecision"), "speaker_role": c("speakerRole"),
    "author_label": nul(TEXT), "evidence_class": c("evidenceClass"),
    "completeness": enum("complete", "partial", "metadata_only", "unavailable"),
    "sensitivity": c("sensitivity"), "access_policy_id": c("policyId"),
    "attachment_refs": arr(c("attachmentId")), "parser_version": nul(STR),
    "parse_warnings": arr(c("warning")),
    "manual_assertion": nul(obj({
        "input_text": STR, "operator": c("ownerRef"), "trusted_surface": c("trustedSurface"),
        "confirmation_method": enum("exact_text_confirm_dialog", "typed_confirmation"),
        "confirmed_at": c("timestamp")})),
    "agent_submission": nul(obj({
        "submitting_principal": c("actorRef"), "submitted_text": TEXT,
        "claimed_user_consent": BOOL})),
    "created_at": c("timestamp"), "extensions": c("extensions"),
}
def when(prop, values, then):
    return {"if": {"properties": {prop: {"enum": values}}, "required": [prop]}, "then": then}
source_rules = {"allOf": [
    when("source_kind", ["manual_assertion"], {"properties": {
        "manual_assertion": {"type": "object"}, "agent_submission": {"type": "null"},
        "speaker_role": {"const": "user"}, "evidence_class": {"enum": ["user_statement", "user_confirmation"]},
        "locator": {"properties": {"kind": {"const": "manual_input"}}}}}),
    when("source_kind", ["agent_submission"], {"properties": {
        "agent_submission": {"type": "object"}, "manual_assertion": {"type": "null"},
        "speaker_role": {"enum": ["assistant", "tool", "unknown"]},
        "evidence_class": {"enum": ["model_claim", "summary", "unknown"]}}}),
    when("source_kind", ["export_message", "imported_document"], {"properties": {
        "import_id": c("importId"), "raw_object_hash": c("sha256")}}),
    when("source_kind", ["export_message", "external_event"], {"properties": {
        "provider": STR, "account_scope": {"type": "string"}}}),
    when("evidence_class", ["user_statement", "user_confirmation"], {"properties": {
        "speaker_role": {"const": "user"}}}),
    when("speaker_role", ["assistant"], {"properties": {
        "evidence_class": {"enum": ["model_claim", "summary", "unknown"]}}}),
]}
write("memory/source-v1.schema.json", record(
    "Enouia SourceRecord v1",
    "A stable evidence location. Role is data, not permission. Cross-field rules not expressible here (time precision, locator/kind pairing, created_at >= captured_at) are enforced by the Rust validator.",
    source_props, source_rules, "source-v1.schema.json"))

attachment_props = {
    "schema_version": c("schemaVersion"), "attachment_id": c("attachmentId"), "revision": c("revision"),
    "source_id": c("sourceId"), "original_name": nul(TEXT), "claimed_media_type": nul(STR),
    "detected_media_type": nul(STR), "size_bytes": nul(COUNT), "object_hash": cn("sha256"),
    "availability": enum("present", "missing", "external_reference", "quarantined", "unsupported"),
    "external_reference": nul(STR), "sensitivity": c("sensitivity"),
    "created_at": c("timestamp"), "extensions": c("extensions"),
}
write("memory/attachment-v1.schema.json", record(
    "Enouia AttachmentRecord v1",
    "Only 'present' carries a verified object hash. URLs are inert references, never fetched automatically.",
    attachment_props, {"allOf": [
        when("availability", ["present"], {"properties": {"object_hash": c("sha256"), "size_bytes": COUNT}}),
        {"if": {"properties": {"availability": {"enum": ["missing", "external_reference", "quarantined", "unsupported"]}}},
         "then": {"properties": {"object_hash": {"type": "null"}, "size_bytes": {"type": "null"}}}},
        when("availability", ["external_reference"], {"properties": {"external_reference": STR}}),
    ]}, "attachment-v1.schema.json"))

# ---------------- memory ----------------
source_rev_ref = c("sourceRevisionRef")
state_item = obj({"item_id": c("itemId"), "claim": STR, "evidence_refs": arr(source_rev_ref, minItems=1),
                  "state_kind": c("stateKind"), "as_of": c("timestampOrNull")})
decision_item = copy.deepcopy(state_item)
decision_item["properties"]["state_kind"] = {"const": "decided"}
open_loop = obj({"item_id": c("itemId"), "description": STR, "evidence_refs": arr(source_rev_ref, minItems=1),
                 "as_of": c("timestampOrNull")})
common_mem = {
    "schema_version": c("schemaVersion"), "memory_id": c("memoryId"), "revision": c("revision"),
    "title": STR, "content": STR, "subject_ids": arr(c("subjectId")), "project_id": cn("projectId"),
    "category": nul(c("label")), "tags": arr(c("label")), "source_id": c("sourceId"),
    "evidence": arr(c("evidenceRef"), minItems=1),
    "epistemic_status": enum("asserted", "corroborated", "uncertain", "disputed"),
    "confidence": nul({"type": "number", "minimum": 0, "maximum": 1}),
    "valid_from": c("timestampOrNull"), "valid_until": c("timestampOrNull"),
    "observed_at": c("timestampOrNull"), "last_verified_at": c("timestampOrNull"),
    "review_after": c("timestampOrNull"), "volatility": c("volatility"), "priority": c("priority"),
    "sensitivity": c("sensitivity"), "access_policy_id": c("policyId"), "egress_policy_id": cn("policyId"),
    "status": enum("active", "superseded", "archived"),
    "supersedes": arr(obj({"memory_id": c("memoryId"), "revision": c("revision"),
                           "effective_from": c("businessTime"), "scope": c("label")})),
    "conflict_group_id": cn("conflictGroupId"), "provenance_state": enum("intact", "broken"),
    "review_id": c("reviewId"), "approved_by": c("ownerRef"), "approved_at": c("timestamp"),
    "declassification_review_id": cn("reviewId"),
    "created_at": c("timestamp"), "updated_at": c("timestamp"), "extensions": c("extensions"),
}
variants = {
    "fact": ({"claim_key": c("label"), "subject_ids": arr(c("subjectId"), minItems=1)}, ["claim_key"]),
    "preference": ({"scope": STR, "strength": enum("explicit", "tentative"),
                    "subject_ids": arr(c("subjectId"), minItems=1)}, ["scope", "strength"]),
    "episode": ({"occurred_start": c("timestampOrNull"), "occurred_end": c("timestampOrNull"),
                 "occurred_precision": c("timePrecision"), "participants": arr(c("subjectId")),
                 "summary": STR}, ["occurred_start", "occurred_end", "occurred_precision", "participants", "summary"]),
    "project_state": ({"project_id": c("projectId"), "state": arr(state_item), "decisions": arr(decision_item),
                       "open_loops": arr(open_loop)}, ["state", "decisions", "open_loops"]),
    "session_checkpoint": ({"checkpoint_id": c("checkpointId"), "checkpoint_revision": c("revision"),
                            "session_id": c("sessionId"), "branch_id": c("branchId"),
                            "covered_events": obj({"from_sequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
                                                   "to_sequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE}}),
                            "coverage_hash": c("sha256"), "last_state": STR,
                            "last_completed_turn_id": cn("turnId"), "open_loops": arr(open_loop)},
                           ["checkpoint_id", "checkpoint_revision", "session_id", "branch_id", "covered_events",
                            "coverage_hash", "last_state", "last_completed_turn_id", "open_loops"]),
}
one_of = []
for t, (specific, req) in variants.items():
    props = dict(common_mem)
    props["type"] = {"const": t}
    props.update(specific)
    required = list(common_mem.keys()) + ["type"] + req
    one_of.append(obj(props, required))
write("memory/memory-v1.schema.json", {
    "$schema": D, "$id": "memory-v1.schema.json", "title": "Enouia CanonicalMemory v1",
    "description": "One reviewed memory revision. Keeps the Runtime M0 required fields and status enum; new fields are required with explicit null for unknown. Type-specific fields are top level. Reference closure, supersession graph, time order, and review binding are enforced by the Rust validators.",
    "oneOf": one_of})

write("memory/project-v1.schema.json", record(
    "Enouia ProjectEntity v1", "Stable project identity with aliases. Alias uniqueness (case-folded) across projects is a cross-record rule.",
    {"schema_version": c("schemaVersion"), "project_id": c("projectId"), "revision": c("revision"),
     "display_name": STR, "aliases": arr(STR, uniqueItems=True), "status": enum("active", "archived"),
     "sensitivity": c("sensitivity"), "review_id": c("reviewId"), "created_at": c("timestamp"),
     "updated_at": c("timestamp"), "extensions": c("extensions")}, None, "project-v1.schema.json"))

# ---------------- candidate / review ----------------
merge_target = {"oneOf": [obj({"kind": {"const": "candidate"}, "candidate_id": c("candidateId")}),
                          obj({"kind": {"const": "memory"}, "memory_id": c("memoryId")})]}
write("memory/candidate-v1.schema.json", record(
    "Enouia CandidateRecord v1",
    "A proposal. Never canonical by itself; terminal statuses cannot be reopened in place. Status history across revisions is a cross-record rule.",
    {"schema_version": c("schemaVersion"), "candidate_id": c("candidateId"), "revision": c("revision"),
     "proposal_kind": enum("create", "revise", "supersede", "archive", "identity_change", "delete"),
     "proposed_type": enum("fact", "preference", "episode", "project_state", "session_checkpoint", "identity"),
     "proposed_content": STR, "proposed_details": nul({"type": "object"}),
     "evidence": arr(c("evidenceRef"), minItems=1), "source_id": c("sourceId"), "reason": STR,
     "origin_kind": enum("owner_manual", "rule_extraction", "model_extraction", "agent_proposal", "external_event"),
     "origin_actor": c("actorRef"), "extraction_run_id": cn("extractionRunId"),
     "confidence": nul({"type": "number", "minimum": 0, "maximum": 1}), "sensitivity": c("sensitivity"),
     "declassification_review_id": cn("reviewId"),
     "status": enum("pending", "accepted", "rejected", "merged", "withdrawn"),
     "target_memory_id": cn("memoryId"), "target_identity_id": cn("identityId"),
     "expected_revision": nul(c("revision")), "proposed_effective_from": nul(c("businessTime")),
     "conflicts": arr(obj({"memory_id": c("memoryId"), "revision": c("revision"),
                           "conflict_group_id": cn("conflictGroupId")})),
     "dedupe_fingerprint": c("sha256"), "reopens_candidate_id": cn("candidateId"),
     "merged_into": nul(merge_target), "resolution_review_id": cn("reviewId"),
     "created_at": c("timestamp"), "updated_at": c("timestamp"), "extensions": c("extensions")},
    {"allOf": [
        when("proposal_kind", ["revise", "supersede", "archive", "delete"], {"properties": {
            "target_memory_id": c("memoryId"), "expected_revision": c("revision")}}),
        when("proposal_kind", ["create"], {"properties": {
            "target_memory_id": {"type": "null"}, "target_identity_id": {"type": "null"}, "expected_revision": {"type": "null"}}}),
        when("proposal_kind", ["identity_change"], {"properties": {"proposed_type": {"const": "identity"}}}),
        when("status", ["merged"], {"properties": {"merged_into": {"type": "object"}}}),
        when("status", ["accepted", "rejected", "merged"], {"properties": {"resolution_review_id": c("reviewId")}}),
        when("status", ["pending", "withdrawn"], {"properties": {"resolution_review_id": {"type": "null"}}}),
        when("origin_kind", ["rule_extraction", "model_extraction"], {"properties": {"extraction_run_id": c("extractionRunId")}}),
        when("origin_kind", ["owner_manual"], {"properties": {"origin_actor": c("ownerRef")}}),
        when("proposal_kind", ["delete"], {"properties": {"origin_kind": {"const": "owner_manual"}}}),
    ]}, "candidate-v1.schema.json"))

write("memory/review-v1.schema.json", record(
    "Enouia ReviewRecord v1",
    "Owner decision on one exact candidate revision, from a trusted local surface. Nonce single use and candidate/commit binding are cross-record rules.",
    {"schema_version": c("schemaVersion"), "review_id": c("reviewId"), "actor": c("ownerRef"),
     "trusted_surface": c("trustedSurface"),
     "action": enum("accept", "edit_accept", "reject", "merge", "supersede", "identity_accept", "confirm_delete"),
     "candidate_id": c("candidateId"), "candidate_revision": c("revision"),
     "final_content_hash": c("sha256"), "approved_diff_hash": c("sha256"),
     "evidence_refs": arr(c("sourceRevisionRef")), "target_expected_revisions": arr(c("recordRef")),
     "approval_nonce": {"type": "string", "pattern": "^[A-Za-z0-9_-]{22,128}$"},
     "effective_from": nul(c("businessTime")), "merge_target": nul(merge_target),
     "resulting_records": arr(c("recordRef")), "reason_code": nul(c("code")),
     "created_at": c("timestamp"), "commit_id": c("commitId")},
    {"allOf": [
        when("action", ["reject"], {"properties": {"resulting_records": {"maxItems": 0}}}),
        when("action", ["accept", "edit_accept", "supersede", "identity_accept", "confirm_delete"], {"properties": {"resulting_records": {"minItems": 1}}}),
        when("action", ["supersede"], {"properties": {"effective_from": c("businessTime")}}),
        when("action", ["merge"], {"properties": {"merge_target": {"type": "object"}}}),
    ]}, "review-v1.schema.json"))

write("memory/identity-v1.schema.json", record(
    "Enouia Identity metadata v1",
    "Sidecar for one Markdown Identity revision (content addressed by content_hash). Identity text never changes ACL, egress, or tool authorization.",
    {"schema_version": c("schemaVersion"), "identity_id": c("identityId"), "revision": c("revision"),
     "slug": enum("core", "runtime_rules", "style", "relationship", "boundaries"), "title": STR,
     "content_hash": c("sha256"), "content_media_type": {"const": "text/markdown; charset=utf-8"},
     "sensitivity": c("sensitivity"), "access_policy_id": c("policyId"), "egress_policy_id": cn("policyId"),
     "previous_revision": nul(c("revision")), "review_id": c("reviewId"), "approved_by": c("ownerRef"),
     "approved_at": c("timestamp"), "created_at": c("timestamp"), "updated_at": c("timestamp"),
     "extensions": c("extensions")}, None, "identity-v1.schema.json"))

# ---------------- session ----------------
write("memory/session-v1.schema.json", record(
    "Enouia SessionRecord v1", "Session header with branch list and provider binding history.",
    {"schema_version": c("schemaVersion"), "session_id": c("sessionId"), "revision": c("revision"),
     "origin_surface": c("clientSurface"),
     "provider_bindings": arr(obj({"binding": c("providerBinding"), "from_sequence": COUNT})),
     "branches": arr(obj({"branch_id": c("branchId"), "parent_branch_id": cn("branchId"),
                          "forked_from_event_id": cn("eventId"), "last_event_seq": COUNT}), minItems=1),
     "default_branch_id": c("branchId"), "parent_session_id": cn("sessionId"),
     "participants": arr(c("actorRef")), "last_event_seq": COUNT,
     "status": enum("open", "closed", "interrupted"), "sensitivity": c("sensitivity"),
     "policy_id": c("policyId"), "created_at": c("timestamp"), "updated_at": c("timestamp"),
     "extensions": c("extensions")}, None, "session-v1.schema.json"))

kinds_state = [("assistant_chunk", "partial"), ("assistant_completed", "completed"),
               ("turn_cancelled", "interrupted"), ("turn_failed", "failed")]
event_rules = [when("kind", [k], {"properties": {"delivery_state": {"const": s}}}) for k, s in kinds_state]
event_rules.append(when("kind", ["user_message", "tool_request", "tool_result", "checkpoint_created", "provider_switched"],
                        {"properties": {"delivery_state": {"const": "not_applicable"}}}))
event_rules.append(when("kind", ["user_message", "assistant_chunk", "assistant_completed", "tool_request", "tool_result"],
                        {"properties": {"content_ref": c("contentRef"), "turn_id": c("turnId")}}))
write("memory/session-event-v1.schema.json", record(
    "Enouia SessionEvent v1",
    "Append-only conversation event. No kind exists for hidden model reasoning. Sequence uniqueness and parent order are cross-record rules.",
    {"schema_version": c("schemaVersion"), "event_id": c("eventId"), "session_id": c("sessionId"),
     "branch_id": c("branchId"), "sequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
     "parent_event_id": cn("eventId"), "turn_id": cn("turnId"),
     "kind": enum("user_message", "assistant_chunk", "assistant_completed", "tool_request", "tool_result",
                  "turn_cancelled", "turn_failed", "checkpoint_created", "provider_switched"),
     "actor": c("actorRef"), "occurred_at": c("timestamp"), "captured_at": c("timestamp"),
     "content_ref": nul(c("contentRef")), "source_refs": arr(c("sourceRevisionRef")),
     "request_id": cn("requestId"),
     "delivery_state": enum("not_applicable", "partial", "completed", "interrupted", "failed"),
     "sensitivity": c("sensitivity"), "extensions": c("extensions")},
    {"allOf": event_rules}, "session-event-v1.schema.json"))

sourced_item = obj({"item_id": c("itemId"), "claim": STR, "state_kind": c("stateKind"),
                    "source_refs": arr({"oneOf": [
                        obj({"kind": {"const": "event"}, "event_id": c("eventId")}),
                        obj({"kind": {"const": "source"}, "source_id": c("sourceId"), "source_revision": c("revision")})]},
                        minItems=1)})
write("memory/checkpoint-v1.schema.json", record(
    "Enouia SessionCheckpoint artifact v1",
    "Provisional Session artifact. Becomes a canonical session_checkpoint memory only after owner review. Coverage closure is a cross-record rule.",
    {"schema_version": c("schemaVersion"), "checkpoint_id": c("checkpointId"), "revision": c("revision"),
     "session_id": c("sessionId"), "branch_id": c("branchId"),
     "coverage": {"oneOf": [
         obj({"kind": {"const": "range"}, "from_sequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
              "to_sequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE}}),
         obj({"kind": {"const": "event_ids"}, "event_ids": arr(c("eventId"), minItems=1, uniqueItems=True)})]},
     "coverage_hash": c("sha256"), "base_vault_commit_id": c("commitId"), "summary": STR,
     "decisions": arr(sourced_item), "open_loops": arr(sourced_item),
     "last_completed_turn_id": cn("turnId"),
     "generated_by": obj({"actor": c("actorRef"), "generator_version": STR}),
     "status": enum("provisional", "reviewed", "stale"), "review_id": cn("reviewId"),
     "sensitivity": c("sensitivity"), "created_at": c("timestamp"), "extensions": c("extensions")},
    {"allOf": [when("status", ["reviewed"], {"properties": {"review_id": c("reviewId")}}),
               when("status", ["provisional", "stale"], {"properties": {"review_id": {"type": "null"}}})]},
    "checkpoint-v1.schema.json"))

# ---------------- commit / deletion / audit ----------------
write("memory/commit-v1.schema.json", record(
    "Enouia CommitManifest v1",
    "Complete catalog of logical records at one commit. A commit is published by one CURRENT replacement in MV-1; this schema cannot prove atomicity or durability. Chain, epochs, idempotency, and catalog closure are cross-record rules.",
    {"schema_version": c("schemaVersion"), "commit_id": c("commitId"), "format_version": {"const": 1},
     "parent_commit_id": cn("commitId"), "sequence": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
     "vault_id": c("vaultId"), "writer_device_id": c("deviceId"), "principal": c("actorRef"),
     "operation_id": c("operationId"),
     "operation_kind": enum("genesis", "import", "candidate_propose", "review_commit", "identity_review",
                            "session_append", "checkpoint_propose", "logical_delete", "purge",
                            "policy_change", "migration", "restore_adopt"),
     "idempotency_key_hash": cn("sha256"), "request_payload_hash": c("sha256"), "created_at": c("timestamp"),
     "catalog": arr(obj({"record_kind": c("recordKind"),
                         "record_id": {"type": "string", "pattern": f"^[a-z]+_{UUID}$"},
                         "revision": c("revision"), "content_hash": c("sha256"), "changed": BOOL})),
     "objects": arr(obj({"object_hash": c("sha256"), "size_bytes": COUNT,
                         "object_kind": enum("raw", "asset", "session_content", "identity_markdown")})),
     "review_ids": arr(c("reviewId")), "tombstone_ids": arr(c("deleteId")),
     "policy_epoch": COUNT, "deletion_epoch": COUNT,
     "receipt": obj({"operation_id": c("operationId"), "result": {"const": "committed"},
                     "records": arr(c("recordRef"))})},
    {"allOf": [
        when("operation_kind", ["genesis"], {"properties": {"parent_commit_id": {"type": "null"},
                                                            "sequence": {"const": 1}, "idempotency_key_hash": {"type": "null"},
                                                            "principal": c("ownerRef")}}),
        {"if": {"properties": {"operation_kind": {"not": {"const": "genesis"}}}, "required": ["operation_kind"]},
         "then": {"properties": {"parent_commit_id": c("commitId"), "idempotency_key_hash": c("sha256"),
                                 "sequence": {"minimum": 2}}}},
    ]}, "commit-v1.schema.json"))

write("memory/tombstone-v1.schema.json", record(
    "Enouia Tombstone v1", "Deletion overrides every status and default Raw immutability. Contains no deleted text.",
    {"schema_version": c("schemaVersion"), "delete_id": c("deleteId"),
     "mode": enum("logical_delete", "purge"), "scope": enum("selected_revisions", "all_revisions", "with_dependents"),
     "targets": arr(obj({"record_kind": c("recordKind"),
                         "record_id": {"type": "string", "pattern": f"^[a-z]+_{UUID}$"},
                         "revision": nul(c("revision"))})),
     "object_hashes": arr(c("sha256")), "requested_by": c("ownerRef"), "review_id": c("reviewId"),
     "deletion_epoch": {"type": "integer", "minimum": 1, "maximum": MAXSAFE}, "created_at": c("timestamp")},
    None, "tombstone-v1.schema.json"))

stores = ["canonical", "raw", "assets", "candidates", "sessions", "checkpoints", "capsules", "index",
          "embeddings", "staging", "exports", "backups", "device_replicas", "gateway"]
write("memory/purge-receipt-v1.schema.json", record(
    "Enouia PurgeReceipt v1",
    "Per-store purge status. There is deliberately no globally_erased state; overall-state honesty is checked by the Rust validator.",
    {"schema_version": c("schemaVersion"), "receipt_id": c("purgeReceiptId"), "delete_id": c("deleteId"),
     "stores": arr(obj({"store": enum(*stores),
                        "state": enum("purged", "not_present", "pending", "failed", "not_manageable"),
                        "confirmed_at": c("timestampOrNull"), "detail_code": nul(c("code"))}), minItems=1),
     "overall_state": enum("live_deleted", "backup_purge_pending", "replica_pending", "locally_purged"),
     "latest_purge_deadline": c("timestampOrNull"), "created_at": c("timestamp")},
    None, "purge-receipt-v1.schema.json"))

write("memory/audit-event-v1.schema.json", record(
    "Enouia AuditEvent v1", "IDs, enums, and codes only: no record text, query text, secrets, or paths.",
    {"schema_version": c("schemaVersion"), "audit_id": c("auditId"), "actor": c("actorRef"),
     "operation": c("operation"), "object_refs": arr(c("recordRef")), "purpose": cn("purpose"),
     "destination": cn("destinationKind"), "policy_epoch": COUNT, "decision": enum("allow", "deny"),
     "error_code": cn("memoryErrorCode"), "request_id": c("requestId"), "created_at": c("timestamp")},
    {"allOf": [when("decision", ["deny"], {"properties": {"error_code": c("memoryErrorCode")}})]},
    "audit-event-v1.schema.json"))

# ---------------- context ----------------
MC = "../memory/common-v1.schema.json"
def m(name):
    return ref(name, MC)
def mn(name):
    return nul(m(name))
destination = obj({"kind": m("destinationKind"), "provider_binding": nul(m("providerBinding"))})
currency = enum("current_supported", "historical_only", "needs_reverification", "conflicted")
memory_item = obj({"memory_id": m("memoryId"), "revision": m("revision"), "type": m("memoryType"),
                   "content": STR, "currency": currency, "valid_from": m("timestampOrNull"),
                   "valid_until": m("timestampOrNull"), "last_verified_at": m("timestampOrNull"),
                   "evidence": arr(m("sourceRevisionRef"), minItems=1), "conflict_group_id": mn("conflictGroupId"),
                   "sensitivity": m("sensitivity")})
cdefs = {"destination": destination, "memoryItem": memory_item, "currency": currency}
write("context/common-v1.schema.json", {"$schema": D, "$id": BASE + "context/common-v1.schema.json",
      "title": "Enouia Context shared definitions v1", "$defs": cdefs})
CC = "common-v1.schema.json"
write("context/capsule-v1.schema.json", record(
    "Enouia Context Capsule v1",
    "Logical context for one request. Keeps the Architecture v0.3 capsule fields. Inclusion eligibility (tombstones, policy, valid time, provenance) is checked against the record set by the Rust validator.",
    {"schema_version": m("schemaVersion"), "capsule_id": m("capsuleId"), "generated_at": m("timestamp"),
     "request_id": m("requestId"), "query": STR, "as_of": m("timestampOrNull"),
     "vault_commit_id": m("commitId"), "policy_epoch": COUNT, "deletion_epoch": COUNT,
     "compiler_version": STR, "ranking_version": STR, "tokenizer_version": STR,
     "client_surface": m("clientSurface"), "destination": ref("destination", CC), "purpose": m("purpose"),
     "session_id": mn("sessionId"), "branch_id": mn("branchId"),
     "identity": arr(obj({"identity_id": m("identityId"), "revision": m("revision"),
                          "slug": enum("core", "runtime_rules", "style", "relationship", "boundaries"), "text": STR})),
     "user_context": arr(ref("memoryItem", CC)), "relationship_context": arr(ref("memoryItem", CC)),
     "active_projects": arr(ref("memoryItem", CC)), "relevant_memories": arr(ref("memoryItem", CC)),
     "recent_session_checkpoints": arr(obj({"checkpoint_id": m("checkpointId"), "revision": m("revision"),
                                            "status": enum("provisional", "reviewed"), "summary": STR})),
     "recent_turns": arr(obj({"event_id": m("eventId"), "role": enum("user", "assistant", "tool"), "text": TEXT})),
     "open_loops": arr(obj({"item_id": m("itemId"), "description": STR, "origin": enum("memory", "checkpoint"),
                            "origin_id": STR, "provisional": BOOL})),
     "provenance": arr(obj({"source_id": m("sourceId"), "source_revision": m("revision"),
                            "occurred_at": m("timestampOrNull"), "time_precision": m("timePrecision"),
                            "locator_kind": enum("json_pointer", "byte_range", "runtime_event", "manual_input", "archive_member")})),
     "budget": obj({"max_tokens": {"type": "integer", "minimum": 1, "maximum": MAXSAFE},
                    "memory_budget_tokens": COUNT, "estimated_tokens": COUNT,
                    "counting_method": enum("utf8_bytes_v1", "provider_exact"), "safety_margin_tokens": COUNT}),
     "verification_needed": arr(obj({"memory_id": mn("memoryId"), "item_id": mn("itemId"),
                                     "conflict_group_id": mn("conflictGroupId"),
                                     "reason": enum("implementation_unverified", "live_value", "review_overdue",
                                                    "conflicted", "no_supported_evidence", "supersession_time_unknown")})),
     "completeness": obj({"complete": BOOL, "limitations": arr(enum(
         "over_budget", "policy_limited", "index_not_ready", "no_supported_memory", "broken_provenance_omitted"),
         uniqueItems=True)})},
    None, "capsule-v1.schema.json"))

reasons_in = ["explicit_entity_match", "direct_support", "current_supported", "historical_match",
              "identity_required", "session_continuity"]
reasons_out = ["unrelated", "superseded", "expired", "not_yet_effective", "conflicted", "pending", "rejected",
               "policy_denied", "over_budget", "broken_provenance", "tombstoned", "archived"]
decision = obj({"record_kind": enum("memory", "candidate", "checkpoint", "identity"),
                "record_id": {"type": "string", "pattern": f"^[a-z]+_{UUID}$"}, "revision": m("revision"),
                "decision": enum("included", "excluded"), "reason": enum(*(reasons_in + reasons_out)),
                "rank": nul(U32), "token_cost": nul(COUNT), "source_reachable": BOOL, "truncated": BOOL},
               extra={"allOf": [
                   when("decision", ["included"], {"properties": {"reason": enum(*reasons_in)}}),
                   when("decision", ["excluded"], {"properties": {"reason": enum(*reasons_out)}})]})
write("context/inspection-v1.schema.json", record(
    "Enouia Context Inspection v1",
    "Local explanation of one compilation. Never sent to a Provider. Restricted viewers never receive hidden (policy_denied/tombstoned) entries.",
    {"schema_version": m("schemaVersion"), "inspection_id": m("inspectionId"), "capsule_id": m("capsuleId"),
     "request_id": m("requestId"), "generated_at": m("timestamp"), "vault_commit_id": m("commitId"),
     "policy_epoch": COUNT, "deletion_epoch": COUNT, "ranking_version": STR,
     "viewer_scope": enum("owner_full", "restricted"), "decisions": arr(decision)},
    {"allOf": [when("viewer_scope", ["restricted"], {"properties": {"decisions": arr(
        {"properties": {"reason": {"not": {"enum": ["policy_denied", "tombstoned"]}}}})}})]},
    "inspection-v1.schema.json"))

write("context/dispatch-v1.schema.json", record(
    "Enouia Dispatch Record v1",
    "What was actually sent: roles, byte hashes, carried capsule memories, tools, and the egress barrier used. Keys and auth headers are never recorded.",
    {"schema_version": m("schemaVersion"), "dispatch_id": m("dispatchId"), "capsule_id": m("capsuleId"),
     "inspection_id": m("inspectionId"), "request_id": m("requestId"), "destination": ref("destination", CC),
     "request_hash": m("sha256"),
     "messages": arr(obj({"role": enum("system", "user", "assistant", "tool"), "content_hash": m("sha256"),
                          "size_bytes": COUNT,
                          "capsule_memory_refs": arr(obj({"memory_id": m("memoryId"), "revision": m("revision")}))}),
                     minItems=1),
     "tools": arr(obj({"name": STR, "definition_hash": m("sha256")})),
     "egress": obj({"policy_epoch": COUNT, "deletion_epoch": COUNT, "egress_policy_id": mn("policyId"),
                    "confirmation_review_id": mn("reviewId"), "checked_at": m("timestamp")}),
     "state": enum("prepared", "sent", "completed", "failed", "cancelled", "outcome_unknown"),
     "prepared_at": m("timestamp"), "sent_at": m("timestampOrNull"), "completed_at": m("timestampOrNull")},
    {"allOf": [when("state", ["sent", "completed", "outcome_unknown"], {"properties": {"sent_at": m("timestamp")}}),
               when("state", ["prepared", "cancelled"], {"properties": {"sent_at": {"type": "null"}}}),
               when("state", ["completed"], {"properties": {"completed_at": m("timestamp")}})]},
    "dispatch-v1.schema.json"))

# ---------------- provider ----------------
cap = enum("supported", "unsupported", "unknown")
write("provider/capabilities-v1.schema.json", record(
    "Enouia ProviderCapabilities v1",
    "Verified capability snapshot. Unverified snapshots may only say unknown; nothing is inferred from a model name.",
    {"schema_version": m("schemaVersion"), "binding": m("providerBinding"), "text_input": cap, "image_input": cap,
     "streaming": cap, "tool_calling": cap, "cancellation": cap,
     "token_counting": enum("exact", "estimated", "unknown"),
     "context_window_tokens": nul(COUNT), "max_output_tokens": nul(COUNT), "verified_at": m("timestampOrNull")},
    {"allOf": [{"if": {"properties": {"verified_at": {"type": "null"}}, "required": ["verified_at"]},
                "then": {"properties": {"text_input": {"const": "unknown"}, "image_input": {"const": "unknown"},
                                        "streaming": {"const": "unknown"}, "tool_calling": {"const": "unknown"},
                                        "cancellation": {"const": "unknown"}, "token_counting": {"const": "unknown"},
                                        "context_window_tokens": {"type": "null"}, "max_output_tokens": {"type": "null"}}}}]},
    "capabilities-v1.schema.json"))
print("ok")

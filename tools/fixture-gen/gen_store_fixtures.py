"""Generates synthetic fixtures for the Vault store file contracts (MV-1,
ADR-MEM-36): descriptor, CURRENT, publish journal line, idempotency entry,
recovery receipt, pinned-commit export manifest, and restore state; and the
segmented catalog (MV-3.0, ADR-MEM-39): catalog segments and stored commits."""
import hashlib, json, pathlib

OUT = pathlib.Path(__file__).resolve().parents[2] / "tests" / "fixtures" / "store"


def uid(prefix, n):
    return f"{prefix}_{n:08x}-0000-4000-8000-{n:012x}"


def sha(text):
    return hashlib.sha256(text.encode("utf-8")).hexdigest()


def canonical(doc):
    return json.dumps(doc, ensure_ascii=False, indent=2, sort_keys=True) + "\n"


def dump(name, value):
    path = OUT / name
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical(value).encode("utf-8"))


OWNER = {"actor_id": uid("prn", 1), "actor_type": "owner"}
VAULT = uid("vlt", 1)
GENESIS = uid("cmt", 1)
HEAD = uid("cmt", 7)
T0 = "2026-09-28T08:00:00.000Z"
T1 = "2026-09-28T09:30:00.000Z"

valid = {
    "descriptor.json": ("store/descriptor-v1.schema.json", "descriptor", {
        "schema_version": 1, "vault_id": VAULT, "format_version": 2, "genesis_commit_id": GENESIS,
        "genesis_device_id": uid("dev", 1), "created_by": OWNER, "created_at": T0}),
    "current.json": ("store/current-v1.schema.json", "current", {
        "schema_version": 1, "vault_id": VAULT, "commit_id": HEAD, "sequence": 7,
        "manifest_sha256": sha("manifest-7")}),
    "publish-record.json": ("store/publish-record-v1.schema.json", "publish_record", {
        "schema_version": 1, "vault_id": VAULT, "commit_id": HEAD, "sequence": 7,
        "manifest_sha256": sha("manifest-7"), "published_at": T1}),
    "idempotency.json": ("store/idempotency-v1.schema.json", "idempotency", {
        "schema_version": 1, "vault_id": VAULT, "principal_id": uid("prn", 1),
        "operation_kind": "review_commit", "key_hash": sha("key-7"),
        "request_payload_hash": sha("payload-7"), "commit_id": HEAD, "sequence": 7}),
    "recovery-receipt.json": ("store/recovery-receipt-v1.schema.json", "recovery_receipt", {
        "schema_version": 1, "recovery_id": uid("op", 90), "vault_id": VAULT,
        "adopted_commit_id": uid("cmt", 6), "adopted_sequence": 6, "manifest_sha256": sha("manifest-6"),
        "previous_current_sha256": sha("torn current"), "evidence": "publish_journal",
        "approved_by": OWNER, "trusted_surface": "trusted_local_cli", "created_at": T1}),
    "export-manifest.json": ("store/export-manifest-v1.schema.json", "export_manifest", {
        "schema_version": 1, "export_format": 2, "vault_id": VAULT, "commit_id": HEAD, "sequence": 7,
        "policy_epoch": 1, "deletion_epoch": 0, "manifest_sha256": sha("manifest-7"), "created_at": T1,
        "files": [
            {"path": f"vault/commits/{GENESIS}.json", "sha256": sha("manifest-1"), "size_bytes": 1200},
            {"path": f"vault/commits/{HEAD}.json", "sha256": sha("manifest-7"), "size_bytes": 4800},
            {"path": f"vault/raw/objects/{sha('raw')}", "sha256": sha("raw"), "size_bytes": 3},
            {"path": f"vault/records/policy/{uid('pol', 1)}/1.json", "sha256": sha("policy"), "size_bytes": 900},
            {"path": "vault/vault.json", "sha256": sha("descriptor"), "size_bytes": 400},
        ]}),
    "restore-state.json": ("store/restore-state-v1.schema.json", "restore_state", {
        "schema_version": 1, "vault_id": VAULT, "restored_commit_id": HEAD,
        "export_manifest_sha256": sha("export"), "restored_at": T1,
        "network_disabled_until_reconciled": True, "reconciled_at": None}),
    "segment-records.json": ("store/catalog-segment-v1.schema.json", "catalog_segment", {
        "schema_version": 1, "segment_kind": "records", "record_kind": "source", "prefix": "0000000",
        "records": [
            {"record_id": uid("src", 1), "revision": 2, "content_hash": sha("source-1@2"),
             "prior_hashes": [sha("source-1@1")], "group_ids": [uid("imp", 1), uid("imp", 2)]},
            {"record_id": uid("src", 2), "revision": 1, "content_hash": sha("source-2@1"),
             "prior_hashes": [], "group_ids": []}],
        "objects": []}),
    "segment-objects.json": ("store/catalog-segment-v1.schema.json", "catalog_segment", {
        "schema_version": 1, "segment_kind": "objects", "record_kind": None, "prefix": "",
        "records": [],
        "objects": sorted([
            {"object_hash": sha("raw export"), "object_kind": "raw", "size_bytes": 10, "stored_with": None},
            {"object_hash": sha("identity markdown"), "object_kind": "identity_markdown", "size_bytes": 17,
             "stored_with": {"record_kind": "identity", "record_id": uid("idn", 1), "revision": 1}}],
            key=lambda o: o["object_hash"])}),
    "stored-commit.json": ("store/stored-commit-v1.schema.json", "stored_commit", {
        "schema_version": 1, "commit_id": HEAD, "format_version": 2, "parent_commit_id": uid("cmt", 6),
        "sequence": 7, "vault_id": VAULT, "writer_device_id": uid("dev", 1), "principal": OWNER,
        "operation_id": uid("op", 7), "operation_kind": "import", "idempotency_key_hash": sha("key-7"),
        "request_payload_hash": sha("payload-7"), "created_at": T1, "segment_capacity": 512,
        "record_segments": [
            {"segment_kind": "records", "record_kind": "policy", "prefix": "", "entry_count": 1,
             "segment_hash": sha("segment policy")},
            {"segment_kind": "records", "record_kind": "source", "prefix": "", "entry_count": 2,
             "segment_hash": sha("segment source")}],
        "object_segments": [
            {"segment_kind": "objects", "record_kind": None, "prefix": "", "entry_count": 2,
             "segment_hash": sha("segment objects")}],
        "review_ids": [], "tombstone_ids": [], "policy_epoch": 1, "deletion_epoch": 0,
        "receipt": {"operation_id": uid("op", 7), "result": "committed",
                    "records": [{"record_kind": "source", "record_id": uid("src", 2), "revision": 1}]}}),
}


def case(id, base, covers, ops, schema, rule):
    return {"id": id, "base": base, "covers": covers, "ops": ops, "schema": schema, "rust_rule": rule}


def st(path, value):
    return {"op": "set", "path": path, "value": value}


def rm(path):
    return {"op": "remove", "path": path}


AGENT = {"actor_id": uid("prn", 5), "actor_type": "agent"}
invalid = [
    case("descriptor-agent-creator", "descriptor.json", "genesis is owner-confirmed",
         [st("/created_by", AGENT)], "reject", "vault.owner_required"),
    case("descriptor-format-1", "descriptor.json", "pre-segmented layout is refused", [st("/format_version", 1)], "reject", "shape"),
    case("descriptor-format-3", "descriptor.json", "unknown vault format", [st("/format_version", 3)], "reject", "shape"),
    case("descriptor-unknown-field", "descriptor.json", "unknown field", [st("/owner_name", "x")], "reject", "shape"),
    case("current-sequence-zero", "current.json", "sequence minimum", [st("/sequence", 0)], "reject", "current.sequence"),
    case("current-bad-hash", "current.json", "manifest hash syntax", [st("/manifest_sha256", "ABC")], "reject", "shape"),
    case("current-schema-2", "current.json", "unknown major is read-only", [st("/schema_version", 2)], "reject",
         "unsupported_schema"),
    case("current-missing-vault", "current.json", "required field", [rm("/vault_id")], "reject", "shape"),
    case("publish-sequence-overflow", "publish-record.json", "safe integer range",
         [st("/sequence", 9007199254740992)], "reject", "number.out_of_range"),
    case("idempotency-genesis", "idempotency.json", "genesis has no idempotency key",
         [st("/operation_kind", "genesis")], "reject", "idempotency.genesis"),
    case("idempotency-sequence-one", "idempotency.json", "sequence 1 is genesis",
         [st("/sequence", 1)], "reject", "idempotency.sequence"),
    case("recovery-system-approver", "recovery-receipt.json", "owner-only recovery choice",
         [st("/approved_by", {"actor_id": uid("prn", 2), "actor_type": "system"})], "reject",
         "recovery.owner_required"),
    case("recovery-guessed-newest", "recovery-receipt.json", "no 'newest manifest' evidence",
         [st("/evidence", "newest_manifest")], "reject", "shape"),
    case("recovery-missing-previous", "recovery-receipt.json", "explicit null required",
         [rm("/previous_current_sha256")], "reject", "shape"),
    case("export-dot-dot", "export-manifest.json", "path escape",
         [st("/files/2/path", "vault/../raw")], "accept", "export.path"),
    case("export-current", "export-manifest.json", "live pointer excluded",
         [st("/files/0/path", "vault/CURRENT")], "accept", "export.path"),
    case("export-staging", "export-manifest.json", "unpublished staging excluded",
         [st("/files/2/path", "vault/staging/op_1/x")], "accept", "export.path"),
    case("export-backslash", "export-manifest.json", "Windows separator",
         [st("/files/2/path", "vault\\raw")], "reject", "export.path"),
    case("export-unsorted", "export-manifest.json", "deterministic order",
         [st("/files/4/path", "vault/aaa.json")], "accept", "export.order"),
    case("export-duplicate", "export-manifest.json", "unique paths",
         [st("/files/3/path", f"vault/raw/objects/{sha('raw')}")], "accept", "export.order"),
    case("export-missing-descriptor", "export-manifest.json", "descriptor required",
         [rm("/files/4")], "accept", "export.required_file"),
    case("export-missing-head", "export-manifest.json", "head manifest required",
         [rm("/files/1")], "accept", "export.required_file"),
    case("restore-network-open", "restore-state.json", "network gate before reconciliation",
         [st("/network_disabled_until_reconciled", False)], "reject", "restore.network_gate"),
    case("restore-reconciled-before-restore", "restore-state.json", "time order",
         [st("/reconciled_at", T0), st("/network_disabled_until_reconciled", False)], "accept",
         "restore.reconciled_order"),
    case("segment-unsorted", "segment-records.json", "deterministic entry order",
         [st("/records/0/record_id", uid("src", 3))], "accept", "segment.order"),
    case("segment-prefix-mismatch", "segment-records.json", "entries lie under the prefix",
         [st("/prefix", "1")], "accept", "segment.prefix_mismatch"),
    case("segment-prefix-not-hex", "segment-records.json", "hex prefix", [st("/prefix", "0g")], "reject",
         "segment.prefix"),
    case("segment-prior-count", "segment-records.json", "one prior hash per earlier revision",
         [st("/records/0/prior_hashes", [])], "accept", "segment.prior_hashes"),
    case("segment-group-kind", "segment-records.json", "sources group by import",
         [st("/records/0/group_ids", [uid("ses", 1)])], "accept", "segment.group"),
    case("segment-mixed-entries", "segment-records.json", "one entry kind per segment",
         [st("/objects", [{"object_hash": sha("x"), "object_kind": "raw", "size_bytes": 1, "stored_with": None}])],
         "reject", "segment.kind_entries"),
    case("segment-identity-unlocated", "segment-objects.json", "identity Markdown names its revision",
         [st("/objects/0/stored_with", None), st("/objects/1/stored_with", None)], "accept",
         "segment.object_location"),
    case("stored-noncanonical", "stored-commit.json", "canonical partition",
         [{"op": "append", "path": "/record_segments", "value": {"segment_kind": "records", "record_kind": "source",
          "prefix": "0", "entry_count": 1, "segment_hash": sha("extra")}}], "accept", "stored.partition"),
    case("stored-overfull", "stored-commit.json", "leaf capacity",
         [st("/record_segments/1/entry_count", 513)], "accept", "stored.partition"),
    case("stored-capacity", "stored-commit.json", "capacity range", [st("/segment_capacity", 8)], "reject",
         "stored.capacity"),
    case("stored-unsorted", "stored-commit.json", "deterministic segment order",
         [st("/record_segments/0/record_kind", "source"), st("/record_segments/1/record_kind", "policy")],
         "accept", "stored.segment_order"),
    case("stored-format-1", "stored-commit.json", "stored commits are format 2", [st("/format_version", 1)],
         "reject", "shape"),
]

for name, (_, _, doc) in valid.items():
    dump(name, doc)
dump("store-manifest.json", {
    "description": "Synthetic Vault store file contracts (ADR-MEM-36, ADR-MEM-39). 'schema: accept' marks a constraint JSON Schema cannot express; the Rust validator must still reject it.",
    "valid": [{"file": name, "schema": schema, "type": kind} for name, (schema, kind, _) in valid.items()],
    "invalid": invalid,
})
print("ok")

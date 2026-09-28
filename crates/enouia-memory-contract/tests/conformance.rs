//! D01 contract/static part: every schema resolves, every valid fixture passes
//! both the JSON Schema harness and the Rust parser and round-trips without
//! field loss, and every invalid mutation is rejected by Rust with the named
//! rule. `schema: accept` cases document constraints JSON Schema cannot prove.

mod support;

use enouia_memory_contract::error::MemoryErrorCode;
use enouia_memory_contract::ids::ID_PREFIXES;
use enouia_memory_contract::ipc::Operation;
use enouia_memory_contract::json::{
    SchemaDisposition, canonical_bytes, ensure_writable, schema_disposition,
};
use enouia_memory_contract::{RecordKind, parse_any};
use serde_json::{Value, json};
use support::regex::Regex;
use support::schema::SchemaStore;
use support::{apply_ops, fixture};

fn kind(value: &Value) -> RecordKind {
    serde_json::from_value(value.clone()).expect("record kind")
}

#[test]
fn regex_subset_behaves_like_ecma_for_used_patterns() {
    let id =
        Regex::new("^mem_[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$");
    assert!(id.is_match("mem_00000001-0000-4000-8000-000000000001"));
    assert!(!id.is_match("mem_00000001-0000-4000-8000-00000000000"));
    assert!(!id.is_match("xmem_00000001-0000-4000-8000-000000000001"));
    let ns = Regex::new("^[a-z][a-z0-9-]*(\\.[a-z][a-z0-9-]*)+$");
    assert!(ns.is_match("org.example.import"));
    assert!(!ns.is_match("enouia"));
    assert!(Regex::new("^(/.*)?$").is_match(""));
    assert!(Regex::new("b+").is_match("abbc"));
    assert!(!Regex::new("^[^/\\\\:][^\\\\:]*$").is_match("C:\\x"));
}

#[test]
fn all_schemas_resolve_and_use_only_supported_keywords() {
    let store = SchemaStore::load();
    let refs = store.check_all_refs();
    assert!(refs > 100, "expected many refs, found {refs}");
    // Validation walks every keyword, so validating each schema's own valid
    // fixtures below also proves no unsupported keyword is silently ignored.
    assert!(store.names().len() >= 20);
}

#[test]
fn id_prefixes_error_codes_and_operations_match_the_schemas() {
    let store = SchemaStore::load();
    let common = store.doc("memory/common-v1.schema.json");
    for (name, prefix) in ID_PREFIXES {
        let pattern = common["$defs"][name]["pattern"].as_str().unwrap();
        assert!(pattern.starts_with(&format!("^{prefix}_")), "{name}");
    }
    let codes: Vec<Value> = MemoryErrorCode::ALL
        .iter()
        .map(|c| serde_json::to_value(c).unwrap())
        .collect();
    assert_eq!(
        common["$defs"]["memoryErrorCode"]["enum"],
        Value::Array(codes)
    );
    let ops: Vec<Value> = Operation::ALL
        .iter()
        .map(|o| serde_json::to_value(o).unwrap())
        .collect();
    assert_eq!(common["$defs"]["operation"]["enum"], Value::Array(ops));
}

#[test]
fn valid_records_pass_schema_and_rust_and_round_trip_losslessly() {
    let store = SchemaStore::load();
    let manifest = fixture("records-manifest.json");
    let valid = manifest["valid"].as_array().unwrap();
    assert!(valid.len() >= 40);
    let mut kinds = std::collections::BTreeSet::new();
    for entry in valid {
        let file = entry["file"].as_str().unwrap();
        let value = fixture(file);
        let errors = store.validate(entry["schema"].as_str().unwrap(), &value);
        assert!(errors.is_empty(), "{file} schema errors: {errors:#?}");
        let record = parse_any(kind(&entry["kind"]), &value)
            .unwrap_or_else(|e| panic!("{file} rejected by Rust: {e}"));
        let round_trip = record.to_value();
        assert_eq!(round_trip, value, "{file} lost or changed fields");
        let bytes = canonical_bytes(&round_trip).unwrap();
        let reparsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(reparsed, value, "{file} canonical bytes changed meaning");
        kinds.insert(entry["kind"].as_str().unwrap().to_owned());
    }
    for required in [
        "source",
        "attachment",
        "project",
        "memory",
        "candidate",
        "review",
        "identity",
        "session",
        "session_event",
        "checkpoint",
        "commit",
        "tombstone",
        "purge_receipt",
        "audit_event",
        "capsule",
        "inspection",
        "dispatch",
        "provider_capabilities",
        "approval",
        "policy",
    ] {
        assert!(kinds.contains(required), "no valid fixture for {required}");
    }
}

#[test]
fn all_five_memory_types_have_valid_fixtures() {
    let manifest = fixture("records-manifest.json");
    let mut types = std::collections::BTreeSet::new();
    for entry in manifest["valid"].as_array().unwrap() {
        if entry["kind"] == "memory" {
            let value = fixture(entry["file"].as_str().unwrap());
            types.insert(value["type"].as_str().unwrap().to_owned());
        }
    }
    let expected: std::collections::BTreeSet<String> = [
        "fact",
        "preference",
        "episode",
        "project_state",
        "session_checkpoint",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();
    assert_eq!(types, expected);
}

#[test]
fn invalid_mutations_are_rejected_with_the_named_rule() {
    let store = SchemaStore::load();
    let manifest = fixture("records-manifest.json");
    let schema_of = |base: &str| {
        manifest["valid"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["file"] == base)
            .unwrap_or_else(|| panic!("no base {base}"))
            .clone()
    };
    let mut schema_blind = 0;
    for case in manifest["invalid"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let base = schema_of(case["base"].as_str().unwrap());
        let mut value = fixture(case["base"].as_str().unwrap());
        apply_ops(&mut value, &case["ops"]);
        let errors = store.validate(base["schema"].as_str().unwrap(), &value);
        match case["schema"].as_str().unwrap() {
            "reject" => assert!(!errors.is_empty(), "{id}: schema unexpectedly accepted"),
            "accept" => {
                assert!(
                    errors.is_empty(),
                    "{id}: schema rejected ({errors:?}); mark it reject"
                );
                schema_blind += 1;
            }
            other => panic!("{id}: bad expectation {other}"),
        }
        let rule = case["rust_rule"].as_str().unwrap();
        match parse_any(kind(&base["kind"]), &value) {
            Ok(_) => panic!("{id}: Rust accepted an invalid record"),
            Err(error) => assert!(
                error.rules().contains(&rule),
                "{id}: expected {rule}, got {error}"
            ),
        }
    }
    assert!(
        schema_blind >= 20,
        "schema-blind constraints: {schema_blind}"
    );
}

#[test]
fn unknown_major_versions_are_read_only_and_never_rewritten() {
    let mut value = fixture("records/memory-fact.json");
    assert_eq!(schema_disposition(&value), SchemaDisposition::Supported);
    value["schema_version"] = json!(2);
    assert_eq!(
        schema_disposition(&value),
        SchemaDisposition::UnknownMajorReadOnly(2)
    );
    assert!(ensure_writable(&value).is_err());
    value.as_object_mut().unwrap().remove("schema_version");
    assert_eq!(schema_disposition(&value), SchemaDisposition::Malformed);
    let error = parse_any(RecordKind::Memory, &value).unwrap_err();
    assert_eq!(error.memory_code(), MemoryErrorCode::UnsupportedSchema);
}

#[test]
fn unknown_non_security_extensions_round_trip_verbatim() {
    let value = fixture("records/attachment-external.json");
    let record = parse_any(RecordKind::Attachment, &value).unwrap();
    assert_eq!(
        record.to_value()["extensions"]["org.example.import"]["note"],
        "kept verbatim"
    );
}

#[test]
fn canonical_bytes_are_sorted_two_space_lf_with_trailing_newline() {
    let value = fixture("records/source-export-user.json");
    let record = parse_any(RecordKind::Source, &value).unwrap();
    let bytes = canonical_bytes(&record.to_value()).unwrap();
    let text = String::from_utf8(bytes).unwrap();
    assert!(text.ends_with("}\n") && !text.contains('\r'));
    assert!(text.starts_with("{\n  \"access_policy_id\""));
    let on_disk =
        std::fs::read_to_string(support::fixtures_root().join("records/source-export-user.json"))
            .unwrap();
    assert_eq!(text, on_disk, "fixture bytes are not canonical");
}

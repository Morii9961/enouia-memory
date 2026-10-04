//! Workspace IPC v1 (ADR-MEM-44): schema and Rust agree on every command's
//! request, the response pairing, and the negative cases; the page cannot
//! name a path, a shell command, or an unknown command.

mod support;

use enouia_memory_contract::workspace::{
    COMMANDS, HostSurface, is_long_running, is_write, parse_request, success_kind,
    validate_response,
};
use serde_json::Value;
use support::schema::SchemaStore;
use support::{apply_ops, fixture};

const SCHEMA: &str = "ipc/workspace-v1.schema.json";

#[test]
fn every_command_has_a_valid_request_that_round_trips() {
    let store = SchemaStore::load();
    let manifest = fixture("workspace-manifest.json");
    let mut seen = Vec::new();
    for request in manifest["valid_requests"].as_array().unwrap() {
        let name = request["command"].as_str().unwrap();
        let errors = store.validate(SCHEMA, request);
        assert!(errors.is_empty(), "{name}: {errors:#?}");
        let (parsed, command) = parse_request(request).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), *request);
        assert_eq!(command.name(), name);
        let back = serde_json::to_value(&command).unwrap();
        assert_eq!(back["arguments"], request["arguments"], "{name}");
        seen.push(name.to_owned());
    }
    assert_eq!(seen, COMMANDS, "one request per command, in schema order");
}

#[test]
fn invalid_requests_are_rejected_by_rust_and_as_declared_by_schema() {
    let store = SchemaStore::load();
    let manifest = fixture("workspace-manifest.json");
    let requests = manifest["valid_requests"].as_array().unwrap();
    for case in manifest["invalid_requests"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let mut value = requests[case["base"].as_u64().unwrap() as usize].clone();
        apply_ops(&mut value, &case["ops"]);
        let errors = store.validate(SCHEMA, &value);
        assert_eq!(
            errors.is_empty(),
            case["schema"] == "accept",
            "{id}: {errors:?}"
        );
        let error = parse_request(&value).expect_err(id);
        let rule = case["rust_rule"].as_str().unwrap();
        assert!(
            error.rules().contains(&rule),
            "{id}: expected {rule}, got {error}"
        );
    }
}

#[test]
fn responses_pair_with_their_command() {
    let store = SchemaStore::load();
    let manifest = fixture("workspace-manifest.json");
    let responses = manifest["valid_responses"].as_array().unwrap();
    for response in responses {
        let errors = store.validate(SCHEMA, &response["message"]);
        assert!(errors.is_empty(), "{}: {errors:#?}", response["command"]);
        validate_response(response["command"].as_str().unwrap(), &response["message"])
            .unwrap_or_else(|e| panic!("{}: {e}", response["command"]));
    }
    for case in manifest["invalid_responses"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let mut value = responses[case["base"].as_u64().unwrap() as usize].clone();
        apply_ops(&mut value, &case["ops"]);
        let errors = store.validate(SCHEMA, &value["message"]);
        assert_eq!(
            errors.is_empty(),
            case["schema"] == "accept",
            "{id}: {errors:?}"
        );
        let error =
            validate_response(value["command"].as_str().unwrap(), &value["message"]).expect_err(id);
        let rule = case["rust_rule"].as_str().unwrap();
        assert!(
            error.rules().contains(&rule),
            "{id}: expected {rule}, got {error}"
        );
    }
}

#[test]
fn schema_and_rust_agree_on_writes_and_long_operations() {
    let schema: Value = serde_json::from_str(
        &std::fs::read_to_string(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../contracts/ipc/workspace-v1.schema.json"),
        )
        .unwrap(),
    )
    .unwrap();
    for name in COMMANDS {
        let key = &schema["$defs"][format!("request_{name}")]["properties"]["idempotencyKey"];
        assert_eq!(key["type"] == "string", is_write(name), "{name}");
        // Long operations never also commit synchronously through the page.
        if is_long_running(name) {
            assert_eq!(
                success_kind(name),
                enouia_memory_contract::workspace::ResponseKind::OperationStarted
            );
        }
        assert_ne!(
            success_kind(name),
            enouia_memory_contract::workspace::ResponseKind::MemoryError,
            "{name}"
        );
    }
    // No command takes a path, SQL, or a caller identity.
    let text = serde_json::to_string(&schema).unwrap().to_lowercase();
    for word in [
        "\"path\"",
        "\"sql\"",
        "\"principal\"",
        "\"actor\"",
        "\"shell\"",
    ] {
        assert!(!text.contains(word), "{word}");
    }
}

/// The host scope (ADR-MEM-45): the workspace page reaches every command and
/// leaves validation to the Core; quick search reaches literal search only,
/// whatever the packet claims about its window or caller.
#[test]
fn host_surfaces_scope_requests_by_native_identity_only() {
    use serde_json::json;
    for name in COMMANDS {
        let request = json!({"command": name});
        assert!(HostSurface::Workspace.allows(&request), "{name}");
        assert_eq!(
            HostSurface::QuickSearch.allows(&request),
            name == "memory_search",
            "{name}"
        );
    }
    for packet in [
        json!(null),
        json!([]),
        json!({}),
        json!({"command": 42}),
        json!({"command": "Memory_search"}),
        json!({"command": "memory_search "}),
        json!({"command": "remember", "window": "main", "principal": "owner"}),
        json!({"command": "remember", "surface": "workspace"}),
    ] {
        assert!(!HostSurface::QuickSearch.allows(&packet), "{packet}");
    }
}

//! Local Memory IPC v1: schema and Rust agree on requests/responses; the
//! operation catalog keeps propose ≠ commit and read ≠ source.

mod support;

use enouia_memory_contract::ipc::{Operation, ResponseKind, parse_request, validate_response};
use enouia_memory_contract::policy::Scope;
use serde_json::Value;
use support::schema::SchemaStore;
use support::{apply_ops, fixture};

const SCHEMA: &str = "ipc/memory-v1.schema.json";

fn operation(value: &Value) -> Operation {
    serde_json::from_value(value.clone()).unwrap()
}

#[test]
fn valid_requests_and_responses_pass_schema_and_rust() {
    let store = SchemaStore::load();
    let manifest = fixture("ipc-manifest.json");
    let requests = manifest["valid_requests"].as_array().unwrap();
    let mut seen = std::collections::BTreeSet::new();
    for request in requests {
        let errors = store.validate(SCHEMA, request);
        assert!(errors.is_empty(), "{}: {errors:#?}", request["operation"]);
        let (parsed, _) =
            parse_request(request).unwrap_or_else(|e| panic!("{}: {e}", request["operation"]));
        assert_eq!(serde_json::to_value(&parsed).unwrap(), *request);
        seen.insert(request["operation"].as_str().unwrap().to_owned());
    }
    assert_eq!(
        seen.len(),
        Operation::ALL.len(),
        "every operation has a request fixture"
    );
    for response in manifest["valid_responses"].as_array().unwrap() {
        let message = &response["message"];
        let errors = store.validate(SCHEMA, message);
        assert!(errors.is_empty(), "{}: {errors:#?}", message["kind"]);
        validate_response(operation(&response["operation"]), message)
            .unwrap_or_else(|e| panic!("{}: {e}", message["kind"]));
    }
}

#[test]
fn invalid_requests_and_responses_are_rejected() {
    let store = SchemaStore::load();
    let manifest = fixture("ipc-manifest.json");
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
    let responses = manifest["valid_responses"].as_array().unwrap();
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
            validate_response(operation(&value["operation"]), &value["message"]).expect_err(id);
        let rule = case["rust_rule"].as_str().unwrap();
        assert!(
            error.rules().contains(&rule),
            "{id}: expected {rule}, got {error}"
        );
    }
}

#[test]
fn propose_never_commits_and_review_is_never_agent_facing() {
    for op in Operation::ALL {
        if matches!(op, Operation::MemoryPropose | Operation::MemoryUpdate) {
            assert_eq!(op.success_kind(), ResponseKind::CandidateProposed);
        }
        if op.success_kind() == ResponseKind::ReviewCommitted {
            assert!(!op.agent_exposable(), "{op:?} exposed to agents");
        }
    }
    assert_eq!(
        Operation::MemoryUpdate.canonical(),
        Operation::MemoryPropose
    );
    assert_eq!(Operation::MemoryRead.required_scope(), Scope::MemoryRead);
    assert_eq!(Operation::MemorySource.required_scope(), Scope::SourceRead);
    assert_ne!(
        Operation::MemoryRead.required_scope(),
        Operation::MemorySource.required_scope()
    );
    let exposed: Vec<Operation> = Operation::ALL
        .into_iter()
        .filter(|o| o.agent_exposable())
        .collect();
    assert_eq!(
        exposed.len(),
        7,
        "six MCP tools plus the memory_update alias"
    );
}

#[test]
fn reserved_operations_are_not_accepted_yet() {
    for name in enouia_memory_contract::ipc::RESERVED_OPERATIONS {
        assert!(serde_json::from_value::<Operation>(Value::String((*name).into())).is_err());
    }
}

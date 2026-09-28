//! Regression tests for the independent MV-0 review findings (F1–F6). Each
//! test reproduces a counterexample from docs/reviews/ and asserts the
//! corrected behavior: rejection, or explicitly preserved uncertainty.

mod support;

use enouia_memory_contract::context::{
    Destination, DestinationKind, DispatchRecord, DispatchTool, MessageRole, OutputConfig,
    request_payload_hash,
};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::provider::{ProviderMessage, ProviderRequest, ToolDefinition};
use enouia_memory_contract::session::ProviderBinding;
use enouia_memory_contract::{RecordKind, parse_any};
use serde_json::{Value, json};
use support::fixture;

fn dispatch_fixture() -> DispatchRecord {
    let set = fixture("sets/morimeta-confirmed.json");
    serde_json::from_value(set["dispatches"][0].clone()).unwrap()
}

fn actual_request(dispatch: &DispatchRecord) -> ProviderRequest {
    let payloads = fixture("expectations/dispatch-payloads.json");
    let payload = &payloads["payloads"][dispatch.dispatch_id.as_str()];
    ProviderRequest {
        dispatch_id: dispatch.dispatch_id.clone(),
        capsule_id: dispatch.capsule_id.clone(),
        destination: serde_json::from_value(payload["destination"].clone()).unwrap(),
        messages: payload["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(|m| ProviderMessage {
                role: serde_json::from_value(m["role"].clone()).unwrap(),
                text: m["text"].as_str().unwrap().to_owned(),
            })
            .collect(),
        tools: Vec::new(),
        output: serde_json::from_value(payload["output"].clone()).unwrap(),
    }
}

fn rules(violations: &[enouia_memory_contract::Violation]) -> Vec<&'static str> {
    violations.iter().map(|v| v.rule).collect()
}

#[test]
fn f3_exact_recorded_request_verifies() {
    let dispatch = dispatch_fixture();
    let request = actual_request(&dispatch);
    assert!(request.verify_against(&dispatch).is_empty());
    assert_eq!(request.payload_hash(), dispatch.request_hash);
}

#[test]
fn f3_same_length_unrelated_payload_is_rejected() {
    let dispatch = dispatch_fixture();
    let mut request = actual_request(&dispatch);
    let bytes = request.messages[1].text.len();
    request.messages[1].text = "X".repeat(bytes);
    assert_eq!(
        request.messages[1].text.len() as u64,
        dispatch.messages[1].size_bytes
    );
    let found = rules(&request.verify_against(&dispatch));
    assert!(
        found.contains(&"provider_request.message_content"),
        "{found:?}"
    );
    assert!(
        found.contains(&"provider_request.payload_hash"),
        "{found:?}"
    );
}

#[test]
fn f3_message_order_output_and_destination_are_bound() {
    let dispatch = dispatch_fixture();
    let mut swapped = actual_request(&dispatch);
    swapped.messages.swap(0, 1);
    assert!(rules(&swapped.verify_against(&dispatch)).contains(&"provider_request.payload_hash"));

    let mut output = actual_request(&dispatch);
    output.output.max_output_tokens += 1;
    assert!(rules(&output.verify_against(&dispatch)).contains(&"provider_request.output"));

    let mut destination = actual_request(&dispatch);
    destination.destination = Destination {
        kind: DestinationKind::ExternalProvider,
        provider_binding: Some(ProviderBinding {
            provider: "example-cloud".into(),
            model: "example-model".into(),
            adapter_version: "0".into(),
        }),
    };
    assert!(
        rules(&destination.verify_against(&dispatch)).contains(&"provider_request.destination")
    );
}

#[test]
fn f3_tool_with_same_name_but_new_definition_is_rejected() {
    let mut dispatch = dispatch_fixture();
    let mut request = actual_request(&dispatch);
    request.tools = vec![ToolDefinition {
        name: "memory_search".into(),
        schema_json: r#"{"type":"object","properties":{"query":{"type":"string"}}}"#.into(),
    }];
    // Record the dispatch for that exact tool, as a compiler would.
    dispatch.tools = vec![DispatchTool {
        name: "memory_search".into(),
        definition_hash: sha256(request.tools[0].schema_json.as_bytes()),
    }];
    let messages: Vec<(MessageRole, _, u64)> = dispatch
        .messages
        .iter()
        .map(|m| (m.role, &m.content_hash, m.size_bytes))
        .collect();
    dispatch.request_hash = request_payload_hash(
        &dispatch.destination,
        &messages,
        &dispatch.tools,
        &dispatch.output,
    );
    assert!(dispatch.validate().is_empty());
    assert!(request.verify_against(&dispatch).is_empty());
    request.tools[0].schema_json =
        r#"{"type":"object","properties":{"query":{"type":"number"}}}"#.into();
    let found = rules(&request.verify_against(&dispatch));
    assert!(found.contains(&"provider_request.tools"), "{found:?}");
    assert!(
        found.contains(&"provider_request.payload_hash"),
        "{found:?}"
    );
}

#[test]
fn f3_dispatch_record_digest_is_self_consistent() {
    let dispatch = dispatch_fixture();
    assert_eq!(dispatch.computed_request_hash(), dispatch.request_hash);
    let output = OutputConfig {
        max_output_tokens: dispatch.output.max_output_tokens + 1,
        streaming: false,
    };
    let mut changed = dispatch.clone();
    changed.output = output;
    assert!(rules(&changed.validate()).contains(&"dispatch.request_hash"));
}

#[test]
fn f6_extreme_budget_is_a_structured_error_not_a_panic() {
    let mut capsule = fixture("records/capsule.json");
    capsule["budget"]["max_tokens"] = json!(u64::MAX);
    capsule["budget"]["estimated_tokens"] = json!(u64::MAX);
    capsule["budget"]["safety_margin_tokens"] = json!(1);
    let result = std::panic::catch_unwind(|| parse_any(RecordKind::Capsule, &capsule));
    let error = result.expect("parser must not panic").unwrap_err();
    assert!(error.rules().contains(&"number.out_of_range"), "{error}");

    // Largest schema-legal values: the sum is checked, not overflowed.
    let limit = 9_007_199_254_740_991_u64;
    capsule["budget"]["max_tokens"] = json!(limit);
    capsule["budget"]["estimated_tokens"] = json!(limit);
    capsule["budget"]["safety_margin_tokens"] = json!(limit);
    let error = parse_any(RecordKind::Capsule, &capsule).unwrap_err();
    assert!(error.rules().contains(&"capsule.budget"), "{error}");
}

#[test]
fn f6_ipc_rejects_out_of_range_integers() {
    let manifest = fixture("ipc-manifest.json");
    let mut request: Value = manifest["valid_requests"][0].clone();
    request["arguments"]["maxTokens"] = json!(u64::MAX);
    let error = enouia_memory_contract::ipc::parse_request(&request).unwrap_err();
    assert!(error.rules().contains(&"number.out_of_range"), "{error}");
}

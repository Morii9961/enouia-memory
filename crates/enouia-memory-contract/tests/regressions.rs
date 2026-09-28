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

fn lifecycle_with_unknown_supersession_start() -> (Value, String, String) {
    let mut set = fixture("sets/lifecycle.json");
    let memories = set["memories"].as_array().unwrap().clone();
    let (index, new) = memories
        .iter()
        .enumerate()
        .find(|(_, m)| !m["supersedes"].as_array().unwrap().is_empty())
        .unwrap();
    let old_id = new["supersedes"][0]["memory_id"]
        .as_str()
        .unwrap()
        .to_owned();
    let new_id = new["memory_id"].as_str().unwrap().to_owned();
    let review_id = new["review_id"].clone();
    set["memories"][index]["supersedes"][0]["effective_from"] = json!("unknown");
    set["memories"][index]["valid_from"] = Value::Null;
    for review in set["reviews"].as_array_mut().unwrap() {
        if review["review_id"] == review_id {
            review["effective_from"] = json!("unknown");
        }
    }
    (set, old_id, new_id)
}

#[test]
fn f5_unknown_supersession_start_is_never_replaced_by_approval_time() {
    use enouia_memory_contract::context::Currency;
    use enouia_memory_contract::set::{RecordSet, validate_set};
    use enouia_memory_contract::temporal::{Effect, currency, effect};
    use enouia_memory_contract::time::Timestamp;

    let (value, old_id, new_id) = lifecycle_with_unknown_supersession_start();
    let set = RecordSet::from_value(&value).unwrap_or_else(|(p, e)| panic!("{p}: {e}"));
    let violations = validate_set(&set);
    assert!(
        violations
            .iter()
            .all(|v| v.rule != "supersession.effective_mismatch"),
        "{violations:?}"
    );
    let latest = set.latest_memories();
    let old = latest
        .iter()
        .find(|m| m.memory_id.as_str() == old_id)
        .unwrap();
    let new = latest
        .iter()
        .find(|m| m.memory_id.as_str() == new_id)
        .unwrap();
    // Approved 2026-09-21; the review's counterexample queried 2026-09-28.
    assert!(new.approved_at.as_str() < "2026-09-28");
    let now = Timestamp::parse("2026-09-28T08:00:00.000Z").unwrap();
    for as_of in [
        "2026-09-25T00:00:00.000Z",
        "2026-09-28T08:00:00.000Z",
        "2030-01-01T00:00:00.000Z",
    ] {
        let as_of = Timestamp::parse(as_of).unwrap();
        assert_eq!(
            effect(old, &latest, &as_of),
            Effect::InEffect {
                supersession_time_unknown: true
            },
            "old fact must not be cut off at {as_of}"
        );
        assert_eq!(
            currency(old, &latest, &as_of, &now),
            Some(Currency::NeedsReverification)
        );
        assert_eq!(
            currency(new, &latest, &as_of, &now),
            Some(Currency::NeedsReverification),
            "new fact must not become current_supported at {as_of}"
        );
    }
}

#[test]
fn f5_effective_time_must_match_the_owner_confirmation() {
    use enouia_memory_contract::set::{RecordSet, validate_set};
    let (mut value, _, _) = lifecycle_with_unknown_supersession_start();
    // Fill in a date the owner never confirmed.
    for memory in value["memories"].as_array_mut().unwrap() {
        if let Some(edge) = memory["supersedes"]
            .as_array_mut()
            .and_then(|e| e.first_mut())
        {
            edge["effective_from"] = json!("2026-09-21T10:01:00.000Z");
        }
    }
    let set = RecordSet::from_value(&value).unwrap();
    let rules: Vec<&str> = validate_set(&set).iter().map(|v| v.rule).collect();
    assert!(
        rules.contains(&"supersession.effective_mismatch"),
        "{rules:?}"
    );
}

fn set_rules(value: &Value) -> Vec<&'static str> {
    use enouia_memory_contract::set::{RecordSet, validate_set};
    let set = RecordSet::from_value(value).unwrap_or_else(|(p, e)| panic!("{p}: {e}"));
    validate_set(&set).iter().map(|v| v.rule).collect()
}

fn find_index(value: &Value, key: &str, id_field: &str, id: &str) -> usize {
    value[key]
        .as_array()
        .unwrap()
        .iter()
        .position(|r| r[id_field] == id)
        .unwrap_or_else(|| panic!("{id} not in {key}"))
}

#[test]
fn f1_valid_external_egress_set_is_consistent() {
    assert!(set_rules(&fixture("sets/morimeta-external-egress.json")).is_empty());
}

#[test]
fn f1_ordinary_accept_review_is_not_egress_consent() {
    use enouia_memory_contract::set::RecordSet;
    let mut value = fixture("sets/morimeta-external-egress.json");
    let accept_review = value["reviews"][0]["review_id"].clone();
    assert_eq!(value["reviews"][0]["action"], "accept");
    let index = find_index(
        &value,
        "dispatches",
        "dispatch_id",
        "dsp_00000002-0000-4000-8000-000000000002",
    );
    value["dispatches"][index]["egress"]["egress_approval_id"] = accept_review;
    // The field only accepts ApprovalRecord IDs: a review cannot even be named.
    let error = RecordSet::from_value(&value).unwrap_err().1;
    assert!(error.rules().contains(&"shape"), "{error}");
    // And without an approval, private egress is refused.
    value["dispatches"][index]["egress"]["egress_approval_id"] = Value::Null;
    assert!(set_rules(&value).contains(&"egress.approval_missing"));
}

#[test]
fn f2_ordinary_accept_review_is_not_delete_confirmation() {
    let mut value = fixture("sets/lifecycle.json");
    let accept = value["reviews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["action"] == "accept")
        .unwrap()["review_id"]
        .clone();
    value["tombstones"][0]["review_id"] = accept;
    assert!(set_rules(&value).contains(&"tombstone.review_action"));
}

#[test]
fn f2_ordinary_accept_review_is_not_declassification() {
    use enouia_memory_contract::set::RecordSet;
    let mut value = fixture("sets/lifecycle.json");
    let index = value["memories"]
        .as_array()
        .unwrap()
        .iter()
        .position(|m| !m["declassification_approval_id"].is_null())
        .unwrap();
    assert_eq!(value["memories"][index]["sensitivity"], "normal");
    let own_review = value["memories"][index]["review_id"].clone();
    value["memories"][index]["declassification_approval_id"] = own_review;
    let error = RecordSet::from_value(&value).unwrap_err().1;
    assert!(error.rules().contains(&"shape"), "{error}");
    value["memories"][index]["declassification_approval_id"] = Value::Null;
    assert!(set_rules(&value).contains(&"sensitivity.downgrade"));
}

#[test]
fn f4_same_principal_and_sensitivity_differ_by_project_and_provider() {
    use enouia_memory_contract::common::{ActorRef, ActorType, Sensitivity};
    use enouia_memory_contract::context::Purpose;
    use enouia_memory_contract::ids::{PrincipalId, ProjectId};
    use enouia_memory_contract::json::Revision;
    use enouia_memory_contract::policy::{
        AccessContext, PolicyRecord, ResourceContext, Scope, evaluate,
    };
    use enouia_memory_contract::record::RecordRef;
    use enouia_memory_contract::time::Timestamp;

    let grant: PolicyRecord = serde_json::from_value(fixture("records/policy-grant.json")).unwrap();
    let default: PolicyRecord =
        serde_json::from_value(fixture("records/policy-default.json")).unwrap();
    let policies = [&default, &grant];
    let owner = ActorRef {
        actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
        actor_type: ActorType::Owner,
    };
    let at = Timestamp::parse("2026-07-04T01:00:00.000Z").unwrap();
    let rev = Revision::new(1).unwrap();
    let resource = |project: &str| ResourceContext {
        record: RecordRef::new(
            RecordKind::Memory,
            "mem_00000002-0000-4000-8000-000000000002",
            rev,
        ),
        project_id: Some(ProjectId::parse(project).unwrap()),
        sensitivity: Sensitivity::Private,
    };
    let morimeta = resource("prj_00000001-0000-4000-8000-000000000001");
    let moriium = resource("prj_00000002-0000-4000-8000-000000000002");
    let granted = Destination {
        kind: DestinationKind::ExternalProvider,
        provider_binding: Some(ProviderBinding {
            provider: "example-cloud".into(),
            model: "example-model".into(),
            adapter_version: "0".into(),
        }),
    };
    let mut other_provider = granted.clone();
    other_provider.provider_binding.as_mut().unwrap().provider = "other-cloud".into();
    let ask = |resource: &ResourceContext, destination: &Destination| {
        evaluate(
            &policies,
            &AccessContext {
                principal: &owner,
                scope: Scope::ProviderSend,
                purpose: Some(Purpose::Answer),
                destination: Some(destination),
                resource,
            },
            &at,
        )
    };
    assert!(ask(&morimeta, &granted).is_allow());
    assert!(
        !ask(&moriium, &granted).is_allow(),
        "other project, same principal/sensitivity"
    );
    assert!(
        !ask(&morimeta, &other_provider).is_allow(),
        "provider switch needs a new grant"
    );
    let mut highly = morimeta.clone();
    highly.sensitivity = Sensitivity::HighlySensitive;
    assert!(
        !ask(&highly, &granted).is_allow(),
        "highly sensitive never leaves automatically"
    );
    let mut revoked = grant.clone();
    revoked.status = enouia_memory_contract::policy::PolicyStatus::Revoked;
    revoked.revoked_at = Some(Timestamp::parse("2026-07-02T00:00:00.000Z").unwrap());
    revoked.revision = Revision::new(2).unwrap();
    let with_revocation = [&default, &grant, &revoked];
    let decision = evaluate(
        &with_revocation,
        &AccessContext {
            principal: &owner,
            scope: Scope::ProviderSend,
            purpose: Some(Purpose::Answer),
            destination: Some(&granted),
            resource: &morimeta,
        },
        &at,
    );
    assert!(!decision.is_allow(), "latest revision (revoked) wins");
}

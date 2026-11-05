#![cfg(windows)]
#[path = "support/agent_correction_fixture.rs"]
mod fixture;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod schema_support;
use enouia_memory_contract::{common::*, ids::*, next::*, time::Timestamp};
use enouia_memory_host::{
    automation::qualified_agent::QualifiedAgentHost,
    mcp::{initialized, opening},
};
use fixture::*;
use serde_json::{Value, json};

fn initialization() -> [Value; 2] {
    [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-tool-links","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ]
}
fn packet(n: u32, operation: &str, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":n,"method":"tools/call","params":{"name":operation,"arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":initialized::CAPABILITY,"request_id":id("req",300+n),"operation":operation,"arguments":arguments}}})
}
fn read_args(memory: &MemoryId, source: bool) -> Value {
    json!({"memory_id":memory,"expected":null,"history_cursor":null,"include_source_text":source,"max_bytes":65536})
}
fn recall_args(max_bytes: u32) -> Value {
    json!({"text":"","conditions":[],"event_window":null,"at":Timestamp::from_unix_ms(NOW).unwrap(),"intent":"browse","limit":1,"max_bytes":max_bytes})
}
fn check(response: &Value) {
    let errors = schema_support::schema::SchemaStore::load()
        .validate("next/mcp-context-tool-links-v1.schema.json", response);
    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(
        serde_json::from_str::<Value>(response["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap(),
        response["result"]["structuredContent"]
    );
}

#[test]
fn linked_reads_preserve_native_data_use_returned_ids_and_leave_legacy_tools_text_only() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let mut scope = scope(&p);
    scope.source_read = false;
    let reader = host
        .reader(&e.descriptor.created_by, actor(ActorType::Agent, 88), scope)
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    for message in initialization() {
        server.handle(&message);
    }
    let before = v.snapshot().unwrap();
    let read = packet(
        2,
        "memory_read",
        read_args(&request.target.memory.memory_id, false),
    );
    let result = server.handle(&read).unwrap();
    check(&result);
    let body = &result["result"]["structuredContent"];
    assert_eq!(result["id"], read["id"]);
    assert_eq!(
        body["request_id"],
        read["params"]["arguments"]["request_id"]
    );
    assert_eq!(body["data"]["source_read"], false);
    assert_eq!(
        body["data"],
        serde_json::to_value(
            reader
                .detail_page(
                    &profile,
                    &request.target.memory.memory_id,
                    None,
                    false,
                    65536,
                    None
                )
                .unwrap()
        )
        .unwrap()
    );
    let link = &result["result"]["content"][1];
    assert_eq!(
        *link,
        json!({"type":"resource_link","uri":format!("enouia-memory://memory/{}",request.target.memory.memory_id.as_str()),"name":"memory_detail","mimeType":"application/json"})
    );
    let resource = server
        .handle(
            &json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":link["uri"]}}),
        )
        .unwrap();
    let resource: Value =
        serde_json::from_str(resource["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(resource["data"], body["data"]);
    let recall = packet(4, "memory_recall", recall_args(16384));
    let recalled = server.handle(&recall).unwrap();
    check(&recalled);
    let recalled_body = &recalled["result"]["structuredContent"];
    let recall_link = &recalled["result"]["content"][1];
    assert_eq!(
        recall_link["uri"],
        format!(
            "enouia-memory://recall/{}",
            recalled_body["request_id"].as_str().unwrap()
        )
    );
    let actual=server.handle(&json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":recall_link["uri"]}})).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(actual["result"]["contents"][0]["text"].as_str().unwrap())
            .unwrap()["data"],
        recalled_body["data"]
    );
    let bounded = server
        .handle(&packet(6, "memory_recall", recall_args(1)))
        .unwrap();
    check(&bounded);
    assert_eq!(
        bounded["result"]["structuredContent"]["data"]["items"],
        json!([])
    );
    assert_eq!(
        bounded["result"]["structuredContent"]["data"]["complete_in_scope"],
        false
    );
    assert_eq!(bounded["result"]["content"].as_array().unwrap().len(), 2);
    let mut legacy = initialized::Server::new(&reader, &profile).unwrap();
    for message in initialization() {
        legacy.handle(&message);
    }
    for call in [read, recall] {
        assert_eq!(
            legacy.handle(&call).unwrap()["result"]["content"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
    }
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn memory_link_keeps_current_resource_recipe_without_copying_tool_source_text() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let reader = host
        .reader(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    for message in initialization() {
        server.handle(&message);
    }
    let before = v.snapshot().unwrap();
    let original = server
        .handle(&packet(
            2,
            "memory_read",
            read_args(&request.target.memory.memory_id, true),
        ))
        .unwrap();
    check(&original);
    let body = &original["result"]["structuredContent"]["data"];
    assert_eq!(
        *body,
        serde_json::to_value(
            reader
                .detail_page(
                    &profile,
                    &request.target.memory.memory_id,
                    None,
                    true,
                    65536,
                    None
                )
                .unwrap()
        )
        .unwrap()
    );
    let linked=server.handle(&json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":original["result"]["content"][1]["uri"]}})).unwrap();
    let current: Value =
        serde_json::from_str(linked["result"]["contents"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(
        current["data"],
        serde_json::to_value(
            reader
                .detail_page(
                    &profile,
                    &request.target.memory.memory_id,
                    None,
                    false,
                    65536,
                    None
                )
                .unwrap()
        )
        .unwrap()
    );
    assert_ne!(current["data"], *body);
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn missing_and_denied_reads_emit_no_link_and_failed_recall_preserves_actual_cache() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let mut scope = scope(&p);
    scope.source_read = false;
    let reader = host
        .reader(&e.descriptor.created_by, actor(ActorType::Agent, 88), scope)
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    for message in initialization() {
        server.handle(&message);
    }
    let original = server
        .handle(&packet(2, "memory_recall", recall_args(16384)))
        .unwrap();
    let request_id = RequestId::parse(
        original["result"]["structuredContent"]["request_id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    let before = v.snapshot().unwrap();
    let missing = server
        .handle(&packet(
            3,
            "memory_read",
            read_args(&MemoryId::parse(&id("mem", 999)).unwrap(), false),
        ))
        .unwrap();
    let denied = server
        .handle(&packet(
            4,
            "memory_read",
            read_args(&request.target.memory.memory_id, true),
        ))
        .unwrap();
    let invalid = server
        .handle(&packet(5, "memory_recall", recall_args(0)))
        .unwrap();
    for response in [&missing, &denied, &invalid] {
        check(response);
        assert_eq!(response["result"]["content"].as_array().unwrap().len(), 1);
        assert!(response["result"]["structuredContent"]["data"].is_null());
    }
    assert_eq!(missing["result"]["isError"], false);
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        "permission_denied"
    );
    assert_eq!(invalid["result"]["isError"], true);
    assert_eq!(
        reader.context_trace(&request_id).unwrap().unwrap(),
        original["result"]["structuredContent"]["data"]
    );
    assert_eq!(v.snapshot().unwrap(), before);
}

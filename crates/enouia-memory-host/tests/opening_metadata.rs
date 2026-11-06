#![cfg(windows)]
#[path = "support/agent_correction_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, next::*, time::Timestamp};
use enouia_memory_host::{
    automation::qualified_agent::QualifiedAgentHost,
    mcp::{initialized, opening},
};
use fixture::*;
use serde_json::{Value, json};

fn initialize() -> Value {
    json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-metadata-check","version":"1"}}})
}
fn recall(n: u32) -> Value {
    json!({"jsonrpc":"2.0","id":n,"method":"tools/call","params":{"name":"memory_recall","arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":initialized::CAPABILITY,"request_id":id("req",400+n),"operation":"memory_recall","arguments":{"text":"","conditions":[],"event_window":null,"at":Timestamp::from_unix_ms(NOW).unwrap(),"intent":"browse","limit":1,"max_bytes":16384}}}})
}

#[test]
fn malformed_metadata_cannot_initialize_or_dispatch_and_valid_objects_stay_opaque() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
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
    let bad = [
        json!(null),
        json!(false),
        json!(7),
        json!("opaque"),
        json!([]),
    ];
    let before = v.snapshot().unwrap();
    let mut admitted = Vec::new();
    for (n, metadata) in bad.iter().enumerate() {
        let mut server = opening::Server::new(&reader, &profile).unwrap();
        let mut message = initialize();
        message["params"]["_meta"] = metadata.clone();
        if server.handle(&message).unwrap()["error"]["code"] != json!(-32602) {
            admitted.push(format!("initialize/{n}"));
        }
        let mut valid = initialize();
        valid["id"] = json!(2);
        assert!(server.handle(&valid).unwrap().get("result").is_some());
        let mut server = opening::Server::new(&reader, &profile).unwrap();
        server.handle(&initialize());
        server.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized","params":{"_meta":metadata}}));
        let result = server
            .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .unwrap();
        if result["error"]["message"] != "not_initialized" {
            admitted.push(format!("notification/{n}"));
        }
        server.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        assert!(
            server
                .handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}))
                .unwrap()
                .get("result")
                .is_some()
        );
    }
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    server.handle(&initialize());
    server.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    let original = server.handle(&recall(2)).unwrap();
    for (n, metadata) in bad.iter().enumerate() {
        for (method_offset, method) in ["tools/call", "tools/list", "ping"].into_iter().enumerate()
        {
            let mut message = recall(10 + (n * 3 + method_offset) as u32);
            message["method"] = json!(method);
            if method != "tools/call" {
                message["params"] = json!({});
            }
            message["params"]["_meta"] = metadata.clone();
            if server.handle(&message).unwrap()["error"]["code"] != json!(-32602) {
                admitted.push(format!("{method}/{n}"));
            }
        }
    }
    assert!(
        admitted.is_empty(),
        "non-object metadata admitted: {admitted:?}"
    );
    let request = RequestId::parse(
        original["result"]["structuredContent"]["request_id"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        reader.context_trace(&request).unwrap().unwrap(),
        original["result"]["structuredContent"]["data"]
    );
    let mut claimed = recall(40);
    claimed["params"]["_meta"] =
        json!({"principal":"owner","scopes":["source_read"],"trace":["opaque",{"keep":true}]});
    let actual = server.handle(&claimed).unwrap();
    assert!(actual["result"]["structuredContent"]["error"].is_null());
    let denied=server.handle(&json!({"jsonrpc":"2.0","id":41,"method":"tools/call","params":{"name":"memory_source","arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":initialized::CAPABILITY,"request_id":id("req",441),"operation":"memory_source","arguments":{"source_id":id("src",8),"revision":null,"from_byte":0,"to_byte":null,"max_bytes":4096}},"_meta":claimed["params"]["_meta"]}})).unwrap();
    assert_eq!(
        denied["result"]["structuredContent"]["error"]["code"],
        "permission_denied"
    );
    assert_eq!(v.snapshot().unwrap(), before);
}

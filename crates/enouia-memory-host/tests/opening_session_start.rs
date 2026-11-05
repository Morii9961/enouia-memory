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
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use fixture::*;
use serde_json::{Value, json};
fn initialize(server: &mut opening::Server<'_, '_>) {
    assert!(server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-session-agent","version":"1"}}})).unwrap().get("result").is_some());
    assert!(
        server
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none()
    );
}
fn call() -> Value {
    json!({"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"session_start","arguments":{"schema_version":2,"kind":"agent_session_start_request","capability":opening::SESSION_CAPABILITY,"request":{"request_id":id("req",160),"at":Timestamp::from_unix_ms(NOW).unwrap()}}}})
}
#[test]
fn session_tool_advertises_closed_schemas_and_returns_exact_actual_context_without_session_writes()
{
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
    let before = v.snapshot().unwrap();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    initialize(&mut server);
    let catalog = server
        .handle(&json!({"jsonrpc":"2.0","id":3,"method":"tools/list"}))
        .unwrap();
    let tools = catalog["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    let tool = tools.iter().find(|t| t["name"] == "session_start").unwrap();
    assert_eq!(
        tool["annotations"],
        json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
    );
    assert_eq!(tool["inputSchema"], opening::session_schema(false));
    assert_eq!(tool["outputSchema"], opening::session_schema(true));
    let message = call();
    let result = server.handle(&message).unwrap();
    let packet = &result["result"]["structuredContent"];
    assert_eq!(result["result"]["isError"], false);
    assert_eq!(result["result"]["content"].as_array().unwrap().len(), 2);
    assert_eq!(
        result["result"]["content"][1],
        json!({"type":"resource_link","uri":packet["context"]["uri"],"name":"actual_recall","mimeType":"application/json"})
    );
    assert_eq!(packet["kind"], "agent_session_start_response");
    assert_eq!(
        serde_json::from_str::<Value>(result["result"]["content"][0]["text"].as_str().unwrap())
            .unwrap(),
        *packet
    );
    let schemas = schema_support::schema::SchemaStore::load();
    assert!(
        schemas
            .validate("next/mcp-session-start-v1.schema.json", packet)
            .is_empty()
    );
    let request = RequestId::parse(&id("req", 160)).unwrap();
    assert_eq!(
        reader.context_trace(&request).unwrap().unwrap(),
        packet["context"]["data"]
    );
    let resource=server.handle(&json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":packet["context"]["uri"]}})).unwrap();
    assert_eq!(
        serde_json::from_str::<Value>(resource["result"]["contents"][0]["text"].as_str().unwrap())
            .unwrap(),
        packet["context"]
    );
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.memories.len(), 1);
        assert!(d.owner_actions.is_empty());
    })
    .unwrap();
    let mut old = initialized::Server::new(&reader, &profile).unwrap();
    old.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}}));
    old.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    assert_eq!(
        old.handle(&message).unwrap()["error"]["message"],
        "unknown_tool"
    );
}
#[test]
fn session_tool_refuses_identity_effect_capture_and_invalid_input_without_overwriting_context() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
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
    initialize(&mut server);
    let initial = server.handle(&call()).unwrap();
    let before = v.snapshot().unwrap();
    for n in 0..12 {
        let mut message = call();
        message["id"] = json!(10 + n);
        match n {
            0 => message["params"]["arguments"]["request"]["principal"] = json!("owner"),
            1 => message["params"]["arguments"]["request"]["session_id"] = json!(id("ses", 1)),
            2 => message["params"]["arguments"]["request"]["capture"] = json!(true),
            3 => message["params"]["arguments"]["request"]["persist"] = json!(true),
            4 => message["params"]["arguments"]["capability"] = json!(initialized::CAPABILITY),
            5 => message["params"]["arguments"]["schema_version"] = json!(1),
            6 => message["params"]["arguments"]["kind"] = json!("agent_correction_request"),
            7 => message["params"]["arguments"]["request"]["request_id"] = json!("bad"),
            8 => {
                message["params"]["arguments"]["request"]["at"] = json!("2026-02-30T00:00:00.000Z")
            }
            9 => message["params"]["arguments"]["request"]["topic"] = json!("界".repeat(700)),
            10 => message["params"]["_meta"] = json!(null),
            _ => {
                message["params"]["arguments"]["kind"] = json!({"agent_session_start_request":null})
            }
        }
        assert_eq!(
            server.handle(&message).unwrap()["error"]["code"],
            -32602,
            "{n}"
        );
    }
    assert_eq!(
        reader
            .context_trace(&RequestId::parse(&id("req", 160)).unwrap())
            .unwrap()
            .unwrap(),
        initial["result"]["structuredContent"]["context"]["data"]
    );
    assert_eq!(v.snapshot().unwrap(), before);
}
#[test]
fn session_tool_obeys_native_attention_budget_and_unknown_retains_full_first_packet() {
    for unknown in [false, true] {
        let faults = Faults::armed();
        let (e, v, p, request) = correction_rig_with(SpeakerRole::User, faults.clone());
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let mut profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        profile.max_bytes = 1;
        let mut server = opening::Server::new(&reader, &profile).unwrap();
        initialize(&mut server);
        let mut message = call();
        message["params"]["_meta"] = json!({"trace":["original",{"all":true}]});
        if unknown {
            let writer = host
                .correction_reporter(&e.descriptor.created_by, principal, scope(&p))
                .unwrap();
            faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(118));
            assert!(writer.submit(&request).is_err());
            assert!(server.handle(&message).is_none());
            assert!(server.closed() && server.outcome_unknown());
            assert_eq!(server.original_unknown(), Some(&message));
            let mut changed = message.clone();
            changed["id"] = json!("replacement");
            assert!(server.handle(&changed).is_none());
            assert_eq!(server.original_unknown(), Some(&message));
        } else {
            let before = v.snapshot().unwrap();
            let result = server.handle(&message).unwrap();
            let data = &result["result"]["structuredContent"]["context"]["data"];
            assert_eq!(data["items"], json!([]));
            assert_eq!(data["complete_in_scope"], false);
            assert_eq!(v.snapshot().unwrap(), before);
        }
    }
}

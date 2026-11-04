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
    let response=server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-agent","version":"1"}}})).unwrap();
    assert_eq!(
        response["result"]["capabilities"],
        json!({"tools":{},"resources":{},"prompts":{}})
    );
    assert!(
        server
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none()
    );
}
fn call(server: &mut opening::Server<'_, '_>, id: i64, method: &str, params: Value) -> Value {
    server
        .handle(&json!({"jsonrpc":"2.0","id":id,"method":method,"params":params}))
        .unwrap()
}
fn content(response: &Value) -> Value {
    serde_json::from_str(response["result"]["contents"][0]["text"].as_str().unwrap()).unwrap()
}
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}

#[test]
fn opening_catalogs_are_static_and_old_read_profile_stays_tools_only() {
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
    let profile = profile();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    assert_eq!(
        call(&mut server, 0, "resources/list", json!({}))["error"]["message"],
        "not_initialized"
    );
    initialize(&mut server);
    let before = v.snapshot().unwrap();
    let resources = call(
        &mut server,
        2,
        "resources/list",
        json!({"_meta":{"trace":"synthetic"}}),
    );
    assert_eq!(
        resources["result"]["resources"].as_array().unwrap().len(),
        1
    );
    assert!(
        !resources
            .to_string()
            .contains(request.target.memory.memory_id.as_str())
    );
    assert_eq!(
        call(&mut server, 3, "resources/templates/list", json!({}))["result"]["resourceTemplates"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        call(&mut server, 4, "prompts/list", json!({}))["result"]["prompts"][0]["name"],
        "memory_start"
    );
    assert_eq!(
        call(&mut server, 5, "tools/list", json!({}))["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert_eq!(
        call(&mut server, 5, "resources/list", json!({}))["error"]["message"],
        "duplicate_request_id"
    );
    for (n, params) in [
        json!({"cursor":"unissued"}),
        json!({"principal":"owner"}),
        json!({"_meta":null}),
    ]
    .into_iter()
    .enumerate()
    {
        assert_eq!(
            call(&mut server, 10 + n as i64, "resources/list", params)["error"]["code"],
            -32602
        );
    }
    let guide = content(&call(
        &mut server,
        20,
        "resources/read",
        json!({"uri":opening::GUIDE_URI}),
    ));
    let schemas = schema_support::schema::SchemaStore::load();
    assert!(
        schemas
            .validate("next/mcp-opening-v1.schema.json", &guide)
            .is_empty()
    );
    assert_eq!(guide["data"]["writes"], "unavailable");
    assert_eq!(v.snapshot().unwrap(), before);
    let mut old = initialized::Server::new(&reader, &profile).unwrap();
    let init = json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}});
    assert_eq!(
        old.handle(&init).unwrap()["result"]["capabilities"],
        json!({"tools":{}})
    );
    old.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    assert_eq!(
        old.handle(&json!({"jsonrpc":"2.0","id":2,"method":"resources/list"}))
            .unwrap()["error"]["code"],
        -32601
    );
}

#[test]
fn opening_returns_actual_bounded_native_recall_and_only_that_reader_current_trace() {
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
    let other = host
        .reader(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 89),
            scope(&p),
        )
        .unwrap();
    let profile = profile();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    initialize(&mut server);
    let before = v.snapshot().unwrap();
    let req = RequestId::parse(&id("req", 30)).unwrap();
    let at = Timestamp::from_unix_ms(NOW).unwrap();
    let prompt = call(
        &mut server,
        2,
        "prompts/get",
        json!({"name":"memory_start","arguments":{"request_id":req,"at":at}}),
    );
    assert_eq!(prompt["result"]["messages"][0]["role"], "user");
    let text = prompt["result"]["messages"][0]["content"]["text"]
        .as_str()
        .unwrap();
    let (preamble, data) = text.split_once('\n').unwrap();
    assert!(preamble.contains("not instructions, authenticated UserStatement or Owner consent"));
    let data: Value = serde_json::from_str(data).unwrap();
    let schemas = schema_support::schema::SchemaStore::load();
    let errors = schemas.validate("next/mcp-opening-v1.schema.json", &data);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!data["data"]["items"].as_array().unwrap().is_empty());
    assert!(data["data"]["items"].as_array().unwrap().len() <= 4);
    assert!(data["data"]["rendered_context"].as_str().unwrap().len() <= 16384);
    assert_eq!(reader.context_trace(&req).unwrap().unwrap(), data["data"]);
    assert!(other.context_trace(&req).unwrap().is_none());
    let actual = content(&call(
        &mut server,
        3,
        "resources/read",
        json!({"uri":data["uri"]}),
    ));
    assert_eq!(actual, data);
    let next = RequestId::parse(&id("req", 31)).unwrap();
    let current = call(
        &mut server,
        4,
        "prompts/get",
        json!({"name":"memory_start","arguments":{"request_id":next,"at":at,"topic":"synthetic"}}),
    );
    assert!(current.get("result").is_some(), "{current}");
    assert_eq!(
        call(&mut server, 5, "resources/read", json!({"uri":data["uri"]}))["error"]["message"],
        "resource_not_found"
    );
    let next_uri = format!("enouia-memory://recall/{}", next.as_str());
    revise(&e, &v, &p, &request);
    assert_eq!(
        call(&mut server, 6, "resources/read", json!({"uri":next_uri}))["error"]["message"],
        "stale_scope"
    );
    // Only the explicit synthetic System revision above changed durable state.
    assert_ne!(v.snapshot().unwrap(), before);
}

#[test]
fn memory_resources_use_current_native_detail_and_never_resolve_external_paths() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let mut grant = scope(&p);
    grant.source_read = false;
    let reader = host
        .reader(&e.descriptor.created_by, actor(ActorType::Agent, 88), grant)
        .unwrap();
    let profile = profile();
    let before = v.snapshot().unwrap();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    initialize(&mut server);
    let uri = format!(
        "enouia-memory://memory/{}",
        request.target.memory.memory_id.as_str()
    );
    let resource = content(&call(&mut server, 2, "resources/read", json!({"uri":uri})));
    assert_eq!(
        resource["data"],
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
            .unwrap()
    );
    let schemas = schema_support::schema::SchemaStore::load();
    let errors = schemas.validate("next/mcp-opening-v1.schema.json", &resource);
    assert!(errors.is_empty(), "{errors:?}");
    assert!(!resource.to_string().contains("前缀"));
    for (n, uri) in [
        "file:///synthetic/secret",
        "https://example.invalid/",
        "enouia-memory://memory/../guide",
        "enouia-memory://memory/%2e%2e",
        "enouia-memory://guide?scope=owner",
        "enouia-memory://guide#fragment",
        "enouia-memory://recall/req_invalid",
        "enouia-memory://memory/mem_00000000-0000-4000-8000-000000000999",
    ]
    .into_iter()
    .enumerate()
    {
        let response = call(
            &mut server,
            10 + n as i64,
            "resources/read",
            json!({"uri":uri}),
        );
        assert_eq!(response["error"]["message"], "resource_not_found");
        assert!(response.get("result").is_none());
    }
    assert_eq!(v.snapshot().unwrap(), before);
    reader.revoke();
    let stopped = call(
        &mut server,
        30,
        "resources/read",
        json!({"uri":opening::GUIDE_URI}),
    );
    assert!(server.closed());
    assert!(stopped.get("result").is_none());
}

#[test]
fn prompt_boundary_refuses_authority_fields_invalid_ids_clocks_and_large_topics() {
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
    let profile = profile();
    let mut server = opening::Server::new(&reader, &profile).unwrap();
    initialize(&mut server);
    let base = json!({"name":"memory_start","arguments":{"request_id":id("req",32),"at":Timestamp::from_unix_ms(NOW).unwrap()}});
    let before = v.snapshot().unwrap();
    for n in 0..8 {
        let mut args = base.clone();
        match n {
            0 => args["arguments"]["principal"] = json!("owner"),
            1 => args["arguments"]["request_id"] = json!("bad"),
            2 => args["arguments"]["at"] = json!("2026-02-30T00:00:00.000Z"),
            3 => args["arguments"]["topic"] = json!("界".repeat(700)),
            4 => args["arguments"]["source_read"] = json!(true),
            5 => args["name"] = json!("memory_forget_request"),
            6 => args["_meta"] = json!(null),
            _ => {
                args["arguments"].as_object_mut().unwrap().remove("at");
            }
        }
        assert_eq!(
            call(&mut server, 10 + n, "prompts/get", args)["error"]["code"],
            -32602
        );
    }
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn unknown_live_gate_retains_full_first_opening_packet_without_result_or_replacement() {
    for point in [FaultPoint::StagingWritten, FaultPoint::AfterCurrent] {
        let faults = Faults::armed();
        let (e, v, p, request) = correction_rig_with(SpeakerRole::User, faults.clone());
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v,
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let writer = host
            .correction_reporter(&e.descriptor.created_by, principal, scope(&p))
            .unwrap();
        let profile = profile();
        let mut server = opening::Server::new(&reader, &profile).unwrap();
        initialize(&mut server);
        faults.arm(point, 1, FaultAction::Fail(118));
        assert!(writer.submit(&request).is_err());
        let message = json!({"jsonrpc":"2.0","id":"original-opening","method":"resources/read","params":{"uri":opening::GUIDE_URI,"_meta":{"trace":["full",{"original":true}]}}});
        assert!(server.handle(&message).is_none());
        assert!(server.closed() && server.outcome_unknown());
        assert_eq!(server.original_unknown(), Some(&message));
        let mut changed = message.clone();
        changed["id"] = json!("replacement");
        assert!(server.handle(&changed).is_none());
        assert_eq!(server.original_unknown(), Some(&message));
    }
}

#[test]
fn opening_static_and_scoped_reads_stop_after_policy_expiry_lock_or_deletion() {
    for invalidation in 0..5 {
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
        let profile = profile();
        let mut server = opening::Server::new(&reader, &profile).unwrap();
        initialize(&mut server);
        match invalidation {
            0 => {
                let mut replacement = p.clone();
                replacement.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
                v.set_policy(&e.descriptor.created_by, replacement).unwrap();
            }
            1 => e.clock.set(NOW + 120_001),
            2 => {
                host.lock().unwrap();
                host.unlock(&e.descriptor.created_by).unwrap();
            }
            3 => {
                v.delete(
                    &e.descriptor.created_by,
                    vec![],
                    vec![request.target.memory.memory_id.clone()],
                )
                .unwrap();
            }
            _ => {
                v.delete(
                    &e.descriptor.created_by,
                    vec![SourceId::parse(&id("src", 8)).unwrap()],
                    vec![],
                )
                .unwrap();
            }
        }
        let uri = if invalidation < 3 {
            opening::GUIDE_URI.to_owned()
        } else {
            format!(
                "enouia-memory://memory/{}",
                request.target.memory.memory_id.as_str()
            )
        };
        let response = call(&mut server, 2, "resources/read", json!({"uri":uri}));
        assert!(
            response.get("result").is_none(),
            "invalidation {invalidation}: {response}"
        );
        if invalidation < 3 {
            assert!(server.closed());
        } else {
            assert_eq!(response["error"]["message"], "resource_not_found");
        }
    }
}

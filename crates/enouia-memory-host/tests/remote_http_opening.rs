#![cfg(windows)]
//! ADR-MEM-129: the actual opening (pipe-10 context) server behind the
//! Streamable HTTP endpoint, on a synthetic initialized root.
#[path = "support/agent_correction_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, next::*, ports::SequentialIdSource, time::Timestamp};
use enouia_memory_host::{
    automation::qualified_agent::QualifiedAgentHost,
    mcp::{opening, remote::*},
};
use fixture::*;
use serde_json::{Value, json};

struct Fixed;
impl Tokens for Fixed {
    fn verify(&self, token: &str) -> Option<Bearer> {
        (token == "tok-owner").then(|| Bearer {
            subject: "owner".into(),
            scopes: vec![READ_SCOPE.into()],
        })
    }
}
fn post(session: Option<&str>, body: Value) -> Request {
    let mut headers = vec![
        ("Content-Type".to_owned(), "application/json".to_owned()),
        (
            "Accept".into(),
            "application/json, text/event-stream".into(),
        ),
        ("Authorization".into(), "Bearer tok-owner".into()),
    ];
    if let Some(session) = session {
        headers.push(("Mcp-Session-Id".into(), session.into()));
        headers.push(("MCP-Protocol-Version".into(), "2025-11-25".into()));
    }
    Request {
        method: "POST".into(),
        target: "/mcp".into(),
        headers,
        body: body.to_string().into_bytes(),
    }
}
fn body(response: &Response) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}

#[test]
fn opening_profile_serves_tools_and_actual_session_start_over_http_without_writes() {
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
    let config = Config::new("https://memory.example/mcp", &["https://auth.example"], &[]).unwrap();
    let ids = SequentialIdSource::new(3);
    let mut endpoint = Endpoint::new(config, &Fixed, &ids, |_: &Bearer| {
        opening::Server::new(&reader, &profile)
    });
    let before = v.snapshot().unwrap();
    let opened = endpoint.respond(&post(None, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-remote-agent","version":"1"}}})));
    assert_eq!(opened.status, 200);
    assert_eq!(
        body(&opened)["result"]["capabilities"],
        json!({"tools":{},"resources":{},"prompts":{}})
    );
    let sid = opened.header_value("mcp-session-id").unwrap().to_owned();
    assert_eq!(
        endpoint
            .respond(&post(
                Some(&sid),
                json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            ))
            .status,
        202
    );
    let tools = body(&endpoint.respond(&post(
        Some(&sid),
        json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}),
    )));
    let names: Vec<&str> = tools["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap())
        .collect();
    assert_eq!(names.len(), 6);
    assert!(names.contains(&"session_start"));
    let request_id = id("req", 161);
    let start = body(&endpoint.respond(&post(Some(&sid), json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"session_start","arguments":{"schema_version":2,"kind":"agent_session_start_request","capability":opening::SESSION_CAPABILITY,"request":{"request_id":request_id,"at":Timestamp::from_unix_ms(NOW).unwrap()}}}}))));
    assert_eq!(start["id"], 3);
    assert_eq!(start["result"]["isError"], false);
    let packet = &start["result"]["structuredContent"];
    assert_eq!(
        (packet["kind"].as_str(), packet["request_id"].as_str()),
        (
            Some("agent_session_start_response"),
            Some(request_id.as_str())
        )
    );
    let refused = body(&endpoint.respond(&post(Some(&sid), json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"session_start","arguments":{"schema_version":2,"kind":"agent_session_start_request","capability":opening::SESSION_CAPABILITY,"request":{"request_id":request_id,"at":Timestamp::from_unix_ms(NOW).unwrap(),"capture":true}}}}))));
    assert_eq!(refused["error"]["code"], -32602);
    assert_eq!(v.snapshot().unwrap(), before);
    let mut delete = post(Some(&sid), json!({}));
    delete.method = "DELETE".into();
    assert_eq!(endpoint.respond(&delete).status, 204);
    assert_eq!(
        endpoint
            .respond(&post(
                Some(&sid),
                json!({"jsonrpc":"2.0","id":5,"method":"tools/list"})
            ))
            .status,
        404
    );
    assert_eq!(endpoint.sessions(), 0);
}

//! ADR-MEM-158 remote write tool around a stub read session; platform-neutral.
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*, foundation::FakeClock, ids::*, next::*, ports::SequentialIdSource,
};
use enouia_memory_host::automation::connector_formation::*;
use enouia_memory_host::mcp::remote::{connector::*, *};
use enouia_memory_vault::fault::Faults;
use fixture::*;
use serde_json::{Value, json};
use std::sync::Arc;

struct Stub;
impl Session for Stub {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        let id = message.get("id")?;
        Some(match message["method"].as_str() {
            Some("initialize") => json!({"jsonrpc":"2.0","id":id,"result":{"capabilities":{}}}),
            Some("tools/list") => {
                json!({"jsonrpc":"2.0","id":id,"result":{"tools":[{"name":"memory_read"}]}})
            }
            _ => json!({"jsonrpc":"2.0","id":id,"error":{"code":-32601,"message":"stub"}}),
        })
    }
    fn closed(&self) -> bool {
        false
    }
    fn unknown(&self) -> Option<&Value> {
        None
    }
}
fn owner_env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    e
}
fn submission() -> Value {
    let v: Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/runtime-owner-capture/cases.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut request = v["base"].clone();
    request["request_id"] = json!(id("req", 7));
    request["conversation_id"] = json!("chatgpt-synthetic");
    request["turns"][0]["message_id"] = json!("m1");
    request["turns"][0]["parent_message_id"] = Value::Null;
    request["turns"][0]["attachments"] = json!([]);
    request["turns"][0]["text"] = json!("I prefer quiet rooms.");
    let unknown = json!({"from":null,"until":null,"precision":"unknown","bound":"within","tz":null,"basis":"unknown"});
    json!({"request": request, "analyst_model": "synthetic-chatgpt-model", "statements": [{
        "message_id": "m1", "quote": "quiet rooms", "title": "quiet rooms",
        "interpretation": "clear_self_report", "kind": "preference",
        "event_time": unknown, "valid_time": unknown, "note": null
    }]})
}
fn call(id: i64, arguments: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":TOOL,"arguments":arguments}})
}
fn init() -> [Value; 2] {
    [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ]
}

#[test]
fn write_tool_needs_write_scope_and_an_initialized_session() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let writer = ConnectorWriter::new(
        v.clone(),
        e.descriptor.created_by.clone(),
        ActorRef {
            actor_type: ActorType::Client,
            actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
        },
        ConnectorConfig {
            account_scope: "chatgpt-connector-synthetic".into(),
            default_timezone: "Asia/Shanghai".into(),
            max_sensitivity: Sensitivity::Private,
            allowed_forms: vec![FormationKind::Preference],
            window_days: 30,
        },
    )
    .unwrap();
    let clock = FakeClock::new(NOW);
    let ids = SequentialIdSource::new(40);
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let read_only = vec!["memory.read".to_string()];
    let read_write = vec!["memory.read".to_string(), "memory.write".to_string()];
    // Without memory.write the tool is neither listed nor callable.
    let mut s = ConnectorSession::new(Stub, Some(&writer), &read_only, &clock, &ids, &profile);
    for m in init() {
        s.handle(&m);
    }
    let list = s
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    assert_eq!(list["result"]["tools"].as_array().unwrap().len(), 1);
    let refused = s
        .handle(&call(3, json!({"submission": submission()})))
        .unwrap();
    assert_eq!(refused["error"]["message"], "stub");
    // With memory.write, calls before initialization go to the read session.
    let mut s = ConnectorSession::new(Stub, Some(&writer), &read_write, &clock, &ids, &profile);
    let early = s
        .handle(&call(1, json!({"submission": submission()})))
        .unwrap();
    assert_eq!(early["error"]["message"], "stub");
    for m in init() {
        s.handle(&m);
    }
    let list = s
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let names: Vec<_> = list["result"]["tools"]
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].clone())
        .collect();
    assert_eq!(
        names,
        vec![json!("memory_read"), json!(TOOL), json!(THEME_TOOL)]
    );
    let bad = s.handle(&call(3, json!({"submission": 1}))).unwrap();
    assert_eq!(bad["result"]["isError"], true);
    let bad_theme = s
        .handle(&json!({"jsonrpc":"2.0","id":30,"method":"tools/call","params":{"name":THEME_TOOL,"arguments":{"theme":1}}}))
        .unwrap();
    assert_eq!(bad_theme["result"]["isError"], true);
    let dry = s
        .handle(&call(
            4,
            json!({"submission": submission(), "preflight": true}),
        ))
        .unwrap();
    assert_eq!(dry["result"]["isError"], false);
    assert_eq!(dry["result"]["structuredContent"]["window_needed"], true);
    let done = s
        .handle(&call(5, json!({"submission": submission()})))
        .unwrap();
    assert_eq!(done["result"]["isError"], false, "{done}");
    let jobs = &done["result"]["structuredContent"]["jobs"];
    assert_eq!(jobs[0]["state"], "formed");
    assert_eq!(jobs[0]["receipts"], 1);
    assert!(s.unknown().is_none());
}

#[test]
fn protected_resource_metadata_lists_write_only_when_enabled() {
    let ids = SequentialIdSource::new(1);
    struct None_;
    impl Tokens for None_ {
        fn verify(&self, _: &str) -> Option<Bearer> {
            None
        }
    }
    for (write, scopes) in [
        (false, json!(["memory.read"])),
        (true, json!(["memory.read", "memory.write"])),
    ] {
        let config = Config::new(
            "https://memory.example/mcp",
            &["https://memory.example"],
            &["https://chatgpt.com"],
        )
        .unwrap();
        let config = if write { config.with_write() } else { config };
        let mut endpoint = Endpoint::new(config, &None_, &ids, |_: &Bearer| None::<Stub>);
        let response = endpoint.respond(&Request {
            method: "GET".into(),
            target: "/.well-known/oauth-protected-resource/mcp".into(),
            headers: vec![],
            body: vec![],
        });
        let body: Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(body["scopes_supported"], scopes);
    }
}

/// ADR-MEM-190: in the read grant's last 14 days, tool results say when it
/// ends so ChatGPT can tell Morii; earlier, and outside tool calls, nothing.
#[test]
fn tool_results_carry_a_grant_notice_near_the_grant_end() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let writer = ConnectorWriter::new(
        v.clone(),
        e.descriptor.created_by.clone(),
        ActorRef {
            actor_type: ActorType::Client,
            actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
        },
        ConnectorConfig {
            account_scope: "chatgpt-connector-synthetic".into(),
            default_timezone: "Asia/Shanghai".into(),
            max_sensitivity: Sensitivity::Private,
            allowed_forms: vec![FormationKind::Preference],
            window_days: 30,
        },
    )
    .unwrap();
    let clock = FakeClock::new(NOW);
    let ids = SequentialIdSource::new(40);
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let scopes = vec!["memory.read".to_string(), "memory.write".to_string()];
    let day = 86_400_000;
    let session = |days: i64| {
        let end = enouia_memory_contract::time::Timestamp::from_unix_ms(NOW + days * day).unwrap();
        let mut s = ConnectorSession::new(Stub, Some(&writer), &scopes, &clock, &ids, &profile)
            .with_grant_end(end);
        for m in init() {
            s.handle(&m);
        }
        s
    };
    let dry = |s: &mut ConnectorSession<Stub>| {
        s.handle(&call(
            4,
            json!({"submission": submission(), "preflight": true}),
        ))
        .unwrap()
    };
    let mut near = session(3);
    let result = dry(&mut near);
    let notice = &result["result"]["structuredContent"]["grant_notice"];
    assert_eq!(notice["days_left"], 3, "{result}");
    assert!(notice["message"].as_str().unwrap().contains("renew"));
    let text: Value =
        serde_json::from_str(result["result"]["content"][0]["text"].as_str().unwrap()).unwrap();
    assert_eq!(&text["grant_notice"], notice);
    let list = near
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    assert!(list["result"].get("grant_notice").is_none());
    let mut far = session(30);
    assert!(
        dry(&mut far)["result"]["structuredContent"]
            .get("grant_notice")
            .is_none()
    );
}

/// ADR-MEM-195: turn times as ChatGPT writes them, and why arguments fail.
#[test]
fn chatgpt_turn_times_become_utc_and_bad_arguments_say_why() {
    use enouia_memory_host::mcp::remote::chatgpt::{conversation_arguments, utc_time};
    for (given, utc) in [
        (
            "2026-11-24T21:05:00+08:00",
            Some("2026-11-24T13:05:00.000Z"),
        ),
        ("2026-11-24T13:05:00Z", Some("2026-11-24T13:05:00.000Z")),
        ("2026-11-24t13:05:00.5z", Some("2026-11-24T13:05:00.500Z")),
        (
            "2026-11-24T01:05:00.123456-05:30",
            Some("2026-11-24T06:35:00.123Z"),
        ),
        (
            "2026-11-24T00:30:00+08:00",
            Some("2026-11-23T16:30:00.000Z"),
        ),
        ("2026-11-24 13:05:00", None),
        ("2026-11-24T13:05:00", None),
        ("2026-11-24T13:05:00+8", None),
        ("2026-13-24T13:05:00Z", None),
        ("２０２６-11-24T13:05:00Z", None),
    ] {
        assert_eq!(utc_time(given).as_deref(), utc, "{given}");
    }
    let mut args = json!({"submission": {"request": {"request_id": "k", "conversation_id": "c", "turns": [
        {"message_id": "a", "speaker": "user", "text": "hi", "occurred_at": "2026-11-24T21:05:00+08:00"},
        {"message_id": "b", "speaker": "user", "text": "hi", "occurred_at": "2026-11-24T13:05:00.000Z"}
    ]}, "analyst_model": "m", "statements": []}});
    conversation_arguments(&mut args);
    let turns = &args["submission"]["request"]["turns"];
    assert_eq!(turns[0]["occurred_at"], "2026-11-24T13:05:00.000Z");
    assert_eq!(turns[0]["original_time"], "2026-11-24T21:05:00+08:00");
    assert_eq!(turns[1]["occurred_at"], "2026-11-24T13:05:00.000Z");
    assert_eq!(turns[1]["original_time"], Value::Null);

    // An extra field ChatGPT invents is named in the refusal.
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let writer = ConnectorWriter::new(
        v.clone(),
        e.descriptor.created_by.clone(),
        ActorRef {
            actor_type: ActorType::Client,
            actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
        },
        ConnectorConfig {
            account_scope: "chatgpt-connector-synthetic".into(),
            default_timezone: "Asia/Shanghai".into(),
            max_sensitivity: Sensitivity::Private,
            allowed_forms: vec![FormationKind::Preference],
            window_days: 30,
        },
    )
    .unwrap();
    let clock = FakeClock::new(NOW);
    let ids = SequentialIdSource::new(40);
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let scopes = vec!["memory.read".to_string(), "memory.write".to_string()];
    let mut s = ConnectorSession::new(Stub, Some(&writer), &scopes, &clock, &ids, &profile);
    for m in init() {
        s.handle(&m);
    }
    let bad = json!({"request": {"request_id": "k", "conversation_id": "c", "turns": [
        {"message_id": "a", "speaker": "user", "text": "hi", "created_at": "today"}
    ]}, "analyst_model": "m", "statements": []});
    let refused = s.handle(&call(9, json!({"submission": bad}))).unwrap();
    assert_eq!(refused["result"]["isError"], true);
    let data = &refused["result"]["structuredContent"];
    assert_eq!(data["error"], "invalid_arguments");
    assert!(
        data["problems"][0].as_str().unwrap().contains("created_at"),
        "{data}"
    );
}

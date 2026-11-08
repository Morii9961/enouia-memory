//! ADR-MEM-129 Streamable HTTP boundary with a fake session; platform-neutral.
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_host::mcp::remote::*;
use serde_json::{Value, json};
use std::cell::RefCell;

#[derive(Default)]
struct Fake {
    seen: Vec<Value>,
    closed: bool,
    unknown: Option<Value>,
}
struct Probe<'a>(&'a RefCell<Vec<Value>>, Fake);
impl Session for Probe<'_> {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        self.0.borrow_mut().push(message.clone());
        self.1.seen.push(message.clone());
        match message["method"].as_str() {
            Some("boom") => {
                self.1.unknown = Some(message.clone());
                None
            }
            Some("bye") => {
                self.1.closed = true;
                Some(json!({"jsonrpc":"2.0","id":message["id"],"result":{}}))
            }
            Some("initialize") if message["params"] == json!({}) => Some(
                json!({"jsonrpc":"2.0","id":message["id"],"error":{"code":-32602,"message":"no"}}),
            ),
            _ if message.get("id").is_none() => None,
            _ => Some(
                json!({"jsonrpc":"2.0","id":message["id"],"result":{"echo":message["method"]}}),
            ),
        }
    }
    fn closed(&self) -> bool {
        self.1.closed
    }
    fn unknown(&self) -> Option<&Value> {
        self.1.unknown.as_ref()
    }
}
struct Fixed;
impl Tokens for Fixed {
    fn verify(&self, token: &str) -> Option<Bearer> {
        let scopes = |s: &[&str]| s.iter().map(|s| s.to_string()).collect();
        match token {
            "tok-owner" => Some(Bearer {
                subject: "owner".into(),
                scopes: scopes(&["memory.read"]),
            }),
            "tok-other" => Some(Bearer {
                subject: "other".into(),
                scopes: scopes(&["memory.read"]),
            }),
            "tok-narrow" => Some(Bearer {
                subject: "owner".into(),
                scopes: scopes(&["profile"]),
            }),
            _ => None,
        }
    }
}
fn config() -> Config {
    Config::new(
        "https://memory.example/mcp",
        &["https://auth.example"],
        &["https://chatgpt.com"],
    )
    .unwrap()
}
fn post(token: Option<&str>, session: Option<&str>, body: Value) -> Request {
    let mut headers = vec![
        ("Content-Type".into(), "application/json".into()),
        (
            "Accept".into(),
            "application/json, text/event-stream".into(),
        ),
    ];
    if let Some(token) = token {
        headers.push(("Authorization".into(), format!("Bearer {token}")));
    }
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
fn init(id: i64) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}})
}
fn call(id: i64, method: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"method":method,"params":{}})
}
fn body(response: &Response) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}

#[test]
fn config_refuses_plain_http_queries_and_missing_authorization_servers() {
    assert!(Config::new("http://memory.example/mcp", &["https://auth.example"], &[]).is_none());
    assert!(
        Config::new(
            "https://memory.example/mcp?x=1",
            &["https://auth.example"],
            &[]
        )
        .is_none()
    );
    assert!(Config::new("https://memory.example", &["https://auth.example"], &[]).is_none());
    assert!(Config::new("https://memory.example/mcp", &[], &[]).is_none());
    assert!(Config::new("https://memory.example/mcp", &["http://auth.example"], &[]).is_none());
}

#[test]
fn metadata_is_public_and_every_mcp_call_needs_a_read_bearer() {
    let seen = RefCell::new(vec![]);
    let ids = SequentialIdSource::new(1);
    let mut opened = 0;
    let mut endpoint = Endpoint::new(config(), &Fixed, &ids, |_: &Bearer| {
        opened += 1;
        Some(Probe(&seen, Fake::default()))
    });
    for target in [
        "/.well-known/oauth-protected-resource/mcp",
        "/.well-known/oauth-protected-resource",
    ] {
        let response = endpoint.respond(&Request {
            method: "GET".into(),
            target: target.into(),
            headers: vec![],
            body: vec![],
        });
        assert_eq!(response.status, 200);
        assert_eq!(
            body(&response),
            json!({"resource":"https://memory.example/mcp","authorization_servers":["https://auth.example"],"scopes_supported":["memory.read"],"bearer_methods_supported":["header"]})
        );
    }
    let missing = endpoint.respond(&post(None, None, init(1)));
    assert_eq!(missing.status, 401);
    assert_eq!(
        missing.header_value("www-authenticate"),
        Some(
            "Bearer resource_metadata=\"https://memory.example/.well-known/oauth-protected-resource/mcp\""
        )
    );
    let invalid = endpoint.respond(&post(Some("tok-forged"), None, init(1)));
    assert_eq!(invalid.status, 401);
    assert!(
        invalid
            .header_value("www-authenticate")
            .unwrap()
            .ends_with("error=\"invalid_token\"")
    );
    let narrow = endpoint.respond(&post(Some("tok-narrow"), None, init(1)));
    assert_eq!(narrow.status, 403);
    assert!(
        narrow
            .header_value("www-authenticate")
            .unwrap()
            .contains("insufficient_scope")
    );
    let mut basic = post(None, None, init(1));
    basic
        .headers
        .push(("Authorization".into(), "Basic dG9rLW93bmVy".into()));
    assert_eq!(endpoint.respond(&basic).status, 401);
    let mut twice = post(Some("tok-owner"), None, init(1));
    twice
        .headers
        .push(("authorization".into(), "Bearer tok-other".into()));
    assert_eq!(endpoint.respond(&twice).status, 400);
    let mut origin = post(Some("tok-owner"), None, init(1));
    origin
        .headers
        .push(("Origin".into(), "https://evil.example".into()));
    assert_eq!(endpoint.respond(&origin).status, 403);
    let mut query = post(Some("tok-owner"), None, init(1));
    query.target = "/mcp?access_token=tok-owner".into();
    assert_eq!(endpoint.respond(&query).status, 404);
    drop(endpoint);
    assert_eq!(opened, 0);
    assert!(seen.borrow().is_empty());
}

#[test]
fn initialize_binds_one_session_to_the_bearer_subject_and_protocol_version() {
    let seen = RefCell::new(vec![]);
    let ids = SequentialIdSource::new(7);
    let mut endpoint = Endpoint::new(config(), &Fixed, &ids, |_: &Bearer| {
        Some(Probe(&seen, Fake::default()))
    });
    let mut allowed = post(Some("tok-owner"), None, init(1));
    allowed
        .headers
        .push(("Origin".into(), "https://chatgpt.com".into()));
    let opened = endpoint.respond(&allowed);
    assert_eq!(opened.status, 200);
    assert_eq!(
        opened.header_value("content-type"),
        Some("application/json")
    );
    assert_eq!(opened.header_value("cache-control"), Some("no-store"));
    let id = opened.header_value("mcp-session-id").unwrap().to_owned();
    assert_eq!(id, "00000000000000000000000000000007");
    assert_eq!(body(&opened)["id"], 1);
    let note = endpoint.respond(&post(
        Some("tok-owner"),
        Some(&id),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ));
    assert_eq!((note.status, note.body.is_empty()), (202, true));
    let listed = endpoint.respond(&post(Some("tok-owner"), Some(&id), call(2, "tools/list")));
    assert_eq!(
        body(&listed),
        json!({"jsonrpc":"2.0","id":2,"result":{"echo":"tools/list"}})
    );
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-other"), Some(&id), call(3, "tools/list")))
            .status,
        404
    );
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-owner"), None, call(3, "tools/list")))
            .status,
        400
    );
    assert_eq!(
        endpoint
            .respond(&post(
                Some("tok-owner"),
                Some("ffff"),
                call(3, "tools/list")
            ))
            .status,
        404
    );
    let mut old = post(Some("tok-owner"), Some(&id), call(3, "tools/list"));
    old.headers.retain(|(n, _)| n != "MCP-Protocol-Version");
    assert_eq!(endpoint.respond(&old).status, 400);
    old.headers
        .push(("MCP-Protocol-Version".into(), "2025-03-26".into()));
    assert_eq!(endpoint.respond(&old).status, 400);
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-owner"), Some(&id), init(4)))
            .status,
        400
    );
    let mut text = post(Some("tok-owner"), Some(&id), call(5, "tools/list"));
    text.headers[0].1 = "text/plain".into();
    assert_eq!(endpoint.respond(&text).status, 415);
    let mut accept = post(Some("tok-owner"), Some(&id), call(5, "tools/list"));
    accept.headers[1].1 = "text/event-stream".into();
    assert_eq!(endpoint.respond(&accept).status, 406);
    let batch = endpoint.respond(&post(
        Some("tok-owner"),
        Some(&id),
        json!([call(6, "tools/list")]),
    ));
    assert_eq!(
        (batch.status, body(&batch)["error"]["code"].clone()),
        (400, json!(-32600))
    );
    let mut duplicate = post(Some("tok-owner"), Some(&id), json!({}));
    duplicate.body =
        br#"{"jsonrpc":"2.0","id":7,"method":"tools/list","method":"tools/call"}"#.to_vec();
    assert_eq!(body(&endpoint.respond(&duplicate))["error"]["code"], -32700);
    let mut large = post(Some("tok-owner"), Some(&id), json!({}));
    large.body = vec![b' '; 256 * 1024 + 1];
    assert_eq!(endpoint.respond(&large).status, 413);
    let get = endpoint.respond(&Request {
        method: "GET".into(),
        target: "/mcp".into(),
        headers: post(Some("tok-owner"), Some(&id), json!({})).headers,
        body: vec![],
    });
    assert_eq!(
        (get.status, get.header_value("allow")),
        (405, Some("POST, DELETE"))
    );
    let methods: Vec<Value> = seen.borrow().iter().map(|m| m["method"].clone()).collect();
    assert_eq!(
        methods,
        [
            json!("initialize"),
            json!("notifications/initialized"),
            json!("tools/list")
        ]
    );
}

#[test]
fn refused_initialize_close_delete_and_session_cap_leave_no_stale_session() {
    let seen = RefCell::new(vec![]);
    let ids = SequentialIdSource::new(1);
    let mut endpoint = Endpoint::new(config(), &Fixed, &ids, |bearer: &Bearer| {
        (bearer.subject == "owner").then(|| Probe(&seen, Fake::default()))
    });
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-other"), None, init(1)))
            .status,
        403
    );
    let mut refused = init(1);
    refused["params"] = json!({});
    let refusal = endpoint.respond(&post(Some("tok-owner"), None, refused));
    assert_eq!(
        (refusal.status, body(&refusal)["error"]["code"].clone()),
        (200, json!(-32602))
    );
    assert_eq!(
        (refusal.header_value("mcp-session-id"), endpoint.sessions()),
        (None, 0)
    );
    let mut ids_seen = vec![];
    for n in 0..MAX_SESSIONS {
        let opened = endpoint.respond(&post(Some("tok-owner"), None, init(n as i64)));
        ids_seen.push(opened.header_value("mcp-session-id").unwrap().to_owned());
    }
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-owner"), None, init(9)))
            .status,
        503
    );
    let closed = endpoint.respond(&post(Some("tok-owner"), Some(&ids_seen[0]), call(2, "bye")));
    assert_eq!(closed.status, 200);
    assert_eq!(
        endpoint
            .respond(&post(
                Some("tok-owner"),
                Some(&ids_seen[0]),
                call(3, "tools/list")
            ))
            .status,
        404
    );
    let mut delete = post(Some("tok-other"), Some(&ids_seen[1]), json!({}));
    delete.method = "DELETE".into();
    assert_eq!(endpoint.respond(&delete).status, 404);
    delete.headers = post(Some("tok-owner"), Some(&ids_seen[1]), json!({})).headers;
    assert_eq!(endpoint.respond(&delete).status, 204);
    assert_eq!(endpoint.respond(&delete).status, 404);
    assert_eq!(endpoint.sessions(), MAX_SESSIONS - 2);
}

#[test]
fn unknown_outcome_halts_every_session_and_retains_the_first_original() {
    let seen = RefCell::new(vec![]);
    let ids = SequentialIdSource::new(1);
    let mut endpoint = Endpoint::new(config(), &Fixed, &ids, |_: &Bearer| {
        Some(Probe(&seen, Fake::default()))
    });
    let a = endpoint
        .respond(&post(Some("tok-owner"), None, init(1)))
        .header_value("mcp-session-id")
        .unwrap()
        .to_owned();
    let b = endpoint
        .respond(&post(Some("tok-owner"), None, init(1)))
        .header_value("mcp-session-id")
        .unwrap()
        .to_owned();
    let boom = json!({"jsonrpc":"2.0","id":5,"method":"boom","params":{"keep":"exact"}});
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-owner"), Some(&a), boom.clone()))
            .status,
        503
    );
    assert_eq!(endpoint.halted(), Some(&boom));
    assert_eq!(endpoint.sessions(), 0);
    let before = seen.borrow().len();
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-owner"), Some(&b), call(6, "tools/list")))
            .status,
        503
    );
    assert_eq!(
        endpoint
            .respond(&post(Some("tok-owner"), None, init(7)))
            .status,
        503
    );
    assert_eq!(seen.borrow().len(), before);
    assert_eq!(endpoint.halted(), Some(&boom));
}

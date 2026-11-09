//! ADR-MEM-132 single-owner OAuth authority; platform-neutral, fake session.
use enouia_memory_contract::{foundation::Clock, hash::sha256, ports::SequentialIdSource};
use enouia_memory_host::mcp::remote::{oauth::*, *};
use serde_json::{Value, json};
use std::cell::Cell;
use std::collections::BTreeMap;

struct Now(Cell<i64>);
impl Clock for Now {
    fn now_unix_ms(&self) -> i64 {
        self.0.get()
    }
}
struct Echo;
impl Session for Echo {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        message
            .get("id")
            .map(|id| json!({"jsonrpc":"2.0","id":id,"result":{"echo":message["method"]}}))
    }
    fn closed(&self) -> bool {
        false
    }
    fn unknown(&self) -> Option<&Value> {
        None
    }
}

const ISSUER: &str = "https://memory.example";
const RESOURCE: &str = "https://memory.example/mcp";
const REDIRECT: &str = "https://chatgpt.com/connector_platform_oauth_redirect";
const VERIFIER: &str = "synthetic-verifier-0123456789-abcdefghijklmnopqrstuvwxyz";

fn challenge(verifier: &str) -> String {
    const A: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let hex = sha256(verifier.as_bytes());
    let raw: Vec<u8> = (0..32)
        .map(|i| u8::from_str_radix(&hex.as_str()[2 * i..2 * i + 2], 16).unwrap())
        .collect();
    let mut out = String::new();
    for chunk in raw.chunks(3) {
        let n = chunk
            .iter()
            .enumerate()
            .fold(0u32, |n, (i, b)| n | (*b as u32) << (16 - 8 * i));
        for i in 0..=chunk.len() {
            out.push(A[(n >> (18 - 6 * i) & 63) as usize] as char);
        }
    }
    out
}
fn enc(s: &str) -> String {
    s.bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-._~".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
fn query(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}
fn get(target: &str) -> Request {
    Request {
        method: "GET".into(),
        target: target.into(),
        headers: vec![],
        body: vec![],
    }
}
fn post_form(target: &str, pairs: &[(&str, &str)]) -> Request {
    Request {
        method: "POST".into(),
        target: target.into(),
        headers: vec![(
            "Content-Type".into(),
            "application/x-www-form-urlencoded".into(),
        )],
        body: query(pairs).into_bytes(),
    }
}
fn post_json(target: &str, body: Value) -> Request {
    Request {
        method: "POST".into(),
        target: target.into(),
        headers: vec![("Content-Type".into(), "application/json".into())],
        body: body.to_string().into_bytes(),
    }
}
fn json_body(response: &Response) -> Value {
    serde_json::from_slice(&response.body).unwrap()
}
fn text(response: &Response) -> String {
    String::from_utf8(response.body.clone()).unwrap()
}
fn location_params(response: &Response) -> BTreeMap<String, String> {
    let location = response.header_value("location").unwrap();
    let (base, query) = location.split_once('?').unwrap();
    assert_eq!(base, REDIRECT);
    query
        .split('&')
        .map(|p| {
            let (k, v) = p.split_once('=').unwrap();
            let bytes = v.as_bytes();
            let mut out = vec![];
            let mut i = 0;
            while i < bytes.len() {
                if bytes[i] == b'%' {
                    out.push(u8::from_str_radix(&v[i + 1..i + 3], 16).unwrap());
                    i += 3;
                } else {
                    out.push(bytes[i]);
                    i += 1;
                }
            }
            (k.to_owned(), String::from_utf8(out).unwrap())
        })
        .collect()
}
fn config() -> OAuthConfig {
    OAuthConfig::new(ISSUER, RESOURCE, &["https://chatgpt.com/"]).unwrap()
}
fn register(authority: &Authority) -> String {
    let response = authority.respond(&post_json("/oauth/register", json!({"redirect_uris":[REDIRECT],"client_name":"ChatGPT","token_endpoint_auth_method":"none","grant_types":["authorization_code","refresh_token"],"response_types":["code"]}))).unwrap();
    assert_eq!(response.status, 201);
    json_body(&response)["client_id"]
        .as_str()
        .unwrap()
        .to_owned()
}
fn authorize_params<'a>(client: &'a str, challenge: &'a str) -> Vec<(&'a str, &'a str)> {
    vec![
        ("response_type", "code"),
        ("client_id", client),
        ("redirect_uri", REDIRECT),
        ("code_challenge", challenge),
        ("code_challenge_method", "S256"),
        ("state", "st<&>"),
        ("scope", "memory.read"),
        ("resource", RESOURCE),
    ]
}
fn approve(authority: &Authority, client: &str) -> String {
    let approval = authority.issue_approval_code();
    let challenge = challenge(VERIFIER);
    let mut params = authorize_params(client, &challenge);
    params.push(("approval_code", &approval));
    let response = authority
        .respond(&post_form("/oauth/authorize", &params))
        .unwrap();
    assert_eq!(response.status, 302, "{}", text(&response));
    let params = location_params(&response);
    assert_eq!(
        (params["state"].as_str(), params["iss"].as_str()),
        ("st<&>", ISSUER)
    );
    params["code"].clone()
}
fn exchange(authority: &Authority, client: &str, code: &str) -> Response {
    authority
        .respond(&post_form(
            "/oauth/token",
            &[
                ("grant_type", "authorization_code"),
                ("code", code),
                ("redirect_uri", REDIRECT),
                ("client_id", client),
                ("code_verifier", VERIFIER),
                ("resource", RESOURCE),
            ],
        ))
        .unwrap()
}
fn refresh(authority: &Authority, client: &str, token: &str) -> Response {
    authority
        .respond(&post_form(
            "/oauth/token",
            &[
                ("grant_type", "refresh_token"),
                ("refresh_token", token),
                ("client_id", client),
            ],
        ))
        .unwrap()
}

#[test]
fn config_requires_https_origin_resource_under_it_and_https_redirect_prefixes() {
    assert!(
        OAuthConfig::new("http://memory.example", RESOURCE, &["https://chatgpt.com/"]).is_none()
    );
    assert!(
        OAuthConfig::new(
            "https://memory.example/",
            RESOURCE,
            &["https://chatgpt.com/"]
        )
        .is_none()
    );
    assert!(
        OAuthConfig::new(
            ISSUER,
            "https://other.example/mcp",
            &["https://chatgpt.com/"]
        )
        .is_none()
    );
    assert!(OAuthConfig::new(ISSUER, RESOURCE, &[]).is_none());
    assert!(OAuthConfig::new(ISSUER, RESOURCE, &["https://chatgpt.com"]).is_none());
    assert!(OAuthConfig::new(ISSUER, RESOURCE, &["http://chatgpt.com/"]).is_none());
}

#[test]
fn metadata_and_registration_accept_only_public_pkce_clients_on_allowed_redirects() {
    let ids = SequentialIdSource::new(1);
    let now = Now(Cell::new(1_000_000));
    let authority = Authority::new(config(), &ids, &now, State::default());
    assert!(authority.respond(&get("/mcp")).is_none());
    let meta = authority
        .respond(&get("/.well-known/oauth-authorization-server"))
        .unwrap();
    assert_eq!(
        json_body(&meta)["token_endpoint"],
        "https://memory.example/oauth/token"
    );
    assert_eq!(
        json_body(&meta)["code_challenge_methods_supported"],
        json!(["S256"])
    );
    assert_eq!(
        json_body(&meta)["token_endpoint_auth_methods_supported"],
        json!(["none"])
    );
    for bad in [
        json!({}),
        json!({"redirect_uris":[]}),
        json!({"redirect_uris":["https://evil.example/cb"]}),
        json!({"redirect_uris":["https://chatgpt.com/cb#frag"]}),
        json!({"redirect_uris":[REDIRECT, 5]}),
        json!({"redirect_uris":[REDIRECT],"token_endpoint_auth_method":"client_secret_basic"}),
        json!({"redirect_uris":[REDIRECT],"grant_types":["client_credentials"]}),
        json!({"redirect_uris":[REDIRECT],"response_types":["token"]}),
        json!([REDIRECT]),
    ] {
        assert_eq!(
            authority
                .respond(&post_json("/oauth/register", bad.clone()))
                .unwrap()
                .status,
            400,
            "{bad}"
        );
    }
    let mut duplicate = post_json("/oauth/register", json!({}));
    duplicate.body =
        format!(r#"{{"redirect_uris":["{REDIRECT}"],"redirect_uris":["https://chatgpt.com/x"]}}"#)
            .into_bytes();
    assert_eq!(authority.respond(&duplicate).unwrap().status, 400);
    for _ in 0..MAX_CLIENTS {
        register(&authority);
    }
    assert_eq!(
        authority
            .respond(&post_json(
                "/oauth/register",
                json!({"redirect_uris":[REDIRECT]})
            ))
            .unwrap()
            .status,
        400
    );
    assert_eq!(authority.respond(&get("/oauth/token")).unwrap().status, 405);
}

#[test]
fn approval_code_gates_authorization_and_unknown_clients_never_redirect() {
    let ids = SequentialIdSource::new(1);
    let now = Now(Cell::new(1_000_000));
    let authority = Authority::new(config(), &ids, &now, State::default());
    let client = register(&authority);
    let challenge = challenge(VERIFIER);
    let page = authority
        .respond(&get(&format!(
            "/oauth/authorize?{}",
            query(&authorize_params(&client, &challenge))
        )))
        .unwrap();
    assert_eq!(page.status, 200);
    assert_eq!(page.header_value("x-frame-options"), Some("DENY"));
    assert!(
        page.header_value("content-security-policy")
            .unwrap()
            .contains("frame-ancestors 'none'")
    );
    let html = text(&page);
    assert!(html.contains("value=\"st&lt;&amp;&gt;\"") && !html.contains("st<&>"));
    let unknown = authority
        .respond(&get(&format!(
            "/oauth/authorize?{}",
            query(&authorize_params("ffff", &challenge))
        )))
        .unwrap();
    assert_eq!(
        (unknown.status, unknown.header_value("location")),
        (400, None)
    );
    let mut wrong_redirect = authorize_params(&client, &challenge);
    wrong_redirect[2].1 = "https://chatgpt.com/other";
    assert_eq!(
        authority
            .respond(&get(&format!(
                "/oauth/authorize?{}",
                query(&wrong_redirect)
            )))
            .unwrap()
            .header_value("location"),
        None
    );
    let mut plain = authorize_params(&client, &challenge);
    plain[4].1 = "plain";
    assert_eq!(
        location_params(
            &authority
                .respond(&get(&format!("/oauth/authorize?{}", query(&plain))))
                .unwrap()
        )["error"],
        "invalid_request"
    );
    let mut wide = authorize_params(&client, &challenge);
    wide[6].1 = "memory.read memory.write";
    assert_eq!(
        location_params(
            &authority
                .respond(&get(&format!("/oauth/authorize?{}", query(&wide))))
                .unwrap()
        )["error"],
        "invalid_scope"
    );
    let mut elsewhere = authorize_params(&client, &challenge);
    elsewhere[7].1 = "https://other.example/mcp";
    assert_eq!(
        location_params(
            &authority
                .respond(&get(&format!("/oauth/authorize?{}", query(&elsewhere))))
                .unwrap()
        )["error"],
        "invalid_target"
    );
    // No approval code issued yet: nothing can be approved.
    let mut submit = authorize_params(&client, &challenge);
    submit.push(("approval_code", "0000000000"));
    let refused = authority
        .respond(&post_form("/oauth/authorize", &submit))
        .unwrap();
    assert_eq!(
        (refused.status, refused.header_value("location")),
        (400, None)
    );
    // Wrong codes burn the approval after five attempts.
    let real = authority.issue_approval_code();
    assert_eq!(real.len(), 10);
    for n in 1..MAX_APPROVAL_FAILURES {
        let response = authority
            .respond(&post_form("/oauth/authorize", &submit))
            .unwrap();
        assert!(text(&response).contains("批准码不正确"), "{n}");
    }
    assert!(
        text(
            &authority
                .respond(&post_form("/oauth/authorize", &submit))
                .unwrap()
        )
        .contains("已作废")
    );
    let mut late = authorize_params(&client, &challenge);
    late.push(("approval_code", &real));
    assert_eq!(
        authority
            .respond(&post_form("/oauth/authorize", &late))
            .unwrap()
            .header_value("location"),
        None
    );
    // An expired approval code is refused.
    let stale = authority.issue_approval_code();
    now.0.set(now.0.get() + APPROVAL_TTL_MS);
    let mut expired = authorize_params(&client, &challenge);
    expired.push(("approval_code", &stale));
    assert_eq!(
        authority
            .respond(&post_form("/oauth/authorize", &expired))
            .unwrap()
            .header_value("location"),
        None
    );
    // A fresh code works once, case-insensitively, then is gone.
    let fresh = authority.issue_approval_code().to_ascii_lowercase();
    let mut ok = authorize_params(&client, &challenge);
    ok.push(("approval_code", &fresh));
    assert_eq!(
        authority
            .respond(&post_form("/oauth/authorize", &ok))
            .unwrap()
            .status,
        302
    );
    assert_eq!(
        authority
            .respond(&post_form("/oauth/authorize", &ok))
            .unwrap()
            .status,
        400
    );
}

#[test]
fn code_exchange_needs_pkce_and_tokens_open_the_mcp_endpoint_through_the_router() {
    let ids = SequentialIdSource::new(1);
    let now = Now(Cell::new(1_000_000));
    let authority = Authority::new(config(), &ids, &now, State::default());
    let client = register(&authority);
    let code = approve(&authority, &client);
    let mut bad = post_form(
        "/oauth/token",
        &[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", REDIRECT),
            ("client_id", &client),
            (
                "code_verifier",
                "wrong-verifier-wrong-verifier-wrong-verifier-000",
            ),
        ],
    );
    assert_eq!(
        json_body(&authority.respond(&bad).unwrap())["error"],
        "invalid_grant"
    );
    // The code was burned by the failed attempt.
    assert_eq!(
        json_body(&exchange(&authority, &client, &code))["error"],
        "invalid_grant"
    );
    let code = approve(&authority, &client);
    bad.body = query(&[
        ("grant_type", "authorization_code"),
        ("code", &code),
        ("redirect_uri", REDIRECT),
        ("client_id", "ffff"),
        ("code_verifier", VERIFIER),
    ])
    .into_bytes();
    assert_eq!(authority.respond(&bad).unwrap().status, 401);
    let issued = exchange(&authority, &client, &code);
    assert_eq!(issued.status, 200);
    assert_eq!(issued.header_value("cache-control"), Some("no-store"));
    let tokens = json_body(&issued);
    assert_eq!(
        (
            tokens["token_type"].as_str(),
            tokens["expires_in"].as_i64(),
            tokens["scope"].as_str()
        ),
        (Some("Bearer"), Some(3600), Some("memory.read"))
    );
    let access = tokens["access_token"].as_str().unwrap().to_owned();
    assert_eq!(
        authority.verify(&access),
        Some(Bearer {
            subject: "owner".into(),
            scopes: vec!["memory.read".into()]
        })
    );
    assert_eq!(authority.verify("forged"), None);
    let endpoint_config = Config::new(RESOURCE, &[ISSUER], &[]).unwrap();
    let mut router = Router {
        authority: &authority,
        endpoint: Endpoint::new(endpoint_config, &authority, &ids, |_: &Bearer| Some(Echo)),
    };
    let mut init = post_json(
        "/mcp",
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}}),
    );
    init.headers
        .push(("Accept".into(), "application/json".into()));
    init.headers
        .push(("Authorization".into(), format!("Bearer {access}")));
    let opened = router.respond(&init);
    assert_eq!(opened.status, 200);
    assert!(opened.header_value("mcp-session-id").is_some());
    assert_eq!(
        router
            .respond(&get("/.well-known/oauth-authorization-server"))
            .status,
        200
    );
    assert_eq!(
        router
            .respond(&get("/.well-known/oauth-protected-resource/mcp"))
            .status,
        200
    );
    // The access token stops at its expiry.
    now.0.set(now.0.get() + ACCESS_TTL_MS);
    assert_eq!(authority.verify(&access), None);
    assert_eq!(router.respond(&init).status, 401);
}

#[test]
fn refresh_rotates_and_replays_revoke_the_whole_family() {
    let ids = SequentialIdSource::new(1);
    let now = Now(Cell::new(1_000_000));
    let authority = Authority::new(config(), &ids, &now, State::default());
    let client = register(&authority);
    let code = approve(&authority, &client);
    let first = json_body(&exchange(&authority, &client, &code));
    let rotated = json_body(&refresh(
        &authority,
        &client,
        first["refresh_token"].as_str().unwrap(),
    ));
    assert!(rotated["access_token"].is_string());
    assert_ne!(rotated["refresh_token"], first["refresh_token"]);
    let access = rotated["access_token"].as_str().unwrap();
    assert!(authority.verify(access).is_some());
    let other = register(&authority);
    assert_eq!(
        json_body(&refresh(
            &authority,
            &other,
            rotated["refresh_token"].as_str().unwrap()
        ))["error"],
        "invalid_grant"
    );
    // Replaying the rotated-away refresh token revokes everything in the family.
    assert_eq!(
        json_body(&refresh(
            &authority,
            &client,
            first["refresh_token"].as_str().unwrap()
        ))["error"],
        "invalid_grant"
    );
    assert_eq!(authority.verify(access), None);
    assert_eq!(
        authority.verify(first["access_token"].as_str().unwrap()),
        None
    );
    assert_eq!(
        json_body(&refresh(
            &authority,
            &client,
            rotated["refresh_token"].as_str().unwrap()
        ))["error"],
        "invalid_grant"
    );
    // Replaying a used authorization code also revokes its family.
    let code = approve(&authority, &client);
    let issued = json_body(&exchange(&authority, &client, &code));
    assert!(
        authority
            .verify(issued["access_token"].as_str().unwrap())
            .is_some()
    );
    assert_eq!(
        json_body(&exchange(&authority, &client, &code))["error"],
        "invalid_grant"
    );
    assert_eq!(
        authority.verify(issued["access_token"].as_str().unwrap()),
        None
    );
    // Codes expire after two minutes.
    let code = approve(&authority, &client);
    now.0.set(now.0.get() + CODE_TTL_MS);
    assert_eq!(
        json_body(&exchange(&authority, &client, &code))["error"],
        "invalid_grant"
    );
    assert_eq!(
        json_body(
            &authority
                .respond(&post_form(
                    "/oauth/token",
                    &[("grant_type", "password"), ("client_id", &client)]
                ))
                .unwrap()
        )["error"],
        "unsupported_grant_type"
    );
}

#[test]
fn persisted_state_holds_digests_only_and_restores_tokens() {
    let ids = SequentialIdSource::new(1);
    let now = Now(Cell::new(1_000_000));
    let authority = Authority::new(config(), &ids, &now, State::default());
    let client = register(&authority);
    let code = approve(&authority, &client);
    let tokens = json_body(&exchange(&authority, &client, &code));
    let saved = serde_json::to_string(&authority.state()).unwrap();
    for secret in [
        &code,
        tokens["access_token"].as_str().unwrap(),
        tokens["refresh_token"].as_str().unwrap(),
    ] {
        assert!(!saved.contains(secret));
    }
    let restored: State = serde_json::from_str(&saved).unwrap();
    let again = Authority::new(config(), &ids, &now, restored);
    assert!(
        again
            .verify(tokens["access_token"].as_str().unwrap())
            .is_some()
    );
    assert!(serde_json::from_str::<State>(&saved.replacen('{', r#"{"extra":1,"#, 1)).is_err());
}

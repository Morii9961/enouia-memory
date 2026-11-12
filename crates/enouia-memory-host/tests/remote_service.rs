#![cfg(any(windows, target_os = "linux"))]
//! ADR-MEM-133 runnable remote service over real loopback sockets, on a
//! synthetic initialized Owner Vault.
#[path = "support/owner_read_grant_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, foundation::Clock, hash::sha256, next::*};
use enouia_memory_host::mcp::remote::{http1, service};
use enouia_memory_vault::OsIdSource;
use fixture::*;
use serde_json::{Value, json};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::sync::atomic::{AtomicBool, Ordering};

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
fn form(pairs: &[(&str, &str)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{k}={}", enc(v)))
        .collect::<Vec<_>>()
        .join("&")
}
struct Reply {
    status: u16,
    head: String,
    body: Vec<u8>,
}
impl Reply {
    fn header(&self, name: &str) -> Option<String> {
        self.head.lines().find_map(|l| {
            let (n, v) = l.split_once(": ")?;
            n.eq_ignore_ascii_case(name).then(|| v.to_owned())
        })
    }
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap()
    }
}
fn send(
    addr: SocketAddr,
    method: &str,
    target: &str,
    headers: &[(&str, String)],
    body: &str,
) -> Reply {
    let mut stream = TcpStream::connect(addr).unwrap();
    let mut raw = format!(
        "{method} {target} HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Length: {}\r\n",
        body.len()
    );
    for (n, v) in headers {
        raw.push_str(&format!("{n}: {v}\r\n"));
    }
    raw.push_str("\r\n");
    raw.push_str(body);
    stream.write_all(raw.as_bytes()).unwrap();
    let mut out = vec![];
    stream.read_to_end(&mut out).unwrap();
    let end = out.windows(4).position(|w| w == b"\r\n\r\n").unwrap();
    let head = String::from_utf8(out[..end].to_vec()).unwrap();
    Reply {
        status: head[9..12].parse().unwrap(),
        head,
        body: out[end + 4..].to_vec(),
    }
}
fn mcp(addr: SocketAddr, token: &str, session: Option<&str>, body: Value) -> Reply {
    let mut headers = vec![
        ("Content-Type", "application/json".to_owned()),
        ("Accept", "application/json, text/event-stream".to_owned()),
        ("Authorization", format!("Bearer {token}")),
    ];
    if let Some(session) = session {
        headers.push(("Mcp-Session-Id", session.to_owned()));
        headers.push(("MCP-Protocol-Version", "2025-11-25".to_owned()));
    }
    send(addr, "POST", "/mcp", &headers, &body.to_string())
}
fn initialize(addr: SocketAddr, token: &str) -> String {
    let opened = mcp(
        addr,
        token,
        None,
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-remote","version":"1"}}}),
    );
    assert_eq!(
        opened.status,
        200,
        "{}",
        String::from_utf8_lossy(&opened.body)
    );
    let session = opened.header("mcp-session-id").unwrap();
    assert_eq!(
        mcp(
            addr,
            token,
            Some(&session),
            json!({"jsonrpc":"2.0","method":"notifications/initialized"})
        )
        .status,
        202
    );
    session
}
fn temp_dir(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "enouia-remote-service-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}
fn config_json(e: &Env, p: &AutomationPolicy, state_dir: &std::path::Path) -> Value {
    json!({
        "vault_root": e.root.path(),
        "host_id": e.descriptor.host_id,
        "state_dir": state_dir,
        "bind": "127.0.0.1:0",
        "resource": "https://memory.example/mcp",
        "issuer": "https://memory.example",
        "redirect_prefixes": ["https://chatgpt.com/"],
        "origins": ["https://chatgpt.com"],
        "agent_id": actor(ActorType::Agent, 88).actor_id,
        "subject_id": p.subject_id,
        "project_id": p.project_id,
        "policy_ids": [p.policy_id],
        "source_ids": p.source_ids,
        "max_sensitivity": "private",
        "source_read": true,
        "profile": value("attention/profile.json"),
    })
}

#[test]
fn config_is_closed_and_state_never_lives_inside_the_vault() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let mut extra = config_json(&e, &p, &temp_dir("closed"));
    extra["password"] = json!("x");
    assert!(serde_json::from_value::<service::ServiceConfig>(extra).is_err());
    let inside: service::ServiceConfig =
        serde_json::from_value(config_json(&e, &p, &e.root.path().join("oauth"))).unwrap();
    let listener = http1::bind_loopback("127.0.0.1:0".parse().unwrap()).unwrap();
    let error = service::serve(
        v.clone(),
        &inside,
        &listener,
        &*e.clock,
        &OsIdSource,
        || false,
    )
    .unwrap_err();
    assert!(error.contains("inside the Vault"), "{error}");
    let mut http: Value = config_json(&e, &p, &temp_dir("http"));
    http["issuer"] = json!("http://memory.example");
    let http: service::ServiceConfig = serde_json::from_value(http).unwrap();
    assert!(service::approve(&http, &OsIdSource, &*e.clock).is_err());
}

#[test]
fn chatgpt_style_flow_reads_the_vault_and_survives_permit_renewal() {
    run_read_flow(false);
}

#[test]
fn read_check_reuses_read_gates_without_listening_or_creating_oauth_state() {
    let (e, v, mut p, _) = correction_rig(SpeakerRole::User);
    let files = Env::new();
    let state_dir = files.path.join("check-oauth");
    let listener = http1::bind_loopback("127.0.0.1:0".parse().unwrap()).unwrap();
    let mut raw = config_json(&e, &p, &state_dir);
    raw["bind"] = json!(listener.local_addr().unwrap().to_string());
    let config: service::ServiceConfig = serde_json::from_value(raw.clone()).unwrap();
    let before = v.snapshot().unwrap();
    let checked = service::read_check(v.clone(), &config, &*e.clock).unwrap();
    assert_eq!(checked["authority_mode"], "current_formation_policy");
    assert_eq!(checked["snapshot"], json!(before));
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(!state_dir.exists());

    let mut grant = read_grant(&e, &p);
    grant.source_read = false;
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    p.active = false;
    p.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    v.set_policy(&e.descriptor.created_by, p).unwrap();
    assert!(service::read_check(v.clone(), &config, &*e.clock).is_err());
    raw["read_grant_id"] = json!(grant.grant_id);
    raw["source_ids"] = json!(grant.source_ids);
    raw["source_read"] = json!(false);
    e.clock.set(grant.valid_until.unix_ms() - 60_000);
    let config: service::ServiceConfig = serde_json::from_value(raw.clone()).unwrap();
    let before = v.snapshot().unwrap();
    let checked = service::read_check(v.clone(), &config, &*e.clock).unwrap();
    assert_eq!(checked["read_ready"], true);
    assert_eq!(checked["authority_mode"], "owner_read_grant");
    assert_eq!(checked["scope"]["valid_until"], json!(grant.valid_until));
    assert!(!checked.to_string().contains("moonlight"));
    for change in 0..8 {
        let mut wrong = raw.clone();
        match change {
            0 => wrong["source_read"] = json!(true),
            1 => wrong["agent_id"] = json!(id("prn", 99)),
            2 => wrong["source_ids"] = json!([id("src", 99)]),
            3 => wrong["read_grant_id"] = json!(id("pol", 99)),
            4 => wrong["issuer"] = json!("http://memory.example"),
            5 => wrong["bind"] = json!("0.0.0.0:8787"),
            6 => wrong["state_dir"] = json!(e.root.path().join("oauth")),
            _ => wrong["profile"]["max_items"] = json!(0),
        }
        let wrong: service::ServiceConfig = serde_json::from_value(wrong).unwrap();
        assert!(
            service::read_check(v.clone(), &wrong, &*e.clock).is_err(),
            "change {change}"
        );
    }
    e.clock.set(grant.valid_until.unix_ms());
    assert!(service::read_check(v.clone(), &config, &*e.clock).is_err());
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(!state_dir.exists());
    assert!(!e.root.path().join("oauth").exists());
}
#[test]
fn explicit_grant_serves_old_memories_after_formation_stops_and_oauth_revocation_stops_old_session()
{
    run_read_flow(true);
}
fn run_read_flow(independent: bool) {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let state_dir = temp_dir(if independent { "grant-flow" } else { "flow" });
    let mut raw = config_json(&e, &p, &state_dir);
    if independent {
        let g = read_grant(&e, &p);
        let plan = v
            .plan_owner_read_grant(&e.descriptor.created_by, g.clone())
            .unwrap();
        v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
            .unwrap();
        raw["read_grant_id"] = json!(g.grant_id);
        raw["source_ids"] = json!(g.source_ids);
        let mut stopped = p.clone();
        stopped.active = false;
        stopped.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
        v.set_policy(&e.descriptor.created_by, stopped).unwrap();
        e.clock.set(p.valid_until.unix_ms());
    }
    let config: service::ServiceConfig = serde_json::from_value(raw.clone()).unwrap();
    let listener = http1::bind_loopback("127.0.0.1:0".parse().unwrap()).unwrap();
    let addr = listener.local_addr().unwrap();
    let before = v.snapshot().unwrap();
    let stop = AtomicBool::new(false);
    let clock = e.clock.clone();
    std::thread::scope(|scope| {
        let client = scope.spawn(|| {
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let json_headers = [("Content-Type", "application/json".to_owned())];
                let form_headers = [("Content-Type", "application/x-www-form-urlencoded".to_owned())];
                assert_eq!(send(addr, "GET", "/.well-known/oauth-protected-resource/mcp", &[], "").json()["authorization_servers"], json!(["https://memory.example"]));
                assert_eq!(send(addr, "GET", "/.well-known/oauth-authorization-server", &[], "").json()["registration_endpoint"], "https://memory.example/oauth/register");
                let registered = send(addr, "POST", "/oauth/register", &json_headers, &json!({"redirect_uris":[REDIRECT]}).to_string());
                assert_eq!(registered.status, 201);
                let client_id = registered.json()["client_id"].as_str().unwrap().to_owned();
                // The approval command runs beside the server and shares the locked state file.
                let approver: service::ServiceConfig = serde_json::from_value(raw.clone()).unwrap();
                let code = service::approve(&approver, &OsIdSource, &*clock).unwrap();
                assert_eq!(code.len(), 10);
                let challenge = challenge(VERIFIER);
                let params = [("response_type", "code"), ("client_id", client_id.as_str()), ("redirect_uri", REDIRECT), ("code_challenge", challenge.as_str()), ("code_challenge_method", "S256"), ("state", "s1"), ("approval_code", code.as_str())];
                let approved = send(addr, "POST", "/oauth/authorize", &form_headers, &form(&params));
                assert_eq!(approved.status, 302, "{}", String::from_utf8_lossy(&approved.body));
                let location = approved.header("location").unwrap();
                let auth_code = location.split("code=").nth(1).unwrap().split('&').next().unwrap().to_owned();
                let tokens = send(addr, "POST", "/oauth/token", &form_headers, &form(&[("grant_type", "authorization_code"), ("code", &auth_code), ("redirect_uri", REDIRECT), ("client_id", &client_id), ("code_verifier", VERIFIER)])).json();
                let access = tokens["access_token"].as_str().unwrap().to_owned();
                assert_eq!(mcp(addr, "forged", None, json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{}})).status, 401);
                let session = initialize(addr, &access);
                let tools = mcp(addr, &access, Some(&session), json!({"jsonrpc":"2.0","id":2,"method":"tools/list"})).json();
                assert_eq!(tools["result"]["tools"].as_array().unwrap().len(), 6);
                let start = mcp(addr, &access, Some(&session), json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"session_start","arguments":{"schema_version":2,"kind":"agent_session_start_request","capability":enouia_memory_host::mcp::opening::SESSION_CAPABILITY,"request":{"request_id":id("req",171),"at":clock.now_unix_ms_timestamp()}}}})).json();
                assert_eq!(start["result"]["structuredContent"]["kind"], "agent_session_start_response", "{start}");
                // Past the renewal point the server swaps the Vault permit: old
                // sessions end and the same token opens a new one.
                clock.set(clock.now_unix_ms() + service::RENEW_MS + 1);
                std::thread::sleep(std::time::Duration::from_millis(300));
                assert_eq!(mcp(addr, &access, Some(&session), json!({"jsonrpc":"2.0","id":4,"method":"tools/list"})).status, 404);
                let renewed = initialize(addr, &access);
                assert_ne!(renewed, session);
                if independent {
                    let status = service::oauth_admin(&approver, None, &OsIdSource, &*clock).unwrap();
                    assert_eq!(status["clients"][0]["client_id"], client_id);
                    assert!(service::oauth_admin(&approver, Some(&client_id), &OsIdSource, &*clock).unwrap()["revoked"].as_bool().unwrap());
                    assert_eq!(mcp(addr, &access, Some(&renewed), json!({"jsonrpc":"2.0","id":5,"method":"tools/list"})).status, 401);
                    let refresh = send(addr, "POST", "/oauth/token", &form_headers, &form(&[("grant_type", "refresh_token"), ("client_id", &client_id), ("refresh_token", tokens["refresh_token"].as_str().unwrap())]));
                    assert!(refresh.status >= 400);
                }
                let saved = std::fs::read_to_string(state_dir.join("oauth-state.json")).unwrap();
                for secret in [code.as_str(), auth_code.as_str(), access.as_str(), tokens["refresh_token"].as_str().unwrap()] {
                    assert!(!saved.contains(secret));
                }
            }));
            stop.store(true, Ordering::SeqCst);
            result
        });
        let served = service::serve(
            v.clone(),
            &config,
            &listener,
            &*e.clock,
            &OsIdSource,
            || stop.load(Ordering::SeqCst),
        );
        if let Err(panic) = client.join().unwrap() {
            std::panic::resume_unwind(panic);
        }
        served.unwrap();
    });
    assert_eq!(v.snapshot().unwrap(), before);
    let _ = std::fs::remove_dir_all(&state_dir);
}

trait Stamp {
    fn now_unix_ms_timestamp(&self) -> enouia_memory_contract::time::Timestamp;
}
impl Stamp for enouia_memory_contract::foundation::FakeClock {
    fn now_unix_ms_timestamp(&self) -> enouia_memory_contract::time::Timestamp {
        enouia_memory_contract::time::Timestamp::from_unix_ms(self.now_unix_ms()).unwrap()
    }
}

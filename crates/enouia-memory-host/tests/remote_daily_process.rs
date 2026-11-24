#![cfg(any(windows, target_os = "linux"))]
//! The ChatGPT connector's daily flow against the real `enouia-memory-remote`
//! process on loopback: a synthetic segmented Owner Vault, an Owner read grant
//! labelled `chatgpt_connector` that follows new windows, and the connector
//! section enabled. The approval code comes from the `approve` command run as
//! a separate process, as Morii would run it beside the service.
#[path = "support/remote_client.rs"]
mod client;
#[path = "support/owner_read_grant_fixture.rs"]
mod fixture;
use client::*;
use enouia_memory_contract::foundation::Clock;
use enouia_memory_contract::{common::*, next::*, time::Timestamp};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, SystemClock, VerifiedRoot,
    automation::segmented::{
        SegmentedAutomationVault, SystemReadScope,
        backup::{activate_owner, export_owner, restore_owner, verify_owner},
    },
    fault::Faults,
    verify_data_root,
};
use fixture::*;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::sync::Arc;

const EXE: &str = env!("CARGO_BIN_EXE_enouia-memory-remote");

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis() as i64
}
fn memory_ids(value: &Value, out: &mut BTreeSet<String>) {
    match value {
        Value::Object(map) => {
            for (key, v) in map {
                match (key.as_str(), v) {
                    ("memory_id", Value::String(id)) => {
                        out.insert(id.clone());
                    }
                    _ => memory_ids(v, out),
                }
            }
        }
        Value::Array(items) => items.iter().for_each(|v| memory_ids(v, out)),
        _ => {}
    }
}
fn conversation(n: u32, text: &str, quote: &str) -> Value {
    let cases: Value = value("runtime-owner-capture/cases.json");
    let mut request = cases["base"].clone();
    request["request_id"] = json!(id("req", 700 + n));
    request["conversation_id"] = json!(format!("chatgpt-synthetic-{n}"));
    request["turns"][0]["text"] = json!(text);
    request["turns"][0]["attachments"] = json!([]);
    let unknown = json!({"from":null,"until":null,"precision":"unknown","bound":"within","tz":null,"basis":"unknown"});
    json!({"request": request, "analyst_model": "synthetic-chatgpt-model", "statements": [{
        "message_id": "m1", "quote": quote, "title": quote,
        "interpretation": "clear_self_report", "kind": "preference",
        "event_time": unknown, "valid_time": unknown, "note": null,
        "entities": [{"name": "Lumen Cafe", "entity_type": "place", "reality": "real", "role": "mentioned"}]
    }]})
}

fn memories(e: &Env, v: &SegmentedAutomationVault) -> usize {
    v.read_scoped(
        &e.descriptor.system_actor,
        Sensitivity::Private,
        SystemReadScope::MoriiFullHistory,
        |d| d.memories.len(),
    )
    .unwrap()
}

#[test]
fn chatgpt_daily_flow_through_the_real_remote_process() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let before = memories(&e, &v);
    // One process opens the Vault at a time: close it before serving.
    drop(v);
    daily_flow(&e, &p, before, &e.root);
}

/// ADR-MEM-177: the move to a VPS is export, copy, restore and activate. The
/// restored root, opened by the real process, serves the same flow.
#[test]
fn a_restored_and_activated_root_serves_the_daily_flow() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let before = memories(&e, &v);
    drop(v);
    let files = Env::new();
    let dir = |name: &str| {
        let path = files.path.join(name);
        std::fs::create_dir(&path).unwrap();
        verify_data_root(&path, &RootPolicy::default()).unwrap()
    };
    let (export, target) = (dir("export"), dir("restored"));
    let clock: Arc<dyn Clock + Send + Sync> = Arc::new(SystemClock);
    let exported = export_owner(&e.root, &export, clock.clone(), Arc::new(OsIdSource)).unwrap();
    assert_eq!(verify_owner(&export).unwrap().head, exported.head);
    restore_owner(&export, &target, &*clock).unwrap();
    // A restore stays gated until Morii activates it.
    assert!(
        SegmentedAutomationVault::open(
            &target,
            e.descriptor.host_id.clone(),
            clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .is_err()
    );
    activate_owner(&target, &export, &*clock).unwrap();
    daily_flow(&e, &p, before, &target);
}

fn daily_flow(e: &Env, p: &AutomationPolicy, before: usize, root: &VerifiedRoot) {
    // Seeds for the config: one policy and its sources. The drafted grant
    // follows the subject's other and later windows.
    let seed = read_grant(e, p);
    let full = SystemReadScope::MoriiFullHistory;

    let files = Env::new();
    let state_dir = files.path.join("oauth");
    let addr: SocketAddr = {
        let probe = TcpListener::bind("127.0.0.1:0").unwrap();
        probe.local_addr().unwrap()
    };
    let config = json!({
        "vault_root": root.path(),
        "host_id": e.descriptor.host_id,
        "state_dir": state_dir,
        "bind": addr.to_string(),
        "resource": "https://memory.example/mcp",
        "issuer": "https://memory.example",
        "redirect_prefixes": ["https://chatgpt.com/"],
        "origins": ["https://chatgpt.com"],
        "agent_id": seed.principal.actor_id,
        "subject_id": seed.subject_id,
        "project_id": seed.project_id,
        "policy_ids": [p.policy_id],
        "source_ids": seed.source_ids,
        "max_sensitivity": "private",
        "source_read": true,
        "profile": value("attention/profile.json"),
        "connector": {
            "producer_id": id("prn", 6),
            "config": {
                "account_scope": "chatgpt-connector",
                "default_timezone": "Asia/Shanghai",
                "max_sensitivity": "private",
                "allowed_forms": ["fact", "preference", "episode", "open_loop"],
                "window_days": 30
            }
        }
    });
    let config_path = files.path.join("remote.json");
    std::fs::write(&config_path, config.to_string()).unwrap();
    let cli = |args: &[&str]| -> Value {
        let out = Command::new(EXE)
            .arg(args[0])
            .arg(&config_path)
            .args(&args[1..])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
        serde_json::from_slice(&out.stdout).unwrap()
    };
    // Morii's grant: drafted from the config, reviewed, then confirmed.
    let expiry = Timestamp::from_unix_ms(now_ms() + 2 * 86_400_000).unwrap();
    let plan_path = files.path.join("plan.json");
    let drafted = cli(&["read-draft", expiry.as_str(), plan_path.to_str().unwrap()]);
    let plan: Value = serde_json::from_slice(&std::fs::read(&plan_path).unwrap()).unwrap();
    let grant = &plan["grant"];
    assert_eq!(drafted["grant_id"], grant["grant_id"]);
    // An existing plan file is never overwritten.
    let again = Command::new(EXE)
        .arg("read-draft")
        .arg(&config_path)
        .args([expiry.as_str(), plan_path.to_str().unwrap()])
        .output()
        .unwrap();
    assert!(!again.status.success());
    assert_eq!(grant["follow_new_sources"], true);
    assert_eq!(grant["purpose"], "chatgpt_connector");
    // ADR-MEM-184: requests are allowed because source text may be read.
    assert_eq!(grant["allow_requests"], true);
    assert_eq!(
        grant["principal"]["actor_id"],
        json!(seed.principal.actor_id)
    );
    assert_eq!(grant["policies"], json!([p.reference()]));
    assert_eq!(grant["source_ids"], json!(seed.source_ids));
    assert_eq!(grant["valid_until"], json!(expiry));
    assert_eq!(
        cli(&["read-status"])["grants"],
        json!([]),
        "a draft writes nothing"
    );
    cli(&[
        "read-set",
        plan_path.to_str().unwrap(),
        "--confirm-owner-read",
    ]);
    let mut config = config;
    config["read_grant_id"] = grant["grant_id"].clone();
    std::fs::write(&config_path, config.to_string()).unwrap();
    assert_eq!(cli(&["read-check"])["authority_mode"], "owner_read_grant");

    let mut server = Server(
        Command::new(EXE)
            .args(["serve".as_ref(), config_path.as_os_str()])
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    // ADR-MEM-189: drain the access log while the service runs.
    let mut stderr = server.0.stderr.take().unwrap();
    let log = std::thread::spawn(move || {
        let mut text = String::new();
        std::io::Read::read_to_string(&mut stderr, &mut text).unwrap();
        text
    });
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(20);
    while TcpStream::connect(addr).is_err() {
        assert!(
            std::time::Instant::now() < deadline,
            "service did not listen"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    // A query string never reaches the access log.
    send(
        addr,
        "GET",
        "/oauth/authorize?state=query-marker-77",
        &[],
        "",
    );
    // OAuth as ChatGPT does it, with Morii's approval code from the CLI.
    let json_headers = [("Content-Type", "application/json".to_owned())];
    let form_headers = [(
        "Content-Type",
        "application/x-www-form-urlencoded".to_owned(),
    )];
    let resource = send(
        addr,
        "GET",
        "/.well-known/oauth-protected-resource/mcp",
        &[],
        "",
    )
    .json();
    assert!(
        resource["scopes_supported"]
            .as_array()
            .unwrap()
            .contains(&json!("memory.write"))
    );
    let registered = send(
        addr,
        "POST",
        "/oauth/register",
        &json_headers,
        &json!({"redirect_uris":[REDIRECT]}).to_string(),
    );
    assert_eq!(registered.status, 201);
    let client_id = registered.json()["client_id"].as_str().unwrap().to_owned();
    let approved = Command::new(EXE)
        .args(["approve".as_ref(), config_path.as_os_str()])
        .output()
        .unwrap();
    assert!(approved.status.success());
    let code = String::from_utf8(approved.stdout)
        .unwrap()
        .trim()
        .to_owned();
    let challenge = challenge(VERIFIER);
    let authorized = send(
        addr,
        "POST",
        "/oauth/authorize",
        &form_headers,
        &form(&[
            ("response_type", "code"),
            ("client_id", &client_id),
            ("redirect_uri", REDIRECT),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
            ("state", "s1"),
            ("scope", "memory.read memory.write"),
            ("approval_code", &code),
        ]),
    );
    assert_eq!(
        authorized.status,
        302,
        "{}",
        String::from_utf8_lossy(&authorized.body)
    );
    let location = authorized.header("location").unwrap();
    let auth_code = location
        .split("code=")
        .nth(1)
        .unwrap()
        .split('&')
        .next()
        .unwrap()
        .to_owned();
    let tokens = send(
        addr,
        "POST",
        "/oauth/token",
        &form_headers,
        &form(&[
            ("grant_type", "authorization_code"),
            ("code", &auth_code),
            ("redirect_uri", REDIRECT),
            ("client_id", &client_id),
            ("code_verifier", VERIFIER),
        ]),
    )
    .json();
    let token = tokens["access_token"].as_str().unwrap().to_owned();
    let session = initialize(addr, &token);
    let mut next = 10;
    let mut rpc = |method: &str, params: Value| {
        next += 1;
        let reply = mcp(
            addr,
            &token,
            Some(&session),
            json!({"jsonrpc":"2.0","id":next,"method":method,"params":params}),
        );
        assert_eq!(
            reply.status,
            200,
            "{}",
            String::from_utf8_lossy(&reply.body)
        );
        reply.json()["result"].clone()
    };
    let listed = rpc("tools/list", json!({}))["tools"].clone();
    let tools: BTreeSet<String> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    // ADR-MEM-178/180: ChatGPT sees flat read inputs, with no `$ref` to
    // resolve, and a description of what each tool is for.
    let descriptions: BTreeSet<_> = listed
        .as_array()
        .unwrap()
        .iter()
        .map(|t| t["description"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(descriptions.len(), tools.len(), "one description per tool");
    for tool in listed.as_array().unwrap() {
        assert!(
            !tool["inputSchema"].to_string().contains("$ref"),
            "{}",
            tool["name"]
        );
    }
    for name in [
        "memory_search",
        "memory_read",
        "memory_submit_conversation",
        "memory_submit_theme",
        "memory_request_correction",
        "memory_request_forget",
    ] {
        assert!(tools.contains(name), "{name} in {tools:?}");
    }
    let mut search_n = 900;
    let mut search = |rpc: &mut dyn FnMut(&str, Value) -> Value| {
        search_n += 1;
        let result = rpc(
            "tools/call",
            json!({"name":"memory_search","arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":enouia_memory_host::mcp::initialized::CAPABILITY,"request_id":id("req",search_n),"operation":"memory_search","arguments":{"filter":CatalogFilter::default(),"cursor":null,"limit":100,"max_bytes":262144,"include_facets":false}}}),
        );
        assert_ne!(result["isError"], json!(true), "{result}");
        let mut ids = BTreeSet::new();
        memory_ids(&result["structuredContent"], &mut ids);
        ids
    };
    let short_search = |rpc: &mut dyn FnMut(&str, Value) -> Value| {
        let result = rpc(
            "tools/call",
            json!({"name":"memory_search","arguments":{"query":"","limit":100}}),
        );
        assert_ne!(result["isError"], json!(true), "{result}");
        let mut ids = BTreeSet::new();
        memory_ids(&result["structuredContent"], &mut ids);
        ids
    };
    let baseline = search(&mut rpc);
    assert!(!baseline.is_empty());
    assert_eq!(short_search(&mut rpc), baseline);
    let short = |rpc: &mut dyn FnMut(&str, Value) -> Value, name: &str, arguments: Value| {
        rpc("tools/call", json!({"name": name, "arguments": arguments}))
    };
    let started = short(&mut rpc, "session_start", json!({}));
    assert_eq!(
        started["structuredContent"]["kind"], "agent_session_start_response",
        "{started}"
    );
    let unknown = short(
        &mut rpc,
        "memory_search",
        json!({"query": "x", "colour": "red"}),
    );
    assert_eq!(unknown["isError"], true);
    assert_eq!(unknown["structuredContent"]["error"], "invalid_arguments");
    let submit = |rpc: &mut dyn FnMut(&str, Value) -> Value, submission: Value, preflight: bool| {
        let result = rpc(
            "tools/call",
            json!({"name":"memory_submit_conversation","arguments":{"submission":submission,"preflight":preflight}}),
        );
        assert_ne!(result["isError"], json!(true), "{result}");
        result["structuredContent"].clone()
    };
    // Preflight writes nothing and says a window would open.
    let first = conversation(1, "I like the window seat at Lumen Cafe.", "window seat");
    let checked = submit(&mut rpc, first.clone(), true);
    assert_eq!(checked["window_needed"], true, "{checked}");
    assert_eq!(search(&mut rpc), baseline);
    let formed = submit(&mut rpc, first.clone(), false);
    assert_eq!(formed["jobs"][0]["state"], "formed", "{formed}");
    // ADR-MEM-183: the receipt names the memory it formed; a replay names it too.
    let first_memories = formed["memories"].clone();
    assert_eq!(first_memories.as_array().unwrap().len(), 1, "{formed}");
    assert_eq!(first_memories[0]["title"], "window seat");
    let replayed = submit(&mut rpc, first, false);
    assert_eq!(replayed["memories"], first_memories);
    assert!(
        replayed["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|j| j["receipts"].as_u64() <= Some(1))
    );
    // The short form: a free-form request key, three fields per turn and
    // no times. The same key replays.
    let second = json!({
        "request": {
            "request_id": "chatgpt-2026-11-24-cafe",
            "conversation_id": "chatgpt-synthetic-2",
            "turns": [
                {"message_id": "a0", "speaker": "assistant", "text": "Where do you like to sit?"},
                {"message_id": "m1", "speaker": "user", "text": "Lumen Cafe has the quietest window seat."}
            ]
        },
        "analyst_model": "synthetic-chatgpt-model",
        "statements": [{
            "message_id": "m1", "quote": "quietest window seat", "title": "quietest window seat",
            "interpretation": "clear_self_report", "kind": "preference"
        }]
    });
    let formed = submit(&mut rpc, second.clone(), false);
    let states: Vec<_> = formed["jobs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|j| j["state"].clone())
        .collect();
    assert!(states.contains(&json!("formed")), "{formed}");
    let second_memories = formed["memories"].clone();
    assert_eq!(second_memories.as_array().unwrap().len(), 1, "{formed}");
    let again = submit(&mut rpc, second.clone(), false);
    assert!(
        again["jobs"]
            .as_array()
            .unwrap()
            .iter()
            .all(|j| j["receipts"].as_u64() <= Some(1)),
        "{again}"
    );
    // ADR-MEM-192: the next day's call of the same chat, numbered from 1
    // again by the short form, continues the chat and replays.
    let next_day = json!({
        "request": {
            "request_id": "chatgpt-2026-11-25-cafe",
            "conversation_id": "chatgpt-synthetic-2",
            "turns": [{"message_id": "2026-11-25-1", "speaker": "user", "text": "Still the window seat for me.", "occurred_at": "2026-11-25T09:00:00+08:00"}]
        },
        "analyst_model": "synthetic-chatgpt-model",
        "statements": []
    });
    let continued = submit(&mut rpc, next_day.clone(), false);
    assert_eq!(continued["capture"]["captured"], true, "{continued}");
    let replayed = submit(&mut rpc, next_day, false);
    assert_eq!(replayed["capture"]["replayed"], true, "{replayed}");
    // ADR-MEM-193: the same request_id with changed content is explained.
    let mut changed = second.clone();
    changed["request"]["turns"][1]["text"] =
        json!("Lumen Cafe has the quietest window seat, and good tea.");
    let refused = short(
        &mut rpc,
        "memory_submit_conversation",
        json!({"submission": changed}),
    );
    assert_eq!(refused["isError"], true, "{refused}");
    assert!(
        refused["structuredContent"]["problems"]
            .to_string()
            .contains("request_id was already used"),
        "{refused}"
    );
    // ADR-MEM-181: a refused submission says what to fix.
    let bad = json!({"submission": {
        "request": {"request_id": "bad-1", "conversation_id": "chatgpt-bad", "turns": [
            {"message_id": "m1", "speaker": "user", "text": "I drink tea in the morning."}
        ]},
        "analyst_model": "synthetic-chatgpt-model",
        "statements": [{"message_id": "m1", "quote": "coffee", "title": "coffee", "interpretation": "clear_self_report", "kind": "theme"}]
    }});
    let refused = short(&mut rpc, "memory_submit_conversation", bad);
    assert_eq!(refused["isError"], true);
    let problems = refused["structuredContent"]["problems"].to_string();
    assert!(
        problems.contains("verbatim") && problems.contains("kind is not allowed"),
        "{refused}"
    );
    // The following grant sees the connector's new window on the next read.
    let after: BTreeSet<_> = search(&mut rpc).difference(&baseline).cloned().collect();
    let receipted: BTreeSet<_> = [&first_memories[0], &second_memories[0]]
        .iter()
        .map(|m| m["memory_id"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(receipted, after);
    assert_eq!(after.len(), 2, "{after:?}");
    // ADR-MEM-182: `days` keeps memories learned recently; the fixture's
    // history was learned a month before the system clock.
    let recent = short(&mut rpc, "memory_search", json!({"days": 1, "limit": 100}));
    assert_ne!(recent["isError"], json!(true), "{recent}");
    let mut recent_ids = BTreeSet::new();
    memory_ids(&recent["structuredContent"], &mut recent_ids);
    assert_eq!(recent_ids, after);
    assert_eq!(
        short(&mut rpc, "memory_search", json!({"days": 0}))["isError"],
        true
    );

    let theme = json!({
        "request_id": "weekly-theme-window-seats",
        "analyst_model": "synthetic-chatgpt-model",
        "title": "window seats",
        "summary": "Morii keeps returning to quiet window seats at the same cafe.",
        "members": after.iter().collect::<Vec<_>>(),
        "note": null
    });
    let submit_theme = |rpc: &mut dyn FnMut(&str, Value) -> Value, preflight: bool| {
        let result = rpc(
            "tools/call",
            json!({"name":"memory_submit_theme","arguments":{"theme":theme.clone(),"preflight":preflight}}),
        );
        assert_ne!(result["isError"], json!(true), "{result}");
        result["structuredContent"].clone()
    };
    // Members must be the connector's own current memories.
    let outside = [
        baseline.iter().next().unwrap(),
        after.iter().next().unwrap(),
    ];
    let refused = short(
        &mut rpc,
        "memory_submit_theme",
        json!({"theme": {"request_id": "bad-theme", "analyst_model": "m", "title": "t", "summary": "s", "members": outside}}),
    );
    assert_eq!(refused["isError"], true);
    assert!(
        refused["structuredContent"]["problems"]
            .to_string()
            .contains("this connector submitted"),
        "{refused}"
    );
    assert_eq!(submit_theme(&mut rpc, true)["memory"], Value::Null);
    let themed = submit_theme(&mut rpc, false);
    let theme_id = themed["memory"]["memory_id"].as_str().unwrap().to_owned();
    assert_eq!(themed["replayed"], false);
    let again = submit_theme(&mut rpc, false);
    assert_eq!(
        (
            again["replayed"].clone(),
            again["memory"]["memory_id"].as_str()
        ),
        (json!(true), Some(theme_id.as_str()))
    );
    assert!(search(&mut rpc).contains(&theme_id));
    // A theme over these memories is not listed as formed from the turns.
    assert_eq!(submit(&mut rpc, second, false)["memories"], second_memories);
    let read = short(&mut rpc, "memory_read", json!({"memory_id": theme_id}));
    assert_ne!(read["isError"], json!(true), "{read}");
    let recalled = short(&mut rpc, "memory_recall", json!({"text": "window seat"}));
    assert_ne!(recalled["isError"], json!(true), "{recalled}");
    let related = short(&mut rpc, "memory_related", json!({"memory_id": theme_id}));
    assert_ne!(related["isError"], json!(true), "{related}");

    // ADR-MEM-184: Morii asks to correct one memory and forget another. The
    // conversation saying so is submitted first; its receipt names the
    // turn's source, which the requests cite. Nothing changes until Morii
    // confirms them in Runtime.
    let asked = submit(
        &mut rpc,
        json!({
            "request": {"request_id": "chatgpt-fix-1", "conversation_id": "chatgpt-synthetic-3", "turns": [
                {"message_id": "m1", "speaker": "user", "text": "Actually I prefer the corner seat now. And please forget the cafe one."}
            ]},
            "analyst_model": "synthetic-chatgpt-model",
            "statements": []
        }),
        false,
    );
    let source = asked["turns"][0]["source"]["source_id"].clone();
    assert!(source.is_string(), "{asked}");
    let first_id = first_memories[0]["memory_id"].clone();
    let second_id = second_memories[0]["memory_id"].clone();
    let corrected = short(
        &mut rpc,
        "memory_request_correction",
        json!({"request_id": "fix-seat", "memory_id": first_id, "source_id": source, "quote": "I prefer the corner seat now", "correction": "Morii now prefers the corner seat."}),
    );
    assert_ne!(corrected["isError"], json!(true), "{corrected}");
    let again = short(
        &mut rpc,
        "memory_request_correction",
        json!({"request_id": "fix-seat", "memory_id": first_id, "source_id": source, "quote": "I prefer the corner seat now", "correction": "Morii now prefers the corner seat."}),
    );
    assert_eq!(
        again["structuredContent"]["report_key"], corrected["structuredContent"]["report_key"],
        "{again}"
    );
    let forgot = short(
        &mut rpc,
        "memory_request_forget",
        json!({"request_id": "forget-cafe", "memory_id": second_id, "source_id": source, "quote": "please forget the cafe one", "reason": "Morii asked to forget it."}),
    );
    assert_ne!(forgot["isError"], json!(true), "{forgot}");
    let unquoted = short(
        &mut rpc,
        "memory_request_forget",
        json!({"request_id": "forget-bad", "memory_id": second_id, "source_id": source, "quote": "words Morii never said", "reason": "x"}),
    );
    assert_eq!(unquoted["isError"], true, "{unquoted}");
    // Requests change no memory: both still read as before.
    assert!(search(&mut rpc).is_superset(&after));

    // ADR-MEM-188: the nightly backup refuses while the service holds the
    // Vault, and leaves no directory behind.
    let backups = files.path.join("backups");
    std::fs::create_dir(&backups).unwrap();
    let busy = Command::new(EXE)
        .args([
            "backup".as_ref(),
            config_path.as_os_str(),
            backups.as_os_str(),
        ])
        .output()
        .unwrap();
    assert!(!busy.status.success());
    assert!(String::from_utf8_lossy(&busy.stderr).contains("Busy"));
    assert_eq!(std::fs::read_dir(&backups).unwrap().count(), 0);
    // Admin commands say plainly that the running service holds the Vault.
    let held = Command::new(EXE)
        .args(["read-status".as_ref(), config_path.as_os_str()])
        .output()
        .unwrap();
    assert!(!held.status.success());
    assert!(
        String::from_utf8_lossy(&held.stderr).contains("another process holds the Vault"),
        "{}",
        String::from_utf8_lossy(&held.stderr)
    );

    // Stop the service (killed, as `systemctl stop` would end it) and back up.
    drop(server);
    let log = log.join().unwrap();
    assert!(
        log.contains(" serving ") && log.contains("writes on"),
        "{log}"
    );
    assert!(log.contains("GET /oauth/authorize "), "{log}");
    assert!(log.contains("POST /mcp 200 "), "{log}");
    assert!(log.contains("rpc=tools/call tool=memory_search"), "{log}");
    assert!(
        log.lines()
            .any(|l| l.ends_with("tool=memory_request_forget error")),
        "{log}"
    );
    for private in [
        "query-marker-77",
        "Lumen",
        "corner seat",
        code.as_str(),
        auth_code.as_str(),
        token.as_str(),
    ] {
        assert!(!log.contains(private), "{private} in {log}");
    }
    let backup = Command::new(EXE)
        .args([
            "backup".as_ref(),
            config_path.as_os_str(),
            backups.as_os_str(),
        ])
        .output()
        .unwrap();
    assert!(
        backup.status.success(),
        "{}",
        String::from_utf8_lossy(&backup.stderr)
    );
    let backup: Value = serde_json::from_slice(&backup.stdout).unwrap();
    assert_eq!(backup["verified"], true);
    let exported = verify_data_root(
        std::path::Path::new(backup["export"].as_str().unwrap()),
        &RootPolicy::default(),
    )
    .unwrap();
    let head = verify_owner(&exported).unwrap().head;
    assert_eq!(json!(head.sequence), backup["sequence"]);

    let reopened = SegmentedAutomationVault::open(
        root,
        e.descriptor.host_id.clone(),
        Arc::new(SystemClock),
        Arc::new(OsIdSource),
        Faults::none(),
    )
    .unwrap();
    let (count, theme) = reopened
        .read_scoped(
            &e.descriptor.system_actor,
            Sensitivity::Private,
            full,
            |d| {
                let theme = d
                    .memories
                    .iter()
                    .find(|m| m.memory.memory.memory_id.as_str() == theme_id)
                    .map(|m| {
                        let i = &m.memory.decision.input;
                        (
                            i.interpretation,
                            i.dimensions.as_ref().map(|f| f.derived_from.len()),
                        )
                    });
                (d.memories.len(), theme)
            },
        )
        .unwrap();
    assert_eq!(count, before + 3);
    // Both requests wait in Morii's queue, still pending.
    let queued = reopened
        .agent_requests(&e.descriptor.created_by, Sensitivity::Private)
        .unwrap();
    let kinds: Vec<_> = queued
        .iter()
        .map(|r| (serde_json::to_value(r.kind).unwrap(), r.pending))
        .collect();
    assert_eq!(
        kinds,
        vec![(json!("correction"), true), (json!("forget"), true)]
    );
    assert_eq!(theme, Some((Interpretation::Inference, Some(2))));
    let saved = std::fs::read_to_string(state_dir.join("oauth-state.json")).unwrap();
    for secret in [code.as_str(), auth_code.as_str(), token.as_str()] {
        assert!(!saved.contains(secret));
    }
}

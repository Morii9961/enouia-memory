#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod support;
use enouia_memory_contract::{
    common::*,
    hash::{Sha256Hex, sha256},
    ids::*,
    next::*,
    policy::Scope,
    ports::IdSource,
    time::Timestamp,
    workspace::HostSurface,
};
use enouia_memory_host::{
    automation::{owner::NativeMemoryOwner, qualified_agent::*},
    mcp::{native::Enrollment, qualified as catalog},
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_vault::{OsIdSource, automation::segmented::SegmentedAgentScope, fault::Faults};
use fixture::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    path::PathBuf,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, mpsc},
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(30);
fn env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    e
}
fn agent() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}
fn scope(e: &Env, count: u32, source_read: bool) -> SegmentedAgentScope {
    let p = e.policy(1, 1, count);
    SegmentedAgentScope {
        subject_id: p.subject_id,
        project_id: p.project_id,
        policy_ids: vec![p.policy_id],
        source_ids: (1..=count)
            .map(|n| SourceId::parse(&id("src", n)).unwrap())
            .collect(),
        max_sensitivity: Sensitivity::Private,
        source_read,
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn packet(n: u32, op: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":n,"method":"tools/call","params":{"name":op,"arguments":{"schema_version":2,"kind":"agent_qualified_read_request","capability":catalog::CAPABILITY,"request_id":RequestId::parse(&id("req",n)).unwrap(),"operation":op,"arguments":args}}})
}
fn search_args() -> Value {
    json!({"filter":CatalogFilter::default(),"cursor":null,"limit":100,"max_bytes":262144,"include_facets":true})
}
struct Agent {
    child: Child,
    input: Option<ChildStdin>,
    events: mpsc::Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Agent {
    fn spawn() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "qualified_process_child", "--nocapture"])
            .env("ENOUIA_QUALIFIED_CHILD", "1")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let output = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else { break };
                if let Some((_, body)) = line.split_once("qualified-event ")
                    && tx.send(serde_json::from_str(body).unwrap()).is_err()
                {
                    break;
                }
            }
        });
        Self {
            child,
            input: Some(input),
            events,
            reader: Some(reader),
        }
    }
    fn send(&mut self, v: Value) {
        let input = self.input.as_mut().unwrap();
        writeln!(input, "{v}").unwrap();
        input.flush().unwrap();
    }
    fn event(&self) -> Value {
        self.events
            .recv_timeout(TIMEOUT)
            .expect("owned qualified Agent child response")
    }
    fn rpc(&mut self, v: Value) -> Value {
        self.send(json!({"message":v}));
        self.event()
    }
    fn connect(&mut self, id: &Sha256Hex, vault: &VaultId, mode: &str) {
        self.send(json!({"pipe":id,"host_pid":std::process::id(),"vault_id":vault,"mode":mode}));
    }
    fn initialize(&mut self) {
        assert_eq!(self.rpc(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"claims cannot enroll Owner","version":"0"}}}))["response"]["result"]["protocolVersion"],"2025-11-25");
        assert!(
            self.rpc(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))["response"]
                .is_null()
        );
    }
    fn finish(&mut self) {
        self.send(json!({"exit":true}));
        let until = Instant::now() + TIMEOUT;
        loop {
            if let Some(s) = self.child.try_wait().unwrap() {
                assert!(s.success());
                break;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(r) = self.reader.take() {
            let _ = r.join();
        }
    }
}
fn event(v: Value) {
    println!("qualified-event {v}");
    std::io::stdout().flush().unwrap();
}
#[test]
fn qualified_process_child() {
    if std::env::var_os("ENOUIA_QUALIFIED_CHILD").is_none() {
        return;
    }
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
    let config: Value = serde_json::from_str(&line).unwrap();
    let id = Sha256Hex::parse(config["pipe"].as_str().unwrap()).unwrap();
    let vault = VaultId::parse(config["vault_id"].as_str().unwrap()).unwrap();
    let peer = ProcessPeer::enroll(
        config["host_pid"].as_u64().unwrap() as u32,
        HostSurface::QuickSearch,
    )
    .unwrap();
    let mode = config["mode"].as_str().unwrap();
    if mode == "bad_hash" || mode == "bad_carrier" {
        let mut hello = catalog::native::hello(&vault);
        if mode == "bad_hash" {
            hello["compatibilityHash"] = json!("0".repeat(64));
        } else {
            hello["sourceCarrier"] = json!("captured-source-import-1");
        }
        let mut pipe = Pipe::connect(&id).unwrap();
        peer.verify_server(&pipe).unwrap();
        let body = serde_json::to_vec(&hello).unwrap();
        let mut frame = vec![1u8];
        frame.extend((body.len() as u32).to_le_bytes());
        frame.extend(body);
        pipe.write_all(&frame, Instant::now() + TIMEOUT).unwrap();
        assert!(
            pipe.read_exact(&mut [0u8; 5], Instant::now() + TIMEOUT)
                .is_err()
        );
        event(json!({"version_refused":true}));
        return;
    }
    if mode != "qualified" {
        let refused = match mode {
            "catalog" => enouia_memory_host::mcp::catalog::native::Client::connect(
                Pipe::connect(&id).unwrap(),
                peer,
                &vault,
                TIMEOUT,
            )
            .is_err(),
            "legacy" => enouia_memory_host::mcp::native::Client::connect(
                Pipe::connect(&id).unwrap(),
                peer,
                &vault,
                TIMEOUT,
            )
            .is_err(),
            "v2" => enouia_memory_host::mcp::v2::native::Client::connect(
                Pipe::connect(&id).unwrap(),
                peer,
                &vault,
                TIMEOUT,
            )
            .is_err(),
            "records" => enouia_memory_host::mcp::v2::native::Client::connect_records(
                Pipe::connect(&id).unwrap(),
                peer,
                &vault,
                TIMEOUT,
            )
            .is_err(),
            _ => panic!("invalid test mode"),
        };
        assert!(refused);
        event(json!({"version_refused":true}));
        return;
    }
    let connection =
        catalog::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT);
    let mut client = match connection {
        Ok(c) => c,
        Err(_) => {
            event(json!({"connect_refused":true}));
            return;
        }
    };
    event(json!({"connected":true}));
    loop {
        line.clear();
        if input.read_line(&mut line).unwrap() == 0 {
            break;
        }
        let command: Value = serde_json::from_str(&line).unwrap();
        if command["exit"] == true {
            break;
        }
        match client.exchange(&command["message"]) {
            Ok(response) => event(json!({"response":response})),
            Err(e) => event(
                json!({"failure":format!("{:?}",e.fault),"original":e.original_message,"retained":client.original_unknown()}),
            ),
        }
    }
}
fn bytes(root: &std::path::Path) -> std::collections::BTreeMap<PathBuf, Vec<u8>> {
    fn walk(
        root: &std::path::Path,
        at: &std::path::Path,
        out: &mut std::collections::BTreeMap<PathBuf, Vec<u8>>,
    ) {
        for entry in std::fs::read_dir(at).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let relative = path.strip_prefix(root).unwrap();
                // OS-held lease/transaction lock bytes cannot be opened for a
                // content snapshot. Every business file remains compared.
                if relative == std::path::Path::new("indexes/host.lock")
                    || relative == std::path::Path::new("vault/LOCK")
                {
                    continue;
                }
                out.insert(relative.to_path_buf(), std::fs::read(path).unwrap());
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn local_refs(value: &Value, root: &Value) {
    match value {
        Value::Object(m) => {
            if let Some(r) = m.get("$ref").and_then(Value::as_str) {
                assert!(r.starts_with('#'));
                assert!(root.pointer(&r[1..]).is_some(), "{r}");
            }
            for v in m.values() {
                local_refs(v, root);
            }
        }
        Value::Array(v) => {
            for n in v {
                local_refs(n, root);
            }
        }
        _ => {}
    }
}

#[test]
fn qualified_request_versions_identity_and_scope_refuse_before_dispatch_counts() {
    let cases: Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/agent-qualified-read/manifest.json"
    ))
    .unwrap();
    let schemas = support::schema::SchemaStore::load();
    for packet in cases["valid"].as_array().unwrap() {
        assert!(
            schemas
                .validate("next/agent-qualified-read-v2.schema.json", packet)
                .is_empty()
        );
    }
    for packet in cases["invalid"].as_array().unwrap() {
        assert!(
            !schemas
                .validate("next/agent-qualified-read-v2.schema.json", &packet["value"])
                .is_empty(),
            "{}",
            packet["name"]
        );
    }
    let e = env();
    let vault = Arc::new(e.create(Faults::none()));
    e.populate(&vault, 8);
    let host = QualifiedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
        .unwrap();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 4, true))
        .unwrap();
    let p = profile();
    let mut server = catalog::Server::new(&reader, &p).unwrap();
    server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"Owner System self claim","version":"0"}}}));
    server.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    for (index, case) in cases["invalid"]
        .as_array()
        .unwrap()
        .iter()
        .take(6)
        .enumerate()
    {
        let rpc = json!({"jsonrpc":"2.0","id":index+2,"method":"tools/call","params":{"name":"memory_search","arguments":case["value"]}});
        assert_eq!(server.handle(&rpc).unwrap()["error"]["code"], -32602);
    }
    assert_eq!(server.counts.search, 0);
    assert_eq!(
        server.counts.read + server.counts.source + server.counts.related + server.counts.recall,
        0
    );
}

#[test]
fn real_owner_pipe_exposes_five_schemas_full_fields_and_same_actual_context() {
    let e = env();
    let vault = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&vault, 8);
    let owner =
        NativeMemoryOwner::from_authenticated_owner(vault.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let targets = vault
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            [0, 7].map(|n| {
                data.memories
                    .iter()
                    .find(|m| m.memory.memory == receipts[n].result)
                    .unwrap()
                    .target
                    .clone()
            })
        })
        .unwrap();
    let preview = owner
        .plan(
            OwnerAction::Tag {
                name: "native-visible-tag".into(),
                targets: targets.into(),
            },
            Sensitivity::Private,
            None,
        )
        .unwrap();
    let consent = owner
        .confirm_from_owner_gesture(
            &preview.plan.operation_id,
            &digest(&preview.plan),
            &preview.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    owner.commit(consent).unwrap();
    let host = QualifiedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
        .unwrap();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 4, true))
        .unwrap();
    let p = profile();
    let id = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut child = Agent::spawn();
    let enrollment = Enrollment::enroll(
        child.child.id(),
        agent(),
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    let before = bytes(e.root.path());
    std::thread::scope(|s| {
        let server = s.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let result =
                catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            result
        });
        child.connect(&id, &e.descriptor.vault_id, "qualified");
        assert_eq!(child.event(), json!({"connected":true}));
        child.initialize();
        let tools =
            child.rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))["response"]["result"]
                ["tools"]
                .clone();
        assert_eq!(tools.as_array().unwrap().len(), 5);
        for tool in tools.as_array().unwrap() {
            local_refs(&tool["inputSchema"], &tool["inputSchema"]);
            local_refs(&tool["outputSchema"], &tool["outputSchema"]);
            assert_eq!(
                tool["annotations"],
                json!({"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false})
            );
        }
        let calls = [
            packet(3, "memory_search", search_args()),
            packet(
                4,
                "memory_read",
                json!({"memory_id":receipts[0].result.memory_id,"expected":null,"include_source_text":true,"max_bytes":262144,"history_cursor":null}),
            ),
            packet(
                5,
                "memory_source",
                json!({"source_id":SourceId::parse(&fixture::id("src",1)).unwrap(),"revision":null,"from_byte":0,"to_byte":null,"max_bytes":8192}),
            ),
            packet(
                6,
                "memory_related",
                json!({"memory_id":receipts[0].result.memory_id,"expected":null,"max_bytes":262144}),
            ),
            packet(
                7,
                "memory_recall",
                json!({"text":"moonlight","conditions":[],"event_window":null,"at":Timestamp::from_unix_ms(NOW).unwrap(),"intent":"current","limit":8,"max_bytes":16384}),
            ),
            packet(
                8,
                "memory_recall",
                json!({"text":"moonlight","conditions":[],"event_window":null,"at":Timestamp::from_unix_ms(NOW).unwrap(),"intent":"current","limit":8,"max_bytes":1}),
            ),
        ];
        let schemas = support::schema::SchemaStore::load();
        let mut exchanges = Vec::new();
        let mut negatives = Vec::new();
        for call in calls {
            let response = child.rpc(call.clone())["response"].clone();
            let actual = &response["result"]["structuredContent"];
            assert!(actual["error"].is_null(), "{response}");
            assert!(
                schemas
                    .validate("next/agent-qualified-read-v2.schema.json", actual)
                    .is_empty(),
                "{}",
                serde_json::to_string_pretty(
                    &schemas.validate("next/agent-qualified-read-v2.schema.json", actual)
                )
                .unwrap()
            );
            assert_eq!(actual["principal"], json!(agent()));
            assert_eq!(actual["capability"], catalog::CAPABILITY);
            assert!(
                !actual
                    .to_string()
                    .contains(receipts[7].result.memory_id.as_str())
            );
            if call["params"]["name"] == "memory_search" {
                assert_eq!(actual["data"]["matched_in_scope"], 4);
                assert_eq!(
                    actual["data"]["selection"]["algorithm"],
                    "owner-qualified-catalog-4"
                );
                assert!(actual["data"]["items"].as_array().unwrap().iter().any(|m| {
                    !m["base"]["owner_marks"]["tags"]
                        .as_array()
                        .unwrap()
                        .is_empty()
                }));
                let mut bad = actual.clone();
                bad["data"]["items"][0]["base"]["base"]["receipt"]["sequence"] = json!(99);
                assert!(
                    !schemas
                        .validate("next/agent-qualified-read-v2.schema.json", &bad)
                        .is_empty()
                );
                negatives.push(json!({"tool":"memory_search","value":bad}));
                let mut bad = actual.clone();
                bad["data"]["selection"]["algorithm"] = json!("literal-alias-catalog-1");
                assert!(
                    !schemas
                        .validate("next/agent-qualified-read-v2.schema.json", &bad)
                        .is_empty()
                );
                negatives.push(json!({"tool":"memory_search","value":bad}));
            }
            if call["params"]["name"] == "memory_recall" {
                let request: RequestId =
                    serde_json::from_value(call["params"]["arguments"]["request_id"].clone())
                        .unwrap();
                assert_eq!(
                    reader.context_trace(&request).unwrap().unwrap(),
                    actual["data"]
                );
            }
            exchanges.push(json!({"tool":call["params"]["name"],"arguments":call["params"]["arguments"],"response":response}));
        }
        assert_eq!(bytes(e.root.path()), before);
        if let Some(path) = std::env::var_os("ENOUIA_MCP_QUALIFIED_TRANSCRIPT") {
            std::fs::write(
                path,
                serde_json::to_vec(
                    &json!({"catalog":tools,"exchanges":exchanges,"negative_outputs":negatives}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        child.finish();
        assert!(server.join().unwrap().is_err());
    });
    assert!(
        reader
            .context_trace(&RequestId::parse(&fixture::id("req", 8)).unwrap())
            .is_err()
    );
}

#[test]
fn old_profiles_hashes_and_unadopted_carriers_are_refused_on_the_real_pipe() {
    for mode in [
        "legacy",
        "v2",
        "records",
        "catalog",
        "bad_hash",
        "bad_carrier",
    ] {
        let e = env();
        let vault = Arc::new(e.create(Faults::none()));
        e.populate(&vault, 1);
        let host =
            QualifiedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
                .unwrap();
        let p = profile();
        let reader = host
            .reader(&e.descriptor.created_by, agent(), scope(&e, 1, false))
            .unwrap();
        let id = sha256(&OsIdSource.random_16());
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut child = Agent::spawn();
        let enrollment =
            Enrollment::enroll(child.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
        let before = bytes(e.root.path());
        std::thread::scope(|s| {
            let server = s.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                let result =
                    catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                result
            });
            child.connect(&id, &e.descriptor.vault_id, mode);
            assert_eq!(child.event(), json!({"version_refused":true}));
            assert!(child.child.wait().unwrap().success());
            assert!(server.join().unwrap().is_err());
        });
        assert_eq!(bytes(e.root.path()), before);
    }
}

#[test]
fn a_different_os_peer_cannot_claim_the_enrolled_qualified_agent() {
    let e = env();
    let vault = Arc::new(e.create(Faults::none()));
    e.populate(&vault, 1);
    let host = QualifiedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
        .unwrap();
    let p = profile();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 1, false))
        .unwrap();
    let id = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&id).unwrap();
    let enrolled = Agent::spawn();
    let mut attacker = Agent::spawn();
    let enrollment =
        Enrollment::enroll(enrolled.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
    let before = bytes(e.root.path());
    std::thread::scope(|s| {
        let server = s.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let result =
                catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            result
        });
        attacker.connect(&id, &e.descriptor.vault_id, "qualified");
        assert_eq!(attacker.event(), json!({"connect_refused":true}));
        assert!(attacker.child.wait().unwrap().success());
        assert!(server.join().unwrap().is_err());
    });
    assert_eq!(bytes(e.root.path()), before);
}

#[test]
fn invalidated_qualified_connections_keep_original_packets_and_never_reconnect() {
    for action in ["lock", "revoke", "expire"] {
        let e = env();
        let vault = Arc::new(e.create(Faults::none()));
        e.populate(&vault, 1);
        let host =
            QualifiedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
                .unwrap();
        let p = profile();
        let reader = host
            .reader(&e.descriptor.created_by, agent(), scope(&e, 1, false))
            .unwrap();
        let id = sha256(&OsIdSource.random_16());
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut child = Agent::spawn();
        let enrollment =
            Enrollment::enroll(child.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
        std::thread::scope(|s| {
            let server = s.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                let result =
                    catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                result
            });
            child.connect(&id, &e.descriptor.vault_id, "qualified");
            assert_eq!(child.event(), json!({"connected":true}));
            child.initialize();
            match action {
                "lock" => host.lock().unwrap(),
                "revoke" => reader.revoke(),
                _ => e.clock.set(NOW + 120_000),
            };
            let original = packet(2, "memory_search", search_args());
            let failure = child.rpc(original.clone());
            assert!(failure["failure"].is_string(), "{failure}");
            assert_eq!(failure["original"], original);
            if failure["failure"] == "OutcomeUnknown" {
                assert_eq!(failure["retained"], original);
            }
            assert_eq!(child.rpc(original)["failure"], "Closed");
            child.finish();
            assert!(server.join().unwrap().is_err());
        });
    }
}

#[test]
fn source_refusal_keeps_pipe_live_but_deletion_and_policy_replacement_remove_access() {
    let e = env();
    let vault = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&vault, 8);
    let host = QualifiedAgentHost::from_authenticated_owner(
        vault.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let p = profile();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 4, false))
        .unwrap();
    let id = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut child = Agent::spawn();
    let enrollment =
        Enrollment::enroll(child.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
    std::thread::scope(|s| {
        let server = s.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let result =
                catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            result
        });
        child.connect(&id, &e.descriptor.vault_id, "qualified");
        assert_eq!(child.event(), json!({"connected":true}));
        child.initialize();
        let denied=child.rpc(packet(2,"memory_source",json!({"source_id":SourceId::parse(&fixture::id("src",1)).unwrap(),"revision":null,"from_byte":0,"to_byte":null,"max_bytes":8192})));
        assert_eq!(
            denied["response"]["result"]["structuredContent"]["error"]["code"],
            "permission_denied"
        );
        let before = child.rpc(packet(3, "memory_search", search_args()));
        assert_eq!(
            before["response"]["result"]["structuredContent"]["data"]["matched_in_scope"],
            4
        );
        vault
            .delete(
                &e.descriptor.created_by,
                vec![SourceId::parse(&fixture::id("src", 1)).unwrap()],
                vec![],
            )
            .unwrap();
        let after = child.rpc(packet(4, "memory_search", search_args()));
        assert_eq!(
            after["response"]["result"]["structuredContent"]["data"]["matched_in_scope"],
            3
        );
        assert!(
            !after
                .to_string()
                .contains(receipts[0].result.memory_id.as_str())
        );
        let absent=child.rpc(packet(5,"memory_read",json!({"memory_id":receipts[0].result.memory_id,"expected":null,"history_cursor":null,"include_source_text":false,"max_bytes":262144})));
        assert!(absent["response"]["result"]["structuredContent"]["data"].is_null());
        assert!(absent["response"]["result"]["structuredContent"]["error"].is_null());
        let mut policy = e.policy(1, 1, 8);
        policy.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
        vault.set_policy(&e.descriptor.created_by, policy).unwrap();
        let original = packet(6, "memory_search", search_args());
        let refused = child.rpc(original.clone());
        if refused["response"]["result"]["structuredContent"]["error"]["code"]
            == "permission_denied"
        {
            assert!(refused["response"]["result"]["structuredContent"]["data"].is_null());
        } else {
            // Revocation closes promptly. DisconnectNamedPipe may discard an
            // unread denial reply; that remains unknown, never a known success.
            assert_eq!(refused["failure"], "OutcomeUnknown", "{refused}");
            assert_eq!(refused["original"], original);
            assert_eq!(refused["retained"], original);
            assert_eq!(child.rpc(original)["failure"], "Closed");
        }
        assert!(
            reader
                .page(&p, &CatalogFilter::default(), None, 1, 262144, false)
                .is_err()
        );
        child.finish();
        assert!(server.join().unwrap().is_ok());
    });
}

#[test]
fn stdio_executable_attaches_to_owner_host_and_reaps_on_eof() {
    let e = env();
    let vault = Arc::new(e.create(Faults::none()));
    e.populate(&vault, 8);
    let host = QualifiedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
        .unwrap();
    let p = profile();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 8, false))
        .unwrap();
    let id = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_enouia-memory-mcp"))
        .args([
            "--protocol",
            "automation-qualified-v1",
            "--pipe",
            id.as_str(),
            "--host-pid",
            &std::process::id().to_string(),
            "--vault-id",
            e.descriptor.vault_id.as_str(),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let input = bridge.stdin.take().unwrap();
    let output = bridge.stdout.take().unwrap();
    let (tx, events) = mpsc::channel();
    let output_reader = std::thread::spawn(move || {
        for line in BufReader::new(output).lines() {
            if tx
                .send(serde_json::from_str(&line.unwrap()).unwrap())
                .is_err()
            {
                break;
            }
        }
    });
    let mut child = Agent {
        child: bridge,
        input: Some(input),
        events,
        reader: Some(output_reader),
    };
    let enrollment =
        Enrollment::enroll(child.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
    let before = bytes(e.root.path());
    std::thread::scope(|s| {
        let server = s.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let r = catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            r
        });
        child.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-qualified-stdio","version":"0"}}}));
        assert_eq!(child.event()["result"]["protocolVersion"], "2025-11-25");
        child.send(json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
        child.send(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        assert_eq!(
            child.event()["result"]["tools"].as_array().unwrap().len(),
            5
        );
        let call = packet(
            3,
            "memory_recall",
            json!({"text":"moonlight","conditions":[],"event_window":null,"at":Timestamp::from_unix_ms(NOW).unwrap(),"intent":"current","limit":8,"max_bytes":16384}),
        );
        let request =
            RequestId::parse(call["params"]["arguments"]["request_id"].as_str().unwrap()).unwrap();
        child.send(call);
        let actual = child.event();
        assert!(actual["error"].is_null(), "{actual}");
        assert_eq!(
            reader.context_trace(&request).unwrap().unwrap(),
            actual["result"]["structuredContent"]["data"]
        );
        assert_eq!(bytes(e.root.path()), before);
        drop(child.input.take());
        let until = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = child.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert!(server.join().unwrap().is_err());
    });
}

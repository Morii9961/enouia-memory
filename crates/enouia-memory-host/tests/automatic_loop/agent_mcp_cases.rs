//! Five MCP-2 tools through a real pipe and independent owned Agent process.
use super::formation_cases::{DimensionalModel, dims, form};
use super::*;
use enouia_memory_contract::{
    foundation::Clock, policy::Scope, ports::IdSource, workspace::HostSurface,
};
use enouia_memory_host::{
    mcp::{native::Enrollment, v2},
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_vault::automation::AgentReadScope;
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin},
    sync::mpsc,
};
const TIMEOUT: Duration = Duration::from_secs(10);
fn agent() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
    }
}
fn scope(e: &Env, source_read: bool) -> AgentReadScope {
    AgentReadScope {
        subject_id: e.policy.subject_id.clone(),
        project_id: e.policy.project_id.clone(),
        source_ids: vec![SourceId::parse(&id("src", 1)).unwrap()],
        max_sensitivity: Sensitivity::Private,
        source_read,
        valid_until: Timestamp::from_unix_ms(e.clock.now_unix_ms() + 120_000).unwrap(),
    }
}
fn profile() -> AttentionProfile {
    serde_json::from_value(support::fixture("../attention/profile.json")).unwrap()
}
fn packet(n: usize, op: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":n,"method":"tools/call","params":{"name":op,"arguments":{"schema_version":2,"kind":"agent_read_request","capability":"memory-agent-read-1",
        "request_id":format!("req_00000001-0000-4000-8000-{n:012x}"),"operation":op,"arguments":args}}})
}
fn search() -> Value {
    json!({"filter":{},"cursor":null,"limit":32,"max_bytes":262144})
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
            .args([
                "--exact",
                "agent_mcp_cases::mcp2_process_child",
                "--nocapture",
            ])
            .env("ENOUIA_MCP2_CHILD", "1")
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
                if let Some((_, body)) = line.split_once("mcp2-event ")
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
            .expect("owned MCP-2 child response")
    }
    fn rpc(&mut self, v: Value) -> Value {
        self.send(json!({"message":v}));
        self.event()
    }
    fn connect(&mut self, id: &Sha256Hex, vault: &VaultId, legacy: bool) {
        self.send(
            json!({"pipe":id,"host_pid":std::process::id(),"vault_id":vault,"legacy":legacy}),
        );
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
    println!("mcp2-event {v}");
    std::io::stdout().flush().unwrap();
}
#[test]
fn mcp2_process_child() {
    if std::env::var_os("ENOUIA_MCP2_CHILD").is_none() {
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
    if config["legacy"] == true {
        assert!(
            enouia_memory_host::mcp::native::Client::connect(
                Pipe::connect(&id).unwrap(),
                peer,
                &vault,
                TIMEOUT
            )
            .is_err()
        );
        event(json!({"legacy_refused":true}));
        return;
    }
    let mut client =
        v2::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT).unwrap();
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
fn mcp2_all_five_tools_use_real_transport_and_the_exact_context_trace_without_reinforcement() {
    let e = super::formation_cases::env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let worker = jobs.spawn(DimensionalModel).unwrap();
    let memory = form(
        &e,
        &host,
        &jobs,
        1,
        "A synthetic pottery 中文 episode.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let hidden = form(
        &e,
        &host,
        &jobs,
        2,
        "Hidden synthetic episode.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    stop(&jobs, worker);
    let reader = host
        .agent_reader(&e.descriptor.created_by, agent(), scope(&e, true))
        .unwrap();
    let p = profile();
    let id = sha256(&enouia_memory_vault::OsIdSource.random_16());
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut child = Agent::spawn();
    let enrollment = Enrollment::enroll(
        child.child.id(),
        agent(),
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    let before = bytes(e.root.path());
    let schemas = support::schema::SchemaStore::load();
    let mut exchanges = Vec::new();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let r = v2::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            r
        });
        child.connect(&id, &e.descriptor.vault_id, false);
        assert_eq!(child.event(), json!({"connected":true}));
        child.initialize();
        let listed = child.rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        let catalog = listed["response"]["result"]["tools"].clone();
        assert_eq!(catalog.as_array().unwrap().len(), 5);
        assert!(listed.to_string().len() < enouia_memory_host::mcp::MAX_OUTPUT_BYTES);
        for tool in catalog.as_array().unwrap() {
            assert!(tool["annotations"]["readOnlyHint"].as_bool().unwrap());
            assert_eq!(tool["annotations"]["idempotentHint"], true);
            assert_eq!(tool["annotations"]["destructiveHint"], false);
            for key in ["inputSchema", "outputSchema"] {
                local_refs(&tool[key], &tool[key]);
            }
        }
        let mut invoke = |packet: Value| -> Value {
            let response = child.rpc(packet.clone())["response"].clone();
            assert!(response["error"].is_null(), "{response}");
            let body = response["result"]["structuredContent"].clone();
            let violations = schemas.validate("next/agent-read-v2.schema.json", &body);
            assert!(violations.is_empty(), "{violations:?} {body}");
            assert_eq!(
                response["result"]["content"][0]["text"],
                json!(body.to_string())
            );
            exchanges.push(json!({"tool":packet["params"]["name"],"arguments":packet["params"]["arguments"],"response":response}));
            body
        };
        let found = invoke(packet(3, "memory_search", search()));
        assert_eq!(found["data"]["page"]["matched_in_scope"], 1);
        assert!(!found.to_string().contains(hidden.memory.memory_id.as_str()));
        let read = invoke(packet(
            4,
            "memory_read",
            json!({"memory_id":memory.memory.memory_id,"expected":null,"include_source_text":true,"max_bytes":262144}),
        ));
        assert_eq!(
            read["data"]["item"]["memory"],
            serde_json::to_value(&memory.memory).unwrap()
        );
        let source = invoke(packet(
            5,
            "memory_source",
            json!({"source_id":id_source(1),"revision":null,"from_byte":0,"to_byte":null,"max_bytes":21}),
        ));
        assert_eq!(source["data"]["complete"], false);
        assert!(source["data"]["excerpt"].as_str().unwrap().len() <= 21);
        let related = invoke(packet(
            6,
            "memory_related",
            json!({"memory_id":memory.memory.memory_id,"expected":null,"max_bytes":262144}),
        ));
        assert_eq!(
            related["data"]["unavailable"],
            json!(["fact_edges", "suggested_edges"])
        );
        let recall_packet = packet(
            7,
            "memory_recall",
            json!({"text":"pottery","conditions":[],"event_window":null,"at":Timestamp::from_unix_ms(NOW).unwrap(),"intent":"current","limit":8,"max_bytes":4096}),
        );
        let request = RequestId::parse(
            recall_packet["params"]["arguments"]["request_id"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let recall = invoke(recall_packet);
        assert_eq!(recall["data"]["trace"]["considered"], 1);
        assert!(
            recall["data"]["rendered_context"]
                .as_str()
                .unwrap()
                .contains("Subject stated")
        );
        assert_eq!(
            serde_json::to_value(reader.context_trace(&request).unwrap().unwrap()).unwrap(),
            recall["data"]
        );
        let attention = found["data"]["page"]["items"][0]["attention"].clone();
        for n in 8..108 {
            let again = invoke(packet(n, "memory_search", search()));
            assert_eq!(again["data"]["page"]["items"][0]["attention"], attention);
        }
        assert_eq!(
            bytes(e.root.path()),
            before,
            "reads/recall cache never persist or reinforce"
        );
        // Missing/hidden selection shares the same absent response.
        let absent = invoke(packet(
            108,
            "memory_read",
            json!({"memory_id":hidden.memory.memory_id,"expected":null,"include_source_text":false,"max_bytes":262144}),
        ));
        assert!(absent["data"].is_null());
        child.finish();
        assert!(server.join().unwrap().is_err());
        if let Some(path) = std::env::var_os("ENOUIA_MCP2_SCHEMA_TRANSCRIPT") {
            std::fs::write(
                path,
                serde_json::to_vec(&json!({"catalog":catalog,"exchanges":exchanges})).unwrap(),
            )
            .unwrap();
        }
    });
}
fn id_source(n: u8) -> SourceId {
    SourceId::parse(&super::id("src", n)).unwrap()
}
#[test]
fn mcp2_old_client_and_forged_or_unfrozen_arguments_are_refused() {
    for legacy in [true, false] {
        let e = super::formation_cases::env();
        let jobs = e.jobs();
        let host = e.host(e.vault(Faults::none()), jobs.clone());
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, false))
            .unwrap();
        let p = profile();
        let id = sha256(&enouia_memory_vault::OsIdSource.random_16());
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut child = Agent::spawn();
        let enrollment =
            Enrollment::enroll(child.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
        let before = bytes(e.root.path());
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                let r = v2::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                r
            });
            child.connect(&id, &e.descriptor.vault_id, legacy);
            if legacy {
                assert_eq!(child.event(), json!({"legacy_refused":true}));
            } else {
                assert_eq!(child.event(), json!({"connected":true}));
                child.initialize();
                for (n, mode) in ["major", "principal", "capability", "entities", "workspace"]
                    .iter()
                    .enumerate()
                {
                    let mut request = packet(n + 2, "memory_search", search());
                    let args = &mut request["params"]["arguments"];
                    match *mode {
                        "major" => args["schema_version"] = json!(1),
                        "principal" => args["principal"] = json!(e.descriptor.created_by),
                        "capability" => args["capability"] = json!("memory-native-read-1"),
                        "entities" => args["arguments"]["filter"]["entities"] = json!(["unfrozen"]),
                        _ => args["command"] = json!("vault_unlock"),
                    }
                    assert!(child.rpc(request)["response"]["error"].is_object());
                }
                let denied=child.rpc(packet(8,"memory_source",json!({"source_id":id_source(1),"revision":null,"from_byte":0,"to_byte":null,"max_bytes":8192})));
                assert_eq!(
                    denied["response"]["result"]["structuredContent"]["error"]["code"],
                    "permission_denied",
                    "{denied}"
                );
                child.finish();
            }
            assert!(server.join().unwrap().is_err() || !legacy);
        });
        assert_eq!(bytes(e.root.path()), before);
    }
}
#[test]
fn mcp2_request_fixture_schema_cases_match() {
    let fixture: Value = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/agent-read/manifest.json"
    ))
    .unwrap();
    let store = support::schema::SchemaStore::load();
    for value in fixture["valid"].as_array().unwrap() {
        assert!(
            store
                .validate("next/agent-read-v2.schema.json", value)
                .is_empty()
        );
    }
    for case in fixture["invalid"].as_array().unwrap() {
        assert!(
            !store
                .validate("next/agent-read-v2.schema.json", &case["value"])
                .is_empty(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn mcp2_invalidated_native_connections_retain_original_and_never_reconnect() {
    for action in ["lock", "revoke", "expire"] {
        let e = super::formation_cases::env();
        let jobs = e.jobs();
        let host = e.host(e.vault(Faults::none()), jobs.clone());
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, false))
            .unwrap();
        let p = profile();
        let id = sha256(&enouia_memory_vault::OsIdSource.random_16());
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut child = Agent::spawn();
        let enrollment =
            Enrollment::enroll(child.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                let r = v2::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                r
            });
            child.connect(&id, &e.descriptor.vault_id, false);
            assert_eq!(child.event(), json!({"connected":true}));
            child.initialize();
            match action {
                "lock" => host.lock().unwrap(),
                "revoke" => reader.revoke(),
                _ => e.clock.set(NOW + 120_000),
            }
            let original = packet(2, "memory_search", search());
            let failed = child.rpc(original.clone());
            assert!(failed["failure"].is_string(), "{failed}");
            assert_eq!(failed["original"], original);
            if failed["failure"] == "OutcomeUnknown" {
                assert_eq!(failed["retained"], original);
            }
            assert_eq!(child.rpc(original)["failure"], "Closed");
            child.finish();
            assert!(server.join().unwrap().is_err());
        });
    }
}

#[test]
fn mcp2_stdio_binary_attaches_to_the_existing_host_without_opening_a_writer() {
    let e = super::formation_cases::env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let worker = jobs.spawn(DimensionalModel).unwrap();
    form(
        &e,
        &host,
        &jobs,
        1,
        "A synthetic stdio pottery episode.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    stop(&jobs, worker);
    let reader = host
        .agent_reader(&e.descriptor.created_by, agent(), scope(&e, true))
        .unwrap();
    let p = profile();
    let id = sha256(&enouia_memory_vault::OsIdSource.random_16());
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut bridge = Command::new(env!("CARGO_BIN_EXE_enouia-memory-mcp"))
        .args([
            "--protocol",
            "automation-v2",
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
            let value: Value =
                serde_json::from_str(&line.unwrap()).expect("only MCP JSON on stdio");
            if tx.send(value).is_err() {
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
    let enrollment = Enrollment::enroll(
        child.child.id(),
        agent(),
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    let before = bytes(e.root.path());
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let r = v2::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            r
        });
        child.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-stdio-v2","version":"0"}}}));
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
            json!({"text":"pottery","conditions":[],"event_window":null,"at":Timestamp::from_unix_ms(NOW).unwrap(),"intent":"current","limit":8,"max_bytes":4096}),
        );
        let request =
            RequestId::parse(call["params"]["arguments"]["request_id"].as_str().unwrap()).unwrap();
        child.send(call);
        let actual = child.event();
        assert!(actual["error"].is_null(), "{actual}");
        assert_eq!(
            actual["result"]["structuredContent"]["data"],
            serde_json::to_value(reader.context_trace(&request).unwrap().unwrap()).unwrap()
        );
        assert_eq!(bytes(e.root.path()), before);
        // EOF, not a new MCP control tool, releases this owned bridge connection.
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

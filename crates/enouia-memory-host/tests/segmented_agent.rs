#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod support;
use enouia_memory_contract::{
    common::*,
    foundation::Clock,
    hash::{Sha256Hex, sha256},
    ids::*,
    next::*,
    policy::Scope,
    ports::IdSource,
    time::Timestamp,
    workspace::HostSurface,
};
use enouia_memory_host::{
    automation::segmented_agent::*,
    mcp::{catalog, native::Enrollment},
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
fn agent() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}
#[test]
fn catalog_request_schema_fixtures_reject_versions_impersonation_and_unbounded_arguments() {
    let manifest = value("agent-catalog-read/manifest.json");
    let store = support::schema::SchemaStore::load();
    for v in manifest["valid"].as_array().unwrap() {
        assert!(
            store
                .validate("next/agent-catalog-read-v2.schema.json", v)
                .is_empty()
        );
    }
    for c in manifest["invalid"].as_array().unwrap() {
        assert!(
            !store
                .validate("next/agent-catalog-read-v2.schema.json", &c["value"])
                .is_empty(),
            "{}",
            c["name"]
        );
    }
}
#[test]
fn scoped_catalog_filters_match_native_catalog_semantics_on_the_same_authorized_rows() {
    use enouia_memory_host::automation::catalog::{
        CatalogReadOptions, CatalogReadRequest, SegmentedMemoryReader,
    };
    let e = Env::new();
    let vault = Arc::new(e.create(Faults::none()));
    e.populate(&vault, 12);
    let system = SegmentedMemoryReader::from_authenticated_system(
        vault.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    let host = SegmentedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
        .unwrap();
    let p = profile();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 12, false))
        .unwrap();
    let mut filters = vec![
        CatalogFilter::default(),
        CatalogFilter {
            text: "合成".into(),
            ..Default::default()
        },
        CatalogFilter {
            text: "moonlight".into(),
            ..Default::default()
        },
        CatalogFilter {
            form: Some(FormationKind::Fact),
            ..Default::default()
        },
        CatalogFilter {
            knowledge: Some(KnowledgeState::Hypothesis),
            ..Default::default()
        },
        CatalogFilter {
            adoption: Some(AdoptionState::Staged),
            ..Default::default()
        },
        CatalogFilter {
            entity: Some(id("ent", 999)),
            ..Default::default()
        },
    ];
    for axis in [
        CatalogTimeAxis::Event,
        CatalogTimeAxis::Valid,
        CatalogTimeAxis::Learned,
        CatalogTimeAxis::SourceCaptured,
    ] {
        for unknown in [true, false] {
            filters.push(CatalogFilter {
                time_axis: Some(axis),
                time_window: Some(FormationTime {
                    from: Timestamp::from_unix_ms(NOW - 86_400_000).unwrap(),
                    until: Some(Timestamp::from_unix_ms(NOW + 86_400_000).unwrap()),
                }),
                include_unknown_time: unknown,
                ..Default::default()
            });
        }
    }
    for filter in filters {
        let request = CatalogReadRequest {
            profile: &p,
            filter: &filter,
            options: CatalogReadOptions {
                limit: 100,
                max_bytes: 262144,
                max_sensitivity: Sensitivity::Private,
                include_facets: false,
            },
        };
        let native = system.memory_page_native(&request, None).unwrap();
        let agent = reader.search(&p, &filter, None, 100, 262144).unwrap();
        assert_eq!(agent["matched_in_scope"], native.matched_in_scope);
        assert_eq!(agent["unknown_time_in_scope"], native.unknown_time_in_scope);
        let native_ids: Vec<_> = native.items.iter().map(|i| json!(i.base.memory)).collect();
        let agent_ids: Vec<_> = agent["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["base"]["memory"].clone())
            .collect();
        assert_eq!(native_ids, agent_ids);
    }
}
fn scope(e: &Env, count: u32, source_read: bool) -> SegmentedAgentScope {
    let p = e.policy(1, 1, count.min(64));
    SegmentedAgentScope {
        subject_id: p.subject_id,
        project_id: p.project_id,
        policy_ids: (1..=count)
            .step_by(64)
            .map(|n| PolicyId::parse(&id("pol", 1000 + n)).unwrap())
            .collect(),
        source_ids: (1..=count)
            .map(|n| SourceId::parse(&id("src", n)).unwrap())
            .collect(),
        max_sensitivity: Sensitivity::Private,
        source_read,
        // The permit also bounds real time; a slow machine must not expire it.
        valid_until: Timestamp::from_unix_ms(e.clock.now_unix_ms() + 300_000).unwrap(),
    }
}
fn packet(n: usize, op: &str, args: Value) -> Value {
    json!({"jsonrpc":"2.0","id":n,"method":"tools/call","params":{"name":op,"arguments":{"schema_version":2,"kind":"agent_catalog_read_request","capability":CAPABILITY,"request_id":format!("req_00000001-0000-4000-8000-{n:012x}"),"operation":op,"arguments":args}}})
}
fn search_args(
    filter: &CatalogFilter,
    cursor: Option<&CatalogCursor>,
    limit: usize,
    max_bytes: usize,
) -> Value {
    json!({"filter":filter,"cursor":cursor,"limit":limit,"max_bytes":max_bytes})
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
            .args(["--exact", "catalog_process_child", "--nocapture"])
            .env("ENOUIA_CATALOG_CHILD", "1")
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
                if let Some((_, body)) = line.split_once("catalog-event ")
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
    println!("catalog-event {v}");
    std::io::stdout().flush().unwrap();
}
#[test]
fn catalog_process_child() {
    if std::env::var_os("ENOUIA_CATALOG_CHILD").is_none() {
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
    if mode != "catalog" {
        let refused = match mode {
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
    let mut client = connection.unwrap();
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
fn native_catalog_scope_pagination_fields_trace_lifecycle_and_business_bytes() {
    let e = Env::new();
    let vault = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&vault, 180);
    let host = SegmentedAgentHost::from_authenticated_owner(
        vault.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 180, true))
        .unwrap();
    let p = profile();
    let before = bytes(e.root.path());
    let mut cursor = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let page = reader
            .search(&p, &CatalogFilter::default(), cursor.as_ref(), 100, 16_384)
            .unwrap();
        assert_eq!(page["matched_in_scope"], 180);
        for item in page["items"].as_array().unwrap() {
            assert!(seen.insert(item["base"]["memory"].clone().to_string()));
            assert_eq!(
                item["record"]["fields"]["event_time"]["precision"],
                "unknown"
            );
        }
        pages += 1;
        cursor = serde_json::from_value(page["next"].clone()).unwrap();
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 180);
    assert!(pages > 2);
    for _ in 0..100 {
        reader
            .search(
                &p,
                &CatalogFilter {
                    text: "合成".into(),
                    ..Default::default()
                },
                None,
                1,
                16_384,
            )
            .unwrap();
    }
    let query = RecallQuery {
        text: "moonlight".into(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(NOW).unwrap(),
        intent: RecallIntent::Current,
        limit: 8,
        max_bytes: 16_384,
    };
    let request = RequestId::parse(&id("req", 9)).unwrap();
    let recall = reader.recall(&p, &query, request.clone()).unwrap();
    assert_eq!(recall["trace"]["considered"], 180);
    assert_eq!(recall["trace_coverage"]["excluded_returned"], 128);
    assert_eq!(recall["trace_coverage"]["complete"], false);
    assert_eq!(reader.context_trace(&request).unwrap(), Some(recall));
    assert_eq!(bytes(e.root.path()), before);
    let filter = CatalogFilter {
        knowledge: Some(KnowledgeState::Hypothesis),
        ..Default::default()
    };
    let old = reader.search(&p, &filter, None, 1, 16_384).unwrap();
    let cursor: CatalogCursor = serde_json::from_value(old["next"].clone()).unwrap();
    assert!(
        reader
            .search(&p, &CatalogFilter::default(), Some(&cursor), 1, 16_384)
            .is_err()
    );
    vault
        .delete(
            &e.descriptor.created_by,
            vec![],
            vec![receipts[0].result.memory_id.clone()],
        )
        .unwrap();
    assert!(reader.context_trace(&request).is_err());
    assert!(
        reader
            .search(&p, &filter, Some(&cursor), 1, 16_384)
            .is_err()
    );
    host.lock().unwrap();
    assert!(reader.search(&p, &filter, None, 1, 16_384).is_err());
    host.unlock(&e.descriptor.created_by).unwrap();
    assert!(reader.search(&p, &filter, None, 1, 16_384).is_err());
}

#[test]
fn all_five_catalog_tools_use_real_pipe_and_independent_agent_with_valid_schemas() {
    let e = Env::new();
    let vault = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&vault, 180);
    let host = SegmentedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
        .unwrap();
    let reader = host
        .reader(&e.descriptor.created_by, agent(), scope(&e, 180, true))
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
            let r = catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            r
        });
        child.connect(&id, &e.descriptor.vault_id, "catalog");
        assert_eq!(child.event(), json!({"connected":true}));
        child.initialize();
        let catalog = child.rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))["response"]
            ["result"]["tools"]
            .clone();
        assert_eq!(catalog.as_array().unwrap().len(), 5);
        for tool in catalog.as_array().unwrap() {
            local_refs(&tool["inputSchema"], &tool["inputSchema"]);
            local_refs(&tool["outputSchema"], &tool["outputSchema"]);
            assert_eq!(tool["annotations"]["readOnlyHint"], true);
        }
        let calls = [
            packet(
                3,
                "memory_search",
                search_args(&CatalogFilter::default(), None, 100, 262144),
            ),
            packet(
                4,
                "memory_read",
                json!({"memory_id":receipts[0].result.memory_id,"expected":null,"include_source_text":true,"max_bytes":262144}),
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
            assert!(response["error"].is_null(), "{response}");
            let body = &response["result"]["structuredContent"];
            assert!(body["error"].is_null(), "{body}");
            let errors = schemas.validate("next/agent-catalog-read-v2.schema.json", body);
            assert!(errors.is_empty(), "{errors:?}");
            if call["params"]["name"] == "memory_recall" {
                let req =
                    RequestId::parse(call["params"]["arguments"]["request_id"].as_str().unwrap())
                        .unwrap();
                assert_eq!(reader.context_trace(&req).unwrap().unwrap(), body["data"]);
            }
            if call["id"] == 8 {
                let mut bad = body.clone();
                bad["data"]["trace"]["excluded"][0][1] = json!("owner_confirmed");
                assert!(
                    !schemas
                        .validate("next/agent-catalog-read-v2.schema.json", &bad)
                        .is_empty()
                );
                negatives.push(json!({"tool":"memory_recall","value":bad}));
            }
            exchanges.push(json!({"tool":call["params"]["name"],"arguments":call["params"]["arguments"],"response":response}));
        }
        assert_eq!(bytes(e.root.path()), before);
        if let Some(path) = std::env::var_os("ENOUIA_MCP_CATALOG_TRANSCRIPT") {
            std::fs::write(
                path,
                serde_json::to_vec(
                    &json!({"catalog":catalog,"exchanges":exchanges,"negative_outputs":negatives}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        child.finish();
        assert!(server.join().unwrap().is_err());
    });
}
#[test]
fn legacy_hostv2_and_hostv3_clients_refuse_segmented_before_any_business_query() {
    let e = Env::new();
    let vault = Arc::new(e.create(Faults::none()));
    e.populate(&vault, 8);
    let host = SegmentedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
        .unwrap();
    let p = profile();
    let before = bytes(e.root.path());
    for mode in ["legacy", "v2", "records"] {
        let reader = host
            .reader(&e.descriptor.created_by, agent(), scope(&e, 8, false))
            .unwrap();
        let id = sha256(&OsIdSource.random_16());
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut child = Agent::spawn();
        let enrollment =
            Enrollment::enroll(child.child.id(), agent(), vec![Scope::MemoryRead]).unwrap();
        std::thread::scope(|s| {
            let server = s.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                let r =
                    catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                r
            });
            child.connect(&id, &e.descriptor.vault_id, mode);
            assert_eq!(child.event(), json!({"version_refused":true}));
            assert!(server.join().unwrap().is_err());
        });
    }
    assert_eq!(bytes(e.root.path()), before);
}
#[test]
fn source_refusal_keeps_connection_live_and_deletion_removes_rows_labels_and_counts() {
    let e = Env::new();
    let vault = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&vault, 8);
    let host = SegmentedAgentHost::from_authenticated_owner(
        vault.clone(),
        e.descriptor.created_by.clone(),
    )
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
            let r = catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            r
        });
        child.connect(&id, &e.descriptor.vault_id, "catalog");
        assert_eq!(child.event(), json!({"connected":true}));
        child.initialize();
        let source = packet(
            2,
            "memory_source",
            json!({"source_id":SourceId::parse(&fixture::id("src",1)).unwrap(),"revision":null,"from_byte":0,"to_byte":null,"max_bytes":8192}),
        );
        assert_eq!(
            child.rpc(source)["response"]["result"]["structuredContent"]["error"]["code"],
            "permission_denied"
        );
        let call = packet(
            3,
            "memory_search",
            search_args(&CatalogFilter::default(), None, 100, 262144),
        );
        assert_eq!(
            child.rpc(call)["response"]["result"]["structuredContent"]["data"]["matched_in_scope"],
            1
        );
        let mut forged = packet(
            4,
            "memory_search",
            search_args(&CatalogFilter::default(), None, 100, 262144),
        );
        forged["params"]["arguments"]["principal"] =
            json!({"actor_type":"owner","actor_id":agent().actor_id});
        assert_eq!(
            child.rpc(forged)["response"]["error"]["message"],
            "unsupported_request"
        );
        for n in 100..400 {
            assert!(
                child.rpc(json!({"jsonrpc":"2.0","id":n,"method":"ping"}))["response"]["result"]
                    .is_object()
            );
        }
        vault
            .delete(
                &e.descriptor.created_by,
                vec![SourceId::parse(&fixture::id("src", 1)).unwrap()],
                vec![],
            )
            .unwrap();
        assert_eq!(
            child.rpc(packet(
                5,
                "memory_search",
                search_args(&CatalogFilter::default(), None, 100, 262144)
            ))["response"]["result"]["structuredContent"]["data"]["matched_in_scope"],
            0
        );
        assert!(child.rpc(packet(6,"memory_read",json!({"memory_id":receipts[0].result.memory_id,"expected":null,"include_source_text":false,"max_bytes":262144})))["response"]["result"]["structuredContent"]["data"].is_null());
        child.finish();
        assert!(server.join().unwrap().is_err());
    });
}
#[test]
fn invalidated_catalog_connections_keep_the_original_packet_and_never_reconnect() {
    for action in ["lock", "revoke", "expire"] {
        let e = Env::new();
        let vault = Arc::new(e.create(Faults::none()));
        e.populate(&vault, 1);
        let host =
            SegmentedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
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
                let r =
                    catalog::native::serve_connection(&reader, &p, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                r
            });
            child.connect(&id, &e.descriptor.vault_id, "catalog");
            assert_eq!(child.event(), json!({"connected":true}));
            child.initialize();
            match action {
                "lock" => host.lock().unwrap(),
                "revoke" => reader.revoke(),
                _ => e.clock.set(NOW + 300_000),
            }
            let original = packet(
                2,
                "memory_search",
                search_args(&CatalogFilter::default(), None, 1, 262144),
            );
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
fn stdio_executable_attaches_to_the_existing_segmented_host_and_reaps_on_eof() {
    let e = Env::new();
    let vault = Arc::new(e.create(Faults::none()));
    e.populate(&vault, 8);
    let host = SegmentedAgentHost::from_authenticated_owner(vault, e.descriptor.created_by.clone())
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
            "automation-catalog-v1",
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
        child.send(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-catalog-stdio","version":"0"}}}));
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

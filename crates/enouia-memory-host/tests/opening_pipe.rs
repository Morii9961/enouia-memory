#![cfg(windows)]
#[path = "support/agent_correction_fixture.rs"]
mod fixture;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod schema_support;
use enouia_memory_contract::{
    common::*,
    hash::{Sha256Hex, sha256},
    ids::*,
    json::canonical_bytes,
    next::*,
    policy::Scope,
    ports::IdSource,
    workspace::HostSurface,
};
use enouia_memory_host::{
    automation::qualified_agent::QualifiedAgentHost,
    mcp::{native::Enrollment, opening},
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_vault::{
    OsIdSource,
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(15);

fn initialization() -> [Value; 2] {
    [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-agent","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ]
}
struct Agent {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Agent {
    fn spawn() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "opening_pipe_process_child", "--nocapture"])
            .env("ENOUIA_OPENING_PIPE_CHILD", "1")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        Self {
            input: child.stdin.take().unwrap(),
            output: BufReader::new(child.stdout.take().unwrap()),
            child,
        }
    }
    fn send(&mut self, value: Value) {
        writeln!(self.input, "{value}").unwrap();
        self.input.flush().unwrap();
    }
    fn event(&mut self) -> Value {
        loop {
            let mut line = String::new();
            assert_ne!(
                self.output.read_line(&mut line).unwrap(),
                0,
                "owned child ended before event"
            );
            if let Some((_, value)) = line.split_once("opening-event ") {
                return serde_json::from_str(value).unwrap();
            }
        }
    }
    fn connect(&mut self, pipe: &Sha256Hex, vault: &VaultId, mode: &str) {
        self.send(json!({"pipe":pipe,"vault":vault,"host_pid":std::process::id(),"mode":mode}));
    }
    fn rpc(&mut self, message: Value) -> Value {
        self.send(json!({"message":message}));
        self.event()
    }
    fn initialize(&mut self) {
        for message in initialization() {
            assert!(self.rpc(message).get("response").is_some());
        }
    }
    fn finish(&mut self) {
        self.send(json!({"exit":true}));
        assert!(self.child.wait().unwrap().success());
    }
}
impl Drop for Agent {
    fn drop(&mut self) {
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
    }
}
fn event(value: Value) {
    println!("opening-event {value}");
    std::io::stdout().flush().unwrap();
}
struct FailAfterInitialize<'a> {
    output: &'a mut Vec<u8>,
    lines: usize,
}
impl Write for FailAfterInitialize<'_> {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self.lines >= 1 {
            return Err(std::io::ErrorKind::BrokenPipe.into());
        }
        self.lines += bytes.iter().filter(|b| **b == b'\n').count();
        self.output.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
fn wait_exit(input: &mut impl BufRead) {
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
}

#[test]
fn opening_pipe_process_child() {
    if std::env::var_os("ENOUIA_OPENING_PIPE_CHILD").is_none() {
        return;
    }
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
    let config: Value = serde_json::from_str(&line).unwrap();
    let id = Sha256Hex::parse(config["pipe"].as_str().unwrap()).unwrap();
    let vault = VaultId::parse(config["vault"].as_str().unwrap()).unwrap();
    let peer = ProcessPeer::enroll(
        config["host_pid"].as_u64().unwrap() as u32,
        HostSurface::QuickSearch,
    )
    .unwrap();
    let mode = config["mode"].as_str().unwrap();
    if mode == "old9" {
        event(
            json!({"refused":enouia_memory_host::mcp::forget_requests::native::Client::connect(Pipe::connect(&id).unwrap(),peer,&vault,TIMEOUT).is_err()}),
        );
        wait_exit(&mut input);
        return;
    }
    if mode == "old8" {
        event(
            json!({"refused":enouia_memory_host::mcp::supersedes::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT).is_err()}),
        );
        wait_exit(&mut input);
        return;
    }
    if mode == "old7" {
        event(
            json!({"refused":enouia_memory_host::mcp::corrections::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT).is_err()}),
        );
        wait_exit(&mut input);
        return;
    }
    if mode == "old6" {
        event(
            json!({"refused":enouia_memory_host::mcp::notes::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT).is_err()}),
        );
        wait_exit(&mut input);
        return;
    }
    if mode == "old5" {
        event(
            json!({"refused":enouia_memory_host::mcp::initialized::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT).is_err()}),
        );
        wait_exit(&mut input);
        return;
    }
    if mode == "bad_hash" || mode == "forged_owner" {
        let mut hello = opening::native::hello(&vault);
        if mode == "bad_hash" {
            hello["compatibilityHash"] = json!("0".repeat(64));
        } else {
            hello["principal"] = json!({"actor_type":"owner"});
        }
        let mut pipe = Pipe::connect(&id).unwrap();
        peer.verify_server(&pipe).unwrap();
        let bytes = serde_json::to_vec(&hello).unwrap();
        let mut frame = vec![1];
        frame.extend((bytes.len() as u32).to_le_bytes());
        frame.extend(bytes);
        pipe.write_all(&frame, Instant::now() + TIMEOUT).unwrap();
        event(json!({"refused":pipe.read_exact(&mut [0; 5], Instant::now() + TIMEOUT).is_err()}));
        wait_exit(&mut input);
        return;
    }
    let mut client = match opening::native::Client::connect(
        Pipe::connect(&id).unwrap(),
        peer,
        &vault,
        TIMEOUT,
    ) {
        Ok(client) => client,
        Err(_) => {
            event(json!({"refused":true}));
            wait_exit(&mut input);
            return;
        }
    };
    event(json!({"connected":true}));
    loop {
        line.clear();
        if input.read_line(&mut line).unwrap() == 0 {
            return;
        }
        let command: Value = serde_json::from_str(&line).unwrap();
        if command["exit"] == true {
            return;
        }
        if let Some(messages) = command["stdio"].as_array() {
            let text = messages
                .iter()
                .map(|m| format!("{m}\n"))
                .collect::<String>();
            let mut input = std::io::Cursor::new(text.into_bytes());
            let mut output = Vec::new();
            let result = if command["fail_output"] == true {
                client.serve_stdio(
                    &mut input,
                    &mut FailAfterInitialize {
                        output: &mut output,
                        lines: 0,
                    },
                )
            } else {
                client.serve_stdio(&mut input, &mut output)
            };
            let output: Vec<Value> = String::from_utf8(output)
                .unwrap()
                .lines()
                .map(|l| serde_json::from_str(l).unwrap())
                .collect();
            event(
                json!({"stdio_ok":result.is_ok(),"output":output,"retained":client.original_unknown()}),
            );
            continue;
        }
        match client.exchange(&command["message"]) {
            Ok(response) => event(json!({"response":response})),
            Err(fault) => event(
                json!({"fault":format!("{:?}",fault.fault),"original":fault.original_message,"retained":client.original_unknown()}),
            ),
        }
    }
}

#[test]
fn real_opening_pipe_serves_scoped_prompt_resource_and_poisons_failed_stdout() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let before = v.snapshot().unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    let mut agent = Agent::spawn();
    let enrollment = Enrollment::enroll(
        agent.child.id(),
        principal,
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
        });
        agent.connect(&token, &e.descriptor.vault_id, "opening");
        assert_eq!(agent.event()["connected"], true);
        agent.initialize();
        let prompt=agent.rpc(json!({"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"memory_start","arguments":{"request_id":request.request_id,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap()}}}));
        assert_eq!(prompt["response"]["result"]["messages"][0]["role"], "user");
        assert_eq!(prompt["response"]["id"], json!(2));
        let uri = format!("enouia-memory://recall/{}", request.request_id.as_str());
        let resource = agent
            .rpc(json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":uri}}));
        let data: Value = serde_json::from_str(
            resource["response"]["result"]["contents"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(data["data"]["request_id"], json!(request.request_id));
        assert_eq!(resource["response"]["id"], json!(3));
        let tools = agent.rpc(json!({"jsonrpc":"2.0","id":15,"method":"tools/list"}));
        let session_tool = tools["response"]["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .find(|t| t["name"] == "session_start")
            .unwrap();
        let mut exchanges = vec![
            json!({"method":"prompts/get","response":prompt["response"]}),
            json!({"method":"resources/read","response":resource["response"]}),
        ];
        for (n,(method,params)) in [
            ("resources/list",json!({})),
            ("resources/templates/list",json!({})),
            ("prompts/list",json!({})),
            ("resources/read",json!({"uri":opening::GUIDE_URI})),
            ("resources/read",json!({"uri":format!("enouia-memory://memory/{}",request.target.memory.memory_id.as_str())})),
        ].into_iter().enumerate() {
            let response=agent.rpc(json!({"jsonrpc":"2.0","id":10+n,"method":method,"params":params}));
            assert_eq!(response["response"]["id"],json!(10+n));
            if method=="resources/read" {
                assert_eq!(response["response"]["result"]["contents"][0]["uri"],params["uri"]);
            }
            exchanges.push(json!({"method":method,"response":response["response"]}));
        }
        let schemas = schema_support::schema::SchemaStore::load();
        let mut bundles = serde_json::Map::new();
        let mut payloads = Vec::new();
        for exchange in &exchanges {
            let method = exchange["method"].as_str().unwrap();
            let response = &exchange["response"];
            let errors = schemas.validate("next/mcp-opening-wire-v1.schema.json", response);
            assert!(errors.is_empty(), "{method}: {errors:?}");
            bundles.insert(method.into(), opening::response_schema(method).unwrap());
            let text = match method {
                "resources/read" => {
                    Some(response["result"]["contents"][0]["text"].as_str().unwrap())
                }
                "prompts/get" => Some(
                    response["result"]["messages"][0]["content"]["text"]
                        .as_str()
                        .unwrap()
                        .split_once('\n')
                        .unwrap()
                        .1,
                ),
                _ => None,
            };
            if let Some(text) = text {
                let payload: Value = serde_json::from_str(text).unwrap();
                assert_eq!(payload["kind"], "agent_context_resource");
                if method == "prompts/get" {
                    assert_eq!(payload["resource_type"], "recall");
                }
                if method == "resources/read" {
                    assert_eq!(payload["uri"], response["result"]["contents"][0]["uri"]);
                }
                let errors = schemas.validate("next/mcp-opening-v1.schema.json", &payload);
                assert!(errors.is_empty(), "{method} embedded payload: {errors:?}");
                payloads.push(json!({"method":method,"payload":payload}));
            }
        }
        assert!(opening::response_schema("tools/call").is_none());
        // Reconstruct the fingerprint from the actual advertised wire data.
        let hello = opening::native::hello(&e.descriptor.vault_id);
        let mut descriptor = hello.clone();
        descriptor.as_object_mut().unwrap().remove("vaultId");
        descriptor
            .as_object_mut()
            .unwrap()
            .remove("compatibilityHash");
        let catalog = |method: &str, key: &str| {
            exchanges.iter().find(|e| e["method"] == method).unwrap()["response"]["result"][key]
                .clone()
        };
        let preamble = format!(
            "{}\n",
            prompt["response"]["result"]["messages"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .split_once('\n')
                .unwrap()
                .0
        );
        let mut material = json!({"descriptor":descriptor,
            "readCompatibility":enouia_memory_host::mcp::initialized::native::hello(&e.descriptor.vault_id)["compatibilityHash"],
            "startupInput":opening::schema(true,false),"startupOutput":opening::schema(true,true),
            "resourceInput":opening::schema(false,false),"resourceOutput":opening::schema(false,true),
            "guide":payloads.iter().find(|p|p["payload"]["resource_type"]=="guide").unwrap()["payload"]["data"],
            "resources":catalog("resources/list","resources"),"templates":catalog("resources/templates/list","resourceTemplates"),
            "prompts":catalog("prompts/list","prompts"),"preamble":preamble,"sessionStartTool":session_tool,"sessionToolResponse":opening::session_result_schema()});
        let previous = sha256(&canonical_bytes(&material).unwrap());
        material["responses"] = json!(bundles);
        assert_eq!(
            hello["compatibilityHash"],
            json!(sha256(&canonical_bytes(&material).unwrap()))
        );
        assert_ne!(hello["compatibilityHash"], json!(previous));
        let mut without_link_contract = material.clone();
        without_link_contract
            .as_object_mut()
            .unwrap()
            .remove("sessionToolResponse");
        assert_ne!(
            hello["compatibilityHash"],
            json!(sha256(&canonical_bytes(&without_link_contract).unwrap()))
        );
        if let Some(path) = std::env::var_os("ENOUIA_MCP_OPENING_TRANSCRIPT") {
            let payload_schemas = json!({"prompts/get":opening::schema(true,true),"resources/read":opening::schema(false,true)});
            std::fs::write(
                path,
                serde_json::to_vec(
                    &json!({"exchanges":exchanges,"schemas":bundles,"payloads":payloads,"payload_schemas":payload_schemas}),
                )
                .unwrap(),
            )
            .unwrap();
        }
        agent.finish();
        let _ = server.join().unwrap();
    });
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(reader.context_trace(&request.request_id).is_err());
    // Another explicitly enrolled connection; failure keeps the complete packet.
    let reader = host
        .reader(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    let mut agent = Agent::spawn();
    let enrollment = Enrollment::enroll(
        agent.child.id(),
        actor(ActorType::Agent, 88),
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
        });
        agent.connect(&token, &e.descriptor.vault_id, "opening");
        assert_eq!(agent.event()["connected"], true);
        let raw = json!({"jsonrpc":"2.0","id":"original-resource","method":"resources/read","params":{"uri":opening::GUIDE_URI,"_meta":{"trace":["complete",{"original":true}]}}});
        let mut messages = initialization().to_vec();
        messages.push(raw.clone());
        agent.send(json!({"stdio":messages,"fail_output":true}));
        let result = agent.event();
        assert_eq!(result["stdio_ok"], false);
        assert_eq!(result["retained"], raw);
        let stopped =
            agent.rpc(json!({"jsonrpc":"2.0","id":"replacement","method":"resources/list"}));
        assert!(stopped.get("fault").is_some());
        assert_eq!(stopped["retained"], raw);
        agent.finish();
        let _ = server.join().unwrap();
    });
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn real_opening_pipe_refuses_other_profiles_claims_write_scopes_and_wrong_process() {
    for (mode, scopes, wrong_peer) in [
        ("old5", vec![Scope::MemoryRead, Scope::SourceRead], false),
        ("old6", vec![Scope::MemoryRead, Scope::SourceRead], false),
        ("old7", vec![Scope::MemoryRead, Scope::SourceRead], false),
        ("old8", vec![Scope::MemoryRead, Scope::SourceRead], false),
        ("old9", vec![Scope::MemoryRead, Scope::SourceRead], false),
        (
            "wrong_agent",
            vec![Scope::MemoryRead, Scope::SourceRead],
            false,
        ),
        (
            "wrong_vault",
            vec![Scope::MemoryRead, Scope::SourceRead],
            false,
        ),
        ("opening", vec![Scope::SourceRead], false),
        ("opening", vec![Scope::ContextRead], false),
        (
            "bad_hash",
            vec![Scope::MemoryRead, Scope::SourceRead],
            false,
        ),
        (
            "forged_owner",
            vec![Scope::MemoryRead, Scope::SourceRead],
            false,
        ),
        ("opening", vec![Scope::MemoryRead], false),
        (
            "opening",
            vec![Scope::MemoryRead, Scope::SourceRead, Scope::MemoryPropose],
            false,
        ),
        (
            "opening",
            vec![Scope::MemoryRead, Scope::SourceRead, Scope::ContextRead],
            false,
        ),
        ("opening", vec![Scope::MemoryRead, Scope::SourceRead], true),
    ] {
        let (e, v, p, _) = correction_rig(SpeakerRole::User);
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let token = sha256(&OsIdSource.random_16());
        let before = v.snapshot().unwrap();
        let mut pipe = Pipe::listen(&token).unwrap();
        let mut agent = Agent::spawn();
        let enrollment = Enrollment::enroll(
            if wrong_peer {
                std::process::id()
            } else {
                agent.child.id()
            },
            if mode == "wrong_agent" {
                actor(ActorType::Agent, 89)
            } else {
                principal
            },
            scopes,
        )
        .unwrap();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
            });
            let vault = if mode == "wrong_vault" {
                VaultId::parse(&id("vlt", 999)).unwrap()
            } else {
                e.descriptor.vault_id.clone()
            };
            if mode == "wrong_vault" {
                assert_ne!(vault, e.descriptor.vault_id);
            }
            agent.connect(&token, &vault, mode);
            assert_eq!(agent.event()["refused"], true, "{mode}");
            agent.finish();
            assert!(server.join().unwrap().is_err());
        });
        assert!(
            reader
                .context_trace(&RequestId::parse(&id("req", 30)).unwrap())
                .is_err()
        );
        assert_eq!(v.snapshot().unwrap(), before);
    }
}

#[test]
fn real_opening_unknown_gate_disconnects_without_reply_and_retains_original_packet() {
    for session_tool in [false, true] {
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
            .correction_reporter(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let token = sha256(&OsIdSource.random_16());
        let mut pipe = Pipe::listen(&token).unwrap();
        let mut agent = Agent::spawn();
        let enrollment = Enrollment::enroll(
            agent.child.id(),
            principal,
            vec![Scope::MemoryRead, Scope::SourceRead],
        )
        .unwrap();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
            });
            agent.connect(&token, &e.descriptor.vault_id, "opening");
            assert_eq!(agent.event()["connected"], true);
            agent.initialize();
            faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(118));
            assert!(writer.submit(&request).is_err());
            let raw = json!({"jsonrpc":"2.0","id":"original-prompt","method":"prompts/get","params":{"name":"memory_start","arguments":{"request_id":request.request_id,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap()},"_meta":{"retained":"complete packet"}}});
            let raw = if session_tool {
                json!({"jsonrpc":"2.0","id":"original-session-start","method":"tools/call","params":{"name":"session_start","arguments":{"schema_version":2,"kind":"agent_session_start_request","capability":opening::SESSION_CAPABILITY,"request":{"request_id":request.request_id,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap()}},"_meta":{"retained":"complete packet"}}})
            } else {
                raw
            };
            let failed = agent.rpc(raw.clone());
            assert!(failed.get("response").is_none());
            assert_eq!(failed["original"], raw);
            assert_eq!(failed["retained"], raw);
            agent.finish();
            assert!(server.join().unwrap().is_err());
        });
    }
}

#[test]
fn explicit_context_launcher_attaches_without_root_or_client_configuration() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let before = v.snapshot().unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_enouia-memory-mcp"))
        .args([
            "--protocol",
            "automation-context-v1",
            "--pipe",
            token.as_str(),
            "--host-pid",
            &std::process::id().to_string(),
            "--vault-id",
            e.descriptor.vault_id.as_str(),
        ])
        .creation_flags(0x08000000)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let enrollment = Enrollment::enroll(
        child.id(),
        principal,
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
        });
        let mut input = child.stdin.take().unwrap();
        for message in initialization() {
            writeln!(input, "{message}").unwrap();
        }
        writeln!(input,"{}",json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":opening::GUIDE_URI}})).unwrap();
        drop(input);
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success(), "{:?}", output.stderr);
        let replies: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(replies.len(), 2);
        assert_eq!(
            replies[0]["result"]["capabilities"],
            json!({"tools":{},"resources":{},"prompts":{}})
        );
        assert_eq!(
            replies[1]["result"]["contents"][0]["uri"],
            opening::GUIDE_URI
        );
        let _ = server.join().unwrap();
    });
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn real_opening_memory_read_does_not_require_or_infer_source_read() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    for source_enrollment in [false, true] {
        let mut grant = scope(&p);
        grant.source_read = false;
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), grant)
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let before = v.snapshot().unwrap();
        let token = sha256(&OsIdSource.random_16());
        let mut pipe = Pipe::listen(&token).unwrap();
        let mut agent = Agent::spawn();
        let mut scopes = vec![Scope::MemoryRead];
        if source_enrollment {
            scopes.push(Scope::SourceRead);
        }
        let enrollment = Enrollment::enroll(agent.child.id(), principal.clone(), scopes).unwrap();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
            });
            agent.connect(&token, &e.descriptor.vault_id, "opening");
            assert_eq!(agent.event()["connected"], true);
            agent.initialize();
            let uri = format!(
                "enouia-memory://memory/{}",
                request.target.memory.memory_id.as_str()
            );
            let response = agent.rpc(
                json!({"jsonrpc":"2.0","id":2,"method":"resources/read","params":{"uri":uri}}),
            );
            let data: Value = serde_json::from_str(
                response["response"]["result"]["contents"][0]["text"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(data["data"]["source_read"], false);
            assert_eq!(data["data"]["sources"], json!([]));
            let denied=agent.rpc(json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_source","arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":enouia_memory_host::mcp::initialized::CAPABILITY,"request_id":id("req",140),"operation":"memory_source","arguments":{"source_id":request.source.source_id,"revision":null,"from_byte":0,"to_byte":null,"max_bytes":4096}}}}));
            assert_eq!(denied["response"]["result"]["isError"], true);
            assert_eq!(
                denied["response"]["result"]["structuredContent"]["error"]["code"],
                "permission_denied"
            );
            assert!(denied["response"]["result"]["structuredContent"]["data"].is_null());
            assert!(!denied.to_string().contains("前缀"));
            let recalled=agent.rpc(json!({"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"memory_recall","arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":enouia_memory_host::mcp::initialized::CAPABILITY,"request_id":id("req",141),"operation":"memory_recall","arguments":{"text":"","conditions":[],"event_window":null,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap(),"intent":"browse","limit":1,"max_bytes":16384}}}}));
            assert!(recalled["response"]["result"]["structuredContent"]["error"].is_null());
            assert_eq!(
                recalled["response"]["result"]["structuredContent"]["data"]["items"]
                    .as_array()
                    .unwrap()
                    .len(),
                1
            );
            let uri = format!("enouia-memory://recall/{}", id("req", 141));
            let resource = agent.rpc(
                json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":uri}}),
            );
            let cached: Value = serde_json::from_str(
                resource["response"]["result"]["contents"][0]["text"]
                    .as_str()
                    .unwrap(),
            )
            .unwrap();
            assert_eq!(
                cached["data"],
                recalled["response"]["result"]["structuredContent"]["data"]
            );
            agent.finish();
            let _ = server.join().unwrap();
        });
        assert_eq!(v.snapshot().unwrap(), before);
    }
}

#[test]
fn real_opening_stale_context_requires_explicit_new_read_on_the_same_connection() {
    use enouia_memory_vault::automation::segmented::AgentNoteRequest;
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let note_writer = host
        .note_writer(&e.descriptor.created_by, principal, scope(&p))
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    let mut agent = Agent::spawn();
    let enrollment = Enrollment::enroll(
        agent.child.id(),
        actor(ActorType::Agent, 88),
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    let old = RequestId::parse(&id("req", 121)).unwrap();
    let fresh = RequestId::parse(&id("req", 122)).unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
        });
        agent.connect(&token, &e.descriptor.vault_id, "opening");
        assert_eq!(agent.event()["connected"], true);
        agent.initialize();
        let first=agent.rpc(json!({"jsonrpc":"2.0","id":2,"method":"prompts/get","params":{"name":"memory_start","arguments":{"request_id":old,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap()}}}));
        let initial: Value = serde_json::from_str(
            first["response"]["result"]["messages"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .split_once('\n')
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(initial["data"]["items"].as_array().unwrap().len(), 1);
        let old_uri = format!("enouia-memory://recall/{}", old.as_str());
        assert!(
            agent.rpc(
                json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":old_uri}})
            )["response"]
                .get("result")
                .is_some()
        );
        note_writer
            .submit(&AgentNoteRequest {
                request_id: request.request_id.clone(),
                source: request.source.clone(),
                quote_hint: request.quote_hint.clone(),
                note: "UNTRUSTED_STALE_CONTEXT_NOTE: not a memory or evidence".into(),
            })
            .unwrap();
        let actual = v
            .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
                (*data.sources.get(&request.source.source_id).unwrap()).clone()
            })
            .unwrap();
        assert_eq!(actual.record.evidence_class, EvidenceClass::UserStatement);
        // Synthetic System formation from the already authorized real Runtime
        // capture; no analyzer/model or new capture authority is introduced.
        publish(&e, &v, e.input(&actual, &p, 11));
        let after_writes = v.snapshot().unwrap();
        let stale=agent.rpc(json!({"jsonrpc":"2.0","id":4,"method":"resources/read","params":{"uri":old_uri,"_meta":{"noReplay":true}}}));
        assert_eq!(stale["response"]["error"]["message"], "stale_scope");
        assert!(stale["response"].get("result").is_none());
        assert_eq!(v.snapshot().unwrap(), after_writes);
        let next=agent.rpc(json!({"jsonrpc":"2.0","id":5,"method":"prompts/get","params":{"name":"memory_start","arguments":{"request_id":fresh,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap()}}}));
        let current: Value = serde_json::from_str(
            next["response"]["result"]["messages"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .split_once('\n')
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(current["data"]["items"].as_array().unwrap().len(), 2);
        assert_eq!(current["data"]["scope_hash"], initial["data"]["scope_hash"]);
        assert_ne!(
            current["data"]["trace"]["snapshot"],
            initial["data"]["trace"]["snapshot"]
        );
        assert!(!current.to_string().contains("UNTRUSTED_STALE_CONTEXT_NOTE"));
        let uri = format!("enouia-memory://recall/{}", fresh.as_str());
        let resource = agent
            .rpc(json!({"jsonrpc":"2.0","id":6,"method":"resources/read","params":{"uri":uri}}));
        assert_eq!(
            serde_json::from_str::<Value>(
                resource["response"]["result"]["contents"][0]["text"]
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
            current
        );
        assert_eq!(
            agent.rpc(
                json!({"jsonrpc":"2.0","id":7,"method":"resources/read","params":{"uri":old_uri}})
            )["response"]["error"]["message"],
            "resource_not_found"
        );
        assert_eq!(v.snapshot().unwrap(), after_writes);
        agent.finish();
        let _ = server.join().unwrap();
    });
    assert!(reader.context_trace(&fresh).is_err());
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
        assert_eq!(data.memories.len(), 2);
        assert!(data.owner_actions.is_empty());
    })
    .unwrap();
}

#[test]
fn real_opening_authority_loss_poisoned_connection_never_recovers_itself() {
    for invalidation in 0..5 {
        let (e, v, p, _) = correction_rig(SpeakerRole::User);
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let token = sha256(&OsIdSource.random_16());
        let mut pipe = Pipe::listen(&token).unwrap();
        let mut agent = Agent::spawn();
        let enrollment = Enrollment::enroll(
            agent.child.id(),
            principal.clone(),
            vec![Scope::MemoryRead, Scope::SourceRead],
        )
        .unwrap();
        let kernel_grant = enrollment.revocation();
        let original = json!({"jsonrpc":"2.0","id":"original-authorized-read","method":"resources/read","params":{"uri":opening::GUIDE_URI,"_meta":{"trace":["full",{"keep":true}]}}});
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
            });
            agent.connect(&token, &e.descriptor.vault_id, "opening");
            assert_eq!(agent.event()["connected"], true);
            agent.initialize();
            match invalidation {
                0 => reader.revoke(),
                1 => {
                    host.lock().unwrap();
                    host.unlock(&e.descriptor.created_by).unwrap();
                }
                2 => {
                    let mut replacement = p.clone();
                    replacement.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
                    v.set_policy(&e.descriptor.created_by, replacement).unwrap();
                }
                3 => e.clock.set(NOW + 120_001),
                _ => kernel_grant.revoke(),
            }
            let after_invalidation = v.snapshot().unwrap();
            let failed = agent.rpc(original.clone());
            assert!(
                failed.get("response").is_none(),
                "invalidation {invalidation}: {failed}"
            );
            assert_eq!(failed["fault"], "OutcomeUnknown");
            assert_eq!(failed["original"], original);
            assert_eq!(failed["retained"], original);
            let mut changed = original.clone();
            changed["id"] = json!("replacement-read");
            let stopped = agent.rpc(changed);
            assert_eq!(stopped["fault"], "Closed");
            assert_eq!(stopped["retained"], original);
            assert_eq!(v.snapshot().unwrap(), after_invalidation);
            agent.finish();
            assert!(server.join().unwrap().is_err());
        });
        assert!(
            reader
                .context_trace(&RequestId::parse(&id("req", 121)).unwrap())
                .is_err()
        );
        if invalidation == 1 {
            // The native test operator explicitly creates new authority and an
            // owned channel after unlock. Neither the old client nor old reader
            // resumes, and the original read Value is not silently replaced.
            let fresh = host
                .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
                .unwrap();
            let token = sha256(&OsIdSource.random_16());
            let mut pipe = Pipe::listen(&token).unwrap();
            let mut agent = Agent::spawn();
            let enrollment = Enrollment::enroll(
                agent.child.id(),
                principal,
                vec![Scope::MemoryRead, Scope::SourceRead],
            )
            .unwrap();
            std::thread::scope(|scope| {
                let server = scope.spawn(|| {
                    pipe.accept(TIMEOUT).unwrap();
                    opening::native::serve_connection(
                        &fresh, &profile, &mut pipe, enrollment, TIMEOUT,
                    )
                });
                agent.connect(&token, &e.descriptor.vault_id, "opening");
                assert_eq!(agent.event()["connected"], true);
                agent.initialize();
                let response = agent.rpc(original.clone());
                assert_eq!(response["response"]["id"], original["id"]);
                assert!(response["response"].get("result").is_some());
                agent.finish();
                let _ = server.join().unwrap();
            });
            assert!(
                reader
                    .context_trace(&RequestId::parse(&id("req", 121)).unwrap())
                    .is_err()
            );
        }
    }
}

#[test]
fn real_opening_tools_and_prompt_share_actual_context_without_requiring_an_opening_prompt() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let before = v.snapshot().unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    let mut agent = Agent::spawn();
    let enrollment = Enrollment::enroll(
        agent.child.id(),
        principal,
        vec![Scope::MemoryRead, Scope::SourceRead],
    )
    .unwrap();
    let first = RequestId::parse(&id("req", 130)).unwrap();
    let prompt_id = RequestId::parse(&id("req", 131)).unwrap();
    let invalid = RequestId::parse(&id("req", 132)).unwrap();
    let last = RequestId::parse(&id("req", 133)).unwrap();
    let recall = |rpc_id: i64, request_id: &RequestId, max_bytes: u64| json!({"jsonrpc":"2.0","id":rpc_id,"method":"tools/call","params":{"name":"memory_recall","arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":enouia_memory_host::mcp::initialized::CAPABILITY,"request_id":request_id,"operation":"memory_recall","arguments":{"text":"","conditions":[],"event_window":null,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap(),"intent":"browse","limit":1,"max_bytes":max_bytes}}}});
    let resource_payload = |response: Value| {
        serde_json::from_str::<Value>(
            response["response"]["result"]["contents"][0]["text"]
                .as_str()
                .unwrap(),
        )
        .unwrap()
    };
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
        });
        agent.connect(&token, &e.descriptor.vault_id, "opening");
        assert_eq!(agent.event()["connected"], true);
        agent.initialize();
        let first_response = agent.rpc(recall(2, &first, 16384));
        let first_data = &first_response["response"]["result"]["structuredContent"];
        assert_eq!(first_data["kind"], "agent_initialized_read_response");
        assert!(first_data["error"].is_null());
        let memory = &request.target.memory.memory_id;
        let filter = CatalogFilter::default();
        let reads = [
            (
                "memory_search",
                json!({"include_facets":false,"filter":filter,"cursor":null,"limit":1,"max_bytes":65536}),
                serde_json::to_value(
                    reader
                        .page(&profile, &filter, None, 1, 65536, false)
                        .unwrap(),
                )
                .unwrap(),
            ),
            (
                "memory_read",
                json!({"memory_id":memory,"expected":null,"history_cursor":null,"include_source_text":false,"max_bytes":65536}),
                serde_json::to_value(
                    reader
                        .detail_page(&profile, memory, None, false, 65536, None)
                        .unwrap(),
                )
                .unwrap(),
            ),
            (
                "memory_source",
                json!({"source_id":request.source.source_id,"revision":null,"from_byte":0,"to_byte":null,"max_bytes":4096}),
                serde_json::to_value(
                    reader
                        .source(&request.source.source_id, None, 0, None, 4096)
                        .unwrap(),
                )
                .unwrap(),
            ),
            (
                "memory_related",
                json!({"memory_id":memory,"expected":null,"max_bytes":65536}),
                serde_json::to_value(reader.related(&profile, memory, None, 65536).unwrap())
                    .unwrap(),
            ),
        ];
        let schemas = schema_support::schema::SchemaStore::load();
        for (n, (operation, arguments, expected)) in reads.into_iter().enumerate() {
            let response=agent.rpc(json!({"jsonrpc":"2.0","id":20+n,"method":"tools/call","params":{"name":operation,"arguments":{"schema_version":2,"kind":"agent_initialized_read_request","capability":enouia_memory_host::mcp::initialized::CAPABILITY,"request_id":id("req",150+n as u32),"operation":operation,"arguments":arguments}}}));
            let envelope = &response["response"]["result"]["structuredContent"];
            assert!(envelope["error"].is_null(), "{operation}: {response}");
            assert_eq!(envelope["kind"], "agent_initialized_read_response");
            assert_eq!(envelope["operation"], operation);
            assert_eq!(envelope["data"], expected);
            let errors = schemas.validate("next/agent-initialized-read-v2.schema.json", envelope);
            assert!(errors.is_empty(), "{operation}: {errors:?}");
        }
        let uri = format!("enouia-memory://recall/{}", first.as_str());
        assert_eq!(
            resource_payload(agent.rpc(
                json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":uri}})
            ))["data"],
            first_data["data"]
        );
        let prompt=agent.rpc(json!({"jsonrpc":"2.0","id":4,"method":"prompts/get","params":{"name":"memory_start","arguments":{"request_id":prompt_id,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap()}}}));
        let prompt_payload: Value = serde_json::from_str(
            prompt["response"]["result"]["messages"][0]["content"]["text"]
                .as_str()
                .unwrap()
                .split_once('\n')
                .unwrap()
                .1,
        )
        .unwrap();
        assert_eq!(
            agent.rpc(
                json!({"jsonrpc":"2.0","id":5,"method":"resources/read","params":{"uri":uri}})
            )["response"]["error"]["message"],
            "resource_not_found"
        );
        let prompt_uri = format!("enouia-memory://recall/{}", prompt_id.as_str());
        let failed = agent.rpc(recall(6, &invalid, 0));
        assert_eq!(failed["response"]["result"]["isError"], true);
        assert_eq!(
            failed["response"]["result"]["structuredContent"]["error"]["code"],
            "invalid_request"
        );
        assert!(failed["response"]["result"]["structuredContent"]["data"].is_null());
        assert_eq!(resource_payload(agent.rpc(json!({"jsonrpc":"2.0","id":7,"method":"resources/read","params":{"uri":prompt_uri}}))),prompt_payload);
        let final_response = agent.rpc(recall(8, &last, 16384));
        let final_data = &final_response["response"]["result"]["structuredContent"];
        assert!(final_data["error"].is_null());
        assert_eq!(
            agent.rpc(json!({"jsonrpc":"2.0","id":8,"method":"resources/list"}))["response"]["error"]
                ["message"],
            "duplicate_request_id"
        );
        let final_uri = format!("enouia-memory://recall/{}", last.as_str());
        assert_eq!(
            resource_payload(agent.rpc(
                json!({"jsonrpc":"2.0","id":9,"method":"resources/read","params":{"uri":final_uri}})
            ))["data"],
            final_data["data"]
        );
        assert_eq!(agent.rpc(json!({"jsonrpc":"2.0","id":10,"method":"resources/read","params":{"uri":prompt_uri}}))["response"]["error"]["message"],"resource_not_found");
        assert_eq!(v.snapshot().unwrap(), before);
        agent.finish();
        let _ = server.join().unwrap();
    });
    assert!(reader.context_trace(&last).is_err());
}

#[test]
fn real_session_start_tool_returns_actual_resource_without_creating_a_session() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let mut read_scope = scope(&p);
    read_scope.source_read = false;
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), read_scope)
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let before = v.snapshot().unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    let mut agent = Agent::spawn();
    let enrollment =
        Enrollment::enroll(agent.child.id(), principal, vec![Scope::MemoryRead]).unwrap();
    let request_id = RequestId::parse(&id("req", 170)).unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
        });
        agent.connect(&token, &e.descriptor.vault_id, "opening");
        assert_eq!(agent.event()["connected"], true);
        agent.initialize();
        let tools = agent.rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
        assert_eq!(
            tools["response"]["result"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            6
        );
        let call = json!({"jsonrpc":"2.0","id":"original-session-read","method":"tools/call","params":{"name":"session_start","arguments":{"schema_version":2,"kind":"agent_session_start_request","capability":opening::SESSION_CAPABILITY,"request":{"request_id":request_id,"at":enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap()}},"_meta":{"trace":"synthetic session recipe"}}});
        let response = agent.rpc(call.clone());
        let packet = &response["response"]["result"]["structuredContent"];
        assert_eq!(response["response"]["id"], call["id"]);
        assert_eq!(packet["request_id"], json!(request_id));
        assert_eq!(packet["kind"], "agent_session_start_response");
        assert_eq!(
            packet["context"]["data"]["items"].as_array().unwrap().len(),
            1
        );
        let schemas = schema_support::schema::SchemaStore::load();
        assert!(
            schemas
                .validate("next/mcp-session-start-v1.schema.json", packet)
                .is_empty()
        );
        let link = &response["response"]["result"]["content"][1];
        assert_eq!(link["type"], "resource_link");
        assert_eq!(link["uri"], packet["context"]["uri"]);
        assert!(
            schemas
                .validate(
                    "next/mcp-session-start-wire-v1.schema.json",
                    &response["response"]
                )
                .is_empty()
        );
        let resource = agent.rpc(
            json!({"jsonrpc":"2.0","id":3,"method":"resources/read","params":{"uri":link["uri"]}}),
        );
        assert_eq!(
            serde_json::from_str::<Value>(
                resource["response"]["result"]["contents"][0]["text"]
                    .as_str()
                    .unwrap()
            )
            .unwrap(),
            packet["context"]
        );
        if let Some(path) = std::env::var_os("ENOUIA_SESSION_START_TRANSCRIPT") {
            std::fs::write(path,serde_json::to_vec(&json!({"arguments":call["params"]["arguments"],"response":packet,"input_schema":opening::session_schema(false),"output_schema":opening::session_schema(true),"wire_response":response["response"],"wire_schema":opening::session_result_schema()})).unwrap()).unwrap();
        }
        assert_eq!(v.snapshot().unwrap(), before);
        agent.finish();
        let _ = server.join().unwrap();
    });
    assert!(reader.context_trace(&request_id).is_err());
}

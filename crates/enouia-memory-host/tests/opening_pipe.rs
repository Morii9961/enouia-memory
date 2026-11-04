#![cfg(windows)]
#[path = "support/agent_correction_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*,
    hash::{Sha256Hex, sha256},
    ids::*,
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
        ("opening", vec![Scope::MemoryRead, Scope::SourceRead], true),
    ] {
        let (e, v, p, _) = correction_rig(SpeakerRole::User);
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v,
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
            if wrong_peer {
                std::process::id()
            } else {
                agent.child.id()
            },
            principal,
            scopes,
        )
        .unwrap();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                opening::native::serve_connection(&reader, &profile, &mut pipe, enrollment, TIMEOUT)
            });
            agent.connect(&token, &e.descriptor.vault_id, mode);
            assert_eq!(agent.event()["refused"], true, "{mode}");
            agent.finish();
            assert!(server.join().unwrap().is_err());
        });
        assert!(
            reader
                .context_trace(&RequestId::parse(&id("req", 30)).unwrap())
                .is_err()
        );
    }
}

#[test]
fn real_opening_unknown_gate_disconnects_without_reply_and_retains_original_packet() {
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
        let failed = agent.rpc(raw.clone());
        assert!(failed.get("response").is_none());
        assert_eq!(failed["original"], raw);
        assert_eq!(failed["retained"], raw);
        agent.finish();
        assert!(server.join().unwrap().is_err());
    });
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

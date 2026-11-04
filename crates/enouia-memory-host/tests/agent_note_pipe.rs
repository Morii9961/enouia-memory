#![cfg(windows)]
#[path = "support/agent_note_fixture.rs"]
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
    mcp::{native::Enrollment, notes},
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_vault::{
    OsIdSource,
    automation::segmented::*,
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin, ChildStdout, Command, Stdio},
    sync::Arc,
    time::{Duration, Instant},
};
const TIMEOUT: Duration = Duration::from_secs(15);

fn initialization() -> [Value; 2] {
    [
        json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-agent","version":"1"}}}),
        json!({"jsonrpc":"2.0","method":"notifications/initialized"}),
    ]
}
fn note_call(request: &enouia_memory_vault::automation::segmented::AgentNoteRequest) -> Value {
    json!({"jsonrpc":"2.0","id":"original-note","method":"tools/call","params":{"name":"memory_note","arguments":{"schema_version":2,"kind":"agent_note_request","capability":notes::CAPABILITY,"request":request}}})
}
struct Agent {
    child: Child,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
}
impl Agent {
    fn spawn() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "note_pipe_process_child", "--nocapture"])
            .env("ENOUIA_NOTE_PIPE_CHILD", "1")
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
            if let Some((_, value)) = line.split_once("note-event ") {
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
    println!("note-event {value}");
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
fn note_pipe_process_child() {
    if std::env::var_os("ENOUIA_NOTE_PIPE_CHILD").is_none() {
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
    if mode == "old5" {
        event(
            json!({"refused":enouia_memory_host::mcp::initialized::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT).is_err()}),
        );
        wait_exit(&mut input);
        return;
    }
    if mode == "bad_hash" || mode == "forged_owner" {
        let mut hello = notes::native::hello(&vault);
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
    let mut client =
        match notes::native::Client::connect(Pipe::connect(&id).unwrap(), peer, &vault, TIMEOUT) {
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
fn real_note_pipe_saves_replays_and_keeps_five_reads_with_the_explicit_write_tool() {
    let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let writer = host
        .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
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
            notes::native::serve_connection(
                &reader, &writer, &profile, &mut pipe, enrollment, TIMEOUT,
            )
        });
        agent.connect(&token, &e.descriptor.vault_id, "notes");
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
        let message = note_call(&request);
        let saved = agent.rpc(message.clone())["response"]["result"]["structuredContent"].clone();
        assert_eq!(saved["receipt"]["state"], "hint_saved");
        let after = v.snapshot().unwrap();
        let mut replay = message;
        replay["id"] = json!("explicit-replay");
        assert_eq!(
            agent.rpc(replay)["response"]["result"]["structuredContent"],
            saved
        );
        assert_eq!(v.snapshot().unwrap(), after);
        agent.finish();
        let _ = server.join().unwrap();
    });
    assert!(writer.check_connection().is_err());
    // Same raw capture, explicitly replayed by the native Runtime producer:
    // note submission did not create this source or queue it.
    let permit = v
        .authorize_owner_runtime_capture(
            &e.descriptor.created_by,
            actor(ActorType::Client, 77),
            RuntimeCaptureScope {
                policy_id: p.policy_id.clone(),
                source_ids: p.source_ids.clone(),
                max_sensitivity: Sensitivity::Private,
                platform: "enouia-runtime".into(),
                account_scope: "account-synthetic".into(),
                default_timezone: "Asia/Shanghai".into(),
                valid_until: scope(&p).valid_until,
            },
        )
        .unwrap();
    let mut raw: RuntimeCaptureRequest =
        serde_json::from_value(value("runtime-owner-capture/cases.json")["base"].clone()).unwrap();
    raw.turns[0].text = "A synthetic preference.".into();
    raw.turns[0].speaker = SpeakerRole::User;
    raw.turns[0].assistant_model = None;
    raw.turns[0].attachments.clear();
    let captured = v.capture_runtime(&permit, &raw, false).unwrap();
    assert!(captured.replayed);
    v.enqueue_captured(&e.descriptor.system_actor, &captured, &p.reference())
        .unwrap();
    struct Analyzer {
        calls: usize,
    }
    impl CaptureAnalyzer for Analyzer {
        fn start(
            &mut self,
            _: &QueuedCapture,
            source: &CapturedSource,
            notes: &[AgentNoteRecord],
            _: &CaptureStopToken,
        ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
            self.calls += 1;
            assert_eq!(notes.len(), 1);
            assert_eq!(notes[0].principal.actor_type, ActorType::Agent);
            assert_eq!(notes[0].evidence.speaker, SpeakerRole::User);
            let unknown = RecordTime {
                from: None,
                until: None,
                precision: RecordPrecision::Unknown,
                bound: TimeBound::Within,
                tz: None,
                basis: TimeBasis::Unknown,
            };
            Err(CaptureAnalysis::Statements(vec![CaptureStatement {
                quote: source.text.clone(),
                title: "Synthetic captured result".into(),
                interpretation: Interpretation::ClearSelfReport,
                dimensions: FormationDimensions {
                    kind: FormationKind::Preference,
                    occurrence: OccurrenceAssertion::Unknown,
                    state: None,
                    change: FormationChange::Initial,
                    conditions: vec![],
                    event_time: None,
                    valid_time: None,
                    counter_sources: vec![],
                    derived_from: vec![],
                    reverts_to: None,
                },
                event_time: unknown.clone(),
                valid_time: unknown,
                note: Some("Synthetic optional cue considered.".into()),
                prompt_version: "note-pipe-synthetic-1".into(),
                model_version: None,
            }]))
        }
    }
    let mut analyzer = Analyzer { calls: 0 };
    let mut worker = CapturedFormation::open(v.clone(), &e.descriptor.system_actor).unwrap();
    for _ in 0..16 {
        if !worker.step(&mut analyzer).unwrap() {
            break;
        }
    }
    assert_eq!(analyzer.calls, 1);
    assert_eq!(worker.status()[0].state, FormationJobState::Formed);
    assert_eq!(worker.status()[0].receipts.len(), 1);
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
        assert_eq!(data.sources.len(), 1);
        assert_eq!(data.memories.len(), 1);
        assert_eq!(
            data.memories[0].memory.decision.input.content,
            "A synthetic preference."
        );
        assert_eq!(
            data.memories[0].memory.decision.input.evidence[0].evidence_class,
            EvidenceClass::UserStatement
        );
    })
    .unwrap();
}

#[test]
fn real_note_pipe_refuses_old_profiles_hashes_claims_scope_and_wrong_kernel_peers() {
    for (mode, scopes, foreign) in [
        ("old5", vec![Scope::MemoryRead, Scope::SourceRead], false),
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
        ("notes", vec![Scope::MemoryRead], false),
        (
            "notes",
            vec![Scope::MemoryRead, Scope::SourceRead, Scope::MemoryPropose],
            false,
        ),
        ("notes", vec![Scope::MemoryRead, Scope::SourceRead], true),
    ] {
        let (e, v, p, _) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let writer = host
            .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let token = sha256(&OsIdSource.random_16());
        let mut pipe = Pipe::listen(&token).unwrap();
        let mut agent = Agent::spawn();
        let other = foreign.then(Agent::spawn);
        let enrollment = Enrollment::enroll(
            other.as_ref().map_or(agent.child.id(), |a| a.child.id()),
            principal,
            scopes,
        )
        .unwrap();
        let before = v.snapshot().unwrap();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                notes::native::serve_connection(
                    &reader, &writer, &profile, &mut pipe, enrollment, TIMEOUT,
                )
            });
            agent.connect(&token, &e.descriptor.vault_id, mode);
            assert_eq!(agent.event()["refused"], true);
            agent.finish();
            assert!(server.join().unwrap().is_err());
        });
        assert_eq!(v.snapshot().unwrap(), before);
    }
}

#[test]
fn real_note_pipe_unknown_retains_full_call_blocks_resend_and_recovers_one_receipt() {
    for (point, persisted) in [
        (FaultPoint::StagingWritten, false),
        (FaultPoint::AfterCurrent, true),
    ] {
        let faults = Faults::armed();
        let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, faults.clone());
        let principal = actor(ActorType::Agent, 88);
        {
            let host = QualifiedAgentHost::from_authenticated_initialized_owner(
                v.clone(),
                e.descriptor.created_by.clone(),
            )
            .unwrap();
            let reader = host
                .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
                .unwrap();
            let writer = host
                .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
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
            std::thread::scope(|scope| {
                let server = scope.spawn(|| {
                    pipe.accept(TIMEOUT).unwrap();
                    notes::native::serve_connection(
                        &reader, &writer, &profile, &mut pipe, enrollment, TIMEOUT,
                    )
                });
                agent.connect(&token, &e.descriptor.vault_id, "notes");
                assert_eq!(agent.event()["connected"], true);
                agent.initialize();
                faults.arm(point, 1, FaultAction::Fail(119));
                let original = note_call(&request);
                let failure = agent.rpc(original.clone());
                assert_eq!(failure["fault"], "OutcomeUnknown");
                assert_eq!(failure["original"], original);
                assert_eq!(failure["retained"], original);
                let mut changed = original.clone();
                changed["id"] = json!("replacement");
                let blocked = agent.rpc(changed);
                assert_eq!(blocked["fault"], "Closed");
                assert_eq!(blocked["retained"], original);
                agent.finish();
                assert!(server.join().unwrap().is_err());
            });
            assert_eq!(
                digest(writer.original_unknown().unwrap().as_ref().unwrap()),
                digest(&request)
            );
        }
        drop(v);
        let v = Arc::new(
            SegmentedAutomationVault::recover(
                &e.root,
                e.descriptor.host_id.clone(),
                &e.descriptor.system_actor,
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none(),
            )
            .unwrap(),
        );
        let recovered = v.snapshot().unwrap();
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let writer = host
            .note_writer(&e.descriptor.created_by, principal, scope(&p))
            .unwrap();
        let receipt = writer.submit(&request).unwrap();
        let after = v.snapshot().unwrap();
        assert_eq!(after == recovered, persisted);
        assert_eq!(writer.submit(&request).unwrap(), receipt);
        assert_eq!(v.snapshot().unwrap(), after);
    }
}

#[test]
fn note_stdio_exposes_the_full_unknown_packet_and_retains_it_after_stdout_failure() {
    for storage_unknown in [false, true] {
        let faults = Faults::armed();
        let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, faults.clone());
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let writer = host
            .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
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
        let original = note_call(&request);
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                notes::native::serve_connection(
                    &reader, &writer, &profile, &mut pipe, enrollment, TIMEOUT,
                )
            });
            agent.connect(&token, &e.descriptor.vault_id, "notes");
            assert_eq!(agent.event()["connected"], true);
            if storage_unknown {
                faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(119));
            }
            let mut messages = initialization().to_vec();
            messages.push(original.clone());
            agent.send(json!({"stdio":messages,"fail_output":!storage_unknown}));
            let result = agent.event();
            assert_eq!(result["retained"], original);
            assert_eq!(result["stdio_ok"], storage_unknown);
            if storage_unknown {
                let error = &result["output"][1]["error"];
                assert_eq!(error["message"], "outcome_unknown");
                assert_eq!(error["data"]["originalRequest"], original);
                assert!(result["output"][1].get("result").is_none());
            } else {
                assert_eq!(result["output"].as_array().unwrap().len(), 1);
            }
            let blocked = agent.rpc(json!({"jsonrpc":"2.0","id":"blocked","method":"ping"}));
            assert_eq!(blocked["fault"], "Closed");
            assert_eq!(blocked["retained"], original);
            agent.finish();
            let _ = server.join().unwrap();
        });
        if !storage_unknown {
            let after = v.snapshot().unwrap();
            let fresh = host
                .note_writer(&e.descriptor.created_by, principal, scope(&p))
                .unwrap();
            assert_eq!(
                fresh.submit(&request).unwrap().state,
                AgentNoteState::HintSaved
            );
            assert_eq!(v.snapshot().unwrap(), after);
        }
    }
}

#[test]
fn note_launcher_attaches_explicitly_and_returns_a_real_saved_receipt_over_stdio() {
    let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let writer = host
        .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let token = sha256(&OsIdSource.random_16());
    let mut pipe = Pipe::listen(&token).unwrap();
    let mut child = Command::new(env!("CARGO_BIN_EXE_enouia-memory-mcp"))
        .args([
            "--protocol",
            "automation-notes-v1",
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
        .stderr(Stdio::null())
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
            notes::native::serve_connection(
                &reader, &writer, &profile, &mut pipe, enrollment, TIMEOUT,
            )
        });
        let mut input = child.stdin.take().unwrap();
        for message in initialization().into_iter().chain([note_call(&request)]) {
            writeln!(input, "{message}").unwrap();
        }
        drop(input);
        let output = child.wait_with_output().unwrap();
        assert!(output.status.success());
        let messages: Vec<Value> = String::from_utf8(output.stdout)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(messages.len(), 2);
        assert_eq!(
            messages[1]["result"]["structuredContent"]["receipt"]["state"],
            "hint_saved"
        );
        let _ = server.join().unwrap();
    });
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn live_note_pipe_revoke_lock_expiry_and_policy_withdrawal_close_before_submission() {
    for invalidation in 0..5 {
        let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let writer = host
            .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
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
        let revoke = enrollment.revocation();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                notes::native::serve_connection(
                    &reader, &writer, &profile, &mut pipe, enrollment, TIMEOUT,
                )
            });
            agent.connect(&token, &e.descriptor.vault_id, "notes");
            assert_eq!(agent.event()["connected"], true);
            agent.initialize();
            match invalidation {
                0 => writer.revoke(),
                1 => host.lock().unwrap(),
                2 => e.clock.set(NOW + 120_001),
                3 => revoke.revoke(),
                _ => {
                    let mut withdrawn = p.clone();
                    withdrawn.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
                    withdrawn.active = false;
                    v.set_policy(&e.descriptor.created_by, withdrawn).unwrap();
                }
            }
            let before = v.snapshot().unwrap();
            let original = note_call(&request);
            let failure = agent.rpc(original.clone());
            assert_eq!(failure["fault"], "OutcomeUnknown");
            assert_eq!(failure["retained"], original);
            assert_eq!(v.snapshot().unwrap(), before);
            agent.finish();
            assert!(server.join().unwrap().is_err());
        });
    }
}

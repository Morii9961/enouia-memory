#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*,
    hash::{Sha256Hex, sha256},
    ids::*,
    next::*,
    time::Timestamp,
    workspace::HostSurface,
};
use enouia_memory_host::{
    automation::{
        owner_runtime_capture_transport::{self as wire, Call, Mode},
        runtime_capture::RuntimeCaptureHost,
    },
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_vault::{automation::segmented::*, fault::Faults};
use fixture::*;
use serde_json::{Value, json};
use std::{
    io::{BufRead, BufReader, Write},
    os::windows::process::CommandExt,
    process::{Child, ChildStdin, Command, Stdio},
    sync::{Arc, mpsc},
    time::Duration,
};
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod schema_support;
const TIMEOUT: Duration = Duration::from_secs(15);
struct Peer {
    child: Child,
    input: Option<ChildStdin>,
    events: mpsc::Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Peer {
    fn spawn() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "runtime_wire_process_child", "--nocapture"])
            .env("ENOUIA_RUNTIME_WIRE_CHILD", "1")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take();
        let output = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(output).lines() {
                let Ok(line) = line else {
                    break;
                };
                if let Some((_, text)) = line.split_once("runtime-wire-event ")
                    && tx.send(serde_json::from_str(text).unwrap()).is_err()
                {
                    break;
                }
            }
        });
        Self {
            child,
            input,
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
            .expect("owned Runtime child response")
    }
    fn stop(&mut self) {
        self.input.take();
        assert!(self.child.wait().unwrap().success());
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.input.take();
        if self.child.try_wait().ok().flatten().is_none() {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn event(v: Value) {
    println!("runtime-wire-event {v}");
    std::io::stdout().flush().unwrap();
}
#[test]
fn runtime_wire_process_child() {
    if std::env::var_os("ENOUIA_RUNTIME_WIRE_CHILD").is_none() {
        return;
    }
    let stdin = std::io::stdin();
    let mut lines = stdin.lock().lines();
    let Some(first) = lines.next() else {
        return;
    };
    let first: Value = serde_json::from_str(&first.unwrap()).unwrap();
    let id: Sha256Hex = serde_json::from_value(first["pipe"].clone()).unwrap();
    let vault: VaultId = serde_json::from_value(first["vault"].clone()).unwrap();
    let pipe = Pipe::connect(&id).unwrap();
    let host = ProcessPeer::enroll(
        first["host_pid"].as_u64().unwrap() as u32,
        HostSurface::QuickSearch,
    )
    .unwrap();
    if first["legacy"] == true {
        event(
            json!({"legacy_refused":enouia_memory_host::mcp::native::Client::connect(pipe,host,&vault,TIMEOUT).is_err()}),
        );
        return;
    }
    let mut client = wire::Client::connect(pipe, host, &vault, TIMEOUT).unwrap();
    event(json!({"connected":true}));
    for line in lines {
        let call: Call = serde_json::from_str(&line.unwrap()).unwrap();
        match client.exchange(&call) {
            Ok(value) => event(json!({"reply":value,"retained":client.original_unknown()})),
            Err(failure) => event(
                json!({"fault":format!("{:?}",failure.fault),"original":failure.original,"retained":client.original_unknown()}),
            ),
        }
    }
}
fn request() -> RuntimeCaptureRequest {
    let v: Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/runtime-owner-capture/cases.json"),
        )
        .unwrap(),
    )
    .unwrap();
    serde_json::from_value(v["base"].clone()).unwrap()
}
fn enroll(
    e: &Env,
    vault: Arc<SegmentedAutomationVault>,
    peer: &Peer,
    p: &AutomationPolicy,
) -> RuntimeCaptureHost {
    RuntimeCaptureHost::enroll_owner(
        vault,
        &e.descriptor.created_by,
        peer.child.id(),
        ActorRef {
            actor_type: ActorType::Client,
            actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
        },
        RuntimeCaptureScope {
            policy_id: p.policy_id.clone(),
            source_ids: p.source_ids.clone(),
            max_sensitivity: Sensitivity::Private,
            platform: "enouia-runtime".into(),
            account_scope: "account-synthetic".into(),
            default_timezone: "Asia/Shanghai".into(),
            valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
        },
    )
    .unwrap()
}
fn pipe_id() -> Sha256Hex {
    use enouia_memory_contract::ports::IdSource;
    sha256(&enouia_memory_vault::OsIdSource.random_16())
}
#[test]
fn real_runtime_process_captures_preflights_and_conflicts_without_an_agent_writer() {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut peer = Peer::spawn();
    let host = enroll(&e, v.clone(), &peer, &p);
    std::thread::scope(|s| {
        let server = s.spawn(move || {
            pipe.accept(TIMEOUT).unwrap();
            wire::serve_connection(&host, &mut pipe, TIMEOUT)
        });
        peer.send(json!({"pipe":id,"vault":e.descriptor.vault_id,"host_pid":std::process::id()}));
        assert_eq!(peer.event()["connected"], true);
        let before = v.snapshot().unwrap();
        let call = Call {
            mode: Mode::Preflight,
            request: request(),
        };
        peer.send(json!(call));
        let preview = peer.event();
        assert_eq!(preview["reply"]["capture"]["captured"], false);
        assert_eq!(before, v.snapshot().unwrap());
        let permit = v
            .authorize_owner_runtime_capture(
                &e.descriptor.created_by,
                ActorRef {
                    actor_type: ActorType::Client,
                    actor_id: PrincipalId::parse(&fixture::id("prn", 77)).unwrap(),
                },
                RuntimeCaptureScope {
                    policy_id: p.policy_id.clone(),
                    source_ids: p.source_ids.clone(),
                    max_sensitivity: Sensitivity::Private,
                    platform: "enouia-runtime".into(),
                    account_scope: "account-synthetic".into(),
                    default_timezone: "Asia/Shanghai".into(),
                    valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
                },
            )
            .unwrap();
        let original = v.capture_runtime(&permit, &request(), false).unwrap();
        let mut call = Call {
            mode: Mode::Write,
            request: request(),
        };
        peer.send(json!(call));
        let result = peer.event();
        assert_eq!(result["reply"]["capture"]["captured"], true);
        assert_eq!(
            result["reply"]["capture"]["sources"],
            json!(original.sources)
        );
        assert_eq!(result["reply"]["capture"]["replayed"], true);
        assert_eq!(result["reply"]["capture"]["formation_scheduled"], false);
        if let Some(path) = std::env::var_os("ENOUIA_OWNER_RUNTIME_CAPTURE_TRANSCRIPT") {
            let write_call = Call {
                mode: Mode::Write,
                request: request(),
            };
            let preflight_call = Call {
                mode: Mode::Preflight,
                request: request(),
            };
            let mut bad = result["reply"].clone();
            bad["capture"]["formation_scheduled"] = json!(true);
            std::fs::write(path,serde_json::to_vec_pretty(&json!({"exchanges":[
                {"call":preflight_call,"reply":preview["reply"]},{"call":write_call,"reply":result["reply"]}
            ],"negative_replies":[bad]})).unwrap()).unwrap();
        }
        assert_eq!(result["reply"]["queue"]["state"], "queued");
        let status_call = Call {
            mode: Mode::Status,
            request: request(),
        };
        peer.send(json!(status_call));
        let status = peer.event();
        assert_eq!(status["reply"]["jobs"][0]["state"], "queued");
        assert_eq!(status["reply"]["jobs"][0]["receipts"], 0);
        let schemas = schema_support::schema::SchemaStore::load();
        for packet in [
            json!(status_call),
            status["reply"].clone(),
            json!(call),
            result["reply"].clone(),
            preview["reply"].clone(),
        ] {
            assert!(
                schemas
                    .validate("next/runtime-owner-capture-pipe-v2.schema.json", &packet)
                    .is_empty(),
                "{packet}"
            );
        }
        let encoded = status.to_string();
        assert!(!encoded.contains(&request().turns[0].text));
        let after = v.snapshot().unwrap();
        assert_ne!(before, after);
        peer.send(json!(call));
        assert_eq!(peer.event()["reply"]["capture"]["replayed"], true);
        assert_eq!(v.snapshot().unwrap(), after);
        call.request.turns[0].text.push('!');
        peer.send(json!(call));
        assert_eq!(peer.event()["reply"]["error"], "invalid_request");
        assert_eq!(v.snapshot().unwrap(), after);
        peer.stop();
        assert!(server.join().unwrap().is_err());
    });
}
#[test]
fn native_revoke_and_expiry_close_active_pipes_without_publishing_or_reconnecting() {
    for expire in [false, true] {
        let mut e = Env::new();
        e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
        e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
        let v = Arc::new(e.create(Faults::none()));
        let p = e.policy(1, 1, 8);
        v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let id = pipe_id();
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut peer = Peer::spawn();
        let host = Arc::new(enroll(&e, v.clone(), &peer, &p));
        std::thread::scope(|s| {
            let serving = host.clone();
            let server = s.spawn(move || {
                pipe.accept(TIMEOUT).unwrap();
                wire::serve_connection(&serving, &mut pipe, TIMEOUT)
            });
            peer.send(
                json!({"pipe":id,"vault":e.descriptor.vault_id,"host_pid":std::process::id()}),
            );
            assert_eq!(peer.event()["connected"], true);
            let before = v.snapshot().unwrap();
            let mut call = Call {
                mode: Mode::Preflight,
                request: request(),
            };
            peer.send(json!(call));
            assert!(peer.event()["reply"]["error"].is_null());
            if expire {
                e.clock.set(NOW + 120_000);
            } else {
                host.revoke();
            }
            call.mode = Mode::Write;
            peer.send(json!(call));
            let failed = peer.event();
            assert_eq!(failed["fault"], "OutcomeUnknown");
            assert_eq!(failed["retained"], json!(call));
            peer.send(json!(call));
            let closed = peer.event();
            assert_eq!(closed["fault"], "Closed");
            assert_eq!(closed["retained"], json!(call));
            peer.stop();
            assert!(server.join().unwrap().is_err());
            assert_eq!(v.snapshot().unwrap(), before);
        });
        let mut unused = Pipe::listen(&pipe_id()).unwrap();
        assert!(wire::serve_connection(&host, &mut unused, TIMEOUT).is_err());
    }
}
#[test]
fn legacy_runtime_capture_hello_is_refused_before_any_source_transaction() {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let before = v.snapshot().unwrap();
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut peer = Peer::spawn();
    let host = enroll(&e, v.clone(), &peer, &p);
    std::thread::scope(|s| {
        let server = s.spawn(move || {
            pipe.accept(TIMEOUT).unwrap();
            wire::serve_connection(&host, &mut pipe, TIMEOUT)
        });
        peer.send(json!({"pipe":id,"vault":e.descriptor.vault_id,"host_pid":std::process::id(),"legacy":true}));
        assert_eq!(peer.event()["legacy_refused"], true);
        peer.stop();
        assert!(server.join().unwrap().is_err());
    });
    assert_eq!(v.snapshot().unwrap(), before);
}
#[test]
fn a_different_kernel_peer_cannot_use_the_enrolled_runtime_process() {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let before = v.snapshot().unwrap();
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut peer = Peer::spawn();
    let host = enroll(&e, v.clone(), &peer, &p);
    std::thread::scope(|s| {
        let server = s.spawn(move || {
            pipe.accept(TIMEOUT).unwrap();
            wire::serve_connection(&host, &mut pipe, TIMEOUT)
        });
        let client_pipe = Pipe::connect(&id).unwrap();
        let server_peer =
            ProcessPeer::enroll(std::process::id(), HostSurface::QuickSearch).unwrap();
        assert!(
            wire::Client::connect(client_pipe, server_peer, &e.descriptor.vault_id, TIMEOUT)
                .is_err()
        );
        assert!(server.join().unwrap().is_err());
    });
    // This child never received the native setup packet and owns no connection.
    peer.stop();
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn owner_wire_background_forms_and_status_reports_only_genuine_commits() {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut peer = Peer::spawn();
    let host = Arc::new(enroll(&e, v.clone(), &peer, &p));
    struct Fake;
    impl CaptureAnalyzer for Fake {
        fn start(
            &mut self,
            _: &QueuedCapture,
            source: &CapturedSource,
            _: &CaptureStopToken,
        ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
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
                title: source.text.clone(),
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
                note: None,
                prompt_version: "wire-fake-2".into(),
                model_version: None,
            }]))
        }
    }
    let worker = host.start_worker(Fake, Duration::from_secs(2)).unwrap();
    std::thread::scope(|scope| {
        let serving = host.clone();
        let server = scope.spawn(move || {
            pipe.accept(TIMEOUT).unwrap();
            wire::serve_connection(&serving, &mut pipe, TIMEOUT)
        });
        peer.send(json!({"pipe":id,"vault":e.descriptor.vault_id,"host_pid":std::process::id()}));
        assert_eq!(peer.event()["connected"], true);
        let mut r = request();
        r.turns[0].text = "I prefer quiet rooms.".into();
        peer.send(json!(Call {
            mode: Mode::Write,
            request: r.clone()
        }));
        let written = peer.event();
        assert_eq!(written["reply"]["queue"]["state"], "queued");
        assert_eq!(written["reply"]["capture"]["formation_scheduled"], false);
        let deadline = std::time::Instant::now() + TIMEOUT;
        loop {
            peer.send(json!(Call {
                mode: Mode::Status,
                request: r.clone()
            }));
            let state = peer.event();
            assert!(state["reply"]["error"].is_null(), "{state}");
            assert!(!state.to_string().contains(&r.turns[0].text));
            if state["reply"]["jobs"][0]["state"] == "formed" {
                assert_eq!(state["reply"]["jobs"][0]["receipts"], 1);
                let schemas = schema_support::schema::SchemaStore::load();
                assert!(
                    schemas
                        .validate(
                            "next/runtime-owner-capture-pipe-v2.schema.json",
                            &state["reply"]
                        )
                        .is_empty()
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "background formation deadline"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            worker.shutdown(),
            enouia_memory_host::automation::captured_worker::WorkerExit::Stopped
        );
        peer.stop();
        assert!(server.join().unwrap().is_err());
    });
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.memories.len(), 1)
    })
    .unwrap();
}

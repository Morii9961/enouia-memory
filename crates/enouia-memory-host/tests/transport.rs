//! Real prototype executable and separate native client over synthetic roots.
#![cfg(windows)]
use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
use enouia_memory_contract::foundation::Clock;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::host::session::ClientHello;
use enouia_memory_contract::ids::{DeviceId, PrincipalId};
use enouia_memory_contract::json::SchemaVersion;
use enouia_memory_contract::ports::IdSource;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_contract::workspace::{HostSurface, is_write, validate_response};
use enouia_memory_host::broker::{Broker, Fault as BrokerFault, State as BrokerState};
use enouia_memory_host::compatibility_hash;
use enouia_memory_host::transport::Client;
use enouia_memory_host::windows::{Pipe, ProcessPeer};
use enouia_memory_vault::host_transaction::HostCoordinator;
use enouia_memory_vault::service::new_genesis;
use enouia_memory_vault::{
    OsIdSource, RootPolicy, SystemClock, Vault, VaultOptions, verify_data_root,
};
use serde_json::{Value, json};
use std::io::{BufRead, BufReader};
use std::os::windows::process::CommandExt;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
    mpsc,
};
use std::time::{Duration, Instant};

static N: AtomicU64 = AtomicU64::new(0);
const TIMEOUT: Duration = Duration::from_secs(5);
struct Fixture {
    root: PathBuf,
    vault: Vault,
    identity: DeviceId,
}
impl Fixture {
    fn new() -> Self {
        let base = std::env::temp_dir().join("enouia-memory-host-transport-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let root = base.join(format!(
            "native-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&root).unwrap();
        let verified = verify_data_root(&root, &RootPolicy::default()).unwrap();
        let owner = ActorRef {
            actor_type: ActorType::Owner,
            actor_id: PrincipalId::from_random(OsIdSource.random_16()),
        };
        let now = Timestamp::from_unix_ms(SystemClock.now_unix_ms()).unwrap();
        let genesis =
            new_genesis(&OsIdSource, owner, TrustedSurface::TrustedWindowsApp, &now).unwrap();
        let vault = Vault::create(
            &verified,
            genesis,
            Arc::new(SystemClock),
            Arc::new(OsIdSource),
            VaultOptions::default(),
        )
        .unwrap();
        enouia_memory_vault::platform::restrict_to_owner(verified.path()).unwrap();
        let identity = DeviceId::from_random(OsIdSource.random_16());
        drop(HostCoordinator::bootstrap(&vault, &verified, identity.clone()).unwrap());
        Self {
            root,
            vault,
            identity,
        }
    }
    fn hello(&self) -> ClientHello {
        ClientHello {
            schema_version: SchemaVersion,
            protocol_version: 1,
            workspace_version: 1,
            vault_id: self.vault.vault_id().clone(),
            compatibility_hash: compatibility_hash(),
        }
    }
    fn current(&self) -> Vec<u8> {
        std::fs::read(self.root.join("vault/CURRENT")).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn pipe_id() -> Sha256Hex {
    sha256(&OsIdSource.random_16())
}
fn until() -> Instant {
    Instant::now() + TIMEOUT
}
fn packet(command: &str, arguments: Value, key: &str) -> Value {
    json!({"schemaVersion":1,"requestId":"req_00000001-0000-4000-8000-000000000001",
        "command":command,"idempotencyKey":is_write(command).then_some(key),"arguments":arguments})
}
fn ok(client: &mut Client, command: &str, arguments: Value, key: &str) -> Value {
    let response = client.call(&packet(command, arguments, key)).unwrap();
    validate_response(command, &response).unwrap();
    assert!(response["error"].is_null(), "{response}");
    response["result"].clone()
}
struct Server(Child);
impl Server {
    fn spawn(f: &Fixture, id: &Sha256Hex, client_pid: u32, scope: HostSurface) -> Self {
        Self::configured(f, id, client_pid, scope, true)
    }
    fn configured(
        f: &Fixture,
        id: &Sha256Hex,
        client_pid: u32,
        scope: HostSurface,
        once: bool,
    ) -> Self {
        let mut command = Command::new(env!("CARGO_BIN_EXE_enouia-memory-host"));
        command
            .arg("--vault")
            .arg(&f.root)
            .arg("--host-id")
            .arg(f.identity.as_str())
            .arg("--pipe")
            .arg(id.as_str())
            .arg("--client-pid")
            .arg(client_pid.to_string())
            .arg("--surface")
            .arg(if scope == HostSurface::Workspace {
                "workspace"
            } else {
                "quick_search"
            })
            .creation_flags(0x08000000)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        if once {
            command.arg("--once");
        }
        Self(command.spawn().unwrap())
    }
    fn ready(&mut self) {
        let stdout = self.0.stdout.take().unwrap();
        let (tx, rx) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            let mut line = String::new();
            BufReader::new(stdout).read_line(&mut line).unwrap();
            tx.send(line).unwrap();
        });
        let line = rx
            .recv_timeout(TIMEOUT)
            .expect("prototype did not become ready");
        reader.join().unwrap();
        let ready: Value = serde_json::from_str(&line).unwrap();
        assert_eq!(ready, json!({"ready":true,"pid":self.0.id()}));
    }
    fn finish(&mut self, success: bool) {
        let deadline = until();
        loop {
            if let Some(status) = self.0.try_wait().unwrap() {
                assert_eq!(status.success(), success);
                return;
            }
            assert!(Instant::now() < deadline, "synthetic Host did not exit");
            std::thread::sleep(Duration::from_millis(5));
        }
    }
    fn peer(&self, scope: HostSurface) -> ProcessPeer {
        ProcessPeer::enroll(self.0.id(), scope).unwrap()
    }
}
impl Drop for Server {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
}
fn send(pipe: &mut Pipe, tag: u8, bytes: &[u8]) {
    let mut header = [tag, 0, 0, 0, 0];
    header[1..].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
    pipe.write_all(&header, until()).unwrap();
    pipe.write_all(bytes, until()).unwrap();
}
fn receive(pipe: &mut Pipe) -> (u8, Value) {
    let mut header = [0; 5];
    pipe.read_exact(&mut header, until()).unwrap();
    let len = u32::from_le_bytes(header[1..].try_into().unwrap()) as usize;
    assert!(len > 0 && len <= 1024 * 1024);
    let mut body = vec![0; len];
    pipe.read_exact(&mut body, until()).unwrap();
    (header[0], serde_json::from_slice(&body).unwrap())
}

#[test]
fn separate_host_process_runs_pending_review_lock_unlock_and_exact_replay() {
    let f = Fixture::new();
    let id = pipe_id();
    let mut server = Server::spawn(&f, &id, std::process::id(), HostSurface::Workspace);
    server.ready();
    let mut client = Client::connect(
        Pipe::connect(&id).unwrap(),
        server.peer(HostSurface::Workspace),
        f.hello(),
        TIMEOUT,
    )
    .unwrap();
    let request = packet(
        "remember",
        json!({"text":"合成：原生进程使用受控存储。","claimKey":"native.storage"}),
        "native-process-remember-key",
    );
    let original = client.call(&request).unwrap();
    assert!(original["error"].is_null());
    assert_eq!(original["result"]["state"], "pending");
    client.lock().unwrap();
    assert!(client.call(&request).is_err());
    let mut competing = Server::spawn(&f, &pipe_id(), std::process::id(), HostSurface::Workspace);
    competing.finish(false);
    client.unlock().unwrap();
    let before = f.current();
    let replay = client.call(&request).unwrap();
    assert_eq!(
        replay["result"]["candidateId"],
        original["result"]["candidateId"]
    );
    assert_eq!(f.current(), before);
    let proposed = &replay["result"];
    let plan = ok(
        &mut client,
        "review_plan",
        json!({"decisions":[{
            "candidateId":proposed["candidateId"],"revision":proposed["revision"],"action":"accept","editedContent":null,"mergeTarget":null
        }]}),
        "native-process-plan-key",
    );
    ok(
        &mut client,
        "review_confirm",
        json!({"planId":plan["planId"],"diffHash":plan["diffHash"]}),
        "native-process-confirm-key",
    );
    let memories = ok(
        &mut client,
        "memory_list",
        json!({"includeInactive":false,"cursor":null,"limit":20}),
        "unused-read-key",
    );
    assert_eq!(memories["items"].as_array().unwrap().len(), 1);
    drop(client);
    server.finish(true);
    let root = verify_data_root(&f.root, &RootPolicy::default()).unwrap();
    assert!(HostCoordinator::open(&f.vault, &root, f.identity.clone()).is_ok());
}

#[test]
fn oversized_truncated_or_claimed_identity_hello_never_reaches_core() {
    for scenario in ["oversized", "truncated", "principal"] {
        let f = Fixture::new();
        let before = f.current();
        let id = pipe_id();
        let mut server = Server::spawn(&f, &id, std::process::id(), HostSurface::Workspace);
        server.ready();
        let mut pipe = Pipe::connect(&id).unwrap();
        server
            .peer(HostSurface::Workspace)
            .verify_server(&pipe)
            .unwrap();
        match scenario {
            "oversized" => pipe.write_all(&[1, 255, 255, 255, 255], until()).unwrap(),
            "truncated" => {
                pipe.write_all(&[1, 100, 0, 0, 0, b'{'], until()).unwrap();
            }
            _ => {
                let mut hello = serde_json::to_value(f.hello()).unwrap();
                hello["principal_id"] = json!("synthetic forged owner");
                send(&mut pipe, 1, &serde_json::to_vec(&hello).unwrap());
            }
        }
        drop(pipe);
        server.finish(true);
        assert_eq!(f.current(), before);
    }
}

#[test]
fn native_control_requires_current_compatibility_even_before_a_session() {
    let f = Fixture::new();
    let before = f.current();
    let id = pipe_id();
    let mut server = Server::spawn(&f, &id, std::process::id(), HostSurface::Workspace);
    server.ready();
    let mut pipe = Pipe::connect(&id).unwrap();
    let mut hello = f.hello();
    hello.compatibility_hash = sha256(b"old native control revision");
    send(&mut pipe, 4, &serde_json::to_vec(&hello).unwrap());
    drop(pipe);
    server.finish(true);
    assert_eq!(f.current(), before);
}

#[test]
fn raw_overlay_peer_cannot_bypass_scope_or_choose_owner_native_controls() {
    for tag in [2u8, 4] {
        let f = Fixture::new();
        let before = f.current();
        let id = pipe_id();
        let mut server = Server::spawn(&f, &id, std::process::id(), HostSurface::QuickSearch);
        server.ready();
        let mut pipe = Pipe::connect(&id).unwrap();
        send(&mut pipe, 1, &serde_json::to_vec(&f.hello()).unwrap());
        let (kind, welcome) = receive(&mut pipe);
        assert_eq!(kind, 129);
        assert_eq!(welcome["surface"], "quick_search");
        let bytes = if tag == 2 {
            serde_json::to_vec(&packet(
                "remember",
                json!({"text":"合成：overlay 不能写入。","claimKey":"denied"}),
                "native-overlay-denied-key",
            ))
            .unwrap()
        } else {
            serde_json::to_vec(&f.hello()).unwrap()
        };
        send(&mut pipe, tag, &bytes);
        drop(pipe);
        server.finish(true);
        assert_eq!(f.current(), before);
    }
}

#[test]
fn a_different_live_owner_process_is_rejected_before_handshake_or_core() {
    let f = Fixture::new();
    let before = f.current();
    // Only this test-created process is enrolled, not our connecting process.
    let mut enrolled = Server(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "idle_enrolled_process_child", "--nocapture"])
            .env("ENOUIA_HOST_IDLE_PEER_CHILD", "1")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let id = pipe_id();
    let mut server = Server::spawn(&f, &id, enrolled.0.id(), HostSurface::Workspace);
    server.ready();
    let result = Client::connect(
        Pipe::connect(&id).unwrap(),
        server.peer(HostSurface::Workspace),
        f.hello(),
        TIMEOUT,
    );
    assert!(result.is_err());
    server.finish(true);
    assert_eq!(f.current(), before);
    drop(enrolled.0.stdin.take());
    enrolled.finish(true);
}

#[test]
fn idle_enrolled_process_child() {
    if std::env::var_os("ENOUIA_HOST_IDLE_PEER_CHILD").is_none() {
        return;
    }
    use std::io::Read;
    let mut byte = [0];
    assert_eq!(std::io::stdin().read(&mut byte).unwrap(), 0);
}

#[test]
fn bad_reply_poisoning_prevents_native_client_reuse_or_implicit_retry() {
    let f = Fixture::new();
    let id = pipe_id();
    let mut server = Pipe::listen(&id).unwrap();
    let mut welcome = serde_json::to_value(f.hello()).unwrap();
    welcome["instance_id"] = json!(sha256(b"synthetic invalid-reply host"));
    welcome["primary"] = json!({"host_id":f.identity,"primary_epoch":1});
    welcome["surface"] = json!("workspace");
    welcome["expires_at"] =
        json!(Timestamp::from_unix_ms(SystemClock.now_unix_ms() + 300_000).unwrap());
    std::thread::scope(|scope| {
        let host = scope.spawn(move || {
            server.accept(TIMEOUT).unwrap();
            assert_eq!(receive(&mut server).0, 1);
            send(&mut server, 129, &serde_json::to_vec(&welcome).unwrap());
            assert_eq!(receive(&mut server).0, 2);
            send(&mut server, 130, b"{}");
            let mut byte = [0];
            assert_eq!(
                server
                    .read_exact(&mut byte, Instant::now() + Duration::from_millis(25))
                    .unwrap_err()
                    .kind(),
                std::io::ErrorKind::TimedOut
            );
        });
        let mut client = Client::connect(
            Pipe::connect(&id).unwrap(),
            ProcessPeer::enroll(std::process::id(), HostSurface::Workspace).unwrap(),
            f.hello(),
            TIMEOUT,
        )
        .unwrap();
        let request = packet("workspace_status", json!({}), "unused-read-key");
        assert!(client.call(&request).is_err());
        assert!(client.call(&request).is_err());
        assert!(client.handshake().is_err());
        host.join().unwrap();
    });
}

#[test]
fn locked_host_accepts_fresh_native_control_connection_without_releasing_lease() {
    let f = Fixture::new();
    let id = pipe_id();
    let mut broker = Server(
        Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "reconnect_native_broker_child", "--nocapture"])
            .env("ENOUIA_HOST_RECONNECT_PIPE", id.as_str())
            .env("ENOUIA_HOST_RECONNECT_VAULT", f.vault.vault_id().as_str())
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap(),
    );
    let mut server = Server::configured(&f, &id, broker.0.id(), HostSurface::Workspace, false);
    server.ready();
    use std::io::Write;
    writeln!(broker.0.stdin.as_mut().unwrap(), "{}", server.0.id()).unwrap();
    broker.finish(true);
    server.finish(true);
    let root = verify_data_root(&f.root, &RootPolicy::default()).unwrap();
    assert!(HostCoordinator::open(&f.vault, &root, f.identity.clone()).is_ok());
}

#[test]
fn reconnect_native_broker_child() {
    let Some(id) = std::env::var_os("ENOUIA_HOST_RECONNECT_PIPE") else {
        return;
    };
    let id = Sha256Hex::parse(&id.to_string_lossy()).unwrap();
    let mut pid = String::new();
    std::io::stdin().read_line(&mut pid).unwrap();
    let pid: u32 = pid.trim().parse().unwrap();
    let vault = enouia_memory_contract::ids::VaultId::parse(
        &std::env::var("ENOUIA_HOST_RECONNECT_VAULT").unwrap(),
    )
    .unwrap();
    let hello = ClientHello {
        schema_version: SchemaVersion,
        protocol_version: 1,
        workspace_version: 1,
        vault_id: vault,
        compatibility_hash: compatibility_hash(),
    };
    let mut broker = Broker::new(HostSurface::Workspace);
    broker
        .open(
            Pipe::connect(&id).unwrap(),
            ProcessPeer::enroll(pid, HostSurface::Workspace).unwrap(),
            hello.clone(),
            TIMEOUT,
        )
        .unwrap();
    assert_eq!(broker.state(), BrokerState::AwaitingHandshake);
    let before_handshake = broker.ticket();
    broker.handshake(&before_handshake).unwrap();
    let request = packet(
        "remember",
        json!({"text":"合成：真正断连后保留请求。","claimKey":"native.reconnect"}),
        "native-physical-reconnect-key",
    );
    assert!(matches!(
        broker.call(&before_handshake, HostSurface::Workspace, &request),
        Err(BrokerFault::Stale)
    ));
    let queued = broker.ticket();
    assert!(matches!(
        broker.call(&queued, HostSurface::QuickSearch, &request),
        Err(BrokerFault::ScopeDenied)
    ));
    assert!(matches!(
        broker.lock(&queued, HostSurface::QuickSearch),
        Err(BrokerFault::ScopeDenied)
    ));
    assert!(matches!(
        broker.unlock(&queued, HostSurface::QuickSearch),
        Err(BrokerFault::ScopeDenied)
    ));
    let original = broker
        .call(&queued, HostSurface::Workspace, &request)
        .unwrap();
    assert!(original["error"].is_null());
    let page_lock = packet("vault_lock", json!({}), "native-broker-page-lock-key");
    let locked = broker
        .call(&queued, HostSurface::Workspace, &page_lock)
        .unwrap();
    assert!(locked["error"].is_null());
    assert_eq!(broker.state(), BrokerState::Locked);
    assert!(matches!(
        broker.call(&queued, HostSurface::Workspace, &request),
        Err(BrokerFault::Stale)
    ));
    broker.disconnect();
    assert_eq!(broker.state(), BrokerState::Disconnected);
    let deadline = until();
    // Only connection establishment is retried. No application packet/send is repeated.
    let pipe = loop {
        match Pipe::connect(&id) {
            Ok(pipe) => break pipe,
            Err(e) if matches!(e.raw_os_error(), Some(231 | 2)) && Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5))
            }
            Err(e) => panic!("synthetic reconnect failed: {:?}", e.kind()),
        }
    };
    broker
        .open(
            pipe,
            ProcessPeer::enroll(pid, HostSurface::Workspace).unwrap(),
            hello,
            TIMEOUT,
        )
        .unwrap();
    let reconnect_ticket = broker.ticket();
    assert!(matches!(
        broker.call(&reconnect_ticket, HostSurface::Workspace, &request),
        Err(BrokerFault::NotReady)
    ));
    assert!(matches!(
        broker.unlock(&queued, HostSurface::Workspace),
        Err(BrokerFault::Stale)
    ));
    broker
        .unlock(&reconnect_ticket, HostSurface::Workspace)
        .unwrap();
    assert_eq!(broker.state(), BrokerState::Ready);
    assert!(matches!(
        broker.call(&reconnect_ticket, HostSurface::Workspace, &request),
        Err(BrokerFault::Stale)
    ));
    let fresh_ticket = broker.ticket();
    let replay = broker
        .call(&fresh_ticket, HostSurface::Workspace, &request)
        .unwrap();
    assert!(replay["error"].is_null());
    assert_eq!(replay["vaultCommitId"], original["vaultCommitId"]);
    assert_eq!(
        replay["result"]["candidateId"],
        original["result"]["candidateId"]
    );
    broker.lock(&fresh_ticket, HostSurface::Workspace).unwrap();
    let locked_ticket = broker.ticket();
    assert!(matches!(
        broker.call(&locked_ticket, HostSurface::Workspace, &request),
        Err(BrokerFault::NotReady)
    ));
    broker
        .unlock(&locked_ticket, HostSurface::Workspace)
        .unwrap();
    broker.disconnect();
}

#[test]
fn broker_refuses_foreign_tickets_invalid_packets_and_escaped_budget_before_io() {
    let mut broker = Broker::new(HostSurface::Workspace);
    let other = Broker::new(HostSurface::Workspace);
    let request = packet("workspace_status", json!({}), "unused");
    assert!(matches!(
        broker.call(&other.ticket(), HostSurface::Workspace, &request),
        Err(BrokerFault::Stale)
    ));
    assert_eq!(broker.state(), BrokerState::Disconnected);
    let ticket = broker.ticket();
    let mut malformed = request.clone();
    malformed["arguments"]["window"] = json!("main");
    assert!(matches!(
        broker.call(&ticket, HostSurface::Workspace, &malformed),
        Err(BrokerFault::InvalidPacket)
    ));
    let decisions: Vec<_> = (1..=20).map(|n| json!({"candidateId":format!("cand_00000001-0000-4000-8000-{n:012x}"),"revision":1,"action":"edit_accept","editedContent":"\u{1}".repeat(20_000),"mergeTarget":null})).collect();
    let oversized = packet(
        "review_plan",
        json!({"decisions":decisions}),
        "escaped-budget-key",
    );
    enouia_memory_contract::workspace::parse_request(&oversized).unwrap();
    assert!(matches!(
        broker.call(&ticket, HostSurface::Workspace, &oversized),
        Err(BrokerFault::InvalidPacket)
    ));
    assert!(matches!(
        broker.call(&ticket, HostSurface::Workspace, &request),
        Err(BrokerFault::NotReady)
    ));
    broker.disconnect();
    assert!(matches!(broker.handshake(&ticket), Err(BrokerFault::Stale)));
}

#[test]
fn broker_lost_reply_after_real_commit_is_uncertain_and_never_retried() {
    use enouia_memory_host::NativeAdmission;
    use enouia_memory_host::service::HostService;
    use enouia_memory_host::transport::serve_connection;
    let f = Fixture::new();
    let root = verify_data_root(&f.root, &RootPolicy::default()).unwrap();
    let host = HostService::open(&root, f.identity.clone(), Default::default()).unwrap();
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let request = packet(
        "remember",
        json!({"text":"合成：提交成功后的响应丢失。","claimKey":"broker.lost-reply"}),
        "broker-original-lost-reply-key",
    );
    let before = f.current();
    let (done_tx, done_rx) = mpsc::channel();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            let peer = ProcessPeer::enroll(std::process::id(), HostSurface::Workspace).unwrap();
            pipe.accept(TIMEOUT).unwrap();
            peer.verify_client(&pipe).unwrap();
            let (tag, hello) = receive(&mut pipe);
            assert_eq!(tag, 1);
            let hello: ClientHello = serde_json::from_value(hello).unwrap();
            let (welcome, connection) = host
                .connect(
                    NativeAdmission::from_authenticated_owner(
                        &f.vault.descriptor().created_by,
                        HostSurface::Workspace,
                    )
                    .unwrap(),
                    &hello,
                )
                .unwrap();
            send(&mut pipe, 129, &serde_json::to_vec(&welcome).unwrap());
            let (tag, packet) = receive(&mut pipe);
            assert_eq!(tag, 2);
            assert_eq!(packet, request);
            peer.verify_client(&pipe).unwrap();
            let original = host.call(&connection, &packet).unwrap();
            assert!(original["error"].is_null());
            assert_ne!(f.current(), before);
            // Lose exactly this reply after a durable real domain operation.
            pipe.disconnect().unwrap();
            drop(connection);
            assert_eq!(
                pipe.accept(Duration::from_millis(50)).unwrap_err().kind(),
                std::io::ErrorKind::TimedOut
            );
            done_tx.send(original).unwrap();
            pipe.accept(TIMEOUT).unwrap();
            assert!(serve_connection(&host, &mut pipe, &peer, TIMEOUT).is_err());
        });
        let mut broker = Broker::new(HostSurface::Workspace);
        broker
            .open(
                Pipe::connect(&id).unwrap(),
                ProcessPeer::enroll(std::process::id(), HostSurface::Workspace).unwrap(),
                f.hello(),
                TIMEOUT,
            )
            .unwrap();
        broker.handshake(&broker.ticket()).unwrap();
        let queued = broker.ticket();
        assert!(matches!(
            broker.call(&queued, HostSurface::Workspace, &request),
            Err(BrokerFault::UncertainCall(_))
        ));
        assert_eq!(broker.state(), BrokerState::Disconnected);
        assert!(matches!(
            broker.call(&queued, HostSurface::Workspace, &request),
            Err(BrokerFault::Stale)
        ));
        assert!(matches!(
            broker.call(&broker.ticket(), HostSurface::Workspace, &request),
            Err(BrokerFault::NotReady)
        ));
        let original = done_rx.recv_timeout(TIMEOUT).unwrap();
        let committed = f.current();
        // Caller now explicitly selects a new channel and original exact key.
        broker
            .open(
                Pipe::connect(&id).unwrap(),
                ProcessPeer::enroll(std::process::id(), HostSurface::Workspace).unwrap(),
                f.hello(),
                TIMEOUT,
            )
            .unwrap();
        broker.handshake(&broker.ticket()).unwrap();
        let replay = broker
            .call(&broker.ticket(), HostSurface::Workspace, &request)
            .unwrap();
        assert_eq!(
            original["result"]["candidateId"],
            replay["result"]["candidateId"]
        );
        assert_eq!(f.current(), committed);
        broker.disconnect();
        server.join().unwrap();
    });
}

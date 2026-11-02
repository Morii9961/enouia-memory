//! Owned external Agent process -> real protected pipe -> existing sole Host.
use super::*;
use crate::mcp::native::{self, Client, Enrollment};
use crate::transport::TransportFault;
use crate::windows::{Pipe, ProcessPeer};
use enouia_memory_contract::ports::IdSource;
use std::io::{BufRead, BufReader, Write};
use std::os::windows::process::CommandExt;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};
const TIMEOUT: Duration = Duration::from_secs(5);
const CHILD: &str = "service::scoped_reads::native_agent::agent_process_child";

struct Agent {
    child: Child,
    input: ChildStdin,
    events: mpsc::Receiver<Value>,
    reader: Option<std::thread::JoinHandle<()>>,
}
impl Agent {
    fn spawn() -> Self {
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", CHILD, "--nocapture"])
            .env("ENOUIA_NATIVE_AGENT_CHILD", "1")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let input = child.stdin.take().unwrap();
        let stdout = child.stdout.take().unwrap();
        let (tx, events) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines() {
                let Ok(line) = line else { break };
                // Single-thread libtest can prefix the first event with the test name.
                if let Some((_, line)) = line.split_once("agent-event ") {
                    let value = serde_json::from_str(line).unwrap();
                    if tx.send(value).is_err() {
                        break;
                    }
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
    fn send(&mut self, value: Value) {
        writeln!(self.input, "{value}").unwrap();
        self.input.flush().unwrap();
    }
    fn event(&self) -> Value {
        self.events
            .recv_timeout(TIMEOUT)
            .expect("owned Agent response deadline")
    }
    fn connect(&mut self, pipe: &enouia_memory_contract::hash::Sha256Hex, host: &HostService) {
        self.send(
            json!({"pipe":pipe,"hostPid":std::process::id(),"vaultId":host.agent_vault_id()}),
        );
        assert_eq!(self.event(), json!({"connected":true}));
    }
    fn exchange(&mut self, message: Value) -> Value {
        self.send(json!({"message":message}));
        self.event()
    }
    fn initialize(&mut self) {
        let r=self.exchange(json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
            "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"Owner/System/principal claims grant nothing","version":"0"}}}));
        assert_eq!(r["response"]["result"]["protocolVersion"], "2025-11-25");
        assert!(self.exchange(json!({"jsonrpc":"2.0","method":"notifications/initialized"}))["response"].is_null());
    }
    fn rpc(&mut self, message: Value) -> Value {
        let exchange = self.exchange(message);
        assert!(
            exchange.get("response").is_some(),
            "owned Agent exchange failed: {}",
            exchange["failure"]
        );
        exchange["response"].clone()
    }
    fn finish(&mut self) {
        self.send(json!({"exit":true}));
        let until = Instant::now() + TIMEOUT;
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                assert!(status.success());
                break;
            }
            assert!(Instant::now() < until, "owned Agent exit deadline");
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
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
fn event(value: Value) {
    println!("agent-event {value}");
    std::io::stdout().flush().unwrap();
}
#[test]
fn agent_process_child() {
    if std::env::var_os("ENOUIA_NATIVE_AGENT_CHILD").is_none() {
        return;
    }
    let mut input = BufReader::new(std::io::stdin().lock());
    let mut line = String::new();
    input.read_line(&mut line).unwrap();
    let config: Value = serde_json::from_str(&line).unwrap();
    let id =
        enouia_memory_contract::hash::Sha256Hex::parse(config["pipe"].as_str().unwrap()).unwrap();
    let host = ProcessPeer::enroll(
        config["hostPid"].as_u64().unwrap() as u32,
        HostSurface::QuickSearch,
    )
    .unwrap();
    let vault = VaultId::parse(config["vaultId"].as_str().unwrap()).unwrap();
    let mut client = Client::connect(Pipe::connect(&id).unwrap(), host, &vault, TIMEOUT).unwrap();
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
            Err(f) => event(
                json!({"failure":format!("{:?}",f.fault),"original":f.original_message,"retained":client.original_unknown()}),
            ),
        }
    }
}
fn tool(id: usize, name: &str, args: Value) -> Value {
    let mut packet = proposal_packet(id, name, args, "unused");
    packet["params"]["arguments"]["idempotencyKey"] = Value::Null;
    packet
}
fn content(response: &Value) -> Value {
    response["result"]["structuredContent"].clone()
}
fn pipe_id() -> enouia_memory_contract::hash::Sha256Hex {
    sha256(&enouia_memory_vault::OsIdSource.random_16())
}
fn run_agent(host: &HostService, scopes: Vec<Scope>, run: impl FnOnce(&mut Agent)) {
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut agent = Agent::spawn();
    let enrollment = Enrollment::enroll(agent.child.id(), super::agent(), scopes).unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let result = native::serve_connection(host, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            result
        });
        agent.connect(&id, host);
        agent.initialize();
        run(&mut agent);
        agent.finish();
        assert!(server.join().unwrap().is_err());
    });
}
#[test]
fn native_agent_current_policy_source_scope_sensitivity_and_deletion() {
    let env = Env::new();
    let host = env.host();
    run_agent(&host, vec![Scope::MemoryRead, Scope::SourceRead], |a| {
        assert_eq!(
            content(&a.rpc(tool(2, "memory_read", read(&env.memory))))["error"]["code"],
            "not_found"
        );
        policy(
            &host.vault,
            &[Scope::MemoryRead, Scope::SourceRead],
            Sensitivity::Normal,
            1,
            false,
        );
        let result = content(&a.rpc(tool(3, "memory_search", search_args())));
        assert_eq!(result["result"]["items"], json!([]));
        assert!(!result.to_string().contains(env.memory.as_str()));
        policy(
            &host.vault,
            &[Scope::MemoryRead],
            Sensitivity::Private,
            2,
            false,
        );
        let allowed = content(&a.rpc(tool(4, "memory_read", read(&env.memory))));
        assert!(allowed["result"].is_object(), "authorized read: {allowed}");
        assert_eq!(
            content(&a.rpc(tool(5, "memory_source", source_args(&env.source))))["error"]["code"],
            "not_found"
        );
        policy(
            &host.vault,
            &[Scope::MemoryRead, Scope::SourceRead],
            Sensitivity::Private,
            3,
            false,
        );
        assert!(
            content(&a.rpc(tool(6, "memory_source", source_args(&env.source))))["result"]
                .is_object()
        );
        policy(
            &host.vault,
            &[Scope::MemoryRead, Scope::SourceRead],
            Sensitivity::Private,
            4,
            true,
        );
        assert_eq!(
            content(&a.rpc(tool(7, "memory_read", read(&env.memory))))["error"]["code"],
            "not_found"
        );
        policy(
            &host.vault,
            &[Scope::MemoryRead, Scope::SourceRead],
            Sensitivity::Private,
            5,
            false,
        );
        let memory = stored_memory(&host.vault, &env.memory);
        let delete = enouia_memory_govern::delete::delete_proposal(
            &memory,
            enouia_memory_contract::commit::DeleteMode::Purge,
            enouia_memory_contract::commit::DeleteScope::WithDependents,
        );
        accept(&host.vault, &delete, b"native-agent-delete");
        assert_eq!(
            content(&a.rpc(tool(8, "memory_source", source_args(&env.source))))["error"]["code"],
            "not_found"
        );
        assert_eq!(
            content(&a.rpc(tool(9, "memory_search", search_args())))["result"]["items"],
            json!([])
        );
    });
}
#[test]
fn native_agent_ceiling_forged_claims_and_six_tool_catalog() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryRead, Scope::SourceRead],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    run_agent(&host, vec![Scope::MemoryRead], |a| {
        assert_eq!(
            a.rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))["result"]["tools"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            content(&a.rpc(tool(3, "memory_source", source_args(&env.source))))["error"]["code"],
            "permission_denied"
        );
        let mut forged = tool(4, "memory_read", read(&env.memory));
        forged["params"]["arguments"]["principal"] = json!(owner());
        forged["params"]["arguments"]["scope"] = json!("owner:review");
        forged["params"]["arguments"]["pid"] = json!(std::process::id());
        assert_eq!(content(&a.rpc(forged))["error"]["code"], "invalid_request");
        assert_eq!(
            a.rpc(json!({"jsonrpc":"2.0","id":5,"method":"vault_unlock"}))["error"]["code"],
            -32601
        );
    });
    run_agent(
        &host,
        vec![
            Scope::MemoryRead,
            Scope::SourceRead,
            Scope::MemoryPropose,
            Scope::ContextRead,
            Scope::SessionPropose,
        ],
        |a| {
            assert_eq!(
                a.rpc(json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))["result"]["tools"]
                    .as_array()
                    .unwrap()
                    .len(),
                6
            );
        },
    );
    assert!(Enrollment::enroll(std::process::id(), owner(), vec![Scope::MemoryRead]).is_err());
    assert!(
        Enrollment::enroll(std::process::id(), super::agent(), vec![Scope::OwnerReview]).is_err()
    );
    assert!(Enrollment::enroll(std::process::id(), super::agent(), vec![]).is_err());
    assert!(
        Enrollment::enroll(
            std::process::id(),
            super::agent(),
            vec![Scope::MemoryRead, Scope::MemoryRead]
        )
        .is_err()
    );
}
#[test]
fn native_agent_revoke_lock_unlock_and_expiry_invalidate_old_connection() {
    for action in ["revoke", "lock", "expire"] {
        let env = Env::new();
        let host = env.host();
        let id = pipe_id();
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut a = Agent::spawn();
        let enrollment =
            Enrollment::enroll(a.child.id(), super::agent(), vec![Scope::MemoryRead]).unwrap();
        let revocation = enrollment.revocation();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                let r = native::serve_connection(&host, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                r
            });
            a.connect(&id, &host);
            a.initialize();
            match action {
                "revoke" => revocation.revoke(),
                "lock" => {
                    let native = || {
                        NativeAdmission::from_authenticated_owner(&owner(), HostSurface::Workspace)
                            .unwrap()
                    };
                    host.lock(native()).unwrap();
                    host.unlock(native()).unwrap();
                }
                _ => env.clock.set(NOW + crate::SESSION_TTL_MS),
            }
            let original = json!({"jsonrpc":"2.0","id":2,"method":"tools/list"});
            let failed = a.exchange(original.clone());
            assert_eq!(failed["original"], original);
            assert!(failed["failure"].is_string());
            assert_eq!(a.exchange(original)["failure"], "Closed");
            a.finish();
            assert!(server.join().unwrap().is_err());
        });
    }
}
#[test]
fn native_agent_lost_reply_after_real_commit_retains_key_and_explicit_replay() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryPropose],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    let original = proposal_packet(
        2,
        "memory_propose",
        proposal_args(&env),
        "native-lost-response-original-key",
    );
    let before = host.vault.pin_current().unwrap();
    let mut committed = Value::Null;
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut a = Agent::spawn();
    let enrollment =
        Enrollment::enroll(a.child.id(), super::agent(), vec![Scope::MemoryPropose]).unwrap();
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let r = native::serve_with_reply(&host, &mut pipe, enrollment, TIMEOUT, |r| {
                if r.as_ref().is_some_and(|v| {
                    v["result"]["structuredContent"]["kind"] == "candidate_proposed"
                }) {
                    committed = content(r.as_ref().unwrap());
                    return Err(TransportFault::Io(std::io::ErrorKind::BrokenPipe));
                }
                Ok(())
            });
            let _ = pipe.disconnect();
            r
        });
        a.connect(&id, &host);
        a.initialize();
        let failed = a.exchange(original.clone());
        assert_eq!(failed["failure"], "OutcomeUnknown");
        assert_eq!(failed["original"], original);
        assert_eq!(failed["retained"], original);
        assert_eq!(a.exchange(original.clone())["failure"], "Closed");
        a.finish();
        assert!(server.join().unwrap().is_err());
    });
    let after = host.vault.pin_current().unwrap();
    assert_eq!(after.sequence, before.sequence + 1);
    assert_eq!(committed["result"]["state"], "pending");
    run_agent(&host, vec![Scope::MemoryPropose], |a| {
        assert_eq!(content(&a.rpc(original.clone())), committed);
        assert_eq!(host.vault.pin_current().unwrap(), after);
        let mut changed = original.clone();
        changed["id"] = json!(3);
        changed["params"]["arguments"]["arguments"]["proposedContent"] =
            json!("different synthetic content");
        assert_eq!(
            content(&a.rpc(changed))["error"]["code"],
            "idempotency_conflict"
        );
    });
}

#[test]
fn native_agent_wrong_kernel_peer_and_version_claims_refuse_before_dispatch() {
    use crate::framing::Tag;
    use crate::transport::{read as receive, write as send};
    let env = Env::new();
    let host = env.host();
    for mode in [
        "wrong-peer",
        "root",
        "schema",
        "layout",
        "capability",
        "hash",
        "scope",
        "control",
        "duplicate",
    ] {
        let id = pipe_id();
        let mut pipe = Pipe::listen(&id).unwrap();
        let mut wrong = Agent::spawn();
        let pid = if mode == "wrong-peer" {
            wrong.child.id()
        } else {
            std::process::id()
        };
        let enrollment = Enrollment::enroll(pid, super::agent(), vec![Scope::MemoryRead]).unwrap();
        let before = host.vault.pin_current().unwrap();
        std::thread::scope(|scope| {
            let server = scope.spawn(|| {
                pipe.accept(TIMEOUT).unwrap();
                let r = native::serve_connection(&host, &mut pipe, enrollment, TIMEOUT);
                let _ = pipe.disconnect();
                r
            });
            let mut client = Pipe::connect(&id).unwrap();
            let mut hello = native::hello(host.agent_vault_id());
            match mode {
                "root" => {
                    hello["vaultId"] = json!(VaultId::from_random(host.vault.random_id_bytes()))
                }
                "schema" => hello["recordSchema"] = json!(2),
                "layout" => hello["layout"] = json!("automation-host-v2"),
                "capability" => hello["capability"] = json!("memory-native-read-1"),
                "hash" => hello["compatibilityHash"] = json!(sha256(b"wrong")),
                "scope" => hello["scopes"] = json!(["owner:review"]),
                _ => {}
            }
            if mode == "duplicate" {
                let bytes = hello.to_string();
                let bytes = bytes.replacen('{', "{\"recordSchema\":1,", 1);
                let mut header = [1, 0, 0, 0, 0];
                header[1..].copy_from_slice(&(bytes.len() as u32).to_le_bytes());
                let _ = client.write_all(&header, Instant::now() + TIMEOUT);
                let _ = client.write_all(bytes.as_bytes(), Instant::now() + TIMEOUT);
            } else {
                let _ = send(
                    &mut client,
                    if mode == "control" {
                        Tag::Unlock
                    } else {
                        Tag::Hello
                    },
                    &hello,
                    TIMEOUT,
                );
            }
            assert!(receive(&mut client, false, TIMEOUT).is_err(), "{mode}");
            assert!(server.join().unwrap().is_err(), "{mode}");
        });
        assert_eq!(host.vault.pin_current().unwrap(), before);
        // This child never received native bootstrap and is owned by this test.
        wrong.child.kill().unwrap();
        wrong.child.wait().unwrap();
    }
}
#[test]
fn native_agent_dead_enrollment_is_not_reusable() {
    let mut a = Agent::spawn();
    let enrollment =
        Enrollment::enroll(a.child.id(), super::agent(), vec![Scope::MemoryRead]).unwrap();
    a.child.kill().unwrap();
    a.child.wait().unwrap();
    assert!(enrollment.live().is_err());
}
#[test]
fn native_agent_automation_root_is_refused_at_descriptor_before_records_or_counts() {
    use enouia_memory_contract::next::{
        AutomationDescriptor, AutomationLayout, AutomationPolicy, RuleVersion,
    };
    use enouia_memory_vault::automation::AutomationVault;
    let env = Env::new();
    let path = env.base.join(format!(
        "automation-refusal-{}-{}",
        std::process::id(),
        ROOTS.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&path).unwrap();
    let root = verify_data_root(&path, &RootPolicy::default()).unwrap();
    let mut descriptor: AutomationDescriptor = serde_json::from_str(include_str!(
        "../../../../tests/fixtures/automation-vault/descriptor.json"
    ))
    .unwrap();
    let mut policy: AutomationPolicy =
        serde_json::from_str(include_str!("../../../../tests/fixtures/next/policy.json")).unwrap();
    descriptor.layout = AutomationLayout::HostV2;
    policy.rule_version = RuleVersion::V3;
    policy.allowed_forms = vec![enouia_memory_contract::next::FormationKind::Fact];
    let automation = AutomationVault::create(
        &root,
        descriptor.clone(),
        policy,
        env.config.clock.clone(),
        env.config.ids.clone(),
        Default::default(),
    )
    .unwrap();
    drop(automation);
    // Deliberately invalid business bytes must never be decoded by legacy Host.
    std::fs::write(path.join("vault/CURRENT"), b"not a business record").unwrap();
    let error = HostService::open(&root, descriptor.host_id, env.config.clone())
        .err()
        .unwrap();
    assert!(
        matches!(error,ServiceFault::Vault(ref e) if e.code==enouia_memory_contract::MemoryErrorCode::UnsupportedSchema)
    );
    assert!(path.starts_with(&env.base));
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn native_agent_context_and_checkpoint_keep_agent_provenance_and_original_semantics() {
    let env = Env::new();
    let host = env.host();
    policy(
        &host.vault,
        &[Scope::ContextRead, Scope::MemoryRead, Scope::SessionPropose],
        Sensitivity::Private,
        1,
        false,
    );
    let (sid, bid, input, _) = checkpoint_session(&host, &env.source);
    run_agent(
        &host,
        vec![Scope::ContextRead, Scope::SessionPropose],
        |a| {
            let context = content(&a.rpc(context_packet(2, context_args())));
            assert_eq!(context["kind"], "context_capsule");
            assert!(context_memories(&context) > 0);
            assert_eq!(
                context["result"]["capsule"]["requested_by"],
                json!(super::agent())
            );
            assert_eq!(context["result"]["capsule"]["client_surface"], "mcp_client");
            let mut unsupported = context_args();
            unsupported["sessionId"] = json!(sid);
            unsupported["branchId"] = json!(bid);
            let refused = content(&a.rpc(context_packet(3, unsupported)));
            assert_eq!(refused["kind"], "memory_error");
            let checkpoint = content(&a.rpc(proposal_packet(
                4,
                "session_checkpoint",
                checkpoint_args(&sid, &bid, &input),
                "native-checkpoint-key",
            )));
            assert_eq!(checkpoint["kind"], "checkpoint_proposed");
            let record = checkpoint_record(&host, &checkpoint);
            assert_eq!(record.generated_by.actor, super::agent());
            assert_eq!(
                record.status,
                enouia_memory_contract::session::CheckpointStatus::Provisional
            );
        },
    );
}
#[test]
fn native_agent_stdout_failure_retains_committed_original_without_resending() {
    use crate::mcp::native::ExchangeFault;
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryPropose],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    let id = pipe_id();
    let mut pipe = Pipe::listen(&id).unwrap();
    let enrollment = Enrollment::enroll(
        std::process::id(),
        super::agent(),
        vec![Scope::MemoryPropose],
    )
    .unwrap();
    let original = proposal_packet(
        2,
        "memory_propose",
        proposal_args(&env),
        "native-stdout-lost-key",
    );
    let before = host.vault.pin_current().unwrap();
    struct BrokenOutput;
    impl Write for BrokenOutput {
        fn write(&mut self, _: &[u8]) -> std::io::Result<usize> {
            Err(std::io::ErrorKind::BrokenPipe.into())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    std::thread::scope(|scope| {
        let server = scope.spawn(|| {
            pipe.accept(TIMEOUT).unwrap();
            let r = native::serve_connection(&host, &mut pipe, enrollment, TIMEOUT);
            let _ = pipe.disconnect();
            r
        });
        let peer = ProcessPeer::enroll(std::process::id(), HostSurface::QuickSearch).unwrap();
        let mut client = Client::connect(
            Pipe::connect(&id).unwrap(),
            peer,
            host.agent_vault_id(),
            TIMEOUT,
        )
        .unwrap();
        client.exchange(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"0"}}})).unwrap();
        client
            .exchange(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .unwrap();
        let mut input = std::io::Cursor::new(format!("{original}\n").into_bytes());
        assert!(client.serve_stdio(&mut input, &mut BrokenOutput).is_err());
        assert_eq!(client.original_unknown(), Some(&original));
        assert_eq!(
            client.exchange(&original).unwrap_err().fault,
            ExchangeFault::Closed
        );
        assert_eq!(
            host.vault.pin_current().unwrap().sequence,
            before.sequence + 1
        );
        drop(client);
        assert!(server.join().unwrap().is_err());
    });
}

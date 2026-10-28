use super::*;
#[path = "../../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_vault::{automation::segmented::*, fault::Faults};
use fixture::*;
use std::{
    io::{Read, Write},
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::Arc,
    time::Instant,
};
const TIMEOUT: Duration = Duration::from_secs(15);
fn request() -> RuntimeCaptureRequest {
    let v = fixture::value("runtime-capture/cases.json");
    serde_json::from_value(v["base"].clone()).unwrap()
}
#[test]
fn closed_packets_refuse_versions_duplicate_keys_identity_scope_and_invalid_modes() {
    let call = Call {
        mode: Mode::Write,
        request: request(),
    };
    let base = serde_json::to_value(call).unwrap();
    assert!(parse(&serde_json::to_vec(&base).unwrap()).is_ok());
    for (path, value) in [
        ("/request/schema_version", json!(1)),
        ("/request/capability", json!("memory-agent-catalog-read-1")),
        ("/mode", json!("owner_confirm")),
    ] {
        let mut v = base.clone();
        *v.pointer_mut(path).unwrap() = value;
        assert!(parse(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    for field in ["principal", "scope", "pid"] {
        let mut v = base.clone();
        v[field] = json!("owner");
        assert!(parse(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    assert!(parse(br#"{"mode":"write","mode":"preflight","request":{}}"#).is_err());
}
#[test]
fn loss_child() {
    if std::env::var_os("ENOUIA_RUNTIME_CAPTURE_LOSS_CHILD").is_none() {
        return;
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
    let cfg: Value = serde_json::from_str(&line).unwrap();
    let id = serde_json::from_value(cfg["pipe"].clone()).unwrap();
    let vault = serde_json::from_value(cfg["vault"].clone()).unwrap();
    let pipe = Pipe::connect(&id).unwrap();
    let host = ProcessPeer::enroll(
        cfg["host_pid"].as_u64().unwrap() as u32,
        enouia_memory_contract::workspace::HostSurface::QuickSearch,
    )
    .unwrap();
    let mut client = Client::connect(pipe, host, &vault, TIMEOUT).unwrap();
    let call = Call {
        mode: Mode::Write,
        request: request(),
    };
    let failure = client.exchange(&call).unwrap_err();
    assert_eq!(failure.fault, ExchangeFault::OutcomeUnknown);
    assert_eq!(failure.original, call);
    assert_eq!(client.original_unknown(), Some(&call));
    let mut changed = call.clone();
    changed.request.turns[0].text.push('!');
    assert_eq!(
        client.exchange(&changed).unwrap_err().fault,
        ExchangeFault::Closed
    );
    assert_eq!(client.original_unknown(), Some(&call));
    println!("runtime-loss-retained");
    std::io::stdout().flush().unwrap();
}
struct OwnedChild(Child);
impl Drop for OwnedChild {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
#[test]
fn witnessed_capture_then_lost_pipe_reply_keeps_original_and_never_reconnects() {
    use enouia_memory_contract::ports::IdSource;
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let before = v.snapshot().unwrap();
    let id = sha256(&enouia_memory_vault::OsIdSource.random_16());
    let mut pipe = Pipe::listen(&id).unwrap();
    let mut child = OwnedChild(
        Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "automation::runtime_capture_transport::tests::loss_child",
                "--nocapture",
            ])
            .env("ENOUIA_RUNTIME_CAPTURE_LOSS_CHILD", "1")
            .creation_flags(0x08000000)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap(),
    );
    let host = RuntimeCaptureHost::enroll(
        v.clone(),
        &e.descriptor.created_by,
        child.0.id(),
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
            valid_until: enouia_memory_contract::time::Timestamp::from_unix_ms(NOW + 120_000)
                .unwrap(),
        },
    )
    .unwrap();
    let mut input = child.0.stdin.take().unwrap();
    writeln!(
        input,
        "{}",
        json!({"pipe":id,"vault":e.descriptor.vault_id,"host_pid":std::process::id()})
    )
    .unwrap();
    input.flush().unwrap();
    pipe.accept(TIMEOUT).unwrap();
    assert!(
        serve_with_reply(&host, &mut pipe, TIMEOUT, || Err(
            TransportFault::InvalidFrame
        ))
        .is_err()
    );
    drop(pipe);
    let deadline = Instant::now() + TIMEOUT;
    let status = loop {
        if let Some(s) = child.0.try_wait().unwrap() {
            break s;
        }
        assert!(
            Instant::now() < deadline,
            "owned Runtime child did not exit"
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    let mut out = String::new();
    child
        .0
        .stdout
        .take()
        .unwrap()
        .read_to_string(&mut out)
        .unwrap();
    let mut err = String::new();
    child
        .0
        .stderr
        .take()
        .unwrap()
        .read_to_string(&mut err)
        .unwrap();
    assert!(status.success(), "{out} {err}");
    assert!(out.contains("runtime-loss-retained"));
    assert_eq!(host.original_unknown().unwrap(), Some(request()));
    let after = v.snapshot().unwrap();
    assert_ne!(before, after);
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.sources.len(), 1);
        assert_eq!(
            d.sources.values().next().unwrap().text,
            request().turns[0].text
        );
    })
    .unwrap();
    assert_eq!(after, v.snapshot().unwrap());
}

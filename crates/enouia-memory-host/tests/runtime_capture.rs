#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, next::*, time::Timestamp};
use enouia_memory_host::automation::{
    application::ApplicationFault, runtime_capture::RuntimeCaptureHost,
};
use enouia_memory_vault::{
    automation::{AutomationFault, segmented::*},
    fault::Faults,
};
use fixture::*;
use std::{
    os::windows::process::CommandExt,
    process::{Child, Command, Stdio},
    sync::Arc,
};
struct OwnedPeer(Child);
impl OwnedPeer {
    fn spawn() -> Self {
        Self(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "runtime_capture_process_child", "--nocapture"])
                .env("ENOUIA_SYNTHETIC_RUNTIME_CHILD", "1")
                .creation_flags(0x08000000)
                .stdin(Stdio::piped())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .unwrap(),
        )
    }
    fn exit(&mut self) {
        self.0.stdin.take();
        assert!(self.0.wait().unwrap().success());
    }
}
impl Drop for OwnedPeer {
    fn drop(&mut self) {
        if self.0.try_wait().ok().flatten().is_none() {
            let _ = self.0.kill();
        }
        let _ = self.0.wait();
    }
}
#[test]
fn runtime_capture_process_child() {
    if std::env::var_os("ENOUIA_SYNTHETIC_RUNTIME_CHILD").is_none() {
        return;
    }
    let mut line = String::new();
    std::io::stdin().read_line(&mut line).unwrap();
}
fn request() -> RuntimeCaptureRequest {
    let v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/runtime-capture/cases.json"),
        )
        .unwrap(),
    )
    .unwrap();
    serde_json::from_value(v["base"].clone()).unwrap()
}
fn host(
    e: &Env,
    v: Arc<SegmentedAutomationVault>,
    peer: &OwnedPeer,
    p: &AutomationPolicy,
) -> RuntimeCaptureHost {
    RuntimeCaptureHost::enroll(
        v,
        &e.descriptor.created_by,
        peer.0.id(),
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
#[test]
fn native_process_exit_revocation_and_lost_response_do_not_resend() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let mut peer = OwnedPeer::spawn();
    let h = host(&e, v.clone(), &peer, &p);
    let r = request();
    let before = v.snapshot().unwrap();
    assert!(h.preflight(&r).unwrap().preflight);
    assert_eq!(before, v.snapshot().unwrap());
    let result = h.capture(&r).unwrap();
    assert!(result.captured);
    assert_ne!(before, result.observed_in);
    h.response_lost(&r).unwrap();
    assert_eq!(h.original_unknown().unwrap(), Some(r.clone()));
    assert!(matches!(
        h.capture(&r),
        Err(ApplicationFault::Vault(AutomationFault::OutcomeUnknown))
    ));
    assert_eq!(result.observed_in, v.snapshot().unwrap());
    let second = host(&e, v.clone(), &peer, &p);
    second.revoke();
    assert!(second.preflight(&r).is_err());
    let third = host(&e, v.clone(), &peer, &p);
    peer.exit();
    assert!(third.preflight(&r).is_err());
    assert_eq!(result.observed_in, v.snapshot().unwrap());
}

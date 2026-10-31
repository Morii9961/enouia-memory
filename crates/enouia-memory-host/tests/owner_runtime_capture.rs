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
fn business_files(
    root: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    fn walk(
        root: &std::path::Path,
        dir: &std::path::Path,
        out: &mut std::collections::BTreeMap<std::path::PathBuf, Vec<u8>>,
    ) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, out);
            } else {
                let rel = path.strip_prefix(root).unwrap();
                if rel != std::path::Path::new("indexes/host.lock")
                    && rel != std::path::Path::new("vault/LOCK")
                {
                    out.insert(rel.to_path_buf(), std::fs::read(&path).unwrap());
                }
            }
        }
    }
    let mut out = std::collections::BTreeMap::new();
    walk(root, root, &mut out);
    out
}
fn owner_env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    e
}
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
                .join("../../tests/fixtures/runtime-owner-capture/cases.json"),
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
    RuntimeCaptureHost::enroll_owner(
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
    let e = owner_env();
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

#[test]
fn owner_capture_preflight_replay_and_typed_metadata_match_initialized_reader_without_forming() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let peer = OwnedPeer::spawn();
    let h = host(&e, v.clone(), &peer, &p);
    let r = request();
    assert_eq!(h.capability(), OWNER_RUNTIME_CAPTURE_CAPABILITY);
    let before = v.snapshot().unwrap();
    let bytes_before = business_files(e.root.path());
    let preview = h.preflight(&r).unwrap();
    assert!(preview.preflight);
    assert!(!preview.captured);
    assert!(!preview.formation_scheduled);
    assert_eq!(v.snapshot().unwrap(), before);
    assert_eq!(business_files(e.root.path()), bytes_before);
    let captured = h.capture(&r).unwrap();
    assert!(captured.captured);
    assert!(!captured.formation_scheduled);
    assert_eq!(captured.capability, OWNER_RUNTIME_CAPTURE_CAPABILITY);
    let schemas = schema_support::schema::SchemaStore::load();
    assert!(
        schemas
            .validate(
                "next/runtime-owner-capture-v2.schema.json",
                &serde_json::to_value(&captured).unwrap()
            )
            .is_empty()
    );
    let after = v.snapshot().unwrap();
    let replay = h.capture(&r).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.sources, captured.sources);
    assert_eq!(after, v.snapshot().unwrap());
    let mut conflict = r.clone();
    conflict.turns[0].text.push_str("changed");
    assert!(h.capture(&conflict).is_err());
    assert_eq!(after, v.snapshot().unwrap());
    let ah=enouia_memory_host::automation::qualified_agent::QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(),e.descriptor.created_by.clone()).unwrap();
    let reader = ah
        .reader(
            &e.descriptor.created_by,
            ActorRef {
                actor_type: ActorType::Agent,
                actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
            },
            SegmentedAgentScope {
                subject_id: p.subject_id.clone(),
                project_id: p.project_id.clone(),
                policy_ids: vec![p.policy_id.clone()],
                source_ids: captured
                    .sources
                    .iter()
                    .map(|s| s.source_id.clone())
                    .collect(),
                max_sensitivity: Sensitivity::Private,
                source_read: true,
                valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
            },
        )
        .unwrap();
    let source = reader
        .source(&captured.sources[0].source_id, None, 0, None, 8192)
        .unwrap()
        .unwrap();
    assert_eq!(
        source["record"]["original_timezone"],
        serde_json::Value::Null
    );
    assert_eq!(source["capture_timezone"]["timezone"], "Asia/Shanghai");
    assert_eq!(source["capture_timezone"]["origin"], "owner_default");
    assert_eq!(source["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(source["attachments"][0]["availability"], "missing");
    assert!(source["attachments"][0]["object_hash"].is_null());
    assert!(source["attachments"][0]["size_bytes"].is_null());
    assert_eq!(
        source["record"]["attachment_refs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(after, v.snapshot().unwrap());
    assert!(enouia_memory_host::automation::qualified_agent::QualifiedAgentHost::from_authenticated_owner(v.clone(),e.descriptor.created_by.clone()).is_err());
}
#[test]
fn owner_capture_exact_profile_subject_role_and_old_pipe_are_required() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let principal = ActorRef {
        actor_type: ActorType::Client,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    };
    let scope = RuntimeCaptureScope {
        policy_id: p.policy_id.clone(),
        source_ids: p.source_ids.clone(),
        max_sensitivity: Sensitivity::Private,
        platform: "enouia-runtime".into(),
        account_scope: "account-synthetic".into(),
        default_timezone: "Asia/Shanghai".into(),
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    };
    assert!(
        v.authorize_runtime_capture(&e.descriptor.created_by, principal.clone(), scope.clone())
            .is_err()
    );
    let mut fake = principal.clone();
    fake.actor_type = ActorType::Agent;
    assert!(
        v.authorize_owner_runtime_capture(&e.descriptor.created_by, fake, scope.clone())
            .is_err()
    );
    let permit = v
        .authorize_owner_runtime_capture(&e.descriptor.created_by, principal.clone(), scope.clone())
        .unwrap();
    let mut old = request();
    old.capability = RuntimeCaptureCapability::V1;
    assert!(v.capture_runtime(&permit, &old, false).is_err());
    let peer = OwnedPeer::spawn();
    let h = host(&e, v.clone(), &peer, &p);
    let pipe_id = enouia_memory_contract::hash::sha256(b"synthetic-owner-old-pipe-refusal");
    let mut pipe = enouia_memory_host::windows::Pipe::listen(&pipe_id).unwrap();
    assert!(matches!(
        enouia_memory_host::automation::runtime_capture_transport::serve_connection(
            &h,
            &mut pipe,
            std::time::Duration::from_millis(50)
        ),
        Err(enouia_memory_host::transport::TransportFault::Incompatible)
    ));
    let mut wrong = p;
    wrong.subject_id = SubjectId::parse(&id("sub", 999)).unwrap();
    wrong.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    v.set_policy(&e.descriptor.created_by, wrong).unwrap();
    assert!(
        v.authorize_owner_runtime_capture(&e.descriptor.created_by, principal, scope)
            .is_err()
    );
    let old = Env::new();
    let old_v = old.create(Faults::none());
    let p = old.policy(1, 1, 8);
    old_v
        .set_policy(&old.descriptor.created_by, p.clone())
        .unwrap();
    let scope = RuntimeCaptureScope {
        policy_id: p.policy_id,
        source_ids: p.source_ids,
        max_sensitivity: Sensitivity::Private,
        platform: "enouia-runtime".into(),
        account_scope: "account-synthetic".into(),
        default_timezone: "Asia/Shanghai".into(),
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    };
    assert!(
        old_v
            .authorize_owner_runtime_capture(
                &old.descriptor.created_by,
                ActorRef {
                    actor_type: ActorType::Client,
                    actor_id: PrincipalId::parse(&id("prn", 77)).unwrap()
                },
                scope
            )
            .is_err()
    );
}
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod schema_support;
#[test]
fn owner_runtime_capture_packets_have_independent_closed_schema_and_native_validation() {
    let corpus: serde_json::Value = serde_json::from_str(include_str!(
        "../../../tests/fixtures/runtime-owner-capture/cases.json"
    ))
    .unwrap();
    let schemas = schema_support::schema::SchemaStore::load();
    for case in corpus["cases"].as_array().unwrap() {
        let mut value = corpus["base"].clone();
        schema_support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/runtime-owner-capture-v2.schema.json", &value)
                .is_empty(),
            case["schema"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            serde_json::from_value::<RuntimeCaptureRequest>(value).is_ok_and(|r| r.valid()),
            case["native"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

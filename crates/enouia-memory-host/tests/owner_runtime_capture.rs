#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, json::Revision, next::*, time::Timestamp};
use enouia_memory_host::automation::{
    application::ApplicationFault, runtime_capture::RuntimeCaptureHost,
};
use enouia_memory_vault::{
    OsIdSource,
    automation::{AutomationFault, segmented::*},
    fault::{FaultAction, FaultPoint, Faults},
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
fn client() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Client,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn scope(p: &AutomationPolicy) -> RuntimeCaptureScope {
    RuntimeCaptureScope {
        policy_id: p.policy_id.clone(),
        source_ids: p.source_ids.clone(),
        max_sensitivity: Sensitivity::Private,
        platform: "enouia-runtime".into(),
        account_scope: "account-synthetic".into(),
        default_timezone: "Asia/Shanghai".into(),
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn turn_request(
    n: u32,
    conversation: &str,
    message: &str,
    parent: Option<&str>,
) -> RuntimeCaptureRequest {
    let mut r = request();
    r.request_id = RequestId::parse(&id("req", n)).unwrap();
    r.conversation_id = conversation.into();
    r.turns[0].message_id = message.into();
    r.turns[0].parent_message_id = parent.map(Into::into);
    r.turns[0].sequence = n;
    r.turns[0].attachments.clear();
    r
}
fn native_capture(
    v: &SegmentedAutomationVault,
    e: &Env,
    p: &AutomationPolicy,
    r: &RuntimeCaptureRequest,
) -> NativeRuntimeCapture {
    let permit = v
        .authorize_owner_runtime_capture(&e.descriptor.created_by, client(), scope(p))
        .unwrap();
    v.capture_runtime(&permit, r, false).unwrap()
}
fn revoked(p: &AutomationPolicy) -> AutomationPolicy {
    let mut p = p.clone();
    p.revision = Revision::new(p.revision.get() + 1).unwrap();
    p.active = false;
    p
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
fn captured_queue_admits_exact_revisions_once_and_survives_reopen_without_forming() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let system = e.descriptor.system_actor.clone();
    let peer = OwnedPeer::spawn();
    let h = host(&e, v.clone(), &peer, &p);
    let r = request();
    let captured = h.capture(&r).unwrap();
    assert!(v.queued_captures(&system).unwrap().is_empty());
    let receipt = h.enqueue(&captured).unwrap();
    assert_eq!(receipt.state, "queued");
    let schemas = schema_support::schema::SchemaStore::load();
    assert!(
        schemas
            .validate(
                "next/runtime-owner-capture-v2.schema.json",
                &serde_json::to_value(&receipt).unwrap()
            )
            .is_empty()
    );
    let pointer: AutomationCurrent =
        serde_json::from_slice(&std::fs::read(e.path.join("vault/CURRENT")).unwrap()).unwrap();
    let segment: serde_json::Value = serde_json::from_slice(
        &std::fs::read(e.path.join(format!(
            "vault/automation-segments/{}.json",
            pointer.manifest_sha256
        )))
        .unwrap(),
    )
    .unwrap();
    assert_eq!(segment["delta"]["change"], "enqueue");
    assert!(
        schemas
            .validate("next/catalog-v2.schema.json", &segment)
            .is_empty()
    );
    let after = v.snapshot().unwrap();
    assert_eq!(h.enqueue(&captured).unwrap(), receipt);
    let replay = h.capture(&r).unwrap();
    assert!(replay.replayed);
    assert_eq!(h.enqueue(&replay).unwrap(), receipt);
    assert_eq!(v.snapshot().unwrap(), after);
    let jobs = v.queued_captures(&system).unwrap();
    assert_eq!(
        jobs.iter().map(|j| j.source.clone()).collect::<Vec<_>>(),
        captured.sources
    );
    assert_eq!(
        jobs.iter().map(|j| j.job_key.clone()).collect::<Vec<_>>(),
        receipt.job_keys
    );
    assert!(!captured.formation_scheduled);
    assert_eq!(
        v.read(&system, Sensitivity::HighlySensitive, |read| read
            .memories
            .len())
            .unwrap(),
        0
    );
    drop(h);
    drop(v);
    let reopened = e.open();
    assert_eq!(reopened.queued_captures(&system).unwrap(), jobs);
    assert_eq!(
        reopened
            .enqueue_captured(&system, &captured, &p.reference())
            .unwrap(),
        receipt
    );
    assert_eq!(reopened.snapshot().unwrap(), after);
}
#[test]
fn captured_queue_treats_capture_as_data_and_never_rebinds_the_original_key() {
    let e = owner_env();
    let v = e.create(Faults::none());
    let (owner, system) = (&e.descriptor.created_by, &e.descriptor.system_actor);
    let (p, other) = (e.policy(1, 1, 8), e.policy(2, 9, 8));
    v.set_policy(owner, p.clone()).unwrap();
    v.set_policy(owner, other.clone()).unwrap();
    let pool = p.reference();
    let first = native_capture(&v, &e, &p, &turn_request(1, "synthetic", "m1", None));
    let second = native_capture(&v, &e, &p, &turn_request(2, "synthetic", "m2", Some("m1")));
    let permit = v
        .authorize_owner_runtime_capture(owner, client(), scope(&p))
        .unwrap();
    let preview = v
        .capture_runtime(
            &permit,
            &turn_request(3, "synthetic", "m3", Some("m2")),
            true,
        )
        .unwrap();
    let before = v.snapshot().unwrap();
    let refused = |capture: &NativeRuntimeCapture, policy: &PolicyRef| {
        assert!(matches!(
            v.enqueue_captured(system, capture, policy),
            Err(AutomationFault::Denied)
        ));
    };
    assert!(v.enqueue_captured(owner, &first, &pool).is_err());
    refused(&preview, &pool);
    refused(&first, &other.reference());
    let mut forged = first.clone();
    forged.captured = false;
    refused(&forged, &pool);
    forged = first.clone();
    forged.capability = RUNTIME_CAPTURE_CAPABILITY;
    refused(&forged, &pool);
    forged = first.clone();
    forged.observed_in.vault_id = VaultId::parse(&id("vlt", 9)).unwrap();
    refused(&forged, &pool);
    forged = first.clone();
    forged.sources[0].source_revision = Revision::new(2).unwrap();
    refused(&forged, &pool);
    forged = first.clone();
    forged.sources = second.sources.clone();
    refused(&forged, &pool);
    forged = first.clone();
    forged.request_hash = second.request_hash.clone();
    refused(&forged, &pool);
    forged = first.clone();
    forged.producer.actor_id = PrincipalId::parse(&id("prn", 78)).unwrap();
    refused(&forged, &pool);
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(v.queued_captures(system).unwrap().is_empty());
    v.enqueue_captured(system, &first, &pool).unwrap();
    let after = v.snapshot().unwrap();
    let changed_policy = revoked(&p).reference();
    forged = first.clone();
    forged.sources = second.sources.clone();
    assert!(matches!(
        v.enqueue_captured(system, &forged, &pool),
        Err(AutomationFault::Conflict)
    ));
    assert!(matches!(
        v.enqueue_captured(system, &first, &changed_policy),
        Err(AutomationFault::Conflict)
    ));
    assert_eq!(v.snapshot().unwrap(), after);
    let old = Env::new();
    assert!(matches!(
        old.create(Faults::none())
            .enqueue_captured(&old.descriptor.system_actor, &first, &pool),
        Err(AutomationFault::Denied)
    ));
}
#[test]
fn captured_queue_keeps_native_turn_order_drops_deleted_and_holds_policy_gaps() {
    let e = owner_env();
    let v = e.create(Faults::none());
    let (owner, system) = (&e.descriptor.created_by, &e.descriptor.system_actor);
    let (p, q) = (e.policy(1, 1, 8), e.policy(2, 1, 8));
    v.set_policy(owner, p.clone()).unwrap();
    v.set_policy(owner, q.clone()).unwrap();
    let first = native_capture(&v, &e, &p, &turn_request(1, "synthetic", "m1", None));
    let second = native_capture(&v, &e, &q, &turn_request(2, "synthetic", "m2", Some("m1")));
    let third = native_capture(&v, &e, &q, &turn_request(3, "synthetic", "m3", Some("m2")));
    let other = native_capture(&v, &e, &q, &turn_request(4, "synthetic-b", "o1", None));
    v.enqueue_captured(system, &second, &q.reference()).unwrap();
    v.enqueue_captured(system, &first, &p.reference()).unwrap();
    v.enqueue_captured(system, &third, &q.reference()).unwrap();
    v.enqueue_captured(system, &other, &q.reference()).unwrap();
    let order = |v: &SegmentedAutomationVault| {
        v.queued_captures(system)
            .unwrap()
            .into_iter()
            .map(|j| j.source)
            .collect::<Vec<_>>()
    };
    let refs =
        |c: &[&NativeRuntimeCapture]| c.iter().flat_map(|c| c.sources.clone()).collect::<Vec<_>>();
    assert_eq!(order(&v), refs(&[&first, &second, &third, &other]));
    v.set_policy(owner, revoked(&p)).unwrap();
    assert_eq!(order(&v), refs(&[&other]));
    v.delete(
        owner,
        first.sources.iter().map(|s| s.source_id.clone()).collect(),
        vec![],
    )
    .unwrap();
    assert_eq!(order(&v), refs(&[&second, &third, &other]));
    v.delete(
        owner,
        second.sources.iter().map(|s| s.source_id.clone()).collect(),
        vec![],
    )
    .unwrap();
    assert_eq!(order(&v), refs(&[&third, &other]));
}
#[test]
fn captured_queue_unknown_publication_recovers_through_the_original_key_only() {
    for (point, persisted) in [
        (FaultPoint::StagingWritten, false),
        (FaultPoint::AfterCurrent, true),
    ] {
        let e = owner_env();
        let faults = Faults::armed();
        let v = e.create(faults.clone());
        let system = &e.descriptor.system_actor;
        let p = e.policy(1, 1, 8);
        v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let r = turn_request(1, "synthetic", "m1", None);
        let captured = native_capture(&v, &e, &p, &r);
        faults.arm(point, 1, FaultAction::Fail(112));
        assert!(matches!(
            v.enqueue_captured(system, &captured, &p.reference()),
            Err(AutomationFault::OutcomeUnknown)
        ));
        drop(v);
        let recovered = SegmentedAutomationVault::recover(
            &e.root,
            e.descriptor.host_id.clone(),
            system,
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap();
        assert_eq!(
            recovered.queued_captures(system).unwrap().len(),
            usize::from(persisted)
        );
        let receipt = recovered
            .enqueue_captured(system, &captured, &p.reference())
            .unwrap();
        let after = recovered.snapshot().unwrap();
        assert_eq!(
            recovered
                .enqueue_captured(system, &captured, &p.reference())
                .unwrap(),
            receipt
        );
        assert_eq!(recovered.snapshot().unwrap(), after);
        assert_eq!(recovered.queued_captures(system).unwrap().len(), 1);
    }
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

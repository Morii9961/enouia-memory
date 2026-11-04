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

#[test]
fn producer_status_is_scoped_current_content_free_and_observes_worker_without_lease() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let peer = OwnedPeer::spawn();
    let h = host(&e, v.clone(), &peer, &p);
    let r = request();
    assert!(h.status(&r).is_err());
    let capture = h.capture(&r).unwrap();
    assert!(h.status(&r).is_err()); // capture/enqueue crash gap
    let queue = h.enqueue(&capture).unwrap();
    let before = v.snapshot().unwrap();
    let jobs = h.status(&r).unwrap();
    assert_eq!(jobs.len(), queue.job_keys.len());
    assert!(
        jobs.iter()
            .all(|j| j.state == FormationJobState::Queued && j.receipts == 0)
    );
    let mut changed = r.clone();
    changed.turns[0].text.push('!');
    assert!(h.status(&changed).is_err());
    let foreign = RuntimeCaptureHost::enroll_owner(
        v.clone(),
        &e.descriptor.created_by,
        peer.0.id(),
        ActorRef {
            actor_type: ActorType::Client,
            actor_id: PrincipalId::parse(&id("prn", 78)).unwrap(),
        },
        scope(&p),
    )
    .unwrap();
    assert!(foreign.status(&r).is_err());
    struct Empty;
    impl CaptureAnalyzer for Empty {
        fn start(
            &mut self,
            _: &QueuedCapture,
            _: &CapturedSource,
            _: &[AgentNoteRecord],
            _: &CaptureStopToken,
        ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
            Err(CaptureAnalysis::NoMemory)
        }
    }
    let mut worker = CapturedFormation::open(v.clone(), &e.descriptor.system_actor).unwrap();
    assert!(worker.step(&mut Empty).unwrap());
    assert_eq!(h.status(&r).unwrap()[0].state, FormationJobState::NoMemory);
    assert_eq!(before, v.snapshot().unwrap());
    v.set_policy(&e.descriptor.created_by, revoked(&p)).unwrap();
    assert!(h.status(&r).is_err());
}

use enouia_memory_host::automation::captured_worker::WorkerExit;
use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
#[derive(Clone, Copy)]
enum FakeAction {
    Statement,
    Delayed,
    PollPanic,
    Wait,
    Panic,
    Unknown,
}
struct HostedAnalyzer {
    calls: Arc<AtomicUsize>,
    polls: Arc<AtomicUsize>,
    actions: VecDeque<FakeAction>,
    started: Option<mpsc::Sender<CaptureStopToken>>,
}
struct PendingFake(Arc<AtomicUsize>, Option<CaptureAnalysis>);
impl PendingCaptureAnalysis for PendingFake {
    fn poll(&mut self, _: &CaptureStopToken) -> Option<CaptureAnalysis> {
        let count = self.0.fetch_add(1, Ordering::SeqCst);
        if count >= 2 { self.1.take() } else { None }
    }
}
struct PollPanic;
impl PendingCaptureAnalysis for PollPanic {
    fn poll(&mut self, _: &CaptureStopToken) -> Option<CaptureAnalysis> {
        panic!("owned fake poll panic");
    }
}
impl CaptureAnalyzer for HostedAnalyzer {
    fn start(
        &mut self,
        _: &QueuedCapture,
        source: &CapturedSource,
        _: &[AgentNoteRecord],
        stop: &CaptureStopToken,
    ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(tx) = &self.started {
            tx.send(stop.clone()).unwrap();
        }
        let action = self.actions.pop_front().unwrap_or(FakeAction::Statement);
        match action {
            FakeAction::Wait => Ok(Box::new(PendingFake(self.polls.clone(), None))),
            FakeAction::PollPanic => Ok(Box::new(PollPanic)),
            FakeAction::Unknown => Err(CaptureAnalysis::Unknown),
            FakeAction::Panic => panic!("owned fake analyzer panic"),
            FakeAction::Statement | FakeAction::Delayed => {
                let analysis = CaptureAnalysis::Statements(vec![CaptureStatement {
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
                    event_time: unknown_time(),
                    valid_time: unknown_time(),
                    note: None,
                    prompt_version: "hosted-fake-2".into(),
                    model_version: None,
                }]);
                if matches!(action, FakeAction::Delayed) {
                    Ok(Box::new(PendingFake(self.polls.clone(), Some(analysis))))
                } else {
                    Err(analysis)
                }
            }
        }
    }
}
fn fake(actions: &[FakeAction]) -> HostedAnalyzer {
    HostedAnalyzer {
        calls: Arc::new(AtomicUsize::new(0)),
        polls: Arc::new(AtomicUsize::new(0)),
        actions: actions.iter().copied().collect(),
        started: None,
    }
}
fn wait_until(mut ready: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(15);
    while !ready() {
        assert!(
            Instant::now() < deadline,
            "bounded synthetic worker deadline"
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn hosted_request(n: u32) -> RuntimeCaptureRequest {
    let message = format!("m{n}");
    let mut request = turn_request(
        n,
        "hosted-synthetic",
        &message,
        if n == 1 { None } else { Some("m1") },
    );
    request.turns[0].text = if n == 1 {
        "I prefer quiet rooms."
    } else {
        "I prefer warm light."
    }
    .into();
    request
}
#[test]
fn hosted_worker_forms_automatically_yields_to_foreground_and_releases_its_lease() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let peer = OwnedPeer::spawn();
    let h = Arc::new(host(&e, v.clone(), &peer, &p));
    for n in 1..=2 {
        let c = h.capture(&hosted_request(n)).unwrap();
        h.enqueue(&c).unwrap();
    }
    h.set_foreground(true).unwrap();
    let analyzer = fake(&[FakeAction::Delayed]);
    let calls = analyzer.calls.clone();
    let worker = h.start_worker(analyzer, Duration::from_secs(2)).unwrap();
    assert!(h.start_worker(fake(&[]), Duration::from_secs(2)).is_err());
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    h.set_foreground(false).unwrap();
    wait_until(|| {
        h.status(&hosted_request(2))
            .is_ok_and(|s| s[0].state == FormationJobState::Formed)
    });
    assert_eq!(h.status(&hosted_request(1)).unwrap()[0].receipts, 1);
    assert_eq!(h.status(&hosted_request(2)).unwrap()[0].receipts, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    assert_eq!(worker.shutdown(), WorkerExit::Stopped);
    let reopened = CapturedFormation::open(v.clone(), &e.descriptor.system_actor).unwrap();
    assert_eq!(reopened.status().len(), 2);
}
#[test]
fn hosted_pending_analysis_backs_off_and_all_native_lifecycle_exits_cancel_without_resend() {
    for mode in 0..5 {
        let e = owner_env();
        let v = Arc::new(e.create(Faults::none()));
        let p = e.policy(1, 1, 8);
        v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let mut peer = OwnedPeer::spawn();
        let h = Arc::new(host(&e, v.clone(), &peer, &p));
        let r = hosted_request(1);
        let c = h.capture(&r).unwrap();
        h.enqueue(&c).unwrap();
        let (tx, rx) = mpsc::channel();
        let mut analyzer = fake(&[FakeAction::Wait]);
        let calls = analyzer.calls.clone();
        let polls = analyzer.polls.clone();
        analyzer.started = Some(tx);
        let worker = h.start_worker(analyzer, Duration::from_secs(5)).unwrap();
        let token = rx.recv_timeout(Duration::from_secs(15)).unwrap();
        std::thread::sleep(Duration::from_millis(100));
        assert!(
            polls.load(Ordering::SeqCst) < 10,
            "pending analysis must not spin"
        );
        match mode {
            0 => h.revoke(),
            1 => e.clock.set(NOW + 120_000),
            2 => {
                v.set_policy(&e.descriptor.created_by, revoked(&p)).unwrap();
            }
            3 => peer.exit(),
            _ => {}
        }
        let retained = if mode == 4 {
            drop(h);
            None
        } else {
            Some(h)
        };
        wait_until(|| worker.exit().is_some());
        assert!(token.is_cancelled());
        assert_eq!(worker.shutdown(), WorkerExit::Stopped);
        drop(retained);
        let mut recovered = CapturedFormation::open(v.clone(), &e.descriptor.system_actor).unwrap();
        assert_eq!(
            recovered.status()[0].state,
            FormationJobState::AnalysisUnknown
        );
        assert!(!recovered.step(&mut fake(&[])).unwrap());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            assert!(d.memories.is_empty())
        })
        .unwrap();
    }
}
#[test]
fn hosted_analysis_panic_and_unknown_are_terminal_without_stopping_the_host_or_later_turns() {
    for action in [
        FakeAction::Panic,
        FakeAction::PollPanic,
        FakeAction::Unknown,
    ] {
        let e = owner_env();
        let v = Arc::new(e.create(Faults::none()));
        let p = e.policy(1, 1, 8);
        v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let peer = OwnedPeer::spawn();
        let h = Arc::new(host(&e, v.clone(), &peer, &p));
        for n in 1..=2 {
            let c = h.capture(&hosted_request(n)).unwrap();
            h.enqueue(&c).unwrap();
        }
        let analyzer = fake(&[action]);
        let calls = analyzer.calls.clone();
        let worker = h.start_worker(analyzer, Duration::from_secs(2)).unwrap();
        wait_until(|| {
            h.status(&hosted_request(2))
                .is_ok_and(|s| s[0].state == FormationJobState::Formed)
        });
        assert_eq!(
            h.status(&hosted_request(1)).unwrap()[0].state,
            FormationJobState::AnalysisUnknown
        );
        assert_eq!(calls.load(Ordering::SeqCst), 2);
        assert_eq!(worker.exit(), None);
        assert_eq!(worker.shutdown(), WorkerExit::Stopped);
    }
}
#[test]
fn hosted_analysis_deadline_cancels_and_never_dispatches_that_job_again() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let peer = OwnedPeer::spawn();
    let h = Arc::new(host(&e, v.clone(), &peer, &p));
    let r = hosted_request(1);
    let c = h.capture(&r).unwrap();
    h.enqueue(&c).unwrap();
    let (tx, rx) = mpsc::channel();
    let mut analyzer = fake(&[FakeAction::Wait]);
    let calls = analyzer.calls.clone();
    analyzer.started = Some(tx);
    let worker = h.start_worker(analyzer, Duration::from_millis(60)).unwrap();
    let token = rx.recv_timeout(Duration::from_secs(15)).unwrap();
    wait_until(|| {
        h.status(&r)
            .is_ok_and(|s| s[0].state == FormationJobState::AnalysisUnknown)
    });
    assert!(token.is_cancelled());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(worker.shutdown(), WorkerExit::Stopped);
}

fn unknown_time() -> RecordTime {
    RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    }
}
#[test]
fn hosted_storage_unknown_stops_enrollment_and_recovery_never_reanalyzes() {
    for point in [FaultPoint::StagingWritten, FaultPoint::AfterCurrent] {
        let e = owner_env();
        let faults = Faults::armed();
        let v = Arc::new(e.create(faults.clone()));
        let p = e.policy(1, 1, 8);
        v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let peer = OwnedPeer::spawn();
        let h = Arc::new(host(&e, v.clone(), &peer, &p));
        let r = hosted_request(1);
        let c = h.capture(&r).unwrap();
        h.enqueue(&c).unwrap();
        faults.arm(point, 1, FaultAction::Fail(112));
        let analyzer = fake(&[]);
        let calls = analyzer.calls.clone();
        let worker = h.start_worker(analyzer, Duration::from_secs(2)).unwrap();
        wait_until(|| worker.exit().is_some());
        assert_eq!(worker.shutdown(), WorkerExit::OutcomeUnknown);
        assert!(matches!(
            h.capture(&r),
            Err(ApplicationFault::Vault(AutomationFault::OutcomeUnknown))
        ));
        assert!(h.start_worker(fake(&[]), Duration::from_secs(2)).is_err());
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        drop(h);
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
        let mut recovered = CapturedFormation::open(v.clone(), &e.descriptor.system_actor).unwrap();
        let mut analyzer = fake(&[]);
        let new_calls = analyzer.calls.clone();
        while recovered.step(&mut analyzer).unwrap() {}
        assert_eq!(new_calls.load(Ordering::SeqCst), 0);
        assert_eq!(recovered.status()[0].receipts.len(), 1);
    }
}

#[test]
fn hosted_lost_capture_response_revokes_concurrently_and_retains_the_exact_original() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let peer = OwnedPeer::spawn();
    let h = Arc::new(host(&e, v.clone(), &peer, &p));
    let r = hosted_request(1);
    let c = h.capture(&r).unwrap();
    h.enqueue(&c).unwrap();
    let (tx, rx) = mpsc::channel();
    let mut analyzer = fake(&[FakeAction::Wait]);
    analyzer.started = Some(tx);
    let calls = analyzer.calls.clone();
    let worker = h.start_worker(analyzer, Duration::from_secs(5)).unwrap();
    let token = rx.recv_timeout(Duration::from_secs(15)).unwrap();
    std::thread::scope(|scope| {
        let revoking = h.clone();
        let request = r.clone();
        let stop = scope.spawn(move || revoking.response_lost(&request).unwrap());
        wait_until(|| worker.exit().is_some());
        stop.join().unwrap();
    });
    assert_eq!(h.original_unknown().unwrap(), Some(r.clone()));
    assert!(token.is_cancelled());
    assert_eq!(worker.shutdown(), WorkerExit::Stopped);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert!(matches!(
        h.capture(&r),
        Err(ApplicationFault::Vault(AutomationFault::OutcomeUnknown))
    ));
    assert_eq!(h.original_unknown().unwrap(), Some(r));
}

/// Scripted System correction port: quotes the whole user turn as a correction.
struct HostedCorrections(Arc<AtomicUsize>);
impl CorrectionAnalyzer for HostedCorrections {
    fn start(
        &mut self,
        _: &AgentCorrectionRecord,
        target: &CorrectionTarget,
        source: &CapturedSource,
        _: &CaptureStopToken,
    ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
        self.0.fetch_add(1, Ordering::SeqCst);
        let mut dimensions = target.dimensions.clone();
        dimensions.change = FormationChange::Correction;
        Err(CaptureAnalysis::Statements(vec![CaptureStatement {
            quote: source.text.clone(),
            title: source.text.clone(),
            interpretation: Interpretation::ClearSelfReport,
            dimensions,
            event_time: unknown_time(),
            valid_time: unknown_time(),
            note: None,
            prompt_version: "hosted-correction-1".into(),
            model_version: None,
        }]))
    }
}
#[test]
fn hosted_worker_consumes_a_saved_correction_report_into_a_revision_automatically() {
    use enouia_memory_host::automation::qualified_agent::QualifiedAgentHost;
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let peer = OwnedPeer::spawn();
    let h = Arc::new(host(&e, v.clone(), &peer, &p));
    let first = h.capture(&hosted_request(1)).unwrap();
    h.enqueue(&first).unwrap();
    // The later user turn is captured but not queued: it reaches memory only
    // through the Agent's saved report and the System's own analysis.
    let correction = h.capture(&hosted_request(2)).unwrap();
    let corrections = Arc::new(AtomicUsize::new(0));
    let worker = h
        .start_worker_with_corrections(
            fake(&[FakeAction::Statement]),
            HostedCorrections(corrections.clone()),
            Duration::from_secs(2),
        )
        .unwrap();
    wait_until(|| {
        h.status(&hosted_request(1))
            .is_ok_and(|s| s[0].state == FormationJobState::Formed)
    });
    let system = &e.descriptor.system_actor;
    let target = v
        .read(system, Sensitivity::Private, |d| {
            d.memories[0].target.clone()
        })
        .unwrap();
    let owner = &e.descriptor.created_by;
    let agents =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let reporter = agents
        .correction_reporter(
            owner,
            ActorRef {
                actor_type: ActorType::Agent,
                actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
            },
            SegmentedAgentScope {
                subject_id: p.subject_id.clone(),
                project_id: p.project_id.clone(),
                policy_ids: vec![p.policy_id.clone()],
                source_ids: p.source_ids.clone(),
                max_sensitivity: Sensitivity::Private,
                source_read: true,
                valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
            },
        )
        .unwrap();
    let request = AgentCorrectionRequest {
        request_id: RequestId::parse(&id("req", 41)).unwrap(),
        target: target.clone(),
        source: correction.sources[0].clone(),
        quote_hint: "I prefer warm light.".into(),
        correction: "Untrusted: the user corrected the earlier preference.".into(),
    };
    // The worker shares the writer lock; Busy is not acceptance, so repeat
    // the unchanged original request.
    let receipt = loop {
        match reporter.submit(&request) {
            Err(ApplicationFault::Vault(AutomationFault::Busy | AutomationFault::Stale)) => {
                std::thread::sleep(Duration::from_millis(5))
            }
            other => break other.unwrap(),
        }
    };
    assert_eq!(receipt.state, AgentCorrectionState::ReportSaved);
    wait_until(|| {
        v.read(system, Sensitivity::Private, |d| {
            d.memories
                .iter()
                .any(|m| m.target.memory.memory_id == target.memory.memory_id && m.target != target)
        })
        .unwrap_or(false)
    });
    assert_eq!(worker.shutdown(), WorkerExit::Stopped);
    assert_eq!(corrections.load(Ordering::SeqCst), 1);
    v.read(system, Sensitivity::Private, |d| {
        assert_eq!(d.memories.len(), 1);
        let input = &d.memories[0].memory.decision.input;
        assert_eq!(input.content, "I prefer warm light.");
        assert_eq!(input.target.as_ref(), Some(&target));
        assert_eq!(input.evidence[0].source_id, request.source.source_id);
        assert!(d.owner_actions.is_empty());
    })
    .unwrap();
    // Stopping may precede the final bookkeeping step; a reopened worker
    // finishes it from the journal without analyzing the report again.
    let mut reopened = CapturedFormation::open(v.clone(), system).unwrap();
    let mut analyzer = HostedCorrections(corrections.clone());
    while reopened
        .step_corrections(&mut analyzer, &CaptureStopToken::default())
        .unwrap()
    {}
    assert_eq!(corrections.load(Ordering::SeqCst), 1);
    assert_eq!(reopened.status().len(), 2);
    assert!(
        reopened
            .status()
            .iter()
            .all(|s| s.state == FormationJobState::Formed && s.receipts.len() == 1)
    );
}

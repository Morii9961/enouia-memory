#![cfg(windows)]
#[path = "support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, next::*, time::Timestamp};
use enouia_memory_vault::{
    automation::{AutomationFault, segmented::*},
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;

fn client() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Client,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn scope(_e: &Env, p: &AutomationPolicy) -> RuntimeCaptureScope {
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
fn request() -> RuntimeCaptureRequest {
    RuntimeCaptureRequest {
        schema_version: NextVersion,
        kind: RuntimeCaptureKind::Capture,
        capability: RuntimeCaptureCapability::V1,
        request_id: RequestId::parse(&id("req", 1)).unwrap(),
        conversation_id: "synthetic-conversation".into(),
        title: None,
        turns: vec![RuntimeCaptureTurn {
            message_id: "m1".into(),
            parent_message_id: None,
            branch_id: "main".into(),
            sequence: 1,
            speaker: SpeakerRole::User,
            text: "原样保留。\nＡ B".into(),
            original_time: None,
            original_timezone: None,
            occurred_at: None,
            content_lang: "zh-Hans".into(),
            assistant_model: None,
            attachments: vec![],
        }],
    }
}
fn files(path: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    business_files(path, path)
}
fn business_files(
    root: &std::path::Path,
    path: &std::path::Path,
) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
    let mut result = std::collections::BTreeMap::new();
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        assert!(!entry.file_type().unwrap().is_symlink());
        let full = entry.path();
        let relative = full.strip_prefix(root).unwrap();
        // Native byte-range locks are synchronization objects, not business
        // content. Do not read around or weaken the active lifetime lease.
        if relative == std::path::Path::new("indexes/host.lock")
            || relative == std::path::Path::new("vault/LOCK")
        {
            continue;
        }
        if entry.file_type().unwrap().is_dir() {
            result.extend(business_files(root, &full));
        } else {
            let bytes = std::fs::read(&full)
                .unwrap_or_else(|e| panic!("synthetic business file {relative:?}: {e}"));
            result.insert(full, bytes);
        }
    }
    result
}
#[test]
fn native_preflight_capture_replay_and_provenance_use_real_transactions() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let permit = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), scope(&e, &p))
        .unwrap();
    let mut r = request();
    let mut machine = r.turns[0].clone();
    machine.message_id = "m2".into();
    machine.parent_message_id = Some("m1".into());
    machine.sequence = 2;
    machine.speaker = SpeakerRole::Assistant;
    machine.assistant_model = Some("synthetic-model".into());
    machine.text = "合成 Agent 话语，不是主人确认。".into();
    machine.original_timezone = Some("Etc/UTC".into());
    r.turns.push(machine);
    r.turns[0].attachments.push(RuntimeAttachmentMetadata {
        original_id: "synthetic-file".into(),
        name: Some("sample.txt".into()),
        media_type: None,
        byte_len: Some(9),
        content_hash: None,
    });
    let before = v.snapshot().unwrap();
    let bytes = files(&e.path);
    let check = v.capture_runtime(&permit, &r, true).unwrap();
    assert!(!check.captured && check.preflight && !check.formation_scheduled);
    assert_eq!(before, v.snapshot().unwrap());
    assert_eq!(bytes, files(&e.path));
    let result = v.capture_runtime(&permit, &r, false).unwrap();
    assert!(result.captured && !result.replayed && !result.formation_scheduled);
    assert_eq!(check.sources, result.sources);
    assert_ne!(before, result.observed_in);
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.sources.len(), 2);
        let a = d.sources.get(&result.sources[0].source_id).unwrap();
        assert_eq!(a.text, r.turns[0].text);
        assert!(a.record.original_timezone.is_none() && a.record.occurred_at.is_none());
        assert_eq!(a.record.time_precision, TimePrecision::Unknown);
        assert_eq!(a.record.evidence_class, EvidenceClass::UserStatement);
        assert!(a.record.manual_assertion.is_none() && a.record.agent_submission.is_none());
        assert!(a.record.attachment_refs.is_empty());
        let m: RuntimeCaptureProvenance =
            serde_json::from_value(a.record.extensions["runtime.capture"].clone()).unwrap();
        assert_eq!(m.resolved_timezone, "Asia/Shanghai");
        assert_eq!(m.timezone_origin, RuntimeTimezoneOrigin::OwnerDefault);
        assert!(!m.attachment_bytes_captured);
        assert_eq!(m.attachments, r.turns[0].attachments);
        assert_eq!(m.producer, client());
        let b = d.sources.get(&result.sources[1].source_id).unwrap();
        assert_eq!(b.record.evidence_class, EvidenceClass::ModelClaim);
        assert!(b.attributed_subject.is_none());
        let m: RuntimeCaptureProvenance =
            serde_json::from_value(b.record.extensions["runtime.capture"].clone()).unwrap();
        assert_eq!(m.timezone_origin, RuntimeTimezoneOrigin::Source);
        assert_eq!(m.resolved_timezone, "Etc/UTC");
    })
    .unwrap();
    let after = v.snapshot().unwrap();
    let replay = v.capture_runtime(&permit, &r, false).unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.sources, result.sources);
    assert_eq!(after, v.snapshot().unwrap());
    let renewed = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), scope(&e, &p))
        .unwrap();
    assert!(v.capture_runtime(&renewed, &r, false).unwrap().replayed);
    r.turns[0].text.push('!');
    assert!(matches!(
        v.capture_runtime(&permit, &r, false),
        Err(AutomationFault::Conflict)
    ));
    assert_eq!(after, v.snapshot().unwrap());
    drop(v);
    let opened = e.open();
    assert_eq!(opened.snapshot().unwrap(), after);
}
#[test]
fn narrowed_pool_capacity_cross_instance_expiry_and_clock_rollback_are_closed() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let permit = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), scope(&e, &p))
        .unwrap();
    let r = request();
    v.capture_runtime(&permit, &r, false).unwrap();
    let mut narrow = scope(&e, &p);
    narrow.source_ids.remove(0);
    let narrowed = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), narrow)
        .unwrap();
    let mut changed = r.clone();
    changed.conversation_id = "changed-conversation".into();
    assert!(v.capture_runtime(&narrowed, &changed, true).is_err());
    let other = Env::new();
    let other_v = other.create(Faults::none());
    assert!(other_v.capture_runtime(&permit, &r, true).is_err());
    let mut small = scope(&e, &p);
    small.source_ids.truncate(1);
    let full = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), small)
        .unwrap();
    changed.request_id = RequestId::parse(&id("req", 2)).unwrap();
    assert!(matches!(
        v.capture_runtime(&full, &changed, true),
        Err(AutomationFault::Capacity)
    ));
    e.clock.set(NOW - 1);
    assert!(v.runtime_capture_live(&permit).is_err());
    e.clock.set(NOW);
    assert!(v.runtime_capture_live(&permit).is_err());
    let renewed = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), scope(&e, &p))
        .unwrap();
    e.clock.set(NOW + 120_000);
    assert!(v.runtime_capture_live(&renewed).is_err());
}
#[test]
fn admission_lifecycle_deletion_and_original_key_conflicts_refuse() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let mut p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    for n in 0..5 {
        let mut s = scope(&e, &p);
        let mut who = client();
        match n {
            0 => who.actor_type = ActorType::Agent,
            1 => s.source_ids.push(SourceId::parse(&id("src", 99)).unwrap()),
            2 => s.max_sensitivity = Sensitivity::HighlySensitive,
            3 => s.platform = "agent-hook".into(),
            _ => s.valid_until = Timestamp::from_unix_ms(NOW + 300_001).unwrap(),
        }
        assert!(
            v.authorize_runtime_capture(&e.descriptor.created_by, who, s)
                .is_err()
        );
    }
    let permit = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), scope(&e, &p))
        .unwrap();
    let mut r = request();
    let result = v.capture_runtime(&permit, &r, false).unwrap();
    r.request_id = RequestId::parse(&id("req", 2)).unwrap();
    assert!(v.capture_runtime(&permit, &r, true).is_err());
    r = request();
    v.delete(
        &e.descriptor.created_by,
        vec![result.sources[0].source_id.clone()],
        vec![],
    )
    .unwrap();
    assert!(v.capture_runtime(&permit, &r, false).is_err());
    p.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    v.set_policy(&e.descriptor.created_by, p).unwrap();
    assert!(v.capture_runtime(&permit, &r, true).is_err());
    permit.revoke();
    assert!(v.runtime_capture_live(&permit).is_err());
}
#[test]
fn preflight_has_no_publication_and_unknown_capture_never_auto_retries() {
    let e = Env::new();
    let faults = Faults::armed();
    let v = e.create(faults.clone());
    let p = e.policy(1, 1, 1);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let permit = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), scope(&e, &p))
        .unwrap();
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(112));
    v.capture_runtime(&permit, &request(), true).unwrap();
    assert!(matches!(
        v.capture_runtime(&permit, &request(), false),
        Err(AutomationFault::OutcomeUnknown)
    ));
    assert!(v.capture_runtime(&permit, &request(), false).is_err());
    drop(v);
    faults.disarm();
    // Interrupted publication is not silently repaired by ordinary open.
    assert!(matches!(
        SegmentedAutomationVault::open(
            &e.root,
            e.descriptor.host_id.clone(),
            e.clock.clone(),
            std::sync::Arc::new(enouia_memory_vault::OsIdSource),
            Faults::none()
        ),
        Err(AutomationFault::RecoveryRequired)
    ));
    let v = SegmentedAutomationVault::recover(
        &e.root,
        e.descriptor.host_id.clone(),
        &e.descriptor.system_actor,
        e.clock.clone(),
        std::sync::Arc::new(enouia_memory_vault::OsIdSource),
        Faults::none(),
    )
    .unwrap();
    let fresh = v
        .authorize_runtime_capture(&e.descriptor.created_by, client(), scope(&e, &p))
        .unwrap();
    // Explicit test reconciliation using the unchanged original packet/key.
    assert!(
        v.capture_runtime(&fresh, &request(), false)
            .unwrap()
            .replayed
    );
    e.clock.set(NOW + 120_000);
    assert!(v.runtime_capture_live(&fresh).is_err());
}

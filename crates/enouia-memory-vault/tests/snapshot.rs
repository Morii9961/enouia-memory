//! V08: a reader that pinned a commit sees one consistent snapshot while a
//! writer publishes newer commits; before content leaves (a response or a
//! Provider dispatch), the newest deletions and policy epoch take effect even
//! for an old pin.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::CommitId;
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::ports::{CommitRequest, IdempotencyScope, StagedRecord};
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_vault::fault::Faults;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use support::Harness;

#[test]
fn readers_pinned_during_concurrent_commits_always_see_whole_commits() {
    for _ in 0..16 {
        concurrent_snapshot_once();
    }
}

fn concurrent_snapshot_once() {
    let harness = Arc::new(Harness::with_commits("snapshot-race", 1));
    let done = Arc::new(AtomicBool::new(false));
    let reader = {
        let harness = harness.clone();
        let done = done.clone();
        std::thread::spawn(move || {
            let vault = harness.reopen(Faults::none());
            let mut last = 0;
            let mut checks = 0;
            while !done.load(Ordering::SeqCst) || checks == 0 {
                let pin = vault.pin_current().unwrap();
                assert!(pin.sequence >= last, "head never moves backwards");
                last = pin.sequence;
                let report = vault.verify(&pin).unwrap();
                assert!(
                    report.is_clean(),
                    "pinned commit {} incomplete",
                    pin.sequence
                );
                checks += 1;
            }
            checks
        })
    };
    for index in 2..harness.lifecycle.commits.len() {
        harness.clock.set(harness.lifecycle.time(index));
        let result = harness.vault.commit(harness.lifecycle.request(index));
        if let Err(error) = result {
            done.store(true, Ordering::SeqCst);
            reader.join().unwrap();
            panic!("commit {index}: {error}");
        }
    }
    done.store(true, Ordering::SeqCst);
    let checks = reader.join().unwrap();
    assert!(checks > 0);
}

#[test]
fn deletions_after_the_pin_are_enforced_before_content_leaves() {
    let last = support::Lifecycle::load().commits.len() - 1;
    let harness = Harness::with_commits("snapshot-delete", last - 1);
    let old = harness.vault.pin_current().unwrap();
    let request = harness.lifecycle.request(last);
    let tombstone: serde_json::Value = serde_json::from_slice(
        &request
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::Tombstone)
            .expect("the last lifecycle commit is a logical delete")
            .bytes,
    )
    .unwrap();
    let target = &tombstone["targets"][0];
    let kind: RecordKind = serde_json::from_value(target["record_kind"].clone()).unwrap();
    let id = target["record_id"].as_str().unwrap();
    let manifest = harness.vault.read_manifest(&old).unwrap();
    let entry = manifest
        .catalog
        .iter()
        .find(|e| e.record_kind == kind && e.record_id == id)
        .unwrap();
    let deleted = RecordRef::new(kind, id, entry.revision);
    let other = manifest
        .catalog
        .iter()
        .find(|e| e.record_kind == RecordKind::Memory && e.record_id != id)
        .map(|e| RecordRef::new(e.record_kind, &e.record_id, e.revision))
        .unwrap();
    assert!(
        harness
            .vault
            .check_fresh(&old, std::slice::from_ref(&deleted))
            .is_ok()
    );

    harness.apply(last);
    // The old pin is still a consistent snapshot...
    assert_eq!(harness.vault.read_manifest(&old).unwrap(), manifest);
    assert!(harness.vault.read_record(&old, &deleted).is_ok());
    // ...but nothing deleted since may leave through it.
    let error = harness
        .vault
        .check_fresh(&old, &[deleted.clone(), other.clone()])
        .unwrap_err();
    assert_eq!(
        error.code(),
        MemoryErrorCode::NotFound,
        "non-disclosing refusal"
    );
    let fresh = harness.vault.check_fresh(&old, &[other]).unwrap();
    assert_eq!(fresh.head.sequence, old.sequence + 1);
    assert_eq!(fresh.head.deletion_epoch, old.deletion_epoch + 1);
    assert!(!fresh.policy_changed);
}

#[test]
fn a_policy_change_after_the_pin_requires_a_new_decision() {
    let harness = Harness::with_commits("snapshot-policy", 4);
    let old = harness.vault.pin_current().unwrap();
    let mut policy = harness.lifecycle.record(
        RecordKind::Policy,
        "pol_00000001-0000-4000-8000-000000000001",
        1,
    );
    policy["revision"] = 2.into();
    policy["purposes"] = serde_json::json!(["answer"]);
    let time = harness.lifecycle.time(4) + 60_000;
    let stamp = enouia_memory_contract::time::Timestamp::from_unix_ms(time).unwrap();
    policy["updated_at"] = stamp.as_str().into();
    let owner = harness.lifecycle.owner();
    harness.clock.set(time);
    let request = CommitRequest {
        commit_id: CommitId::parse("cmt_0000f001-0000-4000-8000-00000000f001").unwrap(),
        expected_commit_id: Some(old.commit_id.clone()),
        idempotency: IdempotencyScope {
            principal_id: owner.actor_id.clone(),
            operation_kind: OperationKind::PolicyChange,
            key_hash: sha256(b"narrow genesis purposes"),
        },
        principal: owner,
        operation_kind: OperationKind::PolicyChange,
        request_payload_hash: sha256(b"policy payload"),
        expected_revisions: vec![(
            RecordKind::Policy,
            "pol_00000001-0000-4000-8000-000000000001".to_owned(),
            Revision::new(1),
        )],
        records: vec![StagedRecord {
            record_kind: RecordKind::Policy,
            record_id: "pol_00000001-0000-4000-8000-000000000001".to_owned(),
            revision: Revision::new(2).unwrap(),
            bytes: canonical_bytes(&policy).unwrap(),
        }],
        objects: Vec::new(),
    };
    harness.vault.commit(request).unwrap();
    let fresh = harness.vault.check_fresh(&old, &[]).unwrap();
    assert!(fresh.policy_changed);
    assert_eq!(fresh.head.policy_epoch, old.policy_epoch + 1);
}

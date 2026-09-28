//! D02 behavior: an unknown major version is never written (records) or
//! opened for writing (vault layout); a migration that fails before
//! publication leaves the old `CURRENT`; a successful migration can be
//! undone by restoring the pinned export taken before it.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::common::TrustedSurface;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::CommitId;
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::ports::{CommitRequest, IdempotencyScope, StagedRecord};
use enouia_memory_contract::record::RecordKind;
use enouia_memory_vault::backup::{export_pinned, restore_export};
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::{Fault, Vault};
use support::{Harness, TempRoot, ids, options, verified};

const POLICY: &str = "pol_00000001-0000-4000-8000-000000000001";

/// A synthetic "migration": the genesis policy rewritten as revision 2.
fn migration(harness: &Harness, time: i64) -> CommitRequest {
    let mut policy = harness.lifecycle.record(RecordKind::Policy, POLICY, 1);
    policy["revision"] = 2.into();
    let stamp = enouia_memory_contract::time::Timestamp::from_unix_ms(time).unwrap();
    policy["updated_at"] = stamp.as_str().into();
    let owner = harness.lifecycle.owner();
    CommitRequest {
        commit_id: CommitId::parse("cmt_0000e001-0000-4000-8000-00000000e001").unwrap(),
        expected_commit_id: Some(harness.vault.pin_current().unwrap().commit_id),
        idempotency: IdempotencyScope {
            principal_id: owner.actor_id.clone(),
            operation_kind: OperationKind::Migration,
            key_hash: sha256(b"migration v1 to v1.1"),
        },
        principal: owner,
        operation_kind: OperationKind::Migration,
        request_payload_hash: sha256(b"migration payload"),
        expected_revisions: vec![(RecordKind::Policy, POLICY.to_owned(), Revision::new(1))],
        records: vec![StagedRecord {
            record_kind: RecordKind::Policy,
            record_id: POLICY.to_owned(),
            revision: Revision::new(2).unwrap(),
            bytes: canonical_bytes(&policy).unwrap(),
        }],
        objects: Vec::new(),
    }
}

#[test]
fn unknown_major_versions_are_never_written_or_opened_for_writing() {
    let harness = Harness::with_commits("d02-major", 1);
    let mut request = harness.lifecycle.request(2);
    let mut value: serde_json::Value = serde_json::from_slice(&request.records[0].bytes).unwrap();
    value["schema_version"] = 2.into();
    request.records[0].bytes = canonical_bytes(&value).unwrap();
    harness.clock.set(harness.lifecycle.time(2));
    let error = harness.vault.commit(request).unwrap_err();
    assert_eq!(error.fault, Fault::Contract(vec!["unsupported_schema"]));

    let descriptor = harness.root.path().join("vault/vault.json");
    let text = std::fs::read_to_string(&descriptor).unwrap();
    std::fs::write(
        &descriptor,
        text.replace("\"format_version\": 1", "\"format_version\": 2"),
    )
    .unwrap();
    let error = Vault::open(
        &verified(harness.root.path()),
        None,
        harness.clock.clone(),
        ids(),
        options(Faults::none()),
    )
    .err()
    .unwrap();
    assert_eq!(error.code(), MemoryErrorCode::UnsupportedSchema);
    assert!(
        std::fs::read_to_string(&descriptor)
            .unwrap()
            .contains("\"format_version\": 2"),
        "left untouched"
    );
}

#[test]
fn a_failed_migration_keeps_the_old_current() {
    let harness = Harness::with_commits("d02-fail", 5);
    let before = harness.vault.pin_current().unwrap();
    let faults = Faults::armed();
    let vault = harness.reopen(faults.clone());
    faults.arm(FaultPoint::BeforeCurrent, 1, FaultAction::Fail(112));
    let time = harness.lifecycle.time(5) + 1_000;
    harness.clock.set(time);
    assert!(vault.commit(migration(&harness, time)).is_err());
    assert_eq!(vault.pin_current().unwrap(), before);
    assert!(vault.verify(&before).unwrap().is_clean());
    faults.disarm();
    vault.commit(migration(&harness, time)).unwrap();
    assert_eq!(vault.pin_current().unwrap().sequence, before.sequence + 1);
}

#[test]
fn a_successful_migration_is_undone_by_the_pre_upgrade_export() {
    let harness = Harness::with_commits("d02-undo", 5);
    let before = harness.vault.pin_current().unwrap();
    let snapshot = TempRoot::new("d02-snapshot");
    export_pinned(&harness.vault, &before, &verified(snapshot.path())).unwrap();
    let time = harness.lifecycle.time(5) + 1_000;
    harness.clock.set(time);
    harness.vault.commit(migration(&harness, time)).unwrap();
    let migrated = harness.vault.pin_current().unwrap();
    assert_eq!(migrated.policy_epoch, before.policy_epoch + 1);

    let target = TempRoot::new("d02-restored");
    let restored = restore_export(
        snapshot.path(),
        &verified(target.path()),
        &harness.lifecycle.owner(),
        TrustedSurface::TrustedLocalCli,
        harness.clock.clone(),
        ids(),
        options(Faults::none()),
    )
    .unwrap();
    let pin = restored.vault.pin_current().unwrap();
    assert_eq!(pin, before, "the pre-upgrade commit, exactly");
    let manifest = restored.vault.read_manifest(&pin).unwrap();
    let policy = manifest
        .catalog
        .iter()
        .find(|e| e.record_id == POLICY)
        .unwrap();
    assert_eq!(policy.revision.get(), 1);
}

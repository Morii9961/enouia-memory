//! MV-1.2: the synthetic lifecycle (genesis → import → sessions → candidates
//! → reviews with supersession → logical delete) replayed as real commits.
//! Every manifest the store writes must equal the frozen fixture manifest
//! (except the store-generated operation ID), every record must read back
//! byte-identical from its pinned commit, and the whole record set must pass
//! every cross-record rule at every step.

mod support;

use enouia_memory_contract::commit::CommitManifest;
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::ports::{CommitOutcome, CommitPin};
use enouia_memory_contract::record::RecordRef;
use enouia_memory_contract::{MemoryErrorCode, parse_record};
use enouia_memory_vault::Fault;
use serde_json::Value;
use support::Harness;

fn without_operation(manifest: &Value) -> Value {
    let mut value = manifest.clone();
    value.as_object_mut().unwrap().remove("operation_id");
    value["receipt"]
        .as_object_mut()
        .unwrap()
        .remove("operation_id");
    value
}

fn pin_of(manifest: &CommitManifest) -> CommitPin {
    CommitPin {
        commit_id: manifest.commit_id.clone(),
        sequence: manifest.sequence,
        policy_epoch: manifest.policy_epoch,
        deletion_epoch: manifest.deletion_epoch,
    }
}

#[test]
fn lifecycle_replay_reproduces_every_fixture_manifest() {
    let harness = Harness::with_commits("replay", 0);
    let total = harness.lifecycle.commits.len();
    assert_eq!(total, 19);
    for index in 0..total {
        if index > 0 {
            harness.apply(index);
        }
        let pin = harness.vault.pin_current().unwrap();
        assert_eq!(pin.sequence, index as u64 + 1);
        let stored = harness.vault.read_manifest(&pin).unwrap();
        let expected = &harness.lifecycle.commits[index];
        assert_eq!(
            without_operation(&serde_json::to_value(&stored).unwrap()),
            without_operation(expected),
            "commit {} differs from the fixture",
            index + 1
        );
        let report = harness.vault.verify(&pin).unwrap();
        assert!(report.is_clean(), "commit {}: {report:?}", index + 1);
        assert_eq!(report.records_checked, stored.catalog.len());
    }
    // Every revision ever cataloged reads back byte-identical from the
    // commit that cataloged it, including older revisions after supersession.
    for expected in &harness.lifecycle.commits {
        let manifest: CommitManifest = serde_json::from_value(expected.clone()).unwrap();
        for entry in &manifest.catalog {
            let reference = RecordRef::new(entry.record_kind, &entry.record_id, entry.revision);
            let bytes = harness
                .vault
                .read_record(&pin_of(&manifest), &reference)
                .unwrap();
            let want = canonical_bytes(&harness.lifecycle.record(
                entry.record_kind,
                &entry.record_id,
                entry.revision.get(),
            ))
            .unwrap();
            assert_eq!(bytes, want, "{reference:?}");
        }
    }
}

#[test]
fn a_reopened_handle_sees_the_same_head_and_bytes() {
    let harness = Harness::with_commits("reopen", 6);
    let other = harness.reopen(Default::default());
    let a = harness.vault.pin_current().unwrap();
    let b = other.pin_current().unwrap();
    assert_eq!(a, b);
    let manifest = other.read_manifest(&b).unwrap();
    let on_disk = std::fs::read(
        harness
            .root
            .path()
            .join(format!("vault/commits/{}.json", manifest.commit_id)),
    )
    .unwrap();
    assert_eq!(parse_record::<CommitManifest>(&on_disk).unwrap(), manifest);
}

#[test]
fn stale_head_and_wrong_revisions_conflict_without_writing() {
    let harness = Harness::with_commits("conflict", 4);
    let before = harness.vault.pin_current().unwrap();
    // Commit 7 expects commit 6 as head; the head is still commit 5.
    harness.clock.set(harness.lifecycle.time(6));
    let error = harness
        .vault
        .commit(harness.lifecycle.request(6))
        .unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::RevisionConflict);
    assert_eq!(error.fault, Fault::HeadMoved);
    // Right head, but claims a revision the caller never saw.
    let mut request = harness.lifecycle.request(5);
    let first = request.records[0].clone_meta();
    request.expected_revisions.push((
        first.0,
        first.1,
        enouia_memory_contract::json::Revision::new(9),
    ));
    harness.clock.set(harness.lifecycle.time(5));
    let error = harness.vault.commit(request).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::RevisionConflict);
    assert_eq!(error.fault, Fault::RevisionMismatch);
    assert_eq!(harness.vault.pin_current().unwrap(), before);
    // The untouched request still applies.
    harness.apply(5);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 6);
}

trait Meta {
    fn clone_meta(&self) -> (enouia_memory_contract::record::RecordKind, String);
}

impl Meta for enouia_memory_contract::ports::StagedRecord {
    fn clone_meta(&self) -> (enouia_memory_contract::record::RecordKind, String) {
        (self.record_kind, self.record_id.clone())
    }
}

#[test]
fn retry_after_success_replays_the_same_receipt() {
    let harness = Harness::with_commits("replay-receipt", 2);
    harness.clock.set(harness.lifecycle.time(3));
    let first = harness.vault.commit(harness.lifecycle.request(3)).unwrap();
    let CommitOutcome::Committed { commit_id, receipt } = first else {
        panic!("first attempt must commit");
    };
    let again = harness.vault.commit(harness.lifecycle.request(3)).unwrap();
    assert_eq!(again, CommitOutcome::Replayed { commit_id, receipt });
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 4);
    // Same key, different payload: a conflict, never an overwrite.
    let mut changed = harness.lifecycle.request(3);
    changed.request_payload_hash = enouia_memory_contract::hash::sha256(b"other payload");
    let error = harness.vault.commit(changed).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::IdempotencyConflict);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 4);
}

#[test]
fn clock_regression_pauses_writes() {
    let harness = Harness::with_commits("clock", 2);
    harness.clock.set(harness.lifecycle.time(2) - 1);
    let error = harness
        .vault
        .commit(harness.lifecycle.request(3))
        .unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::ClockRegression);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 3);
}

#[test]
fn invalid_records_and_cross_record_violations_are_refused() {
    let harness = Harness::with_commits("invalid", 4);
    harness.clock.set(harness.lifecycle.time(5));
    // Non-canonical bytes of a valid record.
    let mut request = harness.lifecycle.request(5);
    let mut pretty = serde_json::to_vec_pretty(
        &serde_json::from_slice::<Value>(&request.records[0].bytes).unwrap(),
    )
    .unwrap();
    pretty.extend_from_slice(b"\r\n");
    request.records[0].bytes = pretty;
    let error = harness.vault.commit(request).unwrap_err();
    assert_eq!(error.fault, Fault::Contract(vec!["store.non_canonical"]));
    // A review committed without the candidate revision it decides: the
    // record set no longer closes, so the whole transaction is refused.
    let mut request = harness.lifecycle.request(5);
    let before = request.records.len();
    request
        .records
        .retain(|r| r.record_kind != enouia_memory_contract::record::RecordKind::Candidate);
    assert!(request.records.len() < before);
    let error = harness.vault.commit(request).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::InvalidRequest, "{error}");
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 5);
}

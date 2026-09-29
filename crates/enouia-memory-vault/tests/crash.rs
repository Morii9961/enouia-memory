//! V02/V03/V04: a writer process is killed at every persistence boundary of
//! a transaction that accepts a candidate and supersedes an older memory
//! (candidate revision, review, new memory, and the superseded old memory in
//! one commit). Readers then see the complete old or the complete new
//! commit, never a mix; unpublished files never become records; a retry of
//! the same request commits once (before publication) or replays the stored
//! receipt (after publication, i.e. a lost response).
//!
//! These are process crashes. The OS cache survives them, so they say
//! nothing about OS crashes or power loss (V09).

mod support;

use enouia_memory_contract::commit::CommitManifest;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::store::CurrentState;
use serde_json::Value;
use std::path::PathBuf;
use support::{Harness, Lifecycle, child_env, open_at, spawn_child};

const CHILD: &str = "child_process_entry";

fn point_named(name: &str) -> FaultPoint {
    match name {
        "WriteBytes" => FaultPoint::WriteBytes,
        "BeforeRename" => FaultPoint::BeforeRename,
        other => *FaultPoint::COMMIT_BOUNDARIES
            .iter()
            .find(|p| format!("{p:?}") == other)
            .unwrap(),
    }
}

#[test]
fn child_process_entry() {
    let Some(point) = child_env("POINT") else {
        return;
    };
    let root = PathBuf::from(child_env("ROOT").unwrap());
    let index: usize = child_env("INDEX").unwrap().parse().unwrap();
    let nth: u32 = child_env("NTH").unwrap().parse().unwrap();
    let faults = Faults::armed();
    faults.arm(point_named(&point), nth, FaultAction::Abort);
    let (vault, lifecycle) = open_at(&root, index, faults, 2_000);
    let _ = vault.commit(lifecycle.request(index));
    std::process::exit(0); // reached only if the fault did not fire
}

/// The first lifecycle commit that writes a review together with two
/// memory revisions (a replacement and the memory it supersedes).
fn supersede_index(lifecycle: &Lifecycle) -> usize {
    lifecycle
        .commits
        .iter()
        .position(|c| {
            let changed: Vec<&Value> = c["catalog"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|e| e["changed"] == true)
                .collect();
            let count = |kind: &str| changed.iter().filter(|e| e["record_kind"] == kind).count();
            count("memory") >= 2 && count("review") == 1 && count("candidate") == 1
        })
        .expect("a supersession commit")
}

fn without_operation(manifest: &CommitManifest) -> Value {
    let mut value = serde_json::to_value(manifest).unwrap();
    value.as_object_mut().unwrap().remove("operation_id");
    value["receipt"]
        .as_object_mut()
        .unwrap()
        .remove("operation_id");
    value
}

fn crash_and_retry(point: &str, nth: u32, published: bool) {
    let lifecycle = Lifecycle::load();
    let index = supersede_index(&lifecycle);
    let harness = Harness::with_commits(&format!("crash-{point}-{nth}"), index - 1);
    let before = harness.vault.pin_current().unwrap();
    let status = spawn_child(
        CHILD,
        &[
            ("POINT", point.to_owned()),
            ("NTH", nth.to_string()),
            ("INDEX", index.to_string()),
            ("ROOT", harness.root.path().display().to_string()),
        ],
    )
    .wait()
    .unwrap();
    assert!(
        !status.success(),
        "{point}: the child must die at the fault"
    );

    // A fresh process view after the crash.
    let reader = harness.reopen(Faults::none());
    assert_eq!(
        reader.recovery_report().unwrap().current,
        CurrentState::Valid
    );
    let pin = reader.pin_current().unwrap();
    let expected_sequence = before.sequence + u64::from(published);
    assert_eq!(pin.sequence, expected_sequence, "{point}: visible head");
    assert!(reader.verify(&pin).unwrap().is_clean(), "{point}");
    let head = reader.read_manifest(&pin).unwrap();
    if !published {
        assert_eq!(pin, before, "{point}: the old commit stays whole");
        let new_ids: Vec<String> = harness
            .lifecycle
            .request(index)
            .records
            .iter()
            .filter(|r| r.record_kind == RecordKind::Review)
            .map(|r| r.record_id.clone())
            .collect();
        assert!(
            head.catalog.iter().all(|e| !new_ids.contains(&e.record_id)),
            "{point}: an unpublished review became visible"
        );
    }

    // Retry the very same request from a new writer.
    harness.clock.set(harness.lifecycle.time(index));
    let outcome = reader.commit(harness.lifecycle.request(index)).unwrap();
    match (published, &outcome) {
        (false, CommitOutcome::Committed { .. }) | (true, CommitOutcome::Replayed { .. }) => {}
        other => panic!("{point}: unexpected retry outcome {other:?}"),
    }
    let pin = reader.pin_current().unwrap();
    assert_eq!(
        pin.sequence,
        before.sequence + 1,
        "{point}: applied exactly once"
    );
    assert!(reader.verify(&pin).unwrap().is_clean());
    let stored = reader.read_manifest(&pin).unwrap();
    let fixture: CommitManifest =
        serde_json::from_value(harness.lifecycle.commits[index].clone()).unwrap();
    assert_eq!(
        without_operation(&stored),
        without_operation(&fixture),
        "{point}"
    );
    assert!(
        reader
            .managed_root()
            .list("vault/staging")
            .unwrap()
            .is_empty(),
        "{point}: staging cleared"
    );
}

#[test]
fn a_crash_before_publication_leaves_the_old_commit_and_retries_once() {
    for point in [
        "AfterLock",
        "StagingWritten",
        "RecordsPlaced",
        "ObjectsPlaced",
        "SegmentsPlaced",
        "ManifestWritten",
        "IdempotencyWritten",
        "BeforeCurrent",
    ] {
        crash_and_retry(point, 1, false);
    }
}

#[test]
fn a_crash_inside_a_file_write_or_before_the_current_rename_is_unpublished() {
    // First managed write of the transaction: an empty staging file.
    crash_and_retry("WriteBytes", 1, false);
    // Second atomic replace = CURRENT: the flushed temporary is not renamed.
    crash_and_retry("BeforeRename", 2, false);
}

#[test]
fn a_crash_after_publication_is_a_lost_response_that_replays() {
    for point in ["AfterCurrent", "AfterJournal"] {
        crash_and_retry(point, 1, true);
    }
}

#[test]
fn a_different_payload_after_a_lost_response_conflicts() {
    let lifecycle = Lifecycle::load();
    let index = supersede_index(&lifecycle);
    let harness = Harness::with_commits("crash-conflict", index - 1);
    let status = spawn_child(
        CHILD,
        &[
            ("POINT", "AfterCurrent".to_owned()),
            ("NTH", "1".to_owned()),
            ("INDEX", index.to_string()),
            ("ROOT", harness.root.path().display().to_string()),
        ],
    )
    .wait()
    .unwrap();
    assert!(!status.success());
    let reader = harness.reopen(Faults::none());
    let mut changed = harness.lifecycle.request(index);
    changed.request_payload_hash = enouia_memory_contract::hash::sha256(b"edited after crash");
    let error = reader.commit(changed).unwrap_err();
    assert_eq!(
        error.code(),
        enouia_memory_contract::MemoryErrorCode::IdempotencyConflict
    );
    // Sanity: the harness helpers really open separate handles.
    let _ = open_at(harness.root.path(), index, Faults::none(), 100);
}

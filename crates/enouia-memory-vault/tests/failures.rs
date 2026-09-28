//! V05: disk full, access denied, and sharing violations at each step leave
//! the old state readable, return an explicit error, and a retry applies the
//! transaction exactly once. The errors are injected at the store's I/O
//! boundaries (the real volume is not filled or locked), so this proves the
//! handling path, not the OS's own error reporting.
//!
//! V06: a damaged `CURRENT`, manifest, record, or object is detected and its
//! range reported; the store never guesses the newest manifest, and only an
//! owner's explicit choice of a verified recovery point resumes writing.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
use enouia_memory_contract::ids::PrincipalId;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_contract::record::RecordRef;
use enouia_memory_contract::store::RecoveryEvidence;
use enouia_memory_vault::Fault;
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::store::CurrentState;
use support::Harness;

const DISK_FULL: i32 = 112;
const ACCESS_DENIED: i32 = 5;
const SHARING_VIOLATION: i32 = 32;

#[test]
fn injected_storage_errors_before_publication_keep_the_old_head() {
    for (code, expected, fault) in [
        (DISK_FULL, MemoryErrorCode::StorageFull, Fault::DiskFull),
        (
            ACCESS_DENIED,
            MemoryErrorCode::StorageFailed,
            Fault::AccessDenied,
        ),
        (
            SHARING_VIOLATION,
            MemoryErrorCode::Busy,
            Fault::SharingViolation,
        ),
    ] {
        for point in [
            FaultPoint::WriteBytes,
            FaultPoint::StagingWritten,
            FaultPoint::RecordsPlaced,
            FaultPoint::ManifestWritten,
            FaultPoint::IdempotencyWritten,
            FaultPoint::BeforeCurrent,
            FaultPoint::BeforeRename,
        ] {
            let harness = Harness::with_commits("inject", 3);
            let faults = Faults::armed();
            let vault = harness.reopen(faults.clone());
            let before = vault.pin_current().unwrap();
            faults.arm(point, 1, FaultAction::Fail(code));
            harness.clock.set(harness.lifecycle.time(4));
            let error = vault.commit(harness.lifecycle.request(4)).unwrap_err();
            assert_eq!(error.code(), expected, "{point:?}");
            assert_eq!(error.fault, fault, "{point:?}");
            assert_eq!(vault.pin_current().unwrap(), before, "{point:?}");
            assert!(vault.verify(&before).unwrap().is_clean());
            faults.disarm();
            let outcome = vault.commit(harness.lifecycle.request(4)).unwrap();
            assert!(
                matches!(outcome, CommitOutcome::Committed { .. }),
                "{point:?}"
            );
            assert_eq!(vault.pin_current().unwrap().sequence, before.sequence + 1);
        }
    }
}

#[test]
fn an_error_after_publication_is_reported_but_the_retry_replays() {
    let harness = Harness::with_commits("inject-after", 3);
    let faults = Faults::armed();
    let vault = harness.reopen(faults.clone());
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(DISK_FULL));
    harness.clock.set(harness.lifecycle.time(4));
    assert!(vault.commit(harness.lifecycle.request(4)).is_err());
    assert_eq!(
        vault.pin_current().unwrap().sequence,
        5,
        "already published"
    );
    let outcome = vault.commit(harness.lifecycle.request(4)).unwrap();
    assert!(matches!(outcome, CommitOutcome::Replayed { .. }));
    assert_eq!(vault.pin_current().unwrap().sequence, 5);
}

fn owner(harness: &Harness) -> ActorRef {
    harness.lifecycle.owner()
}

#[test]
fn damaged_current_blocks_reads_and_writes_until_the_owner_adopts_a_point() {
    let harness = Harness::with_commits("current-torn", 6);
    let head = harness.vault.pin_current().unwrap();
    let current = harness.root.path().join("vault/CURRENT");
    let bytes = std::fs::read(&current).unwrap();
    std::fs::write(&current, &bytes[..bytes.len() / 2]).unwrap();

    let vault = harness.reopen(Faults::none());
    assert_eq!(
        vault.pin_current().unwrap_err().code(),
        MemoryErrorCode::VaultRecovering
    );
    harness.clock.set(harness.lifecycle.time(7));
    let error = vault.commit(harness.lifecycle.request(7)).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::VaultRecovering);

    let report = vault.recovery_report().unwrap();
    assert_eq!(report.current, CurrentState::Unreadable);
    assert!(report.current_sha256.is_some());
    let newest = &report.candidates[0];
    assert_eq!(newest.commit_id, head.commit_id);
    assert_eq!(newest.evidence, RecoveryEvidence::PublishJournal);
    assert!(newest.complete);
    assert_eq!(
        report.candidates.len(),
        7,
        "every published commit is listed"
    );

    // Nobody but the owner, and only a listed complete point.
    let agent = ActorRef {
        actor_id: PrincipalId::parse("prn_00000009-0000-4000-8000-000000000009").unwrap(),
        actor_type: ActorType::Agent,
    };
    assert!(
        vault
            .adopt_recovery_point(&head.commit_id, &agent, TrustedSurface::TrustedLocalCli)
            .is_err()
    );
    let receipt = vault
        .adopt_recovery_point(
            &head.commit_id,
            &owner(&harness),
            TrustedSurface::TrustedLocalCli,
        )
        .unwrap();
    assert_eq!(receipt.evidence, RecoveryEvidence::PublishJournal);
    assert_eq!(receipt.previous_current_sha256, report.current_sha256);
    assert!(
        harness
            .root
            .path()
            .join(format!("vault/recovery/{}.json", receipt.recovery_id))
            .exists()
    );
    assert_eq!(vault.pin_current().unwrap(), head);
    // Adoption is refused once CURRENT is valid again.
    assert!(
        vault
            .adopt_recovery_point(
                &head.commit_id,
                &owner(&harness),
                TrustedSurface::TrustedLocalCli
            )
            .is_err()
    );
    harness.apply(7);
}

#[test]
fn a_damaged_head_manifest_is_never_replaced_by_a_guess() {
    let harness = Harness::with_commits("manifest-torn", 6);
    let head = harness.vault.pin_current().unwrap();
    let file = harness
        .root
        .path()
        .join(format!("vault/commits/{}.json", head.commit_id));
    let mut bytes = std::fs::read(&file).unwrap();
    let last = bytes.len() - 3;
    bytes[last] ^= 0x01;
    std::fs::write(&file, bytes).unwrap();

    let vault = harness.reopen(Faults::none());
    assert_eq!(
        vault.pin_current().unwrap_err().code(),
        MemoryErrorCode::VaultRecovering
    );
    let report = vault.recovery_report().unwrap();
    assert_eq!(report.current, CurrentState::ManifestUnverifiable);
    let damaged = report
        .candidates
        .iter()
        .find(|c| c.commit_id == head.commit_id)
        .unwrap();
    assert!(
        !damaged.complete,
        "the damaged head is not offered as complete"
    );
    assert!(
        vault
            .adopt_recovery_point(
                &head.commit_id,
                &owner(&harness),
                TrustedSurface::TrustedLocalCli
            )
            .is_err()
    );
    let previous = report.candidates.iter().find(|c| c.complete).unwrap();
    assert_eq!(previous.sequence, head.sequence - 1);
    vault
        .adopt_recovery_point(
            &previous.commit_id,
            &owner(&harness),
            TrustedSurface::TrustedLocalCli,
        )
        .unwrap();
    assert_eq!(vault.pin_current().unwrap().sequence, head.sequence - 1);
}

#[test]
fn an_unpublished_complete_commit_is_listed_but_never_adopted_by_itself() {
    let harness = Harness::with_commits("unpublished", 4);
    let faults = Faults::armed();
    let vault = harness.reopen(faults.clone());
    faults.arm(FaultPoint::BeforeCurrent, 1, FaultAction::Fail(DISK_FULL));
    harness.clock.set(harness.lifecycle.time(5));
    assert!(vault.commit(harness.lifecycle.request(5)).is_err());
    std::fs::remove_file(harness.root.path().join("vault/CURRENT")).unwrap();

    let vault = harness.reopen(Faults::none());
    let report = vault.recovery_report().unwrap();
    assert_eq!(report.current, CurrentState::Missing);
    assert_eq!(report.current_sha256, None);
    let unpublished = report
        .candidates
        .iter()
        .find(|c| c.evidence == RecoveryEvidence::VerifiedUnpublished)
        .expect("the complete but unpublished commit is visible to the owner");
    assert_eq!(unpublished.sequence, 6);
    assert_eq!(
        report.candidates[0].evidence,
        RecoveryEvidence::PublishJournal
    );
    assert_eq!(
        report.candidates[0].sequence, 5,
        "journal evidence ranks first"
    );
    assert_eq!(
        vault.pin_current().unwrap_err().code(),
        MemoryErrorCode::VaultRecovering
    );
}

#[test]
fn a_damaged_record_is_reported_by_range_and_refused_without_hiding_others() {
    let harness = Harness::with_commits("record-torn", 8);
    let pin = harness.vault.pin_current().unwrap();
    let manifest = harness.vault.read_manifest(&pin).unwrap();
    let victim = manifest
        .catalog
        .iter()
        .find(|e| e.record_kind == enouia_memory_contract::record::RecordKind::Memory)
        .unwrap();
    let reference = RecordRef::new(victim.record_kind, &victim.record_id, victim.revision);
    let file = harness.root.path().join(format!(
        "vault/records/memory/{}/{}.json",
        victim.record_id,
        victim.revision.get()
    ));
    std::fs::write(&file, b"{\"tampered\":true}\n").unwrap();

    let vault = harness.reopen(Faults::none());
    let report = vault.verify(&pin).unwrap();
    assert_eq!(report.corrupt_records, vec![reference.clone()]);
    assert!(report.missing_records.is_empty());
    let error = vault.read_record(&pin, &reference).unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::StorageFailed);
    assert!(!error.error.retryable);
    assert_eq!(error.fault, Fault::Corrupt("record hash"));
    for other in manifest
        .catalog
        .iter()
        .filter(|e| e.record_id != victim.record_id)
    {
        let other = RecordRef::new(other.record_kind, &other.record_id, other.revision);
        assert!(vault.read_record(&pin, &other).is_ok(), "{other:?}");
    }
    std::fs::remove_file(&file).unwrap();
    assert_eq!(vault.verify(&pin).unwrap().missing_records, vec![reference]);
    // Writes that would validate history refuse to build on damaged data.
    harness.clock.set(harness.lifecycle.time(9));
    assert!(vault.commit(harness.lifecycle.request(9)).is_err());
}

#[test]
fn a_damaged_object_is_reported_and_refused() {
    let harness = Harness::with_commits("object-torn", 12);
    let pin = harness.vault.pin_current().unwrap();
    let manifest = harness.vault.read_manifest(&pin).unwrap();
    let object = manifest
        .objects
        .first()
        .expect("identity markdown object")
        .clone();
    assert_eq!(
        harness
            .vault
            .read_object(&pin, &object.object_hash)
            .unwrap()
            .len() as u64,
        object.size_bytes
    );
    let identity = manifest
        .catalog
        .iter()
        .find(|e| e.record_kind == enouia_memory_contract::record::RecordKind::Identity)
        .unwrap();
    let file = harness.root.path().join(format!(
        "vault/records/identity/{}/{}.md",
        identity.record_id,
        identity.revision.get()
    ));
    std::fs::write(&file, "# tampered\n").unwrap();
    let vault = harness.reopen(Faults::none());
    assert_eq!(
        vault.verify(&pin).unwrap().corrupt_objects,
        vec![object.object_hash.clone()]
    );
    assert_eq!(
        vault
            .read_object(&pin, &object.object_hash)
            .unwrap_err()
            .fault,
        Fault::Corrupt("record hash")
    );
}

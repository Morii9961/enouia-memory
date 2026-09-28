//! Owner-invoked sweep: files a crashed or failed writer left behind are
//! quarantined (never deleted) once `CURRENT` verifies; published files are
//! never touched; the sweep is refused while recovery evidence may matter.

mod support;

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use support::Harness;

#[test]
fn a_healthy_vault_has_nothing_to_sweep() {
    let last = support::Lifecycle::load().commits.len() - 1;
    let harness = Harness::with_commits("sweep-clean", last);
    let report = harness.vault.sweep_unreferenced().unwrap();
    assert!(report.files_checked > 50, "{}", report.files_checked);
    assert!(report.quarantined.is_empty(), "{:?}", report.quarantined);
    assert_eq!(report.sweep_id, None);
    let pin = harness.vault.pin_current().unwrap();
    assert!(harness.vault.verify(&pin).unwrap().is_clean());
}

#[test]
fn leftovers_of_failed_writes_are_quarantined_and_the_retry_still_commits() {
    let harness = Harness::with_commits("sweep-leftovers", 11);
    let before = harness.vault.pin_current().unwrap();
    let faults = Faults::armed();
    let vault = harness.reopen(faults.clone());
    // Commit 13 (index 12) adds an identity revision and its Markdown.
    harness.clock.set(harness.lifecycle.time(12));
    faults.arm(FaultPoint::BeforeCurrent, 1, FaultAction::Fail(112));
    assert!(vault.commit(harness.lifecycle.request(12)).is_err());
    faults.disarm();
    // A writer killed inside an atomic replace leaves its flushed temporary
    // (an injected error cleans it up, so the crash leftover is placed here).
    std::fs::write(
        harness
            .root
            .path()
            .join("vault/CURRENT.tmp-00c0ffee00c0ffee"),
        b"{\"partial\":true}",
    )
    .unwrap();
    assert_eq!(vault.pin_current().unwrap(), before);

    let report = vault.sweep_unreferenced().unwrap();
    let moved = &report.quarantined;
    assert!(
        moved.iter().any(|f| f.starts_with("vault/commits/")),
        "{moved:?}"
    );
    assert!(
        moved.iter().any(|f| f.starts_with("vault/idempotency/")),
        "{moved:?}"
    );
    assert!(
        moved
            .iter()
            .any(|f| f.starts_with("vault/records/identity/")),
        "{moved:?}"
    );
    assert!(moved.iter().any(|f| f.contains(".tmp-")), "{moved:?}");
    let sweep_id = report.sweep_id.clone().unwrap();
    let orphans = harness
        .root
        .path()
        .join("vault/orphans")
        .join(sweep_id.as_str());
    assert_eq!(
        std::fs::read_dir(&orphans).unwrap().count(),
        moved.len(),
        "moved, not deleted"
    );
    assert_eq!(vault.pin_current().unwrap(), before);
    assert!(vault.verify(&before).unwrap().is_clean());
    assert!(
        vault.sweep_unreferenced().unwrap().quarantined.is_empty(),
        "idempotent"
    );

    let outcome = vault.commit(harness.lifecycle.request(12)).unwrap();
    assert!(matches!(outcome, CommitOutcome::Committed { .. }));
    let pin = vault.pin_current().unwrap();
    assert!(vault.verify(&pin).unwrap().is_clean());
    let head = vault.read_manifest(&pin).unwrap();
    for object in &head.objects {
        assert!(vault.read_object(&pin, &object.object_hash).is_ok());
    }
    assert!(vault.sweep_unreferenced().unwrap().quarantined.is_empty());
}

#[test]
fn the_sweep_is_refused_while_current_cannot_be_verified() {
    let harness = Harness::with_commits("sweep-recovering", 3);
    std::fs::write(harness.root.path().join("vault/CURRENT"), b"{").unwrap();
    let vault = harness.reopen(Faults::none());
    let error = vault.sweep_unreferenced().unwrap_err();
    assert_eq!(error.code(), MemoryErrorCode::VaultRecovering);
}

//! Synthetic Windows data + authority transactions, with actual filesystem
//! hashes, OS leases and process aborts. No network, credentials or real Vault.
#![cfg(windows)]
mod support;

use enouia_memory_contract::common::TrustedSurface;
use enouia_memory_contract::host::{AuthorityPhase, HostFault};
use enouia_memory_contract::ids::{DeviceId, OperationId};
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_vault::backup::{export_pinned, mark_reconciled};
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::host_authority::ControlChange;
use enouia_memory_vault::host_transaction::{HostCoordinator, MODE_FILE};
use enouia_memory_vault::{Fault, VaultError};
use support::{Harness, TempRoot, child_env, open_at, spawn_child, verified};

fn identity(n: u32) -> DeviceId {
    DeviceId::parse(&format!("dev_{n:08x}-0000-4000-8000-000000000001")).unwrap()
}
fn operation() -> OperationId {
    OperationId::parse("op_00000001-0000-4000-8000-000000000001").unwrap()
}
fn setup(name: &str) -> Harness {
    let h = Harness::with_commits(name, 0);
    // Restrict only this temporary fixture, never silently change product ACLs.
    enouia_memory_vault::platform::restrict_to_owner(h.root.path()).unwrap();
    h
}
fn is_host(error: VaultError, expected: HostFault) {
    assert_eq!(error.fault, Fault::Host(expected));
}

#[test]
fn lifecycle_data_and_authority_advance_in_one_published_pointer() {
    let h = setup("host-data-lifecycle");
    let root = verified(h.root.path());
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    let initial = host.inspect().unwrap();
    assert_eq!(
        initial.checkpoint,
        h.vault.verified_data_checkpoint().unwrap()
    );
    for index in 1..h.lifecycle.commits.len() {
        h.clock.set(h.lifecycle.time(index));
        assert!(matches!(
            host.commit(h.lifecycle.request(index)).unwrap(),
            CommitOutcome::Committed { .. }
        ));
        let state = host.inspect().unwrap();
        assert_eq!(state.checkpoint.commit_sequence.get(), index as u64 + 1);
        assert_eq!(state.revision.get(), index as u64 + 1);
        assert_eq!(
            state.checkpoint,
            h.vault.verified_data_checkpoint().unwrap()
        );
        assert_ne!(
            state.checkpoint.snapshot_hash,
            initial.checkpoint.snapshot_hash
        );
    }
    let final_state = host.inspect().unwrap();
    drop(host);
    let reopened = h.reopen(Faults::none());
    assert_eq!(
        HostCoordinator::open(&reopened, &root, identity(1))
            .unwrap()
            .inspect()
            .unwrap(),
        final_state
    );
}

#[test]
fn legacy_mutations_and_lossy_export_remain_fenced_after_host_exit() {
    let h = setup("host-data-persistent-fence");
    let root = verified(h.root.path());
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    drop(host);
    let before = std::fs::read(root.path().join("vault/CURRENT")).unwrap();
    h.clock.set(h.lifecycle.time(1));
    is_host(
        h.vault.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Fenced,
    );
    is_host(h.vault.sweep_unreferenced().unwrap_err(), HostFault::Fenced);
    is_host(h.vault.purge_files().unwrap_err(), HostFault::Fenced);
    is_host(mark_reconciled(&h.vault).unwrap_err(), HostFault::Fenced);
    let pin = h.vault.pin_current().unwrap();
    let export = TempRoot::new("host-data-no-lossy-export");
    is_host(
        export_pinned(&h.vault, &pin, &verified(export.path())).unwrap_err(),
        HostFault::Fenced,
    );
    is_host(
        h.vault
            .adopt_recovery_point(
                &pin.commit_id,
                &h.vault.descriptor().created_by,
                TrustedSurface::TrustedWindowsApp,
            )
            .unwrap_err(),
        HostFault::Fenced,
    );
    assert_eq!(
        std::fs::read(root.path().join("vault/CURRENT")).unwrap(),
        before
    );
    assert!(std::fs::read_dir(export.path()).unwrap().next().is_none());
}

#[test]
fn quiescence_promotion_replay_and_native_identity_fence_old_writer() {
    let h = setup("host-data-transfer");
    let root = verified(h.root.path());
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    h.clock.set(h.lifecycle.time(1));
    host.commit(h.lifecycle.request(1)).unwrap();
    let state = host.inspect().unwrap();
    let plan = state.prepare_transfer(operation(), identity(2)).unwrap();
    host.apply(ControlChange::Quiesce {
        writer: &state.primary,
        plan: &plan,
        confirmed_hash: &plan.plan_hash,
    })
    .unwrap();
    // Authorization precedes even an already-published idempotency replay.
    is_host(
        host.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Quiesced,
    );
    is_host(
        host.apply(ControlChange::Promote {
            target: &identity(2),
            plan: &plan,
            confirmed_hash: &plan.plan_hash,
            verified_target: &state.checkpoint,
        })
        .unwrap_err(),
        HostFault::Fenced,
    );
    drop(host);
    let target = HostCoordinator::open(&h.vault, &root, identity(2)).unwrap();
    let promoted = target
        .apply(ControlChange::Promote {
            target: &identity(2),
            plan: &plan,
            confirmed_hash: &plan.plan_hash,
            verified_target: &state.checkpoint,
        })
        .unwrap();
    assert_eq!(promoted.primary.primary_epoch.get(), 2);
    assert_eq!(promoted.checkpoint, state.checkpoint);
    assert_eq!(
        target
            .apply(ControlChange::Promote {
                target: &identity(2),
                plan: &plan,
                confirmed_hash: &plan.plan_hash,
                verified_target: &state.checkpoint
            })
            .unwrap(),
        promoted
    );
    h.clock.set(h.lifecycle.time(2));
    target.commit(h.lifecycle.request(2)).unwrap();
    let commit = h
        .vault
        .stored_commit(&h.vault.pin_current().unwrap())
        .unwrap();
    assert_eq!(commit.writer_device_id, identity(2));
    assert_eq!(target.inspect().unwrap().primary, promoted.primary);
    drop(target);
    let old = HostCoordinator::open(&h.vault, &root, identity(1)).unwrap();
    is_host(
        old.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Fenced,
    );
    assert!(
        old.apply(ControlChange::Abort {
            writer: &state.primary,
            plan: &plan,
            confirmed_hash: &plan.plan_hash
        })
        .is_err()
    );
}

#[test]
fn full_data_verification_rechecks_cached_object_bytes_before_any_write() {
    let h = setup("host-data-corruption");
    let root = verified(h.root.path());
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    h.clock.set(h.lifecycle.time(1));
    host.commit(h.lifecycle.request(1)).unwrap();
    let state = host.inspect().unwrap();
    let files = h
        .vault
        .pinned_files(&h.vault.pin_current().unwrap())
        .unwrap();
    let object = files
        .iter()
        .find(|f| f.file.contains("/objects/"))
        .expect("fixture has raw object");
    let before = std::fs::read(root.path().join("vault/CURRENT")).unwrap();
    std::fs::write(root.path().join(&object.file), b"synthetic corruption").unwrap();
    is_host(host.inspect().unwrap_err(), HostFault::CheckpointMismatch);
    is_host(
        host.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::CheckpointMismatch,
    );
    assert_eq!(
        std::fs::read(root.path().join("vault/CURRENT")).unwrap(),
        before
    );
    assert_eq!(state.checkpoint.commit_sequence.get(), 2);
}

#[test]
fn lost_response_replays_committed_data_without_second_authority_advance() {
    let h = setup("host-data-lost-reply");
    let root = verified(h.root.path());
    drop(HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap());
    let faults = Faults::armed();
    let writer = h.reopen(faults.clone());
    let host = HostCoordinator::open(&writer, &root, identity(1)).unwrap();
    h.clock.set(h.lifecycle.time(1));
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(112));
    is_host(
        host.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::OutcomeUnknown,
    );
    let published = host.inspect().unwrap();
    assert_eq!(published.revision.get(), 2);
    assert!(matches!(
        host.commit(h.lifecycle.request(1)).unwrap(),
        CommitOutcome::Replayed { .. }
    ));
    assert_eq!(host.inspect().unwrap(), published);
}

#[test]
fn incomplete_adoption_missing_authority_and_bad_current_never_recover_as_legacy() {
    let h = setup("host-data-partial-adoption");
    let root = verified(h.root.path());
    let faults = Faults::armed();
    let writer = h.reopen(faults.clone());
    faults.arm(FaultPoint::BeforeCurrent, 1, FaultAction::Fail(112));
    assert!(HostCoordinator::bootstrap(&writer, &root, identity(1)).is_err());
    assert!(root.path().join(MODE_FILE).is_file());
    assert!(HostCoordinator::open(&h.vault, &root, identity(1)).is_err());
    is_host(
        h.vault.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Fenced,
    );
    assert!(HostCoordinator::bootstrap(&h.vault, &root, identity(1)).is_err());

    let h = setup("host-data-missing-authority");
    let root = verified(h.root.path());
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    let bytes = std::fs::read(root.path().join("vault/CURRENT")).unwrap();
    let pointer: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    let file = format!(
        "vault/host-authority/{}.json",
        pointer["host_authority_hash"].as_str().unwrap()
    );
    std::fs::remove_file(root.path().join(file)).unwrap();
    is_host(host.inspect().unwrap_err(), HostFault::Unavailable);
    std::fs::write(root.path().join("vault/CURRENT"), b"{broken}").unwrap();
    assert!(host.inspect().is_err());
    is_host(h.vault.sweep_unreferenced().unwrap_err(), HostFault::Fenced);
    assert_eq!(
        std::fs::read(root.path().join("vault/CURRENT")).unwrap(),
        b"{broken}"
    );
}

#[test]
fn child_process_abort_recovers_matching_old_or_new_data_and_authority() {
    for mode in ["before", "after"] {
        let h = setup(&format!("host-data-process-{mode}"));
        let root = verified(h.root.path());
        let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
        let before = host.inspect().unwrap();
        // OS Host exclusion is real, even for a separate process.
        assert!(
            spawn_child(
                "child_process_entry",
                &[
                    ("MODE", "busy".into()),
                    ("ROOT", root.path().display().to_string())
                ]
            )
            .wait()
            .unwrap()
            .success()
        );
        drop(host);
        let status = spawn_child(
            "child_process_entry",
            &[
                ("MODE", mode.into()),
                ("ROOT", root.path().display().to_string()),
            ],
        )
        .wait()
        .unwrap();
        assert!(!status.success());
        assert!(root.path().join("host-data-child-ready").is_file());
        let reopened = h.reopen(Faults::none());
        let host = HostCoordinator::open(&reopened, &root, identity(1)).unwrap();
        let recovered = host.inspect().unwrap();
        assert_eq!(
            recovered.checkpoint,
            reopened.verified_data_checkpoint().unwrap()
        );
        if mode == "before" {
            assert_eq!(recovered, before);
        } else {
            assert_eq!(recovered.revision.get(), 2);
        }
        h.clock.set(h.lifecycle.time(1));
        let outcome = host.commit(h.lifecycle.request(1)).unwrap();
        assert_eq!(
            matches!(outcome, CommitOutcome::Replayed { .. }),
            mode == "after"
        );
        assert_eq!(host.inspect().unwrap().revision.get(), 2);
        assert!(matches!(
            host.inspect().unwrap().phase,
            AuthorityPhase::Active
        ));
    }
}

#[test]
fn child_process_entry() {
    let Some(mode) = child_env("MODE") else {
        return;
    };
    let path = std::path::PathBuf::from(child_env("ROOT").unwrap());
    let root = verified(&path);
    let faults = Faults::armed();
    let (vault, lifecycle) = open_at(&path, 1, faults.clone(), 0);
    if mode == "busy" {
        assert!(
            matches!(HostCoordinator::open(&vault, &root, identity(1)), Err(e) if e.fault == Fault::Host(HostFault::Busy))
        );
        return;
    }
    let host = HostCoordinator::open(&vault, &root, identity(1)).unwrap();
    faults.arm(
        if mode == "before" {
            FaultPoint::BeforeCurrent
        } else {
            FaultPoint::AfterCurrent
        },
        1,
        FaultAction::Abort,
    );
    std::fs::write(
        path.join("host-data-child-ready"),
        b"synthetic child reached commit",
    )
    .unwrap();
    host.commit(lifecycle.request(1)).unwrap();
    panic!("armed process fault did not fire");
}

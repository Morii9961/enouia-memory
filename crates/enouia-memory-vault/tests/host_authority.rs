//! Real Windows control-file/lease tests with SYNTHETIC Vaults. The checkpoint
//! reader is a fixture; this is not full snapshot, data-transaction or OS crash proof.
#![cfg(windows)]
mod support;

use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::host::{
    AuthorityPhase, AuthorityState, Checkpoint, HostFault, HostResult,
};
use enouia_memory_contract::ids::{DeviceId, OperationId, VaultId};
use enouia_memory_contract::json::Revision;
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::host_authority::{
    AUTHORITY_FILE, CheckpointReader, ControlChange, ControlStore,
};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use support::{Harness, child_env, spawn_child, verified};

#[derive(Clone)]
struct FixtureReader {
    checkpoint: Checkpoint,
    available: Arc<AtomicBool>,
}
impl CheckpointReader for FixtureReader {
    fn verified_checkpoint(&self, _: &VaultId) -> HostResult<Checkpoint> {
        if self.available.load(Ordering::SeqCst) {
            Ok(self.checkpoint.clone())
        } else {
            Err(HostFault::Unavailable)
        }
    }
}
fn host(n: u32) -> DeviceId {
    DeviceId::parse(&format!("dev_{n:08x}-0000-4000-8000-000000000001")).unwrap()
}
fn operation() -> OperationId {
    OperationId::parse("op_00000001-0000-4000-8000-000000000001").unwrap()
}
fn setup(name: &str) -> (Harness, AuthorityState, FixtureReader) {
    let harness = Harness::with_commits(name, 0);
    // The old synthetic lifecycle does not set a protected root DACL. The new
    // control store deliberately requires it; secure ONLY this temporary fixture.
    enouia_memory_vault::platform::restrict_to_owner(harness.root.path()).unwrap();
    let pin = harness.vault.pin_current().unwrap();
    let checkpoint = Checkpoint {
        commit_id: pin.commit_id,
        commit_sequence: Revision::new(pin.sequence).unwrap(),
        snapshot_hash: sha256(b"fixture checkpoint, not complete snapshot verification"),
        policy_epoch: pin.policy_epoch,
        deletion_epoch: pin.deletion_epoch,
    };
    let state = AuthorityState::initial(
        harness.vault.vault_id().clone(),
        host(1),
        checkpoint.clone(),
    )
    .unwrap();
    let reader = FixtureReader {
        checkpoint,
        available: Arc::new(AtomicBool::new(true)),
    };
    (harness, state, reader)
}

#[test]
fn unprotected_root_is_rejected_without_silently_changing_permissions() {
    let (h, state, reader) = setup("host-control-other-root");
    let broad = Harness::with_commits("host-control-unprotected-root", 0);
    let root = verified(broad.root.path());
    let before = enouia_memory_vault::platform::inspect_acl(root.path()).unwrap();
    assert!(!before.is_owner_only());
    assert!(matches!(
        ControlStore::bootstrap(&root, state, reader, Faults::none()),
        Err(HostFault::PermissionDenied)
    ));
    assert_eq!(
        enouia_memory_vault::platform::inspect_acl(root.path()).unwrap(),
        before
    );
    assert!(
        broad
            .vault
            .managed_root()
            .read(AUTHORITY_FILE)
            .unwrap()
            .is_none()
    );
    drop(h);
}

#[test]
fn bootstrap_reopen_and_exact_promotion_preserve_real_vault_head() {
    let (h, state, reader) = setup("host-control-reopen");
    let root = verified(h.root.path());
    let before = h.vault.pin_current().unwrap();
    let mut store =
        ControlStore::bootstrap(&root, state.clone(), reader.clone(), Faults::none()).unwrap();
    let plan = state.prepare_transfer(operation(), host(2)).unwrap();
    store
        .apply(ControlChange::Quiesce {
            writer: &state.primary,
            plan: &plan,
            confirmed_hash: &plan.plan_hash,
        })
        .unwrap();
    drop(store);
    let mut reopened = ControlStore::open(&root, reader.clone(), Faults::none()).unwrap();
    assert!(matches!(
        reopened.inspect().unwrap().phase,
        AuthorityPhase::Quiesced { .. }
    ));
    let promoted = reopened
        .apply(ControlChange::Promote {
            target: &host(2),
            plan: &plan,
            confirmed_hash: &plan.plan_hash,
            verified_target: &reader.checkpoint,
        })
        .unwrap();
    assert_eq!(promoted.primary.primary_epoch.get(), 2);
    let bytes = h
        .vault
        .managed_root()
        .read(AUTHORITY_FILE)
        .unwrap()
        .unwrap();
    assert_eq!(
        reopened
            .apply(ControlChange::Promote {
                target: &host(2),
                plan: &plan,
                confirmed_hash: &plan.plan_hash,
                verified_target: &reader.checkpoint
            })
            .unwrap(),
        promoted
    );
    assert_eq!(
        h.vault
            .managed_root()
            .read(AUTHORITY_FILE)
            .unwrap()
            .unwrap(),
        bytes
    );
    assert_eq!(h.vault.pin_current().unwrap(), before);
    assert_eq!(
        promoted.check_writer(&state.vault_id, &state.primary),
        Err(HostFault::Fenced)
    );
    drop(reopened);
    assert!(matches!(
        ControlStore::bootstrap(&root, state, reader, Faults::none()),
        Err(HostFault::Conflict)
    ));
}

#[test]
fn missing_corrupt_or_unavailable_control_never_bootstraps_or_uses_cache() {
    let (h, state, reader) = setup("host-control-invalid");
    let root = verified(h.root.path());
    assert!(matches!(
        ControlStore::open(&root, reader.clone(), Faults::none()),
        Err(HostFault::Unavailable)
    ));
    assert!(
        h.vault
            .managed_root()
            .read(AUTHORITY_FILE)
            .unwrap()
            .is_none()
    );
    let store =
        ControlStore::bootstrap(&root, state.clone(), reader.clone(), Faults::none()).unwrap();
    reader.available.store(false, Ordering::SeqCst);
    assert_eq!(store.inspect(), Err(HostFault::Unavailable));
    reader.available.store(true, Ordering::SeqCst);
    h.vault
        .managed_root()
        .write_atomic(AUTHORITY_FILE, b"{broken}")
        .unwrap();
    assert_eq!(store.inspect(), Err(HostFault::InvalidState));
    drop(store);
    assert!(matches!(
        ControlStore::open(&root, reader.clone(), Faults::none()),
        Err(HostFault::InvalidState)
    ));
    assert!(matches!(
        ControlStore::bootstrap(&root, state, reader, Faults::none()),
        Err(HostFault::Conflict)
    ));
    assert_eq!(
        h.vault
            .managed_root()
            .read(AUTHORITY_FILE)
            .unwrap()
            .unwrap(),
        b"{broken}"
    );
}

#[test]
fn mismatched_native_checkpoint_and_foreign_vault_are_rejected() {
    let (h, state, reader) = setup("host-control-binding");
    let root = verified(h.root.path());
    let mut foreign = state.clone();
    foreign.vault_id = VaultId::parse("vlt_00000002-0000-4000-8000-000000000001").unwrap();
    assert!(matches!(
        ControlStore::bootstrap(&root, foreign, reader.clone(), Faults::none()),
        Err(HostFault::Fenced)
    ));
    let store = ControlStore::bootstrap(&root, state, reader.clone(), Faults::none()).unwrap();
    drop(store);
    let mut wrong = reader;
    wrong.checkpoint.snapshot_hash = sha256(b"wrong native snapshot");
    assert!(matches!(
        ControlStore::open(&root, wrong, Faults::none()),
        Err(HostFault::CheckpointMismatch)
    ));
}

#[test]
fn prepublication_error_preserves_old_file_and_lost_response_recovers_without_rewrite() {
    let (h, state, reader) = setup("host-control-faults");
    let root = verified(h.root.path());
    let faults = Faults::armed();
    let mut store =
        ControlStore::bootstrap(&root, state.clone(), reader.clone(), faults.clone()).unwrap();
    let plan = state.prepare_transfer(operation(), host(2)).unwrap();
    faults.arm(FaultPoint::BeforeRename, 1, FaultAction::Fail(112));
    assert_eq!(
        store.apply(ControlChange::Quiesce {
            writer: &state.primary,
            plan: &plan,
            confirmed_hash: &plan.plan_hash
        }),
        Err(HostFault::OutcomeUnknown)
    );
    assert_eq!(store.inspect().unwrap(), state);
    faults.disarm();
    store
        .apply(ControlChange::Quiesce {
            writer: &state.primary,
            plan: &plan,
            confirmed_hash: &plan.plan_hash,
        })
        .unwrap();
    faults.arm(FaultPoint::HostAuthorityPublished, 1, FaultAction::Fail(5));
    assert_eq!(
        store.apply(ControlChange::Promote {
            target: &host(2),
            plan: &plan,
            confirmed_hash: &plan.plan_hash,
            verified_target: &reader.checkpoint
        }),
        Err(HostFault::OutcomeUnknown)
    );
    let published = store.inspect().unwrap();
    assert_eq!(published.promotions.len(), 1);
    let bytes = h
        .vault
        .managed_root()
        .read(AUTHORITY_FILE)
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .apply(ControlChange::Promote {
                target: &host(2),
                plan: &plan,
                confirmed_hash: &plan.plan_hash,
                verified_target: &reader.checkpoint
            })
            .unwrap(),
        published
    );
    assert_eq!(
        h.vault
            .managed_root()
            .read(AUTHORITY_FILE)
            .unwrap()
            .unwrap(),
        bytes
    );
}

#[test]
fn control_leases_exclude_another_process_and_the_existing_raw_writer_lock() {
    let (h, state, reader) = setup("host-control-process-lock");
    let root = verified(h.root.path());
    let store = ControlStore::bootstrap(&root, state, reader.clone(), Faults::none()).unwrap();
    for mode in ["busy", "raw-writer"] {
        let status = spawn_child(
            "child_process_entry",
            &[
                ("MODE", mode.into()),
                ("ROOT", h.root.path().display().to_string()),
                (
                    "CHECKPOINT",
                    serde_json::to_string(&reader.checkpoint).unwrap(),
                ),
            ],
        )
        .wait()
        .unwrap();
        assert!(
            status.success(),
            "{mode}: child must observe the held OS lease"
        );
    }
    drop(store);
    assert!(ControlStore::open(&root, reader, Faults::none()).is_ok());
}

#[test]
fn process_abort_before_and_after_publication_reopens_only_complete_old_or_new_state() {
    for mode in ["before", "after"] {
        let (h, state, reader) = setup(&format!("host-control-abort-{mode}"));
        let root = verified(h.root.path());
        let store =
            ControlStore::bootstrap(&root, state.clone(), reader.clone(), Faults::none()).unwrap();
        drop(store);
        let status = spawn_child(
            "child_process_entry",
            &[
                ("MODE", mode.into()),
                ("ROOT", h.root.path().display().to_string()),
                (
                    "CHECKPOINT",
                    serde_json::to_string(&reader.checkpoint).unwrap(),
                ),
            ],
        )
        .wait()
        .unwrap();
        assert!(!status.success(), "child must abort at the armed boundary");
        assert!(
            h.root.path().join("authority-child-ready").is_file(),
            "child must reach the controlled apply"
        );
        let reopened = ControlStore::open(&root, reader, Faults::none()).unwrap();
        let recovered = reopened.inspect().unwrap();
        if mode == "before" {
            assert_eq!(recovered, state);
        } else {
            assert!(matches!(recovered.phase, AuthorityPhase::Quiesced { .. }));
            assert_eq!(recovered.revision.get(), 2);
        }
        assert_eq!(recovered.checkpoint, state.checkpoint);
    }
}

#[test]
fn child_process_entry() {
    let Some(mode) = child_env("MODE") else {
        return;
    };
    let root_path = std::path::PathBuf::from(child_env("ROOT").unwrap());
    let checkpoint = serde_json::from_str(&child_env("CHECKPOINT").unwrap()).unwrap();
    let reader = FixtureReader {
        checkpoint,
        available: Arc::new(AtomicBool::new(true)),
    };
    let root = verified(&root_path);
    if mode == "raw-writer" {
        let managed = enouia_memory_vault::fs::ManagedRoot::new(root.path(), Faults::none());
        assert!(
            matches!(enouia_memory_vault::lock::acquire(&managed, std::time::Duration::ZERO), Err(e) if matches!(e.fault, enouia_memory_vault::Fault::WriterBusy))
        );
        return;
    }
    if mode == "busy" {
        assert!(matches!(
            ControlStore::open(&root, reader, Faults::none()),
            Err(HostFault::Busy)
        ));
        return;
    }
    let faults = Faults::armed();
    let mut store = ControlStore::open(&root, reader, faults.clone()).unwrap();
    let state = store.inspect().unwrap();
    let plan = state.prepare_transfer(operation(), host(2)).unwrap();
    faults.arm(
        if mode == "before" {
            FaultPoint::BeforeRename
        } else {
            FaultPoint::HostAuthorityPublished
        },
        1,
        FaultAction::Abort,
    );
    std::fs::write(
        root.path().join("authority-child-ready"),
        b"synthetic child reached apply",
    )
    .unwrap();
    store
        .apply(ControlChange::Quiesce {
            writer: &state.primary,
            plan: &plan,
            confirmed_hash: &plan.plan_hash,
        })
        .unwrap();
    panic!("armed process fault did not fire");
}

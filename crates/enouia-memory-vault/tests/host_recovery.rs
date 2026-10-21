//! Synthetic native forward recovery, never private roots or independent restore.
#![cfg(windows)]
mod support;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::host::{AuthorityPhase, Checkpoint, HostFault};
use enouia_memory_contract::ids::{DeviceId, OperationId, VaultId};
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::host_authority::{CheckpointReader, ControlChange, ControlStore};
use enouia_memory_vault::host_recovery::{HostRecovery, INTENT_FILE, WITNESS_FILE};
use enouia_memory_vault::host_transaction::{HostCoordinator, MODE_FILE};
use enouia_memory_vault::{Fault, VaultError};
use support::{Harness, verified};
fn identity(n: u32) -> DeviceId {
    DeviceId::parse(&format!("dev_{n:08x}-0000-4000-8000-000000000001")).unwrap()
}
fn setup(name: &str) -> Harness {
    let h = Harness::with_commits(name, 0);
    enouia_memory_vault::platform::restrict_to_owner(h.root.path()).unwrap();
    h
}
fn is_host(e: VaultError, want: HostFault) {
    assert_eq!(e.fault, Fault::Host(want));
}
fn controls(h: &Harness) -> Vec<Option<Vec<u8>>> {
    ["vault/CURRENT", MODE_FILE, INTENT_FILE, WITNESS_FILE]
        .iter()
        .map(|p| std::fs::read(h.root.path().join(p)).ok())
        .collect()
}
struct NoCheckpoint;
impl CheckpointReader for NoCheckpoint {
    fn verified_checkpoint(&self, _: &VaultId) -> Result<Checkpoint, HostFault> {
        panic!("pilot must refuse intent-marked roots before reading a checkpoint")
    }
}
#[test]
fn bootstrap_recovers_each_publication_boundary_only_after_exact_confirmation() {
    for point in [
        FaultPoint::HostIntentPublished,
        FaultPoint::BeforeCurrent,
        FaultPoint::AfterCurrent,
        FaultPoint::HostWitnessPublished,
    ] {
        let h = setup(&format!("host-repair-bootstrap-{point:?}"));
        let root = verified(h.root.path());
        let faults = Faults::armed();
        let writer = h.reopen(faults.clone());
        let before = h.vault.verified_data_checkpoint().unwrap();
        faults.arm(point, 1, FaultAction::Fail(112));
        is_host(
            HostCoordinator::bootstrap(&writer, &root, identity(1))
                .err()
                .unwrap(),
            HostFault::OutcomeUnknown,
        );
        assert!(root.path().join(INTENT_FILE).is_file());
        if point == FaultPoint::HostIntentPublished {
            assert!(!root.path().join(MODE_FILE).exists());
        }
        assert!(matches!(
            ControlStore::open(&root, NoCheckpoint, Faults::none()),
            Err(HostFault::Conflict)
        ));
        is_host(
            h.vault.commit(h.lifecycle.request(1)).unwrap_err(),
            HostFault::Fenced,
        );
        let recovery = HostRecovery::open(&writer, &root, identity(1)).unwrap();
        let observed = controls(&h);
        let plan = recovery.plan().unwrap();
        assert_eq!(controls(&h), observed, "planning must not repair");
        assert_eq!(plan.target().checkpoint, before);
        is_host(
            recovery
                .confirm(&plan, &sha256(b"wrong-confirmation"))
                .unwrap_err(),
            HostFault::PlanMismatch,
        );
        assert_eq!(controls(&h), observed);
        let fixed = recovery.confirm(&plan, plan.hash()).unwrap();
        assert_eq!(fixed.checkpoint, before);
        let after = controls(&h);
        assert_eq!(recovery.confirm(&plan, plan.hash()).unwrap(), fixed);
        assert_eq!(
            controls(&h),
            after,
            "exact completed repair must not publish again"
        );
        assert!(!recovery.plan().unwrap().repair_needed());
        drop(recovery);
        assert_eq!(
            HostCoordinator::open(&writer, &root, identity(1))
                .unwrap()
                .inspect()
                .unwrap(),
            fixed
        );
    }
}
#[test]
fn corrupt_current_recovers_the_latest_witness_without_lowering_any_watermark() {
    let h = setup("host-repair-current");
    let root = verified(h.root.path());
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    for n in 1..h.lifecycle.commits.len() {
        h.clock.set(h.lifecycle.time(n));
        host.commit(h.lifecycle.request(n)).unwrap();
    }
    let latest = host.inspect().unwrap();
    drop(host);
    std::fs::write(root.path().join("vault/CURRENT"), b"{broken}").unwrap();
    let reopened = h.reopen(Faults::none());
    assert!(HostCoordinator::open(&reopened, &root, identity(1)).is_err());
    let recovery = HostRecovery::open(&reopened, &root, identity(1)).unwrap();
    let plan = recovery.plan().unwrap();
    assert_eq!(plan.target(), &latest);
    assert!(plan.repair_needed());
    assert_eq!(recovery.confirm(&plan, plan.hash()).unwrap(), latest);
    assert_eq!(
        reopened.pin_current().unwrap().commit_id,
        latest.checkpoint.commit_id
    );
    assert_eq!(
        reopened.pin_current().unwrap().policy_epoch,
        latest.checkpoint.policy_epoch
    );
    assert_eq!(
        reopened.pin_current().unwrap().deletion_epoch,
        latest.checkpoint.deletion_epoch
    );
}
#[test]
fn missing_or_corrupt_witness_and_changed_observations_never_initialize_or_guess() {
    let h = setup("host-repair-refusals");
    let root = verified(h.root.path());
    let legacy_current = std::fs::read(root.path().join("vault/CURRENT")).unwrap();
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    h.clock.set(h.lifecycle.time(1));
    host.commit(h.lifecycle.request(1)).unwrap();
    drop(host);
    let witness = std::fs::read(root.path().join(WITNESS_FILE)).unwrap();
    let intent = std::fs::read(root.path().join(INTENT_FILE)).unwrap();
    let recovery = HostRecovery::open(&h.vault, &root, identity(1)).unwrap();
    std::fs::remove_file(root.path().join(WITNESS_FILE)).unwrap();
    is_host(recovery.plan().unwrap_err(), HostFault::Unavailable);
    assert!(!root.path().join(WITNESS_FILE).exists());
    std::fs::write(root.path().join(WITNESS_FILE), b"{broken}").unwrap();
    is_host(recovery.plan().unwrap_err(), HostFault::InvalidState);
    std::fs::write(root.path().join(WITNESS_FILE), &witness).unwrap();
    std::fs::write(root.path().join(INTENT_FILE), b"{broken}").unwrap();
    is_host(recovery.plan().unwrap_err(), HostFault::InvalidState);
    std::fs::write(root.path().join(INTENT_FILE), intent).unwrap();
    let selected: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.path().join(INTENT_FILE)).unwrap()).unwrap();
    let previous = root.path().join(format!(
        "vault/host-authority/{}.json",
        selected["before"]["host_authority_hash"].as_str().unwrap()
    ));
    let previous_bytes = std::fs::read(&previous).unwrap();
    std::fs::remove_file(&previous).unwrap();
    is_host(recovery.plan().unwrap_err(), HostFault::Unavailable);
    assert!(!previous.exists());
    std::fs::write(&previous, previous_bytes).unwrap();
    let plan = recovery.plan().unwrap();
    std::fs::write(root.path().join("vault/CURRENT"), b"{changed}").unwrap();
    is_host(
        recovery.confirm(&plan, plan.hash()).unwrap_err(),
        HostFault::Conflict,
    );
    assert_eq!(
        std::fs::read(root.path().join("vault/CURRENT")).unwrap(),
        b"{changed}"
    );
    let fresh = recovery.plan().unwrap();
    recovery.confirm(&fresh, fresh.hash()).unwrap();
    std::fs::write(root.path().join("vault/CURRENT"), &legacy_current).unwrap();
    is_host(recovery.plan().unwrap_err(), HostFault::Conflict);
    assert_eq!(
        std::fs::read(root.path().join("vault/CURRENT")).unwrap(),
        legacy_current
    );
}
#[test]
fn exact_intent_restores_a_missing_authority_but_refuses_damaged_data() {
    let h = setup("host-repair-data");
    let root = verified(h.root.path());
    let host = HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap();
    h.clock.set(h.lifecycle.time(1));
    host.commit(h.lifecycle.request(1)).unwrap();
    let latest = host.inspect().unwrap();
    drop(host);
    let p: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.path().join("vault/CURRENT")).unwrap()).unwrap();
    let path = root.path().join(format!(
        "vault/host-authority/{}.json",
        p["host_authority_hash"].as_str().unwrap()
    ));
    let original = std::fs::read(&path).unwrap();
    std::fs::remove_file(&path).unwrap();
    let recovery = HostRecovery::open(&h.vault, &root, identity(1)).unwrap();
    let plan = recovery.plan().unwrap();
    assert!(plan.repair_needed());
    assert_eq!(recovery.confirm(&plan, plan.hash()).unwrap(), latest);
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let pin = h.vault.pin_current().unwrap();
    let files = h.vault.pinned_files(&pin).unwrap();
    let object = files.iter().find(|f| f.file.contains("/objects/")).unwrap();
    let before = controls(&h);
    std::fs::write(root.path().join(&object.file), b"synthetic damaged bytes").unwrap();
    is_host(recovery.plan().unwrap_err(), HostFault::CheckpointMismatch);
    assert_eq!(controls(&h), before);
}
#[test]
fn repair_lost_reply_reuses_the_original_plan_without_an_extra_publication() {
    let h = setup("host-repair-replay");
    let root = verified(h.root.path());
    let faults = Faults::armed();
    let writer = h.reopen(faults.clone());
    faults.arm(FaultPoint::BeforeCurrent, 1, FaultAction::Fail(112));
    assert!(HostCoordinator::bootstrap(&writer, &root, identity(1)).is_err());
    let recovery = HostRecovery::open(&writer, &root, identity(1)).unwrap();
    let plan = recovery.plan().unwrap();
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(112));
    is_host(
        recovery.confirm(&plan, plan.hash()).unwrap_err(),
        HostFault::OutcomeUnknown,
    );
    let recovered = recovery.confirm(&plan, plan.hash()).unwrap();
    let before = controls(&h);
    assert_eq!(recovery.confirm(&plan, plan.hash()).unwrap(), recovered);
    assert_eq!(controls(&h), before);
    assert_eq!(recovered.revision.get(), 1);
    assert_eq!(recovered.primary.primary_epoch.get(), 1);
}
#[test]
fn interrupted_promotion_requires_the_native_target_and_never_promotes_twice() {
    let h = setup("host-repair-promotion");
    let root = verified(h.root.path());
    let faults = Faults::armed();
    let writer = h.reopen(faults.clone());
    let host = HostCoordinator::bootstrap(&writer, &root, identity(1)).unwrap();
    let initial = host.inspect().unwrap();
    let op = OperationId::parse("op_00000001-0000-4000-8000-000000000001").unwrap();
    let transfer = initial.prepare_transfer(op, identity(2)).unwrap();
    host.apply(ControlChange::Quiesce {
        writer: &initial.primary,
        plan: &transfer,
        confirmed_hash: &transfer.plan_hash,
    })
    .unwrap();
    drop(host);
    let target = HostCoordinator::open(&writer, &root, identity(2)).unwrap();
    faults.arm(FaultPoint::BeforeCurrent, 1, FaultAction::Fail(112));
    is_host(
        target
            .apply(ControlChange::Promote {
                target: &identity(2),
                plan: &transfer,
                confirmed_hash: &transfer.plan_hash,
                verified_target: &initial.checkpoint,
            })
            .unwrap_err(),
        HostFault::OutcomeUnknown,
    );
    drop(target);
    let old = HostRecovery::open(&writer, &root, identity(1)).unwrap();
    is_host(old.plan().unwrap_err(), HostFault::Fenced);
    drop(old);
    let recovery = HostRecovery::open(&writer, &root, identity(2)).unwrap();
    let plan = recovery.plan().unwrap();
    let state = recovery.confirm(&plan, plan.hash()).unwrap();
    assert_eq!(state.primary.primary_epoch.get(), 2);
    assert_eq!(state.checkpoint, initial.checkpoint);
    assert!(matches!(state.phase, AuthorityPhase::Active));
    assert_eq!(recovery.confirm(&plan, plan.hash()).unwrap(), state);
    drop(recovery);
    let old = HostCoordinator::open(&writer, &root, identity(1)).unwrap();
    is_host(
        old.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Fenced,
    );
    drop(old);
    let current = HostCoordinator::open(&writer, &root, identity(2)).unwrap();
    h.clock.set(h.lifecycle.time(1));
    assert!(matches!(
        current.commit(h.lifecycle.request(1)).unwrap(),
        CommitOutcome::Committed { .. }
    ));
}

#[test]
fn prototype_v1_roots_remain_fenced_without_silent_upgrade_or_reinitialization() {
    let h = setup("host-repair-v1-refusal");
    let root = verified(h.root.path());
    drop(HostCoordinator::bootstrap(&h.vault, &root, identity(1)).unwrap());
    let mode = std::fs::read_to_string(root.path().join(MODE_FILE))
        .unwrap()
        .replace("host-current-v2", "host-current-v1");
    std::fs::write(root.path().join(MODE_FILE), mode).unwrap();
    let unsupported = HostRecovery::open(&h.vault, &root, identity(1)).unwrap();
    is_host(unsupported.plan().unwrap_err(), HostFault::InvalidState);
    drop(unsupported);
    std::fs::remove_file(root.path().join(INTENT_FILE)).unwrap();
    std::fs::remove_file(root.path().join(WITNESS_FILE)).unwrap();
    let before = controls(&h);
    assert!(HostCoordinator::open(&h.vault, &root, identity(1)).is_err());
    let recovery = HostRecovery::open(&h.vault, &root, identity(1)).unwrap();
    is_host(recovery.plan().unwrap_err(), HostFault::Unavailable);
    assert_eq!(controls(&h), before);
    is_host(
        h.vault.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Fenced,
    );
}

#[test]
fn bootstrap_partial_new_metadata_recovers_only_the_exact_intent_bytes() {
    for nth in [2, 3] {
        let h = setup(&format!("host-repair-partial-metadata-{nth}"));
        let root = verified(h.root.path());
        let faults = Faults::armed();
        let writer = h.reopen(faults.clone());
        faults.arm(FaultPoint::WriteBytes, nth, FaultAction::Fail(112));
        is_host(
            HostCoordinator::bootstrap(&writer, &root, identity(1))
                .err()
                .unwrap(),
            HostFault::OutcomeUnknown,
        );
        let recovery = HostRecovery::open(&writer, &root, identity(1)).unwrap();
        let plan = recovery.plan().unwrap();
        assert!(plan.repair_needed());
        let changed = if nth == 2 {
            root.path().join(MODE_FILE)
        } else {
            let intent: serde_json::Value =
                serde_json::from_slice(&std::fs::read(root.path().join(INTENT_FILE)).unwrap())
                    .unwrap();
            root.path().join(format!(
                "vault/host-authority/{}.json",
                intent["after"]["host_authority_hash"].as_str().unwrap()
            ))
        };
        std::fs::write(&changed, b"{changed-partial-metadata}").unwrap();
        is_host(
            recovery.confirm(&plan, plan.hash()).unwrap_err(),
            HostFault::Conflict,
        );
        let fresh = recovery.plan().unwrap();
        assert_ne!(fresh.hash(), plan.hash());
        let fixed = recovery.confirm(&fresh, fresh.hash()).unwrap();
        assert_eq!(fixed.revision.get(), 1);
        assert!(!recovery.plan().unwrap().repair_needed());
        drop(recovery);
        assert_eq!(
            HostCoordinator::open(&writer, &root, identity(1))
                .unwrap()
                .inspect()
                .unwrap(),
            fixed
        );
    }
}

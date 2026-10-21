//! Native domain-writer binding over synthetic Windows roots only.
#![cfg(windows)]
mod support;

use enouia_memory_contract::common::{Sensitivity, TimePrecision, TrustedSurface};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::host::publication::{PublicationIntent, PublicationWitness};
use enouia_memory_contract::host::{AuthorityState, HostFault};
use enouia_memory_contract::ids::{DeviceId, OperationId, PolicyId};
use enouia_memory_contract::json::{SchemaVersion, canonical_bytes};
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_contract::store::CurrentPointer;
use enouia_memory_vault::backup::export_pinned;
use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
use enouia_memory_vault::host_recovery::{HostRecovery, INTENT_FILE, WITNESS_FILE};
use enouia_memory_vault::host_transaction::HostCoordinator;
use enouia_memory_vault::service::ManualAssertionInput;
use enouia_memory_vault::{Fault, VaultError};
use support::{Harness, TempRoot, verified};

fn identity(n: u32) -> DeviceId {
    DeviceId::parse(&format!("dev_{n:08x}-0000-4000-8000-000000000001")).unwrap()
}
fn setup(name: &str) -> Harness {
    let h = Harness::with_commits(name, 0);
    enouia_memory_vault::platform::restrict_to_owner(h.root.path()).unwrap();
    h
}
fn adopt(h: &Harness) {
    drop(HostCoordinator::bootstrap(&h.vault, &verified(h.root.path()), identity(1)).unwrap());
}
fn is_host(e: VaultError, want: HostFault) {
    assert_eq!(e.fault, Fault::Host(want));
}
fn assertion(h: &Harness) -> ManualAssertionInput {
    ManualAssertionInput {
        text: "Synthetic owner statement for native Host domain writes".into(),
        operator: h.lifecycle.owner(),
        trusted_surface: TrustedSurface::TrustedWindowsApp,
        confirmation: ConfirmationMethod::TypedConfirmation,
        sensitivity: Sensitivity::Private,
        access_policy_id: PolicyId::parse("pol_00000001-0000-4000-8000-000000000001").unwrap(),
        time_precision: TimePrecision::Millisecond,
    }
}

#[test]
fn binding_requires_explicit_v2_adoption_native_primary_and_exclusive_lease() {
    let h = setup("host-writer-binding");
    let root = verified(h.root.path());
    let before = std::fs::read(root.path().join("vault/CURRENT")).unwrap();
    assert!(
        h.reopen(Faults::none())
            .into_host_writer(&root, identity(1))
            .is_err()
    );
    assert_eq!(
        std::fs::read(root.path().join("vault/CURRENT")).unwrap(),
        before
    );
    adopt(&h);
    is_host(
        h.reopen(Faults::none())
            .into_host_writer(&root, identity(2))
            .err()
            .unwrap(),
        HostFault::Fenced,
    );
    let writer = h
        .reopen(Faults::none())
        .into_host_writer(&root, identity(1))
        .unwrap();
    assert!(writer.host_writer_state().is_ok());
    is_host(
        HostCoordinator::open(&h.vault, &root, identity(1))
            .err()
            .unwrap(),
        HostFault::Busy,
    );
    drop(writer);
    assert!(HostCoordinator::open(&h.vault, &root, identity(1)).is_ok());
}

#[test]
fn domain_commit_path_publishes_one_authority_per_lifecycle_commit() {
    let h = setup("host-writer-lifecycle");
    adopt(&h);
    let root = verified(h.root.path());
    let writer = h
        .reopen(Faults::none())
        .into_host_writer(&root, identity(1))
        .unwrap();
    for n in 1..h.lifecycle.commits.len() {
        h.clock.set(h.lifecycle.time(n));
        assert!(matches!(
            writer.commit(h.lifecycle.request(n)).unwrap(),
            CommitOutcome::Committed { .. }
        ));
        let state = writer.host_writer_state().unwrap();
        assert_eq!(state.revision.get(), n as u64 + 1);
        assert_eq!(state.checkpoint, writer.verified_data_checkpoint().unwrap());
        assert_eq!(
            writer.pin_current().unwrap().commit_id,
            state.checkpoint.commit_id
        );
    }
    is_host(
        h.vault.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Fenced,
    );
    // Binding grants normal domain commits, never the lossy legacy maintenance APIs.
    is_host(writer.sweep_unreferenced().unwrap_err(), HostFault::Fenced);
    is_host(writer.purge_files().unwrap_err(), HostFault::Fenced);
    let target = TempRoot::new("host-writer-no-lossy-export");
    is_host(
        export_pinned(
            &writer,
            &writer.pin_current().unwrap(),
            &verified(target.path()),
        )
        .unwrap_err(),
        HostFault::Fenced,
    );
}

#[test]
fn source_service_replays_after_reopen_but_not_after_witness_corruption() {
    let h = setup("host-writer-source");
    adopt(&h);
    let root = verified(h.root.path());
    let writer = h
        .reopen(Faults::none())
        .into_host_writer(&root, identity(1))
        .unwrap();
    h.clock.set(h.lifecycle.time(1));
    let input = assertion(&h);
    let written = writer
        .record_manual_assertion(&input, b"host-domain-source")
        .unwrap();
    assert!(matches!(written.outcome, CommitOutcome::Committed { .. }));
    assert_eq!(writer.host_writer_state().unwrap().revision.get(), 2);
    drop(writer);
    let writer = h
        .reopen(Faults::none())
        .into_host_writer(&root, identity(1))
        .unwrap();
    let replay = writer
        .record_manual_assertion(&input, b"host-domain-source")
        .unwrap();
    assert_eq!(replay.id, written.id);
    assert!(matches!(replay.outcome, CommitOutcome::Replayed { .. }));
    let before = std::fs::read(root.path().join("vault/CURRENT")).unwrap();
    std::fs::write(
        root.path().join(WITNESS_FILE),
        b"{synthetic corrupt witness}",
    )
    .unwrap();
    assert!(writer.pin_current().is_err());
    assert!(
        writer
            .record_manual_assertion(&input, b"host-domain-source")
            .is_err(),
        "receipt-only service replay must still revalidate authority"
    );
    assert!(writer.commit(h.lifecycle.request(1)).is_err());
    assert_eq!(
        std::fs::read(root.path().join("vault/CURRENT")).unwrap(),
        before
    );
}

#[test]
fn interrupted_domain_publication_requires_explicit_recovery_before_original_replay() {
    let h = setup("host-writer-lost-response");
    adopt(&h);
    let root = verified(h.root.path());
    let faults = Faults::armed();
    let writer = h
        .reopen(faults.clone())
        .into_host_writer(&root, identity(1))
        .unwrap();
    h.clock.set(h.lifecycle.time(1));
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(112));
    is_host(
        writer.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::OutcomeUnknown,
    );
    assert!(writer.commit(h.lifecycle.request(1)).is_err());
    assert!(writer.pin_current().is_err());
    drop(writer);
    let recovery = HostRecovery::open(&h.vault, &root, identity(1)).unwrap();
    let plan = recovery.plan().unwrap();
    recovery.confirm(&plan, plan.hash()).unwrap();
    drop(recovery);
    let writer = h
        .reopen(Faults::none())
        .into_host_writer(&root, identity(1))
        .unwrap();
    assert!(matches!(
        writer.commit(h.lifecycle.request(1)).unwrap(),
        CommitOutcome::Replayed { .. }
    ));
    assert_eq!(writer.host_writer_state().unwrap().revision.get(), 2);
}

// Deliberately bypass native lease enforcement inside this synthetic fixture to
// exercise persistent freshness after a complete valid A -> B -> A transition.
fn replace_control(h: &Harness, after_state: &AuthorityState) {
    let root = h.root.path();
    let before: CurrentPointer =
        serde_json::from_slice(&std::fs::read(root.join("vault/CURRENT")).unwrap()).unwrap();
    let bytes = canonical_bytes(after_state).unwrap();
    let hash = sha256(&bytes);
    let after = CurrentPointer {
        host_authority_hash: Some(hash.clone()),
        ..before.clone()
    };
    std::fs::write(
        root.join(format!("vault/host-authority/{hash}.json")),
        bytes,
    )
    .unwrap();
    std::fs::write(
        root.join(INTENT_FILE),
        canonical_bytes(&PublicationIntent {
            schema_version: SchemaVersion,
            before,
            after: after.clone(),
            authority: after_state.clone(),
        })
        .unwrap(),
    )
    .unwrap();
    std::fs::write(root.join("vault/CURRENT"), canonical_bytes(&after).unwrap()).unwrap();
    std::fs::write(
        root.join(WITNESS_FILE),
        canonical_bytes(&PublicationWitness {
            schema_version: SchemaVersion,
            current: after,
        })
        .unwrap(),
    )
    .unwrap();
}

#[test]
fn captured_primary_epoch_is_not_refreshed_when_the_same_identity_returns() {
    let h = setup("host-writer-stale-epoch");
    adopt(&h);
    let root = verified(h.root.path());
    let writer = h
        .reopen(Faults::none())
        .into_host_writer(&root, identity(1))
        .unwrap();
    let mut state = writer.host_writer_state().unwrap();
    for n in [2, 1] {
        let op = OperationId::parse(&format!("op_{n:08x}-0000-4000-8000-000000000001")).unwrap();
        let plan = state.prepare_transfer(op, identity(n)).unwrap();
        state = state
            .quiesce(&state.primary, &plan, &plan.plan_hash)
            .unwrap();
        replace_control(&h, &state);
        state = state
            .promote(&identity(n), &plan, &plan.plan_hash, &state.checkpoint)
            .unwrap();
        replace_control(&h, &state);
    }
    assert_eq!(state.primary.host_id, identity(1));
    assert_eq!(state.primary.primary_epoch.get(), 3);
    is_host(writer.host_writer_state().unwrap_err(), HostFault::Fenced);
    is_host(writer.pin_current().unwrap_err(), HostFault::Fenced);
    is_host(
        writer.commit(h.lifecycle.request(1)).unwrap_err(),
        HostFault::Fenced,
    );
    drop(writer);
    let rebound = h
        .reopen(Faults::none())
        .into_host_writer(&root, identity(1))
        .unwrap();
    assert_eq!(rebound.host_writer_state().unwrap(), state);
}

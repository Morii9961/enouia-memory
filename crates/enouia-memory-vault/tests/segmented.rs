#![cfg(windows)]
#[path = "support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, hash::sha256, json::Revision, next::*};
use enouia_memory_vault::{
    OsIdSource,
    automation::{AutomationFault, AutomationVault, segmented::SegmentedAutomationVault},
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;
use std::sync::Arc;
#[test]
fn segmented_root_exceeds_old_capacity_reopens_and_never_silently_upgrades() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let receipts = e.populate(&v, 160);
    assert_eq!(
        v.read(
            &e.descriptor.system_actor,
            Sensitivity::HighlySensitive,
            |r| r.memories.len()
        )
        .unwrap(),
        160
    );
    let before = v.snapshot().unwrap();
    drop(v);
    let v = e.open();
    assert_eq!(v.snapshot().unwrap(), before);
    let observed = v
        .read(
            &e.descriptor.system_actor,
            Sensitivity::HighlySensitive,
            |r| {
                r.memories
                    .iter()
                    .map(|m| m.receipt.clone())
                    .collect::<Vec<_>>()
            },
        )
        .unwrap();
    assert!(receipts.iter().all(|r| observed.contains(r)));
    let public = v
        .read(&e.descriptor.system_actor, Sensitivity::Public, |r| {
            r.memories.len()
        })
        .unwrap();
    assert_eq!(public, 160 - 160 / 7);
    let id = receipts[0].result.memory_id.clone();
    v.delete(&e.descriptor.created_by, vec![], vec![id.clone()])
        .unwrap();
    assert!(
        !v.read(
            &e.descriptor.system_actor,
            Sensitivity::HighlySensitive,
            |r| r.memories.iter().any(|m| m.memory.memory.memory_id == id)
        )
        .unwrap()
    );
    drop(v);
    assert!(
        AutomationVault::open(
            &e.root,
            e.descriptor.host_id.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    let v = e.open();
    assert_eq!(
        v.read(
            &e.descriptor.system_actor,
            Sensitivity::HighlySensitive,
            |r| r.memories.len()
        )
        .unwrap(),
        159
    );
}
#[test]
fn publication_faults_recover_only_exact_forward_intent_and_corrupt_segments_refuse() {
    for point in [
        FaultPoint::StagingWritten,
        FaultPoint::HostIntentPublished,
        FaultPoint::BeforeCurrent,
        FaultPoint::AfterCurrent,
        FaultPoint::HostWitnessPublished,
    ] {
        let e = Env::new();
        let faults = Faults::armed();
        let v = e.create(faults.clone());
        let before = v.snapshot().unwrap();
        faults.arm(point, 1, FaultAction::Fail(112));
        assert!(matches!(
            v.set_policy(&e.descriptor.created_by, e.policy(1, 1, 1)),
            Err(AutomationFault::OutcomeUnknown)
        ));
        assert!(v.snapshot().is_err());
        drop(v);
        let recovered = SegmentedAutomationVault::recover(
            &e.root,
            e.descriptor.host_id.clone(),
            &e.descriptor.system_actor,
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap();
        if point == FaultPoint::StagingWritten {
            assert_eq!(recovered.snapshot().unwrap(), before);
        } else {
            assert_ne!(recovered.snapshot().unwrap(), before);
        }
        drop(recovered);
        let pointer: AutomationCurrent =
            serde_json::from_slice(&std::fs::read(e.path.join("vault/CURRENT")).unwrap()).unwrap();
        std::fs::write(
            e.path.join(format!(
                "vault/automation-segments/{}.json",
                pointer.manifest_sha256
            )),
            b"{}",
        )
        .unwrap();
        assert!(
            SegmentedAutomationVault::open(
                &e.root,
                e.descriptor.host_id.clone(),
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none()
            )
            .is_err()
        );
    }
}
#[test]
fn machine_cannot_modify_policy_delete_or_ledger_and_bad_generation_never_advances() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let p = e.policy(1, 1, 1);
    assert!(v.set_policy(&e.descriptor.system_actor, p.clone()).is_err());
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let source = e.source(1, Sensitivity::Private);
    v.capture(
        &e.descriptor.system_actor,
        vec![source.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let input = v
        .prepare(&e.descriptor.system_actor, e.input(&source, &p, 1))
        .unwrap();
    let before = v.snapshot().unwrap();
    let mut decision = v.evaluate(&e.descriptor.system_actor, &input).unwrap();
    decision.input.record_fields.as_mut().unwrap().evidence[0].quote =
        decision.input.record_fields.as_ref().unwrap().evidence[0]
            .quote
            .replace("moonlight", "sunlights");
    let response = serde_json::to_string(&decision).unwrap();
    let b = GenerationBinding {
        operation_id: input.operation_id.clone(),
        input_hash: digest(&input),
        response_hash: sha256(response.as_bytes()),
    };
    assert!(
        v.apply_batch(
            &e.descriptor.system_actor,
            vec![input],
            vec![response],
            vec![b]
        )
        .is_err()
    );
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(
        v.delete(
            &e.descriptor.system_actor,
            vec![source.record.source_id],
            vec![]
        )
        .is_err()
    );
    let mut revoked = p;
    revoked.revision = Revision::new(2).unwrap();
    revoked.active = false;
    v.set_policy(&e.descriptor.created_by, revoked).unwrap();
    assert_eq!(
        v.read(
            &e.descriptor.system_actor,
            Sensitivity::HighlySensitive,
            |r| r.memories.len()
        )
        .unwrap(),
        0
    );
}

#[test]
fn new_catalog_cannot_quote_a_revision_replaced_in_the_same_capture() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let p = e.policy(1, 1, 1);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let source = e.source(1, Sensitivity::Private);
    v.capture(
        &e.descriptor.system_actor,
        vec![source.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let mut entity: RecordCatalog =
        serde_json::from_value(fixture::value("ui-records/catalog.json")).unwrap();
    entity.conversations.clear();
    entity.entities.truncate(1);
    let item = &mut entity.entities[0];
    item.sensitivity = Sensitivity::Private;
    item.access_policy_id = source.record.access_policy_id.clone();
    item.aliases.clear();
    item.evidence = e.input(&source, &p, 1).record_fields.unwrap().evidence;
    let mut revised = source;
    revised.record.revision = Revision::new(2).unwrap();
    let before = v.snapshot().unwrap();
    assert!(
        v.capture(&e.descriptor.system_actor, vec![revised], entity)
            .is_err()
    );
    assert_eq!(v.snapshot().unwrap(), before);
}

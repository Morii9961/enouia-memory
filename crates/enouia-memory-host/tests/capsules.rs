#![cfg(windows)]
//! ADR-MEM-173: Morii's saved recall capsules on a registered (segmented) Vault.
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, next::*};
use enouia_memory_host::automation::owner::NativeMemoryOwner;
use enouia_memory_vault::{automation::segmented::NativeOwnerPreview, fault::Faults};
use fixture::*;
use std::sync::Arc;

fn execute(h: &NativeMemoryOwner, p: NativeOwnerPreview) {
    let t = h
        .confirm_from_owner_gesture(
            &p.plan.operation_id,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    h.commit(t).unwrap();
}

#[test]
fn capsules_save_replace_follow_deletion_and_discard() {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 6);
    let owner = e.descriptor.created_by.clone();
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), owner).unwrap();
    let targets: std::collections::BTreeMap<_, _> = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.memories
                .iter()
                .map(|m| (m.target.memory.memory_id.clone(), m.target.clone()))
                .collect()
        })
        .unwrap();
    let target = |n: usize| targets[&receipts[n].result.memory_id].clone();
    let save = |name: &str, query: &str, members: Vec<TargetRef>| {
        h.plan(
            OwnerAction::Capsule {
                name: name.into(),
                query: query.into(),
                members,
            },
            Sensitivity::Private,
            None,
        )
    };
    assert!(
        h.capsules(Sensitivity::Private)
            .unwrap()
            .capsules
            .is_empty()
    );
    // Invalid names and duplicate members are refused at planning.
    assert!(save("", "walks", vec![target(0)]).is_err());
    assert!(save(&"x".repeat(61), "walks", vec![target(0)]).is_err());
    assert!(save("walks", "walks", vec![target(0), target(0)]).is_err());
    execute(
        &h,
        save("walks", "night walks", vec![target(2), target(0)]).unwrap(),
    );
    execute(&h, save("tea", "tea", vec![target(1)]).unwrap());
    let listed = h.capsules(Sensitivity::Private).unwrap();
    let names: Vec<_> = listed.capsules.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, vec!["tea", "walks"]);
    let walks = &listed.capsules[1];
    assert_eq!(walks.query, "night walks");
    assert_eq!(
        walks
            .members
            .iter()
            .map(|m| m.saved.clone())
            .collect::<Vec<_>>(),
        vec![target(2), target(0)],
        "order is kept"
    );
    assert!(
        walks
            .members
            .iter()
            .all(|m| m.current.as_ref() == Some(&m.saved))
    );
    assert_eq!(walks.saved_by.action, "capsule");
    // Saving the same name replaces it.
    execute(&h, save("walks", "walks again", vec![target(3)]).unwrap());
    let listed = h.capsules(Sensitivity::Private).unwrap();
    assert_eq!(listed.capsules.len(), 2);
    assert_eq!(listed.capsules[1].query, "walks again");
    assert_eq!(listed.capsules[1].members.len(), 1);
    // A deleted member stays listed as saved, with no current revision.
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipts[3].result.memory_id.clone()],
    )
    .unwrap();
    let listed = h.capsules(Sensitivity::Private).unwrap();
    assert_eq!(listed.capsules[1].members[0].current, None);
    // No members discards it.
    execute(&h, save("tea", "", vec![]).unwrap());
    let listed = h.capsules(Sensitivity::Private).unwrap();
    assert_eq!(
        listed
            .capsules
            .iter()
            .map(|c| c.name.as_str())
            .collect::<Vec<_>>(),
        vec!["walks"]
    );
}

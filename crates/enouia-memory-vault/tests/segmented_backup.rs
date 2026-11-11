#![cfg(any(windows, target_os = "linux"))]
#[path = "support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::Sensitivity, hash::sha256, json::canonical_bytes, next::*};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, VerifiedRoot,
    automation::{
        AutomationFault,
        segmented::{SegmentedAutomationVault, backup::*},
    },
    fault::Faults,
    fs::ManagedRoot,
    verify_data_root,
};
use fixture::*;
use std::sync::Arc;

fn owner() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    e
}
fn dir(e: &Env, name: &str) -> VerifiedRoot {
    let path = e.path.join(name);
    std::fs::create_dir(&path).unwrap();
    verify_data_root(&path, &RootPolicy::default()).unwrap()
}
fn export(e: &Env, out: &VerifiedRoot) -> Result<OwnerExport, AutomationFault> {
    export_owner(&e.root, out, e.clock.clone(), Arc::new(OsIdSource))
}

#[test]
fn independent_round_trip_preserves_history_objects_journals_and_deletion_barriers() {
    let e = owner();
    let vault = e.create(Faults::none());
    let receipts = e.populate(&vault, 3);
    vault
        .delete(
            &e.descriptor.created_by,
            vec![],
            vec![receipts[0].result.memory_id.clone()],
        )
        .unwrap();
    let before = vault.snapshot().unwrap();
    let raw = b"synthetic unsupported original and attachment";
    vault
        .archive_import_objects(
            &e.descriptor.system_actor,
            &std::collections::BTreeMap::from([(sha256(raw), raw.to_vec())]),
        )
        .unwrap();
    let source = ManagedRoot::new(e.root.path(), Faults::none());
    let journal = br#"{"synthetic_unknown_outcome":"must never resend"}"#;
    source
        .write_new("sync-state/initialization-owner-1.json", journal)
        .unwrap();
    source
        .write_new("sync-state/captured-formation-1.json", journal)
        .unwrap();
    drop(vault);
    let output = dir(&e, "export");
    let target = dir(&e, "restored");
    // Export destinations cannot be nested under the source. Use sibling roots.
    assert!(export(&e, &output).is_err());
    let independent = owner();
    let restored = owner();
    let manifest = export(&e, &independent.root).unwrap();
    assert!(!manifest.files.iter().any(|f| f.path.ends_with(".lock")));
    // Make the original unavailable at its old path; verification has no source.
    let moved = e.path.with_extension("offline");
    std::fs::rename(&e.path, &moved).unwrap();
    assert_eq!(verify_owner(&independent.root).unwrap(), manifest);
    restore_owner(&independent.root, &restored.root, e.clock.as_ref()).unwrap();
    assert!(
        SegmentedAutomationVault::open(
            &restored.root,
            e.descriptor.host_id.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    assert!(
        SegmentedAutomationVault::recover(
            &restored.root,
            e.descriptor.host_id.clone(),
            &e.descriptor.system_actor,
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    activate_owner(&restored.root, &independent.root, e.clock.as_ref()).unwrap();
    let vault = SegmentedAutomationVault::open(
        &restored.root,
        e.descriptor.host_id.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
        Faults::none(),
    )
    .unwrap();
    assert_eq!(vault.snapshot().unwrap(), before);
    let observed = vault
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
    assert_eq!(observed.len(), 2);
    assert!(!observed.contains(&receipts[0]));
    assert!(observed.contains(&receipts[1]) && observed.contains(&receipts[2]));
    assert_eq!(
        std::fs::read(
            restored
                .path
                .join(enouia_memory_contract::layout::raw_object(&sha256(raw)))
        )
        .unwrap(),
        raw
    );
    for name in ["initialization-owner-1.json", "captured-formation-1.json"] {
        assert_eq!(
            std::fs::read(restored.path.join("sync-state").join(name)).unwrap(),
            journal
        );
    }
    drop(vault);
    // Restore the fixture path for its narrowly owned cleanup; never delete data.
    std::fs::rename(moved, &e.path).unwrap();
    assert!(output.path().exists() && target.path().exists());
}

#[test]
fn busy_nonempty_unknown_journal_and_old_layout_are_refused() {
    let e = owner();
    let out = owner();
    let vault = e.create(Faults::none());
    assert!(matches!(export(&e, &out.root), Err(AutomationFault::Busy)));
    assert!(std::fs::read_dir(out.root.path()).unwrap().next().is_none());
    drop(vault);
    let source = ManagedRoot::new(e.root.path(), Faults::none());
    let lease = source
        .open_lock_file("sync-state/initialization-owner-1.lock")
        .unwrap();
    lease.try_lock().unwrap();
    assert!(matches!(export(&e, &out.root), Err(AutomationFault::Busy)));
    drop(lease);
    source.write_new("sync-state/unknown.json", b"{}").unwrap();
    assert!(export(&e, &out.root).is_err());
    let old = Env::new();
    drop(old.create(Faults::none()));
    assert!(export(&old, &out.root).is_err());
    let fresh = owner();
    drop(fresh.create(Faults::none()));
    let manifest = export(&fresh, &out.root).unwrap();
    assert!(export(&fresh, &out.root).is_err());
    assert!(restore_owner(&out.root, &fresh.root, fresh.clock.as_ref()).is_err());
    assert_eq!(verify_owner(&out.root).unwrap(), manifest);
}

#[test]
fn corrupt_extra_missing_and_unsafe_files_never_touch_restore_target() {
    let e = owner();
    drop(e.create(Faults::none()));
    for case in ["corrupt", "extra", "missing", "unsafe", "descriptor"] {
        let out = owner();
        let target = owner();
        let mut manifest = export(&e, &out.root).unwrap();
        match case {
            "corrupt" => {
                std::fs::write(out.path.join(&manifest.files[0].path), b"damaged").unwrap()
            }
            "extra" => std::fs::write(out.path.join("unexpected.txt"), b"not listed").unwrap(),
            "missing" => {
                // A self-consistent file list still cannot omit a chain segment.
                let index = manifest
                    .files
                    .iter()
                    .position(|f| f.path.starts_with("vault/automation-segments/"))
                    .unwrap();
                let file = manifest.files.remove(index);
                std::fs::remove_file(out.path.join(file.path)).unwrap();
                std::fs::write(out.path.join(MANIFEST), canonical_bytes(&manifest).unwrap())
                    .unwrap();
            }
            "unsafe" => {
                manifest.files[0].path = "vault/../escape".into();
                std::fs::write(out.path.join(MANIFEST), canonical_bytes(&manifest).unwrap())
                    .unwrap();
            }
            "descriptor" => {
                // Rehashing a changed descriptor cannot forge its genesis binding.
                let path = "vault/vault.json";
                let mut descriptor = e.descriptor.clone();
                descriptor.owner_subject_id =
                    Some(enouia_memory_contract::ids::SubjectId::parse(&id("sub", 900)).unwrap());
                let bytes = canonical_bytes(&descriptor).unwrap();
                std::fs::write(out.path.join(path), &bytes).unwrap();
                let file = manifest.files.iter_mut().find(|f| f.path == path).unwrap();
                file.sha256 = sha256(&bytes);
                file.size_bytes = bytes.len() as u64;
                manifest.descriptor_hash = digest(&descriptor);
                std::fs::write(out.path.join(MANIFEST), canonical_bytes(&manifest).unwrap())
                    .unwrap();
            }
            _ => unreachable!(),
        }
        assert!(
            restore_owner(&out.root, &target.root, e.clock.as_ref()).is_err(),
            "{case}"
        );
        assert!(
            std::fs::read_dir(target.path.clone())
                .unwrap()
                .next()
                .is_none(),
            "{case}"
        );
    }
}

#[test]
fn activation_requires_the_exact_current_backup_and_unchanged_restored_files() {
    let e = owner();
    let vault = e.create(Faults::none());
    e.populate(&vault, 1);
    drop(vault);
    let old = owner();
    export(&e, &old.root).unwrap();
    let target = owner();
    restore_owner(&old.root, &target.root, e.clock.as_ref()).unwrap();
    let vault = e.open();
    vault
        .delete(
            &e.descriptor.created_by,
            vec![e.source(1, Sensitivity::Public).record.source_id],
            vec![],
        )
        .unwrap();
    drop(vault);
    let current = owner();
    export(&e, &current.root).unwrap();
    assert!(activate_owner(&target.root, &current.root, e.clock.as_ref()).is_err());
    std::fs::write(target.path.join("vault/vault.json"), b"{}").unwrap();
    assert!(activate_owner(&target.root, &old.root, e.clock.as_ref()).is_err());
}

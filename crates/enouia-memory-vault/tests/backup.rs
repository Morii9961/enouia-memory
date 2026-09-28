//! MV-1.4 backup exit. B01 (plaintext part): a pinned-commit export restores
//! into an independent root after the original root is gone, with identical
//! manifests and records, rebuilt idempotency, and network disabled until
//! reconciliation. B03: damaged or incomplete exports are refused before the
//! target is touched, and a restore never overwrites a healthy Vault.
//! B04 (ACL part): the owner-only protected DACL entry point. D04: Activity
//! data beside the Vault is byte-identical after every Memory operation.
//!
//! Not covered here: encryption (restic is not installed; see restic.rs),
//! a different machine or account, and DPAPI independence of the secret.

mod support;

use enouia_memory_contract::commit::CommitManifest;
use enouia_memory_contract::common::TrustedSurface;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_contract::record::RecordRef;
use enouia_memory_contract::store::RecoveryEvidence;
use enouia_memory_vault::backup::{export_pinned, network_allowed, restore_export, verify_export};
use enouia_memory_vault::fault::Faults;
use enouia_memory_vault::platform::{AclPrincipal, inspect_acl, restrict_to_owner};
use enouia_memory_vault::{Fault, Vault};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::Path;
use support::{Harness, TempRoot, ids, options, verified};

fn tree(root: &Path) -> BTreeMap<String, String> {
    fn walk(dir: &Path, base: &Path, out: &mut BTreeMap<String, String>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, base, out);
            } else {
                let rel = path
                    .strip_prefix(base)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                out.insert(rel, sha256(&std::fs::read(&path).unwrap()).to_string());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk(root, root, &mut out);
    out
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

#[test]
fn a_pinned_export_restores_elsewhere_after_the_original_is_gone() {
    let harness = Harness::with_commits("backup-src", 16);
    let pin = harness.vault.pin_current().unwrap();
    let manifest = harness.vault.read_manifest(&pin).unwrap();
    let originals: Vec<(RecordRef, Vec<u8>)> = manifest
        .catalog
        .iter()
        .map(|e| {
            let r = RecordRef::new(e.record_kind, &e.record_id, e.revision);
            let bytes = harness.vault.read_record(&pin, &r).unwrap();
            (r, bytes)
        })
        .collect();
    let export_root = TempRoot::new("backup-export");
    let export = export_pinned(&harness.vault, &pin, &verified(export_root.path())).unwrap();
    assert_eq!(export.commit_id, pin.commit_id);
    assert!(export.files.iter().all(|f| !f.path.ends_with("CURRENT")));
    assert!(
        export
            .files
            .iter()
            .all(|f| !f.path.contains("/idempotency/"))
    );
    assert_eq!(verify_export(export_root.path()).unwrap(), export);

    // The original root, its caches, and its journal disappear entirely.
    let original = harness.root.path().to_path_buf();
    std::fs::remove_dir_all(&original).unwrap();
    assert!(!original.exists());

    let target = TempRoot::new("backup-restored");
    harness.clock.set(harness.lifecycle.time(16) + 1_000);
    let restored = restore_export(
        export_root.path(),
        &verified(target.path()),
        &harness.lifecycle.owner(),
        TrustedSurface::TrustedLocalCli,
        harness.clock.clone(),
        ids(),
        options(Faults::none()),
    )
    .unwrap();
    assert_eq!(restored.receipt.evidence, RecoveryEvidence::RestoredExport);
    assert!(restored.state.network_disabled_until_reconciled);
    assert!(!network_allowed(&restored.vault).unwrap());
    let vault = restored.vault;
    assert_eq!(vault.pin_current().unwrap(), pin);
    assert_eq!(vault.read_manifest(&pin).unwrap(), manifest);
    for (reference, bytes) in &originals {
        assert_eq!(
            &vault.read_record(&pin, reference).unwrap(),
            bytes,
            "{reference:?}"
        );
    }
    // Idempotency survived: the last applied request replays; the next one
    // commits and still matches the frozen lifecycle.
    harness.clock.set(harness.lifecycle.time(17));
    let replay = vault.commit(harness.lifecycle.request(16)).unwrap();
    assert!(matches!(replay, CommitOutcome::Replayed { .. }));
    assert!(matches!(
        vault.commit(harness.lifecycle.request(17)).unwrap(),
        CommitOutcome::Committed { .. }
    ));
    let head = vault.read_manifest(&vault.pin_current().unwrap()).unwrap();
    let fixture: CommitManifest =
        serde_json::from_value(harness.lifecycle.commits[17].clone()).unwrap();
    assert_eq!(without_operation(&head), without_operation(&fixture));
}

fn restore_into(harness: &Harness, export: &Path, target: &Path) -> Result<Vault, Fault> {
    restore_export(
        export,
        &verified(target),
        &harness.lifecycle.owner(),
        TrustedSurface::TrustedLocalCli,
        harness.clock.clone(),
        ids(),
        options(Faults::none()),
    )
    .map(|r| r.vault)
    .map_err(|e| e.fault)
}

#[test]
fn damaged_or_incomplete_exports_are_refused_before_the_target_is_touched() {
    let harness = Harness::with_commits("backup-bad", 12);
    let pin = harness.vault.pin_current().unwrap();
    type Damage = fn(&Path);
    let cases: [(&str, Damage); 4] = [
        ("flipped byte", |dir| {
            let file = walk_first(dir, "vault/records/memory/");
            let mut bytes = std::fs::read(&file).unwrap();
            bytes[10] ^= 0x20;
            std::fs::write(&file, bytes).unwrap();
        }),
        ("missing record", |dir| {
            std::fs::remove_file(walk_first(dir, "vault/records/review/")).unwrap();
        }),
        ("unlisted file", |dir| {
            std::fs::write(dir.join("vault/records/extra.json"), b"{}").unwrap();
        }),
        ("edited manifest", |dir| {
            let path = dir.join("export-manifest.json");
            let text = std::fs::read_to_string(&path).unwrap();
            std::fs::write(
                &path,
                text.replacen("\"sequence\": 13", "\"sequence\": 12", 1),
            )
            .unwrap();
        }),
    ];
    for (name, damage) in cases {
        let export_root = TempRoot::new("backup-bad-export");
        export_pinned(&harness.vault, &pin, &verified(export_root.path())).unwrap();
        damage(export_root.path());
        assert!(verify_export(export_root.path()).is_err(), "{name}");
        let target = TempRoot::new("backup-bad-target");
        assert!(
            matches!(
                restore_into(&harness, export_root.path(), target.path()),
                Err(Fault::Corrupt(_))
            ),
            "{name}"
        );
        assert_eq!(
            std::fs::read_dir(target.path()).unwrap().count(),
            0,
            "{name}: target untouched"
        );
    }
}

fn walk_first(dir: &Path, prefix: &str) -> std::path::PathBuf {
    tree(dir)
        .keys()
        .find(|k| k.starts_with(prefix))
        .map(|k| dir.join(k))
        .unwrap_or_else(|| panic!("no file under {prefix}"))
}

#[test]
fn a_restore_never_overwrites_a_healthy_vault() {
    let harness = Harness::with_commits("backup-healthy", 8);
    let older = Harness::with_commits("backup-older", 3);
    let export_root = TempRoot::new("backup-older-export");
    let pin = older.vault.pin_current().unwrap();
    export_pinned(&older.vault, &pin, &verified(export_root.path())).unwrap();
    let before = tree(harness.root.path());
    assert_eq!(
        restore_into(&harness, export_root.path(), harness.root.path()).err(),
        Some(Fault::AlreadyInitialized)
    );
    assert_eq!(tree(harness.root.path()), before);
    assert_eq!(harness.vault.pin_current().unwrap().sequence, 9);
}

#[test]
fn exports_need_an_empty_separate_verified_destination() {
    let harness = Harness::with_commits("backup-dest", 2);
    let pin = harness.vault.pin_current().unwrap();
    let busy = TempRoot::new("backup-dest-busy");
    std::fs::write(busy.path().join("x"), b"x").unwrap();
    assert!(export_pinned(&harness.vault, &pin, &verified(busy.path())).is_err());
    let inside = harness.root.child("export-inside");
    assert!(export_pinned(&harness.vault, &pin, &verified(&inside)).is_err());
}

#[test]
fn the_owner_only_acl_entry_point_protects_the_root_and_its_children() {
    let root = TempRoot::new("acl");
    let before = inspect_acl(root.path()).unwrap();
    assert!(
        !before.protected,
        "a fresh temp directory inherits its parent's ACL"
    );
    restrict_to_owner(root.path()).unwrap();
    let after = inspect_acl(root.path()).unwrap();
    assert!(after.is_owner_only(), "{after:?}");
    assert_eq!(after.broad_grants(), 0);
    assert!(
        after
            .entries
            .iter()
            .any(|e| e.principal == AclPrincipal::CurrentUser && e.allow)
    );
    let child = root.path().join("vault");
    std::fs::create_dir(&child).unwrap();
    std::fs::write(child.join("f"), b"x").unwrap();
    let inherited = inspect_acl(&child.join("f")).unwrap();
    assert_eq!(inherited.broad_grants(), 0, "{inherited:?}");
    assert!(inherited.entries.iter().all(|e| e.inherited));
    // A Vault still works on the protected root.
    let _ = verified(root.path());
}

#[test]
fn activity_data_beside_the_vault_is_never_touched() {
    let parent = TempRoot::new("d04");
    let activity = parent.child("activity");
    std::fs::write(
        activity.join("activity.sqlite"),
        b"synthetic activity bytes",
    )
    .unwrap();
    std::fs::create_dir_all(activity.join("pending")).unwrap();
    std::fs::write(activity.join("pending/0001.json"), b"{\"sequence\":1}").unwrap();
    let before = tree(&activity);

    let data_root = parent.child("memory-data");
    let lifecycle = support::Lifecycle::load();
    let clock = std::sync::Arc::new(enouia_memory_contract::foundation::FakeClock::new(
        lifecycle.time(0),
    ));
    let vault = Vault::create(
        &verified(&data_root),
        lifecycle.genesis(),
        clock.clone(),
        ids(),
        options(Faults::none()),
    )
    .unwrap();
    for index in 1..lifecycle.commits.len() {
        clock.set(lifecycle.time(index));
        vault.commit(lifecycle.request(index)).unwrap();
    }
    let pin = vault.pin_current().unwrap();
    assert!(vault.verify(&pin).unwrap().is_clean());
    let export_root = parent.child("memory-export");
    export_pinned(&vault, &pin, &verified(&export_root)).unwrap();
    let restored_root = parent.child("memory-restored");
    restore_export(
        &export_root,
        &verified(&restored_root),
        &lifecycle.owner(),
        TrustedSurface::TrustedLocalCli,
        clock,
        ids(),
        options(Faults::none()),
    )
    .unwrap();
    let _ = vault.recovery_report().unwrap();
    let _ = vault.health();
    assert_eq!(
        tree(&activity),
        before,
        "Activity bytes and sequence files unchanged"
    );
    // The managed-path rules refuse Activity names outright.
    assert!(!enouia_memory_vault::fs::is_managed_path(
        "activity/activity.sqlite"
    ));
}

//! MV-2.4: post-import reconciliation (counts close, every cited revision is
//! re-derived from the archived bytes) and source location after a backup
//! export is restored into an independent root with the original removed.

mod support;

use enouia_memory_contract::common::TrustedSurface;
use enouia_memory_contract::import::ImportStatus;
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_import::pipeline::NeverCancel;
use enouia_memory_import::{audit_import, import_file};
use enouia_memory_vault::backup::{export_pinned, restore_export};
use enouia_memory_vault::{RootPolicy, VaultOptions, verify_data_root};
use serde_json::json;
use std::sync::Arc;
use support::{Env, Member, ZipOptions, sample_export, to_bytes, zip};

#[test]
fn counts_close_and_every_cited_revision_rederives() {
    let env = Env::new("audit");
    let first = import_file(
        &env.vault,
        &env.temp.file("a.json", &to_bytes(&sample_export())),
        &env.options,
        &NeverCancel,
    )
    .unwrap();
    let mut later = sample_export();
    later[1]["mapping"]["b1"]["message"]["content"]["parts"][0] = json!("（合成）改过的原问题。");
    env.tick();
    let second = import_file(
        &env.vault,
        &env.temp.file("b.json", &to_bytes(&later)),
        &env.options,
        &NeverCancel,
    )
    .unwrap();
    env.tick();
    let duplicate = import_file(
        &env.vault,
        &env.temp.file("c.json", &to_bytes(&later)),
        &env.options,
        &NeverCancel,
    )
    .unwrap();

    let a = audit_import(&env.vault, &first.import_id).unwrap();
    assert!(a.is_consistent(), "{a:?}");
    assert_eq!(
        (a.sources_citing, a.coverage_total, a.revisions_checked),
        (10, 10, 10)
    );
    let b = audit_import(&env.vault, &second.import_id).unwrap();
    assert!(b.is_consistent(), "{b:?}");
    assert_eq!(
        b.sources_citing, 1,
        "only the edited message cites the second import"
    );
    let d = audit_import(&env.vault, &duplicate.import_id).unwrap();
    assert!(d.is_consistent());
    assert_eq!(d.sources_citing, 0);
}

#[test]
fn an_unsupported_import_reconciles_as_archived_only() {
    let env = Env::new("audit-partial");
    let bytes = zip(
        &[Member::new("data.bin", b"\x00\x01opaque")],
        &ZipOptions::default(),
    );
    let report = import_file(
        &env.vault,
        &env.temp.file("x.zip", &bytes),
        &env.options,
        &NeverCancel,
    )
    .unwrap();
    assert_eq!(report.manifest.status, ImportStatus::Partial);
    let audit = audit_import(&env.vault, &report.import_id).unwrap();
    assert!(audit.raw_present && audit.is_consistent());
}

#[test]
fn imported_sources_still_resolve_after_backup_restore_elsewhere() {
    let env = Env::new("audit-restore");
    let archive = zip(
        &[
            Member::new("conversations.json", &to_bytes(&sample_export())),
            Member::new("notes-unrelated.txt", b"kept in the raw bytes"),
        ],
        &ZipOptions::default(),
    );
    let report = import_file(
        &env.vault,
        &env.temp.file("e.zip", &archive),
        &env.options,
        &NeverCancel,
    )
    .unwrap();
    let pin = env.vault.pin_current().unwrap();
    let export = env.temp.dir("export");
    export_pinned(
        &env.vault,
        &pin,
        &verify_data_root(&export, &RootPolicy::default()).unwrap(),
    )
    .unwrap();
    std::fs::remove_dir_all(env.temp.path().join("vault-root")).unwrap();

    let target = env.temp.dir("restored");
    let restored = restore_export(
        &export,
        &verify_data_root(&target, &RootPolicy::default()).unwrap(),
        &env.options.owner,
        TrustedSurface::TrustedLocalCli,
        env.clock.clone(),
        Arc::new(SequentialIdSource::new(0xA000)),
        VaultOptions::default(),
    )
    .unwrap();
    let audit = audit_import(&restored.vault, &report.import_id).unwrap();
    assert!(audit.is_consistent(), "{audit:?}");
    assert_eq!(audit.sources_citing, 10);
    let raw = restored
        .vault
        .read_object(
            &restored.vault.pin_current().unwrap(),
            &report.manifest.input_object_hash,
        )
        .unwrap();
    assert_eq!(raw, archive, "the received ZIP comes back byte for byte");
}

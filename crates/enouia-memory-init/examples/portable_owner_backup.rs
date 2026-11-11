//! ADR138. Built-in synthetic data only; retain every newly allocated root.
//! `export` prints a package path. Copy only its export/ and expectations.json.
//! `restore <package>` always allocates a fresh temporary target of its own.
use enouia_memory_contract::{
    common::Sensitivity, foundation::FakeClock, ids::*, json::canonical_bytes, next::*,
    ports::IdSource, time::Timestamp,
};
use enouia_memory_init::{pipeline::*, synthetic};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, VerifiedRoot,
    automation::segmented::{SegmentedAutomationVault, backup::*},
    fault::Faults,
    verify_data_root,
};
use serde_json::{Value, json};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

const NOW: i64 = 1_792_843_200_000;
const FORMAT: &str = "synthetic-owner-portability/1";

fn fresh() -> PathBuf {
    let base = std::env::temp_dir().join("enouia-owner-portability-synthetic");
    std::fs::create_dir_all(&base).unwrap();
    let canonical = std::fs::canonicalize(&base).unwrap();
    let base = PathBuf::from(canonical.to_string_lossy().trim_start_matches(r"\\?\"));
    let path = base.join(format_uuid_v4(OsIdSource.random_16()));
    std::fs::create_dir(&path).unwrap();
    path
}
fn verified(path: &Path) -> VerifiedRoot {
    verify_data_root(path, &RootPolicy::default()).unwrap()
}
fn observation(init: &Initializer) -> Value {
    init.vault.read(&init.vault.descriptor().system_actor, Sensitivity::Private, |data| {
        let sources: Vec<_> = data.sources.values().map(|s| (*s).clone()).collect();
        for memory in &data.memories {
            for quote in &memory.memory.decision.input.record_fields.as_ref().unwrap().evidence {
                assert!(quote.resolves(&sources), "source jump must still resolve");
            }
        }
        json!({
            "snapshot": data.snapshot,
            "sources": data.sources,
            "conversations": data.conversations,
            "memories": data.memories.iter().map(|m| json!({"memory": m.memory, "receipt": m.receipt, "target": m.target})).collect::<Vec<_>>()
        })
    }).unwrap()
}
fn main() {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let clock = Arc::new(FakeClock::new(NOW));
    let template: AutomationDescriptor = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/catalog/descriptor.json"
    ))
    .unwrap();
    let mut policy: AutomationPolicy = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/formation/policy.json"
    ))
    .unwrap();
    match args.as_slice() {
        [command] if command == "export" => {
            let package = fresh();
            let source_path = package.join("source");
            let export_path = package.join("export");
            std::fs::create_dir(&source_path).unwrap();
            std::fs::create_dir(&export_path).unwrap();
            let root = verified(&source_path);
            let mut descriptor = template;
            descriptor.vault_id = VaultId::from_random(OsIdSource.random_16());
            descriptor.layout = AutomationLayout::SegmentedOwnerV1;
            descriptor.owner_subject_id = Some(policy.subject_id.clone());
            policy.vault_id = descriptor.vault_id.clone();
            let suite = synthetic::display_suite();
            let plan = Plan::build(
                &suite.preparation,
                &suite.statements,
                policy,
                FormationMode::Backfill,
                Timestamp::from_unix_ms(NOW).unwrap(),
                &OsIdSource,
            )
            .unwrap();
            let mut init = Initializer::create_synthetic(
                &root,
                descriptor,
                clock.clone(),
                Arc::new(OsIdSource),
            )
            .unwrap();
            init.enqueue(plan, &suite.preparation).unwrap();
            while init.step().unwrap() {}
            let deleted = init.plans()[0].receipts()[0].result.memory_id.clone();
            init.vault
                .delete(
                    &init.vault.descriptor().created_by,
                    vec![],
                    vec![deleted.clone()],
                )
                .unwrap();
            let expected = json!({"format": FORMAT, "ledger": init.ledger().unwrap(), "observation": observation(&init), "deleted_memory": deleted});
            assert!(
                !expected["observation"]["memories"]
                    .as_array()
                    .unwrap()
                    .is_empty()
            );
            drop(init);
            let export =
                export_owner(&root, &verified(&export_path), clock, Arc::new(OsIdSource)).unwrap();
            std::fs::write(
                package.join("expectations.json"),
                canonical_bytes(&expected).unwrap(),
            )
            .unwrap();
            let offline = package.join("source-offline");
            assert_eq!(source_path.parent(), offline.parent());
            std::fs::rename(source_path, offline).unwrap();
            println!(
                "{}",
                json!({"package": package, "files": export.files.len(), "sequence": export.head.sequence, "model_invocations": 0, "real_data": false})
            );
        }
        [command, path] if command == "restore" => {
            let package = verified(Path::new(path));
            let expected: Value = serde_json::from_slice(
                &std::fs::read(package.path().join("expectations.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(expected["format"], FORMAT);
            let export_root = verified(&package.path().join("export"));
            verify_owner(&export_root).unwrap();
            let descriptor: AutomationDescriptor = parse_document(
                &std::fs::read(export_root.path().join("vault/vault.json")).unwrap(),
            )
            .unwrap();
            assert_eq!(descriptor.created_by, template.created_by);
            assert_eq!(descriptor.system_actor, template.system_actor);
            assert_eq!(descriptor.owner_subject_id, Some(policy.subject_id));
            let target_path = fresh();
            let target = verified(&target_path);
            let export = restore_owner(&export_root, &target, clock.as_ref()).unwrap();
            assert!(
                SegmentedAutomationVault::open(
                    &target,
                    descriptor.host_id.clone(),
                    clock.clone(),
                    Arc::new(OsIdSource),
                    Faults::none()
                )
                .is_err()
            );
            activate_owner(&target, &export_root, clock.as_ref()).unwrap();
            let mut init = Initializer::open_synthetic(
                &target,
                descriptor.host_id,
                clock,
                Arc::new(OsIdSource),
            )
            .unwrap();
            let before = init.vault.snapshot().unwrap();
            assert_eq!(init.ledger().unwrap(), expected["ledger"]);
            let actual = observation(&init);
            assert_eq!(actual, expected["observation"]);
            assert!(
                !actual["memories"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|m| m["memory"]["memory"]["memory_id"] == expected["deleted_memory"])
            );
            assert!(
                !init.step().unwrap(),
                "completed initialization must not run again"
            );
            assert_eq!(init.vault.snapshot().unwrap(), before);
            assert_eq!(init.ledger().unwrap()["extraction"]["model_invocations"], 0);
            println!(
                "{}",
                json!({"restored_root": target_path, "files": export.files.len(), "sources": actual["sources"].as_object().unwrap().len(), "visible_memories": actual["memories"].as_array().unwrap().len(), "ledger_equal": true, "source_jumps": true, "deletion_barrier": true, "new_commits": 0, "model_invocations": 0, "real_data": false})
            );
        }
        _ => panic!("usage: portable_owner_backup export | restore <synthetic-package>"),
    }
}

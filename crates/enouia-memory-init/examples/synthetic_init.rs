//! Run only built-in synthetic data in a freshly allocated temporary root.
//! Intentionally retain that root for inspection. No input-directory argument.
use enouia_memory_contract::{
    foundation::FakeClock, ids::*, json::canonical_bytes, next::*, ports::IdSource, time::Timestamp,
};
use enouia_memory_init::{pipeline::*, synthetic};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, fault::Faults, fs::ManagedRoot, verify_data_root,
};
use serde_json::json;
use std::sync::Arc;

fn main() {
    let base = std::env::temp_dir().join("enouia-memory-init-synthetic");
    std::fs::create_dir_all(&base).unwrap();
    let canonical = std::fs::canonicalize(base).unwrap();
    let base = std::path::PathBuf::from(canonical.to_string_lossy().trim_start_matches(r"\\?\"));
    let path = base.join(format!("root-{}", format_uuid_v4(OsIdSource.random_16())));
    std::fs::create_dir(&path).unwrap();
    let root = verify_data_root(&path, &RootPolicy::default()).unwrap();
    let mut descriptor: AutomationDescriptor = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/catalog/descriptor.json"
    ))
    .unwrap();
    descriptor.vault_id = VaultId::from_random(OsIdSource.random_16());
    let mut policy: AutomationPolicy = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/formation/policy.json"
    ))
    .unwrap();
    policy.vault_id = descriptor.vault_id.clone();
    descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    descriptor.owner_subject_id = Some(policy.subject_id.clone());
    let clock = Arc::new(FakeClock::new(1_792_843_200_000));
    let suite = synthetic::display_suite();
    let plan = Plan::build(
        &suite.preparation,
        &suite.statements,
        policy,
        FormationMode::Backfill,
        Timestamp::from_unix_ms(1_792_843_200_000).unwrap(),
        &OsIdSource,
    )
    .unwrap();
    // Preflight is complete before creating a descriptor or touching CURRENT.
    let preflight = plan.preflight.clone();
    let owner = descriptor.created_by.clone();
    let host = descriptor.host_id.clone();
    let mut init =
        Initializer::create_synthetic(&root, descriptor, clock.clone(), Arc::new(OsIdSource))
            .unwrap();
    let batch = init.enqueue(plan, &suite.preparation).unwrap();
    for _ in 0..6 {
        init.step().unwrap();
    }
    drop(init);
    let mut init = Initializer::open_synthetic(&root, host, clock, Arc::new(OsIdSource)).unwrap();
    while init.step().unwrap() {}
    let profile: AttentionProfile = serde_json::from_slice(include_bytes!(
        "../../../tests/fixtures/attention/profile.json"
    ))
    .unwrap();
    let before = init
        .projection_bundle(&profile, "Asia/Shanghai", "2026-06-01", "2026-11-01")
        .unwrap();
    let ledger = init.ledger().unwrap();
    assert!(
        !before["projections"]["themes"]["pages"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    assert_eq!(before["coverage"]["precision_distribution"]["month"], 2);
    assert_eq!(
        before["coverage"]["one_statement"]["multi_statement_sources"],
        1
    );
    let deleted = init.plans()[0].receipts()[0].result.memory_id.clone();
    init.vault
        .delete(&owner, vec![], vec![deleted.clone()])
        .unwrap();
    let after = init
        .projection_bundle(&profile, "Asia/Shanghai", "2026-06-01", "2026-11-01")
        .unwrap();
    assert!(
        !serde_json::to_string(&after)
            .unwrap()
            .contains(deleted.as_str())
    );
    let managed = ManagedRoot::new(root.path(), Faults::none());
    let report = json!({"batch_id":batch,"preflight":preflight,"ledger":ledger,"coverage_before_deletion":before["coverage"],
        "coverage_after_deletion":after["coverage"],"scenarios":suite.scenarios,"deletion_all_seven_rebuilt":true,
        "runtime_ui":"not_run","real_data":false,"model_calls":0});
    managed
        .write_atomic(
            "indexes/initialization-report.json",
            &canonical_bytes(&report).unwrap(),
        )
        .unwrap();
    managed
        .write_atomic(
            "indexes/initialization-projections-before.json",
            &canonical_bytes(&before).unwrap(),
        )
        .unwrap();
    managed
        .write_atomic(
            "indexes/initialization-projections-current.json",
            &canonical_bytes(&after).unwrap(),
        )
        .unwrap();
    // Both are inspection artifacts. Consumers must reauthorize and compare
    // envelope metadata with CURRENT; the before bundle is deliberately stale.
    println!("synthetic_root={}", root.path().display());
    println!("{}", serde_json::to_string_pretty(&report).unwrap());
}

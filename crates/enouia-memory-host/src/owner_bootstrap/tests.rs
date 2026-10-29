use super::*;
use enouia_memory_contract::foundation::FakeClock;
fn fixture() -> Bootstrap {
    let path = std::env::temp_dir().join(format!(
        "enouia-bootstrap-synthetic-{}",
        format_uuid_v4(OsIdSource.random_16())
    ));
    fs::create_dir(&path).unwrap();
    let path = fs::canonicalize(path).unwrap();
    let path = Path::new(path.to_str().unwrap().trim_start_matches(r"\\?\")).to_path_buf();
    platform::restrict_to_owner(&path).unwrap();
    fs::write(
        path.join("DO_NOT_DELETE.USER_DATA"),
        b"synthetic owner data; retain",
    )
    .unwrap();
    fs::write(path.join(META),bytes(&json!({"format":"enouia-owner-data-home/1","storage_class":"synthetic_test","allow_development_cleanup":false,"allow_git_upload":false,"production_vault_created":false,"active_vault":null,"unknown":{"keep":[1,2,3]}})).unwrap()).unwrap();
    Bootstrap::home(
        &path,
        Identity::Test(sha256(b"synthetic-owner-a")),
        Arc::new(FakeClock::new(1_792_843_200_000)),
    )
    .unwrap()
}
fn operation(value: &Value) -> OperationId {
    serde_json::from_value(value["operation_id"].clone()).unwrap()
}
#[test]
fn registered_empty_creation_reopens_and_preserves_unknown_home_fields() {
    let b = fixture();
    let p = b.prepare().unwrap();
    let op = operation(&p);
    assert!(!Path::new(p["root"].as_str().unwrap()).exists());
    let result = b.commit(&op, false).unwrap();
    assert_eq!(result["phase"], "recorded");
    assert_eq!(result["empty_at_creation"], true);
    assert_eq!(result["runtime_adopted"], false);
    let (_, meta) = b.check_home().unwrap();
    assert_eq!(meta["unknown"], json!({"keep":[1,2,3]}));
    assert_eq!(b.commit(&op, false).unwrap(), result);
    assert_eq!(b.prepare().unwrap()["operation_id"], p["operation_id"]);
}
#[test]
fn same_operation_resumes_after_genesis_before_metadata_publication() {
    let b = fixture();
    let p = b.prepare().unwrap();
    let op = operation(&p);
    let (store, lock) = b.store(false).unwrap();
    let mut r = b.load(&store).unwrap();
    r.phase = Phase::Creating;
    b.save(&store, &r).unwrap();
    drop(lock);
    let path = b.home.path().join(&r.root_relative);
    fs::create_dir(b.home.path().join("vaults")).unwrap();
    platform::restrict_to_owner(&b.home.path().join("vaults")).unwrap();
    fs::create_dir(&path).unwrap();
    let root = verify_data_root(&path, &RootPolicy::default()).unwrap();
    drop(
        SegmentedAutomationVault::create(
            &root,
            r.descriptor.clone(),
            b.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap(),
    );
    let done = b.commit(&op, false).unwrap();
    assert_eq!(done["descriptor"], p["descriptor"]);
    assert_eq!(done["snapshot_at_creation"]["primary_epoch"], 1);
}
#[test]
fn other_account_and_modified_binding_do_not_gain_owner() {
    let mut b = fixture();
    let p = b.prepare().unwrap();
    let op = operation(&p);
    b.identity = Identity::Test(sha256(b"synthetic-owner-b"));
    assert!(matches!(
        b.commit(&op, false),
        Err(BootstrapFault::Unauthenticated)
    ));
    b.identity = Identity::Test(sha256(b"synthetic-owner-a"));
    let (store, lock) = b.store(false).unwrap();
    let mut v: Value = serde_json::from_slice(&store.read(JOURNAL).unwrap().unwrap()).unwrap();
    v["root_relative"] = json!("../escape");
    store.write_atomic(JOURNAL, &bytes(&v).unwrap()).unwrap();
    drop(lock);
    assert!(matches!(
        b.commit(&op, false),
        Err(BootstrapFault::Unauthenticated)
    ));
}
#[test]
fn metadata_conflict_and_nonempty_unknown_root_are_retained_not_overwritten() {
    let b = fixture();
    let p = b.prepare().unwrap();
    let op = operation(&p);
    let (_, mut meta) = b.check_home().unwrap();
    meta["concurrent_owner_change"] = json!(true);
    fs::write(b.home.path().join(META), bytes(&meta).unwrap()).unwrap();
    assert!(matches!(
        b.commit(&op, false),
        Err(BootstrapFault::Conflict)
    ));
    assert!(!Path::new(p["root"].as_str().unwrap()).exists());
    let (store, lock) = b.store(false).unwrap();
    let r = b.load(&store).unwrap();
    fs::write(b.home.path().join(META), bytes(&r.baseline).unwrap()).unwrap();
    drop(lock);
    let path = Path::new(p["root"].as_str().unwrap());
    fs::create_dir(b.home.path().join("vaults")).unwrap();
    platform::restrict_to_owner(&b.home.path().join("vaults")).unwrap();
    fs::create_dir(path).unwrap();
    fs::write(path.join("keep-user-data"), b"synthetic").unwrap();
    assert!(b.commit(&op, false).is_err());
    assert_eq!(fs::read(path.join("keep-user-data")).unwrap(), b"synthetic");
}
#[test]
fn duplicate_metadata_and_wire_identity_overrides_are_refused() {
    assert!(json::parse(br#"{"active_vault":null,"active_vault":"other"}"#).is_err());
    assert!(matches!(
        run_cli(&["prepare".into(), "unused".into(), "--actor=owner".into()]),
        Err(BootstrapFault::Invalid)
    ));
    assert!(matches!(
        run_cli(&["prepare".into(), "unused".into(), "--subject=enouia".into()]),
        Err(BootstrapFault::Invalid)
    ));
}
#[test]
fn missing_registration_never_bootstraps_from_a_descriptor() {
    let b = fixture();
    let op = OperationId::from_random(OsIdSource.random_16());
    assert!(matches!(
        b.commit(&op, false),
        Err(BootstrapFault::NotRegistered)
    ));
    assert!(!b.home.path().join("vaults").exists());
}

#[test]
fn native_os_evidence_is_live_but_cannot_adopt_a_synthetic_home() {
    let session = CurrentProcessSession::capture().unwrap();
    session.verify().unwrap();
    let b = fixture();
    assert!(matches!(
        Bootstrap::home(b.home.path(), Identity::Native(session), b.clock.clone()),
        Err(BootstrapFault::UnsafeHome)
    ));
}

#[test]
fn home_lock_excludes_a_second_registration_without_reissuing_ids() {
    let b = fixture();
    let original = b.prepare().unwrap();
    let (_store, lock) = b.store(false).unwrap();
    assert!(matches!(b.prepare(), Err(BootstrapFault::Busy)));
    drop(lock);
    assert_eq!(b.prepare().unwrap(), original);
}

#[test]
fn published_current_without_witness_requires_explicit_same_operation_recovery() {
    use enouia_memory_vault::fault::{FaultAction, FaultPoint};
    let mut b = fixture();
    let preview = b.prepare().unwrap();
    let op = operation(&preview);
    b.storage_faults = Faults::armed();
    b.storage_faults
        .arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(112));
    assert!(matches!(
        b.commit(&op, false),
        Err(BootstrapFault::OutcomeUnknown)
    ));
    assert_eq!(b.check_home().unwrap().1["active_vault"], Value::Null);
    assert!(matches!(
        b.commit(&op, false),
        Err(BootstrapFault::OutcomeUnknown)
    ));
    let result = b.commit(&op, true).unwrap();
    assert_eq!(result["operation_id"], preview["operation_id"]);
    assert_eq!(result["descriptor"], preview["descriptor"]);
    assert_eq!(result["phase"], "recorded");
}

#[test]
fn lost_metadata_response_resolves_original_registration_without_recreating_root() {
    let b = fixture();
    let p = b.prepare().unwrap();
    let op = operation(&p);
    let done = b.commit(&op, false).unwrap();
    let (store, lock) = b.store(false).unwrap();
    let mut r = b.load(&store).unwrap();
    r.phase = Phase::Verified;
    b.save(&store, &r).unwrap();
    drop(lock);
    assert_eq!(b.commit(&op, false).unwrap(), done);
}

#[test]
fn modified_home_bytes_conflict_even_when_json_values_are_equal() {
    let b = fixture();
    let p = b.prepare().unwrap();
    let op = operation(&p);
    let (_, meta) = b.check_home().unwrap();
    fs::write(b.home.path().join(META), serde_json::to_vec(&meta).unwrap()).unwrap();
    assert!(matches!(
        b.commit(&op, false),
        Err(BootstrapFault::Conflict)
    ));
    assert!(!Path::new(p["root"].as_str().unwrap()).exists());
}

#[test]
fn nonempty_business_state_cannot_be_reported_as_an_empty_genesis() {
    let b = fixture();
    let p = b.prepare().unwrap();
    let op = operation(&p);
    b.commit(&op, false).unwrap();
    let (store, _lock) = b.store(false).unwrap();
    let r = b.load(&store).unwrap();
    let root = verify_data_root(
        Path::new(p["root"].as_str().unwrap()),
        &RootPolicy::default(),
    )
    .unwrap();
    let vault = SegmentedAutomationVault::open(
        &root,
        r.descriptor.host_id,
        b.clock.clone(),
        Arc::new(OsIdSource),
        Faults::none(),
    )
    .unwrap();
    let policy: AutomationPolicy = serde_json::from_slice(include_bytes!(
        "../../../../tests/fixtures/formation/policy.json"
    ))
    .unwrap();
    let mut policy = policy;
    policy.vault_id = r.descriptor.vault_id;
    policy.system_actor = r.descriptor.system_actor.clone();
    vault.set_policy(&r.descriptor.created_by, policy).unwrap();
    assert!(
        vault
            .empty_owner_genesis_snapshot(&r.descriptor.system_actor)
            .is_err()
    );
}

#[test]
fn copied_pending_registration_cannot_create_the_same_identity_under_another_home() {
    let a = fixture();
    let p = a.prepare().unwrap();
    let b = fixture();
    let (source, _source_lock) = a.store(false).unwrap();
    let (target, target_lock) = b.store(true).unwrap();
    target
        .write_new(JOURNAL, &source.read(JOURNAL).unwrap().unwrap())
        .unwrap();
    drop(target_lock);
    assert!(matches!(
        b.commit(&operation(&p), false),
        Err(BootstrapFault::Unauthenticated)
    ));
    assert!(!b.home.path().join("vaults").exists());
}

#[test]
fn copied_recorded_registration_cannot_claim_another_home_has_a_created_vault() {
    let a = fixture();
    let p = a.prepare().unwrap();
    a.commit(&operation(&p), false).unwrap();
    let b = fixture();
    let (source, _source_lock) = a.store(false).unwrap();
    let (target, target_lock) = b.store(true).unwrap();
    target
        .write_new(JOURNAL, &source.read(JOURNAL).unwrap().unwrap())
        .unwrap();
    drop(target_lock);
    assert!(matches!(
        b.commit(&operation(&p), false),
        Err(BootstrapFault::Unauthenticated)
    ));
    assert!(matches!(b.prepare(), Err(BootstrapFault::Unauthenticated)));
    assert!(!b.home.path().join("vaults").exists());
}

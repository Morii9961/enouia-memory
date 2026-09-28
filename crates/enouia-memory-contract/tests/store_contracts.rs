//! Vault store file contracts (ADR-MEM-36): every valid fixture passes the
//! schema harness and the Rust parser and round-trips; every invalid mutation
//! is rejected by Rust with the named rule, and by the schema unless marked
//! `schema: accept`.

mod support;

use enouia_memory_contract::ContractError;
use enouia_memory_contract::store::{
    CurrentPointer, ExportManifest, IdempotencyEntry, PublishRecord, RecoveryReceipt, RestoreState,
    StoreDocument, VaultDescriptor, parse_store_value,
};
use serde_json::Value;
use support::apply_ops;
use support::schema::SchemaStore;

fn store_fixture(name: &str) -> Value {
    support::fixture(&format!("../store/{name}"))
}

fn round_trip<T: StoreDocument>(value: &Value) -> Result<Value, ContractError> {
    parse_store_value::<T>(value).map(|doc| serde_json::to_value(doc).unwrap())
}

fn parse(kind: &str, value: &Value) -> Result<Value, ContractError> {
    match kind {
        "descriptor" => round_trip::<VaultDescriptor>(value),
        "current" => round_trip::<CurrentPointer>(value),
        "publish_record" => round_trip::<PublishRecord>(value),
        "idempotency" => round_trip::<IdempotencyEntry>(value),
        "recovery_receipt" => round_trip::<RecoveryReceipt>(value),
        "export_manifest" => round_trip::<ExportManifest>(value),
        "restore_state" => round_trip::<RestoreState>(value),
        other => panic!("unknown store type {other}"),
    }
}

#[test]
fn valid_store_documents_pass_schema_and_rust_and_round_trip() {
    let store = SchemaStore::load();
    let manifest = store_fixture("store-manifest.json");
    let valid = manifest["valid"].as_array().unwrap();
    assert_eq!(valid.len(), 7);
    for entry in valid {
        let file = entry["file"].as_str().unwrap();
        let value = store_fixture(file);
        let errors = store.validate(entry["schema"].as_str().unwrap(), &value);
        assert!(errors.is_empty(), "{file} schema errors: {errors:#?}");
        let back = parse(entry["type"].as_str().unwrap(), &value)
            .unwrap_or_else(|e| panic!("{file} rejected by Rust: {e}"));
        assert_eq!(back, value, "{file} lost or changed fields");
    }
}

#[test]
fn invalid_store_documents_are_rejected_with_the_named_rule() {
    let store = SchemaStore::load();
    let manifest = store_fixture("store-manifest.json");
    let valid = manifest["valid"].as_array().unwrap();
    let mut schema_blind = 0;
    for case in manifest["invalid"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let base_name = case["base"].as_str().unwrap();
        let base = valid.iter().find(|v| v["file"] == base_name).unwrap();
        let mut value = store_fixture(base_name);
        apply_ops(&mut value, &case["ops"]);
        let errors = store.validate(base["schema"].as_str().unwrap(), &value);
        match case["schema"].as_str().unwrap() {
            "reject" => assert!(!errors.is_empty(), "{id}: schema unexpectedly accepted"),
            "accept" => {
                assert!(errors.is_empty(), "{id}: schema rejected ({errors:?})");
                schema_blind += 1;
            }
            other => panic!("{id}: bad expectation {other}"),
        }
        let rule = case["rust_rule"].as_str().unwrap();
        match parse(base["type"].as_str().unwrap(), &value) {
            Ok(_) => panic!("{id}: Rust accepted an invalid store document"),
            Err(error) => assert!(
                error.rules().contains(&rule),
                "{id}: expected {rule}, got {error}"
            ),
        }
    }
    assert!(
        schema_blind >= 7,
        "schema-blind store constraints: {schema_blind}"
    );
}

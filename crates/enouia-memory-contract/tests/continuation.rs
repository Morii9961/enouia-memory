mod support;
use enouia_memory_contract::{json::canonical_bytes, next::*, time::Timestamp};
use serde_json::Value;
use support::schema::SchemaStore;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../continuation/{name}.json"))
}
fn parse(kind: &str, value: &Value) -> bool {
    let bytes = canonical_bytes(value).unwrap();
    match kind {
        "policy" => parse_document::<ContinuationPolicy>(&bytes).is_ok(),
        _ => parse_document::<ContinuationJournal>(&bytes).is_ok(),
    }
}
#[test]
fn continuous_policy_and_journal_schema_cases_match_native_validation() {
    let schemas = SchemaStore::load();
    let manifest = fixture("manifest");
    for entry in manifest["valid"].as_array().unwrap() {
        let name = entry["file"].as_str().unwrap().trim_end_matches(".json");
        let value = fixture(name);
        assert!(
            schemas
                .validate("next/continuation-v2.schema.json", &value)
                .is_empty(),
            "{name}"
        );
        assert!(parse(entry["type"].as_str().unwrap(), &value), "{name}");
    }
    for case in manifest["invalid"].as_array().unwrap() {
        let mut value = fixture(case["base"].as_str().unwrap().trim_end_matches(".json"));
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/continuation-v2.schema.json", &value)
                .is_empty(),
            case["schema"] == "accept",
            "{}",
            case["id"]
        );
        assert!(
            !parse(
                if case["base"] == "policy.json" {
                    "policy"
                } else {
                    "journal"
                },
                &value
            ),
            "{}",
            case["id"]
        );
    }
}
#[test]
fn exact_scope_destination_model_and_expiry_cannot_be_expanded_by_the_input() {
    let policy: ContinuationPolicy = serde_json::from_value(fixture("policy")).unwrap();
    let mut input: AdmissionInput =
        serde_json::from_value(support::fixture("../next/input.json")).unwrap();
    input.model_version = Some(policy.destination.model.clone());
    let now = Timestamp::parse("2026-10-24T12:00:00.000Z").unwrap();
    assert!(policy.covers(&input, &policy.destination, &now));
    let original = serde_json::to_value(&input).unwrap();
    for (path, changed) in [
        ("/actor/actor_type", serde_json::json!("owner")),
        ("/policy/hash", serde_json::json!("9".repeat(64))),
        ("/model_version", serde_json::json!("another-model")),
        ("/producer_version", serde_json::json!("untrusted-producer")),
        ("/sensitivity", serde_json::json!("highly_sensitive")),
        (
            "/project_id",
            serde_json::json!("prj_00000001-0000-4000-8000-000000000001"),
        ),
        (
            "/subject_id",
            serde_json::json!("sub_00000009-0000-4000-8000-000000000001"),
        ),
        (
            "/egress_policy_id",
            serde_json::json!("pol_00000009-0000-4000-8000-000000000001"),
        ),
    ] {
        let mut altered = original.clone();
        *altered.pointer_mut(path).unwrap() = changed;
        let altered: AdmissionInput = serde_json::from_value(altered).unwrap();
        assert!(
            !policy.covers(&altered, &policy.destination, &now),
            "{path}"
        );
    }
    let mut other = policy.destination.clone();
    other.adapter_version.push_str("-changed");
    assert!(!policy.covers(&input, &other, &now));
    assert!(!policy.covers(&input, &policy.destination, &policy.valid_until));
    let mut denied = policy.clone();
    denied.active = false;
    assert!(!denied.covers(&input, &policy.destination, &now));
}
#[test]
fn unknown_costs_keep_reservations_and_known_overruns_increase_charges() {
    let mut journal: ContinuationJournal = serde_json::from_value(fixture("unknown")).unwrap();
    assert_eq!(journal.charges(), Some((100, 10)));
    journal.attempts[0].tokens_used = Some(150);
    journal.attempts[0].cost_microusd = Some(25);
    assert_eq!(journal.charges(), Some((150, 25)));
    assert!(journal.validate().is_empty());
    journal.policy.as_mut().unwrap().revision =
        enouia_memory_contract::json::Revision::new(2).unwrap();
    assert!(!journal.validate().is_empty());
    assert_eq!(journal.charges(), Some((150, 25)));
}

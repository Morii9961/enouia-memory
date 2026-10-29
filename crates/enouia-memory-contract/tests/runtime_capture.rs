mod support;
use enouia_memory_contract::next::RuntimeCaptureRequest;
#[test]
fn runtime_capture_closed_packets_and_native_semantics() {
    let corpus = support::fixture("../runtime-capture/cases.json");
    let schemas = support::schema::SchemaStore::load();
    for case in corpus["cases"].as_array().unwrap() {
        let mut value = corpus["base"].clone();
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/runtime-capture-v2.schema.json", &value)
                .is_empty(),
            case["schema"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        assert_eq!(
            serde_json::from_value::<RuntimeCaptureRequest>(value).is_ok_and(|r| r.valid()),
            case["native"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}
#[test]
fn runtime_wire_modes_and_enrollment_fields_are_closed() {
    let raw = support::fixture("../runtime-capture/cases.json");
    let cases = support::fixture("../runtime-capture/wire-cases.json");
    let schemas = support::schema::SchemaStore::load();
    for case in cases["cases"].as_array().unwrap() {
        let mut value = serde_json::json!({"mode":"write","request":raw["base"]});
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/runtime-capture-pipe-v2.schema.json", &value)
                .is_empty(),
            case["accept"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

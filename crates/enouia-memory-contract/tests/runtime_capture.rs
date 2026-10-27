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

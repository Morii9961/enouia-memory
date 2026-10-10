mod support;
use enouia_memory_contract::provider::Invocation;
use support::{fixture, schema::SchemaStore};

#[test]
fn provider_invocation_schema_and_rust_agree() {
    let schemas = SchemaStore::load();
    let cases = fixture("provider-invocation-cases.json");
    for case in cases["cases"].as_array().unwrap() {
        let mut value = cases["base"].clone();
        value
            .as_object_mut()
            .unwrap()
            .extend(case["set"].as_object().unwrap().clone());
        let expected = case["accept"].as_bool().unwrap();
        assert_eq!(
            schemas
                .validate("provider/invocation-v1.schema.json", &value)
                .is_empty(),
            expected,
            "{}",
            case["name"]
        );
        let accepted =
            serde_json::from_value::<Invocation>(value).is_ok_and(|r| r.validate().is_empty());
        assert_eq!(accepted, expected, "{}", case["name"]);
    }
}

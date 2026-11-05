mod support;

#[test]
fn opening_contract_is_closed_and_has_no_authority_or_effect_fields() {
    let corpus = support::fixture("../mcp-opening/cases.json");
    let schemas = support::schema::SchemaStore::load();
    for case in corpus["cases"].as_array().unwrap() {
        assert_eq!(
            schemas
                .validate("next/mcp-opening-v1.schema.json", &case["value"])
                .is_empty(),
            case["accept"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
    }
}

#[test]
fn opening_success_envelopes_refuse_extra_authority_binary_content_and_unissued_cursors() {
    let corpus = support::fixture("../mcp-opening/wire-cases.json");
    let schemas = support::schema::SchemaStore::load();
    for case in corpus["cases"].as_array().unwrap() {
        assert_eq!(
            schemas
                .validate("next/mcp-opening-wire-v1.schema.json", &case["value"])
                .is_empty(),
            case["accept"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if let Some(method) = case["payload_method"].as_str() {
            let result = &case["value"]["result"];
            let text = if method == "prompts/get" {
                result["messages"][0]["content"]["text"]
                    .as_str()
                    .unwrap()
                    .split_once('\n')
                    .unwrap()
                    .1
            } else {
                result["contents"][0]["text"].as_str().unwrap()
            };
            let accepted = serde_json::from_str::<serde_json::Value>(text).is_ok_and(|payload| {
                schemas
                    .validate("next/mcp-opening-v1.schema.json", &payload)
                    .is_empty()
            });
            assert_eq!(
                accepted,
                case["payload_root_accept"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}

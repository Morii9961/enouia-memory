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
    }
}

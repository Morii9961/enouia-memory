mod support;

#[test]
fn session_resource_link_wire_contract_and_cross_field_checks_are_distinct() {
    let corpus = support::fixture("../mcp-opening/session-link-cases.json");
    let schemas = support::schema::SchemaStore::load();
    for case in corpus["cases"].as_array().unwrap() {
        let mut value = corpus["base"].clone();
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/mcp-session-start-wire-v1.schema.json", &value)
                .is_empty(),
            case["accept"].as_bool().unwrap(),
            "{}",
            case["name"]
        );
        if let Some(matches) = case["text_matches"].as_bool() {
            let parsed: serde_json::Value =
                serde_json::from_str(value["result"]["content"][0]["text"].as_str().unwrap())
                    .unwrap();
            assert_eq!(
                parsed == value["result"]["structuredContent"],
                matches,
                "{}",
                case["name"]
            );
        }
        if let Some(matches) = case["uri_matches"].as_bool() {
            assert_eq!(
                value["result"]["content"][1]["uri"]
                    == value["result"]["structuredContent"]["context"]["uri"],
                matches,
                "{}",
                case["name"]
            );
        }
    }
}

#[test]
fn session_start_contract_refuses_authority_capture_and_false_session_receipts() {
    let corpus = support::fixture("../mcp-opening/session-start-cases.json");
    let schemas = support::schema::SchemaStore::load();
    for direction in ["request", "response"] {
        for case in corpus[format!("{direction}_cases")].as_array().unwrap() {
            let mut value = corpus[direction].clone();
            support::apply_ops(&mut value, &case["ops"]);
            assert_eq!(
                schemas
                    .validate("next/mcp-session-start-v1.schema.json", &value)
                    .is_empty(),
                case["accept"].as_bool().unwrap(),
                "{}",
                case["name"]
            );
        }
    }
}

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

mod support;
use enouia_memory_contract::{json::canonical_bytes, next::*};
use serde_json::Value;
use support::schema::SchemaStore;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../read-model/{name}.json"))
}
fn parse(kind: &str, v: &Value) -> bool {
    let b = canonical_bytes(v).unwrap();
    match kind {
        "page" => parse_document::<MemoryReadPage>(&b).is_ok(),
        "detail" => parse_document::<MemoryReadDetail>(&b).is_ok(),
        _ => parse_document::<MemoryReadSimulation>(&b).is_ok(),
    }
}
#[test]
fn versioned_read_outputs_match_schema_and_refuse_fake_authority_or_receipts() {
    let schemas = SchemaStore::load();
    let manifest = fixture("manifest");
    for e in manifest["valid"].as_array().unwrap() {
        let name = e["file"].as_str().unwrap().trim_end_matches(".json");
        let v = fixture(name);
        assert!(
            schemas
                .validate("next/read-model-v2.schema.json", &v)
                .is_empty(),
            "{name}"
        );
        assert!(parse(e["type"].as_str().unwrap(), &v), "{name}");
    }
    for c in manifest["invalid"].as_array().unwrap() {
        let name = c["base"].as_str().unwrap().trim_end_matches(".json");
        let mut v = fixture(name);
        support::apply_ops(&mut v, &c["ops"]);
        assert_eq!(
            schemas
                .validate("next/read-model-v2.schema.json", &v)
                .is_empty(),
            c["schema"] == "accept",
            "{}",
            c["id"]
        );
        assert!(!parse(name, &v), "{}", c["id"]);
    }
}
#[test]
fn declared_capabilities_are_real_bounded_reads_without_unimplemented_product_claims() {
    let page: MemoryReadPage = serde_json::from_value(fixture("page")).unwrap();
    assert_eq!(page.effect, MemoryReadEffect::NativeRead);
    assert!(
        page.capabilities.selected_history
            && page.capabilities.source_details
            && page.capabilities.read_simulation
    );
    assert!(
        !page.capabilities.global_timeline
            && !page.capabilities.global_graph
            && !page.capabilities.saved_experiment
            && !page.capabilities.legacy_owner_history
            && !page.capabilities.vectors
    );
    assert_eq!(
        page.items[0].system_actor.actor_type,
        enouia_memory_contract::common::ActorType::System
    );
    assert_eq!(page.items[0].knowledge, KnowledgeState::AttributedStatement);
}

#[test]
fn read_outputs_refuse_inconsistent_paging_history_and_recall_accounting() {
    let mut page = fixture("page");
    page["complete_in_scope"] = false.into();
    assert!(
        !parse("page", &page),
        "partial page needs a progressing cursor"
    );

    let mut detail = fixture("detail");
    let current = detail["item"].clone();
    detail["history"].as_array_mut().unwrap().push(current);
    assert!(
        !parse("detail", &detail),
        "history must not duplicate a revision"
    );

    let mut detail = fixture("detail");
    detail["history"][0]["content"] = "Contradictory current revision".into();
    assert!(!parse("detail", &detail), "the same revision has one value");

    let mut detail = fixture("detail");
    let source = detail["sources"][0].clone();
    detail["sources"].as_array_mut().unwrap().push(source);
    assert!(
        !parse("detail", &detail),
        "source identity and revision are unique"
    );

    let mut simulation = fixture("simulation");
    simulation["trace"]["considered"] = 0.into();
    assert!(
        !parse("simulation", &simulation),
        "every considered row is accounted for"
    );

    let mut simulation = fixture("simulation");
    simulation["trace"]["considered"] = 2.into();
    let selection = simulation["trace"]["selected"][0].clone();
    simulation["trace"]["selected"]
        .as_array_mut()
        .unwrap()
        .push(selection);
    assert!(
        !parse("simulation", &simulation),
        "selection identities must be unique"
    );
}

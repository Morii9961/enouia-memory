mod support;
use enouia_memory_contract::{json::canonical_bytes, next::*};
use serde_json::Value;
use support::schema::SchemaStore;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../process/{name}.json"))
}
fn native(kind: &str, v: &Value) -> bool {
    let b = canonical_bytes(v).unwrap();
    match kind {
        "MemoryActivityPage" => parse_document::<MemoryActivityPage>(&b).is_ok(),
        "MemoryChanges" => parse_document::<MemoryChanges>(&b).is_ok(),
        "NativeMemoryDetail" => parse_document::<NativeMemoryDetail>(&b).is_ok(),
        "MemoryConflictGroup" => parse_document::<MemoryConflictGroup>(&b).is_ok(),
        "MemoryRevisionDiff" => parse_document::<MemoryRevisionDiff>(&b).is_ok(),
        "NativeMemoryNeighbors" => parse_document::<NativeMemoryNeighbors>(&b).is_ok(),
        "NativeSourceExcerpt" => parse_document::<NativeSourceExcerpt>(&b).is_ok(),
        "NativeConversationPage" => parse_document::<NativeConversationPage>(&b).is_ok(),
        "NativeConversationDetail" => parse_document::<NativeConversationDetail>(&b).is_ok(),
        _ => panic!("unknown synthetic process document"),
    }
}
#[test]
fn process_schema_corpus_and_native_constraints_are_cross_checked() {
    let m = fixture("manifest");
    let schemas = SchemaStore::load();
    for e in m["valid"].as_array().unwrap() {
        let n = e["file"].as_str().unwrap().trim_end_matches(".json");
        let v = fixture(n);
        let errors = schemas.validate(e["schema"].as_str().unwrap(), &v);
        assert!(errors.is_empty(), "{n}: {errors:?}");
        assert!(native(e["type"].as_str().unwrap(), &v), "{n}");
    }
    for case in m["invalid"].as_array().unwrap() {
        let e = m["valid"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["file"] == case["base"])
            .unwrap();
        let mut v = fixture(case["base"].as_str().unwrap().trim_end_matches(".json"));
        support::apply_ops(&mut v, &case["ops"]);
        assert_eq!(
            schemas
                .validate(e["schema"].as_str().unwrap(), &v)
                .is_empty(),
            case["schema"] == "accept",
            "{}",
            case["id"]
        );
        assert!(!native(e["type"].as_str().unwrap(), &v), "{}", case["id"]);
    }
}

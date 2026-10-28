mod support;
use enouia_memory_contract::{json::canonical_bytes, next::*};
use serde_json::Value;
use support::schema::SchemaStore;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../projections/{name}.json"))
}
fn native(kind: &str, v: &Value) -> bool {
    let b = canonical_bytes(v).unwrap();
    match kind {
        "time" => parse_document::<MemoryTimeBuckets>(&b).is_ok(),
        "theme" => parse_document::<MemoryThemeMembers>(&b).is_ok(),
        "graph" => parse_document::<MemoryGraphPage>(&b).is_ok(),
        _ => panic!("unknown synthetic projection document"),
    }
}
#[test]
fn projection_schema_corpus_and_native_constraints_are_cross_checked() {
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

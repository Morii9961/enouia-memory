mod support;
use enouia_memory_contract::{json::canonical_bytes, next::*};
use serde_json::Value;
use support::schema::SchemaStore;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../owner/{name}.json"))
}
fn native(kind: &str, v: &Value) -> bool {
    let b = canonical_bytes(v).unwrap();
    match kind {
        "segment" => parse_document::<SegmentedCommit>(&b).is_ok(),
        "descriptor" => parse_document::<AutomationDescriptor>(&b).is_ok(),
        "plan" => parse_document::<OwnerActionPlan>(&b).is_ok(),
        "record" => parse_document::<OwnerActionRecord>(&b).is_ok(),
        "metadata" => serde_json::from_slice::<OwnerCorrectionMetadata>(&b)
            .is_ok_and(|d| d.validate().is_empty()),
        "effects" => {
            serde_json::from_slice::<OwnerActionEffects>(&b).is_ok_and(|d| d.validate().is_empty())
        }
        "outcome" => parse_document::<NativeOwnerOutcome>(&b).is_ok(),
        "proof" => {
            serde_json::from_slice::<OwnerActionProof>(&b).is_ok_and(|d| d.validate().is_empty())
        }
        "page" => parse_document::<CatalogMemoryPage>(&b).is_ok(),
        "graph" => parse_document::<MemoryGraphPage>(&b).is_ok(),
        "activity" => parse_document::<MemoryActivityPage>(&b).is_ok(),
        _ => panic!("unknown synthetic owner document"),
    }
}
#[test]
fn owner_schema_corpus_and_native_constraints_are_cross_checked() {
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

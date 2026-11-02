mod support;
use enouia_memory_contract::{json::canonical_bytes, next::*};
use serde_json::Value;
use support::schema::SchemaStore;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../catalog/{name}.json"))
}
fn native(kind: &str, v: &Value) -> bool {
    let b = canonical_bytes(v).unwrap();
    match kind {
        "descriptor" => parse_document::<AutomationDescriptor>(&b).is_ok(),
        "segment" => parse_document::<SegmentedCommit>(&b).is_ok(),
        "page" => parse_document::<CatalogMemoryPage>(&b).is_ok(),
        "stats" => parse_document::<CatalogMemoryStats>(&b).is_ok(),
        "facets" => parse_document::<CatalogMemoryFacets>(&b).is_ok(),
        "filter" => serde_json::from_value::<CatalogFilter>(v.clone()).is_ok_and(|f| f.valid()),
        _ => panic!("unknown synthetic catalog document"),
    }
}
#[test]
fn scalable_schema_corpus_and_native_constraints_are_cross_checked() {
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
#[test]
fn delta_commit_hashes_link_the_complete_synthetic_history_and_do_not_deserialize_as_old_snapshots()
{
    let mut previous: Option<SegmentedCommit> = None;
    for name in ["genesis", "policy", "capture", "admitted"] {
        let c: SegmentedCommit = parse_document(&canonical_bytes(&fixture(name)).unwrap()).unwrap();
        assert_eq!(c.parent, previous.as_ref().map(SegmentedCommit::current));
        assert!(
            parse_document::<AutomationCommit>(&canonical_bytes(&fixture(name)).unwrap()).is_err()
        );
        previous = Some(c);
    }
    let admitted = previous.unwrap();
    let p: CatalogMemoryPage = parse_document(&canonical_bytes(&fixture("page")).unwrap()).unwrap();
    assert_eq!(p.selection.snapshot, admitted.snapshot());
    assert_eq!(
        p.items[0].base.receipt.as_ref().unwrap().committed_in,
        admitted.commit_id
    );
}

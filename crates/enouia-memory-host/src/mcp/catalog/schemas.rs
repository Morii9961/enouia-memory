//! Closed, reachable-only schema bundles; no remote references at runtime.
use serde_json::Value;
use std::collections::BTreeMap;
pub(super) fn documents() -> BTreeMap<&'static str, &'static str> {
    let mut docs = super::super::v2::schemas::documents();
    docs.insert(
        "catalog-v2.schema.json",
        include_str!("../../../../../contracts/next/catalog-v2.schema.json"),
    );
    docs.insert(
        "agent-catalog-read-v2.schema.json",
        include_str!("../../../../../contracts/next/agent-catalog-read-v2.schema.json"),
    );
    docs
}
pub fn schema(tool: &str, output: bool) -> Option<Value> {
    let short = tool.strip_prefix("memory_")?;
    if !["search", "read", "source", "related", "recall"].contains(&short) {
        return None;
    }
    Some(super::super::v2::schemas::bundle(
        documents(),
        "agent-catalog-read-v2.schema.json",
        short,
        output,
    ))
}
pub(super) fn coverage() -> Value {
    let v: Value = serde_json::from_str(documents()["agent-catalog-read-v2.schema.json"])
        .expect("frozen schema");
    v["$defs"]["coverage"]["const"].clone()
}

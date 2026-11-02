//! Closed local schemas; only reachable definitions enter each tool bundle.
use serde_json::Value;
use std::collections::BTreeMap;
pub(super) fn documents() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        (
            "authority-v1.schema.json",
            include_str!("../../../../../contracts/host/authority-v1.schema.json"),
        ),
        (
            "common-v1.schema.json",
            include_str!("../../../../../contracts/memory/common-v1.schema.json"),
        ),
        (
            "source-v1.schema.json",
            include_str!("../../../../../contracts/memory/source-v1.schema.json"),
        ),
        (
            "agent-qualified-read-v2.schema.json",
            include_str!("../../../../../contracts/next/agent-qualified-read-v2.schema.json"),
        ),
        (
            "agent-record-read-v2.schema.json",
            include_str!("../../../../../contracts/next/agent-record-read-v2.schema.json"),
        ),
        (
            "automation-v2.schema.json",
            include_str!("../../../../../contracts/next/automation-v2.schema.json"),
        ),
        (
            "catalog-v2.schema.json",
            include_str!("../../../../../contracts/next/catalog-v2.schema.json"),
        ),
        (
            "formation-v2.schema.json",
            include_str!("../../../../../contracts/next/formation-v2.schema.json"),
        ),
        (
            "owner-execution-v2.schema.json",
            include_str!("../../../../../contracts/next/owner-execution-v2.schema.json"),
        ),
        (
            "process-v2.schema.json",
            include_str!("../../../../../contracts/next/process-v2.schema.json"),
        ),
        (
            "read-model-v2.schema.json",
            include_str!("../../../../../contracts/next/read-model-v2.schema.json"),
        ),
        (
            "storage-v2.schema.json",
            include_str!("../../../../../contracts/next/storage-v2.schema.json"),
        ),
        (
            "ui-records-v2.schema.json",
            include_str!("../../../../../contracts/next/ui-records-v2.schema.json"),
        ),
    ])
}
pub fn schema(tool: &str, output: bool) -> Option<Value> {
    let short = tool.strip_prefix("memory_")?;
    if !["search", "read", "source", "related", "recall"].contains(&short) {
        return None;
    }
    Some(super::super::v2::schemas::bundle(
        documents(),
        "agent-qualified-read-v2.schema.json",
        short,
        output,
    ))
}
pub(super) fn coverage() -> Value {
    let value: Value = serde_json::from_str(documents()["agent-qualified-read-v2.schema.json"])
        .expect("closed schema");
    value["$defs"]["coverage"]["const"].clone()
}

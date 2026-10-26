//! Reachable-only schema bundling; clients need no file or network references.
use serde_json::{Value, json};
use std::collections::BTreeMap;
pub(super) fn documents() -> BTreeMap<&'static str, &'static str> {
    BTreeMap::from([
        (
            "agent-read-v2.schema.json",
            include_str!("../../../../../contracts/next/agent-read-v2.schema.json"),
        ),
        (
            "read-model-v2.schema.json",
            include_str!("../../../../../contracts/next/read-model-v2.schema.json"),
        ),
        (
            "attention-v2.schema.json",
            include_str!("../../../../../contracts/next/attention-v2.schema.json"),
        ),
        (
            "automation-v2.schema.json",
            include_str!("../../../../../contracts/next/automation-v2.schema.json"),
        ),
        (
            "formation-v2.schema.json",
            include_str!("../../../../../contracts/next/formation-v2.schema.json"),
        ),
        (
            "storage-v2.schema.json",
            include_str!("../../../../../contracts/next/storage-v2.schema.json"),
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
            "authority-v1.schema.json",
            include_str!("../../../../../contracts/host/authority-v1.schema.json"),
        ),
    ])
}
fn walk(
    value: &mut Value,
    file: &str,
    docs: &BTreeMap<&str, Value>,
    seen: &mut BTreeMap<String, String>,
    defs: &mut BTreeMap<String, Value>,
) {
    match value {
        Value::Object(map) => {
            map.remove("$id");
            map.remove("$schema");
            map.remove("$defs");
            if let Some(reference) = map.get("$ref").and_then(Value::as_str).map(str::to_owned) {
                let (path, pointer) = reference.split_once('#').unwrap_or((&reference, ""));
                let source = if path.is_empty() {
                    file
                } else {
                    path.rsplit('/').next().expect("schema file")
                };
                let identity = format!("{source}#{pointer}");
                let key = if let Some(key) = seen.get(&identity) {
                    key.clone()
                } else {
                    let key = format!("d{}", seen.len());
                    seen.insert(identity, key.clone());
                    let mut node = docs[source]
                        .pointer(pointer)
                        .expect("frozen schema pointer")
                        .clone();
                    walk(&mut node, source, docs, seen, defs);
                    defs.insert(key.clone(), node);
                    key
                };
                map.insert("$ref".into(), json!(format!("#/$defs/{key}")));
            }
            for node in map.values_mut() {
                walk(node, file, docs, seen, defs);
            }
        }
        Value::Array(nodes) => {
            for node in nodes {
                walk(node, file, docs, seen, defs);
            }
        }
        _ => {}
    }
}
pub fn schema(tool: &str, output: bool) -> Option<Value> {
    let short = tool.strip_prefix("memory_")?;
    if !["search", "read", "source", "related", "recall"].contains(&short) {
        return None;
    }
    let docs: BTreeMap<_, Value> = documents()
        .into_iter()
        .map(|(k, v)| (k, serde_json::from_str(v).expect("frozen schema")))
        .collect();
    let mut schema = json!({"type":"object","$ref":format!("#/$defs/{short}_{}",if output{"response"}else{"request"})});
    let mut defs = BTreeMap::new();
    walk(
        &mut schema,
        "agent-read-v2.schema.json",
        &docs,
        &mut BTreeMap::new(),
        &mut defs,
    );
    schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    schema["$defs"] = json!(defs);
    Some(schema)
}
pub(super) fn coverage() -> Value {
    let schema: Value =
        serde_json::from_str(documents()["agent-read-v2.schema.json"]).expect("frozen schema");
    schema["$defs"]["coverage"]["const"].clone()
}

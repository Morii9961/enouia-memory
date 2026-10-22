use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::host::session::{HOST_PROTOCOL_VERSION, WORKSPACE_PROTOCOL_VERSION};
use enouia_memory_contract::json::canonical_bytes;

/// Exact schema compatibility, not authentication or equality of executable
/// implementations. Native clients pin the Core revision separately.
pub fn compatibility_hash() -> Sha256Hex {
    let schemas = [
        (
            "workspace",
            include_str!("../../../contracts/ipc/workspace-v1.schema.json"),
        ),
        (
            "common",
            include_str!("../../../contracts/memory/common-v1.schema.json"),
        ),
        (
            "authority",
            include_str!("../../../contracts/host/authority-v1.schema.json"),
        ),
        (
            "hello",
            include_str!("../../../contracts/host/client-hello-v1.schema.json"),
        ),
        (
            "welcome",
            include_str!("../../../contracts/host/server-hello-v1.schema.json"),
        ),
    ];
    let schemas: std::collections::BTreeMap<_, _> = schemas
        .into_iter()
        .map(|(k, v)| (k, v.replace("\r\n", "\n")))
        .collect();
    sha256(
        &canonical_bytes(&serde_json::json!({
            "protocol": HOST_PROTOCOL_VERSION,
            "workspace": WORKSPACE_PROTOCOL_VERSION,
            "layout": "host-current-v2",
            "framing": crate::framing::DESCRIPTOR,
            "schemas": schemas,
        }))
        .expect("closed compatibility metadata"),
    )
}

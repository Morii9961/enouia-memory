//! MCP-2: explicit schema-2, scoped read-only tools; legacy six tools unchanged.
mod schemas;
pub use schemas::schema;
#[cfg(windows)]
pub mod native;
use super::{
    MAX_INPUT_BYTES, MAX_OUTPUT_BYTES, MAX_REQUESTS, PROTOCOL_VERSION, Phase, closed, error, reply,
    tool_result,
};
use crate::automation::agent_read::{AgentReader, AgentSearchFilter, CAPABILITY};
use crate::automation::application::ApplicationFault;
use enouia_memory_contract::{
    ids::{MemoryId, RequestId, SourceId},
    json::{MAX_SAFE_INTEGER, Revision, nullable},
    next::*,
    time::Timestamp,
};
use enouia_memory_vault::automation::AutomationFault;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;
const TOOLS: [&str; 5] = [
    "memory_search",
    "memory_read",
    "memory_source",
    "memory_related",
    "memory_recall",
];
#[derive(Clone, Copy, Deserialize, Serialize)]
enum Operation {
    #[serde(rename = "memory_search")]
    Search,
    #[serde(rename = "memory_read")]
    Read,
    #[serde(rename = "memory_source")]
    Source,
    #[serde(rename = "memory_related")]
    Related,
    #[serde(rename = "memory_recall")]
    Recall,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Packet {
    schema_version: NextVersion,
    kind: String,
    capability: String,
    request_id: RequestId,
    operation: Operation,
    arguments: Value,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SearchArgs {
    filter: AgentSearchFilter,
    #[serde(deserialize_with = "nullable")]
    cursor: Option<MemoryReadCursor>,
    limit: usize,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    memory_id: MemoryId,
    #[serde(deserialize_with = "nullable")]
    expected: Option<SnapshotBinding>,
    include_source_text: bool,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SourceArgs {
    source_id: SourceId,
    #[serde(deserialize_with = "nullable")]
    revision: Option<Revision>,
    from_byte: usize,
    #[serde(deserialize_with = "nullable")]
    to_byte: Option<usize>,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RelatedArgs {
    memory_id: MemoryId,
    #[serde(deserialize_with = "nullable")]
    expected: Option<SnapshotBinding>,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Intent {
    Current,
    Historical,
    Browse,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecallArgs {
    text: String,
    conditions: Vec<String>,
    #[serde(deserialize_with = "nullable")]
    event_window: Option<FormationTime>,
    at: Timestamp,
    intent: Intent,
    limit: usize,
    max_bytes: usize,
}
/// Native per-connection read call accounting, never attention reinforcement,
/// Activity input, durable billing or a content log.
#[derive(Default)]
pub struct ReadCounts {
    pub search: u64,
    pub read: u64,
    pub source: u64,
    pub related: u64,
    pub recall: u64,
}
pub struct Server<'a, 'host> {
    reader: &'a AgentReader<'host>,
    profile: &'a AttentionProfile,
    phase: Phase,
    ids: BTreeSet<String>,
    pub counts: ReadCounts,
}
impl<'a, 'host> Server<'a, 'host> {
    /// Trusted embedding only; actual OS enrollment belongs to native adapter.
    pub fn new(reader: &'a AgentReader<'host>, profile: &'a AttentionProfile) -> Option<Self> {
        if !profile.validate().is_empty() {
            return None;
        }
        Some(Self {
            reader,
            profile,
            phase: Phase::New,
            ids: BTreeSet::new(),
            counts: ReadCounts::default(),
        })
    }
    pub fn closed(&self) -> bool {
        self.phase == Phase::Closed
    }
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        if message.to_string().len() > MAX_INPUT_BYTES
            || !closed(message, &["jsonrpc", "id", "method", "params"])
            || message["jsonrpc"] != "2.0"
            || !message["method"].is_string()
        {
            return Some(error(Value::Null, -32600, "invalid_request"));
        }
        let method = message["method"].as_str().expect("checked");
        let params = message.get("params").cloned().unwrap_or_else(|| json!({}));
        let Some(id) = message.get("id") else {
            if method == "notifications/initialized"
                && self.phase == Phase::Initializing
                && closed(&params, &["_meta"])
            {
                self.phase = Phase::Ready;
            }
            return None;
        };
        if !id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128)
            && !id
                .as_i64()
                .is_some_and(|i| i.unsigned_abs() <= MAX_SAFE_INTEGER)
        {
            return Some(error(Value::Null, -32600, "invalid_request_id"));
        }
        let id = id.clone();
        if self.closed() {
            return Some(error(id, -32000, "session_closed"));
        }
        if self.ids.len() >= MAX_REQUESTS {
            self.phase = Phase::Closed;
            return Some(error(id, -32000, "session_request_limit"));
        }
        if !self.ids.insert(id.to_string()) {
            return Some(error(id, -32600, "duplicate_request_id"));
        }
        if method == "ping" && closed(&params, &["_meta"]) {
            return Some(reply(id, json!({})));
        }
        if method == "initialize" {
            if self.phase != Phase::New {
                return Some(error(id, -32600, "already_initialized"));
            }
            if !closed(
                &params,
                &["protocolVersion", "capabilities", "clientInfo", "_meta"],
            ) || !params["protocolVersion"].is_string()
                || !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                return Some(error(id, -32602, "invalid_initialize"));
            }
            self.phase = Phase::Initializing;
            return Some(reply(
                id,
                json!({"protocolVersion":PROTOCOL_VERSION,"capabilities":{"tools":{}},"serverInfo":{"name":"enouia-memory-agent-v2","version":env!("CARGO_PKG_VERSION")}}),
            ));
        }
        if self.phase != Phase::Ready {
            return Some(error(id, -32000, "not_initialized"));
        }
        match method {
            "tools/list" => {
                if !closed(&params, &["_meta"]) {
                    return Some(error(id, -32602, "invalid_tool_cursor"));
                }
                let tools:Vec<_>=TOOLS.iter().map(|name|json!({"name":name,"description":"Scoped automation-root read. Attributed statements are subject statements, not verified facts; staged remains automatic reassessment. Unsupported UI fields are declared in coverage.","inputSchema":schema(name,false).expect("tool"),"outputSchema":schema(name,true).expect("tool"),"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})).collect();
                Some(reply(id, json!({"tools":tools})))
            }
            "tools/call" => Some(self.call(id, &params)),
            _ => Some(error(id, -32601, "method_not_found")),
        }
    }
    fn call(&mut self, id: Value, params: &Value) -> Value {
        if !closed(params, &["name", "arguments", "_meta"])
            || !TOOLS.contains(&params["name"].as_str().unwrap_or(""))
        {
            return error(id, -32602, "unknown_tool");
        }
        let Ok(packet) = serde_json::from_value::<Packet>(params["arguments"].clone()) else {
            return error(id, -32602, "unsupported_request");
        };
        let _version = packet.schema_version;
        if packet.kind != "agent_read_request"
            || packet.capability != CAPABILITY
            || serde_json::to_value(packet.operation).ok().as_ref() != Some(&params["name"])
        {
            return error(id, -32602, "unsupported_request");
        }
        let parsed = match packet.operation {
            Operation::Search => serde_json::from_value::<SearchArgs>(packet.arguments.clone())
                .map(|a| {
                    self.counts.search += 1;
                    self.reader
                        .search(
                            self.profile,
                            &a.filter,
                            a.cursor.as_ref(),
                            a.limit,
                            a.max_bytes,
                        )
                        .and_then(to_value)
                }),
            Operation::Read => {
                serde_json::from_value::<ReadArgs>(packet.arguments.clone()).map(|a| {
                    self.counts.read += 1;
                    self.reader
                        .detail(
                            self.profile,
                            &a.memory_id,
                            a.expected.as_ref(),
                            a.include_source_text,
                            a.max_bytes,
                        )
                        .and_then(to_value)
                })
            }
            Operation::Source => serde_json::from_value::<SourceArgs>(packet.arguments.clone())
                .map(|a| {
                    self.counts.source += 1;
                    self.reader
                        .source(
                            &a.source_id,
                            a.revision,
                            a.from_byte,
                            a.to_byte,
                            a.max_bytes,
                        )
                        .and_then(to_value)
                }),
            Operation::Related => serde_json::from_value::<RelatedArgs>(packet.arguments.clone())
                .map(|a| {
                    self.counts.related += 1;
                    self.reader
                        .related(self.profile, &a.memory_id, a.expected.as_ref(), a.max_bytes)
                        .and_then(to_value)
                }),
            Operation::Recall => serde_json::from_value::<RecallArgs>(packet.arguments.clone())
                .map(|a| {
                    self.counts.recall += 1;
                    let q = RecallQuery {
                        text: a.text,
                        conditions: a.conditions,
                        event_window: a.event_window,
                        at: a.at,
                        intent: match a.intent {
                            Intent::Current => RecallIntent::Current,
                            Intent::Historical => RecallIntent::Historical,
                            Intent::Browse => RecallIntent::Browse,
                        },
                        limit: a.limit,
                        max_bytes: a.max_bytes,
                    };
                    self.reader
                        .recall(self.profile, &q, packet.request_id.clone())
                        .and_then(to_value)
                }),
        };
        let Ok(result) = parsed else {
            return error(id, -32602, "invalid_arguments");
        };
        let (data, fault) = match result {
            Ok(data) => (data, Value::Null),
            Err(fault) => {
                let code = match fault {
                    ApplicationFault::Invalid => "invalid_request",
                    ApplicationFault::Vault(AutomationFault::Stale) => "stale_scope",
                    ApplicationFault::Locked | ApplicationFault::Vault(AutomationFault::Denied) => {
                        // A known per-tool refusal does not revoke a still-live
                        // connection (for example, no source:read permission).
                        if self.reader.check_connection().is_err() {
                            self.phase = Phase::Closed;
                        }
                        "permission_denied"
                    }
                    _ => "unavailable",
                };
                (Value::Null, json!({"code":code,"retryable":false}))
            }
        };
        let failed = !fault.is_null();
        let value = json!({"schema_version":2,"kind":"agent_read_response","capability":CAPABILITY,"request_id":packet.request_id,"operation":packet.operation,"principal":self.reader.principal(),"scope_hash":self.reader.scope_hash(),"data":data,"error":fault,"coverage":schemas::coverage()});
        if value.to_string().len() > MAX_OUTPUT_BYTES / 3 {
            self.phase = Phase::Closed;
            return error(id, -32000, "output_budget");
        }
        reply(id, tool_result(value, failed))
    }
}
fn to_value<T: Serialize>(value: T) -> Result<Value, ApplicationFault> {
    serde_json::to_value(value).map_err(|_| ApplicationFault::Invalid)
}

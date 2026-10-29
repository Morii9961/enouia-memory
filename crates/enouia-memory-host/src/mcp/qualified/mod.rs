//! Explicit Owner-root Agent MCP reads; no System forwarding or writer.
mod schemas;
pub use schemas::schema;
#[cfg(windows)]
pub mod native;
use super::{MAX_OUTPUT_BYTES, closed, error, reply, tool_result};
use crate::automation::application::ApplicationFault;
use crate::automation::qualified_agent::{QualifiedAgentReader, QualifiedHistoryCursor};
pub const CAPABILITY: &str = "memory-agent-qualified-read-1";
use enouia_memory_contract::{
    ids::{MemoryId, RequestId, SourceId},
    json::{Revision, nullable},
    next::*,
    time::Timestamp,
};
use enouia_memory_vault::automation::AutomationFault;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
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
    include_facets: bool,
    filter: CatalogFilter,
    #[serde(deserialize_with = "nullable")]
    cursor: Option<CatalogCursor>,
    limit: usize,
    max_bytes: usize,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ReadArgs {
    #[serde(deserialize_with = "nullable")]
    history_cursor: Option<QualifiedHistoryCursor>,
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
    reader: &'a QualifiedAgentReader<'host>,
    profile: &'a AttentionProfile,
    session: super::read_session::ReadSession,
    pub counts: ReadCounts,
}
impl<'a, 'host> Server<'a, 'host> {
    /// Trusted embedding only; actual OS enrollment belongs to native adapter.
    pub fn new(
        reader: &'a QualifiedAgentReader<'host>,
        profile: &'a AttentionProfile,
    ) -> Option<Self> {
        if !profile.validate().is_empty() || reader.check_connection().is_err() {
            return None;
        }
        Some(Self {
            reader,
            profile,
            session: super::read_session::ReadSession::new(
                "enouia-memory-agent-qualified-v1",
                4096,
            ),
            counts: ReadCounts::default(),
        })
    }
    pub fn closed(&self) -> bool {
        self.session.closed()
    }
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        use super::read_session::Route;
        if self.reader.check_connection().is_err() {
            self.session.close();
        }
        let (id, method, params) = match self.session.route(message) {
            Route::Response(response) => return response,
            Route::Ready { id, method, params } => (id, method, params),
        };
        match method.as_str() {
            "tools/list" => {
                if !closed(&params, &["_meta"]) {
                    return Some(error(id, -32602, "invalid_tool_cursor"));
                }
                let tools:Vec<_>=TOOLS.iter().map(|name|json!({"name":name,"description":"Scoped automation-root read. Attributed statements are subject statements, not verified facts; staged remains automatic reassessment. Coverage is explicit. Recorded relations are not independent verification. Theme suggestions are not facts. Same-conversation and semantic neighbors are unavailable. Native Owner proofs do not permit Owner actions. Reads do not reinforce attention.","inputSchema":schemas::schema(name,false).expect("tool"),"outputSchema":schemas::schema(name,true).expect("tool"),"annotations":{"readOnlyHint":true,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}})).collect();
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
        if packet.kind != "agent_qualified_read_request"
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
                        .page(
                            self.profile,
                            &a.filter,
                            a.cursor.as_ref(),
                            a.limit,
                            a.max_bytes,
                            a.include_facets,
                        )
                        .and_then(to_value)
                }),
            Operation::Read => {
                serde_json::from_value::<ReadArgs>(packet.arguments.clone()).map(|a| {
                    self.counts.read += 1;
                    self.reader
                        .detail_page(
                            self.profile,
                            &a.memory_id,
                            a.expected.as_ref(),
                            a.include_source_text,
                            a.max_bytes,
                            a.history_cursor.as_ref(),
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
                            self.session.close();
                        }
                        "permission_denied"
                    }
                    _ => "unavailable",
                };
                (Value::Null, json!({"code":code,"retryable":false}))
            }
        };
        let failed = !fault.is_null();
        let value = json!({"schema_version":2,"kind":"agent_qualified_read_response","capability":CAPABILITY,"request_id":packet.request_id,"operation":packet.operation,"principal":self.reader.principal(),"scope_hash":self.reader.scope_hash(),"data":data,"error":fault,"coverage":schemas::coverage()});
        if value.to_string().len() > MAX_OUTPUT_BYTES / 3 {
            self.session.close();
            return error(id, -32000, "output_budget");
        }
        reply(id, tool_result(value, failed))
    }
}
fn to_value<T: Serialize>(value: T) -> Result<Value, ApplicationFault> {
    serde_json::to_value(value).map_err(|_| ApplicationFault::Invalid)
}

//! Restricted MCP 2025-11-25 boundary (ADR-MEM-80).
//! Opt-in native enrollment lives in `native`; no owner bridge or automatic retry.
pub mod catalog;
pub(crate) mod json;
#[cfg(windows)]
pub mod native;
pub mod quote_locator;
mod read;
mod read_session;
pub mod v2;
pub use read::{HostPort, ReadPort};

use enouia_memory_contract::common::{ActorRef, ActorType};
use enouia_memory_contract::ipc::{self, Arguments, IpcRequest, Operation};
use enouia_memory_contract::json::MAX_SAFE_INTEGER;
use enouia_memory_contract::policy::Scope;
use serde_json::{Value, json};
use std::collections::BTreeSet;
use std::io::{self, BufRead, Read, Write};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

pub const PROTOCOL_VERSION: &str = "2025-11-25";
pub const MAX_INPUT_BYTES: usize = 256 * 1024;
pub const MAX_OUTPUT_BYTES: usize = 1024 * 1024;
pub const MAX_REQUESTS: usize = 256;
const TOOLS: [(&str, Operation); 6] = [
    ("context_get", Operation::ContextGet),
    ("memory_search", Operation::MemorySearch),
    ("memory_read", Operation::MemoryRead),
    ("memory_source", Operation::MemorySource),
    ("memory_propose", Operation::MemoryPropose),
    ("session_checkpoint", Operation::SessionCheckpoint),
];

/// Native-only scope ceiling, not an authentication token or a record policy.
/// Construct only after separate native enrollment. Clones share revocation.
#[derive(Clone)]
pub struct Grant {
    principal: ActorRef,
    scopes: Vec<Scope>,
    active: Arc<AtomicBool>,
}
impl Grant {
    pub fn from_authenticated_agent(principal: ActorRef, scopes: Vec<Scope>) -> Option<Self> {
        if principal.actor_type != ActorType::Agent
            || scopes
                .iter()
                .any(|s| !TOOLS.iter().any(|(_, op)| op.required_scope() == *s))
        {
            return None;
        }
        Some(Self {
            principal,
            scopes,
            active: Arc::new(AtomicBool::new(true)),
        })
    }
    pub fn revoke(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
    fn allows(&self, op: Operation) -> bool {
        self.active.load(Ordering::SeqCst) && self.scopes.contains(&op.required_scope())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PortFault {
    /// Known refusal before application dispatch.
    Unavailable,
    /// A write may be durable; never reconnect or replay automatically.
    OutcomeUnknown,
}
/// The embedding adapter must enforce current per-record/project/sensitivity/
/// destination policy, native authority and principal-scoped durable receipts.
/// Never implement this by forwarding to the owner's Workspace unchanged.
pub trait AgentPort {
    fn call(
        &mut self,
        principal: &ActorRef,
        request: &IpcRequest,
        arguments: Arguments,
    ) -> Result<Value, PortFault>;
    fn disconnect(&mut self);
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum Phase {
    New,
    Initializing,
    Ready,
    Closed,
}

pub struct Server<P: AgentPort> {
    grant: Grant,
    port: P,
    phase: Phase,
    ids: BTreeSet<String>,
}
fn closed(value: &Value, keys: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|v| v.keys().all(|k| keys.contains(&k.as_str())))
}
fn error(id: Value, code: i32, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}
fn reply(id: Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}
fn tool_result(value: Value, is_error: bool) -> Value {
    json!({"content":[{"type":"text","text":value.to_string()}],"structuredContent":value,"isError":is_error})
}
fn tool_error(code: &str, original: Option<&Value>) -> Value {
    let mut result = json!({"error":{"code":code,"retryable":false}});
    if let Some(original) = original {
        result["originalRequest"] = original.clone();
    }
    tool_result(result, true)
}

/// Closed input schema copied from the existing Memory IPC contract. All
/// references are to bundled common definitions; clients need no file access.
pub fn input_schema(name: &str) -> Option<Value> {
    if !TOOLS.iter().any(|(n, _)| *n == name) && name != "memory_update" {
        return None;
    }
    let ipc: Value = serde_json::from_str(include_str!(
        "../../../../contracts/ipc/memory-v1.schema.json"
    ))
    .expect("frozen IPC schema");
    let common: Value = serde_json::from_str(include_str!(
        "../../../../contracts/memory/common-v1.schema.json"
    ))
    .expect("frozen common schema");
    let mut schema = ipc["$defs"][format!("request_{name}")].clone();
    fn refs(v: &mut Value) {
        match v {
            Value::Object(map) => {
                if let Some(Value::String(r)) = map.get_mut("$ref") {
                    *r = r.replace("../memory/common-v1.schema.json#", "#");
                }
                for v in map.values_mut() {
                    refs(v);
                }
            }
            Value::Array(items) => {
                for v in items {
                    refs(v);
                }
            }
            _ => {}
        }
    }
    refs(&mut schema);
    schema["$defs"] = common["$defs"].clone();
    schema["$schema"] = json!("https://json-schema.org/draft/2020-12/schema");
    Some(schema)
}

impl<P: AgentPort> Server<P> {
    pub fn new(grant: Grant, port: P) -> Self {
        Self {
            grant,
            port,
            phase: Phase::New,
            ids: BTreeSet::new(),
        }
    }
    pub fn disconnect(&mut self) {
        if self.phase != Phase::Closed {
            self.phase = Phase::Closed;
            self.grant.revoke();
            self.port.disconnect();
        }
    }
    /// Handle one already bounded JSON message. The stdio driver additionally
    /// rejects duplicate fields and caps raw bytes before parsing.
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        if message.to_string().len() > MAX_INPUT_BYTES {
            return Some(error(Value::Null, -32600, "message_budget"));
        }
        if !closed(message, &["jsonrpc", "id", "method", "params"])
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
            // Notifications never dispatch tools, enroll or reply.
            return None;
        };
        let valid_id = match id {
            Value::String(s) => !s.is_empty() && s.len() <= 128,
            Value::Number(n) => n
                .as_i64()
                .is_some_and(|n| n.unsigned_abs() <= MAX_SAFE_INTEGER),
            _ => false,
        };
        if !valid_id {
            return Some(error(Value::Null, -32600, "invalid_request_id"));
        }
        let id = id.clone();
        if self.phase == Phase::Closed {
            return Some(error(id, -32000, "session_closed"));
        }
        if self.ids.len() >= MAX_REQUESTS {
            self.disconnect();
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
                || !params["clientInfo"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                return Some(error(id, -32602, "invalid_initialize"));
            }
            self.phase = Phase::Initializing;
            // Version negotiation replies with our pinned supported version.
            return Some(reply(
                id,
                json!({"protocolVersion":PROTOCOL_VERSION,
                "capabilities":{"tools":{}},
                "serverInfo":{"name":"enouia-memory-mcp-boundary","version":env!("CARGO_PKG_VERSION")}}),
            ));
        }
        if self.phase != Phase::Ready {
            return Some(error(id, -32000, "not_initialized"));
        }
        if !self.grant.active.load(Ordering::SeqCst) {
            self.disconnect();
            return Some(error(id, -32001, "admission_revoked"));
        }
        match method {
            "tools/list" => {
                if !closed(&params, &["_meta"]) {
                    return Some(error(id, -32602, "invalid_tool_cursor"));
                }
                let tools: Vec<_> = TOOLS.iter().filter(|(_,op)| self.grant.allows(*op)).map(|(name,op)| {
                    json!({"name":name,"description":"Restricted Memory IPC v1 envelope; proposals remain pending and checkpoints provisional.",
                        "inputSchema":input_schema(name).expect("catalog"),
                        "annotations":{"readOnlyHint":!op.is_write(),"destructiveHint":false,"openWorldHint":false}})
                }).collect();
                Some(reply(id, json!({"tools":tools})))
            }
            "tools/call" => Some(self.call(id, &params)),
            _ => Some(error(id, -32601, "method_not_found")),
        }
    }
    fn call(&mut self, id: Value, params: &Value) -> Value {
        if !closed(params, &["name", "arguments", "_meta"]) || !params["arguments"].is_object() {
            return error(id, -32602, "invalid_tool_call");
        }
        let name = params["name"].as_str().unwrap_or("");
        let op = if name == "memory_update" {
            Operation::MemoryUpdate
        } else if let Some((_, op)) = TOOLS.iter().find(|(n, _)| *n == name) {
            *op
        } else {
            return error(id, -32602, "unknown_tool");
        };
        if !self.grant.allows(op) {
            return reply(id, tool_error("permission_denied", None));
        }
        let original = &params["arguments"];
        let Ok((request, arguments)) = ipc::parse_request(original) else {
            return reply(id, tool_error("invalid_request", None));
        };
        if request.operation != op {
            return reply(id, tool_error("tool_operation_mismatch", None));
        }
        match self.port.call(&self.grant.principal, &request, arguments) {
            Ok(value) => {
                let valid = ipc::validate_response(op, &value).is_ok()
                    && value["requestId"] == original["requestId"]
                    && value.to_string().len() <= MAX_OUTPUT_BYTES / 2 - MAX_INPUT_BYTES;
                if !valid {
                    self.disconnect();
                    return reply(id, tool_error("outcome_unknown", Some(original)));
                }
                let failed = value["kind"] == "memory_error";
                reply(id, tool_result(value, failed))
            }
            Err(PortFault::Unavailable) => reply(id, tool_error("backend_unavailable", None)),
            Err(PortFault::OutcomeUnknown) => {
                self.disconnect();
                reply(id, tool_error("outcome_unknown", Some(original)))
            }
        }
    }
    /// Bounded newline-delimited UTF-8 JSON-RPC. EOF, framing failure and output
    /// failure revoke this session. No banners, diagnostics or input logs.
    pub fn serve(&mut self, input: &mut impl BufRead, output: &mut impl Write) -> io::Result<()> {
        let result = self.serve_inner(input, output);
        self.disconnect();
        result
    }
    fn serve_inner(&mut self, input: &mut impl BufRead, output: &mut impl Write) -> io::Result<()> {
        loop {
            let mut line = Vec::new();
            let read = (&mut *input)
                .take((MAX_INPUT_BYTES + 2) as u64)
                .read_until(b'\n', &mut line)?;
            if read == 0 {
                return Ok(());
            }
            if line.last() != Some(&b'\n') || line.len() > MAX_INPUT_BYTES + 1 {
                return Err(io::Error::from(io::ErrorKind::InvalidData));
            }
            line.pop();
            let response = match json::parse(&line) {
                Ok(message) => self.handle(&message),
                Err(_) => Some(error(Value::Null, -32700, "parse_error")),
            };
            if let Some(response) = response {
                let bytes = response.to_string();
                if bytes.len() > MAX_OUTPUT_BYTES {
                    return Err(io::Error::from(io::ErrorKind::InvalidData));
                }
                output.write_all(bytes.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
            }
            if self.phase == Phase::Closed {
                return Ok(());
            }
        }
    }
}
impl<P: AgentPort> Drop for Server<P> {
    fn drop(&mut self) {
        self.disconnect();
    }
}

//! Explicit correction-report MCP boundary. No old reader grants write authority.
use super::{
    closed, error, initialized,
    read_session::{ReadSession, Route},
    reply, tool_result,
};
use crate::automation::{
    application::ApplicationFault,
    qualified_agent::{AgentCorrectionReporter, QualifiedAgentReader},
};
use enouia_memory_contract::next::AttentionProfile;
use enouia_memory_vault::automation::{AutomationFault, segmented::AgentCorrectionRequest};
use serde::Deserialize;
use serde_json::{Value, json};
#[cfg(windows)]
pub mod native;

pub const CAPABILITY: &str = "memory-agent-correction-1";
const DOCUMENT: &str =
    include_str!("../../../../../contracts/next/agent-correction-mcp-v1.schema.json");
pub fn schema(output: bool) -> Value {
    let mut documents = super::v2::schemas::documents();
    documents.insert("agent-correction-mcp-v1.schema.json", DOCUMENT);
    documents.insert(
        "agent-correction-v1.schema.json",
        include_str!("../../../../../contracts/next/agent-correction-v1.schema.json"),
    );
    super::v2::schemas::bundle(
        documents,
        "agent-correction-mcp-v1.schema.json",
        "correction",
        output,
    )
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Packet {
    schema_version: enouia_memory_contract::next::NextVersion,
    kind: String,
    capability: String,
    request: AgentCorrectionRequest,
}
pub struct Server<'a, 'host> {
    reader: &'a QualifiedAgentReader<'host>,
    writer: &'a AgentCorrectionReporter<'host>,
    reads: initialized::Server<'a, 'host>,
    session: ReadSession,
    uncertain: bool,
    original: Option<Value>,
}
impl<'a, 'host> Server<'a, 'host> {
    /// Trusted native embedding only; OS peer enrollment belongs to transport.
    pub fn new(
        reader: &'a QualifiedAgentReader<'host>,
        writer: &'a AgentCorrectionReporter<'host>,
        profile: &'a AttentionProfile,
    ) -> Option<Self> {
        if reader.principal() != writer.principal()
            || !writer.shares_vault(reader)
            || writer.check_connection().is_err()
        {
            return None;
        }
        Some(Self {
            reader,
            writer,
            reads: initialized::Server::new(reader, profile)?,
            session: ReadSession::new("enouia-memory-agent-corrections-v1", 4096),
            uncertain: false,
            original: None,
        })
    }
    pub fn closed(&self) -> bool {
        self.session.closed()
    }
    pub fn outcome_unknown(&self) -> bool {
        self.uncertain
    }
    pub fn original_unknown(&self) -> Option<&Value> {
        self.original.as_ref()
    }
    pub fn handle(&mut self, message: &Value) -> Option<Value> {
        if let Err(fault) = self.writer.check_connection() {
            self.session.close();
            if matches!(
                fault,
                ApplicationFault::Vault(AutomationFault::OutcomeUnknown)
            ) {
                self.uncertain = true;
                return None;
            }
        }
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
                let mut tools = initialized::tools();
                tools.push(json!({"name":"memory_correction_report","description":"Save an untrusted correction report backed by a permitted Runtime user statement. Success means report_saved only, not a corrected memory, Owner approval or a review queue.","inputSchema":schema(false),"outputSchema":schema(true),"annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}}));
                Some(reply(id, json!({"tools":tools})))
            }
            "tools/call" if params["name"] == "memory_correction_report" => {
                if !closed(&params, &["name", "arguments", "_meta"]) {
                    return Some(error(id, -32602, "invalid_arguments"));
                }
                let Ok(packet) = serde_json::from_value::<Packet>(params["arguments"].clone())
                else {
                    return Some(error(id, -32602, "unsupported_request"));
                };
                let _version = packet.schema_version;
                if packet.kind != "agent_correction_request" || packet.capability != CAPABILITY {
                    return Some(error(id, -32602, "unsupported_request"));
                }
                let (receipt, fault) = match self.writer.submit(&packet.request) {
                    Ok(receipt) => (json!(receipt), Value::Null),
                    Err(ApplicationFault::Vault(AutomationFault::OutcomeUnknown)) => {
                        self.uncertain = true;
                        self.original = Some(message.clone());
                        self.session.close();
                        return None;
                    }
                    Err(fault) => {
                        let code = match fault {
                            ApplicationFault::Invalid => "invalid_request",
                            ApplicationFault::Locked
                            | ApplicationFault::Vault(AutomationFault::Denied) => {
                                "permission_denied"
                            }
                            ApplicationFault::Vault(AutomationFault::Conflict) => "conflict",
                            ApplicationFault::Vault(AutomationFault::Stale) => "stale_scope",
                            ApplicationFault::Vault(AutomationFault::Capacity) => "capacity",
                            ApplicationFault::Vault(AutomationFault::Busy) => "busy",
                            ApplicationFault::Vault(AutomationFault::RecoveryRequired) => {
                                "recovery_required"
                            }
                            _ => "unavailable",
                        };
                        if self.writer.check_connection().is_err() {
                            self.session.close();
                        }
                        (Value::Null, json!({"code":code,"retryable":false}))
                    }
                };
                let failed = !fault.is_null();
                let response = json!({"schema_version":2,"kind":"agent_correction_response","capability":CAPABILITY,"request_id":packet.request.request_id,"receipt":receipt,"error":fault});
                Some(reply(id, tool_result(response, failed)))
            }
            "tools/call" => {
                let response = self.reads.call(id, &params);
                if self.reads.closed() {
                    self.session.close();
                }
                Some(response)
            }
            _ => Some(error(id, -32601, "method_not_found")),
        }
    }
}

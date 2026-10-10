//! Provider port types. The M0 names are kept: `ProviderRequest`,
//! `ProviderResponse`, `ToolRequest`, `ToolResult`, `ProviderCapabilities`.
//! Request/response are private in-memory values (no Serialize), like the
//! common process types; the capability snapshot is a stored contract.

use crate::context::{
    Destination, DispatchRecord, DispatchTool, MessageRole, OutputConfig, request_payload_hash,
};
use crate::error::Violation;
use crate::hash::{Sha256Hex, sha256};
use crate::ids::{CapsuleId, DispatchId};
use crate::json::SchemaVersion;
use crate::session::ProviderBinding;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

/// Tri-state capability. `unknown` is never inferred from a model name.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Supported,
    Unsupported,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TokenCounting {
    Exact,
    Estimated,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderCapabilities {
    pub schema_version: SchemaVersion,
    pub binding: ProviderBinding,
    pub text_input: Capability,
    pub image_input: Capability,
    pub streaming: Capability,
    pub tool_calling: Capability,
    pub cancellation: Capability,
    pub token_counting: TokenCounting,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub context_window_tokens: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub max_output_tokens: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub verified_at: Option<Timestamp>,
}

impl ProviderCapabilities {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.verified_at.is_none() {
            let claims = [
                self.text_input,
                self.image_input,
                self.streaming,
                self.tool_calling,
                self.cancellation,
            ]
            .iter()
            .any(|c| *c != Capability::Unknown)
                || self.token_counting != TokenCounting::Unknown
                || self.context_window_tokens.is_some()
                || self.max_output_tokens.is_some();
            if claims {
                out.push(Violation::new(
                    "capabilities.unverified_claim",
                    "/verified_at",
                ));
            }
        }
        if let (Some(window), Some(output)) = (self.context_window_tokens, self.max_output_tokens)
            && output >= window
        {
            out.push(Violation::new("capabilities.limits", "/max_output_tokens"));
        }
        out
    }

    /// Automatic sending requires a verified exact or declared estimate and a
    /// known window; otherwise the caller must return `unsupported_budget`.
    pub fn budget_supported(&self) -> bool {
        self.verified_at.is_some()
            && self.token_counting != TokenCounting::Unknown
            && self.context_window_tokens.is_some()
    }
}

#[derive(Debug)]
pub struct ProviderMessage {
    pub role: MessageRole,
    pub text: String,
}

#[derive(Debug)]
pub struct ToolDefinition {
    pub name: String,
    pub schema_json: String,
}

/// Built only from a recorded capsule and its DispatchRecord; an adapter
/// receives nothing else from the Vault.
#[derive(Debug)]
pub struct ProviderRequest {
    pub dispatch_id: DispatchId,
    pub capsule_id: CapsuleId,
    pub destination: Destination,
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolDefinition>,
    pub output: OutputConfig,
}

impl ProviderRequest {
    /// SHA-256 of the canonical payload of *this* request (see
    /// `context::request_payload_hash`), computed from the actual bytes.
    pub fn payload_hash(&self) -> Sha256Hex {
        let hashes: Vec<Sha256Hex> = self
            .messages
            .iter()
            .map(|m| sha256(m.text.as_bytes()))
            .collect();
        let messages: Vec<(MessageRole, &Sha256Hex, u64)> = self
            .messages
            .iter()
            .zip(&hashes)
            .map(|(m, h)| (m.role, h, m.text.len() as u64))
            .collect();
        let tools: Vec<DispatchTool> = self
            .tools
            .iter()
            .map(|t| DispatchTool {
                name: t.name.clone(),
                definition_hash: sha256(t.schema_json.as_bytes()),
            })
            .collect();
        request_payload_hash(&self.destination, &messages, &tools, &self.output)
    }

    /// The final send gate: the actual request must be exactly what the
    /// DispatchRecord (and any approval bound to its `request_hash`) describes.
    /// Every message's content hash, size, role and order, every tool's name and
    /// definition hash, the output configuration and the destination are
    /// compared. An empty result means the request may be sent.
    pub fn verify_against(&self, dispatch: &DispatchRecord) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.dispatch_id != dispatch.dispatch_id || self.capsule_id != dispatch.capsule_id {
            out.push(Violation::new("provider_request.ids", "/dispatch_id"));
        }
        if self.destination != dispatch.destination {
            out.push(Violation::new(
                "provider_request.destination",
                "/destination",
            ));
        }
        if self.output != dispatch.output {
            out.push(Violation::new("provider_request.output", "/output"));
        }
        if self.messages.len() != dispatch.messages.len() {
            out.push(Violation::new("provider_request.messages", "/messages"));
        }
        for (index, (actual, recorded)) in self.messages.iter().zip(&dispatch.messages).enumerate()
        {
            if actual.role != recorded.role
                || actual.text.len() as u64 != recorded.size_bytes
                || sha256(actual.text.as_bytes()) != recorded.content_hash
            {
                out.push(Violation::new(
                    "provider_request.message_content",
                    format!("/messages/{index}"),
                ));
            }
        }
        if self.tools.len() != dispatch.tools.len()
            || self
                .tools
                .iter()
                .zip(&dispatch.tools)
                .any(|(actual, recorded)| {
                    actual.name != recorded.name
                        || sha256(actual.schema_json.as_bytes()) != recorded.definition_hash
                })
        {
            out.push(Violation::new("provider_request.tools", "/tools"));
        }
        if self.payload_hash() != dispatch.request_hash {
            out.push(Violation::new(
                "provider_request.payload_hash",
                "/request_hash",
            ));
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinishReason {
    Completed,
    Length,
    Cancelled,
    Failed,
}

#[derive(Debug)]
pub struct ToolRequest {
    pub tool_call_id: String,
    pub name: String,
    pub arguments_json: String,
}

#[derive(Debug)]
pub struct ToolResult {
    pub tool_call_id: String,
    pub content: String,
    pub is_error: bool,
}

/// A response is a candidate for a session event, never a memory write.
#[derive(Debug)]
pub struct ProviderResponse {
    pub text: String,
    pub finish: FinishReason,
    pub tool_requests: Vec<ToolRequest>,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

/// Durable MV-7 state in the owning Session header, without wire text or keys.
/// DispatchRecord remains immutable. An admitted external Dispatch snapshot
/// is OutcomeUnknown; this ledger records the subsequently learned outcome.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationState {
    OutcomeUnknown,
    Completed,
    Length,
    Cancelled,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Invocation {
    pub dispatch_id: DispatchId,
    pub input_event_id: crate::ids::EventId,
    pub wire_hash: Sha256Hex,
    pub wire_size_bytes: u64,
    pub reserved_tokens: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub reserved_cost_microusd: Option<u64>,
    pub state: InvocationState,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub terminal_event_id: Option<crate::ids::EventId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub input_tokens: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub output_tokens: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub error_code: Option<crate::error::MemoryErrorCode>,
}
impl Invocation {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = vec![];
        if self.reserved_tokens == 0
            || self.wire_size_bytes == 0
            || self.wire_size_bytes > 2 * 1024 * 1024
            || (self.state == InvocationState::OutcomeUnknown) != self.terminal_event_id.is_none()
            || (self.state == InvocationState::Completed || self.state == InvocationState::Length)
                && self.error_code.is_some()
        {
            out.push(Violation::new("invocation.state", "/provider_invocations"));
        }
        out
    }
}

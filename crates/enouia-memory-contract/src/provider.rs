//! Provider port types. The M0 names are kept: `ProviderRequest`,
//! `ProviderResponse`, `ToolRequest`, `ToolResult`, `ProviderCapabilities`.
//! Request/response are private in-memory values (no Serialize), like the
//! common process types; the capability snapshot is a stored contract.

use crate::context::{DispatchRecord, MessageRole};
use crate::error::Violation;
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
    pub messages: Vec<ProviderMessage>,
    pub tools: Vec<ToolDefinition>,
    pub max_output_tokens: u64,
}

impl ProviderRequest {
    /// Message count and roles must match the dispatch record exactly.
    pub fn matches_dispatch(&self, dispatch: &DispatchRecord) -> bool {
        self.dispatch_id == dispatch.dispatch_id
            && self.capsule_id == dispatch.capsule_id
            && self.messages.len() == dispatch.messages.len()
            && self
                .messages
                .iter()
                .zip(&dispatch.messages)
                .all(|(m, d)| m.role == d.role && m.text.len() as u64 == d.size_bytes)
            && self.tools.len() == dispatch.tools.len()
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

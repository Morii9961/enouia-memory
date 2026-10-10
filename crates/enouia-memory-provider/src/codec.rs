//! One canonical wire encoding, shared by inspection and HTTP transmission.
use crate::{Result, error};
use enouia_memory_contract::{
    MemoryErrorCode,
    context::{DestinationKind, MessageRole},
    hash::{Sha256Hex, sha256},
    json::canonical_bytes,
    provider::{FinishReason, ProviderRequest, ProviderResponse},
    scan::{contains_local_path, contains_secret_material},
    session::ProviderBinding,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::fmt;

pub const ADAPTER_VERSION: &str = "text-http-1";
pub const ANTHROPIC_VERSION: &str = "2023-06-01";
pub const MAX_BODY_BYTES: usize = 2 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Api {
    OpenAiResponses,
    AnthropicMessages,
}
impl Api {
    pub const fn provider(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "openai",
            Self::AnthropicMessages => "anthropic",
        }
    }
    pub const fn endpoint(self) -> &'static str {
        match self {
            Self::OpenAiResponses => "https://api.openai.com/v1/responses",
            Self::AnthropicMessages => "https://api.anthropic.com/v1/messages",
        }
    }
    pub fn binding(self, model: &str) -> Result<ProviderBinding> {
        if model.is_empty()
            || model.len() > 160
            || !model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
        {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        Ok(ProviderBinding {
            provider: self.provider().into(),
            model: model.into(),
            adapter_version: ADAPTER_VERSION.into(),
        })
    }
}

/// Private text is only exposed through explicit inspection. Debug is redacted.
pub struct WireRequest {
    pub(crate) api: Api,
    pub(crate) body: Vec<u8>,
    pub(crate) binding: ProviderBinding,
    pub(crate) logical_hash: Sha256Hex,
    pub(crate) streaming: bool,
}
impl fmt::Debug for WireRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("WireRequest")
            .field("api", &self.api)
            .field("bytes", &self.body.len())
            .finish_non_exhaustive()
    }
}
impl WireRequest {
    pub fn endpoint(&self) -> &'static str {
        self.api.endpoint()
    }
    pub fn body(&self) -> &[u8] {
        &self.body
    }
    pub fn hash(&self) -> Sha256Hex {
        sha256(&self.body)
    }
    pub fn binding(&self) -> &ProviderBinding {
        &self.binding
    }
    pub fn logical_hash(&self) -> &Sha256Hex {
        &self.logical_hash
    }
}

pub fn encode(api: Api, request: &ProviderRequest) -> Result<WireRequest> {
    let binding = request
        .destination
        .provider_binding
        .as_ref()
        .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?;
    if request.destination.kind != DestinationKind::ExternalProvider
        || &api.binding(&binding.model)? != binding
        || request.messages.is_empty()
        || !request.tools.is_empty()
        || !(16..=1_000_000).contains(&request.output.max_output_tokens)
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    for m in &request.messages {
        if matches!(m.role, MessageRole::Tool)
            || contains_secret_material(&m.text)
            || contains_local_path(&m.text)
        {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
    }
    let body = match api {
        Api::OpenAiResponses => {
            let input: Vec<_> = request.messages.iter().map(|m| json!({
                "role": match m.role { MessageRole::System => "system", MessageRole::User => "user", MessageRole::Assistant => "assistant", MessageRole::Tool => unreachable!() },
                "content": m.text,
            })).collect();
            json!({"model":binding.model,"input":input,"max_output_tokens":request.output.max_output_tokens,
                "stream":request.output.streaming,"store":false,"truncation":"disabled"})
        }
        Api::AnthropicMessages => {
            let mut system = vec![];
            let mut messages = vec![];
            for m in &request.messages {
                if m.role == MessageRole::System {
                    // System text cannot be silently reordered past user/assistant text.
                    if !messages.is_empty() {
                        return Err(error(MemoryErrorCode::InvalidRequest));
                    }
                    system.push(json!({"type":"text","text":m.text}));
                } else {
                    messages.push(json!({"role":if m.role == MessageRole::User {"user"} else {"assistant"},"content":[{"type":"text","text":m.text}]}));
                }
            }
            if messages.is_empty() || messages[0]["role"] != "user" {
                return Err(error(MemoryErrorCode::InvalidRequest));
            }
            json!({"model":binding.model,"system":system,"messages":messages,
                "max_tokens":request.output.max_output_tokens,"stream":request.output.streaming})
        }
    };
    let body = canonical_bytes(&body).map_err(|_| error(MemoryErrorCode::InvalidRequest))?;
    if body.len() > MAX_BODY_BYTES {
        return Err(error(MemoryErrorCode::BudgetExceeded));
    }
    Ok(WireRequest {
        api,
        body,
        binding: binding.clone(),
        logical_hash: request.payload_hash(),
        streaming: request.output.streaming,
    })
}

pub fn decode(api: Api, body: &[u8]) -> Result<ProviderResponse> {
    let v: Value =
        serde_json::from_slice(body).map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
    let mut text = String::new();
    let (finish, usage) = match api {
        Api::OpenAiResponses => {
            let finish = match v["status"].as_str() {
                Some("completed") => FinishReason::Completed,
                Some("incomplete") if v["incomplete_details"]["reason"] == "max_output_tokens" => {
                    FinishReason::Length
                }
                _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
            };
            for item in v["output"]
                .as_array()
                .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?
            {
                match item["type"].as_str() {
                    Some("message") if item["role"] == "assistant" => {
                        for part in item["content"]
                            .as_array()
                            .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?
                        {
                            match part["type"].as_str() {
                                Some("output_text") => {
                                    text.push_str(part["text"].as_str().ok_or_else(|| {
                                        error(MemoryErrorCode::ProviderUnavailable)
                                    })?)
                                }
                                Some("refusal") => {
                                    text.push_str(part["refusal"].as_str().ok_or_else(|| {
                                        error(MemoryErrorCode::ProviderUnavailable)
                                    })?)
                                }
                                _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
                            }
                        }
                    }
                    Some("reasoning") => {}
                    _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
                }
            }
            (finish, &v["usage"])
        }
        Api::AnthropicMessages => {
            if v["type"] != "message" || v["role"] != "assistant" {
                return Err(error(MemoryErrorCode::ProviderUnavailable));
            }
            for part in v["content"]
                .as_array()
                .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?
            {
                match part["type"].as_str() {
                    Some("text") => text.push_str(
                        part["text"]
                            .as_str()
                            .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?,
                    ),
                    // Reasoning from always-on thinking models is validated
                    // but never returned or stored, like OpenAI reasoning items.
                    Some("thinking")
                        if part["thinking"].is_string() && part["signature"].is_string() => {}
                    Some("redacted_thinking") if part["data"].is_string() => {}
                    _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
                }
            }
            let finish = match v["stop_reason"].as_str() {
                Some("end_turn" | "stop_sequence") => FinishReason::Completed,
                // Documented as a truncated response, like max_tokens.
                Some("max_tokens" | "model_context_window_exceeded") => FinishReason::Length,
                _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
            };
            (finish, &v["usage"])
        }
    };
    let (input_tokens, output_tokens) = decode_usage(api, usage)?;
    Ok(ProviderResponse {
        text,
        finish,
        tool_requests: vec![],
        input_tokens,
        output_tokens,
    })
}

/// Normalize only reported counts, without guessing missing usage or price.
/// OpenAI cache details are a subset of input_tokens; Anthropic reports cache
/// creation/read separately from uncached input. Nested creation details are
/// another breakdown and must not be added again.
pub(crate) fn decode_usage(api: Api, usage: &Value) -> Result<(Option<u64>, Option<u64>)> {
    if usage.is_null() {
        return Ok((None, None));
    }
    if !usage.is_object() {
        return Err(error(MemoryErrorCode::ProviderUnavailable));
    }
    let count = |key: &str| -> Result<Option<u64>> {
        match usage.get(key) {
            None | Some(Value::Null) => Ok(None),
            Some(value) => value
                .as_u64()
                .filter(|n| *n <= enouia_memory_contract::json::MAX_SAFE_INTEGER)
                .map(Some)
                .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable)),
        }
    };
    let mut input = count("input_tokens")?;
    if api == Api::AnthropicMessages {
        for key in ["cache_creation_input_tokens", "cache_read_input_tokens"] {
            let cached = count(key)?.unwrap_or(0);
            if let Some(known) = input {
                input = Some(
                    known
                        .checked_add(cached)
                        .filter(|n| *n <= enouia_memory_contract::json::MAX_SAFE_INTEGER)
                        .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?,
                );
            }
        }
    }
    Ok((input, count("output_tokens")?))
}

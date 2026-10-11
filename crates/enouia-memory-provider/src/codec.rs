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
    decode_observing(api, body, &mut (None, None))
}

/// Keep validated message usage even when its stop reason refuses publication.
pub(crate) fn decode_observing(
    api: Api,
    body: &[u8],
    observed: &mut (Option<u64>, Option<u64>),
) -> Result<ProviderResponse> {
    let v: Value =
        serde_json::from_slice(body).map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
    let mut text = String::new();
    let (finish, usage) = match api {
        Api::OpenAiResponses => {
            if v["status"] == "failed"
                || v["status"] == "incomplete"
                    && matches!(
                        v["incomplete_details"]["reason"].as_str(),
                        Some("content_filter" | "max_messages" | "steered")
                    )
            {
                // A rejected terminal Response can report usage, unlike an
                // arbitrary HTTP error body. Its error is nullable; validate
                // an error when present and all supported output before
                // retaining the complete tuple. Never expose terminal text.
                *observed = openai_reported_usage(&v)?;
                return Err(error(MemoryErrorCode::ProviderUnavailable));
            }
            let finish = match v["status"].as_str() {
                Some("completed") => FinishReason::Completed,
                Some("incomplete") if v["incomplete_details"]["reason"] == "max_output_tokens" => {
                    FinishReason::Length
                }
                _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
            };
            text = openai_output_text(&v)?;
            (finish, decode_usage(api, &v["usage"])?)
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
            // Validate the complete usage tuple before retaining either count.
            // The envelope/content above must also be valid; arbitrary error
            // bodies with a usage-shaped field are not message evidence.
            let counts = decode_usage(api, &v["usage"])?;
            *observed = counts;
            let finish = match v["stop_reason"].as_str() {
                Some("end_turn" | "stop_sequence") => FinishReason::Completed,
                // Documented as a truncated response, like max_tokens.
                Some("max_tokens" | "model_context_window_exceeded") => FinishReason::Length,
                _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
            };
            (finish, counts)
        }
    };
    let (input_tokens, output_tokens) = usage;
    *observed = usage;
    Ok(ProviderResponse {
        text,
        finish,
        tool_requests: vec![],
        input_tokens,
        output_tokens,
    })
}

/// Validate Response identity, nullable error and supported output before usage.
/// Callers separately constrain the event/status and admission identity.
pub(crate) fn openai_reported_usage(response: &Value) -> Result<(Option<u64>, Option<u64>)> {
    openai_output_text(response)?;
    decode_usage(Api::OpenAiResponses, &response["usage"])
}

fn openai_output_text(response: &Value) -> Result<String> {
    if response["object"] != "response"
        || response["id"].as_str().is_none_or(str::is_empty)
        || !response["error"].is_null()
            && (response["error"]["code"].as_str().is_none_or(str::is_empty)
                || !response["error"]["message"].is_string())
    {
        return Err(error(MemoryErrorCode::ProviderUnavailable));
    }
    let mut text = String::new();
    for item in response["output"]
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
                        Some("output_text") => text.push_str(
                            part["text"]
                                .as_str()
                                .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?,
                        ),
                        Some("refusal") => text.push_str(
                            part["refusal"]
                                .as_str()
                                .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?,
                        ),
                        _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
                    }
                }
            }
            Some("reasoning") => {}
            _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
        }
    }
    Ok(text)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn openai_terminal_failures_retain_only_validated_json_usage() {
        let base = json!({"id":"resp_synthetic_failed","object":"response","status":"failed",
            "error":{"code":"server_error","message":"Synthetic failure"},"output":[],
            "usage":{"input_tokens":50000,"output_tokens":4}});
        let mut cases = vec![(base.clone(), (Some(50000), Some(4)))];
        let mut with_text = base.clone();
        with_text["output"] = json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":"Failed output must not be delivered"}]}]);
        cases.push((with_text, (Some(50000), Some(4))));
        let mut nullable_error = base.clone();
        nullable_error["error"] = Value::Null;
        cases.push((nullable_error.clone(), (Some(50000), Some(4))));
        for reason in ["content_filter", "max_messages", "steered"] {
            let mut incomplete = nullable_error.clone();
            incomplete["status"] = json!("incomplete");
            incomplete["incomplete_details"] = json!({"reason":reason});
            cases.push((incomplete, (Some(50000), Some(4))));
        }
        let mut incomplete = nullable_error;
        incomplete["status"] = json!("incomplete");
        incomplete["incomplete_details"] = json!({"reason":"unknown_reason"});
        cases.push((incomplete.clone(), (None, None)));
        incomplete["incomplete_details"] = json!({"reason":"content_filter"});
        incomplete["usage"] = Value::Null;
        cases.push((incomplete.clone(), (None, None)));
        incomplete["usage"] = json!({"input_tokens":50000,"output_tokens":"bad"});
        cases.push((incomplete, (None, None)));
        for (usage, expected) in [
            (
                json!({"input_tokens":0,"output_tokens":0}),
                (Some(0), Some(0)),
            ),
            (json!({"output_tokens":4}), (None, Some(4))),
            (Value::Null, (None, None)),
            (
                json!({"input_tokens":50000,"output_tokens":"bad"}),
                (None, None),
            ),
            (json!({"input_tokens":-1,"output_tokens":4}), (None, None)),
            (json!([]), (None, None)),
        ] {
            let mut response = base.clone();
            response["usage"] = usage;
            cases.push((response, expected));
        }
        for (field, value) in [
            ("status", json!("in_progress")),
            ("object", json!("error")),
            ("id", Value::Null),
            ("error", json!([])),
            ("error", json!({"code":42,"message":"Synthetic failure"})),
            ("output", Value::Null),
            (
                "output",
                json!([{"type":"message","role":"user","content":[]}]),
            ),
            (
                "output",
                json!([{"type":"message","role":"assistant","content":[{"type":"output_text","text":42}]}]),
            ),
        ] {
            let mut response = base.clone();
            response[field] = value;
            cases.push((response, (None, None)));
        }
        for (response, expected) in cases {
            let mut observed = (None, None);
            assert_eq!(
                decode_observing(
                    Api::OpenAiResponses,
                    &serde_json::to_vec(&response).unwrap(),
                    &mut observed
                )
                .unwrap_err()
                .code,
                MemoryErrorCode::ProviderUnavailable
            );
            assert_eq!(observed, expected);
        }
    }
}

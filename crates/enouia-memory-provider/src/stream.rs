//! Bounded incremental SSE decoding. A closed socket is never completion.
use crate::{
    Result,
    codec::{Api, decode, decode_usage},
    error,
};
use enouia_memory_contract::{
    MemoryError, MemoryErrorCode,
    provider::{FinishReason, ProviderResponse},
};
use serde_json::{Value, json};

pub const MAX_EVENT_BYTES: usize = 1024 * 1024;
pub struct Decoder {
    api: Api,
    line: Vec<u8>,
    data: Vec<u8>,
    event: String,
    text: String,
    terminal: Option<ProviderResponse>,
    usage: Value,
    stop: Option<String>,
    started: bool,
    next_block: u64,
    open_block: Option<u64>,
    message_deltas_started: bool,
    failure: Option<MemoryError>,
}
impl Decoder {
    pub fn new(api: Api) -> Self {
        Self {
            api,
            line: vec![],
            data: vec![],
            event: String::new(),
            text: String::new(),
            terminal: None,
            usage: json!({}),
            stop: None,
            started: false,
            next_block: 0,
            open_block: None,
            message_deltas_started: false,
            failure: None,
        }
    }
    pub fn push(
        &mut self,
        bytes: &[u8],
        on_text: &mut dyn FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        let result = self.push_inner(bytes, on_text);
        if let Err(failure) = result {
            self.failure = Some(failure);
        }
        result
    }
    fn push_inner(
        &mut self,
        bytes: &[u8],
        on_text: &mut dyn FnMut(&str) -> Result<()>,
    ) -> Result<()> {
        for byte in bytes {
            if *byte == b'\n' {
                self.line(on_text)?;
            } else {
                self.line.push(*byte);
                if self.line.len() + self.data.len() > MAX_EVENT_BYTES {
                    return Err(error(MemoryErrorCode::BudgetExceeded));
                }
            }
        }
        Ok(())
    }
    fn line(&mut self, on_text: &mut dyn FnMut(&str) -> Result<()>) -> Result<()> {
        let raw = std::mem::take(&mut self.line);
        let raw = raw.strip_suffix(b"\r").unwrap_or(&raw);
        if raw.is_empty() {
            if !self.data.is_empty() {
                let data = std::mem::take(&mut self.data);
                let v: Value = serde_json::from_slice(&data)
                    .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
                self.event(v, on_text)?;
            }
            self.event.clear();
        } else if let Some(rest) = raw.strip_prefix(b"data:") {
            if !self.data.is_empty() {
                self.data.push(b'\n');
            }
            self.data
                .extend_from_slice(rest.strip_prefix(b" ").unwrap_or(rest));
            if self.data.len() > MAX_EVENT_BYTES {
                return Err(error(MemoryErrorCode::BudgetExceeded));
            }
        } else if let Some(rest) = raw.strip_prefix(b"event:") {
            self.event = std::str::from_utf8(rest.strip_prefix(b" ").unwrap_or(rest))
                .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?
                .into();
        }
        Ok(())
    }
    fn event(&mut self, v: Value, on_text: &mut dyn FnMut(&str) -> Result<()>) -> Result<()> {
        if self.terminal.is_some() {
            return Err(error(MemoryErrorCode::ProviderUnavailable));
        }
        let kind = v["type"]
            .as_str()
            .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?;
        if !self.event.is_empty() && self.event != kind {
            return Err(error(MemoryErrorCode::ProviderUnavailable));
        }
        match (self.api, kind) {
            (Api::OpenAiResponses, "response.output_text.delta" | "response.refusal.delta") => {
                let text = v["delta"]
                    .as_str()
                    .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?;
                if !text.is_empty() {
                    on_text(text)?;
                    self.text.push_str(text);
                }
            }
            (Api::OpenAiResponses, "response.completed" | "response.incomplete") => {
                let bytes = serde_json::to_vec(&v["response"])
                    .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
                let answer = decode(self.api, &bytes)?;
                // Terminal response must describe exactly the persisted stream.
                if answer.text != self.text {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                self.terminal = Some(answer);
            }
            (Api::OpenAiResponses, "error" | "response.failed")
            | (Api::AnthropicMessages, "error") => {
                return Err(error(MemoryErrorCode::ProviderUnavailable));
            }
            (Api::AnthropicMessages, "message_start") => {
                if self.started
                    || v["message"]["type"] != "message"
                    || v["message"]["role"] != "assistant"
                {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                self.started = true;
                self.usage = v["message"]["usage"].clone();
            }
            (Api::AnthropicMessages, "content_block_start") => {
                if !self.started
                    || v["content_block"]["type"] != "text"
                    || self.open_block.is_some()
                    || self.message_deltas_started
                    || v["index"].as_u64() != Some(self.next_block)
                {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                self.open_block = Some(self.next_block);
                self.next_block += 1;
                let initial = v["content_block"]["text"]
                    .as_str()
                    .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?;
                if !initial.is_empty() {
                    on_text(initial)?;
                    self.text.push_str(initial);
                }
            }
            (Api::AnthropicMessages, "content_block_delta") => {
                if !self.started
                    || v["delta"]["type"] != "text_delta"
                    || self.open_block.is_none()
                    || v["index"].as_u64() != self.open_block
                {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                let text = v["delta"]["text"]
                    .as_str()
                    .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))?;
                if !text.is_empty() {
                    on_text(text)?;
                    self.text.push_str(text);
                }
            }
            (Api::AnthropicMessages, "content_block_stop") => {
                if self.open_block.is_none() || v["index"].as_u64() != self.open_block {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                self.open_block = None;
            }
            (Api::AnthropicMessages, "message_delta") => {
                if !self.started || self.open_block.is_some() {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                self.message_deltas_started = true;
                // Anthropic permits multiple top-level deltas. Usage-only
                // updates must not erase a known stop reason or reopen blocks.
                if let Some(stop) = v["delta"]["stop_reason"].as_str() {
                    if self.stop.as_deref().is_some_and(|known| known != stop) {
                        return Err(error(MemoryErrorCode::ProviderUnavailable));
                    }
                    self.stop = Some(stop.into());
                }
                for key in [
                    "input_tokens",
                    "output_tokens",
                    "cache_creation_input_tokens",
                    "cache_read_input_tokens",
                ] {
                    if let Some(tokens) = v["usage"].get(key).filter(|tokens| !tokens.is_null()) {
                        self.usage[key] = tokens.clone();
                    }
                }
            }
            (Api::AnthropicMessages, "message_stop") => {
                if !self.started || self.open_block.is_some() {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                let finish = match self.stop.as_deref() {
                    Some("end_turn" | "stop_sequence") => FinishReason::Completed,
                    Some("max_tokens") => FinishReason::Length,
                    _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
                };
                let (input_tokens, output_tokens) = decode_usage(self.api, &self.usage)?;
                self.terminal = Some(ProviderResponse {
                    text: self.text.clone(),
                    finish,
                    tool_requests: vec![],
                    input_tokens,
                    output_tokens,
                });
            }
            // Known metadata and unknown future SSE event kinds carry no text.
            _ => {}
        }
        Ok(())
    }
    pub fn finish(self) -> Result<ProviderResponse> {
        if let Some(failure) = self.failure {
            return Err(failure);
        }
        if !self.line.is_empty() || !self.data.is_empty() {
            return Err(error(MemoryErrorCode::ProviderUnavailable));
        }
        self.terminal
            .ok_or_else(|| error(MemoryErrorCode::ProviderUnavailable))
    }
}

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
    first_line: bool,
    after_cr: bool,
    data: Vec<u8>,
    event: String,
    text: String,
    terminal: Option<ProviderResponse>,
    usage: Value,
    observed_usage: (Option<u64>, Option<u64>),
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
            first_line: true,
            after_cr: false,
            data: vec![],
            event: String::new(),
            text: String::new(),
            terminal: None,
            usage: json!({}),
            observed_usage: (None, None),
            stop: None,
            started: false,
            next_block: 0,
            open_block: None,
            message_deltas_started: false,
            failure: None,
        }
    }
    /// Last fully validated reported counters, even when the stream fails.
    /// These may be partial usage; they do not establish a terminal response.
    pub fn observed_usage(&self) -> (Option<u64>, Option<u64>) {
        self.observed_usage
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
            // CR terminates a line immediately. Suppress only its optional
            // following LF, including when that LF arrives in another push.
            if self.after_cr {
                self.after_cr = false;
                if *byte == b'\n' {
                    continue;
                }
            }
            match *byte {
                b'\r' => {
                    self.line(on_text)?;
                    self.after_cr = true;
                }
                b'\n' => self.line(on_text)?,
                _ => {
                    self.line.push(*byte);
                    if self.line.len() + self.data.len() > MAX_EVENT_BYTES {
                        return Err(error(MemoryErrorCode::BudgetExceeded));
                    }
                }
            }
        }
        Ok(())
    }
    fn line(&mut self, on_text: &mut dyn FnMut(&str) -> Result<()>) -> Result<()> {
        let raw = std::mem::take(&mut self.line);
        // A single leading UTF-8 BOM is framing, not part of the first field.
        // Buffering the complete first line also handles a fragmented BOM.
        let raw = if self.first_line {
            self.first_line = false;
            raw.strip_prefix(b"\xef\xbb\xbf").unwrap_or(&raw)
        } else {
            &raw
        };
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
                let expected_status = if kind == "response.completed" {
                    "completed"
                } else {
                    "incomplete"
                };
                if v["response"]["status"] != expected_status {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                let bytes = serde_json::to_vec(&v["response"])
                    .map_err(|_| error(MemoryErrorCode::ProviderUnavailable))?;
                let answer = decode(self.api, &bytes)?;
                // Terminal response must describe exactly the persisted stream.
                if answer.text != self.text {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                self.observed_usage = (answer.input_tokens, answer.output_tokens);
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
                let usage = v["message"]["usage"].clone();
                let normalized = decode_usage(self.api, &usage)?;
                self.started = true;
                // Missing/null usage is unknown, but the mutable running
                // accumulator must always be an object, never external data
                // such as an array/string that would panic on key assignment.
                self.usage = if usage.is_null() { json!({}) } else { usage };
                self.observed_usage = normalized;
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
                if !self.started || self.open_block.is_some() || !v["delta"].is_object() {
                    return Err(error(MemoryErrorCode::ProviderUnavailable));
                }
                // Validate each update before merging: a later cumulative
                // counter must never hide an earlier malformed field.
                decode_usage(self.api, &v["usage"])?;
                self.message_deltas_started = true;
                // Anthropic permits multiple top-level deltas. Usage-only
                // updates must not erase a known stop reason or reopen blocks.
                let stop = match v["delta"].get("stop_reason") {
                    None | Some(Value::Null) => None,
                    Some(Value::String(stop)) => Some(stop.as_str()),
                    _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
                };
                if let Some(stop) = stop {
                    if self.stop.as_deref().is_some_and(|known| known != stop) {
                        return Err(error(MemoryErrorCode::ProviderUnavailable));
                    }
                    self.stop = Some(stop.into());
                }
                let mut merged = self.usage.clone();
                for key in [
                    "input_tokens",
                    "output_tokens",
                    "cache_creation_input_tokens",
                    "cache_read_input_tokens",
                ] {
                    if let Some(tokens) = v["usage"].get(key).filter(|tokens| !tokens.is_null()) {
                        merged[key] = tokens.clone();
                    }
                }
                let normalized = decode_usage(self.api, &merged)?;
                self.usage = merged;
                self.observed_usage = normalized;
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

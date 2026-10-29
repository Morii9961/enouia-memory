//! Shared read-only MCP wire session. It grants no native identity or authority.
use super::{MAX_INPUT_BYTES, PROTOCOL_VERSION, Phase, closed, error, reply};
use enouia_memory_contract::json::MAX_SAFE_INTEGER;
use serde_json::{Value, json};
use std::collections::BTreeSet;
pub(super) enum Route {
    Response(Option<Value>),
    Ready {
        id: Value,
        method: String,
        params: Value,
    },
}
pub(super) struct ReadSession {
    phase: Phase,
    ids: BTreeSet<String>,
    server_name: &'static str,
    max_requests: usize,
}
impl ReadSession {
    pub(super) fn new(server_name: &'static str, max_requests: usize) -> Self {
        Self {
            phase: Phase::New,
            ids: BTreeSet::new(),
            server_name,
            max_requests,
        }
    }
    pub(super) fn closed(&self) -> bool {
        self.phase == Phase::Closed
    }
    pub(super) fn close(&mut self) {
        self.phase = Phase::Closed;
    }
    pub(super) fn route(&mut self, message: &Value) -> Route {
        if message.to_string().len() > MAX_INPUT_BYTES
            || !closed(message, &["jsonrpc", "id", "method", "params"])
            || message["jsonrpc"] != "2.0"
            || !message["method"].is_string()
        {
            return Route::Response(Some(error(Value::Null, -32600, "invalid_request")));
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
            return Route::Response(None);
        };
        if !id.as_str().is_some_and(|s| !s.is_empty() && s.len() <= 128)
            && !id
                .as_i64()
                .is_some_and(|i| i.unsigned_abs() <= MAX_SAFE_INTEGER)
        {
            return Route::Response(Some(error(Value::Null, -32600, "invalid_request_id")));
        }
        let id = id.clone();
        if self.closed() {
            return Route::Response(Some(error(id, -32000, "session_closed")));
        }
        if self.ids.len() >= self.max_requests {
            self.phase = Phase::Closed;
            return Route::Response(Some(error(id, -32000, "session_request_limit")));
        }
        if !self.ids.insert(id.to_string()) {
            return Route::Response(Some(error(id, -32600, "duplicate_request_id")));
        }
        if method == "ping" && closed(&params, &["_meta"]) {
            return Route::Response(Some(reply(id, json!({}))));
        }
        if method == "initialize" {
            if self.phase != Phase::New {
                return Route::Response(Some(error(id, -32600, "already_initialized")));
            }
            if !closed(
                &params,
                &["protocolVersion", "capabilities", "clientInfo", "_meta"],
            ) || !params["protocolVersion"].is_string()
                || !params["capabilities"].is_object()
                || !params["clientInfo"]["name"].is_string()
                || !params["clientInfo"]["version"].is_string()
            {
                return Route::Response(Some(error(id, -32602, "invalid_initialize")));
            }
            self.phase = Phase::Initializing;
            return Route::Response(Some(reply(
                id,
                json!({"protocolVersion":PROTOCOL_VERSION,"capabilities":{"tools":{}},"serverInfo":{"name":self.server_name,"version":env!("CARGO_PKG_VERSION")}}),
            )));
        }
        if self.phase != Phase::Ready {
            return Route::Response(Some(error(id, -32000, "not_initialized")));
        }
        Route::Ready {
            id,
            method: method.to_owned(),
            params,
        }
    }
}

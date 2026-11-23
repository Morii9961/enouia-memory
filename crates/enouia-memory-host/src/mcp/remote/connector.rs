//! ADR-MEM-158: the `memory_submit_conversation` write tool for the remote
//! connector. Wraps one read session; every other message goes to it
//! unchanged. Only a bearer holding `memory.write` sees and may call the tool.
use super::{Session, WRITE_SCOPE};
use crate::automation::application::ApplicationFault;
use crate::automation::connector_formation::{
    ConnectorSubmission, ConnectorTheme, ConnectorWriter,
};
use enouia_memory_contract::foundation::Clock;
use enouia_memory_vault::automation::AutomationFault;
use serde::Deserialize;
use serde_json::{Value, json};

pub const TOOL: &str = "memory_submit_conversation";
/// ADR-MEM-174: ChatGPT submits a theme over daily memories it has read.
pub const THEME_TOOL: &str = "memory_submit_theme";

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ThemeArguments {
    theme: ConnectorTheme,
    #[serde(default)]
    preflight: bool,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Arguments {
    submission: ConnectorSubmission,
    #[serde(default)]
    preflight: bool,
}

pub struct ConnectorSession<'w, S: Session> {
    inner: S,
    writer: Option<&'w ConnectorWriter>,
    clock: &'w dyn Clock,
    initialized: bool,
    ready: bool,
    original: Option<Value>,
}

impl<'w, S: Session> ConnectorSession<'w, S> {
    /// `writer` is `None` unless the service enabled the connector and the
    /// bearer was granted `memory.write`.
    pub fn new(
        inner: S,
        writer: Option<&'w ConnectorWriter>,
        scopes: &[String],
        clock: &'w dyn Clock,
    ) -> Self {
        let writer = writer.filter(|_| scopes.iter().any(|s| s == WRITE_SCOPE));
        Self {
            inner,
            writer,
            clock,
            initialized: false,
            ready: false,
            original: None,
        }
    }
}

pub fn tool() -> Value {
    // ADR-MEM-171: kept apart so the schema stays within `json!` nesting limits.
    let entity = json!({
        "type": "object",
        "required": ["name", "entity_type", "reality", "role"],
        "additionalProperties": false,
        "properties": {
            "name": {"type": "string", "maxLength": 128},
            "entity_type": {"enum": ["person", "project", "place", "organization", "thing", "pet", "work"]},
            "reality": {"enum": ["real", "fictional", "unknown"]},
            "role": {"enum": ["subject", "participant", "mentioned", "place", "project", "organization"]}
        }
    });
    json!({
        "name": TOOL,
        "title": "Submit a conversation and its distilled memories",
        "description": "Send the raw turns of this conversation (1-8 per call) together with the memories you distilled from Morii's own words. Each memory must quote a user turn of the same call literally. Memory validates and stores; set preflight to check without writing. Resubmitting the same request_id replays.",
        "inputSchema": {
            "type": "object",
            "required": ["submission"],
            "additionalProperties": false,
            "properties": {
                "preflight": {"type": "boolean"},
                "submission": {
                    "type": "object",
                    "required": ["request", "analyst_model", "statements"],
                    "additionalProperties": false,
                    "properties": {
                        "request": {"type": "object", "description": "runtime-owner-capture request: schema_version, kind, capability, request_id, conversation_id, title, turns"},
                        "analyst_model": {"type": "string", "maxLength": 128},
                        "statements": {"type": "array", "maxItems": 32, "items": {
                            "type": "object",
                            "required": ["message_id", "quote", "title", "interpretation", "kind", "event_time", "valid_time"],
                            "additionalProperties": false,
                            "properties": {
                                "message_id": {"type": "string"},
                                "quote": {"type": "string", "maxLength": 2048},
                                "title": {"type": "string", "maxLength": 128},
                                "interpretation": {"type": "string"},
                                "kind": {"type": "string"},
                                "event_time": {"type": "object"},
                                "valid_time": {"type": "object"},
                                "note": {"type": ["string", "null"], "maxLength": 512},
                                "entities": {"type": "array", "maxItems": 8, "description": "People, places, projects or other entities this memory names. Each gets its own record; Morii decides later which records are the same.", "items": entity}
                            }
                        }}
                    }
                }
            }
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

pub fn theme_tool() -> Value {
    json!({
        "name": THEME_TOOL,
        "title": "Submit a theme over daily memories",
        "description": "Name a recurring theme you see across 2-32 of Morii's current daily memories (their memory_id values from the read tools) and describe it in your own words. Memory stores it as an inference linked to those memories, never as something Morii said; set preflight to check without writing. Resubmitting the same request_id replays.",
        "inputSchema": {
            "type": "object",
            "required": ["theme"],
            "additionalProperties": false,
            "properties": {
                "preflight": {"type": "boolean"},
                "theme": {
                    "type": "object",
                    "required": ["request_id", "analyst_model", "title", "summary", "members"],
                    "additionalProperties": false,
                    "properties": {
                        "request_id": {"type": "string"},
                        "analyst_model": {"type": "string", "maxLength": 128},
                        "title": {"type": "string", "maxLength": 60},
                        "summary": {"type": "string", "maxLength": 2000},
                        "members": {"type": "array", "minItems": 2, "maxItems": 32, "items": {"type": "string"}},
                        "note": {"type": ["string", "null"], "maxLength": 512}
                    }
                }
            }
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

fn result(id: &Value, data: Value, is_error: bool) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "result": {
        "content": [{"type": "text", "text": data.to_string()}],
        "structuredContent": data,
        "isError": is_error,
    }})
}

impl<S: Session> Session for ConnectorSession<'_, S> {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        let method = message["method"].as_str().unwrap_or_default();
        if method == "notifications/initialized" && self.initialized {
            self.ready = true;
        }
        let call_name = message["params"]["name"].as_str();
        let Some(writer) = self.writer.filter(|_| {
            self.ready
                && method == "tools/call"
                && (call_name == Some(TOOL) || call_name == Some(THEME_TOOL))
        }) else {
            let mut response = self.inner.handle(message)?;
            if method == "initialize" && response.get("result").is_some() {
                self.initialized = true;
            }
            if method == "tools/list"
                && self.writer.is_some()
                && let Some(tools) = response["result"]["tools"].as_array_mut()
            {
                tools.push(tool());
                tools.push(theme_tool());
            }
            return Some(response);
        };
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let arguments = message["params"]["arguments"].clone();
        let now = self.clock.now_unix_ms();
        let outcome = if call_name == Some(THEME_TOOL) {
            let Ok(args) = serde_json::from_value::<ThemeArguments>(arguments) else {
                return Some(result(&id, json!({"error": "invalid_arguments"}), true));
            };
            writer
                .submit_theme(&args.theme, args.preflight, now)
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
        } else {
            let Ok(args) = serde_json::from_value::<Arguments>(arguments) else {
                return Some(result(&id, json!({"error": "invalid_arguments"}), true));
            };
            writer
                .submit(&args.submission, args.preflight, now)
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null))
        };
        match outcome {
            Ok(receipt) => Some(result(&id, receipt, false)),
            Err(ApplicationFault::Vault(
                AutomationFault::OutcomeUnknown | AutomationFault::RecoveryRequired,
            )) => {
                // Never retried here: the endpoint halts on the original message.
                self.original.get_or_insert_with(|| message.clone());
                None
            }
            Err(ApplicationFault::Invalid) => {
                Some(result(&id, json!({"error": "invalid_submission"}), true))
            }
            Err(fault) => Some(result(&id, json!({"error": format!("{fault:?}")}), true)),
        }
    }
    fn closed(&self) -> bool {
        self.inner.closed()
    }
    fn unknown(&self) -> Option<&Value> {
        self.original.as_ref().or_else(|| self.inner.unknown())
    }
}

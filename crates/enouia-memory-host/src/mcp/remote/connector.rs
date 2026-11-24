//! ADR-MEM-158: the `memory_submit_conversation` write tool for the remote
//! connector. Wraps one read session; every other message goes to it
//! unchanged. Only a bearer holding `memory.write` sees and may call the tool.
use super::chatgpt;
use super::{Session, WRITE_SCOPE};
use crate::automation::application::ApplicationFault;
use crate::automation::connector_formation::{
    ConnectorSubmission, ConnectorTheme, ConnectorWriter,
};
use crate::automation::qualified_agent::{AgentCorrectionReporter, AgentForgetRequester};
use enouia_memory_contract::ids::{MemoryId, SourceId};
use enouia_memory_contract::{
    foundation::Clock, next::AttentionProfile, ports::IdSource, time::Timestamp,
};
use enouia_memory_vault::automation::AutomationFault;
use enouia_memory_vault::automation::segmented::{AgentCorrectionRequest, AgentForgetRequest};
use serde::Deserialize;
use serde_json::{Value, json};

pub const TOOL: &str = "memory_submit_conversation";
/// ADR-MEM-174: ChatGPT submits a theme over daily memories it has read.
pub const THEME_TOOL: &str = "memory_submit_theme";
/// ADR-MEM-184: requests Morii confirms in Runtime.
pub const CORRECTION_TOOL: &str = "memory_request_correction";
pub const FORGET_TOOL: &str = "memory_request_forget";

/// ADR-MEM-184: request writers under the connector's request-capable grant.
#[derive(Clone, Copy)]
pub struct RequestFilers<'w> {
    pub correction: &'w AgentCorrectionReporter<'w>,
    pub forget: &'w AgentForgetRequester<'w>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RequestArguments {
    request_id: String,
    memory_id: MemoryId,
    source_id: SourceId,
    quote: String,
    #[serde(default)]
    correction: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

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
    ids: &'w dyn IdSource,
    profile: &'w AttentionProfile,
    requests: Option<RequestFilers<'w>>,
    grant_end: Option<Timestamp>,
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
        ids: &'w dyn IdSource,
        profile: &'w AttentionProfile,
    ) -> Self {
        let writer = writer.filter(|_| scopes.iter().any(|s| s == WRITE_SCOPE));
        Self {
            inner,
            writer,
            clock,
            ids,
            profile,
            requests: None,
            grant_end: None,
            initialized: false,
            ready: false,
            original: None,
        }
    }
}

impl<'w, S: Session> ConnectorSession<'w, S> {
    /// ADR-MEM-184: offer correction and forget requests too. They need the
    /// connector writer, which is what resolves their references.
    pub fn with_requests(mut self, requests: RequestFilers<'w>) -> Self {
        self.requests = Some(requests);
        self
    }
    /// ADR-MEM-190: the read grant's end. In its last `GRANT_NOTICE_MS`,
    /// every tool result carries a `grant_notice` for ChatGPT to relay.
    pub fn with_grant_end(mut self, end: Timestamp) -> Self {
        self.grant_end = Some(end);
        self
    }
    fn file_request(
        &mut self,
        writer: &ConnectorWriter,
        requests: RequestFilers<'w>,
        name: &str,
        id: &Value,
        arguments: Value,
        message: &Value,
    ) -> Option<Value> {
        let invalid = |problem: &str| {
            Some(result(
                id,
                json!({"error": "invalid_request", "problems": [problem]}),
                true,
            ))
        };
        let Ok(args) = serde_json::from_value::<RequestArguments>(arguments) else {
            return Some(result(id, json!({"error": "invalid_arguments"}), true));
        };
        let text = if name == CORRECTION_TOOL {
            args.correction
        } else {
            args.reason
        };
        let (Some(request_id), Some(text)) = (chatgpt::request_id(&args.request_id), text) else {
            return Some(result(id, json!({"error": "invalid_arguments"}), true));
        };
        let refs = match writer.request_refs(&args.memory_id, &args.source_id) {
            Ok(refs) => refs,
            Err(fault) => return Some(result(id, json!({"error": format!("{fault:?}")}), true)),
        };
        let Some((target, source)) = refs else {
            return invalid("memory_id must name a current memory and source_id a captured source");
        };
        let outcome = if name == CORRECTION_TOOL {
            requests
                .correction
                .submit(&AgentCorrectionRequest {
                    request_id,
                    target,
                    source,
                    quote_hint: args.quote,
                    correction: text,
                })
                .map(|r| json!(r))
        } else {
            requests
                .forget
                .submit(&AgentForgetRequest {
                    request_id,
                    target,
                    source,
                    quote_hint: args.quote,
                    forget: text,
                })
                .map(|r| json!(r))
        };
        match outcome {
            Ok(receipt) => Some(result(id, receipt, false)),
            Err(ApplicationFault::Vault(
                AutomationFault::OutcomeUnknown | AutomationFault::RecoveryRequired,
            )) => {
                // Never retried here: the endpoint halts on the original message.
                self.original.get_or_insert_with(|| message.clone());
                None
            }
            Err(ApplicationFault::Invalid) => invalid(
                "the quote must appear verbatim in a user turn of that source, and the connector must be allowed to cite that source and memory",
            ),
            Err(fault) => Some(result(id, json!({"error": format!("{fault:?}")}), true)),
        }
    }
}

fn request_tool(name: &str, title: &str, field: &str, about: &str) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": about,
        "inputSchema": {
            "type": "object",
            "required": ["request_id", "memory_id", "source_id", "quote", field],
            "additionalProperties": false,
            "properties": {
                "request_id": {"type": "string", "maxLength": 128, "description": "Any stable key for this request; the same key replays."},
                "memory_id": {"type": "string", "description": "The memory to correct or forget."},
                "source_id": {"type": "string", "description": "The source of the user turn where Morii asked for it: submit that conversation first, then take the turn's source from its receipt's `turns`."},
                "quote": {"type": "string", "maxLength": 2048, "description": "Morii's own words in that turn, verbatim."},
                field: {"type": "string", "maxLength": 2048}
            }
        },
        "annotations": {"readOnlyHint": false, "destructiveHint": false, "idempotentHint": true, "openWorldHint": false}
    })
}

pub fn correction_tool() -> Value {
    request_tool(
        CORRECTION_TOOL,
        "Ask Morii to confirm a correction",
        "correction",
        "When Morii says a memory is wrong, file a correction request: what should change, backed by his own words in a conversation you submitted. Nothing changes until Morii confirms it in Enouia Runtime.",
    )
}

pub fn forget_tool() -> Value {
    request_tool(
        FORGET_TOOL,
        "Ask Morii to confirm forgetting a memory",
        "reason",
        "When Morii asks to forget a memory, file a forget request backed by his own words in a conversation you submitted. Nothing is forgotten until Morii confirms it in Enouia Runtime.",
    )
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
                        "request": {
                            "type": "object",
                            "required": ["request_id", "conversation_id", "turns"],
                            "description": "The conversation. Constants, branch, sequence, parent and language are filled in; the full runtime-owner-capture form is also accepted.",
                            "properties": {
                                "request_id": {"type": "string", "maxLength": 128, "description": "Any stable key for this submission; the same key replays instead of writing twice."},
                                "conversation_id": {"type": "string", "maxLength": 256, "description": "One stable ID for this chat, reused in every later submission of the same chat."},
                                "title": {"type": ["string", "null"], "maxLength": 256},
                                "turns": {"type": "array", "minItems": 1, "maxItems": 8, "items": {
                                    "type": "object",
                                    "required": ["message_id", "speaker", "text"],
                                    "properties": {
                                        "message_id": {"type": "string", "maxLength": 256, "description": "Unique within this chat across all submissions: never reuse an ID for a different message (for example 2026-11-24-3)."},
                                        "speaker": {"enum": ["user", "assistant"]},
                                        "text": {"type": "string"},
                                        "occurred_at": {"type": ["string", "null"], "description": "RFC 3339 time of the turn, if known."},
                                        "assistant_model": {"type": ["string", "null"], "maxLength": 128}
                                    }
                                }}
                            }
                        },
                        "analyst_model": {"type": "string", "maxLength": 128},
                        "statements": {"type": "array", "maxItems": 32, "items": {
                            "type": "object",
                            "required": ["message_id", "quote", "title", "interpretation", "kind"],
                            "additionalProperties": false,
                            "properties": {
                                "message_id": {"type": "string"},
                                "quote": {"type": "string", "maxLength": 2048},
                                "title": {"type": "string", "maxLength": 128},
                                "interpretation": {"enum": ["clear_self_report", "inference", "ambiguous", "conflicting"]},
                                "kind": {"type": "string"},
                                "event_time": {"type": "object", "description": "Optional; unknown when omitted."},
                                "valid_time": {"type": "object", "description": "Optional; unknown when omitted."},
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
                        "request_id": {"type": "string", "maxLength": 128, "description": "Any stable key for this theme; the same key replays."},
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

impl<S: Session> ConnectorSession<'_, S> {
    fn answer(&mut self, message: &Value) -> Option<Value> {
        let method = message["method"].as_str().unwrap_or_default();
        if method == "notifications/initialized" && self.initialized {
            self.ready = true;
        }
        let call_name = message["params"]["name"].as_str();
        // ADR-MEM-178: complete a short read call before the read session sees it.
        if method == "tools/call"
            && let Some(name) = call_name.filter(|n| chatgpt::SHORT_TOOLS.contains(n))
        {
            let mut arguments = message["params"]["arguments"].clone();
            if arguments.is_null() {
                arguments = json!({});
            }
            let completed = Timestamp::from_unix_ms(self.clock.now_unix_ms()).and_then(|now| {
                chatgpt::read_arguments(name, arguments, &now, self.ids, self.profile)
            });
            let Some(arguments) = completed else {
                let id = message.get("id").cloned().unwrap_or(Value::Null);
                return Some(result(&id, json!({"error": "invalid_arguments"}), true));
            };
            let mut message = message.clone();
            message["params"]["arguments"] = arguments;
            return self.inner.handle(&message);
        }
        let Some(writer) = self.writer.filter(|_| {
            self.ready
                && method == "tools/call"
                && (call_name == Some(TOOL)
                    || call_name == Some(THEME_TOOL)
                    || (self.requests.is_some()
                        && (call_name == Some(CORRECTION_TOOL) || call_name == Some(FORGET_TOOL))))
        }) else {
            let mut response = self.inner.handle(message)?;
            if method == "initialize" && response.get("result").is_some() {
                self.initialized = true;
            }
            if method == "tools/list"
                && let Some(tools) = response["result"]["tools"].as_array_mut()
            {
                for tool in tools.iter_mut() {
                    let name = tool["name"].as_str().unwrap_or_default().to_owned();
                    if let Some(schema) = chatgpt::short_schema(&name) {
                        tool["inputSchema"] = schema;
                    }
                    if let Some(description) = chatgpt::short_description(&name) {
                        tool["description"] = json!(description);
                    }
                }
                if let Some(writer) = self.writer {
                    // The kinds this service accepts, from its configuration.
                    let mut submit = tool();
                    submit["inputSchema"]["properties"]["submission"]["properties"]["statements"]
                        ["items"]["properties"]["kind"] =
                        json!({"enum": writer.config().allowed_forms});
                    tools.push(submit);
                    tools.push(theme_tool());
                    if self.requests.is_some() {
                        tools.push(correction_tool());
                        tools.push(forget_tool());
                    }
                }
            }
            return Some(response);
        };
        let id = message.get("id").cloned().unwrap_or(Value::Null);
        let mut arguments = message["params"]["arguments"].clone();
        let now = self.clock.now_unix_ms();
        if let (Some(requests), Some(name @ (CORRECTION_TOOL | FORGET_TOOL))) =
            (self.requests, call_name)
        {
            return self.file_request(writer, requests, name, &id, arguments, message);
        }
        let outcome = if call_name == Some(THEME_TOOL) {
            chatgpt::theme_arguments(&mut arguments);
            let Ok(args) = serde_json::from_value::<ThemeArguments>(arguments) else {
                return Some(result(&id, json!({"error": "invalid_arguments"}), true));
            };
            let outcome = writer
                .submit_theme(&args.theme, args.preflight, now)
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null));
            if matches!(outcome, Err(ApplicationFault::Invalid)) {
                let mut problems = args.theme.problems();
                if problems.is_empty() {
                    problems.push("every member must be a current memory this connector submitted, in its own domain".into());
                }
                return Some(result(
                    &id,
                    json!({"error": "invalid_submission", "problems": problems}),
                    true,
                ));
            }
            outcome
        } else {
            chatgpt::conversation_arguments(&mut arguments);
            let Ok(args) = serde_json::from_value::<Arguments>(arguments) else {
                return Some(result(&id, json!({"error": "invalid_arguments"}), true));
            };
            let outcome = writer
                .submit(&args.submission, args.preflight, now)
                .map(|r| serde_json::to_value(r).unwrap_or(Value::Null));
            if matches!(outcome, Err(ApplicationFault::Invalid)) {
                let problems = writer.submission_problems(&args.submission);
                return Some(result(
                    &id,
                    json!({"error": "invalid_submission", "problems": problems}),
                    true,
                ));
            }
            outcome
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
}

/// ADR-MEM-190: how long before the read grant ends tool results say so.
pub const GRANT_NOTICE_MS: i64 = 14 * 86_400_000;

impl<S: Session> Session for ConnectorSession<'_, S> {
    fn handle(&mut self, message: &Value) -> Option<Value> {
        let mut response = self.answer(message)?;
        let left = self
            .grant_end
            .as_ref()
            .map(|end| end.unix_ms() - self.clock.now_unix_ms())
            .filter(|left| *left < GRANT_NOTICE_MS);
        if let (Some(left), Some(end)) = (left, &self.grant_end)
            && message["method"] == "tools/call"
            && let Some(data) = response["result"]["structuredContent"].as_object_mut()
        {
            data.insert(
                "grant_notice".into(),
                json!({
                    "ends_at": end,
                    "days_left": left.max(0) / 86_400_000,
                    "message": "Morii's read grant for this connector ends soon; after that this connector stops. Tell Morii to renew it on the server (enouia-memory-remote read-renew-plan).",
                }),
            );
            let text = response["result"]["structuredContent"].to_string();
            response["result"]["content"] = json!([{"type": "text", "text": text}]);
        }
        Some(response)
    }
    fn closed(&self) -> bool {
        self.inner.closed()
            || self.requests.is_some_and(|r| {
                r.correction.check_connection().is_err() || r.forget.check_connection().is_err()
            })
    }
    fn unknown(&self) -> Option<&Value> {
        self.original.as_ref().or_else(|| self.inner.unknown())
    }
}

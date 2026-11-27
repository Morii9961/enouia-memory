//! ADR-MEM-178: ChatGPT-shaped tool inputs on the remote service. The read
//! tools advertise flat inputs without `$ref`; the service adds the frozen
//! envelope (schema version, kind, capability, operation, a fresh request ID,
//! the evaluation time) and the documented defaults. Write tools accept their
//! full form or a short one whose missing constants and per-turn fields are
//! filled in. A free-form `request_id` maps to one stable request ID, so a
//! resubmission still replays. A full envelope passes through unchanged, and
//! the typed parsing behind this layer stays the only validation.
use crate::mcp::{initialized, opening};
use enouia_memory_contract::{
    ids::RequestId, json::canonical_bytes, next::AttentionProfile, ports::IdSource, time::Timestamp,
};
use serde::Deserialize;
use serde_json::{Map, Value, json};

/// Read tools whose input this layer shortens.
pub const SHORT_TOOLS: [&str; 6] = [
    "memory_search",
    "memory_read",
    "memory_source",
    "memory_related",
    "memory_recall",
    "session_start",
];

fn filter_enum(field: &str) -> Value {
    let catalog: Value =
        serde_json::from_str(initialized::schemas::documents()["catalog-v2.schema.json"])
            .expect("frozen schema");
    let options = &catalog["$defs"]["filter"]["properties"][field]["oneOf"];
    let values = options
        .as_array()
        .and_then(|o| o.iter().find_map(|v| v.get("enum")))
        .cloned()
        .expect("filter enum");
    json!({"enum": values})
}

/// ADR-MEM-180: what each read tool is for, worded for ChatGPT. The honesty
/// notes stay: memories are Morii's statements or inferences, suggestions
/// are not facts, and reads change nothing.
/// ADR-MEM-229: with a semantic index, recall and related also go by
/// meaning, and the wording says so.
pub fn short_description(tool: &str, semantic: bool) -> Option<&'static str> {
    Some(match tool {
        "memory_recall" if semantic => {
            "Find the memories most relevant to a question or topic (`text`, a plain sentence in any language), ranked by relevance and attention. Recall matches words and, when the local model is running, meaning; a few related words after the question still help (for 我第一次喝酒是什么时候: 首次 饮酒 啤酒). Use it before answering anything about Morii's life, preferences, people or plans. Memories record what Morii said or what was inferred, not verified facts."
        }
        "memory_related" if semantic => {
            "List memories linked to one memory: lineage, shared entities and themes, and term-overlap, same-name or similar-meaning suggestions. Suggestions are not facts."
        }
        "session_start" => {
            "Call at the start of a conversation: a short, bounded set of Morii's current memories to orient yourself. Optional `topic`. Reads only."
        }
        "memory_recall" => {
            "Find the memories most relevant to a question or topic (`text`, a plain sentence in any language), ranked by relevance and attention. Recall matches words, not meanings, so add a few synonyms and related words after the question (for 我第一次喝酒是什么时候: 首次 饮酒 啤酒). Use it before answering anything about Morii's life, preferences, people or plans. Memories record what Morii said or what was inferred, not verified facts."
        }
        "memory_search" => {
            "List Morii's memories, newest first, optionally matching every word in `query` (for Chinese, give short keywords separated by spaces, not a sentence) or filtered by form or attention. Returns titles and IDs; page with `cursor`. Use memory_read for one memory in full."
        }
        "memory_read" => {
            "Read one memory in full by `memory_id`: content, quoted evidence, times, entities and revision history."
        }
        "memory_source" => {
            "Read the original conversation text behind a memory's evidence, by the `source_id` that memory_read shows."
        }
        "memory_related" => {
            "List memories linked to one memory: lineage, shared entities and themes, and term-overlap or same-name suggestions. Suggestions are not facts."
        }
        _ => return None,
    })
}

/// The flat input schema advertised for a shortened read tool.
pub fn short_schema(tool: &str) -> Option<Value> {
    let id = |what: &str| json!({"type": "string", "description": format!("A {what} ID from an earlier result")});
    let (properties, required) = match tool {
        "memory_search" => (
            json!({
                "query": {"type": "string", "maxLength": 2048, "description": "Words to look for. Leave empty to list recent memories."},
                "form": filter_enum("form"),
                "attention": filter_enum("attention"),
                "adoption": filter_enum("adoption"),
                "days": {"type": "integer", "minimum": 1, "maximum": 366, "description": "Only memories Memory learned in the last N days, for example 7 for this week."},
                "limit": {"type": "integer", "minimum": 1, "maximum": 100},
                "cursor": {"type": "object", "description": "The `next` value of the previous page, to continue it."}
            }),
            json!([]),
        ),
        "memory_recall" => (
            json!({
                "text": {"type": "string", "maxLength": 2048, "description": "What to recall, in plain words, followed by a few synonyms or related words separated by spaces."},
                "intent": {"enum": ["current", "historical", "browse"]},
                "conditions": {"type": "array", "maxItems": 8, "items": {"type": "string", "minLength": 1, "maxLength": 256}},
                "limit": {"type": "integer", "minimum": 1, "maximum": 32}
            }),
            json!(["text"]),
        ),
        "memory_read" => (
            json!({"memory_id": id("memory"), "include_source_text": {"type": "boolean"}}),
            json!(["memory_id"]),
        ),
        "memory_source" => (
            json!({
                "source_id": id("source"),
                "revision": {"type": ["integer", "null"], "minimum": 1},
                "from_byte": {"type": "integer", "minimum": 0, "maximum": 16384}
            }),
            json!(["source_id"]),
        ),
        "memory_related" => (json!({"memory_id": id("memory")}), json!(["memory_id"])),
        "session_start" => (
            json!({"topic": {"type": "string", "maxLength": 512, "description": "What this conversation is about, if known."}}),
            json!([]),
        ),
        _ => return None,
    };
    Some(json!({
        "type": "object",
        "additionalProperties": false,
        "properties": properties,
        "required": required
    }))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Search {
    #[serde(default)]
    query: String,
    #[serde(default)]
    form: Option<Value>,
    #[serde(default)]
    attention: Option<Value>,
    #[serde(default)]
    adoption: Option<Value>,
    #[serde(default)]
    days: Option<u16>,
    #[serde(default)]
    limit: Option<u32>,
    #[serde(default)]
    cursor: Option<Value>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Recall {
    text: String,
    #[serde(default)]
    intent: Option<String>,
    #[serde(default)]
    conditions: Vec<String>,
    #[serde(default)]
    limit: Option<u32>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Read {
    memory_id: String,
    #[serde(default)]
    include_source_text: bool,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Source {
    source_id: String,
    #[serde(default)]
    revision: Option<u32>,
    #[serde(default)]
    from_byte: u32,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Related {
    memory_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Start {
    #[serde(default)]
    topic: String,
}

/// Completes a shortened read call into the frozen envelope. Recall sizes
/// default to, and are capped at, the service's attention profile. A call that
/// already carries `schema_version` is returned unchanged.
pub fn read_arguments(
    tool: &str,
    arguments: Value,
    now: &Timestamp,
    ids: &dyn IdSource,
    profile: &AttentionProfile,
) -> Result<Value, String> {
    // ADR-MEM-196: every refusal says why.
    let parse = |e: serde_json::Error| e.to_string();
    if arguments.get("schema_version").is_some() {
        return Ok(arguments);
    }
    let request_id = RequestId::from_random(ids.random_16());
    if tool == "session_start" {
        let start: Start = serde_json::from_value(arguments).map_err(parse)?;
        return Ok(json!({
            "schema_version": 2,
            "kind": "agent_session_start_request",
            "capability": opening::SESSION_CAPABILITY,
            "request": {"request_id": request_id, "at": now, "topic": start.topic}
        }));
    }
    let inner = match tool {
        "memory_search" => {
            let s: Search = serde_json::from_value(arguments).map_err(parse)?;
            let mut filter =
                serde_json::to_value(enouia_memory_contract::next::CatalogFilter::default())
                    .map_err(parse)?;
            filter["text"] = json!(s.query);
            for (key, value) in [
                ("form", s.form),
                ("attention", s.attention),
                ("adoption", s.adoption),
            ] {
                if let Some(value) = value {
                    filter[key] = value;
                }
            }
            // ADR-MEM-182: recent memories by the time Memory learned them.
            if let Some(days) = s.days.filter(|d| (1..=366).contains(d)) {
                let from = Timestamp::from_unix_ms(now.unix_ms() - i64::from(days) * 86_400_000)
                    .ok_or("days reaches before year 0000")?;
                filter["time_axis"] = json!("learned");
                filter["time_window"] = json!({"from": from, "until": null});
            } else if s.days.is_some() {
                return Err("days must be 1 to 366".into());
            }
            // A page holds 1 to 100 memories; a larger limit pages instead.
            json!({"filter": filter, "cursor": s.cursor, "limit": s.limit.unwrap_or(20).clamp(1, 100), "max_bytes": 65_536, "include_facets": false})
        }
        "memory_recall" => {
            let r: Recall = serde_json::from_value(arguments).map_err(parse)?;
            json!({"text": r.text, "conditions": r.conditions, "event_window": null, "at": now, "intent": r.intent.unwrap_or_else(|| "current".into()), "limit": r.limit.map_or(profile.max_items, u64::from).min(profile.max_items), "max_bytes": profile.max_bytes})
        }
        "memory_read" => {
            let r: Read = serde_json::from_value(arguments).map_err(parse)?;
            json!({"memory_id": r.memory_id, "expected": null, "include_source_text": r.include_source_text, "max_bytes": 65_536, "history_cursor": null})
        }
        "memory_source" => {
            let s: Source = serde_json::from_value(arguments).map_err(parse)?;
            json!({"source_id": s.source_id, "revision": s.revision, "from_byte": s.from_byte, "to_byte": null, "max_bytes": 8_192})
        }
        "memory_related" => {
            let r: Related = serde_json::from_value(arguments).map_err(parse)?;
            json!({"memory_id": r.memory_id, "expected": null, "max_bytes": 65_536})
        }
        _ => return Err(format!("{tool} has no short form")),
    };
    Ok(json!({
        "schema_version": 2,
        "kind": "agent_initialized_read_request",
        "capability": initialized::CAPABILITY,
        "request_id": request_id,
        "operation": tool,
        "arguments": inner
    }))
}

/// A request ID as given, or the stable one derived from a free-form key of
/// 1 to 128 characters, so the same key always replays the same write.
pub fn request_id(key: &str) -> Option<RequestId> {
    if let Ok(id) = RequestId::parse(key) {
        return Some(id);
    }
    if key.is_empty() || key.chars().count() > 128 {
        return None;
    }
    let digest = enouia_memory_contract::hash::sha256(
        &canonical_bytes(&json!(["chatgpt-request-key-1", key])).expect("closed seed"),
    );
    let mut bytes = [0u8; 16];
    for (i, byte) in bytes.iter_mut().enumerate() {
        *byte = u8::from_str_radix(&digest.as_str()[2 * i..2 * i + 2], 16).unwrap_or(0);
    }
    Some(RequestId::from_random(bytes))
}

fn request_key(value: &mut Value) {
    if let Some(id) = value.as_str().and_then(request_id) {
        *value = json!(id);
    }
}

fn fill(object: &mut Map<String, Value>, key: &str, value: Value) {
    object.entry(key).or_insert(value);
}

/// ADR-MEM-195: an RFC 3339 time with any offset and precision, as the
/// Vault's UTC millisecond form; `None` when it is not one.
pub fn utc_time(value: &str) -> Option<String> {
    let b = value.as_bytes();
    if !value.is_ascii()
        || b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let mut rest = &value[19..];
    let mut millis = "000".to_owned();
    if let Some(fraction) = rest.strip_prefix('.') {
        let digits = fraction.bytes().take_while(u8::is_ascii_digit).count();
        if digits == 0 {
            return None;
        }
        millis = format!("{:0<3}", &fraction[..digits.min(3)]);
        rest = &fraction[digits..];
    }
    let offset_minutes = match rest {
        "Z" | "z" => 0,
        _ => {
            let sign = match rest.as_bytes().first()? {
                b'+' => 1,
                b'-' => -1,
                _ => return None,
            };
            let (h, m) = rest[1..].split_once(':')?;
            if h.len() != 2 || m.len() != 2 {
                return None;
            }
            sign * (h.parse::<i64>().ok()? * 60 + m.parse::<i64>().ok()?)
        }
    };
    let local = Timestamp::parse(&format!("{}T{}.{millis}Z", &value[..10], &value[11..19])).ok()?;
    Timestamp::from_unix_ms(local.unix_ms() - offset_minutes * 60_000).map(|t| t.to_string())
}

fn unknown_time() -> Value {
    json!({"from": null, "until": null, "precision": "unknown", "bound": "within", "tz": null, "basis": "unknown"})
}

/// Fills a short conversation submission in place: request constants, a
/// linear `main` branch in the given order, and unknown times by default.
pub fn conversation_arguments(arguments: &mut Value) {
    let Some(request) = arguments
        .pointer_mut("/submission/request")
        .and_then(Value::as_object_mut)
    else {
        return;
    };
    fill(request, "schema_version", json!(2));
    fill(request, "kind", json!("runtime_capture_request"));
    fill(
        request,
        "capability",
        json!("memory-runtime-owner-capture-native-1"),
    );
    fill(request, "title", Value::Null);
    if let Some(id) = request.get_mut("request_id") {
        request_key(id);
    }
    if let Some(turns) = request.get_mut("turns").and_then(Value::as_array_mut) {
        let mut parent = Value::Null;
        for (index, turn) in turns.iter_mut().enumerate() {
            let Some(turn) = turn.as_object_mut() else {
                continue;
            };
            // ADR-MEM-195: a turn time as ChatGPT writes it becomes UTC; the
            // text it gave is kept as the original time.
            if let Some(given) = turn
                .get("occurred_at")
                .and_then(Value::as_str)
                .map(str::to_owned)
                && Timestamp::parse(&given).is_err()
                && let Some(utc) = utc_time(&given)
            {
                turn.insert("occurred_at".into(), json!(utc));
                fill(turn, "original_time", json!(given));
            }
            fill(turn, "parent_message_id", parent.clone());
            fill(turn, "branch_id", json!("main"));
            fill(turn, "sequence", json!(index + 1));
            fill(turn, "original_time", Value::Null);
            fill(turn, "original_timezone", Value::Null);
            fill(turn, "occurred_at", Value::Null);
            fill(turn, "content_lang", json!("und"));
            fill(turn, "assistant_model", Value::Null);
            fill(turn, "attachments", json!([]));
            parent = turn.get("message_id").cloned().unwrap_or(Value::Null);
        }
    }
    if let Some(statements) = arguments
        .pointer_mut("/submission/statements")
        .and_then(Value::as_array_mut)
    {
        for statement in statements.iter_mut().filter_map(Value::as_object_mut) {
            fill(statement, "event_time", unknown_time());
            fill(statement, "valid_time", unknown_time());
            fill(statement, "note", Value::Null);
        }
    }
}

/// A theme's free-form `request_id` becomes its stable request ID.
pub fn theme_arguments(arguments: &mut Value) {
    if let Some(id) = arguments.pointer_mut("/theme/request_id") {
        request_key(id);
    }
}

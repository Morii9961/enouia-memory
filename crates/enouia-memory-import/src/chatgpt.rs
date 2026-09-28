//! ChatGPT export adapter (structure-probing, IMPORT_REVIEW §1).
//!
//! No real export was available when this was written, so the adapter only
//! relies on the widely documented shape of `conversations.json`: an array
//! of conversations, each with a `mapping` of nodes (`id`, `message`,
//! `parent`, `children`) and an optional `current_node`. Anything else is
//! ignored (the raw bytes keep it). A node that does not match that shape is
//! counted as unparseable, never guessed at.
//!
//! Message text is the string `parts` of `message.content` joined with `\n`;
//! its SHA-256 is the source's `content_hash`. Times come only from the
//! message's own `create_time`; a missing time stays unknown.

use crate::parsed::{ParentLink, ParsedAttachment, ParsedMessage, Unit, pointer_token, warning};
use enouia_memory_contract::common::{EvidenceClass, Locator, SpeakerRole, TimePrecision};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::source::{Availability, Completeness, SourceKind};
use enouia_memory_contract::time::Timestamp;
use serde_json::Value;

pub const ADAPTER: &str = "chatgpt-conversations";
pub const VERSION: &str = "1";
pub const SCHEMA: &str = "chatgpt-mapping-v1";

/// Looks up a file the export carries for an attachment ID.
pub trait FileLookup {
    /// `Some(Ok((name, bytes)))`, `Some(Err(()))` for a refused member, or `None`.
    fn find(&self, file_id: &str) -> Option<Result<(String, Vec<u8>), ()>>;
}

pub struct NoFiles;

impl FileLookup for NoFiles {
    fn find(&self, _file_id: &str) -> Option<Result<(String, Vec<u8>), ()>> {
        None
    }
}

/// Is this JSON value shaped like a ChatGPT `conversations.json`?
pub fn probe(value: &Value) -> bool {
    value.as_array().is_some_and(|items| {
        !items.is_empty()
            && items
                .iter()
                .all(|c| c.get("mapping").is_some_and(Value::is_object))
    })
}

fn role(value: &Value) -> SpeakerRole {
    match value["author"]["role"].as_str() {
        Some("user") => SpeakerRole::User,
        Some("assistant") => SpeakerRole::Assistant,
        Some("tool") => SpeakerRole::Tool,
        Some("system") => SpeakerRole::System,
        _ => SpeakerRole::Unknown,
    }
}

fn evidence(role: SpeakerRole) -> EvidenceClass {
    match role {
        SpeakerRole::User => EvidenceClass::UserStatement,
        SpeakerRole::Assistant => EvidenceClass::ModelClaim,
        _ => EvidenceClass::Unknown,
    }
}

/// Seconds since the epoch (possibly fractional) → UTC milliseconds.
fn time(value: &Value) -> (Option<String>, Option<Timestamp>, TimePrecision) {
    let Some(seconds) = value.as_f64().filter(|s| s.is_finite() && *s > 0.0) else {
        return (None, None, TimePrecision::Unknown);
    };
    let original = value.to_string();
    let millis = (seconds * 1000.0).round();
    if millis > 253_402_300_799_999.0 {
        return (Some(original), None, TimePrecision::Unknown);
    }
    let precision = if seconds.fract() == 0.0 {
        TimePrecision::Second
    } else {
        TimePrecision::Millisecond
    };
    match Timestamp::from_unix_ms(millis as i64) {
        Some(ts) => (Some(original), Some(ts), precision),
        None => (Some(original), None, TimePrecision::Unknown),
    }
}

fn attachment_for(id: &str, meta: &Value, files: &dyn FileLookup) -> ParsedAttachment {
    let name = meta["name"].as_str().map(str::to_owned);
    let claimed = meta["mime_type"].as_str().map(str::to_owned);
    match files.find(id) {
        Some(Ok((_, bytes))) => ParsedAttachment {
            original_name: name,
            claimed_media_type: claimed,
            size_bytes: Some(bytes.len() as u64),
            availability: Availability::Present,
            bytes: Some(bytes),
            external_reference: None,
        },
        Some(Err(())) => ParsedAttachment {
            original_name: name,
            claimed_media_type: claimed,
            size_bytes: None,
            availability: Availability::Quarantined,
            bytes: None,
            external_reference: None,
        },
        None => ParsedAttachment {
            original_name: name,
            claimed_media_type: claimed,
            // A declared size is not a verified one; only present bytes have a size.
            size_bytes: None,
            availability: Availability::Missing,
            bytes: None,
            external_reference: None,
        },
    }
}

fn message(
    conv_index: usize,
    node_id: &str,
    node: &Value,
    parent: ParentLink,
    wrap: &dyn Fn(String) -> Locator,
    files: &dyn FileLookup,
) -> Option<ParsedMessage> {
    let msg = &node["message"];
    let content = msg.get("content").filter(|c| c.is_object())?;
    msg.get("author").filter(|a| a.is_object())?;
    let parts = content["parts"].as_array().cloned().unwrap_or_default();
    let mut text = Vec::new();
    let mut attachments = Vec::new();
    let mut warnings = Vec::new();
    let pointer = format!(
        "/{conv_index}/mapping/{}/message/content",
        pointer_token(node_id)
    );
    let mut non_text = false;
    for part in &parts {
        match part {
            Value::String(s) => text.push(s.as_str()),
            Value::Object(obj) => {
                non_text = true;
                if let Some(asset) = obj.get("asset_pointer").and_then(Value::as_str) {
                    let file_id = asset.rsplit('/').next().unwrap_or(asset);
                    attachments.push(attachment_for(file_id, part, files));
                } else if let Some(url) = obj.get("url").and_then(Value::as_str) {
                    attachments.push(ParsedAttachment {
                        original_name: None,
                        claimed_media_type: obj
                            .get("mime_type")
                            .and_then(Value::as_str)
                            .map(str::to_owned),
                        size_bytes: None,
                        availability: Availability::ExternalReference,
                        bytes: None,
                        external_reference: Some(url.to_owned()),
                    });
                }
            }
            _ => non_text = true,
        }
    }
    for meta in msg["metadata"]["attachments"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if let Some(id) = meta["id"].as_str() {
            attachments.push(attachment_for(id, meta, files));
        }
    }
    if non_text && text.is_empty() && attachments.is_empty() {
        warnings.push(warning("non_text_content", Some(pointer.clone())));
    }
    let joined = text.join("\n");
    let role = role(msg);
    let (original_time, occurred_at, time_precision) = time(&msg["create_time"]);
    if occurred_at.is_none() {
        warnings.push(warning("time_unknown", Some(pointer.clone())));
    }
    Some(ParsedMessage {
        kind: SourceKind::ExportMessage,
        upstream_id: Some(
            msg["id"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| node_id.to_owned()),
        ),
        parent,
        locator: wrap(pointer),
        content_hash: sha256(joined.as_bytes()),
        original_time,
        occurred_at,
        time_precision,
        role,
        evidence: evidence(role),
        completeness: if non_text {
            Completeness::Partial
        } else {
            Completeness::Complete
        },
        attachments,
        warnings,
    })
}

/// Parse `conversations.json` bytes. `member` is set when the JSON came from
/// an archive member, so locators name the member and its hash.
pub fn parse(
    bytes: &[u8],
    member: Option<(&str, &Sha256Hex)>,
    files: &dyn FileLookup,
) -> Result<Vec<Unit>, &'static str> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| "invalid_json")?;
    if !probe(&value) {
        return Err("unrecognized_structure");
    }
    let wrap = |pointer: String| match member {
        Some((name, hash)) => Locator::ArchiveMember {
            member_name: name.to_owned(),
            member_hash: hash.clone(),
            inner: Box::new(Locator::JsonPointer { pointer }),
        },
        None => Locator::JsonPointer { pointer },
    };
    let mut units = Vec::new();
    for (index, conversation) in value.as_array().expect("probed").iter().enumerate() {
        let mapping = conversation["mapping"].as_object().expect("probed");
        let conversation_id = conversation["conversation_id"]
            .as_str()
            .or_else(|| conversation["id"].as_str())
            .map(str::to_owned);
        // Nearest ancestor that carries a message, or Root / Missing.
        let parent_of = |node: &Value| -> ParentLink {
            let mut current = node["parent"].as_str();
            let mut steps = 0;
            while let Some(id) = current {
                steps += 1;
                if steps > mapping.len() {
                    return ParentLink::Missing; // a parent cycle
                }
                match mapping.get(id) {
                    None => return ParentLink::Missing,
                    Some(parent) if parent["message"].is_object() => {
                        return ParentLink::Upstream(
                            parent["message"]["id"].as_str().unwrap_or(id).to_owned(),
                        );
                    }
                    Some(parent) => current = parent["parent"].as_str(),
                }
            }
            ParentLink::Root
        };
        let mut messages = Vec::new();
        let mut unparseable = 0;
        let mut ids: Vec<&String> = mapping.keys().collect();
        ids.sort();
        for node_id in ids {
            let node = &mapping[node_id];
            if node["message"].is_null() {
                continue;
            }
            match message(index, node_id, node, parent_of(node), &wrap, files) {
                Some(parsed) => messages.push(parsed),
                None => unparseable += 1,
            }
        }
        let current_leaf = conversation["current_node"].as_str().and_then(|id| {
            mapping
                .get(id)
                .map(|n| n["message"]["id"].as_str().unwrap_or(id).to_owned())
        });
        units.push(Unit {
            conversation_id,
            messages,
            current_leaf,
            threaded: true,
            unparseable,
        });
    }
    Ok(units)
}

//! Enouia Runtime native session export (format defined by this project):
//!
//! ```json
//! {"format": "enouia-runtime-session/1",
//!  "sessions": [{"session_key": "…",
//!                "events": [{"event_key": "…", "sequence": 1,
//!                            "parent_event_key": null, "role": "user",
//!                            "occurred_at": "2026-09-01T10:00:00.000Z",
//!                            "text": "…"}]}]}
//! ```
//!
//! Keys are opaque upstream identifiers from another Runtime installation.
//! Imported events become `export_message` sources (provider
//! `enouia-runtime`), never live session events of this Vault.

use crate::parsed::{ParentLink, ParsedMessage, Unit, warning};
use enouia_memory_contract::common::{EvidenceClass, Locator, SpeakerRole, TimePrecision};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::source::{Completeness, SourceKind};
use enouia_memory_contract::time::Timestamp;
use serde_json::Value;

pub const ADAPTER: &str = "enouia-runtime-session";
pub const VERSION: &str = "1";
pub const FORMAT: &str = "enouia-runtime-session/1";
pub const PROVIDER: &str = "enouia-runtime";

pub fn probe(value: &Value) -> bool {
    value["format"] == FORMAT && value["sessions"].is_array()
}

pub fn parse(value: &Value) -> Vec<Unit> {
    let mut units = Vec::new();
    for (si, session) in value["sessions"]
        .as_array()
        .into_iter()
        .flatten()
        .enumerate()
    {
        let mut messages = Vec::new();
        let mut unparseable = 0;
        let events = session["events"].as_array().cloned().unwrap_or_default();
        let keys: std::collections::BTreeSet<&str> = events
            .iter()
            .filter_map(|e| e["event_key"].as_str())
            .collect();
        let mut leaf: Option<(u64, String)> = None;
        for (ei, event) in events.iter().enumerate() {
            let (Some(key), Some(text), Some(sequence)) = (
                event["event_key"].as_str(),
                event["text"].as_str(),
                event["sequence"].as_u64(),
            ) else {
                unparseable += 1;
                continue;
            };
            let role = match event["role"].as_str() {
                Some("user") => SpeakerRole::User,
                Some("assistant") => SpeakerRole::Assistant,
                Some("tool") => SpeakerRole::Tool,
                Some("system") => SpeakerRole::System,
                _ => SpeakerRole::Unknown,
            };
            let parent = match event["parent_event_key"].as_str() {
                None => ParentLink::Root,
                Some(p) if keys.contains(p) => ParentLink::Upstream(p.to_owned()),
                Some(_) => ParentLink::Missing,
            };
            let original = event["occurred_at"].as_str();
            let occurred = original.and_then(|t| Timestamp::parse(t).ok());
            let pointer = format!("/sessions/{si}/events/{ei}/text");
            let mut warnings = Vec::new();
            if occurred.is_none() {
                warnings.push(warning("time_unknown", Some(pointer.clone())));
            }
            if leaf.as_ref().is_none_or(|(s, _)| sequence > *s) {
                leaf = Some((sequence, key.to_owned()));
            }
            messages.push(ParsedMessage {
                kind: SourceKind::ExportMessage,
                upstream_id: Some(key.to_owned()),
                parent,
                locator: Locator::JsonPointer { pointer },
                content_hash: sha256(text.as_bytes()),
                original_time: original.map(str::to_owned),
                time_precision: if occurred.is_some() {
                    TimePrecision::Millisecond
                } else {
                    TimePrecision::Unknown
                },
                occurred_at: occurred,
                role,
                evidence: match role {
                    SpeakerRole::User => EvidenceClass::UserStatement,
                    SpeakerRole::Assistant => EvidenceClass::ModelClaim,
                    _ => EvidenceClass::Unknown,
                },
                completeness: Completeness::Complete,
                attachments: Vec::new(),
                warnings,
            });
        }
        units.push(Unit {
            conversation_id: session["session_key"].as_str().map(str::to_owned),
            messages,
            current_leaf: leaf.map(|(_, k)| k),
            threaded: true,
            unparseable,
        });
    }
    units
}

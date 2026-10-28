//! Parse selected bytes without writing a root. Original objects remain exact.
use enouia_memory_contract::{
    common::{EvidenceClass, Locator, SpeakerRole, TimePrecision, Warning},
    hash::{Sha256Hex, sha256},
    time::Timestamp,
};
use enouia_memory_import::{
    detect::{Detection, detect},
    locate,
    parsed::{ParentLink, ParsedAttachment},
    zip::ZipLimits,
};
use serde_json::Value;
use std::collections::BTreeMap;

pub const PARSER: &str = "memory-init-1";
pub const MAX_MATERIAL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Format {
    /// Existing ChatGPT JSON/ZIP, Runtime-session/1, Markdown adapters.
    Auto,
    /// Claude array export, `uuid` / `chat_messages`.
    Claude,
    /// Explicit snapshot envelope, never a directory or live Vault handle.
    LegacySnapshot,
    /// JSON bucket export; raw YAML can be preserved as Unsupported.
    Buckets,
    Document,
    Unsupported,
}

#[derive(Clone)]
pub struct Material {
    /// Opaque path alias, not a filename or absolute path.
    pub alias: String,
    pub format: Format,
    pub bytes: Vec<u8>,
    pub account_alias: String,
}

/// Parser work product, not a SourceRecord or authority document.
#[derive(Clone)]
pub struct Message {
    pub raw_hash: Sha256Hex,
    pub platform: String,
    pub account_alias: String,
    pub conversation: Option<String>,
    pub conversation_title: Option<String>,
    pub upstream_id: Option<String>,
    pub parent: ParentLink,
    pub branch: Option<String>,
    pub locator: Locator,
    pub text: String,
    pub role: SpeakerRole,
    pub evidence: EvidenceClass,
    pub original_time: Option<String>,
    pub occurred_at: Option<Timestamp>,
    pub original_timezone: Option<String>,
    pub precision: TimePrecision,
    pub author: Option<String>,
    pub assistant_model: Option<String>,
    pub language: String,
    pub attachments: Vec<ParsedAttachment>,
    pub warnings: Vec<Warning>,
    /// Exact upstream metadata is retained as data, including annotations and
    /// legacy review claims. It grants no owner authority.
    pub metadata: Value,
    pub agent_submission: bool,
    pub completeness: enouia_memory_contract::source::Completeness,
    pub sensitivity: enouia_memory_contract::common::Sensitivity,
}

#[derive(Clone, Default)]
pub struct Preparation {
    pub objects: BTreeMap<Sha256Hex, Vec<u8>>,
    pub inputs: Vec<(String, Sha256Hex, usize, String, String)>,
    pub messages: Vec<Message>,
    pub unparseable: usize,
    pub unsupported: usize,
    pub duplicates: usize,
    pub duplicate_messages: usize,
    pub excluded_by_legacy_deletion: usize,
    pub selection_metadata: Value,
}
impl Preparation {
    pub fn watermark(&self) -> Sha256Hex {
        enouia_memory_contract::next::digest(&(PARSER, &self.inputs, &self.selection_metadata))
    }
}

#[derive(Clone, serde::Serialize)]
pub struct DocumentMetadata {
    pub author: Option<String>,
    pub modified_at: Option<String>,
    pub content_lang: Option<String>,
}

/// Native selected metadata, without opening a file or inventing its author.
/// A caller acquiring real documents must obtain separate selection authority.
pub fn prepare_with_document_metadata(
    materials: &[Material],
    metadata: &BTreeMap<String, DocumentMetadata>,
) -> Result<Preparation, &'static str> {
    let mut prep = prepare(materials)?;
    for (alias, meta) in metadata {
        let material = materials
            .iter()
            .find(|m| &m.alias == alias && m.format == Format::Document)
            .ok_or("init.document_metadata_scope")?;
        let hash = sha256(&material.bytes);
        for msg in prep.messages.iter_mut().filter(|m| {
            m.raw_hash == hash
                && m.platform == "document"
                && m.account_alias == material.account_alias
        }) {
            msg.author = meta.author.clone();
            msg.original_time = meta.modified_at.clone();
            msg.occurred_at = meta
                .modified_at
                .as_deref()
                .and_then(|s| Timestamp::parse(s).ok());
            msg.precision = if msg.occurred_at.is_some() {
                TimePrecision::Millisecond
            } else {
                TimePrecision::Unknown
            };
            msg.language = meta.content_lang.clone().unwrap_or_else(|| "und".into());
            msg.metadata = serde_json::to_value(meta).map_err(|_| "init.document_metadata")?;
            if msg.author.is_some() && msg.occurred_at.is_some() && msg.language != "und" {
                msg.warnings
                    .retain(|w| w.code != "author_time_language_unknown");
            }
        }
    }
    prep.selection_metadata =
        serde_json::to_value(metadata).map_err(|_| "init.document_metadata")?;
    // Reapply original-time ordering after adding explicit document timestamps.
    prep.messages
        .sort_by(|a, b| match (&a.occurred_at, &b.occurred_at) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
    Ok(prep)
}

fn warning(code: &str) -> Warning {
    Warning {
        code: code.into(),
        pointer: None,
    }
}
fn string(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_owned)
}
fn role(s: Option<&str>) -> SpeakerRole {
    match s {
        Some("user" | "human") => SpeakerRole::User,
        Some("assistant") => SpeakerRole::Assistant,
        Some("tool") => SpeakerRole::Tool,
        Some("system") => SpeakerRole::System,
        _ => SpeakerRole::Unknown,
    }
}
fn class(r: SpeakerRole) -> EvidenceClass {
    match r {
        SpeakerRole::User => EvidenceClass::UserStatement,
        SpeakerRole::Assistant => EvidenceClass::ModelClaim,
        _ => EvidenceClass::Unknown,
    }
}
fn message(raw_hash: &Sha256Hex, m: &Material, text: String, locator: Locator) -> Message {
    Message {
        raw_hash: raw_hash.clone(),
        platform: "document".into(),
        account_alias: m.account_alias.clone(),
        conversation: None,
        conversation_title: None,
        upstream_id: None,
        parent: ParentLink::Root,
        branch: None,
        locator,
        text,
        role: SpeakerRole::Unknown,
        evidence: EvidenceClass::Unknown,
        original_time: None,
        occurred_at: None,
        original_timezone: None,
        precision: TimePrecision::Unknown,
        author: None,
        assistant_model: None,
        language: "und".into(),
        attachments: Vec::new(),
        warnings: Vec::new(),
        metadata: Value::Null,
        agent_submission: false,
        completeness: enouia_memory_contract::source::Completeness::Complete,
        sensitivity: enouia_memory_contract::common::Sensitivity::Private,
    }
}

/// No I/O. Unknown time/language stays explicit. Source ordering is stable;
/// unknown timestamps sort last and are never assigned the import timestamp.
pub fn prepare(materials: &[Material]) -> Result<Preparation, &'static str> {
    let mut out = Preparation::default();
    for m in materials {
        if m.alias.is_empty()
            || m.alias.len() > 64
            || !m
                .alias
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
            || !enouia_memory_contract::source::is_account_alias(&m.account_alias)
            || m.bytes.len() > MAX_MATERIAL_BYTES
            || out.inputs.iter().any(|(alias, ..)| alias == &m.alias)
        {
            return Err("init.material_scope");
        }
        let hash = sha256(&m.bytes);
        let format = format!("{:?}", m.format);
        let already_parsed = out.inputs.iter().any(|(_, old, _, account, old_format)| {
            old == &hash && account == &m.account_alias && old_format == &format
        });
        out.inputs.push((
            m.alias.clone(),
            hash.clone(),
            m.bytes.len(),
            m.account_alias.clone(),
            format,
        ));
        out.objects.insert(hash.clone(), m.bytes.clone());
        if already_parsed {
            out.duplicates += 1;
            continue;
        }
        match m.format {
            Format::Auto => parse_auto(m, &hash, &mut out),
            Format::Claude => parse_claude(m, &hash, &mut out),
            Format::LegacySnapshot | Format::Buckets => parse_snapshot(m, &hash, &mut out),
            Format::Document => match String::from_utf8(m.bytes.clone()) {
                Ok(text) if !text.trim().is_empty() => {
                    let mut msg = message(
                        &hash,
                        m,
                        text,
                        Locator::ByteRange {
                            start: 0,
                            end: m.bytes.len() as u64,
                        },
                    );
                    msg.warnings.push(warning("author_time_language_unknown"));
                    out.messages.push(msg);
                }
                _ => out.unsupported += 1,
            },
            Format::Unsupported => out.unsupported += 1,
        }
    }
    let mut seen = BTreeMap::new();
    out.messages.retain(|m| {
        let (Some(conversation), Some(message)) = (&m.conversation, &m.upstream_id) else {
            return true;
        };
        let key = (
            m.platform.clone(),
            m.account_alias.clone(),
            conversation.clone(),
            message.clone(),
        );
        // Identical text alone is insufficient: a later export may supply a
        // missing model/timezone/attachment. Preserve it for explicit revision
        // reconciliation (currently a preflight conflict), never discard it.
        let attachments:Vec<_>=m.attachments.iter().map(|a|serde_json::json!({
            "name":a.original_name,"type":a.claimed_media_type,"size":a.size_bytes,
            "availability":a.availability,"hash":a.bytes.as_ref().map(|b|sha256(b)),"external":a.external_reference
        })).collect();
        let signature = enouia_memory_contract::next::digest(&serde_json::json!({
            "text":sha256(m.text.as_bytes()),"role":m.role,"class":m.evidence,
            "parent":format!("{:?}",m.parent),"branch":m.branch,"title":m.conversation_title,
            "time":m.original_time,"timezone":m.original_timezone,"occurred":m.occurred_at,"precision":m.precision,
            "author":m.author,"model":m.assistant_model,"language":m.language,"metadata":m.metadata,
            "attachments":attachments,"warnings":m.warnings,"complete":m.completeness,"sensitivity":m.sensitivity
        }));
        if seen.get(&key) == Some(&signature) {
            out.duplicate_messages += 1;
            false
        } else {
            seen.insert(key, signature);
            true
        }
    });
    out.messages
        .sort_by(|a, b| match (&a.occurred_at, &b.occurred_at) {
            (Some(a), Some(b)) => a.cmp(b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
    Ok(out)
}

fn parse_auto(m: &Material, hash: &Sha256Hex, out: &mut Preparation) {
    let Detection::Parsed(parsed) = detect(&m.bytes, &ZipLimits::default()) else {
        out.unsupported += 1;
        return;
    };
    let json: Value = serde_json::from_slice(&m.bytes).unwrap_or_else(|_| {
        enouia_memory_import::zip::Archive::open(&m.bytes, ZipLimits::default())
            .ok()
            .and_then(|archive| {
                archive
                    .find("conversations.json")
                    .and_then(|entry| archive.read(entry).ok())
            })
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or(Value::Null)
    });
    for (ui, unit) in parsed.units.into_iter().enumerate() {
        out.unparseable += unit.unparseable as usize;
        for p in unit.messages {
            let Ok(bytes) = locate::resolve(&m.bytes, &p.locator) else {
                out.unparseable += 1;
                continue;
            };
            if sha256(&bytes) != p.content_hash {
                out.unparseable += 1;
                continue;
            }
            let Ok(text) = String::from_utf8(bytes) else {
                out.unparseable += 1;
                continue;
            };
            let mut msg = message(hash, m, text, p.locator);
            msg.platform = parsed.provider.clone().unwrap_or_else(|| "document".into());
            msg.conversation = unit.conversation_id.clone();
            msg.upstream_id = p.upstream_id;
            msg.parent = p.parent;
            msg.role = p.role;
            msg.evidence = p.evidence;
            msg.original_time = p.original_time;
            msg.occurred_at = p.occurred_at;
            msg.precision = p.time_precision;
            msg.attachments = p.attachments;
            msg.completeness = p.completeness;
            msg.warnings = p.warnings;
            msg.warnings.extend(parsed.warnings.clone());
            // Preserve supplementary fields the v1 parser intentionally leaves
            // in Raw. Archive metadata remains reachable from Raw if not JSON.
            if let Some(conv) = json.get(ui) {
                msg.conversation_title = string(conv, "title");
                if let Some(node) = conv["mapping"].as_object().and_then(|nodes| {
                    nodes
                        .values()
                        .find(|n| n["message"]["id"].as_str() == msg.upstream_id.as_deref())
                }) {
                    let meta = &node["message"];
                    msg.author = string(&meta["author"], "name");
                    msg.assistant_model = (msg.role == SpeakerRole::Assistant)
                        .then(|| string(&meta["metadata"], "model_slug"))
                        .flatten();
                    msg.original_timezone = string(meta, "timezone");
                    msg.language = string(meta, "content_lang").unwrap_or_else(|| "und".into());
                    msg.metadata = meta.clone();
                }
            } else if let Some(sessions) = json["sessions"].as_array() {
                let session = &sessions[ui];
                msg.conversation_title = string(session, "title");
                if let Some(event) = session["events"].as_array().and_then(|events| {
                    events
                        .iter()
                        .find(|e| e["event_key"].as_str() == msg.upstream_id.as_deref())
                }) {
                    msg.branch = string(event, "branch_id");
                    msg.author = string(event, "author_label");
                    msg.original_timezone = string(event, "original_timezone");
                    msg.language = string(event, "content_lang").unwrap_or_else(|| "und".into());
                    msg.assistant_model = (msg.role == SpeakerRole::Assistant)
                        .then(|| string(event, "assistant_model"))
                        .flatten();
                    msg.metadata = event.clone();
                }
            }
            out.messages.push(msg);
        }
    }
}

fn parse_claude(m: &Material, hash: &Sha256Hex, out: &mut Preparation) {
    let Ok(Value::Array(conversations)) = serde_json::from_slice::<Value>(&m.bytes) else {
        out.unsupported += 1;
        return;
    };
    for (ci, c) in conversations.iter().enumerate() {
        let Some(messages) = c["chat_messages"].as_array() else {
            out.unparseable += 1;
            continue;
        };
        for (mi, entry) in messages.iter().enumerate() {
            let (Some(text), Some(id)) = (entry["text"].as_str(), entry["uuid"].as_str()) else {
                out.unparseable += 1;
                continue;
            };
            let mut msg = message(
                hash,
                m,
                text.into(),
                Locator::JsonPointer {
                    pointer: format!("/{ci}/chat_messages/{mi}/text"),
                },
            );
            msg.platform = "claude".into();
            msg.conversation = string(c, "uuid");
            msg.conversation_title = string(c, "name");
            msg.upstream_id = Some(id.into());
            msg.role = role(entry["sender"].as_str());
            msg.evidence = class(msg.role);
            // Absence of a parent is unknown; array position does not prove lineage.
            msg.parent = string(entry, "parent_message_uuid")
                .map(ParentLink::Upstream)
                .unwrap_or(ParentLink::Missing);
            msg.original_time = string(entry, "created_at");
            msg.occurred_at = msg
                .original_time
                .as_deref()
                .and_then(|s| Timestamp::parse(s).ok());
            msg.original_timezone = string(entry, "timezone");
            msg.precision = if msg.occurred_at.is_some() {
                TimePrecision::Millisecond
            } else {
                TimePrecision::Unknown
            };
            msg.author = string(entry, "author_label");
            msg.assistant_model = (msg.role == SpeakerRole::Assistant)
                .then(|| string(entry, "model"))
                .flatten();
            msg.language = string(entry, "content_lang").unwrap_or_else(|| "und".into());
            msg.metadata = entry.clone();
            if msg.occurred_at.is_none() {
                msg.warnings.push(warning("time_unknown"));
            }
            for a in entry["attachments"].as_array().into_iter().flatten() {
                let external = string(a, "url");
                msg.attachments.push(ParsedAttachment {
                    original_name: string(a, "file_name").or_else(|| string(a, "name")),
                    claimed_media_type: string(a, "file_type").or_else(|| string(a, "mime_type")),
                    size_bytes: None,
                    availability: if external.is_some() {
                        enouia_memory_contract::source::Availability::ExternalReference
                    } else {
                        enouia_memory_contract::source::Availability::Missing
                    },
                    bytes: None,
                    external_reference: external,
                });
            }
            out.messages.push(msg);
        }
    }
}

fn parse_snapshot(m: &Material, hash: &Sha256Hex, out: &mut Preparation) {
    let Ok(value) = serde_json::from_slice::<Value>(&m.bytes) else {
        out.unsupported += 1;
        return;
    };
    if m.format == Format::LegacySnapshot && value.is_object() {
        use enouia_memory_contract::set::{RecordSet, validate_set};
        let Ok(set) = RecordSet::from_value(&value) else {
            out.unparseable += 1;
            return;
        };
        if !validate_set(&set).is_empty() {
            out.unparseable += 1;
            return;
        }
        for (index, memory) in set.memories.iter().enumerate() {
            use enouia_memory_contract::record::RecordKind;
            let deleted = |kind, id: &str, rev| {
                set.tombstones.iter().any(|t| {
                    t.targets.iter().any(|target| {
                        target.record_kind == kind
                            && target.record_id == id
                            && target.revision.is_none_or(|r| r == rev)
                    })
                })
            };
            if deleted(
                RecordKind::Memory,
                memory.memory_id.as_str(),
                memory.revision,
            ) || memory
                .evidence
                .iter()
                .any(|e| deleted(RecordKind::Source, e.source_id.as_str(), e.source_revision))
                || memory
                    .context_source_refs
                    .iter()
                    .any(|r| deleted(RecordKind::Source, r.source_id.as_str(), r.source_revision))
            {
                out.excluded_by_legacy_deletion += 1;
                continue;
            }
            let mut msg = message(
                hash,
                m,
                memory.content.clone(),
                Locator::JsonPointer {
                    pointer: format!("/memories/{index}/content"),
                },
            );
            msg.platform = "legacy-snapshot".into();
            msg.upstream_id = Some(format!("{}:{}", memory.memory_id, memory.revision.get()));
            let exact_review = set.reviews.iter().any(|r| {
                r.review_id == memory.review_id
                    && r.actor == memory.approved_by
                    && r.final_content_hash == sha256(memory.content.as_bytes())
            });
            if exact_review {
                msg.role = SpeakerRole::User;
                msg.evidence = EvidenceClass::UserConfirmation;
            } else {
                msg.role = SpeakerRole::Assistant;
                msg.evidence = EvidenceClass::ModelClaim;
                msg.agent_submission = true;
                msg.warnings
                    .push(warning("legacy_revision_not_exact_owner_text"));
            }
            msg.original_time = Some(memory.approved_at.as_str().into());
            msg.occurred_at = Some(memory.approved_at.clone());
            msg.precision = TimePrecision::Millisecond;
            msg.author = Some("legacy-owner-review".into());
            msg.metadata = value["memories"][index].clone();
            msg.sensitivity = memory
                .sensitivity
                .max(enouia_memory_contract::common::Sensitivity::Private);
            out.messages.push(msg);
        }
        for (index, candidate) in set.candidates.iter().enumerate() {
            use enouia_memory_contract::record::RecordKind;
            let deleted = |kind, id: &str, rev| {
                set.tombstones.iter().any(|t| {
                    t.targets.iter().any(|r| {
                        r.record_kind == kind
                            && r.record_id == id
                            && r.revision.is_none_or(|v| v == rev)
                    })
                })
            };
            if deleted(
                RecordKind::Candidate,
                candidate.candidate_id.as_str(),
                candidate.revision,
            ) || set.tombstones.iter().any(|t| {
                t.targets.iter().any(|r| {
                    r.record_kind == RecordKind::Memory
                        && candidate
                            .target_memory_id
                            .as_ref()
                            .is_some_and(|id| r.record_id == id.as_str())
                })
            }) || candidate
                .evidence
                .iter()
                .any(|r| deleted(RecordKind::Source, r.source_id.as_str(), r.source_revision))
                || candidate
                    .context_source_refs
                    .iter()
                    .any(|r| deleted(RecordKind::Source, r.source_id.as_str(), r.source_revision))
            {
                out.excluded_by_legacy_deletion += 1;
                continue;
            }
            let mut msg = message(
                hash,
                m,
                candidate.proposed_content.clone(),
                Locator::JsonPointer {
                    pointer: format!("/candidates/{index}/proposed_content"),
                },
            );
            msg.platform = "legacy-snapshot".into();
            msg.agent_submission = true;
            msg.upstream_id = Some(format!(
                "{}:{}",
                candidate.candidate_id,
                candidate.revision.get()
            ));
            msg.role = SpeakerRole::Assistant;
            msg.evidence = EvidenceClass::ModelClaim;
            msg.metadata = value["candidates"][index].clone();
            msg.sensitivity = candidate
                .sensitivity
                .max(enouia_memory_contract::common::Sensitivity::Private);
            msg.warnings
                .push(warning("legacy_candidate_not_owner_statement"));
            out.messages.push(msg);
        }
        return;
    }
    let Some(entries) = value.as_array() else {
        out.unsupported += 1;
        return;
    };
    for (index, entry) in entries.iter().enumerate() {
        let Some(text) = entry["content"].as_str() else {
            out.unparseable += 1;
            continue;
        };
        let mut msg = message(
            hash,
            m,
            text.into(),
            Locator::JsonPointer {
                pointer: format!("/{index}/content"),
            },
        );
        msg.platform = if m.format == Format::Buckets {
            "bucket-json"
        } else {
            "legacy-snapshot"
        }
        .into();
        msg.upstream_id = string(entry, "memory_id").or_else(|| string(entry, "id"));
        // JSON claiming review/approval is only data. A separately validated
        // legacy owner witness is needed before UserConfirmation is assigned.
        msg.agent_submission = true;
        msg.role = SpeakerRole::Assistant;
        msg.evidence = EvidenceClass::ModelClaim;
        msg.original_time = string(entry, "created_at").or_else(|| string(entry, "created"));
        msg.occurred_at = msg
            .original_time
            .as_deref()
            .and_then(|s| Timestamp::parse(s).ok());
        msg.precision = if msg.occurred_at.is_some() {
            TimePrecision::Millisecond
        } else {
            TimePrecision::Unknown
        };
        msg.original_timezone = string(entry, "original_timezone");
        msg.language = string(entry, "content_lang").unwrap_or_else(|| "und".into());
        msg.metadata = entry.clone();
        msg.warnings.push(warning(if m.format == Format::Buckets {
            "derived_bucket_not_owner_statement"
        } else {
            "legacy_review_not_authenticated"
        }));
        out.messages.push(msg);
    }
}

//! Adapter output: format-neutral messages grouped into resumable units.
//! Adapters only describe what the received bytes say; the pipeline assigns
//! internal IDs, reconciles with earlier imports, and derives branches.

use enouia_memory_contract::common::{EvidenceClass, Locator, SpeakerRole, TimePrecision, Warning};
use enouia_memory_contract::hash::Sha256Hex;
use enouia_memory_contract::import::{AdapterRef, ArchiveMember, CursorUnit, InputKind};
use enouia_memory_contract::source::{Availability, Completeness, SourceKind};
use enouia_memory_contract::time::Timestamp;

/// How a message relates to its predecessor in the upstream data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ParentLink {
    /// First message of its conversation (or a document section).
    Root,
    /// The upstream ID of the parent message.
    Upstream(String),
    /// The upstream data names a parent that is not present.
    Missing,
}

#[derive(Clone, Debug)]
pub struct ParsedAttachment {
    pub original_name: Option<String>,
    pub claimed_media_type: Option<String>,
    pub size_bytes: Option<u64>,
    pub availability: Availability,
    /// Bytes found in the received archive (only when `Present`).
    pub bytes: Option<Vec<u8>>,
    /// Kept as text only; never fetched.
    pub external_reference: Option<String>,
}

#[derive(Clone, Debug)]
pub struct ParsedMessage {
    pub kind: SourceKind,
    /// Upstream message ID, when the format has one.
    pub upstream_id: Option<String>,
    pub parent: ParentLink,
    pub locator: Locator,
    /// SHA-256 of the message text as the adapter defines it.
    pub content_hash: Sha256Hex,
    pub original_time: Option<String>,
    pub occurred_at: Option<Timestamp>,
    pub time_precision: TimePrecision,
    pub role: SpeakerRole,
    pub evidence: EvidenceClass,
    pub completeness: Completeness,
    pub attachments: Vec<ParsedAttachment>,
    pub warnings: Vec<Warning>,
}

/// One resumable unit: a conversation, a document file, or a session.
#[derive(Clone, Debug)]
pub struct Unit {
    pub conversation_id: Option<String>,
    pub messages: Vec<ParsedMessage>,
    /// Upstream ID of the leaf the upstream marks as current, if any.
    pub current_leaf: Option<String>,
    /// Whether the messages form a reply tree (branches are derived).
    pub threaded: bool,
    pub unparseable: u64,
}

/// A recognized input, fully parsed into units in deterministic order.
#[derive(Clone, Debug)]
pub struct ParsedInput {
    pub input_kind: InputKind,
    pub adapter: AdapterRef,
    pub source_schema_observed: String,
    pub provider: Option<String>,
    pub unit_kind: CursorUnit,
    pub members: Vec<ArchiveMember>,
    pub units: Vec<Unit>,
    pub warnings: Vec<Warning>,
}

pub fn warning(code: &str, pointer: Option<String>) -> Warning {
    Warning {
        code: code.to_owned(),
        pointer,
    }
}

/// RFC 6901 escaping of one reference token.
pub fn pointer_token(token: &str) -> String {
    token.replace('~', "~0").replace('/', "~1")
}

/// File types never stored as attachment bytes (executables and scripts).
pub fn is_dangerous_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    [
        ".exe", ".dll", ".bat", ".cmd", ".com", ".scr", ".ps1", ".vbs", ".js", ".jse", ".wsf",
        ".msi", ".lnk", ".hta", ".jar", ".sh",
    ]
    .iter()
    .any(|ext| lower.ends_with(ext))
}

/// Media type from magic bytes, for the few types exports usually carry.
pub fn sniff_media_type(bytes: &[u8]) -> Option<&'static str> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if bytes.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if bytes.starts_with(b"%PDF-") {
        Some("application/pdf")
    } else if bytes.starts_with(b"PK\x03\x04") {
        Some("application/zip")
    } else if bytes.starts_with(b"MZ") {
        Some("application/x-msdownload")
    } else {
        None
    }
}

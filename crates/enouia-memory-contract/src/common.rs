//! Enumerations and small structures shared by several record kinds.

use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{EventId, PrincipalId, SourceId};
use crate::json::Revision;
use serde::{Deserialize, Serialize};

/// Sensitivity is a security axis, independent of priority and volatility.
/// Order matters: later variants are stricter.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Sensitivity {
    Public,
    Normal,
    Private,
    HighlySensitive,
}

/// Relative retrieval priority only. P0 never bypasses sensitivity or currency.
#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Priority {
    P0,
    P1,
    P2,
}

/// Currency risk only; unrelated to priority.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Volatility {
    Stable,
    Changing,
    Live,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceClass {
    UserStatement,
    UserConfirmation,
    ExternalObservation,
    ModelClaim,
    Summary,
    Unknown,
}

impl EvidenceClass {
    pub const fn is_user_evidence(self) -> bool {
        matches!(self, Self::UserStatement | Self::UserConfirmation)
    }
}

/// Speaker role is source data, never a permission.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeakerRole {
    User,
    Assistant,
    Tool,
    System,
    Unknown,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimePrecision {
    Millisecond,
    Second,
    Minute,
    Hour,
    Day,
    Month,
    Year,
    Unknown,
}

/// Authenticated principal kind. Filled by the writer from the transport's
/// authentication context, never trusted from a caller-supplied field.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ActorType {
    Owner,
    Device,
    Client,
    Agent,
    Provider,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActorRef {
    pub actor_id: PrincipalId,
    pub actor_type: ActorType,
}

/// A trusted local surface on which the owner can confirm an exact diff.
/// MCP, agents, and imported text are deliberately not members.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustedSurface {
    TrustedWindowsApp,
    TrustedLocalCli,
}

/// Where evidence lives inside the object identified by an `object_hash`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Locator {
    JsonPointer {
        pointer: String,
    },
    ByteRange {
        start: u64,
        end: u64,
    },
    RuntimeEvent {
        event_id: EventId,
    },
    ManualInput,
    ArchiveMember {
        member_name: String,
        member_hash: Sha256Hex,
        inner: Box<Locator>,
    },
}

impl Locator {
    pub fn validate(&self, path: &str, out: &mut Vec<Violation>) {
        match self {
            Self::JsonPointer { pointer } => {
                if !(pointer.is_empty() || pointer.starts_with('/')) {
                    out.push(Violation::new("locator.json_pointer", path));
                }
            }
            Self::ByteRange { start, end } => {
                if start >= end {
                    out.push(Violation::new("locator.byte_range", path));
                }
            }
            Self::RuntimeEvent { .. } | Self::ManualInput => {}
            Self::ArchiveMember {
                member_name, inner, ..
            } => {
                if !is_safe_member_name(member_name) {
                    out.push(Violation::new("locator.member_name", path));
                }
                inner.validate(&format!("{path}/inner"), out);
            }
        }
    }

    /// True when `self` addresses the same place as `other` or a byte sub-range.
    pub fn within(&self, other: &Locator) -> bool {
        match (self, other) {
            (Self::ByteRange { start, end }, Self::ByteRange { start: s, end: e }) => {
                start >= s && end <= e
            }
            (
                Self::ArchiveMember {
                    member_name,
                    member_hash,
                    inner,
                },
                Self::ArchiveMember {
                    member_name: n,
                    member_hash: h,
                    inner: i,
                },
            ) => member_name == n && member_hash == h && inner.within(i),
            (a, b) => a == b,
        }
    }
}

/// Archive member names are data; reject traversal, absolute, drive, and
/// backslash forms so no later extractor can be steered outside its root.
pub fn is_safe_member_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 512
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.chars().any(char::is_control)
        && name
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != "..")
}

/// A precise evidence citation: which source revision, where, over which bytes,
/// what kind of evidence it is, and which claim of the citing record it supports.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EvidenceRef {
    pub source_id: SourceId,
    pub source_revision: Revision,
    pub locator: Locator,
    pub object_hash: Sha256Hex,
    pub evidence_class: EvidenceClass,
    /// `"content"` for the record's main statement or an `itm_` item ID.
    pub supports: String,
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRevisionRef {
    pub source_id: SourceId,
    pub source_revision: Revision,
}

/// Physical content referenced by hash (session content, dispatch messages).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContentRef {
    pub object_hash: Sha256Hex,
    pub size_bytes: u64,
    pub media_type: String,
}

/// Structured, text-free warning from a parser or validator.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Warning {
    pub code: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub pointer: Option<String>,
}

pub fn is_code(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z'))
        && value.len() <= 64
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_'))
}

pub fn validate_warnings(warnings: &[Warning], path: &str, out: &mut Vec<Violation>) {
    for (index, warning) in warnings.iter().enumerate() {
        if !is_code(&warning.code) {
            out.push(Violation::new("warning.code", format!("{path}/{index}")));
        }
    }
}

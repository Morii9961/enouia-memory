//! Record kinds, typed references between records, and strict parsing.

use crate::error::{ContractError, Violation};
use crate::ids::check_prefixed_uuid;
use crate::json::{Revision, ensure_writable};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordKind {
    Source,
    Attachment,
    Project,
    Memory,
    Candidate,
    Review,
    Identity,
    Session,
    SessionEvent,
    Checkpoint,
    Commit,
    Tombstone,
    PurgeReceipt,
    AuditEvent,
    Capsule,
    Inspection,
    Dispatch,
    ProviderCapabilities,
}

impl RecordKind {
    /// ID prefix of the record's own identifier (None for capability snapshots).
    pub const fn id_prefix(self) -> Option<&'static str> {
        Some(match self {
            Self::Source => "src",
            Self::Attachment => "att",
            Self::Project => "prj",
            Self::Memory => "mem",
            Self::Candidate => "cand",
            Self::Review => "rvw",
            Self::Identity => "idn",
            Self::Session => "ses",
            Self::SessionEvent => "evt",
            Self::Checkpoint => "ckp",
            Self::Commit => "cmt",
            Self::Tombstone => "del",
            Self::PurgeReceipt => "prg",
            Self::AuditEvent => "aud",
            Self::Capsule => "cap",
            Self::Inspection => "insp",
            Self::Dispatch => "dsp",
            Self::ProviderCapabilities => return None,
        })
    }

    /// Kinds stored as revisioned records under `vault/records/<kind>/<id>/`.
    pub const fn is_revisioned(self) -> bool {
        matches!(
            self,
            Self::Source
                | Self::Attachment
                | Self::Project
                | Self::Memory
                | Self::Candidate
                | Self::Identity
                | Self::Session
                | Self::Checkpoint
        )
    }
}

/// `{record_kind, record_id, revision}`; the ID prefix must match the kind.
#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordRef {
    pub record_kind: RecordKind,
    pub record_id: String,
    pub revision: Revision,
}

impl RecordRef {
    pub fn new(record_kind: RecordKind, record_id: &str, revision: Revision) -> Self {
        Self {
            record_kind,
            record_id: record_id.to_owned(),
            revision,
        }
    }

    pub fn validate(&self, path: &str, out: &mut Vec<Violation>) {
        let ok = self
            .record_kind
            .id_prefix()
            .is_some_and(|prefix| check_prefixed_uuid(&self.record_id, prefix).is_ok());
        if !ok {
            out.push(Violation::new("ref.kind_prefix", path));
        }
    }
}

/// A parseable, self-validating record document.
pub trait Record: Serialize + DeserializeOwned {
    const KIND: RecordKind;
    fn validate(&self) -> Vec<Violation>;
}

macro_rules! impl_record {
    ($type:ty, $kind:ident) => {
        impl Record for $type {
            const KIND: RecordKind = RecordKind::$kind;
            fn validate(&self) -> Vec<Violation> {
                <$type>::validate(self)
            }
        }
    };
}

impl_record!(crate::source::SourceRecord, Source);
impl_record!(crate::source::AttachmentRecord, Attachment);
impl_record!(crate::memory::ProjectEntity, Project);
impl_record!(crate::memory::CanonicalMemory, Memory);
impl_record!(crate::candidate::CandidateRecord, Candidate);
impl_record!(crate::candidate::ReviewRecord, Review);
impl_record!(crate::identity::IdentityMetadata, Identity);
impl_record!(crate::session::SessionRecord, Session);
impl_record!(crate::session::SessionEvent, SessionEvent);
impl_record!(crate::session::SessionCheckpoint, Checkpoint);
impl_record!(crate::commit::CommitManifest, Commit);
impl_record!(crate::commit::Tombstone, Tombstone);
impl_record!(crate::commit::PurgeReceipt, PurgeReceipt);
impl_record!(crate::commit::AuditEvent, AuditEvent);
impl_record!(crate::context::ContextCapsule, Capsule);
impl_record!(crate::context::ContextInspection, Inspection);
impl_record!(crate::context::DispatchRecord, Dispatch);
impl_record!(crate::provider::ProviderCapabilities, ProviderCapabilities);

/// Strict parse of a JSON value: supported major version, exact shape, then
/// semantic rules. Every rejection has a stable rule identifier.
pub fn parse_value<T: Record>(value: &Value) -> Result<T, ContractError> {
    if !value.is_object() {
        return Err(ContractError::Malformed);
    }
    ensure_writable(value)?;
    let mut ranges = Vec::new();
    crate::json::check_safe_integers(value, "", &mut ranges);
    if !ranges.is_empty() {
        return Err(ContractError::Invalid(ranges));
    }
    let record: T = serde_json::from_value(value.clone())
        .map_err(|e| ContractError::Shape(shape_message(&e)))?;
    let violations = record.validate();
    if violations.is_empty() {
        Ok(record)
    } else {
        Err(ContractError::Invalid(violations))
    }
}

pub fn parse_record<T: Record>(bytes: &[u8]) -> Result<T, ContractError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ContractError::Malformed)?;
    parse_value(&value)
}

/// Local diagnostic only (serde may name an offending enum value). IPC errors
/// carry `MemoryErrorCode::InvalidRequest`, never this text.
fn shape_message(error: &serde_json::Error) -> String {
    let text = error.to_string();
    text.split(" at line ").next().unwrap_or("").to_owned()
}

/// Kind-erased record for fixtures, diagnostics, and generic tooling.
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Debug, PartialEq)]
pub enum AnyRecord {
    Source(crate::source::SourceRecord),
    Attachment(crate::source::AttachmentRecord),
    Project(crate::memory::ProjectEntity),
    Memory(Box<crate::memory::CanonicalMemory>),
    Candidate(Box<crate::candidate::CandidateRecord>),
    Review(crate::candidate::ReviewRecord),
    Identity(crate::identity::IdentityMetadata),
    Session(crate::session::SessionRecord),
    SessionEvent(crate::session::SessionEvent),
    Checkpoint(crate::session::SessionCheckpoint),
    Commit(crate::commit::CommitManifest),
    Tombstone(crate::commit::Tombstone),
    PurgeReceipt(crate::commit::PurgeReceipt),
    AuditEvent(crate::commit::AuditEvent),
    Capsule(Box<crate::context::ContextCapsule>),
    Inspection(crate::context::ContextInspection),
    Dispatch(crate::context::DispatchRecord),
    ProviderCapabilities(crate::provider::ProviderCapabilities),
}

impl AnyRecord {
    pub fn to_value(&self) -> Value {
        let result = match self {
            Self::Source(r) => serde_json::to_value(r),
            Self::Attachment(r) => serde_json::to_value(r),
            Self::Project(r) => serde_json::to_value(r),
            Self::Memory(r) => serde_json::to_value(r),
            Self::Candidate(r) => serde_json::to_value(r),
            Self::Review(r) => serde_json::to_value(r),
            Self::Identity(r) => serde_json::to_value(r),
            Self::Session(r) => serde_json::to_value(r),
            Self::SessionEvent(r) => serde_json::to_value(r),
            Self::Checkpoint(r) => serde_json::to_value(r),
            Self::Commit(r) => serde_json::to_value(r),
            Self::Tombstone(r) => serde_json::to_value(r),
            Self::PurgeReceipt(r) => serde_json::to_value(r),
            Self::AuditEvent(r) => serde_json::to_value(r),
            Self::Capsule(r) => serde_json::to_value(r),
            Self::Inspection(r) => serde_json::to_value(r),
            Self::Dispatch(r) => serde_json::to_value(r),
            Self::ProviderCapabilities(r) => serde_json::to_value(r),
        };
        result.unwrap_or(Value::Null)
    }
}

pub fn parse_any(kind: RecordKind, value: &Value) -> Result<AnyRecord, ContractError> {
    Ok(match kind {
        RecordKind::Source => AnyRecord::Source(parse_value(value)?),
        RecordKind::Attachment => AnyRecord::Attachment(parse_value(value)?),
        RecordKind::Project => AnyRecord::Project(parse_value(value)?),
        RecordKind::Memory => AnyRecord::Memory(Box::new(parse_value(value)?)),
        RecordKind::Candidate => AnyRecord::Candidate(Box::new(parse_value(value)?)),
        RecordKind::Review => AnyRecord::Review(parse_value(value)?),
        RecordKind::Identity => AnyRecord::Identity(parse_value(value)?),
        RecordKind::Session => AnyRecord::Session(parse_value(value)?),
        RecordKind::SessionEvent => AnyRecord::SessionEvent(parse_value(value)?),
        RecordKind::Checkpoint => AnyRecord::Checkpoint(parse_value(value)?),
        RecordKind::Commit => AnyRecord::Commit(parse_value(value)?),
        RecordKind::Tombstone => AnyRecord::Tombstone(parse_value(value)?),
        RecordKind::PurgeReceipt => AnyRecord::PurgeReceipt(parse_value(value)?),
        RecordKind::AuditEvent => AnyRecord::AuditEvent(parse_value(value)?),
        RecordKind::Capsule => AnyRecord::Capsule(Box::new(parse_value(value)?)),
        RecordKind::Inspection => AnyRecord::Inspection(parse_value(value)?),
        RecordKind::Dispatch => AnyRecord::Dispatch(parse_value(value)?),
        RecordKind::ProviderCapabilities => AnyRecord::ProviderCapabilities(parse_value(value)?),
    })
}

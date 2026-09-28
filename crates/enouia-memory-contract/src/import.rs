//! ImportManifest (IMPORT_REVIEW §2–3, ADR-MEM-38): the versioned record of
//! one user-selected import, stored at `vault/raw/manifests/<id>/<rev>.json`.
//!
//! A new revision is committed at each step (archived, each parse batch,
//! completion) together with the sources that step produced, so the resume
//! cursor, the parser version, and the counts always describe exactly what
//! the same commit contains. Raw completeness (the received bytes) and parse
//! completeness are separate: an unsupported format is still archived.

use crate::common::{Warning, is_code, validate_warnings};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::ImportId;
use crate::json::{Extensions, Revision, SchemaVersion, validate_extensions};
use crate::source::is_account_alias;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    Planned,
    Archiving,
    Archived,
    Parsing,
    Completed,
    Partial,
    Failed,
}

/// What the received bytes were recognized as. `unknown` is still archived.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InputKind {
    ChatgptExportZip,
    ChatgptConversationsJson,
    MarkdownFile,
    MarkdownArchiveZip,
    RuntimeNativeSession,
    UnknownArchive,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdapterRef {
    pub name: String,
    pub version: String,
}

/// What happened to one member of a received archive.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemberDisposition {
    /// Parsed into sources.
    Parsed,
    /// Kept only as part of the archived bytes (e.g. an attachment file).
    PreservedOnly,
    /// Refused as dangerous or malformed; never extracted.
    Quarantined,
    /// Not needed by the adapter.
    Skipped,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArchiveMember {
    /// Sanitized relative member name; never an absolute or `..` path.
    pub member_name: String,
    /// SHA-256 of the member's decompressed bytes (`null` when not read).
    #[serde(deserialize_with = "crate::json::nullable")]
    pub member_hash: Option<Sha256Hex>,
    pub size_bytes: u64,
    pub disposition: MemberDisposition,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub reason_code: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CursorUnit {
    Conversation,
    File,
    Event,
}

/// Resume point: units fully committed so far. Units are processed in the
/// adapter's deterministic order, so a resumed import skips exactly these.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportCursor {
    pub unit: CursorUnit,
    pub completed: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub total: Option<u64>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportCounts {
    pub sources_created: u64,
    pub sources_revised: u64,
    pub sources_unchanged: u64,
    pub conversations: u64,
    pub branches: u64,
    pub missing_parents: u64,
    pub unparseable: u64,
    pub attachments_present: u64,
    pub attachments_missing: u64,
    pub attachments_external: u64,
    pub attachments_quarantined: u64,
}

/// Coverage of one upstream conversation (or one Markdown file). No titles:
/// only upstream IDs, counts, and times.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationCoverage {
    #[serde(deserialize_with = "crate::json::nullable")]
    pub original_conversation_id: Option<String>,
    /// Sources produced for this conversation by this import.
    pub message_count: u64,
    pub branch_count: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub earliest_source_time: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub latest_source_time: Option<Timestamp>,
    pub unknown_time_count: u64,
    pub missing_parents: u64,
    pub unparseable: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ImportManifest {
    pub schema_version: SchemaVersion,
    pub import_id: ImportId,
    pub revision: Revision,
    pub status: ImportStatus,
    pub input_kind: InputKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub provider: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub account_scope: Option<String>,
    /// The exact bytes received, archived as a Raw object before parsing.
    pub input_object_hash: Sha256Hex,
    pub input_size_bytes: u64,
    pub received_at: Timestamp,
    pub updated_at: Timestamp,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub adapter: Option<AdapterRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub source_schema_observed: Option<String>,
    pub members: Vec<ArchiveMember>,
    pub cursor: ImportCursor,
    pub counts: ImportCounts,
    pub coverage: Vec<ConversationCoverage>,
    pub warnings: Vec<Warning>,
    /// Set when these exact bytes were already imported: nothing new is parsed.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub duplicate_of: Option<ImportId>,
    pub extensions: Extensions,
}

fn is_label(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z' | '0'..='9'))
        && value.len() <= 128
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | ':' | '-' | '/'))
}

/// A sanitized archive member name: relative, `/`-separated, no empty, `.`,
/// or `..` segment, no drive, colon, or backslash, at most 512 bytes.
pub fn is_safe_member_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 512
        && !name.starts_with('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.chars().any(char::is_control)
        && name
            .split('/')
            .all(|s| !s.is_empty() && s != "." && s != "..")
}

impl ImportManifest {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let parsing_done = matches!(self.status, ImportStatus::Parsing | ImportStatus::Completed);
        if parsing_done && self.adapter.is_none() {
            out.push(Violation::new("import.adapter_required", "/adapter"));
        }
        if let Some(adapter) = &self.adapter
            && (!is_label(&adapter.name) || !is_label(&adapter.version))
        {
            out.push(Violation::new("import.adapter_label", "/adapter"));
        }
        if let Some(total) = self.cursor.total
            && self.cursor.completed > total
        {
            out.push(Violation::new("import.cursor", "/cursor"));
        }
        if self.status == ImportStatus::Completed
            && self.cursor.total != Some(self.cursor.completed)
        {
            out.push(Violation::new("import.cursor", "/cursor"));
        }
        if matches!(self.status, ImportStatus::Planned | ImportStatus::Archiving)
            && (self.cursor.completed != 0 || self.counts != ImportCounts::default())
        {
            out.push(Violation::new("import.premature_parse", "/cursor"));
        }
        if self.received_at > self.updated_at {
            out.push(Violation::new("import.time_order", "/updated_at"));
        }
        if let Some(alias) = &self.account_scope
            && !is_account_alias(alias)
        {
            out.push(Violation::new("source.account_alias", "/account_scope"));
        }
        if self
            .source_schema_observed
            .as_ref()
            .is_some_and(|s| !is_label(s))
        {
            out.push(Violation::new(
                "import.adapter_label",
                "/source_schema_observed",
            ));
        }
        let mut names = BTreeSet::new();
        for (index, member) in self.members.iter().enumerate() {
            if !is_safe_member_name(&member.member_name) || !names.insert(&member.member_name) {
                out.push(Violation::new(
                    "import.member_name",
                    format!("/members/{index}/member_name"),
                ));
            }
            if member.disposition == MemberDisposition::Parsed && member.member_hash.is_none() {
                out.push(Violation::new(
                    "import.member_hash",
                    format!("/members/{index}/member_hash"),
                ));
            }
            if member.reason_code.as_ref().is_some_and(|c| !is_code(c)) {
                out.push(Violation::new(
                    "warning.code",
                    format!("/members/{index}/reason_code"),
                ));
            }
        }
        for (index, coverage) in self.coverage.iter().enumerate() {
            if let (Some(a), Some(b)) =
                (&coverage.earliest_source_time, &coverage.latest_source_time)
                && a > b
            {
                out.push(Violation::new(
                    "import.coverage_order",
                    format!("/coverage/{index}"),
                ));
            }
            if coverage.unknown_time_count > coverage.message_count {
                out.push(Violation::new(
                    "import.coverage_count",
                    format!("/coverage/{index}"),
                ));
            }
        }
        if self.duplicate_of.is_some()
            && (self.counts.sources_created != 0 || self.counts.sources_revised != 0)
        {
            out.push(Violation::new("import.duplicate", "/duplicate_of"));
        }
        if self.duplicate_of.as_ref() == Some(&self.import_id) {
            out.push(Violation::new("import.duplicate", "/duplicate_of"));
        }
        validate_warnings(&self.warnings, "/warnings", &mut out);
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn member_names_must_stay_relative() {
        assert!(is_safe_member_name("conversations.json"));
        assert!(is_safe_member_name("files/图片 1.png"));
        for bad in [
            "",
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            "C:x",
            "a\\b",
            "./a",
            "a\u{0}b",
        ] {
            assert!(!is_safe_member_name(bad), "{bad:?}");
        }
    }
}

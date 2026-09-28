//! SourceRecord and AttachmentRecord (DATA_MODEL §2).

use crate::common::{
    ActorRef, ActorType, EvidenceClass, Locator, Sensitivity, SpeakerRole, TimePrecision,
    TrustedSurface, Warning, validate_warnings,
};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{AttachmentId, BranchId, ImportId, PolicyId, SourceId};
use crate::json::{Extensions, Knowable, Revision, SchemaVersion, validate_extensions};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SourceKind {
    ExportMessage,
    RuntimeEvent,
    ImportedDocument,
    ManualAssertion,
    ExternalEvent,
    AgentSubmission,
}

impl SourceKind {
    pub const fn is_import(self) -> bool {
        matches!(self, Self::ExportMessage | Self::ImportedDocument)
    }

    pub const fn is_external(self) -> bool {
        matches!(self, Self::ExportMessage | Self::ExternalEvent)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Completeness {
    Complete,
    Partial,
    MetadataOnly,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConfirmationMethod {
    ExactTextConfirmDialog,
    TypedConfirmation,
}

/// What the owner actually typed and confirmed on a trusted surface. It proves
/// Morii confirmed the statement, not that the world independently verified it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ManualAssertion {
    pub input_text: String,
    pub operator: ActorRef,
    pub trusted_surface: TrustedSurface,
    pub confirmation_method: ConfirmationMethod,
    pub confirmed_at: Timestamp,
}

/// Text handed over by an external agent/tool. `claimed_user_consent` is kept
/// as data only; it never becomes a manual assertion or approval.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSubmission {
    pub submitting_principal: ActorRef,
    pub submitted_text: String,
    pub claimed_user_consent: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecord {
    pub schema_version: SchemaVersion,
    pub source_id: SourceId,
    pub revision: Revision,
    pub source_kind: SourceKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub provider: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub account_scope: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub import_id: Option<ImportId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub raw_object_hash: Option<Sha256Hex>,
    /// Digest of the exact evidence bytes the locator selects.
    pub content_hash: Sha256Hex,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub original_conversation_id: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub original_message_id: Option<String>,
    pub parent_source_ids: Knowable<Vec<SourceId>>,
    pub branch_id: Knowable<Option<BranchId>>,
    pub locator: Locator,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub original_time: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub original_timezone: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub occurred_at: Option<Timestamp>,
    pub captured_at: Timestamp,
    pub time_precision: TimePrecision,
    pub speaker_role: SpeakerRole,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub author_label: Option<String>,
    pub evidence_class: EvidenceClass,
    pub completeness: Completeness,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    pub attachment_refs: Vec<AttachmentId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parser_version: Option<String>,
    pub parse_warnings: Vec<Warning>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub manual_assertion: Option<ManualAssertion>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub agent_submission: Option<AgentSubmission>,
    pub created_at: Timestamp,
    pub extensions: Extensions,
}

/// Local opaque account alias: never an e-mail, login, or credential.
pub fn is_account_alias(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z' | '0'..='9'))
        && value.len() <= 64
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '-'))
}

impl SourceRecord {
    /// The object an evidence citation's `object_hash` must name: the received
    /// raw object for imports, otherwise the evidence content itself.
    pub fn anchor_hash(&self) -> &Sha256Hex {
        self.raw_object_hash.as_ref().unwrap_or(&self.content_hash)
    }

    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let kind = self.source_kind;
        if (kind == SourceKind::ManualAssertion) != self.manual_assertion.is_some() {
            out.push(Violation::new("source.kind_fields", "/manual_assertion"));
        }
        if (kind == SourceKind::AgentSubmission) != self.agent_submission.is_some() {
            out.push(Violation::new("source.kind_fields", "/agent_submission"));
        }
        if kind.is_import() != (self.import_id.is_some() && self.raw_object_hash.is_some())
            || (!kind.is_import() && (self.import_id.is_some() || self.raw_object_hash.is_some()))
        {
            out.push(Violation::new("source.import_fields", "/import_id"));
        }
        if kind.is_external() && (self.provider.is_none() || self.account_scope.is_none()) {
            out.push(Violation::new("source.external_fields", "/provider"));
        }
        if let Some(alias) = &self.account_scope
            && !is_account_alias(alias)
        {
            out.push(Violation::new("source.account_alias", "/account_scope"));
        }
        let locator_ok = match (&self.locator, kind) {
            (Locator::ManualInput, SourceKind::ManualAssertion) => true,
            (_, SourceKind::ManualAssertion) | (Locator::ManualInput, _) => false,
            (Locator::RuntimeEvent { .. }, SourceKind::RuntimeEvent) => true,
            (_, SourceKind::RuntimeEvent) | (Locator::RuntimeEvent { .. }, _) => false,
            _ => true,
        };
        if !locator_ok {
            out.push(Violation::new("source.locator_kind", "/locator"));
        }
        self.locator.validate("/locator", &mut out);
        self.validate_roles(&mut out);
        if self.occurred_at.is_none() != (self.time_precision == TimePrecision::Unknown) {
            out.push(Violation::new("source.time_precision", "/time_precision"));
        }
        if self.created_at < self.captured_at {
            out.push(Violation::new("source.time_order", "/created_at"));
        }
        validate_warnings(&self.parse_warnings, "/parse_warnings", &mut out);
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }

    /// Speaker role and evidence class may never be confused: an assistant or
    /// agent cannot produce user evidence, and user evidence needs a user speaker.
    fn validate_roles(&self, out: &mut Vec<Violation>) {
        let class = self.evidence_class;
        let role = self.speaker_role;
        let confused = (class.is_user_evidence() && role != SpeakerRole::User)
            || (role == SpeakerRole::Assistant
                && !matches!(
                    class,
                    EvidenceClass::ModelClaim | EvidenceClass::Summary | EvidenceClass::Unknown
                ))
            || (self.source_kind == SourceKind::AgentSubmission
                && (role == SpeakerRole::User
                    || !matches!(
                        class,
                        EvidenceClass::ModelClaim | EvidenceClass::Summary | EvidenceClass::Unknown
                    )))
            || (self.source_kind == SourceKind::ManualAssertion
                && (role != SpeakerRole::User || !class.is_user_evidence()));
        if confused {
            out.push(Violation::new("source.role_confusion", "/evidence_class"));
        }
        if let Some(manual) = &self.manual_assertion
            && manual.operator.actor_type != ActorType::Owner
        {
            out.push(Violation::new(
                "source.manual_operator",
                "/manual_assertion/operator",
            ));
        }
        if let Some(agent) = &self.agent_submission
            && agent.submitting_principal.actor_type == ActorType::Owner
        {
            out.push(Violation::new(
                "source.agent_principal",
                "/agent_submission/submitting_principal",
            ));
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Present,
    Missing,
    ExternalReference,
    Quarantined,
    Unsupported,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttachmentRecord {
    pub schema_version: SchemaVersion,
    pub attachment_id: AttachmentId,
    pub revision: Revision,
    pub source_id: SourceId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub original_name: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub claimed_media_type: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub detected_media_type: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub size_bytes: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub object_hash: Option<Sha256Hex>,
    pub availability: Availability,
    /// An upstream URL kept as inert data. It is never fetched automatically.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub external_reference: Option<String>,
    pub sensitivity: Sensitivity,
    pub created_at: Timestamp,
    pub extensions: Extensions,
}

impl AttachmentRecord {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let present = self.availability == Availability::Present;
        if present != self.object_hash.is_some() || present != self.size_bytes.is_some() {
            out.push(Violation::new(
                "attachment.availability_hash",
                "/object_hash",
            ));
        }
        if (self.availability == Availability::ExternalReference)
            != self.external_reference.is_some()
        {
            out.push(Violation::new(
                "attachment.external_reference",
                "/external_reference",
            ));
        }
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}

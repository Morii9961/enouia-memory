//! Context Capsule, Inspection, and Dispatch records (CONTEXT_MODEL §1, §6).
//! Capsule = logical context; Inspection = local explanation; Dispatch = what
//! was actually sent. All three share `request_id`; Inspection is never sent.

use crate::common::{Sensitivity, SourceRevisionRef, TimePrecision};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::identity::IdentitySlug;
use crate::ids::{
    BranchId, CapsuleId, CheckpointId, CommitId, ConflictGroupId, DispatchId, EventId, IdentityId,
    InspectionId, ItemId, MemoryId, PolicyId, RequestId, ReviewId, SessionId,
};
use crate::json::{Revision, SchemaVersion};
use crate::memory::MemoryType;
use crate::record::RecordKind;
use crate::scan::{any_string, contains_local_path, contains_secret_material};
use crate::session::{ClientSurface, ProviderBinding};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DestinationKind {
    LocalMock,
    LocalModel,
    ExternalProvider,
}

impl DestinationKind {
    pub const fn is_local(self) -> bool {
        matches!(self, Self::LocalMock | Self::LocalModel)
    }
}

/// The server derives and verifies destination; a client cannot claim `local`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Destination {
    pub kind: DestinationKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub provider_binding: Option<ProviderBinding>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    Answer,
    ContinueSession,
    Checkpoint,
    Extraction,
    InspectionPreview,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Currency {
    CurrentSupported,
    HistoricalOnly,
    NeedsReverification,
    Conflicted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityItem {
    pub identity_id: IdentityId,
    pub revision: Revision,
    pub slug: IdentitySlug,
    pub text: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryItem {
    pub memory_id: MemoryId,
    pub revision: Revision,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub content: String,
    pub currency: Currency,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub valid_from: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub valid_until: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub last_verified_at: Option<Timestamp>,
    pub evidence: Vec<SourceRevisionRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub conflict_group_id: Option<ConflictGroupId>,
    pub sensitivity: Sensitivity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointItemStatus {
    Provisional,
    Reviewed,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CheckpointItem {
    pub checkpoint_id: CheckpointId,
    pub revision: Revision,
    pub status: CheckpointItemStatus,
    pub summary: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TurnRole {
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TurnItem {
    pub event_id: EventId,
    pub role: TurnRole,
    pub text: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LoopOrigin {
    Memory,
    Checkpoint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenLoopItem {
    pub item_id: ItemId,
    pub description: String,
    pub origin: LoopOrigin,
    pub origin_id: String,
    /// True when the loop comes from an unreviewed (provisional) checkpoint.
    pub provisional: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProvenanceItem {
    pub source_id: crate::ids::SourceId,
    pub source_revision: Revision,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub occurred_at: Option<Timestamp>,
    pub time_precision: TimePrecision,
    /// Locator kind only (e.g. `json_pointer`); never a local path.
    pub locator_kind: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CountingMethod {
    /// Fixed conservative MV-0/Mock estimator: UTF-8 bytes of content plus
    /// declared wrapper overhead. Deterministic, not a tokenizer upper bound.
    Utf8BytesV1,
    ProviderExact,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Budget {
    pub max_tokens: u64,
    pub memory_budget_tokens: u64,
    pub estimated_tokens: u64,
    pub counting_method: CountingMethod,
    pub safety_margin_tokens: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationReason {
    ImplementationUnverified,
    LiveValue,
    ReviewOverdue,
    Conflicted,
    NoSupportedEvidence,
    SupersessionTimeUnknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VerificationNeeded {
    #[serde(deserialize_with = "crate::json::nullable")]
    pub memory_id: Option<MemoryId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub item_id: Option<ItemId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub conflict_group_id: Option<ConflictGroupId>,
    pub reason: VerificationReason,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Limitation {
    OverBudget,
    PolicyLimited,
    IndexNotReady,
    NoSupportedMemory,
    BrokenProvenanceOmitted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Completeness {
    pub complete: bool,
    pub limitations: Vec<Limitation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextCapsule {
    pub schema_version: SchemaVersion,
    pub capsule_id: CapsuleId,
    pub generated_at: Timestamp,
    pub request_id: RequestId,
    pub query: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub as_of: Option<Timestamp>,
    pub vault_commit_id: CommitId,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub compiler_version: String,
    pub ranking_version: String,
    pub tokenizer_version: String,
    pub client_surface: ClientSurface,
    pub destination: Destination,
    pub purpose: Purpose,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub session_id: Option<SessionId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub branch_id: Option<BranchId>,
    pub identity: Vec<IdentityItem>,
    pub user_context: Vec<MemoryItem>,
    pub relationship_context: Vec<MemoryItem>,
    pub active_projects: Vec<MemoryItem>,
    pub relevant_memories: Vec<MemoryItem>,
    pub recent_session_checkpoints: Vec<CheckpointItem>,
    pub recent_turns: Vec<TurnItem>,
    pub open_loops: Vec<OpenLoopItem>,
    pub provenance: Vec<ProvenanceItem>,
    pub budget: Budget,
    pub verification_needed: Vec<VerificationNeeded>,
    pub completeness: Completeness,
}

impl ContextCapsule {
    pub fn memory_items(&self) -> impl Iterator<Item = &MemoryItem> {
        self.user_context
            .iter()
            .chain(&self.relationship_context)
            .chain(&self.active_projects)
            .chain(&self.relevant_memories)
    }

    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let value = serde_json::to_value(self).unwrap_or_default();
        if any_string(&value, &contains_local_path) {
            out.push(Violation::new("capsule.local_path", "/"));
        }
        if any_string(&value, &contains_secret_material) {
            out.push(Violation::new("capsule.secret_material", "/"));
        }
        let budget = &self.budget;
        // Checked: a hostile budget must yield a violation, never an overflow.
        let needed = budget
            .estimated_tokens
            .checked_add(budget.safety_margin_tokens);
        if budget.memory_budget_tokens > budget.max_tokens
            || needed.is_none_or(|needed| needed > budget.max_tokens)
        {
            out.push(Violation::new("capsule.budget", "/budget"));
        }
        if self.completeness.complete != self.completeness.limitations.is_empty() {
            out.push(Violation::new("capsule.completeness", "/completeness"));
        }
        let needs_binding = self.destination.kind != DestinationKind::LocalMock;
        if needs_binding && self.destination.provider_binding.is_none() {
            out.push(Violation::new(
                "capsule.destination_binding",
                "/destination",
            ));
        }
        let mut seen = BTreeSet::new();
        for item in self.memory_items() {
            if !seen.insert(&item.memory_id) {
                out.push(Violation::new(
                    "capsule.duplicate_memory",
                    item.memory_id.as_str(),
                ));
            }
            if item.evidence.is_empty() {
                out.push(Violation::new(
                    "capsule.item_evidence",
                    item.memory_id.as_str(),
                ));
            }
            if item.currency == Currency::Conflicted && item.conflict_group_id.is_none() {
                out.push(Violation::new(
                    "capsule.conflict_flag",
                    item.memory_id.as_str(),
                ));
            }
        }
        if self.session_id.is_none() && self.branch_id.is_some() {
            out.push(Violation::new(
                "capsule.branch_without_session",
                "/branch_id",
            ));
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerScope {
    OwnerFull,
    Restricted,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Included,
    Excluded,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionReason {
    ExplicitEntityMatch,
    DirectSupport,
    CurrentSupported,
    HistoricalMatch,
    IdentityRequired,
    SessionContinuity,
    Unrelated,
    Superseded,
    Expired,
    NotYetEffective,
    Conflicted,
    Pending,
    Rejected,
    PolicyDenied,
    OverBudget,
    BrokenProvenance,
    Tombstoned,
    Archived,
}

impl DecisionReason {
    pub const fn is_inclusion(self) -> bool {
        matches!(
            self,
            Self::ExplicitEntityMatch
                | Self::DirectSupport
                | Self::CurrentSupported
                | Self::HistoricalMatch
                | Self::IdentityRequired
                | Self::SessionContinuity
        )
    }

    /// Reasons that would disclose hidden records to a restricted viewer.
    pub const fn is_hidden(self) -> bool {
        matches!(self, Self::PolicyDenied | Self::Tombstoned)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InspectionDecision {
    pub record_kind: RecordKind,
    pub record_id: String,
    pub revision: Revision,
    pub decision: Decision,
    pub reason: DecisionReason,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub rank: Option<u32>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub token_cost: Option<u64>,
    pub source_reachable: bool,
    pub truncated: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContextInspection {
    pub schema_version: SchemaVersion,
    pub inspection_id: InspectionId,
    pub capsule_id: CapsuleId,
    pub request_id: RequestId,
    pub generated_at: Timestamp,
    pub vault_commit_id: CommitId,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub ranking_version: String,
    pub viewer_scope: ViewerScope,
    pub decisions: Vec<InspectionDecision>,
}

impl ContextInspection {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let mut seen = BTreeSet::new();
        for (index, decision) in self.decisions.iter().enumerate() {
            let path = format!("/decisions/{index}");
            if !matches!(
                decision.record_kind,
                RecordKind::Memory
                    | RecordKind::Candidate
                    | RecordKind::Checkpoint
                    | RecordKind::Identity
            ) {
                out.push(Violation::new("inspection.record_kind", path.clone()));
            }
            crate::record::RecordRef::new(
                decision.record_kind,
                &decision.record_id,
                decision.revision,
            )
            .validate(&path, &mut out);
            if (decision.decision == Decision::Included) != decision.reason.is_inclusion() {
                out.push(Violation::new("inspection.reason", path.clone()));
            }
            if self.viewer_scope == ViewerScope::Restricted && decision.reason.is_hidden() {
                out.push(Violation::new("inspection.hidden_disclosure", path.clone()));
            }
            if !seen.insert((decision.record_kind, decision.record_id.clone())) {
                out.push(Violation::new("inspection.duplicate", path));
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MessageRole {
    System,
    User,
    Assistant,
    Tool,
}

#[derive(Clone, Debug, Eq, PartialEq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRevisionRef {
    pub memory_id: MemoryId,
    pub revision: Revision,
}

/// One rendered message: role, hash/size of the exact bytes (full text lives in
/// a private Session object), and which capsule memories it carries.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchMessage {
    pub role: MessageRole,
    pub content_hash: Sha256Hex,
    pub size_bytes: u64,
    pub capsule_memory_refs: Vec<MemoryRevisionRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchTool {
    pub name: String,
    pub definition_hash: Sha256Hex,
}

/// Output configuration that is part of the approved request.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OutputConfig {
    pub max_output_tokens: u64,
    pub streaming: bool,
}

pub const REQUEST_PAYLOAD_VERSION: u64 = 1;

#[derive(Serialize)]
struct PayloadMessage<'a> {
    role: MessageRole,
    content_hash: &'a Sha256Hex,
    size_bytes: u64,
}

/// The canonical request payload whose digest is `request_hash`. It covers
/// the destination, every message (role, SHA-256 of its exact UTF-8 bytes,
/// byte size, in order), every tool (name and SHA-256 of its exact definition
/// bytes, in order) and the output configuration. Encoding: canonical JSON
/// bytes (`json::canonical_bytes`) of
/// `{payload_version, destination, messages, tools, output}`.
/// Approvals bind to this digest, so any change to content, order, tools,
/// output, or destination requires a new approval.
pub fn request_payload_hash(
    destination: &Destination,
    messages: &[(MessageRole, &Sha256Hex, u64)],
    tools: &[DispatchTool],
    output: &OutputConfig,
) -> Sha256Hex {
    let messages: Vec<PayloadMessage<'_>> = messages
        .iter()
        .map(|(role, content_hash, size_bytes)| PayloadMessage {
            role: *role,
            content_hash,
            size_bytes: *size_bytes,
        })
        .collect();
    let payload = serde_json::json!({
        "payload_version": REQUEST_PAYLOAD_VERSION,
        "destination": destination,
        "messages": messages,
        "tools": tools,
        "output": output,
    });
    let bytes = crate::json::canonical_bytes(&payload).unwrap_or_default();
    crate::hash::sha256(&bytes)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EgressDecision {
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub egress_policy_id: Option<PolicyId>,
    /// Required when private content goes to an external destination.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub confirmation_review_id: Option<ReviewId>,
    pub checked_at: Timestamp,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DispatchState {
    Prepared,
    Sent,
    Completed,
    Failed,
    Cancelled,
    /// Sent but the process could not learn whether the destination received it.
    OutcomeUnknown,
}

/// What was actually sent. Keys and auth headers are never recorded.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DispatchRecord {
    pub schema_version: SchemaVersion,
    pub dispatch_id: DispatchId,
    pub capsule_id: CapsuleId,
    pub inspection_id: InspectionId,
    pub request_id: RequestId,
    pub destination: Destination,
    pub request_hash: Sha256Hex,
    pub messages: Vec<DispatchMessage>,
    pub tools: Vec<DispatchTool>,
    pub output: OutputConfig,
    pub egress: EgressDecision,
    pub state: DispatchState,
    pub prepared_at: Timestamp,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub sent_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub completed_at: Option<Timestamp>,
}

impl DispatchRecord {
    pub fn memory_refs(&self) -> BTreeSet<&MemoryRevisionRef> {
        self.messages
            .iter()
            .flat_map(|m| &m.capsule_memory_refs)
            .collect()
    }

    /// Digest recomputed from this record's own destination, messages, tools,
    /// and output configuration.
    pub fn computed_request_hash(&self) -> Sha256Hex {
        let messages: Vec<(MessageRole, &Sha256Hex, u64)> = self
            .messages
            .iter()
            .map(|m| (m.role, &m.content_hash, m.size_bytes))
            .collect();
        request_payload_hash(&self.destination, &messages, &self.tools, &self.output)
    }

    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.computed_request_hash() != self.request_hash {
            out.push(Violation::new("dispatch.request_hash", "/request_hash"));
        }
        if self.output.max_output_tokens == 0 {
            out.push(Violation::new("dispatch.output", "/output"));
        }
        let value = serde_json::to_value(self).unwrap_or_default();
        if any_string(&value, &contains_secret_material)
            || any_string(&value, &|s| {
                s.to_ascii_lowercase().contains("authorization")
            })
        {
            out.push(Violation::new("dispatch.credential_material", "/"));
        }
        if any_string(&value, &contains_local_path) {
            out.push(Violation::new("dispatch.local_path", "/"));
        }
        let sent = matches!(
            self.state,
            DispatchState::Sent | DispatchState::Completed | DispatchState::OutcomeUnknown
        );
        if sent != self.sent_at.is_some() && self.state != DispatchState::Failed {
            out.push(Violation::new("dispatch.sent_at", "/sent_at"));
        }
        if (self.state == DispatchState::Completed) != self.completed_at.is_some() {
            out.push(Violation::new("dispatch.completed_at", "/completed_at"));
        }
        if self.destination.kind != DestinationKind::LocalMock
            && self.destination.provider_binding.is_none()
        {
            out.push(Violation::new(
                "dispatch.destination_binding",
                "/destination",
            ));
        }
        if self.destination.kind == DestinationKind::ExternalProvider
            && self.egress.egress_policy_id.is_none()
        {
            out.push(Violation::new("dispatch.egress_policy", "/egress"));
        }
        if self.messages.is_empty() {
            out.push(Violation::new("dispatch.messages", "/messages"));
        }
        out
    }
}

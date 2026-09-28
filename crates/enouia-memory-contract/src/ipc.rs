//! Local Memory IPC v1 (INTERFACES §1–§2). Same conventions as the M0 Activity
//! IPC: `schemaVersion: 1`, a `kind` discriminant, camelCase DTO fields. Stored
//! records embedded in results keep their snake_case storage form.
//!
//! Interfaces accept logical IDs only: no paths, SQL, shell text, or
//! caller-asserted identity. Import/backup/restore/delete operations are
//! reserved names whose arguments are frozen in MV-1/MV-2/MV-3.

use crate::candidate::{CandidateStatus, ProposalKind, ProposedType, ReviewAction};
use crate::common::Sensitivity;
use crate::context::{ContextCapsule, Currency, DestinationKind, Purpose};
use crate::error::{ContractError, MemoryError, MemoryErrorCode, Violation};
use crate::hash::Sha256Hex;
use crate::ids::{
    BranchId, CandidateId, CheckpointId, CommitId, EventId, IdentityId, InspectionId, MemoryId,
    OperationId, ProjectId, RequestId, ReviewId, SessionId, SourceId,
};
use crate::json::{Revision, SchemaVersion};
use crate::memory::{CanonicalMemory, MemoryType, StateKind};
use crate::policy::Scope;
use crate::record::RecordRef;
use crate::source::Availability;
use crate::time::{BusinessTime, Timestamp};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const SEARCH_DEFAULT_LIMIT: u32 = 20;
pub const SEARCH_MAX_LIMIT: u32 = 100;
pub const SOURCE_EXCERPT_MAX_BYTES: u64 = 8 * 1024;
pub const QUERY_MAX_CHARS: usize = 2000;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    ContextGet,
    MemorySearch,
    MemoryRead,
    MemorySource,
    MemoryPropose,
    /// Legacy name from Architecture v0.3 §16: an alias of `memory_propose`
    /// restricted to revise/supersede. It always returns a pending candidate.
    MemoryUpdate,
    SessionCheckpoint,
    CandidateList,
    CandidateReview,
    IdentityReview,
    OperationGet,
    OperationCancel,
}

/// Names reserved for later milestones; not accepted by this contract version.
pub const RESERVED_OPERATIONS: &[&str] = &[
    "import_preview",
    "import_start",
    "backup",
    "restore_preview",
    "delete_preview",
    "delete_commit",
    "attachment_get",
];

impl Operation {
    pub const ALL: [Self; 12] = [
        Self::ContextGet,
        Self::MemorySearch,
        Self::MemoryRead,
        Self::MemorySource,
        Self::MemoryPropose,
        Self::MemoryUpdate,
        Self::SessionCheckpoint,
        Self::CandidateList,
        Self::CandidateReview,
        Self::IdentityReview,
        Self::OperationGet,
        Self::OperationCancel,
    ];

    pub const fn canonical(self) -> Self {
        match self {
            Self::MemoryUpdate => Self::MemoryPropose,
            other => other,
        }
    }

    pub const fn required_scope(self) -> Scope {
        match self.canonical() {
            Self::ContextGet => Scope::ContextRead,
            Self::MemorySearch | Self::MemoryRead => Scope::MemoryRead,
            Self::MemorySource => Scope::SourceRead,
            Self::MemoryPropose | Self::MemoryUpdate => Scope::MemoryPropose,
            Self::SessionCheckpoint => Scope::SessionPropose,
            Self::CandidateList | Self::CandidateReview => Scope::OwnerReview,
            Self::IdentityReview => Scope::OwnerIdentity,
            Self::OperationGet | Self::OperationCancel => Scope::OperationRead,
        }
    }

    /// Writes a Vault commit (candidate, checkpoint artifact, or review result).
    pub const fn is_write(self) -> bool {
        matches!(
            self,
            Self::MemoryPropose
                | Self::MemoryUpdate
                | Self::SessionCheckpoint
                | Self::CandidateReview
                | Self::IdentityReview
        )
    }

    /// The early MCP tool set (INTERFACES §2). Review operations are never
    /// exposed to agents regardless of the scopes a token claims.
    pub const fn agent_exposable(self) -> bool {
        matches!(
            self,
            Self::ContextGet
                | Self::MemorySearch
                | Self::MemoryRead
                | Self::MemorySource
                | Self::MemoryPropose
                | Self::MemoryUpdate
                | Self::SessionCheckpoint
        )
    }

    /// The only success kind each operation may return.
    pub const fn success_kind(self) -> ResponseKind {
        match self {
            Self::ContextGet => ResponseKind::ContextCapsule,
            Self::MemorySearch => ResponseKind::MemorySearchResult,
            Self::MemoryRead => ResponseKind::MemoryRecord,
            Self::MemorySource => ResponseKind::SourceExcerpt,
            Self::MemoryPropose | Self::MemoryUpdate => ResponseKind::CandidateProposed,
            Self::SessionCheckpoint => ResponseKind::CheckpointProposed,
            Self::CandidateList => ResponseKind::CandidatePage,
            Self::CandidateReview | Self::IdentityReview => ResponseKind::ReviewCommitted,
            Self::OperationGet | Self::OperationCancel => ResponseKind::OperationStatus,
        }
    }
}

/// Idempotency keys are scoped to principal + canonical operation; the writer
/// stores the key's hash and the request payload hash in the receipt.
pub fn is_idempotency_key(value: &str) -> bool {
    (16..=128).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IpcRequest {
    pub schema_version: SchemaVersion,
    pub request_id: RequestId,
    pub operation: Operation,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub idempotency_key: Option<String>,
    pub arguments: Value,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct TimeMode {
    #[serde(deserialize_with = "crate::json::nullable")]
    pub as_of: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub known_at_commit_id: Option<CommitId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DestinationRequest {
    pub kind: DestinationKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub provider_binding_name: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextGetArgs {
    pub query: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub session_id: Option<SessionId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub branch_id: Option<BranchId>,
    pub project_hints: Vec<ProjectId>,
    pub time_mode: TimeMode,
    /// A request only; the server re-derives and verifies the destination.
    pub destination: DestinationRequest,
    pub purpose: Purpose,
    pub max_tokens: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySearchArgs {
    /// Literal text. Never spliced into SQL/FTS syntax.
    pub query: String,
    pub types: Vec<MemoryType>,
    pub project_ids: Vec<ProjectId>,
    pub include_historical: bool,
    pub time_mode: TimeMode,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub cursor: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryReadArgs {
    pub memory_id: MemoryId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub revision: Option<Revision>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ByteRangeArg {
    pub start: u64,
    pub end: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySourceArgs {
    pub source_id: SourceId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub source_revision: Option<Revision>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub range: Option<ByteRangeArg>,
    pub max_bytes: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedEvidence {
    pub source_id: SourceId,
    pub source_revision: Revision,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub range: Option<ByteRangeArg>,
    pub supports: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryProposeArgs {
    pub proposal_kind: ProposalKind,
    pub proposed_type: ProposedType,
    pub proposed_content: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub proposed_details: Option<serde_json::Map<String, Value>>,
    pub evidence: Vec<ProposedEvidence>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub target_memory_id: Option<MemoryId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub expected_revision: Option<Revision>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub proposed_effective_from: Option<BusinessTime>,
    pub reason: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProposedItem {
    pub claim: String,
    pub state_kind: StateKind,
    pub event_ids: Vec<EventId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SessionCheckpointArgs {
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub from_sequence: u64,
    pub to_sequence: u64,
    pub summary: String,
    pub decisions: Vec<ProposedItem>,
    pub open_loops: Vec<ProposedItem>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateListArgs {
    pub status: CandidateStatus,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub project_id: Option<ProjectId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub cursor: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub limit: Option<u32>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ExpectedRevisionArg {
    pub memory_id: MemoryId,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MergeTargetKind {
    Candidate,
    Memory,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MergeTargetArg {
    pub kind: MergeTargetKind,
    pub id: String,
}

/// Only honored from a trusted owner surface; the transport supplies the actor.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateReviewArgs {
    pub candidate_id: CandidateId,
    pub candidate_revision: Revision,
    pub action: ReviewAction,
    pub final_content_hash: Sha256Hex,
    pub approved_diff_hash: Sha256Hex,
    pub target_expected_revisions: Vec<ExpectedRevisionArg>,
    pub approval_nonce: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub edited_content: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub effective_from: Option<BusinessTime>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub merge_target: Option<MergeTargetArg>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IdentityReviewArgs {
    pub identity_id: IdentityId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub expected_revision: Option<Revision>,
    pub content_hash: Sha256Hex,
    pub approved_diff_hash: Sha256Hex,
    pub approval_nonce: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationArgs {
    pub operation_id: OperationId,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Arguments {
    ContextGet(ContextGetArgs),
    MemorySearch(MemorySearchArgs),
    MemoryRead(MemoryReadArgs),
    MemorySource(MemorySourceArgs),
    MemoryPropose(MemoryProposeArgs),
    SessionCheckpoint(SessionCheckpointArgs),
    CandidateList(CandidateListArgs),
    CandidateReview(CandidateReviewArgs),
    IdentityReview(IdentityReviewArgs),
    Operation(OperationArgs),
}

fn typed<T: DeserializeOwned>(value: &Value) -> Result<T, ContractError> {
    serde_json::from_value(value.clone()).map_err(|e| ContractError::Shape(e.to_string()))
}

fn check_limit(limit: Option<u32>, out: &mut Vec<Violation>) {
    if limit.is_some_and(|l| l == 0 || l > SEARCH_MAX_LIMIT) {
        out.push(Violation::new("ipc.limit", "/arguments/limit"));
    }
}

fn check_query(query: &str, out: &mut Vec<Violation>) {
    let chars = query.chars().count();
    if chars == 0 || chars > QUERY_MAX_CHARS {
        out.push(Violation::new("ipc.query_length", "/arguments/query"));
    }
}

/// Parse and validate a request envelope plus its operation's arguments.
pub fn parse_request(value: &Value) -> Result<(IpcRequest, Arguments), ContractError> {
    if value.get("schemaVersion").and_then(Value::as_i64) != Some(1) {
        return Err(ContractError::UnsupportedSchema {
            found: value.get("schemaVersion").and_then(Value::as_i64),
        });
    }
    let request: IpcRequest = typed(value)?;
    let mut out = Vec::new();
    match (&request.idempotency_key, request.operation.is_write()) {
        (Some(key), true) if is_idempotency_key(key) => {}
        (None, false) => {}
        _ => out.push(Violation::new("ipc.idempotency_key", "/idempotencyKey")),
    }
    let a = &request.arguments;
    let arguments = match request.operation {
        Operation::ContextGet => {
            let args: ContextGetArgs = typed(a)?;
            check_query(&args.query, &mut out);
            if args.max_tokens == 0 {
                out.push(Violation::new("ipc.budget", "/arguments/maxTokens"));
            }
            Arguments::ContextGet(args)
        }
        Operation::MemorySearch => {
            let args: MemorySearchArgs = typed(a)?;
            check_query(&args.query, &mut out);
            check_limit(args.limit, &mut out);
            Arguments::MemorySearch(args)
        }
        Operation::MemoryRead => Arguments::MemoryRead(typed(a)?),
        Operation::MemorySource => {
            let args: MemorySourceArgs = typed(a)?;
            if args.max_bytes == 0 || args.max_bytes > SOURCE_EXCERPT_MAX_BYTES {
                out.push(Violation::new(
                    "ipc.source_max_bytes",
                    "/arguments/maxBytes",
                ));
            }
            if let Some(range) = &args.range
                && (range.start >= range.end || range.end - range.start > SOURCE_EXCERPT_MAX_BYTES)
            {
                out.push(Violation::new("ipc.source_range", "/arguments/range"));
            }
            Arguments::MemorySource(args)
        }
        Operation::MemoryPropose | Operation::MemoryUpdate => {
            let args: MemoryProposeArgs = typed(a)?;
            let alias_ok = request.operation != Operation::MemoryUpdate
                || matches!(
                    args.proposal_kind,
                    ProposalKind::Revise | ProposalKind::Supersede
                );
            if args.proposal_kind == ProposalKind::Delete {
                out.push(Violation::new(
                    "ipc.delete_not_proposable",
                    "/arguments/proposalKind",
                ));
            }
            if !alias_ok {
                out.push(Violation::new(
                    "ipc.update_alias",
                    "/arguments/proposalKind",
                ));
            }
            if args.evidence.is_empty() {
                out.push(Violation::new(
                    "ipc.evidence_required",
                    "/arguments/evidence",
                ));
            }
            let targets = matches!(
                args.proposal_kind,
                ProposalKind::Revise | ProposalKind::Supersede | ProposalKind::Archive
            );
            if targets != (args.target_memory_id.is_some() && args.expected_revision.is_some()) {
                out.push(Violation::new(
                    "ipc.target_revision",
                    "/arguments/expectedRevision",
                ));
            }
            Arguments::MemoryPropose(args)
        }
        Operation::SessionCheckpoint => {
            let args: SessionCheckpointArgs = typed(a)?;
            if args.from_sequence == 0 || args.from_sequence > args.to_sequence {
                out.push(Violation::new(
                    "ipc.checkpoint_range",
                    "/arguments/fromSequence",
                ));
            }
            if args
                .decisions
                .iter()
                .chain(&args.open_loops)
                .any(|item| item.event_ids.is_empty())
            {
                out.push(Violation::new("ipc.checkpoint_sources", "/arguments"));
            }
            Arguments::SessionCheckpoint(args)
        }
        Operation::CandidateList => {
            let args: CandidateListArgs = typed(a)?;
            check_limit(args.limit, &mut out);
            Arguments::CandidateList(args)
        }
        Operation::CandidateReview => {
            let args: CandidateReviewArgs = typed(a)?;
            if !crate::candidate::is_nonce(&args.approval_nonce) {
                out.push(Violation::new(
                    "review.nonce_format",
                    "/arguments/approvalNonce",
                ));
            }
            if (args.action == ReviewAction::EditAccept) != args.edited_content.is_some() {
                out.push(Violation::new(
                    "ipc.edited_content",
                    "/arguments/editedContent",
                ));
            }
            if (args.action == ReviewAction::Supersede) != args.effective_from.is_some() {
                out.push(Violation::new(
                    "review.effective_from",
                    "/arguments/effectiveFrom",
                ));
            }
            if (args.action == ReviewAction::Merge) != args.merge_target.is_some() {
                out.push(Violation::new(
                    "review.merge_target",
                    "/arguments/mergeTarget",
                ));
            }
            if let Some(target) = &args.merge_target {
                let prefix = match target.kind {
                    MergeTargetKind::Candidate => "cand",
                    MergeTargetKind::Memory => "mem",
                };
                if crate::ids::check_prefixed_uuid(&target.id, prefix).is_err() {
                    out.push(Violation::new(
                        "ref.kind_prefix",
                        "/arguments/mergeTarget/id",
                    ));
                }
            }
            Arguments::CandidateReview(args)
        }
        Operation::IdentityReview => {
            let args: IdentityReviewArgs = typed(a)?;
            if !crate::candidate::is_nonce(&args.approval_nonce) {
                out.push(Violation::new(
                    "review.nonce_format",
                    "/arguments/approvalNonce",
                ));
            }
            Arguments::IdentityReview(args)
        }
        Operation::OperationGet | Operation::OperationCancel => Arguments::Operation(typed(a)?),
    };
    if out.is_empty() {
        Ok((request, arguments))
    } else {
        Err(ContractError::Invalid(out))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResponseKind {
    ContextCapsule,
    MemorySearchResult,
    MemoryRecord,
    SourceExcerpt,
    CandidateProposed,
    CheckpointProposed,
    CandidatePage,
    ReviewCommitted,
    OperationStatus,
    MemoryError,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IpcResponse {
    pub schema_version: SchemaVersion,
    pub request_id: RequestId,
    pub kind: ResponseKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub vault_commit_id: Option<CommitId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub policy_epoch: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub operation_id: Option<OperationId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub result: Option<Value>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub error: Option<MemoryError>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContextResult {
    pub capsule: ContextCapsule,
    pub inspection_id: InspectionId,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SearchHit {
    pub memory_id: MemoryId,
    pub revision: Revision,
    #[serde(rename = "type")]
    pub memory_type: MemoryType,
    pub snippet: String,
    pub currency: Currency,
    pub evidence_count: u32,
    pub updated_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemorySearchResult {
    pub items: Vec<SearchHit>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub next_cursor: Option<String>,
    pub stale: bool,
    pub partial: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryRecordResult {
    pub record: CanonicalMemory,
    pub superseded_by: Vec<MemoryId>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AttachmentAvailability {
    pub attachment_id: crate::ids::AttachmentId,
    pub availability: Availability,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SourceExcerpt {
    pub source_id: SourceId,
    pub source_revision: Revision,
    pub excerpt: String,
    pub byte_start: u64,
    pub byte_end: u64,
    pub truncated: bool,
    pub attachments: Vec<AttachmentAvailability>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalState {
    Pending,
    Duplicate,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateProposed {
    pub candidate_id: CandidateId,
    pub revision: Revision,
    pub state: ProposalState,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedCheckpointStatus {
    Provisional,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CheckpointProposed {
    pub checkpoint_id: CheckpointId,
    pub revision: Revision,
    pub status: ProposedCheckpointStatus,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidateSummary {
    pub candidate_id: CandidateId,
    pub revision: Revision,
    pub proposal_kind: ProposalKind,
    pub proposed_type: ProposedType,
    pub status: CandidateStatus,
    pub sensitivity: Sensitivity,
    pub created_at: Timestamp,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct CandidatePage {
    pub items: Vec<CandidateSummary>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub next_cursor: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ReviewCommitted {
    pub review_id: ReviewId,
    pub commit_id: CommitId,
    pub records: Vec<RecordRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Queued,
    Running,
    Succeeded,
    Failed,
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Progress {
    pub done: u64,
    pub total: u64,
}

/// Progress is never evidence of completion; only `succeeded` with a commit is.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OperationStatus {
    pub operation_id: OperationId,
    pub state: OperationState,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub progress: Option<Progress>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub error_code: Option<MemoryErrorCode>,
}

/// Validate a response envelope and its typed result against the operation.
pub fn validate_response(
    operation: Operation,
    value: &Value,
) -> Result<IpcResponse, ContractError> {
    let response: IpcResponse = typed(value)?;
    let mut out = Vec::new();
    let is_error = response.kind == ResponseKind::MemoryError;
    if is_error != response.error.is_some() || is_error == response.result.is_some() {
        out.push(Violation::new("ipc.result_or_error", "/"));
    }
    if !is_error && response.kind != operation.success_kind() {
        out.push(Violation::new("ipc.kind_for_operation", "/kind"));
    }
    if !out.is_empty() {
        return Err(ContractError::Invalid(out));
    }
    if let Some(result) = &response.result {
        let parsed = match response.kind {
            ResponseKind::ContextCapsule => typed::<ContextResult>(result).map(|r| {
                out.extend(r.capsule.validate());
            }),
            ResponseKind::MemorySearchResult => typed::<MemorySearchResult>(result).map(|r| {
                if r.items.len() > SEARCH_MAX_LIMIT as usize {
                    out.push(Violation::new("ipc.limit", "/result/items"));
                }
            }),
            ResponseKind::MemoryRecord => typed::<MemoryRecordResult>(result).map(|r| {
                out.extend(r.record.validate());
            }),
            ResponseKind::SourceExcerpt => typed::<SourceExcerpt>(result).map(|r| {
                if r.excerpt.len() as u64 > SOURCE_EXCERPT_MAX_BYTES || r.byte_start >= r.byte_end {
                    out.push(Violation::new("ipc.source_max_bytes", "/result/excerpt"));
                }
            }),
            ResponseKind::CandidateProposed => typed::<CandidateProposed>(result).map(|_| ()),
            ResponseKind::CheckpointProposed => typed::<CheckpointProposed>(result).map(|_| ()),
            ResponseKind::CandidatePage => typed::<CandidatePage>(result).map(|_| ()),
            ResponseKind::ReviewCommitted => typed::<ReviewCommitted>(result).map(|r| {
                if response.vault_commit_id.as_ref() != Some(&r.commit_id) {
                    out.push(Violation::new("ipc.commit_header", "/vaultCommitId"));
                }
            }),
            ResponseKind::OperationStatus => typed::<OperationStatus>(result).map(|_| ()),
            ResponseKind::MemoryError => Ok(()),
        };
        parsed?;
    }
    if !is_error && operation.is_write() && response.operation_id.is_none() {
        out.push(Violation::new("ipc.write_operation_id", "/operationId"));
    }
    if !is_error
        && response.vault_commit_id.is_none()
        && response.kind != ResponseKind::OperationStatus
    {
        out.push(Violation::new("ipc.snapshot_header", "/vaultCommitId"));
    }
    if out.is_empty() {
        Ok(response)
    } else {
        Err(ContractError::Invalid(out))
    }
}

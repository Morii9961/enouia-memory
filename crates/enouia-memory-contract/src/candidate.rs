//! CandidateRecord and ReviewRecord (DATA_MODEL §6). Models and agents can only
//! create candidates; a canonical write needs an owner review on a trusted surface.

use crate::common::{
    ActorRef, ActorType, EvidenceRef, Sensitivity, SourceRevisionRef, TrustedSurface,
};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{
    CandidateId, CommitId, ConflictGroupId, ExtractionRunId, IdentityId, MemoryId, ReviewId,
    SourceId,
};
use crate::json::{Extensions, Revision, SchemaVersion, validate_extensions};
use crate::record::RecordRef;
use crate::scan::contains_secret_material;
use crate::time::{BusinessTime, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposalKind {
    Create,
    Revise,
    Supersede,
    Archive,
    IdentityChange,
    /// Owner-only request to forget/purge a record; never exposed to agents.
    Delete,
}

/// Memory type or `identity` for Identity proposals.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedType {
    Fact,
    Preference,
    Episode,
    ProjectState,
    SessionCheckpoint,
    Identity,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CandidateStatus {
    Pending,
    Accepted,
    Rejected,
    Merged,
    Withdrawn,
}

impl CandidateStatus {
    pub const fn is_terminal(self) -> bool {
        !matches!(self, Self::Pending)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginKind {
    OwnerManual,
    RuleExtraction,
    ModelExtraction,
    AgentProposal,
    ExternalEvent,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateConflict {
    pub memory_id: MemoryId,
    pub revision: Revision,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub conflict_group_id: Option<ConflictGroupId>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum MergeTarget {
    Candidate { candidate_id: CandidateId },
    Memory { memory_id: MemoryId },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CandidateRecord {
    pub schema_version: SchemaVersion,
    pub candidate_id: CandidateId,
    pub revision: Revision,
    pub proposal_kind: ProposalKind,
    pub proposed_type: ProposedType,
    pub proposed_content: String,
    /// Type-specific proposed fields, validated against the memory contract at review.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub proposed_details: Option<serde_json::Map<String, Value>>,
    pub evidence: Vec<EvidenceRef>,
    pub source_id: SourceId,
    pub reason: String,
    pub origin_kind: OriginKind,
    pub origin_actor: ActorRef,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub extraction_run_id: Option<ExtractionRunId>,
    /// Advisory only. It never approves, ranks for approval, or upgrades evidence.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub confidence: Option<f64>,
    pub sensitivity: Sensitivity,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub declassification_review_id: Option<ReviewId>,
    pub status: CandidateStatus,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub target_memory_id: Option<MemoryId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub target_identity_id: Option<IdentityId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub expected_revision: Option<Revision>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub proposed_effective_from: Option<BusinessTime>,
    pub conflicts: Vec<CandidateConflict>,
    pub dedupe_fingerprint: Sha256Hex,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub reopens_candidate_id: Option<CandidateId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub merged_into: Option<MergeTarget>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub resolution_review_id: Option<ReviewId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub extensions: Extensions,
}

impl CandidateRecord {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let kind = self.proposal_kind;
        let targets_memory = matches!(
            kind,
            ProposalKind::Revise
                | ProposalKind::Supersede
                | ProposalKind::Archive
                | ProposalKind::Delete
        );
        if kind == ProposalKind::Delete && self.origin_kind != OriginKind::OwnerManual {
            out.push(Violation::new(
                "candidate.delete_owner_only",
                "/origin_kind",
            ));
        }
        if targets_memory != self.target_memory_id.is_some()
            || (targets_memory || self.target_identity_id.is_some())
                != self.expected_revision.is_some()
        {
            out.push(Violation::new(
                "candidate.target_fields",
                "/target_memory_id",
            ));
        }
        if (kind == ProposalKind::IdentityChange) != (self.proposed_type == ProposedType::Identity)
            || (kind != ProposalKind::IdentityChange && self.target_identity_id.is_some())
        {
            out.push(Violation::new("candidate.proposed_type", "/proposed_type"));
        }
        if (kind == ProposalKind::Supersede) != self.proposed_effective_from.is_some() {
            out.push(Violation::new(
                "candidate.effective_from",
                "/proposed_effective_from",
            ));
        }
        let terminal_review = matches!(
            self.status,
            CandidateStatus::Accepted | CandidateStatus::Rejected | CandidateStatus::Merged
        );
        if terminal_review != self.resolution_review_id.is_some() {
            out.push(Violation::new(
                "candidate.resolution_review",
                "/resolution_review_id",
            ));
        }
        if (self.status == CandidateStatus::Merged) != self.merged_into.is_some() {
            out.push(Violation::new("candidate.merged_into", "/merged_into"));
        }
        let extracted = matches!(
            self.origin_kind,
            OriginKind::RuleExtraction | OriginKind::ModelExtraction
        );
        if extracted != self.extraction_run_id.is_some() {
            out.push(Violation::new(
                "candidate.extraction_run",
                "/extraction_run_id",
            ));
        }
        let actor = self.origin_actor.actor_type;
        let actor_ok = match self.origin_kind {
            OriginKind::OwnerManual => actor == ActorType::Owner,
            OriginKind::RuleExtraction => actor == ActorType::System,
            OriginKind::ModelExtraction => {
                matches!(actor, ActorType::Provider | ActorType::System)
            }
            OriginKind::AgentProposal => matches!(actor, ActorType::Agent | ActorType::Client),
            OriginKind::ExternalEvent => matches!(actor, ActorType::Device | ActorType::System),
        };
        if !actor_ok {
            out.push(Violation::new("candidate.origin_actor", "/origin_actor"));
        }
        if self.evidence.is_empty() {
            out.push(Violation::new("candidate.evidence_required", "/evidence"));
        }
        if !self.evidence.iter().any(|e| e.source_id == self.source_id) {
            out.push(Violation::new("candidate.primary_source", "/source_id"));
        }
        if self.created_at > self.updated_at {
            out.push(Violation::new("candidate.time_order", "/updated_at"));
        }
        if let Some(confidence) = self.confidence
            && !(0.0..=1.0).contains(&confidence)
        {
            out.push(Violation::new("candidate.confidence_range", "/confidence"));
        }
        if contains_secret_material(&self.proposed_content) {
            out.push(Violation::new(
                "candidate.secret_material",
                "/proposed_content",
            ));
        }
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewAction {
    Accept,
    EditAccept,
    Reject,
    Merge,
    Supersede,
    IdentityAccept,
    /// Owner confirmation of a delete proposal; results in a Tombstone.
    ConfirmDelete,
}

/// Owner decision on one exact candidate revision. `approval_nonce` is issued by
/// the trusted client for one diff, short-lived and single-use.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewRecord {
    pub schema_version: SchemaVersion,
    pub review_id: ReviewId,
    pub actor: ActorRef,
    pub trusted_surface: TrustedSurface,
    pub action: ReviewAction,
    pub candidate_id: CandidateId,
    pub candidate_revision: Revision,
    pub final_content_hash: Sha256Hex,
    pub approved_diff_hash: Sha256Hex,
    pub evidence_refs: Vec<SourceRevisionRef>,
    pub target_expected_revisions: Vec<RecordRef>,
    pub approval_nonce: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub effective_from: Option<BusinessTime>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub merge_target: Option<MergeTarget>,
    pub resulting_records: Vec<RecordRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub reason_code: Option<String>,
    pub created_at: Timestamp,
    pub commit_id: CommitId,
}

pub fn is_nonce(value: &str) -> bool {
    (22..=128).contains(&value.len())
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

impl ReviewRecord {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.actor.actor_type != ActorType::Owner {
            out.push(Violation::new("review.owner_required", "/actor"));
        }
        if !is_nonce(&self.approval_nonce) {
            out.push(Violation::new("review.nonce_format", "/approval_nonce"));
        }
        let writes = matches!(
            self.action,
            ReviewAction::Accept
                | ReviewAction::EditAccept
                | ReviewAction::Supersede
                | ReviewAction::IdentityAccept
                | ReviewAction::ConfirmDelete
        );
        if self.action == ReviewAction::ConfirmDelete
            && self
                .resulting_records
                .iter()
                .any(|r| r.record_kind != crate::record::RecordKind::Tombstone)
        {
            out.push(Violation::new(
                "review.delete_results",
                "/resulting_records",
            ));
        }
        if writes && self.resulting_records.is_empty()
            || self.action == ReviewAction::Reject && !self.resulting_records.is_empty()
        {
            out.push(Violation::new(
                "review.action_results",
                "/resulting_records",
            ));
        }
        if (self.action == ReviewAction::Merge) != self.merge_target.is_some() {
            out.push(Violation::new("review.merge_target", "/merge_target"));
        }
        if (self.action == ReviewAction::Supersede) != self.effective_from.is_some() {
            out.push(Violation::new("review.effective_from", "/effective_from"));
        }
        if let Some(code) = &self.reason_code
            && !crate::common::is_code(code)
        {
            out.push(Violation::new("review.reason_code", "/reason_code"));
        }
        for (index, reference) in self
            .resulting_records
            .iter()
            .chain(&self.target_expected_revisions)
            .enumerate()
        {
            reference.validate(&format!("/records/{index}"), &mut out);
        }
        out
    }
}

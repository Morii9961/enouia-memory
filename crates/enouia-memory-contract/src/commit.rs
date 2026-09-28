//! CommitManifest, deletion records, and audit events (DATA_MODEL §8).
//!
//! A commit is defined by immutable revisions + this complete manifest + one
//! `CURRENT` pointer naming it. Several independent file replacements are not a
//! cross-file transaction. These types describe the manifest; publication and
//! durability are MV-1 store behavior and are not proven by parsing.

use crate::common::{ActorRef, ActorType};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{
    AuditId, CommitId, DeleteId, DeviceId, OperationId, PurgeReceiptId, RequestId, ReviewId,
    VaultId,
};
use crate::json::{Revision, SchemaVersion};
use crate::record::{RecordKind, RecordRef};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const VAULT_FORMAT_VERSION: u64 = 1;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct FormatVersion;

impl Serialize for FormatVersion {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(VAULT_FORMAT_VERSION)
    }
}

impl<'de> Deserialize<'de> for FormatVersion {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u64::deserialize(deserializer)? {
            VAULT_FORMAT_VERSION => Ok(Self),
            _ => Err(serde::de::Error::custom("unsupported vault format_version")),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationKind {
    Genesis,
    Import,
    CandidatePropose,
    ReviewCommit,
    IdentityReview,
    SessionAppend,
    CheckpointPropose,
    LogicalDelete,
    Purge,
    PolicyChange,
    Migration,
    RestoreAdopt,
    OwnerApproval,
}

/// One logical record → its current revision in this commit (complete catalog).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntry {
    pub record_kind: RecordKind,
    pub record_id: String,
    pub revision: Revision,
    pub content_hash: Sha256Hex,
    pub changed: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObjectKind {
    Raw,
    Asset,
    SessionContent,
    IdentityMarkdown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ObjectEntry {
    pub object_hash: Sha256Hex,
    pub size_bytes: u64,
    pub object_kind: ObjectKind,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReceiptResult {
    Committed,
}

/// The durable answer to a write. A retry with the same idempotency scope and
/// payload hash returns this receipt instead of applying the operation again.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OperationReceipt {
    pub operation_id: OperationId,
    pub result: ReceiptResult,
    pub records: Vec<RecordRef>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommitManifest {
    pub schema_version: SchemaVersion,
    pub commit_id: CommitId,
    pub format_version: FormatVersion,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent_commit_id: Option<CommitId>,
    pub sequence: u64,
    pub vault_id: VaultId,
    pub writer_device_id: DeviceId,
    pub principal: ActorRef,
    pub operation_id: OperationId,
    pub operation_kind: OperationKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub idempotency_key_hash: Option<Sha256Hex>,
    pub request_payload_hash: Sha256Hex,
    pub created_at: Timestamp,
    pub catalog: Vec<CatalogEntry>,
    pub objects: Vec<ObjectEntry>,
    pub review_ids: Vec<ReviewId>,
    pub tombstone_ids: Vec<DeleteId>,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub receipt: OperationReceipt,
}

impl CommitManifest {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let genesis = self.operation_kind == OperationKind::Genesis;
        if genesis != self.parent_commit_id.is_none()
            || genesis != (self.sequence == 1)
            || self.sequence == 0
        {
            out.push(Violation::new("commit.genesis", "/parent_commit_id"));
        }
        if genesis == self.idempotency_key_hash.is_some() {
            out.push(Violation::new(
                "commit.idempotency_key",
                "/idempotency_key_hash",
            ));
        }
        if genesis && self.principal.actor_type != ActorType::Owner {
            out.push(Violation::new("commit.genesis_owner", "/principal"));
        }
        let mut keys = BTreeSet::new();
        for (index, entry) in self.catalog.iter().enumerate() {
            let reference = RecordRef::new(entry.record_kind, &entry.record_id, entry.revision);
            reference.validate(&format!("/catalog/{index}"), &mut out);
            let single = matches!(
                entry.record_kind,
                RecordKind::Review
                    | RecordKind::Approval
                    | RecordKind::Tombstone
                    | RecordKind::PurgeReceipt
                    | RecordKind::SessionEvent
            );
            if !(entry.record_kind.is_revisioned() || single && entry.revision.get() == 1) {
                out.push(Violation::new(
                    "commit.catalog_kind",
                    format!("/catalog/{index}"),
                ));
            }
            if !keys.insert((entry.record_kind, entry.record_id.clone())) {
                out.push(Violation::new(
                    "commit.catalog_duplicate",
                    format!("/catalog/{index}"),
                ));
            }
        }
        if self.receipt.operation_id != self.operation_id {
            out.push(Violation::new("commit.receipt_operation", "/receipt"));
        }
        let changed: BTreeSet<RecordRef> = self
            .catalog
            .iter()
            .filter(|e| e.changed)
            .map(|e| RecordRef::new(e.record_kind, &e.record_id, e.revision))
            .collect();
        let receipt: BTreeSet<RecordRef> = self.receipt.records.iter().cloned().collect();
        if changed != receipt {
            out.push(Violation::new("commit.receipt_records", "/receipt/records"));
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteMode {
    LogicalDelete,
    Purge,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeleteScope {
    SelectedRevisions,
    AllRevisions,
    WithDependents,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DeleteTarget {
    pub record_kind: RecordKind,
    pub record_id: String,
    /// `null` targets every revision of the record.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub revision: Option<Revision>,
}

/// Deletion overrides every status and Raw's default immutability. It carries
/// no deleted text; history views show only this record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Tombstone {
    pub schema_version: SchemaVersion,
    pub delete_id: DeleteId,
    pub mode: DeleteMode,
    pub scope: DeleteScope,
    pub targets: Vec<DeleteTarget>,
    pub object_hashes: Vec<Sha256Hex>,
    pub requested_by: ActorRef,
    pub review_id: ReviewId,
    pub deletion_epoch: u64,
    pub created_at: Timestamp,
}

impl Tombstone {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.requested_by.actor_type != ActorType::Owner {
            out.push(Violation::new("tombstone.owner_required", "/requested_by"));
        }
        if self.deletion_epoch == 0 {
            out.push(Violation::new("tombstone.epoch", "/deletion_epoch"));
        }
        if self.targets.is_empty() && self.object_hashes.is_empty() {
            out.push(Violation::new("tombstone.targets", "/targets"));
        }
        for (index, target) in self.targets.iter().enumerate() {
            let ok = target.record_kind.id_prefix().is_some_and(|prefix| {
                crate::ids::check_prefixed_uuid(&target.record_id, prefix).is_ok()
            });
            if !ok {
                out.push(Violation::new(
                    "ref.kind_prefix",
                    format!("/targets/{index}"),
                ));
            }
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StoreKind {
    Canonical,
    Raw,
    Assets,
    Candidates,
    Sessions,
    Checkpoints,
    Capsules,
    Index,
    Embeddings,
    Staging,
    Exports,
    Backups,
    DeviceReplicas,
    Gateway,
}

impl StoreKind {
    pub const fn is_local(self) -> bool {
        !matches!(self, Self::Backups | Self::DeviceReplicas | Self::Gateway)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorePurgeState {
    Purged,
    NotPresent,
    Pending,
    Failed,
    NotManageable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StorePurge {
    pub store: StoreKind,
    pub state: StorePurgeState,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub confirmed_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub detail_code: Option<String>,
}

/// Overall purge state. There is deliberately no `globally_erased` value.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PurgeOverall {
    LiveDeleted,
    BackupPurgePending,
    ReplicaPending,
    LocallyPurged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PurgeReceipt {
    pub schema_version: SchemaVersion,
    pub receipt_id: PurgeReceiptId,
    pub delete_id: DeleteId,
    pub stores: Vec<StorePurge>,
    pub overall_state: PurgeOverall,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub latest_purge_deadline: Option<Timestamp>,
    pub created_at: Timestamp,
}

impl PurgeReceipt {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let stores: BTreeSet<_> = self.stores.iter().map(|s| s.store as u8).collect();
        if stores.len() != self.stores.len() || self.stores.is_empty() {
            out.push(Violation::new("purge.stores_unique", "/stores"));
        }
        for (index, store) in self.stores.iter().enumerate() {
            if (store.state == StorePurgeState::Purged) != store.confirmed_at.is_some() {
                out.push(Violation::new(
                    "purge.confirmation",
                    format!("/stores/{index}"),
                ));
            }
        }
        let unfinished = |local: bool| {
            self.stores.iter().any(|s| {
                s.store.is_local() == local
                    && matches!(s.state, StorePurgeState::Pending | StorePurgeState::Failed)
            })
        };
        let overclaim = match self.overall_state {
            PurgeOverall::LocallyPurged => unfinished(true) || unfinished(false),
            PurgeOverall::BackupPurgePending | PurgeOverall::ReplicaPending => unfinished(true),
            PurgeOverall::LiveDeleted => false,
        };
        if overclaim {
            out.push(Violation::new("purge.overall_state", "/overall_state"));
        }
        if self.overall_state == PurgeOverall::BackupPurgePending
            && self.latest_purge_deadline.is_none()
        {
            out.push(Violation::new("purge.deadline", "/latest_purge_deadline"));
        }
        out
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditDecision {
    Allow,
    Deny,
}

/// Who/what/why/where/result. No record text, query text, secrets, or paths:
/// every field is an ID, enum, or code, so the shape itself forbids free text.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuditEvent {
    pub schema_version: SchemaVersion,
    pub audit_id: AuditId,
    pub actor: ActorRef,
    pub operation: crate::ipc::Operation,
    pub object_refs: Vec<RecordRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub purpose: Option<crate::context::Purpose>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub destination: Option<crate::context::DestinationKind>,
    pub policy_epoch: u64,
    pub decision: AuditDecision,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub error_code: Option<crate::error::MemoryErrorCode>,
    pub request_id: RequestId,
    pub created_at: Timestamp,
}

impl AuditEvent {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.decision == AuditDecision::Deny && self.error_code.is_none() {
            out.push(Violation::new("audit.deny_code", "/error_code"));
        }
        for (index, reference) in self.object_refs.iter().enumerate() {
            reference.validate(&format!("/object_refs/{index}"), &mut out);
        }
        out
    }
}

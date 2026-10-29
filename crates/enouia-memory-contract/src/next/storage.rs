//! Explicit automation-root documents. None implements legacy Record/StoreDocument.
use super::*;
use crate::{
    common::{ActorRef, ActorType, SourceRevisionRef, SpeakerRole},
    error::Violation,
    hash::{Sha256Hex, sha256},
    host::{AuthorityState, Checkpoint, PrimaryBinding},
    ids::*,
    json::{Revision, canonical_bytes},
    source::{Completeness, SourceKind, SourceRecord},
    store::StoreDocument,
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_AUTOMATION_COMMIT_BYTES: usize = 1024 * 1024;
pub const MAX_AUTOMATION_ENTRIES: usize = 128;
pub const MAX_AUTOMATION_BATCH: usize = 8;
pub const MAX_AUTOMATION_CHAIN: usize = 64;
pub const MAX_AUTOMATION_CHAIN_BYTES: usize = 32 * 1024 * 1024;

macro_rules! kind {
    ($t:ident,$v:ident,$s:literal) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        pub enum $t {
            #[serde(rename=$s)]
            $v,
        }
    };
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AutomationLayout {
    #[serde(rename = "automation-host-v1")]
    HostV1,
    #[serde(rename = "automation-host-v2")]
    HostV2,
    #[serde(rename = "automation-host-v3")]
    HostV3,
    #[serde(rename = "automation-segmented-host-v1")]
    SegmentedV1,
    #[serde(rename = "automation-segmented-owner-v1")]
    SegmentedOwnerV1,
}
kind!(DescriptorKind, AutomationVault, "automation_vault");
kind!(CurrentKind, AutomationCurrent, "automation_current");
kind!(AutomationCommitKind, AutomationCommit, "automation_commit");
kind!(MachineKind, MachineMemory, "machine_memory");
kind!(
    PublicationKind,
    AutomationPublication,
    "automation_publication"
);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationDescriptor {
    pub schema_version: NextVersion,
    pub kind: DescriptorKind,
    pub layout: AutomationLayout,
    pub vault_id: VaultId,
    pub host_id: DeviceId,
    pub created_by: ActorRef,
    pub system_actor: ActorRef,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::record_fields::present"
    )]
    pub owner_subject_id: Option<SubjectId>,
}
impl NextDocument for AutomationDescriptor {
    fn validate(&self) -> Vec<Violation> {
        let mut errors = Vec::new();
        if (self.layout == AutomationLayout::SegmentedOwnerV1) != self.owner_subject_id.is_some() {
            errors.push(Violation::new("owner.subject_binding", "/owner_subject_id"));
        }
        if self.created_by.actor_type != ActorType::Owner
            || self.system_actor.actor_type != ActorType::System
            || self.created_by.actor_id == self.system_actor.actor_id
        {
            errors.push(Violation::new("automation.principals", ""));
        }
        errors
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationCurrent {
    pub schema_version: NextVersion,
    pub kind: CurrentKind,
    pub vault_id: VaultId,
    pub commit_id: CommitId,
    pub sequence: Revision,
    pub manifest_sha256: Sha256Hex,
}
impl NextDocument for AutomationCurrent {
    fn validate(&self) -> Vec<Violation> {
        Vec::new()
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapturedSource {
    pub record: SourceRecord,
    pub text: String,
    /// Existing v1 attachment metadata retained in the exact captured revision.
    /// Absent on older roots; never reconstructed from a filename on read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub attachments: Vec<crate::source::AttachmentRecord>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::record_fields::present"
    )]
    pub record_fields: Option<SourceRecordFields>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub attributed_subject: Option<SubjectId>,
}
impl CapturedSource {
    pub fn reference(&self) -> SourceRevisionRef {
        SourceRevisionRef {
            source_id: self.record.source_id.clone(),
            source_revision: self.record.revision,
        }
    }
    pub fn validate(&self) -> Vec<Violation> {
        let mut errors = self.record.validate();
        if (!self.attachments.is_empty() || self.record_fields.is_some())
            && (self.attachments.len() > 128
                || self.attachments.iter().any(|a| {
                    !a.validate().is_empty()
                        || a.source_id != self.record.source_id
                        || a.sensitivity < self.record.sensitivity
                })
                || self
                    .attachments
                    .iter()
                    .map(|a| &a.attachment_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    != self.attachments.len()
                || self
                    .attachments
                    .iter()
                    .map(|a| &a.attachment_id)
                    .collect::<BTreeSet<_>>()
                    != self.record.attachment_refs.iter().collect::<BTreeSet<_>>())
        {
            errors.push(Violation::new("automation.attachments", "/attachments"));
        }
        if self.record_fields.as_ref().is_some_and(|f| !f.valid(self)) {
            errors.push(Violation::new("ui.source_fields", "/record_fields"));
        }
        if !matches!(
            self.record.source_kind,
            SourceKind::RuntimeEvent
                | SourceKind::ExportMessage
                | SourceKind::ImportedDocument
                | SourceKind::AgentSubmission
        ) || !matches!(
            self.record.completeness,
            Completeness::Complete | Completeness::Partial
        ) || self.text.trim().is_empty()
            || self.text.len() > 16 * 1024
            || self.record.content_hash != sha256(self.text.as_bytes())
            || (self.record.speaker_role == SpeakerRole::User) != self.attributed_subject.is_some()
            || (self.record.speaker_role == SpeakerRole::User
                && !matches!(
                    self.record.evidence_class,
                    crate::common::EvidenceClass::UserStatement
                        | crate::common::EvidenceClass::UserConfirmation
                ))
        {
            errors.push(Violation::new("automation.capture", "/sources"));
        }
        errors
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MachineMemory {
    pub schema_version: NextVersion,
    pub kind: MachineKind,
    pub memory: MemoryRevisionRef,
    pub request: AdmissionInput,
    pub decision: DecisionDraft,
    pub response_hash: Sha256Hex,
    pub response_json: String,
    pub committed_in: CommitId,
}
impl MachineMemory {
    pub fn target(&self) -> TargetRef {
        TargetRef {
            memory: self.memory.clone(),
            hash: digest(self),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationBinding {
    pub operation_id: OperationId,
    pub input_hash: Sha256Hex,
    pub response_hash: Sha256Hex,
}

/// Stored with the data; it becomes a success receipt only after native witness.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReceipt {
    pub job_key: Sha256Hex,
    pub binding_hash: Sha256Hex,
    pub operation_id: OperationId,
    pub request_hash: Sha256Hex,
    pub response_hash: Sha256Hex,
    pub decision_hash: Sha256Hex,
    pub committed_in: CommitId,
    pub sequence: Revision,
    pub result: MemoryRevisionRef,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationData {
    pub policy: AutomationPolicy,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::record_fields::present"
    )]
    pub record_catalog: Option<RecordCatalog>,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::record_fields::present"
    )]
    pub owner_actions: Option<OwnerActionLedger>,
    pub policy_watermark: Revision,
    pub deletion_watermark: Revision,
    pub sources: Vec<CapturedSource>,
    pub memories: Vec<MachineMemory>,
    pub receipts: Vec<MemoryReceipt>,
    pub deleted_sources: Vec<SourceId>,
    pub deleted_memories: Vec<MemoryId>,
}
impl AutomationData {
    pub fn validate(&self) -> Vec<Violation> {
        let mut errors = self.policy.validate();
        if let Some(catalog) = &self.record_catalog {
            errors.extend(catalog.validate(&self.sources));
        }
        if let Some(ledger) = &self.owner_actions {
            errors.extend(ledger.validate());
        }
        if [
            self.sources.len(),
            self.memories.len(),
            self.receipts.len(),
            self.deleted_sources.len(),
            self.deleted_memories.len(),
        ]
        .iter()
        .any(|n| *n > MAX_AUTOMATION_ENTRIES)
            || self.deleted_sources.iter().collect::<BTreeSet<_>>().len()
                != self.deleted_sources.len()
            || self.deleted_memories.iter().collect::<BTreeSet<_>>().len()
                != self.deleted_memories.len()
        {
            errors.push(Violation::new("automation.capacity_or_duplicate", ""));
            return errors;
        }
        let mut sources = BTreeSet::new();
        for source in &self.sources {
            errors.extend(source.validate());
            if !sources.insert(source.reference()) {
                errors.push(Violation::new("automation.source_duplicate", "/sources"));
            }
        }
        let mut memories = BTreeSet::new();
        let mut operations = BTreeSet::new();
        for memory in &self.memories {
            errors.extend(memory.request.validate());
            if memory.decision.input.record_fields.is_some()
                && !super::record_fields::validate_record_evidence(&memory.decision.input, self)
            {
                errors.push(Violation::new("ui.evidence_closure", "/memories"));
            }
            errors.extend(memory.decision.validate());
            if !memories.insert((memory.memory.memory_id.clone(), memory.memory.revision))
                || !operations.insert(memory.decision.input.operation_id.clone())
                || memory.memory != memory.decision.input.result
                || memory.response_hash != sha256(memory.response_json.as_bytes())
                || parse_document::<DecisionDraft>(memory.response_json.as_bytes())
                    .ok()
                    .as_ref()
                    != Some(&memory.decision)
                || check_generation_binding(&memory.request, &memory.decision.input).is_err()
                || memory
                    .decision
                    .input
                    .input_refs
                    .iter()
                    .any(|reference| !sources.contains(reference))
            {
                errors.push(Violation::new("automation.memory_binding", "/memories"));
            }
        }
        if self.receipts.len() != self.memories.len()
            || self
                .receipts
                .iter()
                .zip(&self.memories)
                .any(|(receipt, memory)| {
                    receipt.result != memory.memory
                        || receipt.operation_id != memory.decision.input.operation_id
                        || receipt.committed_in != memory.committed_in
                        || receipt.request_hash != digest(&memory.request)
                        || receipt.response_hash != memory.response_hash
                        || receipt.decision_hash != digest(&memory.decision)
                })
        {
            errors.push(Violation::new("automation.receipt_binding", "/receipts"));
        }
        errors
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AutomationChange {
    Genesis,
    Capture,
    AdmitBatch,
    Policy,
    DeleteSource,
    DeleteMemory,
    /// Wire shape reserved now; no publication entry until G08.
    OwnerAction,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationCommit {
    pub schema_version: NextVersion,
    pub kind: AutomationCommitKind,
    pub commit_id: CommitId,
    pub sequence: Revision,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent: Option<AutomationCurrent>,
    pub actor: ActorRef,
    pub created_at: Timestamp,
    pub change: AutomationChange,
    pub data: AutomationData,
    pub authority: AuthorityState,
}
impl AutomationCommit {
    pub fn current(&self) -> AutomationCurrent {
        AutomationCurrent {
            schema_version: NextVersion,
            kind: CurrentKind::AutomationCurrent,
            vault_id: self.data.policy.vault_id.clone(),
            commit_id: self.commit_id.clone(),
            sequence: self.sequence,
            manifest_sha256: digest(self),
        }
    }
    pub fn snapshot(&self) -> SnapshotBinding {
        SnapshotBinding {
            vault_id: self.data.policy.vault_id.clone(),
            base_commit_id: self.commit_id.clone(),
            primary_epoch: self.authority.primary.primary_epoch,
            policy_watermark: self.data.policy_watermark,
            deletion_watermark: self.data.deletion_watermark,
        }
    }
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            commit_id: self.commit_id.clone(),
            commit_sequence: self.sequence,
            snapshot_hash: digest(&self.data),
            policy_epoch: self.data.policy_watermark.get(),
            deletion_epoch: self.data.deletion_watermark.get(),
        }
    }
    pub fn assemble(
        descriptor: &AutomationDescriptor,
        before: Option<&Self>,
        actor: ActorRef,
        change: AutomationChange,
        data: AutomationData,
        commit_id: CommitId,
        created_at: Timestamp,
    ) -> Result<Self, crate::ContractError> {
        let sequence = Revision::new(before.map_or(1, |old| old.sequence.get() + 1))
            .ok_or_else(|| super::invalid("automation.counter", ""))?;
        let checkpoint = Checkpoint {
            commit_id: commit_id.clone(),
            commit_sequence: sequence,
            snapshot_hash: digest(&data),
            policy_epoch: data.policy_watermark.get(),
            deletion_epoch: data.deletion_watermark.get(),
        };
        let authority = match before {
            Some(old) => old.authority.advance(
                &PrimaryBinding {
                    host_id: descriptor.host_id.clone(),
                    primary_epoch: old.authority.primary.primary_epoch,
                },
                &old.checkpoint(),
                checkpoint,
            ),
            None => AuthorityState::initial(
                descriptor.vault_id.clone(),
                descriptor.host_id.clone(),
                checkpoint,
            ),
        }
        .map_err(|_| super::invalid("automation.authority", ""))?;
        let result = Self {
            schema_version: NextVersion,
            kind: AutomationCommitKind::AutomationCommit,
            commit_id,
            sequence,
            parent: before.map(Self::current),
            actor,
            created_at,
            change,
            data,
            authority,
        };
        validate_delta(descriptor, before, &result)?;
        Ok(result)
    }
}
impl NextDocument for AutomationCommit {
    const MAX_BYTES: usize = MAX_AUTOMATION_COMMIT_BYTES;
    fn validate(&self) -> Vec<Violation> {
        let mut errors = self.data.validate();
        errors.extend(StoreDocument::validate(&self.authority));
        if self.authority.checkpoint != self.checkpoint()
            || self.authority.vault_id != self.data.policy.vault_id
            || canonical_bytes(self).map_or(true, |b| b.len() > Self::MAX_BYTES)
        {
            errors.push(Violation::new("automation.commit_closure", ""));
        }
        errors
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationPublication {
    pub schema_version: NextVersion,
    pub kind: PublicationKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub before: Option<AutomationCurrent>,
    pub after: AutomationCurrent,
}
impl NextDocument for AutomationPublication {
    fn validate(&self) -> Vec<Violation> {
        if self.before.as_ref().is_some_and(|b| {
            b.vault_id != self.after.vault_id
                || b.sequence.get() + 1 != self.after.sequence.get()
                || b.commit_id == self.after.commit_id
                || b.manifest_sha256 == self.after.manifest_sha256
        }) || (self.before.is_none() && self.after.sequence.get() != 1)
        {
            vec![Violation::new("automation.publication_transition", "")]
        } else {
            Vec::new()
        }
    }
}

//! ADR-MEM-94: immutable bounded deltas, preserving ADR-MEM-93 record fields.
use super::*;
use crate::{
    common::ActorRef,
    error::Violation,
    hash::Sha256Hex,
    host::{AuthorityState, Checkpoint},
    ids::*,
    json::{Revision, canonical_bytes},
    store::StoreDocument,
    time::Timestamp,
};
use serde::{Deserialize, Serialize};

pub const MAX_SEGMENT_BYTES: usize = 4 * 1024 * 1024;
pub const MAX_SEGMENTED_REVISIONS: usize = 65_536;
pub const MAX_SEGMENTED_COMMITS: usize = 65_536;
pub const MAX_SEGMENTED_CHAIN_BYTES: usize = 1024 * 1024 * 1024;
pub const MAX_SEGMENTED_POLICIES: usize = 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "change", rename_all = "snake_case", deny_unknown_fields)]
pub enum SegmentedDelta {
    Genesis {
        owner_actions: OwnerActionLedger,
    },
    Policy {
        policy: Box<AutomationPolicy>,
    },
    Capture {
        sources: Vec<CapturedSource>,
        catalog: RecordCatalog,
    },
    Admit {
        job_key: Sha256Hex,
        requests: Vec<AdmissionInput>,
        responses: Vec<String>,
        bindings: Vec<GenerationBinding>,
    },
    Delete {
        sources: Vec<SourceId>,
        memories: Vec<MemoryId>,
    },
    OwnerAction {
        record: Box<OwnerActionRecord>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SegmentedKind {
    #[serde(rename = "segmented_automation_commit")]
    Commit,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SegmentedCommit {
    pub schema_version: NextVersion,
    pub kind: SegmentedKind,
    pub vault_id: VaultId,
    pub commit_id: CommitId,
    pub sequence: Revision,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent: Option<AutomationCurrent>,
    pub actor: ActorRef,
    pub created_at: Timestamp,
    pub policy_watermark: Revision,
    pub deletion_watermark: Revision,
    pub delta: SegmentedDelta,
    pub authority: AuthorityState,
}
impl SegmentedCommit {
    pub fn checkpoint(&self) -> Checkpoint {
        Checkpoint {
            commit_id: self.commit_id.clone(),
            commit_sequence: self.sequence,
            snapshot_hash: digest(&(&self.parent, &self.delta)),
            policy_epoch: self.policy_watermark.get(),
            deletion_epoch: self.deletion_watermark.get(),
        }
    }
    pub fn current(&self) -> AutomationCurrent {
        AutomationCurrent {
            schema_version: NextVersion,
            kind: CurrentKind::AutomationCurrent,
            vault_id: self.vault_id.clone(),
            commit_id: self.commit_id.clone(),
            sequence: self.sequence,
            manifest_sha256: digest(self),
        }
    }
    pub fn snapshot(&self) -> SnapshotBinding {
        SnapshotBinding {
            vault_id: self.vault_id.clone(),
            base_commit_id: self.commit_id.clone(),
            primary_epoch: self.authority.primary.primary_epoch,
            policy_watermark: self.policy_watermark,
            deletion_watermark: self.deletion_watermark,
        }
    }
}
impl NextDocument for SegmentedCommit {
    const MAX_BYTES: usize = MAX_SEGMENT_BYTES;
    fn validate(&self) -> Vec<Violation> {
        let mut out = StoreDocument::validate(&self.authority);
        if self.authority.vault_id != self.vault_id
            || self.authority.checkpoint != self.checkpoint()
            || canonical_bytes(self).map_or(true, |b| b.len() > MAX_SEGMENT_BYTES)
            || self.parent.as_ref().is_some_and(|p| {
                p.vault_id != self.vault_id
                    || p.sequence.get() + 1 != self.sequence.get()
                    || p.commit_id == self.commit_id
            })
            || (self.parent.is_none() && self.sequence.get() != 1)
        {
            out.push(Violation::new("segmented.commit", ""));
        }
        let valid = match &self.delta {
            SegmentedDelta::Genesis { owner_actions } => {
                *owner_actions == OwnerActionLedger::default()
            }
            SegmentedDelta::Policy { policy } => {
                policy.validate().is_empty() && policy.rule_version == RuleVersion::V3
            }
            SegmentedDelta::Capture { sources, catalog } => {
                !sources.is_empty()
                    && sources.len() <= 64
                    && sources
                        .iter()
                        .all(|s| s.record_fields.is_some() && s.validate().is_empty())
                    && catalog.entities.len() <= 128
                    && catalog.conversations.len() <= 128
            }
            SegmentedDelta::Admit {
                job_key,
                requests,
                responses,
                bindings,
            } => {
                !requests.is_empty()
                    && requests.len() <= MAX_AUTOMATION_BATCH
                    && responses.len() == requests.len()
                    && bindings.len() == requests.len()
                    && *job_key == admission_batch_key(requests)
                    && requests
                        .iter()
                        .zip(responses)
                        .zip(bindings)
                        .all(|((r, s), b)| {
                            r.record_fields.is_some()
                                && r.validate().is_empty()
                                && b.operation_id == r.operation_id
                                && b.input_hash == digest(r)
                                && b.response_hash == crate::hash::sha256(s.as_bytes())
                                && parse_document::<DecisionDraft>(s.as_bytes())
                                    .is_ok_and(|d| check_generation_binding(r, &d.input).is_ok())
                        })
            }
            SegmentedDelta::Delete { sources, memories } => {
                !sources.is_empty() || !memories.is_empty()
            }
            SegmentedDelta::OwnerAction { .. } => false,
        };
        if !valid {
            out.push(Violation::new("segmented.delta", "/delta"));
        }
        out
    }
}

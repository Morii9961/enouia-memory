//! Saved Agent supersede reports: untrusted interpretation beside original
//! user evidence, never a memory revision or Owner action (ADR-MEM-124).
use super::*;
use crate::{
    common::{ActorRef, ActorType, EvidenceClass, SpeakerRole},
    hash::Sha256Hex,
    ids::*,
};
use serde::{Deserialize, Serialize};

pub const AGENT_SUPERSEDE_CAPABILITY: &str = "memory-agent-supersede-native-1";
pub const MAX_AGENT_SUPERSEDES_PER_TARGET: usize = 32;

pub fn agent_supersede_key(
    vault: &VaultId,
    principal: &ActorRef,
    request: &RequestId,
) -> Sha256Hex {
    digest(&(AGENT_SUPERSEDE_CAPABILITY, vault, principal, request))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSupersedeRecord {
    pub report_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub target: TargetRef,
    pub policy: PolicyRef,
    pub evidence: QuotedEvidence,
    pub supersede: String,
}
impl AgentSupersedeRecord {
    pub fn valid(&self, vault: &VaultId) -> bool {
        self.principal.actor_type == ActorType::Agent
            && self.report_key == agent_supersede_key(vault, &self.principal, &self.request_id)
            && self.evidence.valid_shape()
            && self.evidence.reference.supports == "content"
            && self.evidence.speaker == SpeakerRole::User
            && self.evidence.reference.evidence_class == EvidenceClass::UserStatement
            && !self.supersede.trim().is_empty()
            && self.supersede.len() <= 8192
            && self.supersede.chars().count() <= 2000
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentSupersedeState {
    #[serde(rename = "supersede_report_saved")]
    SupersedeReportSaved,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSupersedeReceipt {
    pub report_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub target: TargetRef,
    pub committed_in: CommitId,
    pub state: AgentSupersedeState,
}

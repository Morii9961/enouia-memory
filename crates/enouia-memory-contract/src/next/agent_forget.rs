//! Recorded Agent forget requests: untrusted interpretation beside original
//! user evidence, never a memory revision or Owner action (ADR-MEM-125).
use super::*;
use crate::{
    common::{ActorRef, ActorType, EvidenceClass, SpeakerRole},
    hash::Sha256Hex,
    ids::*,
};
use serde::{Deserialize, Serialize};

pub const AGENT_FORGET_CAPABILITY: &str = "memory-agent-forget-native-1";
pub const MAX_AGENT_FORGET_REQUESTS_PER_TARGET: usize = 32;

pub fn agent_forget_key(vault: &VaultId, principal: &ActorRef, request: &RequestId) -> Sha256Hex {
    digest(&(AGENT_FORGET_CAPABILITY, vault, principal, request))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentForgetRecord {
    pub request_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub target: TargetRef,
    pub policy: PolicyRef,
    pub evidence: QuotedEvidence,
    pub forget: String,
}
impl AgentForgetRecord {
    pub fn valid(&self, vault: &VaultId) -> bool {
        self.principal.actor_type == ActorType::Agent
            && self.request_key == agent_forget_key(vault, &self.principal, &self.request_id)
            && self.evidence.valid_shape()
            && self.evidence.reference.supports == "content"
            && self.evidence.speaker == SpeakerRole::User
            && self.evidence.reference.evidence_class == EvidenceClass::UserStatement
            && !self.forget.trim().is_empty()
            && self.forget.len() <= 8192
            && self.forget.chars().count() <= 2000
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentForgetState {
    #[serde(rename = "forget_request_saved")]
    ForgetRequestSaved,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentForgetReceipt {
    pub request_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub target: TargetRef,
    pub committed_in: CommitId,
    pub state: AgentForgetState,
}

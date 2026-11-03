//! Saved Agent correction reports: untrusted interpretation beside original
//! user evidence, never a memory revision or Owner action (ADR-MEM-121).
use super::*;
use crate::{
    common::{ActorRef, ActorType, EvidenceClass, SpeakerRole},
    hash::Sha256Hex,
    ids::*,
};
use serde::{Deserialize, Serialize};

pub const AGENT_CORRECTION_CAPABILITY: &str = "memory-agent-correction-native-1";
pub const MAX_AGENT_CORRECTIONS_PER_TARGET: usize = 32;

pub fn agent_correction_key(
    vault: &VaultId,
    principal: &ActorRef,
    request: &RequestId,
) -> Sha256Hex {
    digest(&(AGENT_CORRECTION_CAPABILITY, vault, principal, request))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCorrectionRecord {
    pub report_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub target: TargetRef,
    pub policy: PolicyRef,
    pub evidence: QuotedEvidence,
    pub correction: String,
}
impl AgentCorrectionRecord {
    pub fn valid(&self, vault: &VaultId) -> bool {
        self.principal.actor_type == ActorType::Agent
            && self.report_key == agent_correction_key(vault, &self.principal, &self.request_id)
            && self.evidence.valid_shape()
            && self.evidence.reference.supports == "content"
            && self.evidence.speaker == SpeakerRole::User
            && self.evidence.reference.evidence_class == EvidenceClass::UserStatement
            && !self.correction.trim().is_empty()
            && self.correction.len() <= 8192
            && self.correction.chars().count() <= 2000
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentCorrectionState {
    #[serde(rename = "report_saved")]
    ReportSaved,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCorrectionReceipt {
    pub report_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub target: TargetRef,
    pub committed_in: CommitId,
    pub state: AgentCorrectionState,
}

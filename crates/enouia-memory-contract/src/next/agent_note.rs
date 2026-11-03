//! Optional Agent interpretations of existing sources, never source or memory
//! authority. Persisted only by the explicitly delegated native intake.
use super::*;
use crate::{
    common::{ActorRef, ActorType},
    hash::Sha256Hex,
    ids::*,
};
use serde::{Deserialize, Serialize};

pub const AGENT_NOTE_CAPABILITY: &str = "memory-agent-note-native-1";
pub const MAX_AGENT_NOTES_PER_SOURCE: usize = 32;

pub fn agent_note_key(vault: &VaultId, principal: &ActorRef, request: &RequestId) -> Sha256Hex {
    digest(&(AGENT_NOTE_CAPABILITY, vault, principal, request))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentNoteRecord {
    pub note_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub policy: PolicyRef,
    pub evidence: QuotedEvidence,
    pub note: String,
}
impl AgentNoteRecord {
    pub fn valid(&self, vault: &VaultId) -> bool {
        self.principal.actor_type == ActorType::Agent
            && self.note_key == agent_note_key(vault, &self.principal, &self.request_id)
            && self.evidence.valid_shape()
            && !self.note.trim().is_empty()
            && self.note.len() <= 8192
            && self.note.chars().count() <= 2000
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AgentNoteState {
    #[serde(rename = "hint_saved")]
    HintSaved,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentNoteReceipt {
    pub note_key: Sha256Hex,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub committed_in: CommitId,
    pub state: AgentNoteState,
}

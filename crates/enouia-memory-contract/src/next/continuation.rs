//! Separate standing egress authority. Never a V1 ApprovalRecord or owner review.
use super::*;
use crate::{
    common::{ActorRef, ActorType, Sensitivity},
    error::Violation,
    hash::Sha256Hex,
    ids::*,
    json::{MAX_SAFE_INTEGER, Revision},
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_CONTINUATION_ATTEMPTS: usize = 64;
pub const MAX_CONTINUATION_BYTES: usize = 1024 * 1024;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GenerationApi {
    Synthetic,
    OpenaiResponses,
    AnthropicMessages,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationDestination {
    pub api: GenerationApi,
    pub model: String,
    pub adapter_version: String,
}
impl GenerationDestination {
    fn valid(&self) -> bool {
        [&self.model, &self.adapter_version]
            .iter()
            .all(|s| !s.trim().is_empty() && s.len() <= 128 && !s.chars().any(char::is_control))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContinuationPolicyKind {
    #[serde(rename = "continuous_generation_policy")]
    ContinuousGenerationPolicy,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationPolicy {
    pub schema_version: NextVersion,
    pub kind: ContinuationPolicyKind,
    pub policy_id: PolicyId,
    pub revision: Revision,
    pub granted_by: ActorRef,
    pub system_actor: ActorRef,
    pub vault_id: VaultId,
    pub admission_policy: PolicyRef,
    pub subject_id: SubjectId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub project_id: Option<ProjectId>,
    pub source_ids: Vec<SourceId>,
    pub producer_version: String,
    pub destination: GenerationDestination,
    pub max_sensitivity: Sensitivity,
    pub call_tokens: u64,
    pub call_cost_microusd: u64,
    pub total_tokens: u64,
    pub total_cost_microusd: u64,
    pub active: bool,
    pub valid_from: Timestamp,
    pub valid_until: Timestamp,
}
impl NextDocument for ContinuationPolicy {
    fn validate(&self) -> Vec<Violation> {
        let mut errors = Vec::new();
        if self.granted_by.actor_type != ActorType::Owner
            || self.system_actor.actor_type != ActorType::System
            || self.granted_by.actor_id == self.system_actor.actor_id
            || self.source_ids.is_empty()
            || self.source_ids.len() > MAX_SOURCES
            || self.source_ids.iter().collect::<BTreeSet<_>>().len() != self.source_ids.len()
            || self.max_sensitivity > Sensitivity::Private
            || !self.destination.valid()
            || self.producer_version.trim().is_empty()
            || self.producer_version.len() > 128
            || self.producer_version.chars().any(char::is_control)
            || self.valid_from >= self.valid_until
            || self.call_tokens == 0
            || self.call_cost_microusd == 0
            || self.total_tokens < self.call_tokens
            || self.total_cost_microusd < self.call_cost_microusd
            || self.total_tokens > MAX_SAFE_INTEGER
            || self.total_cost_microusd > MAX_SAFE_INTEGER
        {
            errors.push(Violation::new("continuation.policy", ""));
        }
        errors
    }
}
impl ContinuationPolicy {
    pub fn reference(&self) -> PolicyRef {
        PolicyRef {
            policy_id: self.policy_id.clone(),
            revision: self.revision,
            hash: digest(self),
        }
    }
    /// Caller supplies current native scope/lifecycle; JSON cannot authorize itself.
    pub fn covers(
        &self,
        input: &AdmissionInput,
        destination: &GenerationDestination,
        now: &Timestamp,
    ) -> bool {
        self.validate().is_empty()
            && self.active
            && now >= &self.valid_from
            && now < &self.valid_until
            && input.validate().is_empty()
            && input.actor == self.system_actor
            && input.snapshot.vault_id == self.vault_id
            && input.policy == self.admission_policy
            && input.egress_policy_id == self.policy_id
            && input.subject_id == self.subject_id
            && input.project_id == self.project_id
            && input.producer_version == self.producer_version
            && input.model_version.as_deref() == Some(destination.model.as_str())
            && &self.destination == destination
            && input.sensitivity <= self.max_sensitivity
            && input
                .input_refs
                .iter()
                .all(|r| self.source_ids.contains(&r.source_id))
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContinuationState {
    Running,
    Complete,
    Failed,
    OutcomeUnknown,
    Revoked,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationAttempt {
    pub work_key: Sha256Hex,
    pub operation_id: OperationId,
    pub input_hash: Sha256Hex,
    pub policy: PolicyRef,
    pub snapshot: SnapshotBinding,
    pub state: ContinuationState,
    pub reserved_tokens: u64,
    pub reserved_cost_microusd: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub tokens_used: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub cost_microusd: Option<u64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub response_hash: Option<Sha256Hex>,
}
impl ContinuationAttempt {
    pub fn tokens_charged(&self) -> u64 {
        self.reserved_tokens.max(self.tokens_used.unwrap_or(0))
    }
    pub fn cost_charged(&self) -> u64 {
        self.reserved_cost_microusd
            .max(self.cost_microusd.unwrap_or(0))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContinuationJournalKind {
    #[serde(rename = "continuous_generation_journal")]
    ContinuousGenerationJournal,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ContinuationJournal {
    pub schema_version: NextVersion,
    pub kind: ContinuationJournalKind,
    pub vault_id: VaultId,
    pub owner: ActorRef,
    pub system_actor: ActorRef,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub policy: Option<ContinuationPolicy>,
    pub policy_history: Vec<ContinuationPolicy>,
    pub last_ms: i64,
    pub attempts: Vec<ContinuationAttempt>,
}
impl ContinuationJournal {
    pub fn charges(&self) -> Option<(u64, u64)> {
        self.attempts
            .iter()
            .try_fold((0u64, 0u64), |(tokens, cost), a| {
                Some((
                    tokens.checked_add(a.tokens_charged())?,
                    cost.checked_add(a.cost_charged())?,
                ))
            })
    }
}
impl NextDocument for ContinuationJournal {
    const MAX_BYTES: usize = MAX_CONTINUATION_BYTES;
    fn validate(&self) -> Vec<Violation> {
        let mut errors = Vec::new();
        if self.owner.actor_type != ActorType::Owner
            || self.system_actor.actor_type != ActorType::System
            || self.owner.actor_id == self.system_actor.actor_id
            || self.last_ms < 0
            || self.last_ms as u64 > MAX_SAFE_INTEGER
            || self.attempts.len() > MAX_CONTINUATION_ATTEMPTS
            || self.policy_history.len() > MAX_CONTINUATION_ATTEMPTS
            || self.policy.as_ref() != self.policy_history.last()
            || self
                .charges()
                .is_none_or(|(t, c)| t > MAX_SAFE_INTEGER || c > MAX_SAFE_INTEGER)
            || self.policy.as_ref().is_some_and(|p| {
                !p.validate().is_empty()
                    || p.vault_id != self.vault_id
                    || p.granted_by != self.owner
                    || p.system_actor != self.system_actor
            })
        {
            errors.push(Violation::new("continuation.journal", ""));
        }
        for (index, policy) in self.policy_history.iter().enumerate() {
            if !policy.validate().is_empty()
                || policy.revision.get() != index as u64 + 1
                || policy.vault_id != self.vault_id
                || policy.granted_by != self.owner
                || policy.system_actor != self.system_actor
                || self
                    .policy
                    .as_ref()
                    .is_none_or(|p| p.policy_id != policy.policy_id)
            {
                errors.push(Violation::new(
                    "continuation.policy_history",
                    "/policy_history",
                ));
            }
        }
        let mut keys = BTreeSet::new();
        let mut operations = BTreeSet::new();
        for a in &self.attempts {
            if !keys.insert(&a.work_key)
                || !operations.insert(&a.operation_id)
                || a.snapshot.vault_id != self.vault_id
                || !self
                    .policy_history
                    .iter()
                    .any(|p| p.reference() == a.policy)
                || a.reserved_tokens == 0
                || a.reserved_cost_microusd == 0
                || [
                    Some(a.reserved_tokens),
                    Some(a.reserved_cost_microusd),
                    a.tokens_used,
                    a.cost_microusd,
                ]
                .into_iter()
                .flatten()
                .any(|v| v > MAX_SAFE_INTEGER)
                || (a.state == ContinuationState::Complete) != a.response_hash.is_some()
                || (a.state == ContinuationState::Complete
                    && (a.tokens_used.is_none() || a.cost_microusd.is_none()))
                || (a.state == ContinuationState::Running
                    && (a.tokens_used.is_some() || a.cost_microusd.is_some()))
            {
                errors.push(Violation::new("continuation.attempt", "/attempts"));
            }
        }
        if self
            .attempts
            .iter()
            .filter(|a| a.state == ContinuationState::Running)
            .count()
            > 1
        {
            errors.push(Violation::new("continuation.concurrency", "/attempts"));
        }
        errors
    }
}

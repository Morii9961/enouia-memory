use super::{Fault, Result};
use enouia_memory_contract::{
    common::ActorType,
    hash::{Sha256Hex, sha256},
    json::{MAX_SAFE_INTEGER, canonical_bytes},
    next::*,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const MAX_JOBS: usize = 16;
pub const MAX_ITEMS: usize = 8;
pub const MAX_JOURNAL_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Limits {
    pub max_jobs: u64,
    pub total_tokens: u64,
    pub total_cost_microusd: u64,
    pub call_tokens: u64,
    pub call_cost_microusd: u64,
    pub call_timeout_ms: u64,
    pub max_result_bytes: u64,
}
impl Limits {
    pub fn validate(&self) -> Result<()> {
        if !(1..=MAX_JOBS as u64).contains(&self.max_jobs)
            || self.call_tokens == 0
            || self.call_cost_microusd == 0
            || self.total_tokens < self.call_tokens
            || self.total_cost_microusd < self.call_cost_microusd
            || self.total_tokens > MAX_SAFE_INTEGER
            || self.total_cost_microusd > MAX_SAFE_INTEGER
            || !(1..=60_000).contains(&self.call_timeout_ms)
            || !(1..=MAX_DOCUMENT_BYTES as u64).contains(&self.max_result_bytes)
        {
            return Err(Fault::Invalid);
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Running,
    Generated,
    Cancelled,
    Invalidated,
    OutcomeUnknown,
    Failed,
    BudgetBlocked,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Job {
    pub key: Sha256Hex,
    pub inputs: Vec<AdmissionInput>,
    pub state: JobState,
    pub cursor: u64,
    pub attempts: u64,
    pub uncertain: bool,
    pub results: Vec<GenerationResult>,
}

/// Original untrusted generation response, not an admission or a commit receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GenerationResult {
    pub input_hash: Sha256Hex,
    pub response_hash: Sha256Hex,
    pub response_json: String,
}
impl GenerationResult {
    pub fn validate(&self, input: &AdmissionInput, limits: &Limits) -> Result<()> {
        if self.input_hash != digest(input)
            || self.response_json.len() as u64 > limits.max_result_bytes
            || self.response_hash != sha256(self.response_json.as_bytes())
            || serde_json::from_str::<serde_json::Value>(&self.response_json).is_err()
        {
            return Err(Fault::Invalid);
        }
        Ok(())
    }
}
impl Job {
    pub fn validate(&self, limits: &Limits) -> Result<()> {
        validate_inputs(&self.inputs)?;
        if self.key != work_key(&self.inputs)
            || self.cursor != self.results.len() as u64
            || self.cursor > self.inputs.len() as u64
            || self.attempts < self.cursor
            || self.attempts > self.cursor + 1
            || self.attempts > self.inputs.len() as u64
        {
            return Err(Fault::Invalid);
        }
        match self.state {
            JobState::Queued | JobState::BudgetBlocked
                if self.cursor == self.inputs.len() as u64
                    || self.attempts != self.cursor
                    || self.uncertain =>
            {
                return Err(Fault::Invalid);
            }
            JobState::Running
                if self.cursor == self.inputs.len() as u64
                    || self.attempts != self.cursor + 1
                    || !self.uncertain =>
            {
                return Err(Fault::Invalid);
            }
            JobState::Generated
                if self.cursor != self.inputs.len() as u64
                    || self.attempts != self.cursor
                    || self.uncertain =>
            {
                return Err(Fault::Invalid);
            }
            JobState::OutcomeUnknown if !self.uncertain || self.attempts != self.cursor + 1 => {
                return Err(Fault::Invalid);
            }
            JobState::Failed if self.uncertain || self.attempts != self.cursor + 1 => {
                return Err(Fault::Invalid);
            }
            JobState::Cancelled | JobState::Invalidated
                if self.uncertain != (self.attempts == self.cursor + 1) =>
            {
                return Err(Fault::Invalid);
            }
            _ => {}
        }
        for (input, result) in self.inputs.iter().zip(&self.results) {
            result.validate(input, limits)?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum JournalKind {
    #[serde(rename = "memory_job_journal")]
    MemoryJobJournal,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Journal {
    pub schema_version: NextVersion,
    pub kind: JournalKind,
    pub limits: Limits,
    pub reserved_tokens: u64,
    pub reserved_cost_microusd: u64,
    pub last_ms: i64,
    pub jobs: Vec<Job>,
}
impl Journal {
    pub fn parse(bytes: &[u8]) -> Result<Self> {
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(Fault::Capacity);
        }
        let value: serde_json::Value = serde_json::from_slice(bytes).map_err(|_| Fault::Invalid)?;
        if value
            .get("schema_version")
            .and_then(serde_json::Value::as_u64)
            != Some(2)
        {
            return Err(Fault::UnsupportedVersion);
        }
        let result: Self = serde_json::from_slice(bytes).map_err(|_| Fault::Invalid)?;
        result.validate()?;
        Ok(result)
    }
    pub fn validate(&self) -> Result<()> {
        self.limits.validate()?;
        if self.last_ms < 0
            || self.last_ms as u64 > MAX_SAFE_INTEGER
            || self.jobs.len() as u64 > self.limits.max_jobs
        {
            return Err(Fault::Invalid);
        }
        let mut keys = BTreeSet::new();
        let mut operations = BTreeSet::new();
        let mut work_items = BTreeSet::new();
        let mut attempts = 0;
        for job in &self.jobs {
            job.validate(&self.limits)?;
            if !keys.insert(&job.key) {
                return Err(Fault::Invalid);
            }
            for input in &job.inputs {
                if !operations.insert(&input.operation_id)
                    || !work_items.insert(work_key(std::slice::from_ref(input)))
                {
                    return Err(Fault::Invalid);
                }
            }
            attempts += job.attempts;
        }
        if self.reserved_tokens != attempts * self.limits.call_tokens
            || self.reserved_cost_microusd != attempts * self.limits.call_cost_microusd
            || self.reserved_tokens > self.limits.total_tokens
            || self.reserved_cost_microusd > self.limits.total_cost_microusd
        {
            return Err(Fault::Invalid);
        }
        let bytes = canonical_bytes(self).map_err(|_| Fault::Invalid)?.len() as u64;
        // Reserve worst-case JSON string escaping and fixed wrapper growth
        // BEFORE admitting work. A valid completed response must always fit;
        // otherwise a full journal could strand a running job at publication.
        let remaining: u64 = self
            .jobs
            .iter()
            .filter(|job| {
                matches!(
                    job.state,
                    JobState::Queued | JobState::Running | JobState::BudgetBlocked
                ) || (job.state == JobState::Invalidated && !job.uncertain)
            })
            .map(|job| job.inputs.len() as u64 - job.cursor)
            .sum();
        let future = remaining * (self.limits.max_result_bytes * 6 + 512);
        if bytes + future + 512 > MAX_JOURNAL_BYTES as u64 {
            return Err(Fault::Capacity);
        }
        Ok(())
    }
}

pub(super) fn validate_inputs(inputs: &[AdmissionInput]) -> Result<()> {
    if inputs.is_empty() || inputs.len() > MAX_ITEMS {
        return Err(Fault::Invalid);
    }
    let first = &inputs[0];
    let mut operations = BTreeSet::new();
    let mut results = BTreeSet::new();
    let mut work_items = BTreeSet::new();
    for input in inputs {
        if !input.validate().is_empty()
            || input.actor.actor_type != ActorType::System
            || input.snapshot != first.snapshot
            || input.policy != first.policy
            || input.actor != first.actor
            || !operations.insert(&input.operation_id)
            || !results.insert(&input.result.memory_id)
            || !work_items.insert(work_key(std::slice::from_ref(input)))
        {
            return Err(Fault::Invalid);
        }
    }
    Ok(())
}

/// Work identity excludes generated operation/result IDs, preserving originals
/// on replay. Exact evidence, dependencies, policy and snapshot remain bound.
pub fn work_key(inputs: &[AdmissionInput]) -> Sha256Hex {
    admission_batch_key(inputs)
}

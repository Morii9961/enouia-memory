//! Durable admission for owner-selected local history materialization.
//! This describes local work only; it never grants egress or approves memory.
use crate::{
    error::Violation,
    hash::Sha256Hex,
    ids::{CommitId, ImportId, SubjectId},
    json::Revision,
    session::ProviderBinding,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Missing is legacy data; a present field must be an actual admission.
pub fn deserialize_admission<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<HistoryMaterialization>, D::Error> {
    HistoryMaterialization::deserialize(deserializer).map(Some)
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryMaterialization {
    pub import_id: ImportId,
    pub import_revision: Revision,
    pub input_object_hash: Sha256Hex,
    pub preview_plan_hash: Sha256Hex,
    pub preview_commit_id: CommitId,
    pub messages_per_batch: u32,
    pub max_path_references: u32,
    /// Explicit selection order. Export branch selection is not approval.
    pub batch_indices: Vec<u32>,
    pub binding: ProviderBinding,
    pub subject_id: SubjectId,
    pub max_candidates: u64,
    pub max_pending: u64,
    pub max_reserved_tokens: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub max_reserved_cost_microusd: Option<u64>,
    /// Hash of the local admission key, never a credential or sending grant.
    pub submission_key_hash: Sha256Hex,
}

impl HistoryMaterialization {
    pub fn validate(&self) -> Vec<Violation> {
        let bad = !(1..=15).contains(&self.messages_per_batch)
            || !(1..=100_000).contains(&self.max_path_references)
            || self.batch_indices.is_empty()
            || self.batch_indices.len() > 32
            || self.batch_indices.iter().any(|index| *index >= 100_000)
            || self.batch_indices.iter().collect::<BTreeSet<_>>().len() != self.batch_indices.len()
            || !matches!(self.binding.provider.as_str(), "openai" | "anthropic")
            || self.binding.adapter_version != "text-http-1"
            || self.binding.model.is_empty()
            || self.binding.model.len() > 160
            || !self
                .binding
                .model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"-_.:".contains(&b))
            || !(1..=32).contains(&self.max_candidates)
            || !(1..=500).contains(&self.max_pending)
            || !(1..=9_007_199_254_740_991).contains(&self.max_reserved_tokens)
            || self
                .max_reserved_cost_microusd
                .is_some_and(|n| n > 9_007_199_254_740_991);
        if bad {
            vec![Violation::new(
                "history.admission_bounds",
                "/history_materialization",
            )]
        } else {
            vec![]
        }
    }
}

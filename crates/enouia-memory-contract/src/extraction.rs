//! Optional bounded model extraction, persisted with the owning Session.
use crate::{
    common::SourceRevisionRef,
    error::Violation,
    hash::Sha256Hex,
    ids::{CandidateId, EventId, ExtractionRunId, SubjectId},
    session::ProviderBinding,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionSource {
    pub source: SourceRevisionRef,
    pub content_hash: Sha256Hex,
    /// UTF-8 byte offsets within the resolved source text, not a filesystem path.
    pub start: u64,
    pub end: u64,
    pub snippet_hash: Sha256Hex,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtractionState {
    Ready,
    Paused,
    Completed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExtractionJob {
    pub run_id: ExtractionRunId,
    pub input_event_id: EventId,
    pub input_hash: Sha256Hex,
    pub prompt_version: String,
    pub binding: ProviderBinding,
    pub subject_id: SubjectId,
    pub sources: Vec<ExtractionSource>,
    pub max_candidates: u64,
    pub max_pending: u64,
    pub max_reserved_tokens: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub max_reserved_cost_microusd: Option<u64>,
    pub state: ExtractionState,
    /// Cursor into the immutable saved response, advanced after each proposal.
    pub cursor: u64,
    /// null represents a suppressed, already reviewed candidate.
    pub candidates: Vec<Option<CandidateId>>,
}
impl ExtractionJob {
    pub fn validate(&self) -> Vec<Violation> {
        let bad = !matches!(
            self.prompt_version.as_str(),
            "extract-text-1" | "extract-text-2"
        ) || !matches!(self.binding.provider.as_str(), "openai" | "anthropic")
            || self.binding.model.is_empty()
            || self.binding.model.len() > 160
            || !self
                .binding
                .model
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
            || self.binding.adapter_version != "text-http-1"
            || self.sources.is_empty()
            || self.sources.len() > 16
            || self.sources.iter().any(|s| {
                s.start >= s.end || s.end > 9_007_199_254_740_991 || s.end - s.start > 32_768
            })
            || self
                .sources
                .iter()
                .try_fold(0u64, |sum, s| sum.checked_add(s.end.checked_sub(s.start)?))
                .is_none_or(|sum| sum > 65_536)
            || self.max_candidates == 0
            || self.max_candidates > 32
            || self.max_pending == 0
            || self.max_pending > 500
            || self.max_reserved_tokens == 0
            || self.max_reserved_tokens > 9_007_199_254_740_991
            || self
                .max_reserved_cost_microusd
                .is_some_and(|n| n > 9_007_199_254_740_991)
            || self.cursor != self.candidates.len() as u64
            || self.cursor > self.max_candidates;
        if bad {
            vec![Violation::new("extraction.shape", "/extraction_jobs")]
        } else {
            vec![]
        }
    }
}

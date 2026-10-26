//! Deterministic selection of already authorized candidates. No grant or IO.
use super::*;
use crate::{error::Violation, hash::Sha256Hex, ids::MemoryId, json::Revision, time::Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AttentionAlgorithm {
    #[serde(rename = "attention-rank-1")]
    V1,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AttentionProfileKind {
    #[serde(rename = "attention_profile")]
    AttentionProfile,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecallAlias {
    pub cue: String,
    pub terms: Vec<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionProfile {
    pub schema_version: NextVersion,
    pub kind: AttentionProfileKind,
    pub revision: Revision,
    pub algorithm: AttentionAlgorithm,
    pub fade_after_days: u64,
    pub archive_after_days: u64,
    pub protected_forms: Vec<FormationKind>,
    pub protected_ids: Vec<MemoryId>,
    pub aliases: Vec<RecallAlias>,
    pub max_items: u64,
    pub max_bytes: u64,
    pub maintenance_interval_ms: u64,
}
fn text_ok(text: &str) -> bool {
    !text.trim().is_empty() && text.len() <= 256 && !text.chars().any(char::is_control)
}
impl NextDocument for AttentionProfile {
    fn validate(&self) -> Vec<Violation> {
        if self.fade_after_days == 0
            || self.archive_after_days <= self.fade_after_days
            || self.archive_after_days > 36_500
            || self.protected_forms.iter().collect::<BTreeSet<_>>().len()
                != self.protected_forms.len()
            || self.protected_ids.len() > 128
            || self.protected_ids.iter().collect::<BTreeSet<_>>().len() != self.protected_ids.len()
            || self.aliases.len() > 16
            || self.aliases.iter().any(|a| {
                !text_ok(&a.cue)
                    || a.terms.is_empty()
                    || a.terms.len() > 8
                    || a.terms.iter().any(|s| !text_ok(s))
            })
            || !(1..=32).contains(&self.max_items)
            || !(1..=65_536).contains(&self.max_bytes)
            || !(100..=60_000).contains(&self.maintenance_interval_ms)
        {
            vec![Violation::new("attention.profile", "")]
        } else {
            vec![]
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecallIntent {
    Current,
    Historical,
    Browse,
}
pub struct RecallQuery {
    pub text: String,
    pub conditions: Vec<String>,
    pub event_window: Option<FormationTime>,
    pub at: Timestamp,
    pub intent: RecallIntent,
    pub limit: usize,
    pub max_bytes: usize,
}
impl RecallQuery {
    pub fn valid(&self, profile: &AttentionProfile) -> bool {
        self.text.len() <= 2048
            && self.conditions.len() <= 8
            && self.conditions.iter().all(|s| text_ok(s))
            && (self.intent == RecallIntent::Browse || !self.text.trim().is_empty())
            && self
                .event_window
                .as_ref()
                .is_none_or(|t| t.until.as_ref().is_none_or(|end| end > &t.from))
            && self.limit > 0
            && self.limit as u64 <= profile.max_items
            && self.max_bytes > 0
            && self.max_bytes as u64 <= profile.max_bytes
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionState {
    Active,
    Fading,
    Archived,
    Protected,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallReason {
    Lexical,
    Alias,
    Conditions,
    TimeWindow,
    Protected,
    Fresh,
    Fading,
    HistoricalRescue,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecallExclusionReason {
    NotAdopted,
    Conditions,
    TimeWindow,
    Validity,
    Archived,
    NoCue,
    Budget,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionRow {
    pub memory: MemoryRevisionRef,
    pub state: AttentionState,
    pub source_time: Timestamp,
}
pub struct RecallCandidate<'a> {
    pub memory: &'a MachineMemory,
    pub source_time: Timestamp,
    pub rendered_bytes: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecallSelection {
    pub memory: MemoryRevisionRef,
    pub attention: AttentionState,
    pub score: u64,
    pub reasons: Vec<RecallReason>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecallTrace {
    pub algorithm: AttentionAlgorithm,
    pub snapshot: SnapshotBinding,
    pub profile_hash: Sha256Hex,
    pub at: Timestamp,
    pub authorized_at: Timestamp,
    pub considered: usize,
    pub selected: Vec<RecallSelection>,
    pub excluded: Vec<(MemoryRevisionRef, RecallExclusionReason)>,
    pub rendered_bytes: usize,
}
pub fn attention_state(
    profile: &AttentionProfile,
    memory: &MachineMemory,
    source_time: &Timestamp,
    at: &Timestamp,
) -> AttentionState {
    if profile.protected_ids.contains(&memory.memory.memory_id)
        || memory
            .decision
            .input
            .dimensions
            .as_ref()
            .is_some_and(|d| profile.protected_forms.contains(&d.kind))
    {
        return AttentionState::Protected;
    }
    let days = at.unix_ms().saturating_sub(source_time.unix_ms()).max(0) as u64 / 86_400_000;
    if days >= profile.archive_after_days {
        AttentionState::Archived
    } else if days >= profile.fade_after_days {
        AttentionState::Fading
    } else {
        AttentionState::Active
    }
}
/// Candidate construction is a native trust boundary. Never pass inaccessible
/// records, even to obtain a hidden count or exclusion reason.
pub fn select_recall(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    current_time: &Timestamp,
    candidates: &[RecallCandidate<'_>],
) -> Result<RecallTrace, crate::ContractError> {
    select_with_limit(profile, query, snapshot, current_time, candidates, 128)
}
/// Explicit scalable native selection. Its potentially large exclusion vector
/// is not a legacy wire trace; transports must declare bounded trace coverage.
/// Callers supply only independently authorized current candidates.
pub fn select_catalog_recall(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    current_time: &Timestamp,
    candidates: &[RecallCandidate<'_>],
) -> Result<RecallTrace, crate::ContractError> {
    select_with_limit(
        profile,
        query,
        snapshot,
        current_time,
        candidates,
        MAX_SEGMENTED_REVISIONS,
    )
}
fn select_with_limit(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    current_time: &Timestamp,
    candidates: &[RecallCandidate<'_>],
    max_candidates: usize,
) -> Result<RecallTrace, crate::ContractError> {
    super::checked(profile.validate())?;
    if !query.valid(profile) {
        return Err(super::invalid("attention.query", ""));
    }
    if candidates.len() > max_candidates
        || candidates
            .iter()
            .map(|c| &c.memory.memory.memory_id)
            .collect::<BTreeSet<_>>()
            .len()
            != candidates.len()
    {
        return Err(super::invalid("attention.candidates", ""));
    }
    let mut trace = RecallTrace {
        algorithm: profile.algorithm,
        snapshot: snapshot.clone(),
        profile_hash: digest(profile),
        at: query.at.clone(),
        authorized_at: current_time.clone(),
        considered: candidates.len(),
        selected: vec![],
        excluded: vec![],
        rendered_bytes: 0,
    };
    let words: Vec<_> = query
        .text
        .split_whitespace()
        .map(str::to_lowercase)
        .collect();
    let mut ranked = Vec::new();
    for candidate in candidates {
        let memory = candidate.memory;
        let input = &memory.decision.input;
        let dimensions = input.dimensions.as_ref();
        let state = attention_state(profile, memory, &candidate.source_time, &query.at);
        let body = input.content.to_lowercase();
        let mut reasons = Vec::new();
        let lexical = words.iter().filter(|w| body.contains(w.as_str())).count() as u64;
        let alias = profile
            .aliases
            .iter()
            .filter(|a| query.text.to_lowercase().contains(&a.cue.to_lowercase()))
            .flat_map(|a| &a.terms)
            .any(|term| body.contains(&term.to_lowercase()));
        let exclude = if memory.decision.outcome.adoption != AdoptionState::Adopted {
            Some(RecallExclusionReason::NotAdopted)
        } else if dimensions.is_some_and(|d| {
            d.conditions.iter().any(|condition| {
                !query
                    .conditions
                    .iter()
                    .any(|c| c.eq_ignore_ascii_case(condition))
            })
        }) {
            Some(RecallExclusionReason::Conditions)
        } else if query.intent != RecallIntent::Historical
            && dimensions
                .and_then(|d| d.valid_time.as_ref())
                .is_some_and(|t| {
                    current_time < &t.from
                        || t.until.as_ref().is_some_and(|end| current_time >= end)
                })
        {
            Some(RecallExclusionReason::Validity)
        } else if query.event_window.as_ref().is_some_and(|window| {
            dimensions
                .and_then(|d| d.event_time.as_ref())
                .is_none_or(|event| {
                    if let Some(end) = &event.until {
                        end <= &window.from
                            || window.until.as_ref().is_some_and(|w| &event.from >= w)
                    } else {
                        event.from < window.from
                            || window.until.as_ref().is_some_and(|w| &event.from >= w)
                    }
                })
        }) {
            Some(RecallExclusionReason::TimeWindow)
        } else if lexical == 0 && !alias && query.intent != RecallIntent::Browse {
            Some(RecallExclusionReason::NoCue)
        } else if state == AttentionState::Archived
            && (query.intent != RecallIntent::Historical || lexical == 0 && !alias)
        {
            Some(RecallExclusionReason::Archived)
        } else {
            None
        };
        if let Some(reason) = exclude {
            trace.excluded.push((memory.memory.clone(), reason));
            continue;
        }
        if lexical > 0 {
            reasons.push(RecallReason::Lexical);
        }
        if alias {
            reasons.push(RecallReason::Alias);
        }
        if dimensions.is_some_and(|d| !d.conditions.is_empty()) {
            reasons.push(RecallReason::Conditions);
        }
        if query.event_window.is_some() {
            reasons.push(RecallReason::TimeWindow);
        }
        let age_score = match state {
            AttentionState::Active => {
                reasons.push(RecallReason::Fresh);
                3
            }
            AttentionState::Fading => {
                reasons.push(RecallReason::Fading);
                1
            }
            AttentionState::Archived => {
                reasons.push(RecallReason::HistoricalRescue);
                0
            }
            AttentionState::Protected => {
                reasons.push(RecallReason::Protected);
                3
            }
        };
        let score = lexical * 100 + u64::from(alias) * 60 + age_score;
        ranked.push((
            RecallSelection {
                memory: memory.memory.clone(),
                attention: state,
                score,
                reasons,
            },
            candidate.rendered_bytes,
        ));
    }
    ranked.sort_by(|(a, _), (b, _)| {
        b.score
            .cmp(&a.score)
            .then_with(|| a.memory.memory_id.cmp(&b.memory.memory_id))
            .then_with(|| b.memory.revision.cmp(&a.memory.revision))
    });
    for (selection, bytes) in ranked {
        let bytes = bytes.saturating_add(usize::from(!trace.selected.is_empty()));
        if trace.selected.len() >= query.limit
            || trace
                .rendered_bytes
                .checked_add(bytes)
                .is_none_or(|b| b > query.max_bytes)
        {
            trace
                .excluded
                .push((selection.memory, RecallExclusionReason::Budget));
            continue;
        }
        trace.rendered_bytes += bytes;
        trace.selected.push(selection);
    }
    Ok(trace)
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AttentionProjectionKind {
    #[serde(rename = "attention_projection")]
    AttentionProjection,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionProjection {
    pub schema_version: NextVersion,
    pub kind: AttentionProjectionKind,
    pub algorithm: AttentionAlgorithm,
    pub snapshot: SnapshotBinding,
    pub profile_hash: Sha256Hex,
    pub at: Timestamp,
    pub rows: Vec<AttentionRow>,
}
impl NextDocument for AttentionProjection {
    fn validate(&self) -> Vec<Violation> {
        if self.rows.len() > 128
            || self
                .rows
                .iter()
                .map(|r| &r.memory.memory_id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.rows.len()
        {
            vec![Violation::new("attention.projection", "")]
        } else {
            vec![]
        }
    }
}

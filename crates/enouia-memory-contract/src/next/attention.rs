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
    select_with_limit(
        profile,
        query,
        snapshot,
        current_time,
        candidates,
        128,
        None,
    )
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
        None,
    )
}
/// Native qualification supplies one attention state per already authorized
/// candidate (for example catalog-4 Owner protection). These values are tuning
/// inputs, never grants. All adoption/condition/time/budget rules still apply.
pub fn select_qualified_catalog_recall(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    current_time: &Timestamp,
    candidates: &[RecallCandidate<'_>],
    attention: &[AttentionState],
) -> Result<RecallTrace, crate::ContractError> {
    select_with_limit(
        profile,
        query,
        snapshot,
        current_time,
        candidates,
        MAX_SEGMENTED_REVISIONS,
        Some(attention),
    )
}
/// The lexical terms of a recall query, lowercased and without duplicates.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct RecallTerms {
    /// Each whitespace-separated word, plus the non-CJK parts of a mixed word
    /// with at least two letters or digits ("Enouia迁移" adds "enouia"). A
    /// memory containing one counts one hit, as before (ADR-MEM-209).
    pub words: Vec<String>,
    /// Each run of CJK characters ("我去过京都吗"). Memories are matched
    /// against fragments of these runs (ADR-MEM-212).
    pub runs: Vec<String>,
}
/// ADR-MEM-212: the longest CJK fragment considered, in characters.
pub const MAX_RECALL_FRAGMENT_CHARS: usize = 8;
/// ADR-MEM-212: CJK fragments weighed per query.
pub const MAX_RECALL_FRAGMENTS: usize = 256;
/// ADR-MEM-209/212: Chinese, Japanese and Korean are written without spaces,
/// so their runs are matched by fragment.
pub fn recall_terms(text: &str) -> RecallTerms {
    let cjk = |c: char| matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{ac00}'..='\u{d7af}');
    let mut terms = RecallTerms::default();
    let mut seen = BTreeSet::new();
    for word in text.split_whitespace() {
        let word = word.to_lowercase();
        if seen.insert(word.clone()) {
            terms.words.push(word.clone());
        }
        let chars: Vec<char> = word.chars().collect();
        let mut start = 0;
        while start < chars.len() {
            let kind = cjk(chars[start]);
            let mut end = start;
            while end < chars.len() && cjk(chars[end]) == kind {
                end += 1;
            }
            let run: String = chars[start..end].iter().collect();
            if kind {
                if !terms.runs.contains(&run) {
                    terms.runs.push(run);
                }
            } else if run.chars().filter(|c| c.is_alphanumeric()).count() >= 2
                && seen.insert(run.clone())
            {
                terms.words.push(run);
            }
            start = end;
        }
    }
    terms
}
fn select_with_limit(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    current_time: &Timestamp,
    candidates: &[RecallCandidate<'_>],
    max_candidates: usize,
    qualified_attention: Option<&[AttentionState]>,
) -> Result<RecallTrace, crate::ContractError> {
    super::checked(profile.validate())?;
    if !query.valid(profile) {
        return Err(super::invalid("attention.query", ""));
    }
    if qualified_attention.is_some_and(|states| states.len() != candidates.len())
        || candidates.len() > max_candidates
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
    let terms = recall_terms(&query.text);
    let bodies: Vec<String> = candidates
        .iter()
        .map(|c| c.memory.decision.input.content.to_lowercase())
        .collect();
    // ADR-MEM-212: a CJK fragment weighs log2(N / df) over the candidates,
    // in integers so every platform ranks alike, and never more than its
    // rarer character: "我去过" (common characters) weighs little, "京都"
    // keeps the weight of 京. A memory takes, per run, its best fragment's
    // weight times its length, so containing the whole word "摄影集" beats
    // containing "摄影" many times.
    let n = candidates.len() as u64;
    let df = |term: &str| bodies.iter().filter(|b| b.contains(term)).count() as u64;
    let weight = |df: u64| n.checked_div(df).map_or(0, |q| u64::from(q.ilog2()));
    let mut fragments: Vec<(usize, String, u64, u64)> = Vec::new();
    for (run_index, run) in terms.runs.iter().enumerate() {
        let chars: Vec<char> = run.chars().collect();
        let char_df: Vec<u64> = chars
            .iter()
            .map(|c| df(c.encode_utf8(&mut [0; 4])))
            .collect();
        for len in 1..=chars.len().min(MAX_RECALL_FRAGMENT_CHARS) {
            for start in 0..=chars.len() - len {
                if fragments.len() >= MAX_RECALL_FRAGMENTS {
                    break;
                }
                let text: String = chars[start..start + len].iter().collect();
                let found = df(&text);
                let rarer = char_df[start..start + len]
                    .iter()
                    .copied()
                    .min()
                    .unwrap_or(0);
                let w = if found == 0 {
                    0
                } else {
                    weight(found.max(rarer))
                };
                if w > 0
                    && !fragments
                        .iter()
                        .any(|(r, t, ..)| *r == run_index && *t == text)
                {
                    fragments.push((run_index, text, len as u64, w));
                }
            }
        }
    }
    // ADR-MEM-212: only strong fragments count. A fragment of two or more
    // characters needs weight 2 (in at most a quarter of the memories); a
    // single character needs weight 5 (at most 1 in 32), so a rare topic
    // character such as 茶 counts and 京 or 摄 alone does not. A library too
    // small to reach these uses the heaviest weight it has.
    let heaviest = |single: bool| {
        fragments
            .iter()
            .filter(|(.., len, _)| (*len == 1) == single)
            .map(|(.., w)| *w)
            .max()
            .unwrap_or(0)
    };
    let (piece_strong, char_strong) = (heaviest(false).min(2), heaviest(true).min(5));
    fragments.retain(|(.., len, w)| *w >= if *len == 1 { char_strong } else { piece_strong });
    let lexical: Vec<u64> = bodies
        .iter()
        .map(|body| {
            let words = terms
                .words
                .iter()
                .filter(|w| body.contains(w.as_str()))
                .count() as u64;
            let fragments: u64 = (0..terms.runs.len())
                .map(|run_index| {
                    fragments
                        .iter()
                        .filter(|(r, text, ..)| *r == run_index && body.contains(text.as_str()))
                        .map(|(.., len, w)| w * len)
                        .max()
                        .unwrap_or(0)
                })
                .sum();
            words + fragments
        })
        .collect();
    let mut ranked = Vec::new();
    for (index, candidate) in candidates.iter().enumerate() {
        let memory = candidate.memory;
        let input = &memory.decision.input;
        let dimensions = input.dimensions.as_ref();
        let state = qualified_attention.map_or_else(
            || attention_state(profile, memory, &candidate.source_time, &query.at),
            |states| states[index],
        );
        let body = &bodies[index];
        let mut reasons = Vec::new();
        let lexical = lexical[index];
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

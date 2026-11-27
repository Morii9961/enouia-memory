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
    /// ADR-MEM-220: similar in meaning by a local model's vectors.
    Semantic,
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
        RecallExtras::default(),
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
        RecallExtras::default(),
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
        RecallExtras {
            attention: Some(attention),
            ..RecallExtras::default()
        },
    )
}
/// ADR-MEM-220: catalog or qualified selection with semantic vectors. The
/// input's candidate vectors follow `candidates`; a candidate without one is
/// scored lexically only.
pub fn select_semantic_recall(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    current_time: &Timestamp,
    candidates: &[RecallCandidate<'_>],
    attention: Option<&[AttentionState]>,
    semantic: SemanticInput<'_>,
) -> Result<RecallTrace, crate::ContractError> {
    select_with_limit(
        profile,
        query,
        snapshot,
        current_time,
        candidates,
        MAX_SEGMENTED_REVISIONS,
        RecallExtras {
            attention,
            semantic: Some(semantic),
            ..RecallExtras::default()
        },
    )
}
/// Optional inputs a native caller computed before selection. None of them
/// grants anything; every adoption, condition, time and budget rule applies.
#[derive(Clone, Copy, Debug, Default)]
pub struct RecallExtras<'a> {
    /// One attention state per candidate (native qualification).
    pub attention: Option<&'a [AttentionState]>,
    /// ADR-MEM-220/221: semantic vectors.
    pub semantic: Option<SemanticInput<'a>>,
    /// ADR-MEM-219: the query's characters that lie inside its topic nouns
    /// (京都, 茶, 摄影集), as the host's tagger found them. A strong CJK term
    /// made only of these counts twice.
    pub naming: Option<&'a BTreeSet<char>>,
    /// ADR-MEM-225: the query's words of two or more characters as the
    /// host's segmenter cut them (大学, 摄影). A two-character piece equal to
    /// one is a word, however common its characters are elsewhere.
    pub words: Option<&'a BTreeSet<String>>,
}
/// Catalog or qualified selection with any of the optional inputs.
pub fn select_recall_with(
    profile: &AttentionProfile,
    query: &RecallQuery,
    snapshot: &SnapshotBinding,
    current_time: &Timestamp,
    candidates: &[RecallCandidate<'_>],
    extras: RecallExtras<'_>,
) -> Result<RecallTrace, crate::ContractError> {
    select_with_limit(
        profile,
        query,
        snapshot,
        current_time,
        candidates,
        MAX_SEGMENTED_REVISIONS,
        extras,
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
    /// against its pieces and characters (ADR-MEM-213).
    pub runs: Vec<String>,
}
/// ADR-MEM-212/213: CJK pieces and characters weighed per query.
pub const MAX_RECALL_FRAGMENTS: usize = 256;
/// ADR-MEM-209/212: Chinese, Japanese and Korean are written without spaces,
/// so their runs are matched by fragment.
/// ADR-MEM-214: characters that ask rather than name: sentence particles
/// and interrogatives. A question is matched against statements, where these
/// are rare, so rarity would overrate them; they split CJK runs instead.
pub const RECALL_QUESTION_CHARS: &str = "吗呢吧啊呀嘛么什怎哪谁几";
pub fn recall_terms(text: &str) -> RecallTerms {
    // 0: other text, 1: CJK, 2: a question character (dropped).
    let kind = |c: char| {
        if RECALL_QUESTION_CHARS.contains(c) {
            2
        } else if matches!(c, '\u{3040}'..='\u{30ff}' | '\u{3400}'..='\u{4dbf}' | '\u{4e00}'..='\u{9fff}' | '\u{f900}'..='\u{faff}' | '\u{ac00}'..='\u{d7af}')
        {
            1
        } else {
            0
        }
    };
    let mut terms = RecallTerms::default();
    let mut seen = BTreeSet::new();
    for word in text.split_whitespace() {
        let word = word.to_lowercase();
        let chars: Vec<char> = word.chars().collect();
        if !chars.iter().all(|c| kind(*c) == 2) && seen.insert(word.clone()) {
            terms.words.push(word.clone());
        }
        let mut start = 0;
        while start < chars.len() {
            let this = kind(chars[start]);
            let mut end = start;
            while end < chars.len() && kind(chars[end]) == this {
                end += 1;
            }
            let run: String = chars[start..end].iter().collect();
            if this == 1 {
                if !terms.runs.contains(&run) {
                    terms.runs.push(run);
                }
            } else if this == 0
                && run.chars().filter(|c| c.is_alphanumeric()).count() >= 2
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
    extras: RecallExtras<'_>,
) -> Result<RecallTrace, crate::ContractError> {
    let (qualified_attention, semantic) = (extras.attention, extras.semantic);
    super::checked(profile.validate())?;
    if !query.valid(profile) {
        return Err(super::invalid("attention.query", ""));
    }
    if qualified_attention.is_some_and(|states| states.len() != candidates.len())
        || semantic.is_some_and(|s| s.candidates.len() != candidates.len() || !s.query.valid())
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
    // ADR-MEM-214: recall matches a memory's content and its title.
    let bodies: Vec<String> = candidates
        .iter()
        .map(|c| {
            let input = &c.memory.decision.input;
            match &input.record_fields {
                Some(fields) => format!("{}\n{}", input.content, fields.title),
                None => input.content.clone(),
            }
            .to_lowercase()
        })
        .collect();
    // ADR-MEM-213: each CJK run of the query gives its two-character pieces
    // and its characters. A term weighs log2(N / df) over the candidates, in
    // integers so every platform ranks alike; a piece never more than its
    // rarer character, so "我去" or "去过" (common characters) weighs little
    // while "京都" keeps the weight of 京.
    let n = candidates.len() as u64;
    let df = |term: &str| bodies.iter().filter(|b| b.contains(term)).count() as u64;
    let weight = |df: u64| n.checked_div(df).map_or(0, |q| u64::from(q.ilog2()));
    // ADR-MEM-214: a piece is a word, not a chance pairing, when it holds
    // more than a quarter of its rarer character's memories: 京都 holds most
    // of 京, while 过京 (from "说过京都") holds almost none. A chance pairing
    // is weak whatever its weight.
    let mut pieces: Vec<(String, u64, bool)> = Vec::new();
    let mut chars: Vec<(String, u64)> = Vec::new();
    for run in &terms.runs {
        let run: Vec<char> = run.chars().collect();
        for pair in run.windows(2) {
            let text: String = pair.iter().collect();
            if pieces.len() + chars.len() >= MAX_RECALL_FRAGMENTS
                || pieces.iter().any(|(t, ..)| *t == text)
            {
                continue;
            }
            let found = df(&text);
            let rarer = pair
                .iter()
                .map(|c| df(c.encode_utf8(&mut [0; 4])))
                .min()
                .unwrap_or(0);
            let w = if found == 0 {
                0
            } else {
                weight(found.max(rarer))
            };
            // ADR-MEM-225: a word the segmenter found is not a chance pairing
            // even when one of its characters is common in other words (大学
            // where 学 mostly means 学习).
            let segmented = extras.words.is_some_and(|words| words.contains(&text));
            if w > 0 {
                pieces.push((text, w, found * 4 > rarer || segmented));
            }
        }
        for c in &run {
            let text = c.to_string();
            if pieces.len() + chars.len() >= MAX_RECALL_FRAGMENTS
                || chars.iter().any(|(t, _)| *t == text)
            {
                continue;
            }
            let w = weight(df(&text));
            if w > 0 {
                chars.push((text, w));
            }
        }
    }
    // ADR-MEM-214: strong terms decide; weak ones only order ties. A piece is
    // strong at weight 3 (in at most an eighth of the memories), a character
    // at weight 4 (at most 1 in 16): 京都, 酒 or 茶 are strong, a chance
    // pairing of common characters such as 去过 is weak. A library too small
    // to reach these uses the heaviest weight it has.
    let piece_strong = pieces
        .iter()
        .filter(|(.., word)| *word)
        .map(|(_, w, _)| *w)
        .max()
        .unwrap_or(0)
        .min(3);
    let char_strong = chars.iter().map(|(_, w)| *w).max().unwrap_or(0).min(4);
    let cjk = !pieces.is_empty() || !chars.is_empty();
    // ADR-MEM-219: the host tags the query's words. A strong term inside a noun
    // (京都, 茶, 摄影集) names what is asked about and counts twice; verbs,
    // adverbs, time and measure words (去过, 喜欢, 平时, 第一次) frame the
    // question and count once.
    let no_naming = BTreeSet::new();
    let naming = extras.naming.unwrap_or(&no_naming);
    let names = |term: &str| {
        if term.chars().all(|c| naming.contains(&c)) {
            2
        } else {
            1
        }
    };
    // A title names what a memory is about: a strong piece in it counts as
    // two body mentions (ADR-MEM-214).
    let titles: Vec<String> = candidates
        .iter()
        .map(|c| {
            c.memory
                .decision
                .input
                .record_fields
                .as_ref()
                .map_or(String::new(), |f| f.title.to_lowercase())
        })
        .collect();
    let lexical: Vec<u64> = bodies
        .iter()
        .zip(&titles)
        .map(|(body, title)| {
            let words = terms
                .words
                .iter()
                .filter(|w| body.contains(w.as_str()))
                .count() as u64;
            if !cjk {
                return words;
            }
            // A strong piece counts up to twice (a title and its body, or a
            // memory returning to the word); a character inside a matched
            // piece is not counted again; weak terms and further repeats only
            // break ties.
            let mut strong = words;
            let mut weak = 0;
            let mut covered = BTreeSet::new();
            for (piece, w, word) in &pieces {
                let times = body.matches(piece.as_str()).take(4).count() as u64;
                if times == 0 {
                    continue;
                }
                // ADR-MEM-215: only a word covers its characters; a chance
                // pairing (影集 where 影 mostly means 电影) leaves 集 to count.
                if *word {
                    covered.extend(piece.chars());
                }
                if *word && *w >= piece_strong {
                    let named = if title.contains(piece.as_str()) { 1 } else { 0 };
                    let mentions = times + named;
                    strong += w * mentions.min(2) * names(piece);
                    weak += mentions.saturating_sub(2);
                } else {
                    weak += w;
                }
            }
            for (c, w) in &chars {
                if body.contains(c.as_str()) && !c.chars().any(|c| covered.contains(&c)) {
                    if *w >= char_strong {
                        strong += w * names(c);
                    } else {
                        weak += w;
                    }
                }
            }
            // ADR-MEM-214: holding a whole run of three or more characters
            // (摄影集) adds its heaviest piece once more. ADR-MEM-216: a title
            // naming that whole run adds it again, outside the two-mention
            // cap, so the memory about 摄影集 leads one repeating 摄影.
            for run in &terms.runs {
                if run.chars().count() >= 3 && body.contains(run.as_str()) {
                    let heaviest = run
                        .chars()
                        .collect::<Vec<_>>()
                        .windows(2)
                        .filter_map(|pair| {
                            let text: String = pair.iter().collect();
                            pieces.iter().find(|(t, ..)| *t == text).map(|(_, w, _)| *w)
                        })
                        .max()
                        .unwrap_or(0);
                    strong += heaviest;
                    if title.contains(run.as_str()) {
                        strong += heaviest;
                    }
                }
            }
            if strong == 0 {
                0
            } else {
                strong * 16 + weak.min(15)
            }
        })
        .collect();
    // ADR-MEM-221: each candidate's similarity to the query, and their spread.
    let similarities: Vec<Option<i64>> = (0..candidates.len())
        .map(|i| semantic.and_then(|s| s.candidates[i].and_then(|v| semantic_milli(s.query, v))))
        .collect();
    let semantic_stats =
        SemanticStats::of(&similarities.iter().flatten().copied().collect::<Vec<_>>());
    let mut ranked = Vec::new();
    let mut filtered = Vec::new();
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
        // ADR-MEM-220/221: a similarity standing out from this query's own
        // distribution adds score, and further out it cues the memory alone.
        let (semantic_bonus, semantic_cue) =
            similarities[index].map_or((0, false), |s| semantic_stats.score(s));
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
        } else if lexical == 0 && !alias && !semantic_cue && query.intent != RecallIntent::Browse {
            Some(RecallExclusionReason::NoCue)
        } else if state == AttentionState::Archived
            && (query.intent != RecallIntent::Historical || lexical == 0 && !alias && !semantic_cue)
        {
            Some(RecallExclusionReason::Archived)
        } else {
            None
        };
        if let Some(reason) = exclude {
            filtered.push((memory.memory.clone(), reason));
            continue;
        }
        if lexical > 0 {
            reasons.push(RecallReason::Lexical);
        }
        if semantic_bonus > 0 {
            reasons.push(RecallReason::Semantic);
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
        let score = lexical * 100 + semantic_bonus + u64::from(alias) * 60 + age_score;
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
    // ADR-MEM-227: the memories that had a cue but did not fit come first,
    // best first, so a bounded trace shows the near misses before the
    // memories nothing in the query pointed to.
    trace.excluded.extend(filtered);
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

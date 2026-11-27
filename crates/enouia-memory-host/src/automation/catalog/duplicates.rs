//! ADR-MEM-230: possible duplicates among the selected current memories, for
//! Morii to review. Read-only and recomputed from the freshly authorized
//! set; resolving one (withdraw, forget, correct) stays an Owner action.
use super::projections::{finish, header};
use super::*;
use enouia_memory_contract::time::Timestamp;
use serde::{Deserialize, Serialize};

/// Two contents share at least this share of their character pairs
/// (Jaccard, in thousandths).
pub const DUPLICATE_WORDING_MILLI: u64 = 600;
/// Shorter contents are too short for the wording signal.
pub const DUPLICATE_MIN_PAIRS: usize = 4;
/// A model similarity at least this high (cosine, in thousandths). On
/// Runtime's real library (bge-m3) the closest pairs of distinct memories,
/// such as two exams of one course, stayed at or below 830.
pub const DUPLICATE_SEMANTIC_MILLI: i64 = 900;
/// Pairs are compared among at most this many selected memories.
pub const MAX_DUPLICATE_CANDIDATES: usize = 2000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateQuery {
    /// Also compare model vectors when the reader has an index.
    pub semantic: bool,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DuplicateReason {
    /// Most of their wording is the same.
    SameWording,
    /// The local model finds them nearly identical in meaning.
    SameMeaning,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateMember {
    pub memory: TargetRef,
    pub title: String,
    pub captured_at: Timestamp,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DuplicateSuggestion {
    /// The earlier captured memory first.
    pub earlier: DuplicateMember,
    pub later: DuplicateMember,
    pub reasons: Vec<DuplicateReason>,
    pub wording_milli: u64,
    pub semantic_milli: Option<i64>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryDuplicateSuggestions {
    pub kind: String,
    pub header: ProjectionHeader,
    pub query: DuplicateQuery,
    /// Whether model similarity was compared for this page.
    pub semantic_compared: bool,
    /// Every pair found; `items` holds the closest within the budget.
    pub total_pairs: u64,
    /// False when more than `MAX_DUPLICATE_CANDIDATES` memories were selected.
    pub complete_in_scope: bool,
    pub items: Vec<DuplicateSuggestion>,
}
impl NextDocument for MemoryDuplicateSuggestions {
    fn validate(&self) -> Vec<enouia_memory_contract::error::Violation> {
        let ok = self.items.len() as u64 <= self.total_pairs
            && self.items.iter().all(|s| {
                !s.reasons.is_empty()
                    && s.earlier.memory.memory.memory_id != s.later.memory.memory.memory_id
                    && s.earlier.captured_at <= s.later.captured_at
                    && s.wording_milli <= 1000
                    && s.reasons.contains(&DuplicateReason::SameMeaning)
                        == s.semantic_milli
                            .is_some_and(|m| m >= DUPLICATE_SEMANTIC_MILLI)
                    && s.reasons.contains(&DuplicateReason::SameWording)
                        == (s.wording_milli >= DUPLICATE_WORDING_MILLI)
            });
        if ok {
            vec![]
        } else {
            vec![enouia_memory_contract::error::Violation::new(
                "duplicates",
                "",
            )]
        }
    }
}
/// Character pairs of the letters and digits of a text, lowercased.
fn pairs(text: &str) -> BTreeSet<(char, char)> {
    let chars: Vec<char> = text
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    chars.windows(2).map(|w| (w[0], w[1])).collect()
}
impl SegmentedMemoryReader {
    /// Pairs of selected memories that may say the same thing, closest
    /// first. Memories in one revision lineage (one derived from or
    /// reverting to the other) are versions, not duplicates, and are left
    /// out. Suggestions only.
    pub fn memory_duplicate_suggestions_native(
        &self,
        request: &CatalogReadRequest<'_>,
        query: &DuplicateQuery,
    ) -> ApplicationResult<MemoryDuplicateSuggestions> {
        self.read(request, |data| {
            let selection = selection(&data, request, None)?;
            let now = selection.evaluated_at.clone();
            let all = rows(&data, request.profile, &now)?;
            let selected: Vec<_> = all
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .collect();
            let compared = &selected[..selected.len().min(MAX_DUPLICATE_CANDIDATES)];
            let index = self.semantic.as_deref().filter(|_| query.semantic);
            let vectors: Vec<Option<SemanticVector>> = compared
                .iter()
                .map(|r| {
                    index.and_then(|i| {
                        i.memory(&r.entry.target)
                            .filter(|v| v.valid() && v.model == i.model())
                    })
                })
                .collect();
            let semantic_compared = vectors.iter().filter(|v| v.is_some()).count() >= 2;
            let wording: Vec<_> = compared
                .iter()
                .map(|r| pairs(&r.entry.memory.decision.input.content))
                .collect();
            let lineage = |a: &Row<'_>, b: &Row<'_>| {
                let d = a.entry.memory.decision.input.dimensions.as_ref();
                d.is_some_and(|d| {
                    d.derived_from
                        .iter()
                        .chain(d.reverts_to.iter())
                        .any(|t| t.memory.memory_id == *b.id())
                })
            };
            let member = |r: &Row<'_>| DuplicateMember {
                memory: r.entry.target.clone(),
                title: r
                    .entry
                    .memory
                    .decision
                    .input
                    .record_fields
                    .as_ref()
                    .map_or(String::new(), |f| f.title.clone()),
                captured_at: r.captured.clone(),
            };
            let mut found = Vec::new();
            for i in 0..compared.len() {
                for j in i + 1..compared.len() {
                    let (a, b) = (compared[i], compared[j]);
                    if lineage(a, b) || lineage(b, a) {
                        continue;
                    }
                    let (wa, wb) = (&wording[i], &wording[j]);
                    let wording_milli =
                        if wa.len() < DUPLICATE_MIN_PAIRS || wb.len() < DUPLICATE_MIN_PAIRS {
                            0
                        } else {
                            let shared = wa.intersection(wb).count() as u64;
                            shared * 1000 / (wa.len() as u64 + wb.len() as u64 - shared)
                        };
                    let semantic_milli = vectors[i]
                        .as_ref()
                        .zip(vectors[j].as_ref())
                        .and_then(|(x, y)| semantic_milli(x, y));
                    let mut reasons = Vec::new();
                    if wording_milli >= DUPLICATE_WORDING_MILLI {
                        reasons.push(DuplicateReason::SameWording);
                    }
                    if semantic_milli.is_some_and(|m| m >= DUPLICATE_SEMANTIC_MILLI) {
                        reasons.push(DuplicateReason::SameMeaning);
                    }
                    if reasons.is_empty() {
                        continue;
                    }
                    let (earlier, later) = if (&a.captured, a.id()) <= (&b.captured, b.id()) {
                        (a, b)
                    } else {
                        (b, a)
                    };
                    found.push(DuplicateSuggestion {
                        earlier: member(earlier),
                        later: member(later),
                        reasons,
                        wording_milli,
                        semantic_milli,
                    });
                }
            }
            // Closest first: both reasons, then the stronger similarity.
            found.sort_by(|x, y| {
                let key = |s: &DuplicateSuggestion| {
                    (
                        std::cmp::Reverse(s.reasons.len()),
                        std::cmp::Reverse(
                            s.semantic_milli.unwrap_or(0).max(s.wording_milli as i64),
                        ),
                        s.earlier.memory.memory.memory_id.clone(),
                        s.later.memory.memory.memory_id.clone(),
                    )
                };
                key(x).cmp(&key(y))
            });
            let h = header(
                &data,
                selection,
                "duplicate-suggestions-1",
                query,
                selected.len(),
                compared.len(),
                vec![
                    "duplicates_are_suggestions_morii_decides".into(),
                    "revision_lineage_is_not_a_duplicate".into(),
                    if semantic_compared {
                        "same_meaning_is_model_similarity".into()
                    } else {
                        "model_similarity_not_compared".into()
                    },
                ],
            );
            let mut result = MemoryDuplicateSuggestions {
                kind: "memory_duplicate_suggestions".into(),
                header: h,
                query: query.clone(),
                semantic_compared,
                total_pairs: found.len() as u64,
                complete_in_scope: compared.len() == selected.len(),
                items: Vec::new(),
            };
            let mut used = bytes(&result)? + 2048;
            for suggestion in found.into_iter().take(request.options.limit) {
                let cost = bytes(&suggestion)? + 16;
                if used + cost > request.options.max_bytes {
                    break;
                }
                used += cost;
                result.items.push(suggestion);
            }
            finish(result, request.options.max_bytes)
        })
    }
}

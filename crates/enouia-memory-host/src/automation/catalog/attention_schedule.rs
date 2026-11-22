//! ADR-MEM-164: time-derived attention transitions and review suggestions.
//! Read-only and recomputed from the freshly authorized native set and the
//! attention profile; nothing here is persisted or written as a revision.
use super::projections::{finish, header, same_header};
use super::*;
use enouia_memory_contract::time::Timestamp;
use serde::{Deserialize, Serialize};

const DAY_MS: i64 = 86_400_000;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionScheduleQuery {
    /// Half-open window `[from, until)`, at most 366 days.
    pub from: Timestamp,
    pub until: Timestamp,
}
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionTransition {
    Fading,
    Archived,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionScheduleEvent {
    pub memory: TargetRef,
    pub title: String,
    pub transition: AttentionTransition,
    /// Source capture time plus the profile's day count for this transition.
    pub at: Timestamp,
    /// Already happened at the evaluation time.
    pub past: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AttentionScheduleCursor {
    pub header: ProjectionHeader,
    pub at: Timestamp,
    pub memory: MemoryId,
    pub transition: AttentionTransition,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryAttentionSchedule {
    pub kind: String,
    pub header: ProjectionHeader,
    pub query: AttentionScheduleQuery,
    pub fade_after_days: u64,
    pub archive_after_days: u64,
    /// Selected memories that never fade (profile or Morii protection).
    pub protected_in_scope: u64,
    pub events: Vec<AttentionScheduleEvent>,
    pub next: Option<AttentionScheduleCursor>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewQuery {
    /// Look-ahead from the evaluation time, 1 to 90 days.
    pub horizon_days: u64,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewReason {
    /// Fading now and archived within the horizon.
    ArchivesSoon,
    /// The recorded validity ends within the horizon.
    ValidityEndsSoon,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReviewSuggestion {
    pub memory: TargetRef,
    pub title: String,
    pub attention: AttentionState,
    /// The earliest time among the reasons below.
    pub due: Timestamp,
    pub reasons: Vec<ReviewReason>,
    pub archives_at: Option<Timestamp>,
    pub valid_until: Option<Timestamp>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReviewSuggestions {
    pub kind: String,
    pub header: ProjectionHeader,
    pub query: ReviewQuery,
    /// Suggestions in the horizon; `items` holds the soonest within the budget.
    pub total_in_horizon: u64,
    pub items: Vec<ReviewSuggestion>,
}
impl NextDocument for MemoryAttentionSchedule {
    fn validate(&self) -> Vec<enouia_memory_contract::error::Violation> {
        valid(self.events.windows(2).all(|w| {
            (&w[0].at, &w[0].memory.memory.memory_id, w[0].transition)
                < (&w[1].at, &w[1].memory.memory.memory_id, w[1].transition)
        }))
    }
}
impl NextDocument for MemoryReviewSuggestions {
    fn validate(&self) -> Vec<enouia_memory_contract::error::Violation> {
        valid(
            self.items.len() as u64 <= self.total_in_horizon
                && self.items.iter().all(|s| !s.reasons.is_empty()),
        )
    }
}
fn valid(ok: bool) -> Vec<enouia_memory_contract::error::Violation> {
    if ok {
        vec![]
    } else {
        vec![enouia_memory_contract::error::Violation::new(
            "attention.schedule",
            "",
        )]
    }
}
fn plus_days(at: &Timestamp, days: u64) -> ApplicationResult<Timestamp> {
    Timestamp::from_unix_ms(at.unix_ms() + days as i64 * DAY_MS).ok_or_else(invalid)
}
impl SegmentedMemoryReader {
    /// When each selected, unprotected memory enters Fading and Archived, as
    /// `attention_state` computes them, for transitions inside the window.
    pub fn memory_attention_schedule_native(
        &self,
        request: &CatalogReadRequest<'_>,
        query: &AttentionScheduleQuery,
        cursor: Option<&AttentionScheduleCursor>,
    ) -> ApplicationResult<MemoryAttentionSchedule> {
        self.read(request, |data| {
            if query.until <= query.from
                || query.until.unix_ms() - query.from.unix_ms() > 366 * DAY_MS
            {
                return Err(invalid());
            }
            let selection = selection(&data, request, cursor.map(|c| &c.header.selection))?;
            let now = selection.evaluated_at.clone();
            let all = rows(&data, request.profile, &now)?;
            let selected: Vec<_> = all
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .collect();
            let profile = request.profile;
            let mut events = Vec::new();
            for row in &selected {
                if row.attention == AttentionState::Protected {
                    continue;
                }
                for (transition, days) in [
                    (AttentionTransition::Fading, profile.fade_after_days),
                    (AttentionTransition::Archived, profile.archive_after_days),
                ] {
                    let at = plus_days(&row.captured, days)?;
                    if at >= query.from && at < query.until {
                        events.push(AttentionScheduleEvent {
                            memory: row.entry.target.clone(),
                            title: row
                                .entry
                                .memory
                                .decision
                                .input
                                .record_fields
                                .as_ref()
                                .unwrap()
                                .title
                                .clone(),
                            transition,
                            past: at <= now,
                            at,
                        });
                    }
                }
            }
            events.sort_by(|a, b| {
                (&a.at, &a.memory.memory.memory_id, a.transition).cmp(&(
                    &b.at,
                    &b.memory.memory.memory_id,
                    b.transition,
                ))
            });
            let protected = selected
                .iter()
                .filter(|r| r.attention == AttentionState::Protected)
                .count();
            let h = header(
                &data,
                selection,
                "profile-attention-schedule-1",
                query,
                selected.len(),
                selected.len(),
                vec![
                    "transitions_are_derived_from_capture_time_and_profile_not_recorded".into(),
                    "protected_memories_have_no_transitions".into(),
                ],
            );
            if cursor.is_some_and(|c| !same_header(&c.header, &h)) {
                return Err(AutomationFault::Stale.into());
            }
            let remaining: Vec<_> = events
                .into_iter()
                .filter(|e| {
                    cursor.is_none_or(|c| {
                        (&e.at, &e.memory.memory.memory_id, e.transition)
                            > (&c.at, &c.memory, c.transition)
                    })
                })
                .collect();
            let mut result = MemoryAttentionSchedule {
                kind: "memory_attention_schedule".into(),
                header: h.clone(),
                query: query.clone(),
                fade_after_days: profile.fade_after_days,
                archive_after_days: profile.archive_after_days,
                protected_in_scope: protected as u64,
                events: Vec::new(),
                next: None,
            };
            let mut used = bytes(&result)? + 2048;
            for event in remaining.iter().take(request.options.limit) {
                let cost = bytes(event)? + 16;
                if used + cost > request.options.max_bytes {
                    break;
                }
                used += cost;
                result.events.push(event.clone());
            }
            if result.events.len() < remaining.len() {
                let last = result.events.last().ok_or_else(invalid)?;
                result.next = Some(AttentionScheduleCursor {
                    header: h,
                    at: last.at.clone(),
                    memory: last.memory.memory.memory_id.clone(),
                    transition: last.transition,
                });
            }
            finish(result, request.options.max_bytes)
        })
    }
    /// Memories worth a look soon, with time-based reasons. Suggestions only:
    /// whether to review, correct or protect stays Morii's decision.
    pub fn memory_review_suggestions_native(
        &self,
        request: &CatalogReadRequest<'_>,
        query: &ReviewQuery,
    ) -> ApplicationResult<MemoryReviewSuggestions> {
        self.read(request, |data| {
            if !(1..=90).contains(&query.horizon_days) {
                return Err(invalid());
            }
            let selection = selection(&data, request, None)?;
            let now = selection.evaluated_at.clone();
            let end = plus_days(&now, query.horizon_days)?;
            let all = rows(&data, request.profile, &now)?;
            let selected: Vec<_> = all
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .collect();
            let mut suggestions = Vec::new();
            for row in &selected {
                let mut reasons = Vec::new();
                let archives_at = if row.attention == AttentionState::Fading {
                    Some(plus_days(
                        &row.captured,
                        request.profile.archive_after_days,
                    )?)
                    .filter(|at| at > &now && at <= &end)
                } else {
                    None
                };
                if archives_at.is_some() {
                    reasons.push(ReviewReason::ArchivesSoon);
                }
                let valid_until = row
                    .entry
                    .memory
                    .decision
                    .input
                    .dimensions
                    .as_ref()
                    .unwrap()
                    .valid_time
                    .as_ref()
                    .and_then(|t| t.until.clone())
                    .filter(|at| at > &now && at <= &end);
                if valid_until.is_some() {
                    reasons.push(ReviewReason::ValidityEndsSoon);
                }
                let Some(due) = [archives_at.clone(), valid_until.clone()]
                    .into_iter()
                    .flatten()
                    .min()
                else {
                    continue;
                };
                suggestions.push(ReviewSuggestion {
                    memory: row.entry.target.clone(),
                    title: row
                        .entry
                        .memory
                        .decision
                        .input
                        .record_fields
                        .as_ref()
                        .unwrap()
                        .title
                        .clone(),
                    attention: row.attention,
                    due,
                    reasons,
                    archives_at,
                    valid_until,
                });
            }
            suggestions.sort_by(|a, b| {
                (&a.due, &a.memory.memory.memory_id).cmp(&(&b.due, &b.memory.memory.memory_id))
            });
            let h = header(
                &data,
                selection,
                "time-review-suggestions-1",
                query,
                selected.len(),
                selected.len(),
                vec![
                    "review_suggestions_are_time_based_not_content_judgements".into(),
                    "items_are_the_soonest_within_limit_and_byte_budget".into(),
                ],
            );
            let mut result = MemoryReviewSuggestions {
                kind: "memory_review_suggestions".into(),
                header: h,
                query: query.clone(),
                total_in_horizon: suggestions.len() as u64,
                items: Vec::new(),
            };
            let mut used = bytes(&result)? + 2048;
            for suggestion in suggestions.into_iter().take(request.options.limit) {
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

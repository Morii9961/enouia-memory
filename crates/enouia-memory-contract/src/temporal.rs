//! Deterministic valid-time rules (DATA_MODEL §5, CONTEXT_MODEL §2/§5).
//!
//! `superseded` is a relation, not a global off switch: a replacement that takes
//! effect in the future never hides a fact that is still in effect now.
//! Inputs are the latest revisions visible at one pinned commit (`known_at`).

use crate::common::Volatility;
use crate::context::Currency;
use crate::memory::{CanonicalMemory, MemoryStatus};
use crate::time::{BusinessTime, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    InEffect {
        /// A replacement exists whose start is unknown and later than `as_of`
        /// could be; the fact is kept but must be re-verified.
        supersession_time_unknown: bool,
    },
    NotYetEffective,
    Ended,
    Superseded {
        by: String,
    },
    Archived,
}

/// When a supersession edge takes effect. An explicit value wins; an unknown
/// value falls back to the replacement's own `valid_from`; if both are unknown
/// the owner's approval time is the latest point by which it certainly applies.
fn supersession_start(replacement: &CanonicalMemory, edge: &BusinessTime) -> (Timestamp, bool) {
    match (edge, &replacement.valid_from) {
        (BusinessTime::Known(at), _) => (at.clone(), false),
        (BusinessTime::Unknown, Some(from)) => (from.clone(), false),
        (BusinessTime::Unknown, None) => (replacement.approved_at.clone(), true),
    }
}

pub fn effect(memory: &CanonicalMemory, latest: &[&CanonicalMemory], as_of: &Timestamp) -> Effect {
    if memory.status == MemoryStatus::Archived {
        return Effect::Archived;
    }
    if memory.valid_from.as_ref().is_some_and(|from| from > as_of) {
        return Effect::NotYetEffective;
    }
    if memory
        .valid_until
        .as_ref()
        .is_some_and(|until| until <= as_of)
    {
        return Effect::Ended;
    }
    let mut unknown = false;
    for replacement in latest {
        if replacement.memory_id == memory.memory_id || replacement.status == MemoryStatus::Archived
        {
            continue;
        }
        for edge in replacement
            .supersedes
            .iter()
            .filter(|edge| edge.memory_id == memory.memory_id)
        {
            let (start, inferred) = supersession_start(replacement, &edge.effective_from);
            if &start <= as_of {
                return Effect::Superseded {
                    by: replacement.memory_id.to_string(),
                };
            }
            unknown |= inferred;
        }
    }
    Effect::InEffect {
        supersession_time_unknown: unknown,
    }
}

/// Currency label for a record that `effect` did not exclude. Returns None for
/// records that must not be used at `as_of` at all (not yet effective, archived).
pub fn currency(
    memory: &CanonicalMemory,
    latest: &[&CanonicalMemory],
    as_of: &Timestamp,
    now: &Timestamp,
) -> Option<Currency> {
    match effect(memory, latest, as_of) {
        Effect::NotYetEffective | Effect::Archived => None,
        Effect::Ended | Effect::Superseded { .. } => Some(Currency::HistoricalOnly),
        Effect::InEffect {
            supersession_time_unknown,
        } => {
            let conflicted = memory.conflict_group_id.as_ref().is_some_and(|group| {
                latest.iter().any(|other| {
                    other.memory_id != memory.memory_id
                        && other.conflict_group_id.as_ref() == Some(group)
                        && matches!(effect(other, latest, as_of), Effect::InEffect { .. })
                })
            });
            let current_question = as_of >= now;
            if conflicted {
                Some(Currency::Conflicted)
            } else if current_question
                && (memory.volatility == Volatility::Live
                    || memory.review_after.as_ref().is_some_and(|due| due <= now)
                    || supersession_time_unknown)
            {
                Some(Currency::NeedsReverification)
            } else {
                Some(Currency::CurrentSupported)
            }
        }
    }
}

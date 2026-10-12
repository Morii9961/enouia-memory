//! Read-only historical initialization planning (ADR-MEM-49).
//!
//! References address the supplied adapter output, not approved memories.
//! Keep the original bytes and the matching ParsedInput together. The plan
//! neither reads a Vault nor authorizes extraction, disclosure, or review.

use crate::parsed::{ParentLink, ParsedInput, Unit};
use enouia_memory_contract::source::{Availability, Completeness};
use enouia_memory_contract::time::Timestamp;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct MessageRef {
    pub unit: usize,
    pub message: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryBatch {
    /// A root-to-leaf path; distinct edited branches never share a batch.
    pub leaf: MessageRef,
    /// Export selection only, not owner approval or evidence of truth.
    pub selected_in_export: bool,
    pub observed_path_start: Option<Timestamp>,
    pub messages: Vec<MessageRef>,
    /// Previous message on this same path, for context across batch limits.
    pub preceding: Option<MessageRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum HistoryIssueKind {
    AmbiguousMessageId,
    BrokenParentChain,
    UnknownSelection,
    NonMonotonicTime,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryIssue {
    pub unit: usize,
    pub kind: HistoryIssueKind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryPlan {
    /// Each parsed message appears exactly once here or in `undated`.
    pub chronological: Vec<MessageRef>,
    pub undated: Vec<MessageRef>,
    /// Actual observed range, not proof of complete platform history.
    pub earliest: Option<Timestamp>,
    pub latest: Option<Timestamp>,
    pub unparseable: u64,
    pub incomplete_messages: usize,
    pub unavailable_attachments: usize,
    pub batches: Vec<HistoryBatch>,
    /// Retained for reconciliation; unsafe graphs get no extraction batches.
    pub issues: Vec<HistoryIssue>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct HistoryLimits {
    pub messages_per_batch: usize,
    /// Counts repeated ancestors in different paths as well as new messages.
    pub max_path_references: usize,
}

impl Default for HistoryLimits {
    fn default() -> Self {
        Self {
            messages_per_batch: 32,
            max_path_references: 100_000,
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HistoryPlanError {
    InvalidLimits,
    ReferenceBudgetExceeded,
}

/// Plan only already-parsed input. There is no file, network, secret, or
/// memory-write port. Callers must separately authorize any subsequent use.
pub fn plan_history(
    input: &ParsedInput,
    limits: HistoryLimits,
) -> Result<HistoryPlan, HistoryPlanError> {
    if !(1..=256).contains(&limits.messages_per_batch) || limits.max_path_references == 0 {
        return Err(HistoryPlanError::InvalidLimits);
    }
    let mut plan = HistoryPlan {
        chronological: Vec::new(),
        undated: Vec::new(),
        earliest: None,
        latest: None,
        unparseable: 0,
        incomplete_messages: 0,
        unavailable_attachments: 0,
        batches: Vec::new(),
        issues: Vec::new(),
    };
    let mut references = 0usize;
    for (unit_index, unit) in input.units.iter().enumerate() {
        plan.unparseable += unit.unparseable;
        for (message_index, message) in unit.messages.iter().enumerate() {
            let reference = MessageRef {
                unit: unit_index,
                message: message_index,
            };
            if message.occurred_at.is_some() {
                plan.chronological.push(reference);
            } else {
                plan.undated.push(reference);
            }
            plan.incomplete_messages += usize::from(message.completeness != Completeness::Complete);
            plan.unavailable_attachments += message
                .attachments
                .iter()
                .filter(|a| a.availability != Availability::Present)
                .count();
        }
        let paths = match paths(unit, limits.max_path_references - references) {
            Ok(Some(paths)) => paths,
            Ok(None) => return Err(HistoryPlanError::ReferenceBudgetExceeded),
            Err(kind) => {
                plan.issues.push(HistoryIssue {
                    unit: unit_index,
                    kind,
                });
                continue;
            }
        };
        let selection_known = !unit.threaded
            || paths.iter().any(|path| {
                unit.current_leaf.as_ref().is_some_and(|selected| {
                    unit.messages[*path.last().expect("nonempty path")]
                        .upstream_id
                        .as_ref()
                        == Some(selected)
                })
            });
        if !selection_known {
            plan.issues.push(HistoryIssue {
                unit: unit_index,
                kind: HistoryIssueKind::UnknownSelection,
            });
        }
        for path in paths {
            references = references
                .checked_add(path.len())
                .filter(|&n| n <= limits.max_path_references)
                .ok_or(HistoryPlanError::ReferenceBudgetExceeded)?;
            let known_times: Vec<_> = path
                .iter()
                .filter_map(|&i| unit.messages[i].occurred_at.as_ref())
                .collect();
            let observed_path_start = known_times.iter().copied().min().cloned();
            if known_times.windows(2).any(|w| w[0] > w[1]) {
                plan.issues.push(HistoryIssue {
                    unit: unit_index,
                    kind: HistoryIssueKind::NonMonotonicTime,
                });
            }
            let leaf_index = *path.last().expect("nonempty path");
            let leaf = MessageRef {
                unit: unit_index,
                message: leaf_index,
            };
            let selected = unit.threaded
                && unit.current_leaf.is_some()
                && unit.current_leaf == unit.messages[leaf_index].upstream_id;
            let mut preceding = None;
            for chunk in path.chunks(limits.messages_per_batch) {
                let messages: Vec<_> = chunk
                    .iter()
                    .map(|&message| MessageRef {
                        unit: unit_index,
                        message,
                    })
                    .collect();
                let last = messages.last().copied();
                plan.batches.push(HistoryBatch {
                    leaf,
                    selected_in_export: selected,
                    observed_path_start: observed_path_start.clone(),
                    messages,
                    preceding,
                });
                preceding = last;
            }
        }
    }
    // Time is for navigation only. Never reorder a reply path by wall clock.
    plan.chronological.sort_by(|a, b| {
        input.units[a.unit].messages[a.message]
            .occurred_at
            .cmp(&input.units[b.unit].messages[b.message].occurred_at)
            .then_with(|| a.cmp(b))
    });
    plan.earliest = plan
        .chronological
        .first()
        .and_then(|r| input.units[r.unit].messages[r.message].occurred_at.clone());
    plan.latest = plan
        .chronological
        .last()
        .and_then(|r| input.units[r.unit].messages[r.message].occurred_at.clone());
    plan.issues.sort_by_key(|i| (i.unit, i.kind));
    plan.issues.dedup();
    // Stable sort keeps successive chunks of the same path together.
    plan.batches.sort_by(|a, b| {
        a.observed_path_start
            .is_none()
            .cmp(&b.observed_path_start.is_none())
            .then_with(|| a.observed_path_start.cmp(&b.observed_path_start))
            .then_with(|| a.leaf.cmp(&b.leaf))
    });
    Ok(plan)
}

/// Validate each parent chain once before expanding branches. This prevents
/// cycles hidden in a disconnected component from disappearing from coverage.
fn paths(unit: &Unit, budget: usize) -> Result<Option<Vec<Vec<usize>>>, HistoryIssueKind> {
    if unit.messages.is_empty() {
        return Ok(Some(Vec::new()));
    }
    if !unit.threaded {
        return Ok(
            (unit.messages.len() <= budget).then(|| vec![(0..unit.messages.len()).collect()])
        );
    }
    let mut ids = BTreeMap::new();
    for (i, message) in unit.messages.iter().enumerate() {
        let Some(id) = message.upstream_id.as_deref() else {
            return Err(HistoryIssueKind::AmbiguousMessageId);
        };
        if ids.insert(id, i).is_some() {
            return Err(HistoryIssueKind::AmbiguousMessageId);
        }
    }
    let mut parents = Vec::new();
    let mut non_leaves = BTreeSet::new();
    for message in &unit.messages {
        let parent = match &message.parent {
            ParentLink::Root => None,
            ParentLink::Upstream(id) => Some(
                *ids.get(id.as_str())
                    .ok_or(HistoryIssueKind::BrokenParentChain)?,
            ),
            ParentLink::Missing => return Err(HistoryIssueKind::BrokenParentChain),
        };
        if let Some(p) = parent {
            non_leaves.insert(p);
        }
        parents.push(parent);
    }
    let mut verified = BTreeSet::new();
    for start in 0..unit.messages.len() {
        let mut visiting = BTreeSet::new();
        let mut at = Some(start);
        while let Some(i) = at {
            if verified.contains(&i) {
                break;
            }
            if !visiting.insert(i) {
                return Err(HistoryIssueKind::BrokenParentChain);
            }
            at = parents[i];
        }
        verified.extend(visiting);
    }
    let mut result = Vec::new();
    let mut references = 0;
    for leaf in (0..unit.messages.len()).filter(|i| !non_leaves.contains(i)) {
        let mut path = Vec::new();
        let mut at = Some(leaf);
        while let Some(i) = at {
            if references == budget {
                return Ok(None);
            }
            references += 1;
            path.push(i);
            at = parents[i];
        }
        path.reverse();
        result.push(path);
    }
    Ok(Some(result))
}

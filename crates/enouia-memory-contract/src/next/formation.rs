//! Explicit semantic labels and their pure constraints, not a language model.
use super::*;
use crate::{common::SourceRevisionRef, error::Violation, time::Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationKind {
    Fact,
    Preference,
    Episode,
    ProjectState,
    Plan,
    OpenLoop,
    RelationshipExperience,
    SubjectiveUnderstanding,
    Theme,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OccurrenceAssertion {
    Unknown,
    Intended,
    ReportedOccurred,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationState {
    Planned,
    Decided,
    Implemented,
    Tested,
    Released,
    Open,
    Paused,
    Cancelled,
    Completed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationChange {
    Initial,
    Update,
    Correction,
    Pause,
    Resume,
    Cancellation,
    Completion,
    Synthesis,
    Reversion,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormationTime {
    pub from: Timestamp,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub until: Option<Timestamp>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FormationDimensions {
    pub kind: FormationKind,
    pub occurrence: OccurrenceAssertion,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub state: Option<FormationState>,
    pub change: FormationChange,
    pub conditions: Vec<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub event_time: Option<FormationTime>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub valid_time: Option<FormationTime>,
    pub counter_sources: Vec<SourceRevisionRef>,
    pub derived_from: Vec<TargetRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub reverts_to: Option<TargetRef>,
}
pub(super) fn deserialize_dimensions<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<FormationDimensions>, D::Error> {
    FormationDimensions::deserialize(d).map(Some)
}
impl FormationDimensions {
    pub fn validate(&self) -> Vec<Violation> {
        use FormationKind as K;
        use FormationState as S;
        let valid_state = match self.kind {
            K::Plan => matches!(
                self.state,
                Some(S::Planned | S::Decided | S::Paused | S::Cancelled | S::Completed)
            ),
            K::ProjectState => matches!(
                self.state,
                Some(
                    S::Planned
                        | S::Decided
                        | S::Implemented
                        | S::Tested
                        | S::Released
                        | S::Paused
                        | S::Cancelled
                )
            ),
            K::OpenLoop => matches!(
                self.state,
                Some(S::Open | S::Paused | S::Cancelled | S::Completed)
            ),
            _ => self.state.is_none(),
        };
        if !valid_state
            || self.conditions.len() > 8
            || self.conditions.iter().collect::<BTreeSet<_>>().len() != self.conditions.len()
            || self
                .conditions
                .iter()
                .any(|s| s.trim().is_empty() || s.len() > 256 || s.chars().any(char::is_control))
            || [&self.event_time, &self.valid_time]
                .into_iter()
                .flatten()
                .any(|t| t.until.as_ref().is_some_and(|end| end <= &t.from))
            || (self.kind == K::Episode && self.occurrence != OccurrenceAssertion::ReportedOccurred)
            || (self.kind == K::Plan && self.occurrence == OccurrenceAssertion::ReportedOccurred)
            || (self.event_time.is_some()
                && self.occurrence != OccurrenceAssertion::ReportedOccurred)
            || self.counter_sources.len() > MAX_SOURCES
            || self.counter_sources.iter().collect::<BTreeSet<_>>().len()
                != self.counter_sources.len()
            || self.derived_from.len() > 8
            || self
                .derived_from
                .iter()
                .map(|t| &t.memory.memory_id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.derived_from.len()
            || (self.change == FormationChange::Reversion) != self.reverts_to.is_some()
        {
            vec![Violation::new("formation.shape", "/dimensions")]
        } else {
            vec![]
        }
    }
    pub fn same_claim(&self, old: &Self) -> bool {
        self.kind == old.kind
            && self.occurrence == old.occurrence
            && self.state == old.state
            && self.conditions == old.conditions
            && self.event_time == old.event_time
            && self.valid_time == old.valid_time
            && self.counter_sources == old.counter_sources
            && self.derived_from == old.derived_from
    }
}
#[derive(Clone)]
pub struct HistoricalMemoryView {
    pub target: TargetRef,
    pub content: String,
    pub interpretation: Interpretation,
    pub dimensions: Option<FormationDimensions>,
    pub input_refs: Vec<SourceRevisionRef>,
}

pub(super) fn validate_formation(
    input: &AdmissionInput,
    context: &EvaluationContext<'_>,
) -> Result<(), crate::ContractError> {
    use FormationChange as C;
    use FormationKind as K;
    use FormationState as S;
    let Some(d) = &input.dimensions else {
        return Ok(());
    };
    let deny = || super::invalid("formation.scope_or_transition", "/dimensions");
    if context.policy.rule_version != RuleVersion::V3
        || !context.policy.allowed_forms.contains(&d.kind)
        || d.counter_sources
            .iter()
            .any(|r| !input.context_source_refs.contains(r))
        || (!d.counter_sources.is_empty()
            && !matches!(
                input.interpretation,
                Interpretation::Conflicting | Interpretation::Ambiguous
            ))
        || (matches!(d.kind, K::SubjectiveUnderstanding | K::Theme)
            && input.interpretation == Interpretation::ClearSelfReport)
        || (d.kind == K::Theme && (d.derived_from.is_empty() || d.change != C::Synthesis))
    {
        return Err(deny());
    }
    if d.event_time.as_ref().is_some_and(|t| {
        &t.from > context.now || t.until.as_ref().is_some_and(|end| end > context.now)
    }) {
        return Err(deny());
    }
    for parent in &d.derived_from {
        if input.interpretation == Interpretation::ClearSelfReport {
            return Err(deny());
        }
        let p = context
            .targets
            .iter()
            .find(|p| p.target == *parent)
            .ok_or_else(deny)?;
        if p.legacy_owner_record
            || p.deleted
            || !p.writable
            || p.target.memory.memory_id == input.result.memory_id
            || p.policy != input.policy
            || p.subject_id != input.subject_id
            || p.project_id != input.project_id
            || p.access_policy_id != input.access_policy_id
            || p.egress_policy_id != input.egress_policy_id
            || p.sensitivity > input.sensitivity
            || p.input_refs.iter().any(|r| !input.input_refs.contains(r))
        {
            return Err(deny());
        }
    }
    let target = input
        .target
        .as_ref()
        .and_then(|t| context.targets.iter().find(|v| v.target == *t));
    match target {
        None => {
            if !matches!(d.change, C::Initial | C::Synthesis) {
                return Err(deny());
            }
        }
        Some(t) => {
            let old = t.dimensions.as_ref().ok_or_else(deny)?;
            if old.kind != d.kind || matches!(d.change, C::Initial | C::Synthesis) {
                return Err(deny());
            }
            if !matches!(d.change, C::Reversion)
                && input.evidence.iter().all(|e| {
                    t.input_refs.contains(&SourceRevisionRef {
                        source_id: e.source_id.clone(),
                        source_revision: e.source_revision,
                    })
                })
            {
                return Err(deny());
            }
            let terminal = matches!(old.state, Some(S::Cancelled | S::Completed | S::Released));
            let legal = match d.change {
                C::Pause => {
                    !terminal
                        && old.state.is_some()
                        && old.state != Some(S::Paused)
                        && d.state == Some(S::Paused)
                }
                C::Resume => {
                    old.state == Some(S::Paused)
                        && t.history
                            .iter()
                            .rev()
                            .filter_map(|v| v.dimensions.as_ref())
                            .find(|v| v.state != Some(S::Paused))
                            .is_some_and(|v| v.state == d.state)
                }
                C::Cancellation => {
                    !terminal && old.state.is_some() && d.state == Some(S::Cancelled)
                }
                C::Completion => {
                    !terminal && old.state != Some(S::Paused) && d.state == Some(S::Completed)
                }
                C::Update => {
                    !terminal
                        && match (old.state, d.state) {
                            (Some(S::Planned), Some(S::Decided))
                            | (Some(S::Decided), Some(S::Implemented))
                            | (Some(S::Implemented), Some(S::Tested))
                            | (Some(S::Tested), Some(S::Released)) => true,
                            (a, b) => a == b,
                        }
                }
                C::Correction => true,
                C::Reversion => {
                    let revert = d.reverts_to.as_ref().unwrap();
                    t.history.iter().any(|h| {
                        h.target.memory.revision < t.target.memory.revision
                            && &h.target == revert
                            && h.content == input.content
                            && h.interpretation == input.interpretation
                            && h.dimensions
                                .as_ref()
                                .is_some_and(|prior| d.same_claim(prior))
                            && h.input_refs.iter().all(|r| input.input_refs.contains(r))
                    })
                }
                _ => false,
            };
            if !legal {
                return Err(deny());
            }
        }
    }
    Ok(())
}

//! Versioned native read DTOs. Parsing is not native authentication.
use super::*;
use crate::{
    common::{ActorRef, ActorType, EvidenceRef, SourceRevisionRef},
    error::Violation,
    hash::Sha256Hex,
    ids::*,
    source::SourceRecord,
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemoryReadCapability {
    #[serde(rename = "memory-native-read-1")]
    V1,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryReadEffect {
    NativeRead,
    SyntheticFixture,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadCapabilities {
    pub current_list: bool,
    pub selected_history: bool,
    pub source_details: bool,
    pub local_lineage: bool,
    pub read_simulation: bool,
    pub global_timeline: bool,
    pub global_graph: bool,
    pub saved_experiment: bool,
    pub legacy_owner_history: bool,
    pub vectors: bool,
}
impl MemoryReadCapabilities {
    pub fn native() -> Self {
        Self {
            current_list: true,
            selected_history: true,
            source_details: true,
            local_lineage: true,
            read_simulation: true,
            global_timeline: false,
            global_graph: false,
            saved_experiment: false,
            legacy_owner_history: false,
            vectors: false,
        }
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadFilter {
    #[serde(deserialize_with = "crate::json::nullable")]
    pub form: Option<FormationKind>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub adoption: Option<AdoptionState>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub event_window: Option<FormationTime>,
    pub include_unknown_event_time: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadCursor {
    pub snapshot: SnapshotBinding,
    pub filter_hash: Sha256Hex,
    pub after: MemoryId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryValidity {
    Unspecified,
    InRange,
    NotYetValid,
    Expired,
}
pub fn memory_validity(dimensions: &FormationDimensions, at: &Timestamp) -> MemoryValidity {
    match &dimensions.valid_time {
        None => MemoryValidity::Unspecified,
        Some(t) if at < &t.from => MemoryValidity::NotYetValid,
        Some(t) if t.until.as_ref().is_some_and(|end| at >= end) => MemoryValidity::Expired,
        Some(_) => MemoryValidity::InRange,
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadItem {
    pub memory: MemoryRevisionRef,
    pub system_actor: ActorRef,
    pub subject_id: SubjectId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub project_id: Option<ProjectId>,
    pub content: String,
    pub knowledge: KnowledgeState,
    pub adoption: AdoptionState,
    pub dimensions: FormationDimensions,
    pub policy: PolicyRef,
    pub rule_version: RuleVersion,
    pub committed_in: CommitId,
    pub learned_at: Timestamp,
    pub source_captured_at: Timestamp,
    pub attention: AttentionState,
    pub validity: MemoryValidity,
    pub evidence: Vec<EvidenceRef>,
    pub context_source_refs: Vec<SourceRevisionRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub receipt: Option<MemoryReceipt>,
}
impl MemoryReadItem {
    pub(super) fn valid(&self) -> bool {
        self.system_actor.actor_type == ActorType::System
            && ((self.knowledge == KnowledgeState::AttributedStatement)
                == (self.adoption == AdoptionState::Adopted))
            && (!matches!(
                self.dimensions.kind,
                FormationKind::Theme | FormationKind::SubjectiveUnderstanding
            ) || self.knowledge != KnowledgeState::AttributedStatement)
            && self.rule_version == RuleVersion::V3
            && !self.content.trim().is_empty()
            && self.content.len() <= 16 * 1024
            && self.dimensions.validate().is_empty()
            && self.evidence.len() <= 128
            && self.context_source_refs.len() <= 128
            && self
                .receipt
                .as_ref()
                .is_none_or(|r| r.result == self.memory && r.committed_in == self.committed_in)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemoryPageKind {
    #[serde(rename = "automation_memory_page")]
    Page,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadPage {
    pub schema_version: NextVersion,
    pub kind: MemoryPageKind,
    pub capability: MemoryReadCapability,
    pub effect: MemoryReadEffect,
    pub capabilities: MemoryReadCapabilities,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub profile_hash: Sha256Hex,
    pub items: Vec<MemoryReadItem>,
    pub matched_in_scope: u64,
    pub unknown_event_time_in_scope: u64,
    pub complete_in_scope: bool,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub next: Option<MemoryReadCursor>,
    pub maintenance_worker_alive: bool,
}
impl NextDocument for MemoryReadPage {
    fn validate(&self) -> Vec<Violation> {
        if self.effect != MemoryReadEffect::NativeRead
            || self.capabilities != MemoryReadCapabilities::native()
            || self.items.len() > 32
            || self.matched_in_scope > 128
            || self.unknown_event_time_in_scope > 128
            || self.items.len() as u64 > self.matched_in_scope
            || self
                .items
                .iter()
                .map(|i| &i.memory.memory_id)
                .collect::<BTreeSet<_>>()
                .len()
                != self.items.len()
            || self.items.iter().any(|i| {
                !i.valid()
                    || i.validity != memory_validity(&i.dimensions, &self.authorized_at)
                    || i.learned_at > self.authorized_at
                    || i.receipt.is_none()
            })
            || self
                .items
                .windows(2)
                .any(|w| w[0].memory.memory_id >= w[1].memory.memory_id)
            || self.complete_in_scope != self.next.is_none()
            || self.next.as_ref().is_some_and(|c| {
                c.snapshot != self.snapshot
                    || self
                        .items
                        .last()
                        .is_none_or(|i| i.memory.memory_id != c.after)
            })
        {
            vec![Violation::new("read_model.page", "")]
        } else {
            vec![]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemorySourceView {
    pub record: SourceRecord,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub attributed_subject: Option<SubjectId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub text: Option<String>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryLinkKind {
    DerivedFrom,
    Reversion,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadLink {
    pub from: MemoryRevisionRef,
    pub to: TargetRef,
    pub kind: MemoryLinkKind,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemoryDetailKind {
    #[serde(rename = "automation_memory_detail")]
    Detail,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadDetail {
    pub schema_version: NextVersion,
    pub kind: MemoryDetailKind,
    pub capability: MemoryReadCapability,
    pub effect: MemoryReadEffect,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub profile_hash: Sha256Hex,
    pub item: MemoryReadItem,
    pub history: Vec<MemoryReadItem>,
    pub sources: Vec<MemorySourceView>,
    pub neighbors: Vec<MemoryReadLink>,
    pub complete_in_scope: bool,
}
impl NextDocument for MemoryReadDetail {
    fn validate(&self) -> Vec<Violation> {
        if self.effect != MemoryReadEffect::NativeRead
            || !self.item.valid()
            || self.item.validity != memory_validity(&self.item.dimensions, &self.authorized_at)
            || self.item.learned_at > self.authorized_at
            || self.item.receipt.is_none()
            || self.history.len() > 128
            || self.sources.len() > 128
            || self.neighbors.len() > 32
            || self.history.iter().any(|i| {
                !i.valid()
                    || i.validity != memory_validity(&i.dimensions, &self.authorized_at)
                    || i.learned_at > self.authorized_at
                    || i.receipt.is_none()
                    || i.memory.memory_id != self.item.memory.memory_id
                    || i.memory.revision > self.item.memory.revision
                    || (i.memory == self.item.memory && i != &self.item)
            })
            || self
                .history
                .windows(2)
                .any(|w| w[0].memory.revision >= w[1].memory.revision)
            || self
                .sources
                .iter()
                .map(|s| (&s.record.source_id, s.record.revision))
                .collect::<BTreeSet<_>>()
                .len()
                != self.sources.len()
            || self.sources.iter().any(|s| {
                !s.record.validate().is_empty()
                    || s.text.as_ref().is_some_and(|t| {
                        t.len() > 16 * 1024
                            || crate::hash::sha256(t.as_bytes()) != s.record.content_hash
                    })
            })
        {
            vec![Violation::new("read_model.detail", "")]
        } else {
            vec![]
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum MemorySimulationKind {
    #[serde(rename = "memory_recall_simulation")]
    Simulation,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryReadSimulation {
    pub schema_version: NextVersion,
    pub kind: MemorySimulationKind,
    pub capability: MemoryReadCapability,
    pub effect: MemoryReadEffect,
    pub trace: RecallTrace,
    pub rendered_context: String,
}
impl NextDocument for MemoryReadSimulation {
    fn validate(&self) -> Vec<Violation> {
        if self.trace.considered > 128
            || self.trace.selected.len() > 32
            || self.trace.excluded.len() > 128
            || self.trace.rendered_bytes != self.rendered_context.len()
            || self.rendered_context.len() > 65536
            || self.trace.selected.len() + self.trace.excluded.len() != self.trace.considered
            || self
                .trace
                .selected
                .iter()
                .map(|s| &s.memory.memory_id)
                .chain(self.trace.excluded.iter().map(|(m, _)| &m.memory_id))
                .collect::<BTreeSet<_>>()
                .len()
                != self.trace.considered
        {
            vec![Violation::new("read_model.simulation", "")]
        } else {
            vec![]
        }
    }
}

//! CanonicalMemory (five types) and the project entity (DATA_MODEL §3–§5).
//!
//! Records keep the Runtime M0 required fields (`schema_version`, `memory_id`,
//! `type`, `content`, `source_id`, `created_at`, `updated_at`, `status` with
//! `active|superseded|archived`) and add the MV-0 fields as required keys whose
//! unknown values are explicit `null`. Type-specific fields sit at top level.

use crate::common::{
    ActorRef, ActorType, EvidenceClass, EvidenceRef, Priority, Sensitivity, SourceRevisionRef,
    TimePrecision, Volatility,
};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{
    BranchId, CheckpointId, ConflictGroupId, ItemId, MemoryId, PolicyId, ProjectId, ReviewId,
    SessionId, SubjectId, TurnId,
};
use crate::json::{Extensions, Revision, SchemaVersion, validate_extensions};
use crate::scan::contains_secret_material;
use crate::time::{BusinessTime, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryType {
    Fact,
    Preference,
    Episode,
    ProjectState,
    SessionCheckpoint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryStatus {
    Active,
    Superseded,
    Archived,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EpistemicStatus {
    Asserted,
    Corroborated,
    Uncertain,
    Disputed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProvenanceState {
    Intact,
    Broken,
}

/// A precise supersession edge: which record revision, from when, for what scope.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Supersedes {
    pub memory_id: MemoryId,
    pub revision: Revision,
    pub effective_from: BusinessTime,
    pub scope: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StateKind {
    Planned,
    Decided,
    Implemented,
    Tested,
    Released,
    Unknown,
}

/// One independently evidenced project decision/state item. Design decisions,
/// plans, implementation, tests, and releases are never collapsed into one flag.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StateItem {
    pub item_id: ItemId,
    pub claim: String,
    pub evidence_refs: Vec<SourceRevisionRef>,
    pub state_kind: StateKind,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub as_of: Option<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OpenLoop {
    pub item_id: ItemId,
    pub description: String,
    pub evidence_refs: Vec<SourceRevisionRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub as_of: Option<Timestamp>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferenceStrength {
    Explicit,
    Tentative,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EventRange {
    pub from_sequence: u64,
    pub to_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FactFields {
    pub claim_key: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PreferenceFields {
    pub scope: String,
    pub strength: PreferenceStrength,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EpisodeFields {
    #[serde(deserialize_with = "crate::json::nullable")]
    pub occurred_start: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub occurred_end: Option<Timestamp>,
    pub occurred_precision: TimePrecision,
    pub participants: Vec<SubjectId>,
    pub summary: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectStateFields {
    pub state: Vec<StateItem>,
    pub decisions: Vec<StateItem>,
    pub open_loops: Vec<OpenLoop>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCheckpointFields {
    pub checkpoint_id: CheckpointId,
    pub checkpoint_revision: Revision,
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub covered_events: EventRange,
    pub coverage_hash: Sha256Hex,
    pub last_state: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub last_completed_turn_id: Option<TurnId>,
    pub open_loops: Vec<OpenLoop>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MemoryBody {
    Fact(FactFields),
    Preference(PreferenceFields),
    Episode(EpisodeFields),
    ProjectState(ProjectStateFields),
    SessionCheckpoint(SessionCheckpointFields),
}

impl MemoryBody {
    pub const fn memory_type(&self) -> MemoryType {
        match self {
            Self::Fact(_) => MemoryType::Fact,
            Self::Preference(_) => MemoryType::Preference,
            Self::Episode(_) => MemoryType::Episode,
            Self::ProjectState(_) => MemoryType::ProjectState,
            Self::SessionCheckpoint(_) => MemoryType::SessionCheckpoint,
        }
    }
}

/// Unknown top-level keys are rejected by the type-specific struct that
/// receives the flattened remainder (serde cannot deny them on this struct).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CanonicalMemory {
    pub schema_version: SchemaVersion,
    pub memory_id: MemoryId,
    pub revision: Revision,
    pub title: String,
    pub content: String,
    pub subject_ids: Vec<SubjectId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub project_id: Option<ProjectId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub category: Option<String>,
    pub tags: Vec<String>,
    pub source_id: crate::ids::SourceId,
    pub evidence: Vec<EvidenceRef>,
    pub epistemic_status: EpistemicStatus,
    /// Advisory model estimate. Never approval, never a security or truth signal.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub confidence: Option<f64>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub valid_from: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub valid_until: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub observed_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub last_verified_at: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub review_after: Option<Timestamp>,
    pub volatility: Volatility,
    pub priority: Priority,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    /// `null` means no egress authorization: external destinations are denied.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub egress_policy_id: Option<PolicyId>,
    pub status: MemoryStatus,
    pub supersedes: Vec<Supersedes>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub conflict_group_id: Option<ConflictGroupId>,
    pub provenance_state: ProvenanceState,
    pub review_id: ReviewId,
    pub approved_by: ActorRef,
    pub approved_at: Timestamp,
    #[serde(deserialize_with = "crate::json::nullable")]
    /// Owner approval bound to this exact revision's declassification.
    pub declassification_approval_id: Option<crate::ids::ApprovalId>,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub extensions: Extensions,
    #[serde(flatten)]
    pub body: MemoryBody,
}

pub fn is_label(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some('a'..='z' | '0'..='9'))
        && value.len() <= 128
        && chars.all(|c| matches!(c, 'a'..='z' | '0'..='9' | '_' | '.' | ':' | '-'))
}

impl CanonicalMemory {
    pub const fn memory_type(&self) -> MemoryType {
        self.body.memory_type()
    }

    /// Item IDs defined by this record, which evidence may `support`.
    pub fn item_ids(&self) -> Vec<&ItemId> {
        match &self.body {
            MemoryBody::ProjectState(fields) => fields
                .state
                .iter()
                .chain(&fields.decisions)
                .map(|item| &item.item_id)
                .chain(fields.open_loops.iter().map(|item| &item.item_id))
                .collect(),
            MemoryBody::SessionCheckpoint(fields) => {
                fields.open_loops.iter().map(|item| &item.item_id).collect()
            }
            _ => Vec::new(),
        }
    }

    fn item_evidence(&self) -> Vec<(&ItemId, &[SourceRevisionRef])> {
        match &self.body {
            MemoryBody::ProjectState(fields) => fields
                .state
                .iter()
                .chain(&fields.decisions)
                .map(|item| (&item.item_id, item.evidence_refs.as_slice()))
                .chain(
                    fields
                        .open_loops
                        .iter()
                        .map(|item| (&item.item_id, item.evidence_refs.as_slice())),
                )
                .collect(),
            MemoryBody::SessionCheckpoint(fields) => fields
                .open_loops
                .iter()
                .map(|item| (&item.item_id, item.evidence_refs.as_slice()))
                .collect(),
            _ => Vec::new(),
        }
    }

    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.created_at > self.updated_at {
            out.push(Violation::new("memory.time_order", "/updated_at"));
        }
        // `created_at` is when the logical memory was first created; the review
        // that produced *this* revision must precede this revision's update time.
        if self.approved_at > self.updated_at {
            out.push(Violation::new("memory.approval_time", "/approved_at"));
        }
        if let (Some(from), Some(until)) = (&self.valid_from, &self.valid_until)
            && from >= until
        {
            out.push(Violation::new("memory.valid_interval", "/valid_until"));
        }
        if self.approved_by.actor_type != ActorType::Owner {
            out.push(Violation::new("memory.approval_actor", "/approved_by"));
        }
        if self.evidence.is_empty() {
            out.push(Violation::new("memory.evidence_required", "/evidence"));
        }
        if !self.evidence.iter().any(|e| e.source_id == self.source_id) {
            out.push(Violation::new("memory.primary_source", "/source_id"));
        }
        if let Some(confidence) = self.confidence
            && !(0.0..=1.0).contains(&confidence)
        {
            out.push(Violation::new("memory.confidence_range", "/confidence"));
        }
        let items: BTreeSet<&str> = self.item_ids().iter().map(|id| id.as_str()).collect();
        if items.len() != self.item_ids().len() {
            out.push(Violation::new("memory.item_ids_unique", "/"));
        }
        for (index, evidence) in self.evidence.iter().enumerate() {
            evidence
                .locator
                .validate(&format!("/evidence/{index}/locator"), &mut out);
            if evidence.supports != "content" && !items.contains(evidence.supports.as_str()) {
                out.push(Violation::new(
                    "memory.evidence_supports",
                    format!("/evidence/{index}/supports"),
                ));
            }
        }
        let cited: BTreeSet<SourceRevisionRef> = self
            .evidence
            .iter()
            .map(|e| SourceRevisionRef {
                source_id: e.source_id.clone(),
                source_revision: e.source_revision,
            })
            .collect();
        for (item, refs) in self.item_evidence() {
            if refs.is_empty() || refs.iter().any(|r| !cited.contains(r)) {
                out.push(Violation::new("memory.item_evidence", item.as_str()));
            }
        }
        if self.epistemic_status == EpistemicStatus::Corroborated {
            let distinct: BTreeSet<_> = self.evidence.iter().map(|e| &e.source_id).collect();
            let independent = self.evidence.iter().any(|e| {
                !matches!(
                    e.evidence_class,
                    EvidenceClass::ModelClaim | EvidenceClass::Summary | EvidenceClass::Unknown
                )
            });
            if distinct.len() < 2 || !independent {
                out.push(Violation::new("memory.corroborated", "/epistemic_status"));
            }
        }
        let mut targets = BTreeSet::new();
        for (index, edge) in self.supersedes.iter().enumerate() {
            if edge.memory_id == self.memory_id {
                out.push(Violation::new(
                    "memory.self_supersede",
                    format!("/supersedes/{index}"),
                ));
            }
            if !targets.insert(&edge.memory_id) {
                out.push(Violation::new(
                    "memory.duplicate_supersede",
                    format!("/supersedes/{index}"),
                ));
            }
            if !is_label(&edge.scope) {
                out.push(Violation::new(
                    "memory.supersede_scope",
                    format!("/supersedes/{index}/scope"),
                ));
            }
        }
        for tag in &self.tags {
            if !is_label(tag) {
                out.push(Violation::new("memory.tag", "/tags"));
            }
        }
        if let Some(category) = &self.category
            && !is_label(category)
        {
            out.push(Violation::new("memory.category", "/category"));
        }
        if contains_secret_material(&self.content) || contains_secret_material(&self.title) {
            out.push(Violation::new("memory.secret_material", "/content"));
        }
        self.validate_body(&mut out);
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }

    fn validate_body(&self, out: &mut Vec<Violation>) {
        match &self.body {
            MemoryBody::Fact(fields) => {
                if self.subject_ids.is_empty() {
                    out.push(Violation::new("memory.subjects_required", "/subject_ids"));
                }
                if !is_label(&fields.claim_key) {
                    out.push(Violation::new("memory.claim_key", "/claim_key"));
                }
            }
            MemoryBody::Preference(fields) => {
                if self.subject_ids.is_empty() {
                    out.push(Violation::new("memory.subjects_required", "/subject_ids"));
                }
                if fields.scope.trim().is_empty() {
                    out.push(Violation::new("memory.preference_scope", "/scope"));
                }
            }
            MemoryBody::Episode(fields) => {
                if let (Some(start), Some(end)) = (&fields.occurred_start, &fields.occurred_end)
                    && start > end
                {
                    out.push(Violation::new("memory.episode_interval", "/occurred_end"));
                }
                if fields.occurred_start.is_none()
                    && fields.occurred_precision != TimePrecision::Unknown
                {
                    out.push(Violation::new(
                        "memory.episode_precision",
                        "/occurred_precision",
                    ));
                }
            }
            MemoryBody::ProjectState(fields) => {
                if self.project_id.is_none() {
                    out.push(Violation::new("memory.project_required", "/project_id"));
                }
                if fields.state.is_empty() && fields.decisions.is_empty() {
                    out.push(Violation::new("memory.project_items", "/state"));
                }
                if fields
                    .decisions
                    .iter()
                    .any(|item| item.state_kind != StateKind::Decided)
                {
                    out.push(Violation::new("memory.decision_kind", "/decisions"));
                }
            }
            MemoryBody::SessionCheckpoint(fields) => {
                if fields.covered_events.from_sequence == 0
                    || fields.covered_events.from_sequence > fields.covered_events.to_sequence
                {
                    out.push(Violation::new("memory.checkpoint_range", "/covered_events"));
                }
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProjectStatus {
    Active,
    Archived,
}

/// Stable project entity. Renames add aliases; similarly named projects are
/// never merged by string similarity (MoriMeta and Moriium are distinct).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectEntity {
    pub schema_version: SchemaVersion,
    pub project_id: ProjectId,
    pub revision: Revision,
    pub display_name: String,
    pub aliases: Vec<String>,
    pub status: ProjectStatus,
    pub sensitivity: Sensitivity,
    pub review_id: ReviewId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub extensions: Extensions,
}

pub fn fold_alias(value: &str) -> String {
    value.trim().to_lowercase()
}

impl ProjectEntity {
    pub fn names(&self) -> Vec<String> {
        std::iter::once(&self.display_name)
            .chain(&self.aliases)
            .map(|name| fold_alias(name))
            .collect()
    }

    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let names = self.names();
        let unique: BTreeSet<&String> = names.iter().collect();
        if unique.len() != names.len() || names.iter().any(String::is_empty) {
            out.push(Violation::new("project.alias_unique", "/aliases"));
        }
        if self.created_at > self.updated_at {
            out.push(Violation::new("project.time_order", "/updated_at"));
        }
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}

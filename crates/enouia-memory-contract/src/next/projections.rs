//! Disposable native projections. All counters describe a current authorized scope.
use super::*;
use crate::{error::Violation, hash::Sha256Hex, ids::*, json::Revision, time::Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectionHeader {
    pub selection: CatalogSelection,
    pub algorithm: String,
    pub version: Revision,
    pub parameters_hash: Sha256Hex,
    pub authorized_at: Timestamp,
    pub scope: String,
    pub matched_inputs: u64,
    pub included_inputs: u64,
    pub complete_in_scope: bool,
    pub limits: Vec<String>,
}
impl ProjectionHeader {
    pub fn valid(&self, algorithm: &str) -> bool {
        self.selection.valid()
            && matches!(
                self.selection.algorithm,
                CatalogReadAlgorithm::V2 | CatalogReadAlgorithm::V3 | CatalogReadAlgorithm::V4
            )
            && self.algorithm == algorithm
            && self.version.get() == 1
            && self.selection.evaluated_at <= self.authorized_at
            && self.scope == "authorized_current_machine_revisions"
            && self.included_inputs <= self.matched_inputs
            && self.matched_inputs <= MAX_SEGMENTED_REVISIONS as u64
            && self.limits.len() <= 16
            && self.limits.iter().all(|s| !s.is_empty() && s.len() <= 256)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BucketGranularity {
    Day,
    Month,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TimeBucketQuery {
    pub axis: CatalogTimeAxis,
    pub timezone: String,
    pub granularity: BucketGranularity,
    pub from_date: String,
    pub until_date: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryTimeBucket {
    pub local_start: String,
    pub from: Timestamp,
    pub until: Timestamp,
    pub possible_records: u64,
    pub staged: u64,
    pub adopted: u64,
    pub captured_material: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryTimeBuckets {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProjectionHeader,
    pub query: TimeBucketQuery,
    pub timezone_database: String,
    pub interval_rule: String,
    pub material_axis: CatalogTimeAxis,
    pub material_scope: String,
    pub authorized_material_in_scope: u64,
    pub unknown_time_in_scope: u64,
    pub buckets: Vec<MemoryTimeBucket>,
}
impl NextDocument for MemoryTimeBuckets {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "memory_time_buckets"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("iana-overlap-buckets-1")
            && self.header.complete_in_scope
            && self.material_axis == CatalogTimeAxis::SourceCaptured
            && self.material_scope == "authorized_current_sources_unfiltered"
            && self.authorized_material_in_scope <= MAX_SEGMENTED_REVISIONS as u64
            && self.interval_rule == "possible_overlap_half_open_not_additive"
            && !self.timezone_database.is_empty()
            && !self.query.timezone.is_empty()
            && !self.buckets.is_empty()
            && self.buckets.len() <= 366
            && self.unknown_time_in_scope <= self.header.matched_inputs
            && self.buckets.iter().all(|b| {
                b.from <= b.until
                    && b.staged + b.adopted <= b.possible_records
                    && b.possible_records <= self.header.matched_inputs
                    && b.captured_material <= self.authorized_material_in_scope
            })
            && self
                .buckets
                .windows(2)
                .all(|w| w[0].until == w[1].from && w[0].local_start < w[1].local_start);
        if valid {
            vec![]
        } else {
            vec![Violation::new("projection.time", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeMembership {
    pub memory: TargetRef,
    pub score_milli: u16,
    pub basis: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeFamily {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner_proofs: Vec<OwnerActionProof>,
    pub theme: TargetRef,
    pub name: String,
    pub member_count: u64,
    pub owner_named: bool,
    pub pinned: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ThemeCursor {
    pub header: ProjectionHeader,
    pub theme: MemoryId,
    pub after: MemoryId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryThemeMembers {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProjectionHeader,
    pub family: Option<ThemeFamily>,
    pub items: Vec<ThemeMembership>,
    pub next: Option<ThemeCursor>,
}
impl NextDocument for MemoryThemeMembers {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "memory_theme_members"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("recorded-shared-entity-families-1")
            && self.items.len() <= 100
            && self.family.as_ref().is_none_or(|f| {
                !f.name.trim().is_empty()
                    && f.name.chars().count() <= 60
                    && f.member_count <= self.header.matched_inputs
                    && f.owner_proofs.iter().all(OwnerActionProof::valid)
                    && (!f.owner_named || f.owner_proofs.iter().any(|p| p.action == "theme_rename"))
                    && (!f.pinned || f.owner_proofs.iter().any(|p| p.action == "theme_pin"))
            })
            && self.items.iter().all(|m| {
                m.score_milli <= 1000
                    && ["recorded_derivation", "shared_entity_suggestion"]
                        .contains(&m.basis.as_str())
            })
            && self
                .items
                .windows(2)
                .all(|w| w[0].memory.memory.memory_id < w[1].memory.memory.memory_id)
            && self.next.as_ref().is_none_or(|c| {
                c.header == self.header
                    && self
                        .items
                        .last()
                        .is_some_and(|m| m.memory.memory.memory_id == c.after)
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("projection.theme", "")]
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeKind {
    DerivedFrom,
    Reversion,
    SameConversation,
    SameEntity,
    ThemeCooccurrence,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GraphEdgeClass {
    RecordedRelation,
    AlgorithmSuggestion,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryGraphEdge {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner_relations: Vec<OwnerActionProof>,
    pub from: MemoryId,
    pub to: MemoryId,
    pub relation: GraphEdgeKind,
    pub class: GraphEdgeClass,
    pub witness: String,
    pub score_milli: Option<u16>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphEdgeCoverage {
    pub relation: GraphEdgeKind,
    pub candidates: u64,
    pub retained: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryGraphNode {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_marks: Option<OwnerMemoryMarks>,
    pub memory: TargetRef,
    pub title: String,
    pub form: FormationKind,
    pub knowledge: KnowledgeState,
    pub adoption: AdoptionState,
    pub validity: MemoryValidity,
    pub attention: AttentionState,
    pub event_time: RecordTime,
    pub entities: Vec<String>,
    pub themes: Vec<MemoryId>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub effective_sensitivity: Option<crate::common::Sensitivity>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphQuery {
    pub node_limit: usize,
    pub edge_limit_per_kind: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GraphCursor {
    pub header: ProjectionHeader,
    pub offset: usize,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryGraphPage {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProjectionHeader,
    pub nodes: Vec<MemoryGraphNode>,
    pub edges: Vec<MemoryGraphEdge>,
    pub edge_coverage: Vec<GraphEdgeCoverage>,
    pub semantic_neighbors_available: bool,
    pub next: Option<GraphCursor>,
}
impl NextDocument for MemoryGraphPage {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "memory_graph_page"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("typed-authorized-graph-1")
            && self.header.included_inputs <= 5000
            && self.nodes.len() <= 100
            && self.edges.len() <= 100
            && self.nodes.len() + self.edges.len() <= 100
            && self.nodes.iter().all(|n| {
                record_fields::text(&n.title, 60)
                    && n.owner_marks.as_ref().is_none_or(OwnerMemoryMarks::valid)
                    && (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                        || n.owner_marks.is_some())
                    && (!matches!(
                        self.header.selection.algorithm,
                        CatalogReadAlgorithm::V3 | CatalogReadAlgorithm::V4
                    ) || n.effective_sensitivity.is_some())
                    && n.event_time.valid()
                    && n.entities.len() <= 32
                    && n.entities
                        .iter()
                        .all(|e| crate::ids::check_prefixed_uuid(e, "ent").is_ok())
            })
            && self
                .nodes
                .iter()
                .map(|n| &n.memory.memory.memory_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.nodes.len()
            && self
                .edge_coverage
                .iter()
                .map(|c| c.relation)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.edge_coverage.len()
            && self.edge_coverage.len() <= 5
            && !self.semantic_neighbors_available
            && self
                .edge_coverage
                .iter()
                .all(|c| c.retained <= c.candidates && c.retained <= 20_000)
            && self.edges.iter().all(|e| {
                e.from != e.to
                    && e.owner_relations.len() <= 64
                    && e.owner_relations.iter().all(|p| {
                        p.valid() && ["entity_merge", "entity_split"].contains(&p.action.as_str())
                    })
                    && (e.owner_relations.is_empty() || e.relation == GraphEdgeKind::SameEntity)
                    && if e.relation == GraphEdgeKind::ThemeCooccurrence {
                        e.class == GraphEdgeClass::AlgorithmSuggestion
                            && e.score_milli.is_some_and(|s| s <= 1000)
                    } else {
                        e.class == GraphEdgeClass::RecordedRelation && e.score_milli.is_none()
                    }
            })
            && self.next.as_ref().is_none_or(|c| c.header == self.header);
        if valid {
            vec![]
        } else {
            vec![Violation::new("projection.graph", "")]
        }
    }
}

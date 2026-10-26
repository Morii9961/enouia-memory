//! ADR-MEM-94 transport-neutral scalable read results. Parsing is not authority.
use super::*;
use crate::{
    common::{ActorType, Sensitivity},
    error::Violation,
    hash::Sha256Hex,
    ids::*,
    time::Timestamp,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogCapability {
    #[serde(rename = "memory-native-read-2")]
    V2,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogTimeAxis {
    Event,
    Valid,
    Learned,
    SourceCaptured,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFilter {
    pub text: String,
    pub form: Option<FormationKind>,
    pub state: Option<FormationState>,
    pub knowledge: Option<KnowledgeState>,
    pub adoption: Option<AdoptionState>,
    pub validity: Option<MemoryValidity>,
    pub attention: Option<AttentionState>,
    pub entity: Option<String>,
    pub project: Option<ProjectId>,
    pub theme: Option<MemoryId>,
    pub time_axis: Option<CatalogTimeAxis>,
    pub time_window: Option<FormationTime>,
    pub include_unknown_time: bool,
}
impl CatalogFilter {
    pub fn valid(&self) -> bool {
        self.text.len() <= 2048
            && !self.text.chars().any(char::is_control)
            && self
                .entity
                .as_ref()
                .is_none_or(|v| crate::ids::check_prefixed_uuid(v, "ent").is_ok())
            && self.time_axis.is_some() == self.time_window.is_some()
            && self
                .time_window
                .as_ref()
                .is_none_or(|t| t.until.as_ref().is_none_or(|end| end > &t.from))
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogReadAlgorithm {
    #[serde(rename = "literal-alias-catalog-1")]
    V1,
    #[serde(rename = "literal-family-catalog-2")]
    V2,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogSelection {
    pub algorithm: CatalogReadAlgorithm,
    pub version: crate::json::Revision,
    pub profile_hash: Sha256Hex,
    pub withdrawal_watermark: crate::json::Revision,
    pub snapshot: SnapshotBinding,
    pub scope_hash: Sha256Hex,
    pub query_hash: Sha256Hex,
    pub evaluated_at: Timestamp,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogCursor {
    pub selection: CatalogSelection,
    pub after: MemoryId,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CatalogFacetAxis {
    Form,
    Knowledge,
    Adoption,
    Validity,
    Attention,
    State,
    Entity,
    Project,
    Theme,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FacetCountScope {
    OtherFilters,
    Matched,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFacetCursor {
    pub selection: CatalogSelection,
    pub axis: CatalogFacetAxis,
    pub count_scope: FacetCountScope,
    pub after: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFacetValue {
    pub value: String,
    pub label: String,
    pub count: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogFacetGroup {
    pub axis: CatalogFacetAxis,
    pub count_scope: FacetCountScope,
    pub values: Vec<CatalogFacetValue>,
    pub total_values: u64,
    pub next: Option<CatalogFacetCursor>,
}
impl CatalogFacetGroup {
    pub fn valid(&self, s: &CatalogSelection) -> bool {
        self.values.len() <= 100
            && self.total_values <= MAX_SEGMENTED_REVISIONS as u64
            && self.values.iter().all(|v| {
                !v.value.is_empty()
                    && v.value.len() <= 128
                    && !v.label.trim().is_empty()
                    && v.label.chars().count() <= 128
                    && v.count > 0
                    && v.count <= MAX_SEGMENTED_REVISIONS as u64
            })
            && self.values.windows(2).all(|w| w[0].value < w[1].value)
            && self.values.len() as u64 <= self.total_values
            && self.next.as_ref().is_none_or(|c| {
                &c.selection == s
                    && c.axis == self.axis
                    && c.count_scope == self.count_scope
                    && self.values.last().is_some_and(|v| v.value == c.after)
            })
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogEntity {
    pub entity_id: String,
    pub role: EntityRole,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogReadItem {
    pub base: MemoryReadItem,
    pub title: String,
    pub content_lang: String,
    pub event_time: RecordTime,
    pub valid_time: RecordTime,
    pub entities: Vec<CatalogEntity>,
}
impl CatalogReadItem {
    fn valid(&self, at: &Timestamp) -> bool {
        self.base.valid()
            && self.base.system_actor.actor_type == ActorType::System
            && self.base.receipt.is_some()
            && self.base.learned_at <= *at
            && self.base.validity == memory_validity(&self.base.dimensions, at)
            && !self.title.trim().is_empty()
            && self.title.chars().count() <= 60
            && !self.title.chars().any(char::is_control)
            && valid_content_lang(&self.content_lang)
            && self.event_time.matches(&self.base.dimensions.event_time)
            && self.valid_time.matches(&self.base.dimensions.valid_time)
            && self.entities.len() <= 32
            && self
                .entities
                .iter()
                .all(|e| crate::ids::check_prefixed_uuid(&e.entity_id, "ent").is_ok())
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogPageKind {
    #[serde(rename = "catalog_memory_page")]
    Page,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogMemoryPage {
    pub schema_version: NextVersion,
    pub kind: CatalogPageKind,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub selection: CatalogSelection,
    pub authorized_at: Timestamp,
    pub matched_in_scope: u64,
    pub unknown_time_in_scope: u64,
    pub count_exact: bool,
    pub scope: String,
    pub items: Vec<CatalogReadItem>,
    pub complete_in_scope: bool,
    pub next: Option<CatalogCursor>,
    pub facets: Vec<CatalogFacetGroup>,
}
impl NextDocument for CatalogMemoryPage {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.selection.valid()
            && self.effect == MemoryReadEffect::NativeRead
            && self.count_exact
            && self.scope == "authorized_current_machine_revisions"
            && self.selection.evaluated_at <= self.authorized_at
            && self.items.len() <= 100
            && self.matched_in_scope <= MAX_SEGMENTED_REVISIONS as u64
            && self.unknown_time_in_scope <= MAX_SEGMENTED_REVISIONS as u64
            && self.items.len() as u64 <= self.matched_in_scope
            && self
                .items
                .iter()
                .all(|v| v.valid(&self.selection.evaluated_at))
            && self
                .items
                .windows(2)
                .all(|w| w[0].base.memory.memory_id < w[1].base.memory.memory_id)
            && self.complete_in_scope == self.next.is_none()
            && self.next.as_ref().is_none_or(|c| {
                c.selection == self.selection
                    && self
                        .items
                        .last()
                        .is_some_and(|v| v.base.memory.memory_id == c.after)
            })
            && self.facets.len() <= 9
            && self.facets.iter().all(|f| f.valid(&self.selection));
        if valid {
            vec![]
        } else {
            vec![Violation::new("catalog.page", "")]
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogStatsKind {
    #[serde(rename = "catalog_memory_stats")]
    Stats,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogMemoryStats {
    pub schema_version: NextVersion,
    pub kind: CatalogStatsKind,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub selection: CatalogSelection,
    pub authorized_at: Timestamp,
    pub matched_in_scope: u64,
    pub count_exact: bool,
    pub scope: String,
    pub groups: Vec<CatalogFacetGroup>,
    pub theme_membership: String,
}
impl NextDocument for CatalogMemoryStats {
    fn validate(&self) -> Vec<Violation> {
        if !self.selection.valid()
            || self.effect != MemoryReadEffect::NativeRead
            || !self.count_exact
            || self.scope != "authorized_current_machine_revisions"
            || self.selection.evaluated_at > self.authorized_at
            || self.matched_in_scope > MAX_SEGMENTED_REVISIONS as u64
            || self.groups.len() > 9
            || self
                .groups
                .iter()
                .any(|g| g.count_scope != FacetCountScope::Matched || !g.valid(&self.selection))
            || self.theme_membership
                != if self.selection.algorithm == CatalogReadAlgorithm::V1 {
                    "recorded_derivation_only"
                } else {
                    "recorded_shared_entity_families"
                }
        {
            vec![Violation::new("catalog.stats", "")]
        } else {
            vec![]
        }
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CatalogFacetsKind {
    #[serde(rename = "catalog_memory_facets")]
    Facets,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CatalogMemoryFacets {
    pub schema_version: NextVersion,
    pub kind: CatalogFacetsKind,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub selection: CatalogSelection,
    pub authorized_at: Timestamp,
    pub group: CatalogFacetGroup,
}
impl NextDocument for CatalogMemoryFacets {
    fn validate(&self) -> Vec<Violation> {
        if self.selection.valid()
            && self.effect == MemoryReadEffect::NativeRead
            && self.selection.evaluated_at <= self.authorized_at
            && self.group.valid(&self.selection)
        {
            vec![]
        } else {
            vec![Violation::new("catalog.facets", "")]
        }
    }
}
/// Hash-only query binding; not a permission or authenticated cursor signature.
pub fn catalog_query_hash(
    filter: &CatalogFilter,
    profile: &AttentionProfile,
    max: Sensitivity,
) -> Sha256Hex {
    digest(&(CatalogReadAlgorithm::V2, 2u32, filter, profile, max))
}

impl CatalogSelection {
    pub fn valid(&self) -> bool {
        self.version.get()
            == match self.algorithm {
                CatalogReadAlgorithm::V1 => 1,
                CatalogReadAlgorithm::V2 => 2,
            }
    }
}

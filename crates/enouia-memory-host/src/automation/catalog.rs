//! Explicit native read-2; no default IPC, frontend, MCP or provider integration.
use super::application::{ApplicationFault, ApplicationResult};
use super::*;
use enouia_memory_contract::{common::Sensitivity, ids::*, next::*};
use enouia_memory_vault::automation::{
    AutomationFault,
    segmented::{AuthorizedSegmentedRead, SegmentedAutomationVault, SegmentedMemory},
};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock},
};

pub struct CatalogReadOptions {
    pub limit: usize,
    pub max_bytes: usize,
    pub max_sensitivity: Sensitivity,
    pub include_facets: bool,
}
pub struct CatalogReadRequest<'a> {
    pub profile: &'a AttentionProfile,
    pub filter: &'a CatalogFilter,
    pub options: CatalogReadOptions,
}
pub struct SegmentedMemoryReader {
    vault: Arc<SegmentedAutomationVault>,
    system: ActorRef,
    unlocked: RwLock<bool>,
}
const AXES: [CatalogFacetAxis; 9] = [
    CatalogFacetAxis::Form,
    CatalogFacetAxis::Knowledge,
    CatalogFacetAxis::Adoption,
    CatalogFacetAxis::Validity,
    CatalogFacetAxis::Attention,
    CatalogFacetAxis::State,
    CatalogFacetAxis::Entity,
    CatalogFacetAxis::Project,
    CatalogFacetAxis::Theme,
];
fn invalid() -> ApplicationFault {
    ApplicationFault::Invalid
}
fn bytes(v: &impl serde::Serialize) -> ApplicationResult<usize> {
    canonical_bytes(v).map(|b| b.len()).map_err(|_| invalid())
}
fn wire(v: &impl serde::Serialize) -> String {
    serde_json::to_value(v)
        .unwrap()
        .as_str()
        .unwrap()
        .to_owned()
}
struct Row<'a> {
    entry: &'a SegmentedMemory,
    captured: Timestamp,
    validity: MemoryValidity,
    attention: AttentionState,
    entities: Vec<&'a EntityRecord>,
    themes: Arc<BTreeMap<MemoryId, projections::ThemeSeed<'a>>>,
}
impl Row<'_> {
    fn id(&self) -> &MemoryId {
        &self.entry.memory.memory.memory_id
    }
    fn matches(
        &self,
        filter: &CatalogFilter,
        profile: &AttentionProfile,
        except: Option<CatalogFacetAxis>,
        without_time: bool,
    ) -> bool {
        let i = &self.entry.memory.decision.input;
        let d = i.dimensions.as_ref().unwrap();
        let text = filter
            .text
            .split_whitespace()
            .map(str::to_lowercase)
            .all(|term| {
                let mut cues = vec![term.clone()];
                for a in &profile.aliases {
                    if a.cue.to_lowercase() == term {
                        cues.extend(a.terms.iter().map(|s| s.to_lowercase()));
                    }
                }
                cues.iter().any(|cue| {
                    self.entry.search_text.contains(cue)
                        || self.entities.iter().any(|e| {
                            e.display_name.to_lowercase().contains(cue)
                                || e.aliases
                                    .iter()
                                    .any(|a| a.name.to_lowercase().contains(cue))
                        })
                })
            });
        text && (except == Some(CatalogFacetAxis::Form) || filter.form.is_none_or(|v| d.kind == v))
            && (except == Some(CatalogFacetAxis::Knowledge)
                || filter
                    .knowledge
                    .is_none_or(|v| self.entry.memory.decision.outcome.knowledge == v))
            && (except == Some(CatalogFacetAxis::Adoption)
                || filter
                    .adoption
                    .is_none_or(|v| self.entry.memory.decision.outcome.adoption == v))
            && (except == Some(CatalogFacetAxis::Validity)
                || filter.validity.is_none_or(|v| self.validity == v))
            && (except == Some(CatalogFacetAxis::Attention)
                || filter.attention.is_none_or(|v| self.attention == v))
            && (except == Some(CatalogFacetAxis::State)
                || filter.state.is_none_or(|v| d.state == Some(v)))
            && (except == Some(CatalogFacetAxis::Entity)
                || filter
                    .entity
                    .as_ref()
                    .is_none_or(|v| self.entities.iter().any(|e| &e.entity_id == v)))
            && (except == Some(CatalogFacetAxis::Project)
                || filter
                    .project
                    .as_ref()
                    .is_none_or(|v| i.project_id.as_ref() == Some(v)))
            && (except == Some(CatalogFacetAxis::Theme)
                || filter.theme.as_ref().is_none_or(|v| {
                    self.themes
                        .get(v)
                        .is_some_and(|seed| seed.score(self.entry).is_some())
                }))
            && (without_time || self.time_matches(filter))
    }
    fn range(&self, axis: CatalogTimeAxis) -> (Option<&Timestamp>, Option<&Timestamp>) {
        let f = self
            .entry
            .memory
            .decision
            .input
            .record_fields
            .as_ref()
            .unwrap();
        match axis {
            CatalogTimeAxis::Event => (f.event_time.from.as_ref(), f.event_time.until.as_ref()),
            CatalogTimeAxis::Valid => (f.valid_time.from.as_ref(), f.valid_time.until.as_ref()),
            CatalogTimeAxis::Learned => (Some(&self.entry.learned_at), None),
            CatalogTimeAxis::SourceCaptured => (Some(&self.captured), None),
        }
    }
    fn time_matches(&self, filter: &CatalogFilter) -> bool {
        let Some(window) = &filter.time_window else {
            return true;
        };
        let (from, until) = self.range(filter.time_axis.unwrap());
        let Some(start) = from else {
            return filter.include_unknown_time;
        };
        until.map_or(start >= &window.from, |end| end > &window.from)
            && window.until.as_ref().is_none_or(|end| start < end)
    }
    fn item(&self) -> CatalogReadItem {
        let m = &self.entry.memory;
        let i = &m.decision.input;
        let f = i.record_fields.as_ref().unwrap();
        CatalogReadItem {
            base: MemoryReadItem {
                memory: m.memory.clone(),
                system_actor: i.actor.clone(),
                subject_id: i.subject_id.clone(),
                project_id: i.project_id.clone(),
                content: i.content.clone(),
                knowledge: m.decision.outcome.knowledge,
                adoption: m.decision.outcome.adoption,
                dimensions: i.dimensions.clone().unwrap(),
                policy: i.policy.clone(),
                rule_version: m.decision.rule_version,
                committed_in: m.committed_in.clone(),
                learned_at: self.entry.learned_at.clone(),
                source_captured_at: self.captured.clone(),
                attention: self.attention,
                validity: self.validity,
                evidence: i.evidence.clone(),
                context_source_refs: i.context_source_refs.clone(),
                receipt: Some(self.entry.receipt.clone()),
            },
            title: f.title.clone(),
            content_lang: f.content_lang.clone(),
            event_time: f.event_time.clone(),
            valid_time: f.valid_time.clone(),
            effective_sensitivity: Some(effective(self.entry)),
            formation_reason: Some(reason(self.entry)),
            entities: f
                .entities
                .iter()
                .map(|e| CatalogEntity {
                    entity_id: e.entity_id.clone(),
                    role: e.role,
                })
                .collect(),
        }
    }
    fn theme_values(&self) -> impl Iterator<Item = (&MemoryId, &str)> {
        self.themes
            .iter()
            .filter(|(_, seed)| seed.score(self.entry).is_some())
            .map(|(id, seed)| {
                (
                    id,
                    seed.entry
                        .memory
                        .decision
                        .input
                        .record_fields
                        .as_ref()
                        .unwrap()
                        .title
                        .as_str(),
                )
            })
    }
    fn values(&self, axis: CatalogFacetAxis) -> Vec<(String, String)> {
        let i = &self.entry.memory.decision.input;
        let d = i.dimensions.as_ref().unwrap();
        let single = |v: String| vec![(v.clone(), v)];
        match axis {
            CatalogFacetAxis::Form => single(wire(&d.kind)),
            CatalogFacetAxis::Knowledge => {
                single(wire(&self.entry.memory.decision.outcome.knowledge))
            }
            CatalogFacetAxis::Adoption => {
                single(wire(&self.entry.memory.decision.outcome.adoption))
            }
            CatalogFacetAxis::Validity => single(wire(&self.validity)),
            CatalogFacetAxis::Attention => single(wire(&self.attention)),
            CatalogFacetAxis::State => single(
                d.state
                    .map_or_else(|| "not_applicable".into(), |s| wire(&s)),
            ),
            CatalogFacetAxis::Entity => self
                .entities
                .iter()
                .map(|e| (e.entity_id.clone(), e.display_name.clone()))
                .collect(),
            CatalogFacetAxis::Project => single(
                i.project_id
                    .as_ref()
                    .map_or_else(|| "none".into(), |p| p.to_string()),
            ),
            CatalogFacetAxis::Theme => self
                .theme_values()
                .map(|(id, title)| (id.to_string(), title.to_owned()))
                .collect(),
        }
    }
}
fn rows<'a>(
    data: &AuthorizedSegmentedRead<'a>,
    profile: &AttentionProfile,
    at: &Timestamp,
) -> ApplicationResult<Vec<Row<'a>>> {
    let themes = Arc::new(projections::seeds(data));
    data.memories
        .iter()
        .map(|entry| {
            let i = &entry.memory.decision.input;
            let captured = i
                .input_refs
                .iter()
                .filter_map(|r| {
                    data.sources
                        .get(&r.source_id)
                        .map(|s| s.record.captured_at.clone())
                })
                .max()
                .ok_or_else(invalid)?;
            let ids: BTreeSet<_> = i
                .record_fields
                .as_ref()
                .unwrap()
                .entities
                .iter()
                .map(|e| &e.entity_id)
                .collect();
            let entities = ids
                .into_iter()
                .map(|id| data.entities.get(id).copied().ok_or_else(invalid))
                .collect::<ApplicationResult<_>>()?;
            Ok(Row {
                entry,
                captured: captured.clone(),
                validity: memory_validity(i.dimensions.as_ref().unwrap(), at),
                attention: attention_state(profile, &entry.memory, &captured, at),
                entities,
                themes: themes.clone(),
            })
        })
        .collect()
}
fn selection(
    data: &AuthorizedSegmentedRead<'_>,
    request: &CatalogReadRequest<'_>,
    expected: Option<&CatalogSelection>,
) -> ApplicationResult<CatalogSelection> {
    let scope_hash = digest(&data.memories.iter().map(|m| &m.target).collect::<Vec<_>>());
    let selected = CatalogSelection {
        algorithm: CatalogReadAlgorithm::V3,
        version: enouia_memory_contract::json::Revision::new(3).unwrap(),
        profile_hash: digest(request.profile),
        withdrawal_watermark: data.withdrawal_watermark,
        snapshot: data.snapshot.clone(),
        scope_hash,
        query_hash: catalog_query_hash(
            request.filter,
            request.profile,
            request.options.max_sensitivity,
        ),
        evaluated_at: expected.map_or_else(|| data.now.clone(), |s| s.evaluated_at.clone()),
    };
    if selected.evaluated_at > data.now || expected.is_some_and(|s| s != &selected) {
        return Err(AutomationFault::Stale.into());
    }
    Ok(selected)
}
fn facet(
    rows: &[Row<'_>],
    request: &CatalogReadRequest<'_>,
    axis: CatalogFacetAxis,
    count_scope: FacetCountScope,
    selection: &CatalogSelection,
    after: Option<&str>,
    limit: usize,
) -> CatalogFacetGroup {
    let mut counts: BTreeMap<String, (String, u64)> = BTreeMap::new();
    let except = if count_scope == FacetCountScope::OtherFilters {
        Some(axis)
    } else {
        None
    };
    for row in rows
        .iter()
        .filter(|r| r.matches(request.filter, request.profile, except, false))
    {
        for (value, label) in row.values(axis) {
            let v = counts.entry(value).or_insert((label, 0));
            v.1 += 1;
        }
    }
    let total_values = counts.len() as u64;
    let remaining: Vec<_> = counts
        .into_iter()
        .filter(|(v, _)| after.is_none_or(|a| v.as_str() > a))
        .collect();
    let values: Vec<_> = remaining
        .iter()
        .take(limit)
        .map(|(value, (label, count))| CatalogFacetValue {
            value: value.clone(),
            label: label.clone(),
            count: *count,
        })
        .collect();
    let next = if values.len() < remaining.len() {
        values.last().map(|v| CatalogFacetCursor {
            selection: selection.clone(),
            axis,
            count_scope,
            after: v.value.clone(),
        })
    } else {
        None
    };
    CatalogFacetGroup {
        axis,
        count_scope,
        values,
        total_values,
        next,
    }
}
fn trim(group: &mut CatalogFacetGroup, selection: &CatalogSelection) -> bool {
    if group.values.len() <= 1 {
        return false;
    }
    group.values.pop();
    group.next = group.values.last().map(|v| CatalogFacetCursor {
        selection: selection.clone(),
        axis: group.axis,
        count_scope: group.count_scope,
        after: v.value.clone(),
    });
    true
}
impl SegmentedMemoryReader {
    /// The caller supplies independently authenticated native System identity.
    pub fn from_authenticated_system(
        vault: Arc<SegmentedAutomationVault>,
        system: ActorRef,
    ) -> ApplicationResult<Self> {
        if system != vault.descriptor().system_actor || system.actor_type != ActorType::System {
            return Err(invalid());
        }
        vault.snapshot()?;
        Ok(Self {
            vault,
            system,
            unlocked: RwLock::new(true),
        })
    }
    pub fn lock(&self) -> ApplicationResult<()> {
        *self
            .unlocked
            .write()
            .map_err(|_| ApplicationFault::Locked)? = false;
        Ok(())
    }
    pub fn unlock(&self, system: &ActorRef) -> ApplicationResult<()> {
        if *system != self.system {
            return Err(invalid());
        }
        self.vault.snapshot()?;
        *self
            .unlocked
            .write()
            .map_err(|_| ApplicationFault::Locked)? = true;
        Ok(())
    }
    fn read<T>(
        &self,
        request: &CatalogReadRequest<'_>,
        f: impl FnOnce(AuthorizedSegmentedRead<'_>) -> ApplicationResult<T>,
    ) -> ApplicationResult<T> {
        let gate = self.unlocked.read().map_err(|_| ApplicationFault::Locked)?;
        if !*gate {
            return Err(ApplicationFault::Locked);
        }
        if !request.filter.valid()
            || !request.profile.validate().is_empty()
            || !(1..=100).contains(&request.options.limit)
            || !(1024..=MAX_DOCUMENT_BYTES).contains(&request.options.max_bytes)
        {
            return Err(invalid());
        }
        self.vault
            .read(&self.system, request.options.max_sensitivity, f)?
    }
    pub fn memory_page_native(
        &self,
        request: &CatalogReadRequest<'_>,
        cursor: Option<&CatalogCursor>,
    ) -> ApplicationResult<CatalogMemoryPage> {
        self.read(request, |data| {
            let selection = selection(&data, request, cursor.map(|c| &c.selection))?;
            let rows = rows(&data, request.profile, &selection.evaluated_at)?;
            let matched: Vec<_> = rows
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .collect();
            let remaining: Vec<_> = matched
                .iter()
                .filter(|r| cursor.is_none_or(|c| r.id() > &c.after))
                .collect();
            let unknown_time_in_scope = rows
                .iter()
                .filter(|r| {
                    r.matches(request.filter, request.profile, None, true)
                        && r.range(request.filter.time_axis.unwrap_or(CatalogTimeAxis::Event))
                            .0
                            .is_none()
                })
                .count() as u64;
            let facets = if request.options.include_facets {
                projections::theme_budget(&data, data.memories.len())?;
                AXES.iter()
                    .map(|axis| {
                        facet(
                            &rows,
                            request,
                            *axis,
                            FacetCountScope::OtherFilters,
                            &selection,
                            None,
                            16,
                        )
                    })
                    .collect()
            } else {
                vec![]
            };
            let mut page = CatalogMemoryPage {
                schema_version: NextVersion,
                kind: CatalogPageKind::Page,
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                selection: selection.clone(),
                authorized_at: data.now,
                matched_in_scope: matched.len() as u64,
                unknown_time_in_scope,
                count_exact: true,
                scope: "authorized_current_machine_revisions".into(),
                items: vec![],
                complete_in_scope: true,
                next: None,
                facets,
            };
            let mut used = bytes(&page)?.saturating_add(1024);
            for row in remaining.iter().take(request.options.limit) {
                let item = row.item();
                let encoded = canonical_bytes(&item).map_err(|_| invalid())?;
                let cost = encoded.len() + 4 * encoded.iter().filter(|b| **b == b'\n').count() + 8;
                if used.saturating_add(cost) > request.options.max_bytes {
                    break;
                }
                used += cost;
                page.items.push(item);
            }
            page.complete_in_scope = page.items.len() == remaining.len();
            if !page.complete_in_scope {
                page.next = page.items.last().map(|i| CatalogCursor {
                    selection: selection.clone(),
                    after: i.base.memory.memory_id.clone(),
                });
                if page.next.is_none() {
                    return Err(invalid());
                }
            }
            if bytes(&page)? > request.options.max_bytes || !page.validate().is_empty() {
                return Err(invalid());
            }
            Ok(page)
        })
    }
    pub fn memory_stats_native(
        &self,
        request: &CatalogReadRequest<'_>,
        expected: Option<&CatalogSelection>,
    ) -> ApplicationResult<CatalogMemoryStats> {
        self.read(request, |data| {
            let selection = selection(&data, request, expected)?;
            let rows = rows(&data, request.profile, &selection.evaluated_at)?;
            let matched_in_scope = rows
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .count() as u64;
            projections::theme_budget(&data, data.memories.len())?;
            let groups = AXES
                .iter()
                .map(|axis| {
                    facet(
                        &rows,
                        request,
                        *axis,
                        FacetCountScope::Matched,
                        &selection,
                        None,
                        request.options.limit.min(16),
                    )
                })
                .collect();
            let mut stats = CatalogMemoryStats {
                schema_version: NextVersion,
                kind: CatalogStatsKind::Stats,
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                selection: selection.clone(),
                authorized_at: data.now,
                matched_in_scope,
                count_exact: true,
                scope: "authorized_current_machine_revisions".into(),
                groups,
                theme_membership: "recorded_shared_entity_families".into(),
            };
            while bytes(&stats)? > request.options.max_bytes {
                let Some(group) = stats
                    .groups
                    .iter_mut()
                    .filter(|g| g.values.len() > 1)
                    .max_by_key(|g| g.values.len())
                else {
                    return Err(invalid());
                };
                trim(group, &selection);
            }
            if !stats.validate().is_empty() {
                return Err(invalid());
            }
            Ok(stats)
        })
    }
    pub fn memory_facets_native(
        &self,
        request: &CatalogReadRequest<'_>,
        axis: CatalogFacetAxis,
        count_scope: FacetCountScope,
        cursor: Option<&CatalogFacetCursor>,
        expected: Option<&CatalogSelection>,
    ) -> ApplicationResult<CatalogMemoryFacets> {
        self.read(request, |data| {
            if cursor.is_some_and(|c| {
                c.axis != axis
                    || c.count_scope != count_scope
                    || expected.is_some_and(|e| e != &c.selection)
            }) {
                return Err(AutomationFault::Stale.into());
            }
            let selection = selection(&data, request, cursor.map(|c| &c.selection).or(expected))?;
            let rows = rows(&data, request.profile, &selection.evaluated_at)?;
            if axis == CatalogFacetAxis::Theme {
                projections::theme_budget(&data, data.memories.len())?;
            }
            let group = facet(
                &rows,
                request,
                axis,
                count_scope,
                &selection,
                cursor.map(|c| c.after.as_str()),
                request.options.limit,
            );
            let mut page = CatalogMemoryFacets {
                schema_version: NextVersion,
                kind: CatalogFacetsKind::Facets,
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                selection: selection.clone(),
                authorized_at: data.now,
                group,
            };
            while bytes(&page)? > request.options.max_bytes {
                if !trim(&mut page.group, &selection) {
                    return Err(invalid());
                }
            }
            if !page.validate().is_empty() {
                return Err(invalid());
            }
            Ok(page)
        })
    }
}

mod projections;

fn effective(entry: &SegmentedMemory) -> Sensitivity {
    // Publication/current-read validation proves this declared floor covers every
    // original source, entity/alias dependency and recursively current parent.
    // A policy or caller ceiling is a limit, never the record's classification.
    entry.memory.decision.input.sensitivity
}
fn reason(entry: &SegmentedMemory) -> MemoryFormationReason {
    let i = &entry.memory.decision.input;
    let f = i.record_fields.as_ref().unwrap();
    MemoryFormationReason {
        interpretation: i.interpretation,
        rule_version: entry.memory.decision.rule_version,
        prompt_version: f.formation.prompt_version.clone(),
        model_version: f.annotations.as_ref().and_then(|a| a.model_version.clone()),
        mode: f.formation.mode,
        import_batch_id: f.formation.import_batch_id.clone(),
        note: f
            .annotations
            .as_ref()
            .and_then(|a| a.formation_note.clone()),
        note_is_machine_annotation: true,
    }
}
mod process;

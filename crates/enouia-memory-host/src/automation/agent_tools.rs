//! MCP-2 native read operations over the same scoped V2 authorization.
use super::*;
use enouia_memory_contract::{
    hash::sha256,
    ids::{ProjectId, RequestId, SourceId},
    source::SourceRecord,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SearchTimeAxis {
    #[default]
    Event,
    Valid,
    Learned,
    SourceCaptured,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSearchFilter {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub form: Option<FormationKind>,
    #[serde(default)]
    pub adoption: Option<AdoptionState>,
    #[serde(default)]
    pub state: Option<FormationState>,
    #[serde(default)]
    pub project_id: Option<ProjectId>,
    #[serde(default)]
    pub time_axis: SearchTimeAxis,
    #[serde(default)]
    pub window: Option<FormationTime>,
    #[serde(default)]
    pub include_unknown_time: bool,
}
#[derive(Serialize)]
pub struct AgentSearchResult {
    pub page: AgentPage,
    pub time_axis: SearchTimeAxis,
    pub unknown_time_in_scope: u64,
}
#[derive(Serialize)]
pub struct AgentSource {
    schema_version: NextVersion,
    kind: &'static str,
    capability: &'static str,
    effect: &'static str,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub record: SourceRecord,
    pub attributed_subject: Option<enouia_memory_contract::ids::SubjectId>,
    pub excerpt: String,
    pub excerpt_hash: Sha256Hex,
    pub from_byte: usize,
    pub to_byte: usize,
    pub full_byte_len: usize,
    pub complete: bool,
    pub next_start: Option<usize>,
}
#[derive(Clone, Serialize)]
pub struct AgentRecall {
    schema_version: NextVersion,
    kind: &'static str,
    capability: &'static str,
    effect: &'static str,
    pub request_id: RequestId,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub trace: RecallTrace,
    pub rendered_context: String,
    pub items: Vec<MemoryReadItem>,
    pub complete_in_scope: bool,
}
#[derive(Serialize)]
pub struct AgentRelated {
    schema_version: NextVersion,
    kind: &'static str,
    capability: &'static str,
    effect: &'static str,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub memory: MemoryRevisionRef,
    pub provenance_edges: Vec<MemoryReadLink>,
    pub fact_edges: Vec<MemoryReadLink>,
    pub suggested_edges: Vec<MemoryReadLink>,
    pub unavailable: Vec<&'static str>,
    pub complete_in_scope: bool,
}
fn range(item: &MemoryReadItem, axis: SearchTimeAxis) -> Option<FormationTime> {
    match axis {
        SearchTimeAxis::Event => item.dimensions.event_time.clone(),
        SearchTimeAxis::Valid => item.dimensions.valid_time.clone(),
        SearchTimeAxis::Learned => Some(FormationTime {
            from: item.learned_at.clone(),
            until: None,
        }),
        SearchTimeAxis::SourceCaptured => Some(FormationTime {
            from: item.source_captured_at.clone(),
            until: None,
        }),
    }
}
fn overlaps(value: &FormationTime, window: &FormationTime) -> bool {
    value
        .until
        .as_ref()
        .map_or(value.from >= window.from, |end| end > &window.from)
        && window.until.as_ref().is_none_or(|end| &value.from < end)
}
impl AgentReader<'_> {
    pub fn source_read(&self) -> bool {
        self.permit.source_read()
    }
    pub fn search(
        &self,
        profile: &AttentionProfile,
        filter: &AgentSearchFilter,
        cursor: Option<&MemoryReadCursor>,
        limit: usize,
        max_bytes: usize,
    ) -> ApplicationResult<AgentSearchResult> {
        let _gate = self.host.gate()?;
        self.live()?;
        if filter.query.len() > 2048
            || filter
                .window
                .as_ref()
                .is_some_and(|w| w.until.as_ref().is_some_and(|end| end <= &w.from))
            || !(2048..=MAX_DOCUMENT_BYTES).contains(&max_bytes)
        {
            return Err(ApplicationFault::Invalid);
        }
        let terms: Vec<_> = filter
            .query
            .split_whitespace()
            .map(str::to_lowercase)
            .collect();
        if terms.len() > 32 || !profile.validate().is_empty() {
            return Err(ApplicationFault::Invalid);
        }
        let mut data = self
            .host
            .vault
            .authorized_agent_read_data(&self.permit, None)?;
        let mut selected = std::collections::BTreeSet::new();
        let mut unknown = 0;
        for memory in &data.current {
            let item = super::super::read_model::item(&data, memory, profile)?;
            let lower_content = item.content.to_lowercase();
            if filter.form.is_some_and(|f| item.dimensions.kind != f)
                || filter.adoption.is_some_and(|a| item.adoption != a)
                || filter
                    .state
                    .is_some_and(|s| item.dimensions.state != Some(s))
                || filter
                    .project_id
                    .as_ref()
                    .is_some_and(|p| item.project_id.as_ref() != Some(p))
                || !terms.iter().all(|t| lower_content.contains(t))
            {
                continue;
            }
            let time = range(&item, filter.time_axis);
            if time.is_none() {
                unknown += 1;
            }
            if filter.window.as_ref().is_some_and(|w| {
                time.as_ref()
                    .map_or(!filter.include_unknown_time, |t| !overlaps(t, w))
            }) {
                continue;
            }
            selected.insert(memory.memory.memory_id.clone());
        }
        data.current
            .retain(|m| selected.contains(&m.memory.memory_id));
        let page = project_page(
            data,
            profile,
            &MemoryReadFilter::default(),
            cursor,
            limit,
            max_bytes - 1024,
            PageScope {
                filter_hash: digest(&(
                    filter,
                    profile,
                    self.permit.max_sensitivity(),
                    self.scope_hash(),
                )),
                maintenance_worker_alive: false,
            },
        )?;
        let page = AgentPage {
            schema_version: NextVersion,
            kind: "agent_memory_page",
            capability: CAPABILITY,
            effect: "agent_read",
            principal: self.principal().clone(),
            scope_hash: self.scope_hash().clone(),
            snapshot: page.snapshot,
            authorized_at: page.authorized_at,
            profile_hash: page.profile_hash,
            items: page.items,
            matched_in_scope: page.matched_in_scope,
            unknown_event_time_in_scope: page.unknown_event_time_in_scope,
            complete_in_scope: page.complete_in_scope,
            next: page.next,
        };
        self.finish(
            AgentSearchResult {
                page,
                time_axis: filter.time_axis,
                unknown_time_in_scope: unknown,
            },
            max_bytes,
        )
    }
    pub fn source(
        &self,
        id: &SourceId,
        revision: Option<enouia_memory_contract::json::Revision>,
        from: usize,
        to: Option<usize>,
        max_bytes: usize,
    ) -> ApplicationResult<Option<AgentSource>> {
        let _gate = self.host.gate()?;
        self.live()?;
        if !(1..=8192).contains(&max_bytes) {
            return Err(ApplicationFault::Invalid);
        }
        let Some((snapshot, authorized_at, source)) =
            self.host.vault.agent_source(&self.permit, id)?
        else {
            self.live()?;
            return Ok(None);
        };
        if revision.is_some_and(|r| r != source.record.revision) {
            return Ok(None);
        }
        let text = &source.text;
        let full = text.len();
        let mut end = to.unwrap_or(full.min(from.saturating_add(max_bytes)));
        if from > full
            || end > full
            || end < from
            || !text.is_char_boundary(from)
            || (to.is_some() && !text.is_char_boundary(end))
            || end - from > max_bytes
        {
            return Err(ApplicationFault::Invalid);
        }
        if to.is_none() {
            while end > from && !text.is_char_boundary(end) {
                end -= 1;
            }
        }
        if end == from && from < full {
            return Err(ApplicationFault::Invalid);
        }
        let excerpt = text[from..end].to_string();
        let complete = from == 0 && end == full;
        self.finish(
            AgentSource {
                schema_version: NextVersion,
                kind: "agent_memory_source",
                capability: CAPABILITY,
                effect: "agent_read",
                principal: self.principal().clone(),
                scope_hash: self.scope_hash().clone(),
                snapshot,
                authorized_at,
                record: source.record,
                attributed_subject: source.attributed_subject,
                excerpt_hash: sha256(excerpt.as_bytes()),
                excerpt,
                from_byte: from,
                to_byte: end,
                full_byte_len: full,
                complete,
                next_start: (end < full).then_some(end),
            },
            MAX_DOCUMENT_BYTES,
        )
        .map(Some)
    }
    pub fn related(
        &self,
        profile: &AttentionProfile,
        id: &MemoryId,
        expected: Option<&SnapshotBinding>,
        max_bytes: usize,
    ) -> ApplicationResult<Option<AgentRelated>> {
        let Some(detail) = self.detail(profile, id, expected, false, max_bytes)? else {
            return Ok(None);
        };
        let _gate = self.host.gate()?;
        self.live()?;
        let fresh = self
            .host
            .vault
            .authorized_agent_read_data(&self.permit, None)?;
        if fresh.snapshot != detail.snapshot {
            return Err(AutomationFault::Stale.into());
        }
        self.finish(
            AgentRelated {
                schema_version: NextVersion,
                kind: "agent_memory_related",
                capability: CAPABILITY,
                effect: "agent_read",
                principal: self.principal().clone(),
                scope_hash: self.scope_hash().clone(),
                snapshot: detail.snapshot,
                memory: detail.item.memory,
                provenance_edges: detail.neighbors,
                fact_edges: vec![],
                suggested_edges: vec![],
                unavailable: vec!["fact_edges", "suggested_edges"],
                complete_in_scope: detail.complete_in_scope,
            },
            max_bytes,
        )
        .map(Some)
    }
    pub fn recall(
        &self,
        profile: &AttentionProfile,
        query: &RecallQuery,
        request_id: RequestId,
    ) -> ApplicationResult<AgentRecall> {
        let _gate = self.host.gate()?;
        self.live()?;
        let data = self
            .host
            .vault
            .authorized_agent_read_data(&self.permit, None)?;
        let rendered: Vec<_> = data
            .current
            .iter()
            .map(super::super::application::render_memory)
            .collect();
        let candidates: Vec<_> = data
            .current
            .iter()
            .zip(&rendered)
            .map(|(m, text)| {
                let time = m
                    .decision
                    .input
                    .input_refs
                    .iter()
                    .filter_map(|r| {
                        data.sources
                            .iter()
                            .find(|s| s.reference() == *r)
                            .map(|s| s.record.captured_at.clone())
                    })
                    .max()
                    .ok_or(ApplicationFault::Invalid)?;
                Ok(RecallCandidate {
                    memory: m,
                    source_time: time,
                    rendered_bytes: text.len(),
                })
            })
            .collect::<ApplicationResult<Vec<_>>>()?;
        let trace = select_recall(profile, query, &data.snapshot, &data.now, &candidates)
            .map_err(|_| ApplicationFault::Invalid)?;
        let mut text = Vec::new();
        let mut items = Vec::new();
        for selected in &trace.selected {
            let index = data
                .current
                .iter()
                .position(|m| m.memory == selected.memory)
                .ok_or(ApplicationFault::Invalid)?;
            text.push(rendered[index].as_str());
            items.push(super::super::read_model::item(
                &data,
                &data.current[index],
                profile,
            )?);
        }
        let result = self.finish(
            AgentRecall {
                schema_version: NextVersion,
                kind: "agent_memory_recall",
                capability: CAPABILITY,
                effect: "agent_read",
                request_id,
                principal: self.principal().clone(),
                scope_hash: self.scope_hash().clone(),
                rendered_context: text.join("\n"),
                items,
                complete_in_scope: !trace
                    .excluded
                    .iter()
                    .any(|(_, reason)| *reason == RecallExclusionReason::Budget),
                trace,
            },
            MAX_DOCUMENT_BYTES,
        )?;
        *self.context.lock().map_err(|_| ApplicationFault::Locked)? = Some(result.clone());
        Ok(result)
    }
    /// Context displays this actual computed trace, never a separately reranked
    /// simulation. It does not attest the client received/used the response.
    pub fn context_trace(&self, request: &RequestId) -> ApplicationResult<Option<AgentRecall>> {
        let _gate = self.host.gate()?;
        self.live()?;
        let data = self
            .host
            .vault
            .authorized_agent_read_data(&self.permit, None)?;
        let result = self
            .context
            .lock()
            .map_err(|_| ApplicationFault::Locked)?
            .clone();
        let Some(result) = result.filter(|r| &r.request_id == request) else {
            return Ok(None);
        };
        if result.trace.snapshot != data.snapshot {
            return Err(AutomationFault::Stale.into());
        }
        self.finish(result, MAX_DOCUMENT_BYTES).map(Some)
    }
}

//! Native Agent-only qualified reads. No System reader, MCP route or OS enrollment.
use super::{
    agent_read::AgentRecordProjection,
    application::{ApplicationFault, ApplicationResult},
    catalog::{
        CatalogReadOptions, CatalogReadRequest, project_page, qualified_items, qualified_relations,
    },
};
use enouia_memory_contract::{
    common::*, hash::Sha256Hex, ids::*, json::Revision, next::*, time::Timestamp,
};
use enouia_memory_vault::automation::{
    AutomationFault,
    segmented::{
        AuthorizedSegmentedAgentRead, QUALIFIED_AGENT_READ_CAPABILITY, SegmentedAgentPermit,
        SegmentedAgentScope, SegmentedAutomationVault, SegmentedMemory,
    },
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{
    Arc, Mutex, RwLock, RwLockReadGuard,
    atomic::{AtomicBool, Ordering},
};

pub const CAPABILITY: &str = QUALIFIED_AGENT_READ_CAPABILITY;
pub struct QualifiedAgentHost {
    vault: Arc<SegmentedAutomationVault>,
    owner: ActorRef,
    generation: RwLock<Option<Arc<AtomicBool>>>,
}
pub struct QualifiedAgentReader<'a> {
    host: &'a QualifiedAgentHost,
    permit: SegmentedAgentPermit,
    generation: Arc<AtomicBool>,
    context: Mutex<Option<Value>>,
}
/// Data only. Neither parsing nor copying a page grants native authority.
#[derive(Serialize)]
pub struct QualifiedAgentPage {
    pub schema_version: NextVersion,
    pub kind: &'static str,
    pub capability: &'static str,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub selection: CatalogSelection,
    pub authorized_at: Timestamp,
    pub matched_in_scope: u64,
    pub unknown_time_in_scope: u64,
    pub count_exact: bool,
    pub items: Vec<QualifiedAgentItem>,
    pub complete_in_scope: bool,
    pub next: Option<CatalogCursor>,
    pub facets: Vec<CatalogFacetGroup>,
}
#[derive(Serialize)]
pub struct QualifiedAgentItem {
    pub base: CatalogReadItem,
    pub record: AgentRecordProjection,
}
/// Data only; native current qualification is always required again.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct QualifiedHistoryCursor {
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub profile_hash: Sha256Hex,
    pub memory_id: MemoryId,
    pub before_revision: Revision,
}
impl QualifiedAgentHost {
    /// Caller has authenticated the native Owner; not an OS enrollment method.
    pub fn from_authenticated_owner(
        vault: Arc<SegmentedAutomationVault>,
        owner: ActorRef,
    ) -> ApplicationResult<Self> {
        if owner.actor_type != ActorType::Owner
            || owner != vault.descriptor().created_by
            || vault.descriptor().layout != AutomationLayout::SegmentedOwnerV1
        {
            return Err(AutomationFault::Denied.into());
        }
        vault.snapshot()?;
        Ok(Self {
            vault,
            owner,
            generation: RwLock::new(Some(Arc::new(AtomicBool::new(true)))),
        })
    }
    pub fn lock(&self) -> ApplicationResult<()> {
        if let Some(old) = self
            .generation
            .write()
            .map_err(|_| ApplicationFault::Locked)?
            .take()
        {
            old.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    pub fn unlock(&self, owner: &ActorRef) -> ApplicationResult<()> {
        if owner != &self.owner {
            return Err(AutomationFault::Denied.into());
        }
        self.host_live()?;
        let mut gate = self
            .generation
            .write()
            .map_err(|_| ApplicationFault::Locked)?;
        if let Some(old) = gate.replace(Arc::new(AtomicBool::new(true))) {
            old.store(false, Ordering::SeqCst);
        }
        Ok(())
    }
    fn host_live(&self) -> ApplicationResult<()> {
        self.vault.snapshot().map(|_| ()).map_err(Into::into)
    }
    pub fn reader(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> ApplicationResult<QualifiedAgentReader<'_>> {
        if owner != &self.owner {
            return Err(AutomationFault::Denied.into());
        }
        let gate = self
            .generation
            .read()
            .map_err(|_| ApplicationFault::Locked)?;
        let generation = gate.as_ref().ok_or(ApplicationFault::Locked)?.clone();
        let permit = self
            .vault
            .authorize_qualified_agent_read(owner, principal, scope)?;
        Ok(QualifiedAgentReader {
            host: self,
            permit,
            generation,
            context: Mutex::new(None),
        })
    }
}
impl QualifiedAgentReader<'_> {
    pub fn principal(&self) -> &ActorRef {
        self.permit.principal()
    }
    pub fn scope_hash(&self) -> &Sha256Hex {
        self.permit.scope_hash()
    }
    pub fn revoke(&self) {
        self.permit.revoke();
    }
    pub fn vault_id(&self) -> &VaultId {
        &self.host.vault.descriptor().vault_id
    }
    pub fn source_read(&self) -> bool {
        self.permit.source_read()
    }
    fn gate(&self) -> ApplicationResult<RwLockReadGuard<'_, Option<Arc<AtomicBool>>>> {
        let gate = self
            .host
            .generation
            .read()
            .map_err(|_| ApplicationFault::Locked)?;
        if gate.is_none()
            || !self.generation.load(Ordering::SeqCst)
            || self.permit.capability() != CAPABILITY
        {
            return Err(ApplicationFault::Locked);
        }
        self.host.vault.check_agent_read_live(&self.permit)?;
        Ok(gate)
    }
    fn read<T>(
        &self,
        selected: Option<&MemoryId>,
        f: impl FnOnce(AuthorizedSegmentedAgentRead<'_>) -> ApplicationResult<T>,
    ) -> ApplicationResult<T> {
        let _gate = self.gate()?;
        let result = self.host.vault.read_agent(&self.permit, selected, f)??;
        self.host.vault.check_agent_read_live(&self.permit)?;
        Ok(result)
    }
    pub(crate) fn check_connection(&self) -> ApplicationResult<()> {
        let _gate = self.gate()?;
        Ok(())
    }
    pub fn page(
        &self,
        profile: &AttentionProfile,
        filter: &CatalogFilter,
        cursor: Option<&CatalogCursor>,
        limit: usize,
        max_bytes: usize,
        include_facets: bool,
    ) -> ApplicationResult<QualifiedAgentPage> {
        if !filter.valid()
            || !profile.validate().is_empty()
            || !(1..=100).contains(&limit)
            || !(4096..=MAX_DOCUMENT_BYTES).contains(&max_bytes)
        {
            return Err(ApplicationFault::Invalid);
        }
        let request = CatalogReadRequest {
            profile,
            filter,
            options: CatalogReadOptions {
                limit,
                max_bytes,
                max_sensitivity: self.permit.max_sensitivity(),
                include_facets,
            },
        };
        self.read(None, |data| -> ApplicationResult<QualifiedAgentPage> {
            let page = project_page(&data.current, &request, cursor, Some(self.scope_hash()))?;
            let visible = current_refs(&data);
            let mut items = Vec::new();
            for base in page.items {
                let entry = data
                    .current
                    .memories
                    .iter()
                    .find(|m| m.memory.memory == base.base.memory)
                    .ok_or(ApplicationFault::Invalid)?;
                items.push(qualified_item(&data, entry, base, &visible)?);
            }
            let native_count = items.len();
            let mut result = QualifiedAgentPage {
                schema_version: NextVersion,
                kind: "qualified_agent_page",
                capability: CAPABILITY,
                principal: self.principal().clone(),
                scope_hash: self.scope_hash().clone(),
                selection: page.selection,
                authorized_at: page.authorized_at,
                matched_in_scope: page.matched_in_scope,
                unknown_time_in_scope: page.unknown_time_in_scope,
                count_exact: page.count_exact,
                items,
                complete_in_scope: page.complete_in_scope,
                next: page.next,
                facets: page.facets,
            };
            loop {
                result.complete_in_scope =
                    page.complete_in_scope && result.items.len() == native_count;
                result.next = if result.complete_in_scope {
                    None
                } else {
                    Some(CatalogCursor {
                        selection: result.selection.clone(),
                        after: result
                            .items
                            .last()
                            .ok_or(ApplicationFault::Vault(AutomationFault::Capacity))?
                            .base
                            .base
                            .memory
                            .memory_id
                            .clone(),
                    })
                };
                if enouia_memory_contract::json::canonical_bytes(&result)
                    .map_err(|_| ApplicationFault::Invalid)?
                    .len()
                    <= max_bytes
                {
                    return Ok(result);
                }
                result
                    .items
                    .pop()
                    .ok_or(ApplicationFault::Vault(AutomationFault::Capacity))?;
            }
        })
    }

    pub fn source(
        &self,
        id: &SourceId,
        revision: Option<Revision>,
        from: usize,
        to: Option<usize>,
        max_bytes: usize,
    ) -> ApplicationResult<Option<Value>> {
        if !(1..=8192).contains(&max_bytes) {
            return Err(ApplicationFault::Invalid);
        }
        self.read(None, |data| {
            if !self.source_read() {
                return Err(AutomationFault::Denied.into());
            }
            let Some(source) = data
                .current
                .sources
                .get(id)
                .filter(|s| revision.is_none_or(|r| r == s.record.revision))
            else {
                return Ok(None);
            };
            let text = &source.text;
            let mut end = to.unwrap_or(text.len().min(from.saturating_add(max_bytes)));
            if from > text.len()
                || end > text.len()
                || end < from
                || end - from > max_bytes
                || !text.is_char_boundary(from)
                || to.is_some() && !text.is_char_boundary(end)
            {
                return Err(ApplicationFault::Invalid);
            }
            if to.is_none() {
                while end > from && !text.is_char_boundary(end) {
                    end -= 1;
                }
            }
            if end == from && from < text.len() {
                return Err(ApplicationFault::Invalid);
            }
            let excerpt = &text[from..end];
            bounded(
                json!({"capability":CAPABILITY,"scope_hash":self.scope_hash(),
                "snapshot":data.current.snapshot,"authorized_at":data.current.now,
                "record":source.record,"record_fields":source.record_fields,
                "attributed_subject":source.attributed_subject,"excerpt":excerpt,
                "excerpt_hash":enouia_memory_contract::hash::sha256(excerpt.as_bytes()),
                "from_byte":from,"to_byte":end,"full_byte_len":text.len(),
                "complete":from == 0 && end == text.len(),
                "next_start":(end < text.len()).then_some(end)}),
                MAX_DOCUMENT_BYTES,
            )
            .map(Some)
        })
    }

    pub fn detail(
        &self,
        profile: &AttentionProfile,
        id: &MemoryId,
        expected: Option<&SnapshotBinding>,
        include_source_text: bool,
        max_bytes: usize,
    ) -> ApplicationResult<Option<Value>> {
        self.detail_page(profile, id, expected, include_source_text, max_bytes, None)
    }
    pub fn detail_page(
        &self,
        profile: &AttentionProfile,
        id: &MemoryId,
        expected: Option<&SnapshotBinding>,
        include_source_text: bool,
        max_bytes: usize,
        cursor: Option<&QualifiedHistoryCursor>,
    ) -> ApplicationResult<Option<Value>> {
        validate_read(profile, max_bytes)?;
        self.read(Some(id), |data| {
            if include_source_text && !self.source_read() { return Err(AutomationFault::Denied.into()); }
            check_snapshot(&data, expected)?;
            if cursor.is_some_and(|c| c.scope_hash != *self.scope_hash() || c.snapshot != data.current.snapshot
                || c.profile_hash != digest(profile) || &c.memory_id != id) { return Err(AutomationFault::Stale.into()); }
            let Some(entry) = data.current.memories.iter().find(|m| &m.memory.memory.memory_id == id) else { return Ok(None); };
            let current = current_refs(&data);
            let base = qualified_items(&data.current, profile, &data.current.now)?
                .into_iter().find(|b| &b.base.memory.memory_id == id).ok_or(ApplicationFault::Invalid)?;
            let selected = qualified_item(&data, entry, base, &current)?;
            let history_count = data.history.len();
            let mut revisions: Vec<_> = data.history.iter().copied()
                .filter(|m| cursor.is_none_or(|c| m.memory.memory.revision < c.before_revision)).collect();
            revisions.sort_by_key(|m| m.memory.memory.revision);
            let remaining_count = revisions.len();
            let mut selected_revisions = revisions.split_off(revisions.len().saturating_sub(32));
            let mut history = Vec::new();
            for old in &selected_revisions {
                let input = &old.memory.decision.input;
                history.push(json!({"memory":old.target,"content":input.content,
                    "knowledge":old.memory.decision.outcome.knowledge,
                    "adoption":old.memory.decision.outcome.adoption,
                    "dimensions":input.dimensions,"learned_at":old.learned_at,
                    "is_current":old.memory.memory == entry.memory.memory,
                    "receipt":NativeReadReceipt::from(&old.receipt),
                    "record":record(&data, old, &current, &[])?}));
            }
            let refs: BTreeSet<_> = data.history.iter().flat_map(|m| &m.memory.decision.input.input_refs).collect();
            let source_count = data.current.sources.values().filter(|s| self.source_read() && refs.contains(&s.reference())).count();
            let current_sources: BTreeSet<_> = entry.memory.decision.input.input_refs.iter().collect();
            let mut sources: Vec<_> = data.current.sources.values()
                .filter(|s| self.source_read() && current_sources.contains(&s.reference()))
                .chain(data.current.sources.values().filter(|s| self.source_read() && refs.contains(&s.reference()) && !current_sources.contains(&s.reference())))
                .take(32)
                .map(|s| json!({"record":s.record,"record_fields":s.record_fields,
                    "attributed_subject":s.attributed_subject,"text":include_source_text.then_some(&s.text)})).collect();
            let mut text_complete = true;
            loop {
                let history_complete = history.len() == remaining_count;
                let next_history = if history_complete { None } else {
                    Some(QualifiedHistoryCursor {scope_hash:self.scope_hash().clone(), snapshot:data.current.snapshot.clone(),
                        profile_hash:digest(profile), memory_id:id.clone(),
                        before_revision:selected_revisions.first().ok_or(ApplicationFault::Vault(AutomationFault::Capacity))?.memory.memory.revision})
                };
                let result = json!({"capability":CAPABILITY,"scope_hash":self.scope_hash(),
                    "snapshot":data.current.snapshot,"authorized_at":data.current.now,
                    "profile_hash":digest(profile),"item":selected,"history":history,
                    "history_count_in_scope":history_count,"history_complete":history_complete,"next_history":next_history,
                    "sources":sources,"source_read":self.source_read(),
                    "source_count_in_scope":source_count,"sources_complete":sources.len()==source_count,
                    "source_text_complete":text_complete,
                    "complete_in_scope":history_complete && sources.len()==source_count && text_complete});
                if encoded(&result)? <= max_bytes { return Ok(Some(result)); }
                if sources.iter().any(|s| !s["text"].is_null()) {
                    for s in &mut sources { s["text"] = Value::Null; }
                    text_complete = false;
                } else if !sources.is_empty() { sources.pop(); }
                else if history.len() > 1 { history.remove(0); selected_revisions.remove(0); }
                else { return Err(AutomationFault::Capacity.into()); }
            }
        })
    }

    pub fn related(
        &self,
        profile: &AttentionProfile,
        id: &MemoryId,
        expected: Option<&SnapshotBinding>,
        max_bytes: usize,
    ) -> ApplicationResult<Option<Value>> {
        validate_read(profile, max_bytes)?;
        self.read(Some(id), |data| {
            check_snapshot(&data, expected)?;
            let Some(mut relations) = qualified_relations(&data.current, profile, id, 128)? else { return Ok(None); };
            loop {
                for c in &mut relations.coverage {
                    c.retained = relations.edges.iter().filter(|e| e.relation == c.relation).count() as u64;
                }
                let recorded: Vec<_> = relations.edges.iter().filter(|e| e.class == GraphEdgeClass::RecordedRelation).collect();
                let suggested: Vec<_> = relations.edges.iter().filter(|e| e.class == GraphEdgeClass::AlgorithmSuggestion).collect();
                let result = json!({"capability":CAPABILITY,"scope_hash":self.scope_hash(),
                    "snapshot":data.current.snapshot,"authorized_at":data.current.now,
                    "algorithm":"owner-qualified-related-1","memory_id":id,
                    "recorded_edges":recorded,"suggested_edges":suggested,
                    "edge_coverage":relations.coverage,
                    "coverage_scope":"supported_authorized_current_relations",
                    "unavailable":["same_conversation","semantic_neighbors"],
                    "complete_in_scope":relations.coverage.iter().all(|c| c.candidates==c.retained)});
                if encoded(&result)? <= max_bytes { return Ok(Some(result)); }
                relations.edges.pop().ok_or(ApplicationFault::Vault(AutomationFault::Capacity))?;
            }
        })
    }

    pub fn recall(
        &self,
        profile: &AttentionProfile,
        query: &RecallQuery,
        request_id: RequestId,
    ) -> ApplicationResult<Value> {
        if !profile.validate().is_empty() || !query.valid(profile) {
            return Err(ApplicationFault::Invalid);
        }
        self.read(None, |data| {
            let current = current_refs(&data);
            let entries: BTreeMap<_, _> = data.current.memories.iter()
                .map(|m| (&m.memory.memory.memory_id, *m)).collect();
            let mut candidates = Vec::new();
            let mut attention = Vec::new();
            for base in qualified_items(&data.current, profile, &query.at)? {
                let entry = entries.get(&base.base.memory.memory_id).ok_or(ApplicationFault::Invalid)?;
                let state = base.base.attention;
                let source_time = base.base.source_captured_at.clone();
                let projection = qualified_item(&data, entry, base, &current)?;
                let rendered = format!("{}\nQualified record: {}", super::application::render_memory(&entry.memory),
                    serde_json::to_string(&projection).map_err(|_| ApplicationFault::Invalid)?);
                candidates.push(RecallCandidate { memory:&entry.memory, source_time, rendered_bytes:rendered.len() });
                attention.push(state);
            }
            let mut trace = select_qualified_catalog_recall(profile, query, &data.current.snapshot,
                &data.current.now, &candidates, &attention).map_err(|_| ApplicationFault::Invalid)?;
            let mut reasons: BTreeMap<String, u64> = BTreeMap::new();
            for (_, reason) in &trace.excluded {
                let key = serde_json::to_value(reason).map_err(|_| ApplicationFault::Invalid)?;
                *reasons.entry(key.as_str().ok_or(ApplicationFault::Invalid)?.to_owned()).or_default() += 1;
            }
            let excluded = trace.excluded.len();
            trace.excluded.truncate(128);
            // Rank the whole authorized set without retaining every full record
            // or rendered body. Only selected output survives this second pass.
            let selected_ids: BTreeSet<_> = trace.selected.iter().map(|s| &s.memory.memory_id).collect();
            let mut items = BTreeMap::new();
            let mut renders = BTreeMap::new();
            for base in qualified_items(&data.current, profile, &query.at)? {
                if !selected_ids.contains(&base.base.memory.memory_id) { continue; }
                let entry = entries.get(&base.base.memory.memory_id).ok_or(ApplicationFault::Invalid)?;
                let projection = qualified_item(&data, entry, base, &current)?;
                let rendered = format!("{}\nQualified record: {}", super::application::render_memory(&entry.memory),
                    serde_json::to_string(&projection).map_err(|_| ApplicationFault::Invalid)?);
                renders.insert(entry.memory.memory.memory_id.clone(), rendered);
                items.insert(entry.memory.memory.memory_id.clone(), projection);
            }
            let selected: Vec<_> = trace.selected.iter().map(|s| items.get(&s.memory.memory_id).ok_or(ApplicationFault::Invalid)).collect::<ApplicationResult<_>>()?;
            let texts: Vec<_> = trace.selected.iter().map(|s| renders.get(&s.memory.memory_id).map(String::as_str).ok_or(ApplicationFault::Invalid)).collect::<ApplicationResult<_>>()?;
            let result = bounded(json!({"capability":CAPABILITY,"scope_hash":self.scope_hash(),
                "request_id":request_id,"qualification":"owner-qualified-catalog-4",
                "trace":trace,"trace_coverage":{"excluded_in_scope":excluded,"excluded_returned":excluded.min(128),
                    "complete":excluded<=128,"reason_counts":reasons},
                "items":selected,"rendered_context":texts.join("\n"),
                "complete_in_scope":!reasons.contains_key("budget")}), MAX_DOCUMENT_BYTES)?;
            *self.context.lock().map_err(|_| ApplicationFault::Locked)? = Some(result.clone());
            Ok(result)
        })
    }

    /// Actual same Agent response; current scope/snapshot checks precede access.
    pub fn context_trace(&self, request: &RequestId) -> ApplicationResult<Option<Value>> {
        self.read(None, |data| {
            let result = self
                .context
                .lock()
                .map_err(|_| ApplicationFault::Locked)?
                .clone();
            let Some(result) = result.filter(|r| r["request_id"] == json!(request)) else {
                return Ok(None);
            };
            if result["trace"]["snapshot"] != json!(data.current.snapshot)
                || result["scope_hash"] != json!(self.scope_hash())
            {
                return Err(AutomationFault::Stale.into());
            }
            Ok(Some(result))
        })
    }
}

fn validate_read(profile: &AttentionProfile, max: usize) -> ApplicationResult<()> {
    if profile.validate().is_empty() && (4096..=MAX_DOCUMENT_BYTES).contains(&max) {
        Ok(())
    } else {
        Err(ApplicationFault::Invalid)
    }
}
fn encoded(value: &impl Serialize) -> ApplicationResult<usize> {
    enouia_memory_contract::json::canonical_bytes(value)
        .map(|b| b.len())
        .map_err(|_| ApplicationFault::Invalid)
}
fn bounded<T: Serialize>(value: T, max: usize) -> ApplicationResult<T> {
    if encoded(&value)? > max {
        Err(AutomationFault::Capacity.into())
    } else {
        Ok(value)
    }
}
fn check_snapshot(
    data: &AuthorizedSegmentedAgentRead<'_>,
    expected: Option<&SnapshotBinding>,
) -> ApplicationResult<()> {
    if expected.is_some_and(|s| s != &data.current.snapshot) {
        Err(AutomationFault::Stale.into())
    } else {
        Ok(())
    }
}
fn current_refs(data: &AuthorizedSegmentedAgentRead<'_>) -> BTreeSet<(MemoryId, Revision)> {
    data.current
        .memories
        .iter()
        .map(|m| (m.memory.memory.memory_id.clone(), m.memory.memory.revision))
        .collect()
}
fn record(
    data: &AuthorizedSegmentedAgentRead<'_>,
    entry: &SegmentedMemory,
    current: &BTreeSet<(MemoryId, Revision)>,
    extra: &[CatalogEntity],
) -> ApplicationResult<AgentRecordProjection> {
    let input = &entry.memory.decision.input;
    let mut fields = input
        .record_fields
        .clone()
        .ok_or(ApplicationFault::Invalid)?;
    if let Some(a) = &mut fields.annotations {
        a.suggested_relations
            .retain(|r| current.contains(&(r.target.memory_id.clone(), r.target.revision)));
    }
    let ids: BTreeSet<_> = fields
        .entities
        .iter()
        .map(|e| &e.entity_id)
        .chain(extra.iter().map(|e| &e.entity_id))
        .collect();
    let entities = ids
        .into_iter()
        .map(|id| {
            data.current
                .entities
                .get(id)
                .map(|e| (*e).clone())
                .ok_or(ApplicationFault::Invalid)
        })
        .collect::<ApplicationResult<_>>()?;
    Ok(AgentRecordProjection {
        memory: entry.memory.memory.clone(),
        producer_version: input.producer_version.clone(),
        model_version: input.model_version.clone(),
        annotations_scope: "authorized_current_targets",
        fields,
        entities,
    })
}
fn qualified_item(
    data: &AuthorizedSegmentedAgentRead<'_>,
    entry: &SegmentedMemory,
    base: CatalogReadItem,
    current: &BTreeSet<(MemoryId, Revision)>,
) -> ApplicationResult<QualifiedAgentItem> {
    let record = record(data, entry, current, &base.entities)?;
    Ok(QualifiedAgentItem { base, record })
}

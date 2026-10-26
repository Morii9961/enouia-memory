//! Independent Agent view of one already leased segmented Vault.
//! No System read port, writer creation or permission from wire parameters.
use super::{
    agent_read::AgentRecordProjection,
    application::{ApplicationFault, ApplicationResult},
    *,
};
use enouia_memory_contract::{common::*, ids::*, json::Revision};
use enouia_memory_vault::automation::{
    AutomationFault,
    segmented::{
        AuthorizedSegmentedAgentRead, SegmentedAgentPermit, SegmentedAgentScope,
        SegmentedAutomationVault, SegmentedMemory,
    },
};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, RwLock, RwLockReadGuard},
};

pub const CAPABILITY: &str = "memory-agent-catalog-read-1";
pub struct SegmentedAgentHost {
    vault: Arc<SegmentedAutomationVault>,
    owner: ActorRef,
    generation: RwLock<Option<Arc<AtomicBool>>>,
}
pub struct SegmentedAgentReader<'a> {
    host: &'a SegmentedAgentHost,
    permit: SegmentedAgentPermit,
    generation: Arc<AtomicBool>,
    context: Mutex<Option<Value>>,
}
impl SegmentedAgentHost {
    /// Trusted native embedding supplies independently authenticated Owner.
    /// Reuses the existing Arc; never opens another Vault or lifetime lease.
    pub fn from_authenticated_owner(
        vault: Arc<SegmentedAutomationVault>,
        owner: ActorRef,
    ) -> ApplicationResult<Self> {
        if owner.actor_type != ActorType::Owner || owner != vault.descriptor().created_by {
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
    pub fn unlock(&self, native_owner: &ActorRef) -> ApplicationResult<()> {
        if native_owner != &self.owner {
            return Err(AutomationFault::Denied.into());
        }
        self.vault.snapshot()?;
        let mut g = self
            .generation
            .write()
            .map_err(|_| ApplicationFault::Locked)?;
        if let Some(old) = g.take() {
            old.store(false, Ordering::SeqCst);
        }
        *g = Some(Arc::new(AtomicBool::new(true)));
        Ok(())
    }
    pub fn reader(
        &self,
        native_owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> ApplicationResult<SegmentedAgentReader<'_>> {
        let gate = self
            .generation
            .read()
            .map_err(|_| ApplicationFault::Locked)?;
        let generation = gate.as_ref().ok_or(ApplicationFault::Locked)?.clone();
        let permit = self
            .vault
            .authorize_agent_read(native_owner, principal, scope)?;
        Ok(SegmentedAgentReader {
            host: self,
            permit,
            generation,
            context: Mutex::new(None),
        })
    }
}
fn invalid() -> ApplicationFault {
    ApplicationFault::Invalid
}
fn encoded(v: &impl Serialize) -> ApplicationResult<usize> {
    canonical_bytes(v).map(|v| v.len()).map_err(|_| invalid())
}
#[derive(Clone, Serialize)]
pub struct AgentCatalogItem {
    pub base: MemoryReadItem,
    pub record: AgentRecordProjection,
}
fn item(
    data: &AuthorizedSegmentedAgentRead<'_>,
    entry: &SegmentedMemory,
    profile: &AttentionProfile,
    at: &Timestamp,
    current: &BTreeSet<(MemoryId, Revision)>,
) -> ApplicationResult<AgentCatalogItem> {
    let m = &entry.memory;
    let i = &m.decision.input;
    let mut fields = i.record_fields.clone().ok_or_else(invalid)?;
    if let Some(a) = &mut fields.annotations {
        a.suggested_relations
            .retain(|r| current.contains(&(r.target.memory_id.clone(), r.target.revision)));
    }
    let captured = i
        .input_refs
        .iter()
        .filter_map(|r| {
            data.current
                .sources
                .get(&r.source_id)
                .filter(|s| s.reference() == *r)
                .map(|s| s.record.captured_at.clone())
        })
        .max()
        .ok_or_else(invalid)?;
    let mut seen = BTreeSet::new();
    let entities = fields
        .entities
        .iter()
        .filter(|e| seen.insert(e.entity_id.clone()))
        .map(|e| {
            data.current
                .entities
                .get(&e.entity_id)
                .map(|v| (*v).clone())
                .ok_or_else(invalid)
        })
        .collect::<ApplicationResult<Vec<_>>>()?;
    Ok(AgentCatalogItem {
        base: MemoryReadItem {
            memory: m.memory.clone(),
            system_actor: i.actor.clone(),
            subject_id: i.subject_id.clone(),
            project_id: i.project_id.clone(),
            content: i.content.clone(),
            knowledge: m.decision.outcome.knowledge,
            adoption: m.decision.outcome.adoption,
            dimensions: i.dimensions.clone().ok_or_else(invalid)?,
            policy: i.policy.clone(),
            rule_version: m.decision.rule_version,
            committed_in: m.committed_in.clone(),
            learned_at: entry.learned_at.clone(),
            attention: attention_state(profile, m, &captured, at),
            source_captured_at: captured,
            validity: memory_validity(i.dimensions.as_ref().ok_or_else(invalid)?, at),
            evidence: i.evidence.clone(),
            context_source_refs: i.context_source_refs.clone(),
            receipt: Some(entry.receipt.clone()),
        },
        record: AgentRecordProjection {
            memory: m.memory.clone(),
            producer_version: i.producer_version.clone(),
            model_version: i.model_version.clone(),
            annotations_scope: "authorized_current_targets",
            fields,
            entities,
        },
    })
}
struct Row<'a> {
    entry: &'a SegmentedMemory,
    captured: Timestamp,
    attention: AttentionState,
    validity: MemoryValidity,
    entities: Vec<&'a EntityRecord>,
    themes: Vec<MemoryId>,
}
impl Row<'_> {
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
    fn matches(
        &self,
        filter: &CatalogFilter,
        profile: &AttentionProfile,
        without_time: bool,
    ) -> bool {
        let i = &self.entry.memory.decision.input;
        let d = i.dimensions.as_ref().unwrap();
        filter
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
            })
            && filter.form.is_none_or(|v| d.kind == v)
            && filter.state.is_none_or(|v| d.state == Some(v))
            && filter
                .knowledge
                .is_none_or(|v| self.entry.memory.decision.outcome.knowledge == v)
            && filter
                .adoption
                .is_none_or(|v| self.entry.memory.decision.outcome.adoption == v)
            && filter.validity.is_none_or(|v| self.validity == v)
            && filter.attention.is_none_or(|v| self.attention == v)
            && filter
                .entity
                .as_ref()
                .is_none_or(|v| self.entities.iter().any(|e| &e.entity_id == v))
            && filter
                .project
                .as_ref()
                .is_none_or(|v| i.project_id.as_ref() == Some(v))
            && filter
                .theme
                .as_ref()
                .is_none_or(|v| self.themes.contains(v))
            && (without_time
                || filter.time_window.as_ref().is_none_or(|w| {
                    let (from, until) = self.range(filter.time_axis.unwrap());
                    from.map_or(filter.include_unknown_time, |start| {
                        until.map_or(start >= &w.from, |end| end > &w.from)
                            && w.until.as_ref().is_none_or(|end| start < end)
                    })
                }))
    }
}
fn rows<'a>(
    data: &AuthorizedSegmentedAgentRead<'a>,
    profile: &AttentionProfile,
    at: &Timestamp,
) -> ApplicationResult<Vec<Row<'a>>> {
    let mut themes: BTreeMap<MemoryId, Vec<MemoryId>> = BTreeMap::new();
    for m in &data.current.memories {
        let d = m.memory.decision.input.dimensions.as_ref().unwrap();
        if d.kind == FormationKind::Theme {
            for target in &d.derived_from {
                themes
                    .entry(target.memory.memory_id.clone())
                    .or_default()
                    .push(m.memory.memory.memory_id.clone());
            }
        }
    }
    data.current
        .memories
        .iter()
        .map(|m| {
            let i = &m.memory.decision.input;
            let captured = i
                .input_refs
                .iter()
                .filter_map(|r| {
                    data.current
                        .sources
                        .get(&r.source_id)
                        .map(|s| s.record.captured_at.clone())
                })
                .max()
                .ok_or_else(invalid)?;
            let entities = i
                .record_fields
                .as_ref()
                .ok_or_else(invalid)?
                .entities
                .iter()
                .map(|e| {
                    data.current
                        .entities
                        .get(&e.entity_id)
                        .copied()
                        .ok_or_else(invalid)
                })
                .collect::<ApplicationResult<Vec<_>>>()?;
            Ok(Row {
                entry: m,
                captured: captured.clone(),
                attention: attention_state(profile, &m.memory, &captured, at),
                validity: memory_validity(i.dimensions.as_ref().unwrap(), at),
                entities,
                themes: themes
                    .get(&m.memory.memory.memory_id)
                    .cloned()
                    .unwrap_or_default(),
            })
        })
        .collect()
}
impl SegmentedAgentReader<'_> {
    pub fn principal(&self) -> &ActorRef {
        self.permit.principal()
    }
    pub fn scope_hash(&self) -> &Sha256Hex {
        self.permit.scope_hash()
    }
    pub fn vault_id(&self) -> &VaultId {
        &self.host.vault.descriptor().vault_id
    }
    pub fn source_read(&self) -> bool {
        self.permit.source_read()
    }
    pub fn revoke(&self) {
        self.permit.revoke();
    }
    fn gate(&self) -> ApplicationResult<RwLockReadGuard<'_, Option<Arc<AtomicBool>>>> {
        let gate = self
            .host
            .generation
            .read()
            .map_err(|_| ApplicationFault::Locked)?;
        if gate.is_none() || !self.generation.load(Ordering::SeqCst) {
            return Err(ApplicationFault::Locked);
        }
        self.host.vault.check_agent_read_live(&self.permit)?;
        Ok(gate)
    }
    pub(crate) fn check_connection(&self) -> ApplicationResult<()> {
        let _gate = self.gate()?;
        Ok(())
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
    fn finish<T: Serialize>(&self, value: T, max: usize) -> ApplicationResult<T> {
        if encoded(&value)? > max {
            return Err(invalid());
        }
        Ok(value)
    }
    pub fn search(
        &self,
        profile: &AttentionProfile,
        filter: &CatalogFilter,
        cursor: Option<&CatalogCursor>,
        limit: usize,
        max_bytes: usize,
    ) -> ApplicationResult<Value> {
        if !filter.valid()
            || !profile.validate().is_empty()
            || !(1..=100).contains(&limit)
            || !(4096..=MAX_DOCUMENT_BYTES).contains(&max_bytes)
        {
            return Err(invalid());
        }
        self.read(None,|data| {
            let at=cursor.map_or_else(||data.current.now.clone(),|c|c.selection.evaluated_at.clone());
            let selection=CatalogSelection { algorithm:CatalogReadAlgorithm::V1,version:Revision::new(1).unwrap(),profile_hash:digest(profile),withdrawal_watermark:data.current.withdrawal_watermark,snapshot:data.current.snapshot.clone(),scope_hash:self.scope_hash().clone(),query_hash:digest(&(filter,profile,self.scope_hash())),evaluated_at:at.clone() };
            if at>data.current.now || cursor.is_some_and(|c|c.selection!=selection) { return Err(AutomationFault::Stale.into()); }
            let rows=rows(&data,profile,&at)?;
            let unknown=rows.iter().filter(|r|r.matches(filter,profile,true) && r.range(filter.time_axis.unwrap_or(CatalogTimeAxis::Event)).0.is_none()).count();
            let matches:Vec<_>=rows.iter().filter(|r|r.matches(filter,profile,false)).collect();
            let remaining:Vec<_>=matches.iter().filter(|r|cursor.is_none_or(|c|r.entry.memory.memory.memory_id>c.after)).collect();
            let current=data.current.memories.iter().map(|m|(m.memory.memory.memory_id.clone(),m.memory.memory.revision)).collect();
            let mut items:Vec<_>=remaining.iter().take(limit).map(|r|item(&data,r.entry,profile,&at,&current)).collect::<ApplicationResult<_>>()?;
            loop {
                let complete=items.len()==remaining.len();
                let next=if complete { None } else { Some(CatalogCursor { selection:selection.clone(),after:items.last().ok_or_else(invalid)?.base.memory.memory_id.clone() }) };
                let result=json!({"selection":selection,"authorized_at":data.current.now,"matched_in_scope":matches.len(),"unknown_time_in_scope":unknown,"count_exact":true,"items":items,"complete_in_scope":complete,"next":next});
                if encoded(&result)?<=max_bytes { return Ok(result); }
                items.pop().ok_or_else(invalid)?;
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
            return Err(invalid());
        }
        self.read(None,|data| {
            if !self.source_read() { return Err(AutomationFault::Denied.into()); }
            let Some(s)=data.current.sources.get(id).filter(|s|revision.is_none_or(|r|r==s.record.revision)) else { return Ok(None); };
            let text=&s.text; let full=text.len(); let mut end=to.unwrap_or(full.min(from.saturating_add(max_bytes)));
            if from>full || end>full || end<from || !text.is_char_boundary(from) || (to.is_some() && !text.is_char_boundary(end)) || end-from>max_bytes { return Err(invalid()); }
            if to.is_none() { while end>from && !text.is_char_boundary(end) { end-=1; } }
            if end==from && from<full { return Err(invalid()); }
            let excerpt=&text[from..end];
            let result=json!({"snapshot":data.current.snapshot,"authorized_at":data.current.now,"record":s.record,"record_fields":s.record_fields,"attributed_subject":s.attributed_subject,"excerpt":excerpt,"excerpt_hash":enouia_memory_contract::hash::sha256(excerpt.as_bytes()),"from_byte":from,"to_byte":end,"full_byte_len":full,"complete":from==0 && end==full,"next_start":(end<full).then_some(end)});
            self.finish(result,MAX_DOCUMENT_BYTES).map(Some)
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
        if !profile.validate().is_empty() || !(4096..=MAX_DOCUMENT_BYTES).contains(&max_bytes) {
            return Err(invalid());
        }
        self.read(Some(id),|data| {
            if include_source_text && !self.source_read() { return Err(AutomationFault::Denied.into()); }
            if expected.is_some_and(|s|s!=&data.current.snapshot) { return Err(AutomationFault::Stale.into()); }
            let Some(entry)=data.current.memories.iter().find(|m|&m.memory.memory.memory_id==id) else { return Ok(None); };
            let current=data.current.memories.iter().map(|m|(m.memory.memory.memory_id.clone(),m.memory.memory.revision)).collect();
            let selected=item(&data,entry,profile,&data.current.now,&current)?;
            let mut history:Vec<_>=data.history.iter().map(|m|item(&data,m,profile,&data.current.now,&current)).collect::<ApplicationResult<_>>()?;
            let refs:BTreeSet<_>=data.history.iter().flat_map(|m|m.memory.decision.input.input_refs.iter().cloned()).collect();
            let mut sources:Vec<Value>=data.current.sources.values().filter(|s|self.source_read() && refs.contains(&s.reference())).map(|s|json!({"record":s.record,"record_fields":s.record_fields,"attributed_subject":s.attributed_subject,"text":include_source_text.then_some(&s.text)})).collect();
            let (neighbors, links_complete)=links(&data,id);
            let mut complete=links_complete;
            loop {
                let result=json!({"snapshot":data.current.snapshot,"authorized_at":data.current.now,"profile_hash":digest(profile),"item":selected,"history":history,"sources":sources,"source_read":self.source_read(),"neighbors":neighbors,"complete_in_scope":complete});
                if encoded(&result)?<=max_bytes { return Ok(Some(result)); }
                complete=false;
                if sources.iter().any(|s|!s["text"].is_null()) { for s in &mut sources { s["text"]=Value::Null; } }
                else if !history.is_empty() { history.remove(0); }
                else { return Err(invalid()); }
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
        if !profile.validate().is_empty() || !(4096..=MAX_DOCUMENT_BYTES).contains(&max_bytes) {
            return Err(invalid());
        }
        self.read(Some(id),|data| {
            if expected.is_some_and(|s|s!=&data.current.snapshot) { return Err(AutomationFault::Stale.into()); }
            let Some(m)=data.current.memories.iter().find(|m|&m.memory.memory.memory_id==id) else { return Ok(None); };
            let (edges,complete)=links(&data,id);
            self.finish(json!({"snapshot":data.current.snapshot,"memory":m.memory.memory,"provenance_edges":edges,"fact_edges":[],"suggested_edges":[],"unavailable":["fact_edges","suggested_edges"],"complete_in_scope":complete}),max_bytes).map(Some)
        })
    }
    pub fn recall(
        &self,
        profile: &AttentionProfile,
        query: &RecallQuery,
        request_id: RequestId,
    ) -> ApplicationResult<Value> {
        if !profile.validate().is_empty() || !query.valid(profile) {
            return Err(invalid());
        }
        self.read(None,|data| {
            let current=data.current.memories.iter().map(|m|(m.memory.memory.memory_id.clone(),m.memory.memory.revision)).collect();
            let mut candidates=Vec::with_capacity(data.current.memories.len());
            for m in &data.current.memories {
                let projection=item(&data,m,profile,&data.current.now,&current)?;
                let rendered=render(m,&projection.record)?;
                candidates.push(RecallCandidate { memory:&m.memory,source_time:projection.base.source_captured_at,rendered_bytes:rendered.len() });
            }
            let mut trace=select_catalog_recall(profile,query,&data.current.snapshot,&data.current.now,&candidates).map_err(|_|invalid())?;
            let mut reasons:BTreeMap<String,u64>=BTreeMap::new();
            for (_,reason) in &trace.excluded { let name=serde_json::to_value(reason).map_err(|_|invalid())?.as_str().ok_or_else(invalid)?.to_owned(); *reasons.entry(name).or_default()+=1; }
            let excluded=trace.excluded.len();
            trace.excluded.truncate(128);
            let by_id:BTreeMap<_,_>=data.current.memories.iter().map(|m|(&m.memory.memory.memory_id,*m)).collect();
            let mut items=Vec::new();let mut texts=Vec::new();
            for s in &trace.selected {
                let entry=by_id.get(&s.memory.memory_id).ok_or_else(invalid)?;
                let projection=item(&data,entry,profile,&data.current.now,&current)?;
                texts.push(render(entry,&projection.record)?);items.push(projection);
            }
            let result=self.finish(json!({"request_id":request_id,"scope_hash":self.scope_hash(),"trace":trace,"trace_coverage":{"excluded_in_scope":excluded,"excluded_returned":excluded.min(128),"complete":excluded<=128,"reason_counts":reasons},"rendered_context":texts.join("\n"),"items":items,"complete_in_scope":!reasons.contains_key("budget")}),MAX_DOCUMENT_BYTES)?;
            *self.context.lock().map_err(|_|ApplicationFault::Locked)?=Some(result.clone());
            Ok(result)
        })
    }
    /// Same actual response/trace as MCP, with current withdrawal checks.
    pub fn context_trace(&self, request: &RequestId) -> ApplicationResult<Option<Value>> {
        self.read(None, |data| {
            let result = self
                .context
                .lock()
                .map_err(|_| ApplicationFault::Locked)?
                .clone();
            let Some(result) = result.filter(|v| v["request_id"] == json!(request)) else {
                return Ok(None);
            };
            if result["trace"]["snapshot"] != json!(data.current.snapshot) {
                return Err(AutomationFault::Stale.into());
            }
            Ok(Some(result))
        })
    }
}
fn render(entry: &SegmentedMemory, record: &AgentRecordProjection) -> ApplicationResult<String> {
    Ok(format!(
        "{}\nRecord fields: {}",
        super::application::render_memory(&entry.memory),
        serde_json::to_string(record).map_err(|_| invalid())?
    ))
}
fn links(data: &AuthorizedSegmentedAgentRead<'_>, id: &MemoryId) -> (Vec<MemoryReadLink>, bool) {
    let peers: BTreeSet<_> = data
        .current
        .memories
        .iter()
        .chain(&data.history)
        .map(|m| {
            (
                m.memory.memory.memory_id.clone(),
                m.memory.memory.revision,
                digest(&m.target),
            )
        })
        .collect();
    let mut edges = Vec::new();
    for m in &data.current.memories {
        let d = m.memory.decision.input.dimensions.as_ref().unwrap();
        for (t, kind) in d
            .derived_from
            .iter()
            .map(|t| (t, MemoryLinkKind::DerivedFrom))
            .chain(d.reverts_to.iter().map(|t| (t, MemoryLinkKind::Reversion)))
        {
            if (&m.memory.memory.memory_id == id || &t.memory.memory_id == id)
                && peers.contains(&(t.memory.memory_id.clone(), t.memory.revision, digest(t)))
            {
                edges.push(MemoryReadLink {
                    from: m.memory.memory.clone(),
                    to: t.clone(),
                    kind,
                });
            }
        }
    }
    let complete = edges.len() <= 32;
    edges.truncate(32);
    (edges, complete)
}

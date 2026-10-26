use super::*;
use enouia_memory_contract::{common::SourceRevisionRef, json::Revision};
use enouia_memory_vault::automation::segmented::{AuthorizedProcessRead, PublicationStamp};
fn process_header(
    data: &AuthorizedProcessRead<'_>,
    request: &CatalogReadRequest<'_>,
    algorithm: &str,
    parameters: &impl serde::Serialize,
    expected: Option<&ProcessHeader>,
) -> ApplicationResult<ProcessHeader> {
    let selection = selection(&data.current, request, expected.map(|h| &h.selection))?;
    let h = ProcessHeader {
        selection,
        algorithm: algorithm.into(),
        version: Revision::new(1).unwrap(),
        parameters_hash: digest(parameters),
        permission_hash: data.permission_hash.clone(),
        authorized_at: data.current.now.clone(),
        scope: "authorized_current_dependencies".into(),
    };
    if expected.is_some_and(|old| {
        let mut old = old.clone();
        old.authorized_at = h.authorized_at.clone();
        old != h
    }) {
        return Err(AutomationFault::Stale.into());
    }
    Ok(h)
}
fn finish<T: NextDocument + serde::Serialize>(value: T, max: usize) -> ApplicationResult<T> {
    if !value.validate().is_empty() || bytes(&value)? > max {
        Err(invalid())
    } else {
        Ok(value)
    }
}
fn cost(value: &impl serde::Serialize) -> ApplicationResult<usize> {
    let b = canonical_bytes(value).map_err(|_| invalid())?;
    Ok(b.len() + 8 * b.iter().filter(|v| **v == b'\n').count() + 16)
}
fn proof(stamp: &PublicationStamp) -> NativePublicationProof {
    NativePublicationProof {
        commit_id: stamp.commit_id.clone(),
        commit_hash: stamp.commit_hash.clone(),
        actor: stamp.actor.clone(),
        written_at: stamp.at.clone(),
    }
}
fn summary(entry: &SegmentedMemory) -> NativeRevisionSummary {
    let i = &entry.memory.decision.input;
    NativeRevisionSummary {
        memory: entry.target.clone(),
        title: i.record_fields.as_ref().unwrap().title.clone(),
        knowledge: entry.memory.decision.outcome.knowledge,
        adoption: entry.memory.decision.outcome.adoption,
        interpretation: i.interpretation,
        change: i.dimensions.as_ref().unwrap().change,
        learned_at: entry.learned_at.clone(),
        effective_sensitivity: effective(entry),
        receipt: entry.receipt.clone(),
    }
}
fn activity_kind(entry: &SegmentedMemory) -> NativeActivityKind {
    match entry
        .memory
        .decision
        .input
        .dimensions
        .as_ref()
        .unwrap()
        .change
    {
        FormationChange::Initial => NativeActivityKind::Formation,
        FormationChange::Synthesis => NativeActivityKind::Synthesis,
        FormationChange::Correction => NativeActivityKind::Correction,
        FormationChange::Reversion => NativeActivityKind::Reversion,
        FormationChange::Update => NativeActivityKind::Reevaluation,
        _ => NativeActivityKind::StateChange,
    }
}
impl SegmentedMemoryReader {
    fn process<T>(
        &self,
        request: &CatalogReadRequest<'_>,
        after: Option<&SnapshotBinding>,
        history: bool,
        f: impl FnOnce(AuthorizedProcessRead<'_>) -> ApplicationResult<T>,
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
        self.vault.read_process(
            &self.system,
            request.options.max_sensitivity,
            after,
            history,
            f,
        )?
    }
    pub fn memory_activity_feed_native(
        &self,
        request: &CatalogReadRequest<'_>,
        query: &ActivityQuery,
        cursor: Option<&ActivityCursor>,
    ) -> ApplicationResult<MemoryActivityPage> {
        self.process(request, None, true, |data| {
            if query.include_sources
                && (request.filter != &CatalogFilter::default() || query.memory.is_some())
            {
                return Err(invalid());
            }
            let h = process_header(
                &data,
                request,
                "witnessed-native-activity-1",
                query,
                cursor.map(|c| &c.header),
            )?;
            let all = rows(&data.current, request.profile, &h.selection.evaluated_at)?;
            let selected: BTreeSet<_> = all
                .iter()
                .filter(|r| {
                    r.matches(request.filter, request.profile, None, false)
                        && query.memory.as_ref().is_none_or(|id| r.id() == id)
                })
                .map(|r| r.id())
                .collect();
            let mut events = Vec::new();
            if query.include_sources
                && query
                    .event_kind
                    .is_none_or(|k| k == NativeActivityKind::MaterialCaptured)
            {
                for e in &data.captures {
                    events.push(MemoryActivityEvent {
                        event_id: digest(&(
                            "source-captured",
                            &e.stamp.commit_id,
                            e.source.reference(),
                        )),
                        event_kind: NativeActivityKind::MaterialCaptured,
                        memory: None,
                        source: Some(e.source.reference()),
                        title: None,
                        change: None,
                        knowledge: None,
                        adoption: None,
                        effective_sensitivity: e.source.record.sensitivity,
                        formation_reason: None,
                        proof: proof(e.stamp),
                        receipt: None,
                    });
                }
            }
            for event in &data.machine_events {
                let e = event.entry;
                let i = &e.memory.decision.input;
                let kind = activity_kind(e);
                if selected.contains(&e.memory.memory.memory_id)
                    && query.event_kind.is_none_or(|k| k == kind)
                {
                    events.push(MemoryActivityEvent {
                        event_id: digest(&(
                            "machine-publication",
                            &event.stamp.commit_id,
                            &e.memory.memory,
                        )),
                        event_kind: kind,
                        memory: Some(e.memory.memory.clone()),
                        source: None,
                        title: Some(i.record_fields.as_ref().unwrap().title.clone()),
                        change: Some(i.dimensions.as_ref().unwrap().change),
                        knowledge: Some(e.memory.decision.outcome.knowledge),
                        adoption: Some(e.memory.decision.outcome.adoption),
                        effective_sensitivity: effective(e),
                        formation_reason: Some(reason(e)),
                        proof: proof(event.stamp),
                        receipt: Some(e.receipt.clone()),
                    });
                }
            }
            events.sort_by(|a, b| {
                (&b.proof.written_at, &b.event_id).cmp(&(&a.proof.written_at, &a.event_id))
            });
            let matched = events.len();
            let remaining: Vec<_> = events
                .into_iter()
                .filter(|e| {
                    cursor.is_none_or(|c| {
                        (&e.proof.written_at, &e.event_id) < (&c.after_time, &c.after_id)
                    })
                })
                .collect();
            let mut page = MemoryActivityPage {
                schema_version: NextVersion,
                kind: "memory_activity_page".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                matched_in_scope: matched as u64,
                items: Vec::new(),
                next: None,
                complete_in_scope: true,
                time_derived_events_available: false,
            };
            let mut used = bytes(&page)? + 2048;
            for event in remaining.iter().take(request.options.limit) {
                let size = cost(event)?;
                if used + size > request.options.max_bytes {
                    break;
                }
                used += size;
                page.items.push(event.clone());
            }
            page.complete_in_scope = page.items.len() == remaining.len();
            if !page.complete_in_scope {
                page.next = page.items.last().map(|e| ActivityCursor {
                    header: h,
                    after_time: e.proof.written_at.clone(),
                    after_id: e.event_id.clone(),
                });
                if page.next.is_none() {
                    return Err(invalid());
                }
            }
            finish(page, request.options.max_bytes)
        })
    }
    pub fn memory_changes_since_native(
        &self,
        request: &CatalogReadRequest<'_>,
        anchor: Option<&ChangeAnchor>,
    ) -> ApplicationResult<MemoryChanges> {
        self.process(request, anchor.map(|a| &a.snapshot), false, |data| {
            let h = process_header(&data, request, "authorized-native-changes-1", &(), None)?;
            let invalid = anchor.is_some_and(|a| {
                a.permission_hash != data.permission_hash || a.query_hash != h.selection.query_hash
            });
            let mut status = if anchor.is_none() {
                ChangeStatus::Initialize
            } else if data.reset_required || invalid {
                ChangeStatus::ReloadRequired
            } else {
                ChangeStatus::UnchangedInScope
            };
            let mut memories = Vec::new();
            let mut sources = Vec::new();
            if !matches!(
                status,
                ChangeStatus::Initialize | ChangeStatus::ReloadRequired
            ) {
                let changed: BTreeSet<_> = data
                    .changed_memories
                    .iter()
                    .map(|m| &m.memory.memory.memory_id)
                    .collect();
                if !changed.is_empty() {
                    for row in rows(&data.current, request.profile, &h.selection.evaluated_at)? {
                        if changed.contains(row.id())
                            && row.matches(request.filter, request.profile, None, false)
                        {
                            memories.push(row.entry.memory.memory.clone());
                        }
                    }
                }
                if request.filter == &CatalogFilter::default() {
                    sources = data.changed_sources.iter().map(|s| s.reference()).collect();
                }
                if memories.len() + sources.len() > 100 {
                    status = ChangeStatus::ReloadRequired;
                    memories.clear();
                    sources.clear();
                } else if !memories.is_empty() || !sources.is_empty() {
                    status = ChangeStatus::Changed;
                }
            }
            let next_anchor = ChangeAnchor {
                snapshot: h.selection.snapshot.clone(),
                permission_hash: h.permission_hash.clone(),
                query_hash: h.selection.query_hash.clone(),
            };
            let invalidated = anchor.is_none_or(|a| a.snapshot != next_anchor.snapshot)
                || invalid
                || data.reset_required;
            finish(
                MemoryChanges {
                    schema_version: NextVersion,
                    kind: "memory_changes".into(),
                    capability: CatalogCapability::V2,
                    effect: MemoryReadEffect::NativeRead,
                    header: h,
                    status,
                    next_anchor,
                    memories,
                    sources,
                    all_projections_invalidated: invalidated,
                    complete_in_scope: true,
                },
                request.options.max_bytes,
            )
        })
    }
    pub fn memory_detail_native(
        &self,
        request: &CatalogReadRequest<'_>,
        memory: &MemoryId,
        cursor: Option<&HistoryCursor>,
    ) -> ApplicationResult<NativeMemoryDetail> {
        self.process(request, None, true, |data| {
            if cursor.is_some_and(|c| &c.memory != memory) {
                return Err(AutomationFault::Stale.into());
            }
            let h = process_header(
                &data,
                request,
                "authorized-native-detail-1",
                memory,
                cursor.map(|c| &c.header),
            )?;
            let all = rows(&data.current, request.profile, &h.selection.evaluated_at)?;
            let row = all.iter().find(|r| {
                r.id() == memory && r.matches(request.filter, request.profile, None, false)
            });
            let mut history: Vec<_> = data
                .history
                .iter()
                .filter(|m| row.is_some() && &m.memory.memory.memory_id == memory)
                .copied()
                .collect();
            history.sort_by_key(|a| std::cmp::Reverse(a.memory.memory.revision));
            let count = history.len();
            let remaining: Vec<_> = history
                .iter()
                .filter(|m| cursor.is_none_or(|c| m.memory.memory.revision < c.before_revision))
                .collect();
            let mut detail = NativeMemoryDetail {
                schema_version: NextVersion,
                kind: "native_memory_detail".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                item: row.map(|r| r.item()),
                fields: row.and_then(|r| r.entry.memory.decision.input.record_fields.clone()),
                history: Vec::new(),
                history_count_in_scope: count as u64,
                next_history: None,
            };
            let mut used = bytes(&detail)? + 2048;
            for entry in remaining.iter().take(request.options.limit.min(32)) {
                let value = summary(entry);
                let size = cost(&value)?;
                if used + size > request.options.max_bytes {
                    break;
                }
                used += size;
                detail.history.push(value);
            }
            if detail.history.len() < remaining.len() {
                detail.next_history = detail.history.last().map(|e| HistoryCursor {
                    header: h,
                    memory: memory.clone(),
                    before_revision: e.memory.memory.revision,
                });
                if detail.next_history.is_none() {
                    return Err(invalid());
                }
            }
            finish(detail, request.options.max_bytes)
        })
    }
    pub fn memory_conflicts_native(
        &self,
        request: &CatalogReadRequest<'_>,
        memory: &MemoryId,
        cursor: Option<&ConflictCursor>,
    ) -> ApplicationResult<MemoryConflictGroup> {
        self.process(request, None, false, |data| {
            if cursor.is_some_and(|c| &c.memory != memory) {
                return Err(AutomationFault::Stale.into());
            }
            let h = process_header(
                &data,
                request,
                "recorded-counter-evidence-1",
                memory,
                cursor.map(|c| &c.header),
            )?;
            let all = rows(&data.current, request.profile, &h.selection.evaluated_at)?;
            let selected = all.iter().find(|r| {
                r.id() == memory && r.matches(request.filter, request.profile, None, false)
            });
            let counter: Vec<_> = selected
                .into_iter()
                .flat_map(|r| {
                    &r.entry
                        .memory
                        .decision
                        .input
                        .dimensions
                        .as_ref()
                        .unwrap()
                        .counter_sources
                })
                .filter(|r| {
                    data.current
                        .sources
                        .get(&r.source_id)
                        .is_some_and(|s| s.record.revision == r.source_revision)
                })
                .cloned()
                .collect();
            let selected = selected.filter(|_| !counter.is_empty());
            let members: Vec<_> = all
                .iter()
                .filter(|r| {
                    selected.is_some()
                        && r.matches(request.filter, request.profile, None, false)
                        && (r.id() == memory
                            || r.entry.memory.decision.input.evidence.iter().any(|e| {
                                counter.iter().any(|c| {
                                    c.source_id == e.source_id
                                        && c.source_revision == e.source_revision
                                })
                            }))
                })
                .collect();
            let remaining: Vec<_> = members
                .iter()
                .filter(|r| cursor.is_none_or(|c| r.id() > &c.after))
                .collect();
            let mut result = MemoryConflictGroup {
                schema_version: NextVersion,
                kind: "memory_conflict_group".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                selected: selected.map(|r| r.entry.memory.memory.clone()),
                basis: "recorded_counter_source_overlap_not_independent_verification".into(),
                counter_evidence: counter,
                matched_in_scope: members.len() as u64,
                items: Vec::new(),
                next: None,
            };
            let mut used = bytes(&result)? + 2048;
            for row in remaining.iter().take(request.options.limit) {
                let item = row.item();
                let size = cost(&item)?;
                if used + size > request.options.max_bytes {
                    break;
                }
                used += size;
                result.items.push(item);
            }
            if result.items.len() < remaining.len() {
                result.next = result.items.last().map(|m| ConflictCursor {
                    header: h,
                    memory: memory.clone(),
                    after: m.base.memory.memory_id.clone(),
                });
                if result.next.is_none() {
                    return Err(invalid());
                }
            }
            finish(result, request.options.max_bytes)
        })
    }
    pub fn memory_revision_diff_native(
        &self,
        request: &CatalogReadRequest<'_>,
        memory: &MemoryId,
        before: Revision,
        after: Revision,
    ) -> ApplicationResult<MemoryRevisionDiff> {
        self.process(request,None,true,|data| {
            let h=process_header(&data,request,"structural-revision-diff-1",&(memory,before,after),None)?;
            let all=rows(&data.current,request.profile,&h.selection.evaluated_at)?;
            let permitted=all.iter().any(|r|r.id()==memory&&r.matches(request.filter,request.profile,None,false));
            let find=|revision|data.history.iter().find(|m|&m.memory.memory.memory_id==memory&&m.memory.memory.revision==revision).copied();
            let pair=if permitted&&before.get()+1==after.get(){find(before).zip(find(after))}else{None};
            let mut fields=Vec::new();
            if let Some((a,b))=pair {
                let a=&a.memory.decision;let b=&b.memory.decision;
                let values_a=serde_json::json!({"content":a.input.content,"title":a.input.record_fields.as_ref().unwrap().title,"interpretation":a.input.interpretation,"knowledge":a.outcome.knowledge,"adoption":a.outcome.adoption,"dimensions":a.input.dimensions,"entities":a.input.record_fields.as_ref().unwrap().entities,"effective_sensitivity":a.input.sensitivity,"formation":a.input.record_fields.as_ref().unwrap().formation,"annotations":a.input.record_fields.as_ref().unwrap().annotations,"evidence":a.input.evidence,"dependencies":a.input.context_source_refs});
                let values_b=serde_json::json!({"content":b.input.content,"title":b.input.record_fields.as_ref().unwrap().title,"interpretation":b.input.interpretation,"knowledge":b.outcome.knowledge,"adoption":b.outcome.adoption,"dimensions":b.input.dimensions,"entities":b.input.record_fields.as_ref().unwrap().entities,"effective_sensitivity":b.input.sensitivity,"formation":b.input.record_fields.as_ref().unwrap().formation,"annotations":b.input.record_fields.as_ref().unwrap().annotations,"evidence":b.input.evidence,"dependencies":b.input.context_source_refs});
                for (name,value) in values_a.as_object().unwrap(){if &values_b[name]!=value{fields.push(RevisionFieldDiff {field:name.clone(),before:value.clone(),after:values_b[name].clone()});}}
            }
            finish(MemoryRevisionDiff {schema_version:NextVersion,kind:"memory_revision_diff".into(),capability:CatalogCapability::V2,effect:MemoryReadEffect::NativeRead,header:h,before:pair.map(|(a,_)|a.target.clone()),after:pair.map(|(_,b)|b.target.clone()),fields,structural_only:true},request.options.max_bytes)
        })
    }
}
mod conversations;
mod neighbors;

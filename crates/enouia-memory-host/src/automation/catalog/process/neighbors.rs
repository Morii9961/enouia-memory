use super::*;
fn relation(
    name: &str,
    direction: &str,
    witnesses: Vec<String>,
    count: usize,
) -> NativeNeighborRelation {
    NativeNeighborRelation {
        owner_relations: vec![],
        relation: name.into(),
        class: if name == "theme_cooccurrence" {
            GraphEdgeClass::AlgorithmSuggestion
        } else {
            GraphEdgeClass::RecordedRelation
        },
        direction: direction.into(),
        witnesses_complete: witnesses.len() == count,
        witness_count_in_scope: count as u64,
        witnesses,
    }
}
fn conversations(
    data: &AuthorizedSegmentedRead<'_>,
    entry: &SegmentedMemory,
) -> BTreeSet<SessionId> {
    entry
        .memory
        .decision
        .input
        .input_refs
        .iter()
        .filter_map(|r| data.sources.get(&r.source_id))
        .filter_map(|s| s.record_fields.as_ref()?.conversation_id.clone())
        .filter(|id| data.conversations.contains_key(id))
        .collect()
}
fn relations(
    data: &AuthorizedSegmentedRead<'_>,
    selected: &SegmentedMemory,
    other: &SegmentedMemory,
    seeds: &BTreeMap<MemoryId, super::super::projections::ThemeSeed<'_>>,
    suggestions: bool,
    current: bool,
) -> Vec<NativeNeighborRelation> {
    let input = &selected.memory.decision.input;
    let d = input.dimensions.as_ref().unwrap();
    let theirs = &other.memory.decision.input;
    let od = theirs.dimensions.as_ref().unwrap();
    let mut out = Vec::new();
    if d.derived_from.contains(&other.target) {
        out.push(relation(
            "derived_from",
            "outgoing",
            vec![digest(&other.target).to_string()],
            1,
        ));
    }
    if od.derived_from.contains(&selected.target) {
        out.push(relation(
            "derived_from",
            "incoming",
            vec![digest(&selected.target).to_string()],
            1,
        ));
    }
    if d.reverts_to.as_ref() == Some(&other.target) {
        out.push(relation(
            "reversion",
            "outgoing",
            vec![digest(&other.target).to_string()],
            1,
        ));
    }
    if input.target.as_ref() == Some(&other.target) {
        out.push(relation(
            if d.change == FormationChange::Correction {
                "corrects"
            } else {
                "revises"
            },
            "outgoing",
            vec![digest(&other.target).to_string()],
            1,
        ));
    }
    if current {
        let own: BTreeSet<_> = input
            .record_fields
            .as_ref()
            .unwrap()
            .entities
            .iter()
            .map(|e| {
                data.entity_redirects
                    .get(&(
                        selected.memory.memory.memory_id.clone(),
                        e.entity_id.clone(),
                    ))
                    .unwrap_or(&e.entity_id)
            })
            .collect();
        let their: BTreeSet<_> = theirs
            .record_fields
            .as_ref()
            .unwrap()
            .entities
            .iter()
            .map(|e| {
                data.entity_redirects
                    .get(&(other.memory.memory.memory_id.clone(), e.entity_id.clone()))
                    .unwrap_or(&e.entity_id)
            })
            .collect();
        let shared: Vec<_> = own.intersection(&their).map(|id| (**id).clone()).collect();
        if !shared.is_empty() {
            let n = shared.len();
            let mut relation = relation("same_entity", "undirected", shared.clone(), n);
            relation.owner_relations = data
                .entity_proofs
                .iter()
                .filter(|((id, original), _)| {
                    (id == &selected.memory.memory.memory_id
                        || id == &other.memory.memory.memory_id)
                        && data
                            .entity_redirects
                            .get(&(id.clone(), original.clone()))
                            .is_some_and(|e| shared.contains(e))
                })
                .map(|(_, p)| (p.operation_id.clone(), p.clone()))
                .collect::<BTreeMap<_, _>>()
                .into_values()
                .collect();
            out.push(relation);
        }
        let own = conversations(data, selected);
        let their = conversations(data, other);
        let shared: Vec<_> = own.intersection(&their).map(ToString::to_string).collect();
        if !shared.is_empty() {
            let n = shared.len();
            out.push(relation("same_conversation", "undirected", shared, n));
        }
        if suggestions {
            let shared: Vec<_> = seeds
                .iter()
                .filter(|(_, s)| s.score(selected).is_some() && s.score(other).is_some())
                .map(|(id, _)| id.to_string())
                .collect();
            if !shared.is_empty() {
                let count = shared.len();
                out.push(relation(
                    "theme_cooccurrence",
                    "undirected",
                    shared.into_iter().take(128).collect(),
                    count,
                ));
            }
        }
    }
    out
}
impl SegmentedMemoryReader {
    pub fn memory_neighbors_native(
        &self,
        request: &CatalogReadRequest<'_>,
        memory: &MemoryId,
        include_suggestions: bool,
        cursor: Option<&NeighborCursor>,
    ) -> ApplicationResult<NativeMemoryNeighbors> {
        self.process(request, None, true, |data| {
            if cursor.is_some_and(|c| &c.memory != memory) {
                return Err(AutomationFault::Stale.into());
            }
            let h = process_header(
                &data,
                request,
                "typed-selected-neighbors-1",
                &(memory, include_suggestions),
                cursor.map(|c| &c.header),
            )?;
            let all = rows(&data.current, request.profile, &h.selection.evaluated_at)?;
            let selected = all.iter().find(|r| {
                r.id() == memory && r.matches(request.filter, request.profile, None, false)
            });
            let mut page = NativeMemoryNeighbors {
                schema_version: NextVersion,
                kind: "native_memory_neighbors".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                selected: selected.map(|r| r.entry.target.clone()),
                matched_in_scope: 0,
                items: Vec::new(),
                next: None,
            };
            if let Some(selected) = selected {
                if include_suggestions {
                    super::super::projections::theme_budget(
                        &data.current,
                        data.current.memories.len(),
                    )?;
                }
                let seeds = super::super::projections::seeds(&data.current)?;
                let mut candidates: BTreeMap<_, _> = all
                    .iter()
                    .filter(|r| {
                        r.id() != memory && r.matches(request.filter, request.profile, None, false)
                    })
                    .map(|r| {
                        (
                            (r.id().clone(), r.entry.memory.memory.revision),
                            (r.entry, true),
                        )
                    })
                    .collect();
                let i = &selected.entry.memory.decision.input;
                if request.filter == &CatalogFilter::default() {
                    for target in i
                        .target
                        .iter()
                        .chain(i.dimensions.as_ref().unwrap().reverts_to.iter())
                    {
                        if let Some(entry) = data.history.iter().find(|m| m.target == *target) {
                            candidates.insert(
                                (
                                    entry.memory.memory.memory_id.clone(),
                                    entry.memory.memory.revision,
                                ),
                                (*entry, false),
                            );
                        }
                    }
                }
                let mut remaining = 0;
                let mut used = bytes(&page)? + 2048;
                let mut collecting = true;
                for ((id, revision), (other, current)) in candidates {
                    let values = relations(
                        &data.current,
                        selected.entry,
                        other,
                        &seeds,
                        include_suggestions,
                        current,
                    );
                    if values.is_empty() {
                        continue;
                    }
                    page.matched_in_scope += 1;
                    if cursor
                        .is_some_and(|c| (&id, revision) <= (&c.after.memory_id, c.after.revision))
                    {
                        continue;
                    }
                    remaining += 1;
                    if collecting && page.items.len() < request.options.limit {
                        let item = NativeMemoryNeighbor {
                            other: summary(other, &data),
                            relations: values,
                        };
                        let size = cost(&item)?;
                        if used + size <= request.options.max_bytes {
                            used += size;
                            page.items.push(item);
                        } else {
                            collecting = false;
                        }
                    }
                }
                if page.items.len() < remaining {
                    page.next = page.items.last().map(|n| NeighborCursor {
                        header: h,
                        memory: memory.clone(),
                        after: n.other.memory.memory.clone(),
                    });
                    if page.next.is_none() {
                        return Err(invalid());
                    }
                }
            }
            finish(page, request.options.max_bytes)
        })
    }
}

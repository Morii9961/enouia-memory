//! No persisted cache: rebuild only from the freshly authorized native set.
use super::*;
use enouia_memory_contract::json::Revision;
use jiff::civil::Date;

pub(super) struct Family {
    pub family: ThemeFamily,
    pub members: Vec<ThemeMembership>,
}
pub(super) struct ThemeSeed<'a> {
    pub entry: &'a SegmentedMemory,
    entities: BTreeSet<&'a str>,
}
impl ThemeSeed<'_> {
    pub(super) fn score(&self, entry: &SegmentedMemory) -> Option<(u16, &'static str)> {
        if self.entry.memory.memory.memory_id == entry.memory.memory.memory_id {
            return None;
        }
        if self
            .entry
            .memory
            .decision
            .input
            .dimensions
            .as_ref()
            .unwrap()
            .derived_from
            .contains(&entry.target)
        {
            return Some((1000, "recorded_derivation"));
        }
        entry
            .memory
            .decision
            .input
            .record_fields
            .as_ref()
            .unwrap()
            .entities
            .iter()
            .any(|e| e.role != EntityRole::Subject && self.entities.contains(e.entity_id.as_str()))
            .then_some((750, "shared_entity_suggestion"))
    }
}
pub(super) fn seeds<'a>(data: &AuthorizedSegmentedRead<'a>) -> BTreeMap<MemoryId, ThemeSeed<'a>> {
    data.memories
        .iter()
        .filter_map(|entry| {
            let i = &entry.memory.decision.input;
            (i.dimensions.as_ref().unwrap().kind == FormationKind::Theme).then(|| {
                (
                    entry.memory.memory.memory_id.clone(),
                    ThemeSeed {
                        entry,
                        entities: i
                            .record_fields
                            .as_ref()
                            .unwrap()
                            .entities
                            .iter()
                            .filter(|e| e.role != EntityRole::Subject)
                            .map(|e| e.entity_id.as_str())
                            .collect(),
                    },
                )
            })
        })
        .collect()
}
fn family(data: &AuthorizedSegmentedRead<'_>, theme: &MemoryId) -> Option<Family> {
    let seeds = seeds(data);
    let seed = seeds.get(theme)?;
    let members: Vec<_> = data
        .memories
        .iter()
        .filter_map(|entry| {
            seed.score(entry)
                .map(|(score_milli, basis)| ThemeMembership {
                    memory: entry.target.clone(),
                    score_milli,
                    basis: basis.into(),
                })
        })
        .collect();
    Some(Family {
        family: ThemeFamily {
            theme: seed.entry.target.clone(),
            name: seed
                .entry
                .memory
                .decision
                .input
                .record_fields
                .as_ref()
                .unwrap()
                .title
                .clone(),
            member_count: members.len() as u64,
            owner_named: false,
            pinned: false,
        },
        members,
    })
}
pub(super) fn theme_budget(
    data: &AuthorizedSegmentedRead<'_>,
    inputs: usize,
) -> ApplicationResult<()> {
    let count = data
        .memories
        .iter()
        .filter(|m| {
            m.memory.decision.input.dimensions.as_ref().unwrap().kind == FormationKind::Theme
        })
        .count();
    if inputs.saturating_mul(count) > 16_777_216 {
        return Err(AutomationFault::Capacity.into());
    }
    Ok(())
}
fn header(
    data: &AuthorizedSegmentedRead<'_>,
    selection: CatalogSelection,
    algorithm: &str,
    parameters: &impl serde::Serialize,
    matched: usize,
    included: usize,
    limits: Vec<String>,
) -> ProjectionHeader {
    ProjectionHeader {
        selection,
        algorithm: algorithm.into(),
        version: Revision::new(1).unwrap(),
        parameters_hash: digest(parameters),
        authorized_at: data.now.clone(),
        scope: "authorized_current_machine_revisions".into(),
        matched_inputs: matched as u64,
        included_inputs: included as u64,
        complete_in_scope: matched == included,
        limits,
    }
}
fn same_header(expected: &ProjectionHeader, current: &ProjectionHeader) -> bool {
    let mut expected = expected.clone();
    expected.authorized_at = current.authorized_at.clone();
    expected == *current
}
fn finish<T: NextDocument + serde::Serialize>(value: T, max_bytes: usize) -> ApplicationResult<T> {
    if !value.validate().is_empty() || bytes(&value)? > max_bytes {
        return Err(invalid());
    }
    Ok(value)
}
fn boundary(date: Date, zone: &jiff::tz::TimeZone) -> ApplicationResult<Timestamp> {
    // Jiff resolves a midnight gap forward and a fold earlier; start_of_day
    // also handles transitions at midnight. The database is bundled, never OS TZ.
    let instant = date
        .to_zoned(zone.clone())
        .and_then(|z| z.start_of_day())
        .map_err(|_| invalid())?
        .timestamp();
    Timestamp::from_unix_ms(instant.as_millisecond()).ok_or_else(invalid)
}
fn civil(value: &str) -> ApplicationResult<Date> {
    let date: Date = value.parse().map_err(|_| invalid())?;
    if date.to_string() != value || value.len() != 10 {
        return Err(invalid());
    }
    Ok(date)
}
fn overlap(
    from: Option<&Timestamp>,
    until: Option<&Timestamp>,
    start: &Timestamp,
    end: &Timestamp,
) -> bool {
    from.is_some_and(|from| from < end && until.map_or(from >= start, |until| until > start))
        && start < end
}
impl SegmentedMemoryReader {
    pub fn memory_time_buckets_native(
        &self,
        request: &CatalogReadRequest<'_>,
        query: &TimeBucketQuery,
        expected: Option<&CatalogSelection>,
    ) -> ApplicationResult<MemoryTimeBuckets> {
        self.read(request, |data| {
            if request.filter.time_axis.is_some() {
                return Err(invalid());
            }
            let from = civil(&query.from_date)?;
            let until = civil(&query.until_date)?;
            if from >= until
                || query.timezone.len() > 128
                || (query.granularity == BucketGranularity::Month
                    && (from.day() != 1 || until.day() != 1))
            {
                return Err(invalid());
            }
            let (canonical, tzif) = jiff_tzdb::get(&query.timezone).ok_or_else(invalid)?;
            let zone = jiff::tz::TimeZone::tzif(canonical, tzif).map_err(|_| invalid())?;
            let selection = selection(&data, request, expected)?;
            let all = rows(&data, request.profile, &selection.evaluated_at)?;
            let selected: Vec<_> = all
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .collect();
            let unknown = selected
                .iter()
                .filter(|r| r.range(query.axis).0.is_none())
                .count();
            let mut buckets = Vec::new();
            let mut date = from;
            while date < until {
                if buckets.len() >= 366 {
                    return Err(AutomationFault::Capacity.into());
                }
                let next = match query.granularity {
                    BucketGranularity::Day => date.tomorrow(),
                    BucketGranularity::Month => date.last_of_month().tomorrow(),
                }
                .map_err(|_| invalid())?;
                let start = boundary(date, &zone)?;
                let end = boundary(next, &zone)?;
                let matches: Vec<_> = selected
                    .iter()
                    .filter(|r| {
                        let (a, b) = r.range(query.axis);
                        overlap(a, b, &start, &end)
                    })
                    .collect();
                buckets.push(MemoryTimeBucket {
                    local_start: date.to_string(),
                    from: start.clone(),
                    until: end.clone(),
                    possible_records: matches.len() as u64,
                    staged: matches
                        .iter()
                        .filter(|r| {
                            r.entry.memory.decision.outcome.adoption == AdoptionState::Staged
                        })
                        .count() as u64,
                    adopted: matches
                        .iter()
                        .filter(|r| {
                            r.entry.memory.decision.outcome.adoption == AdoptionState::Adopted
                        })
                        .count() as u64,
                    captured_material: data
                        .sources
                        .values()
                        .filter(|s| s.record.captured_at >= start && s.record.captured_at < end)
                        .count() as u64,
                });
                date = next;
            }
            let h = header(
                &data,
                selection,
                "iana-overlap-buckets-1",
                query,
                selected.len(),
                selected.len(),
                vec![
                    "overlapping_possible_intervals_are_not_additive".into(),
                    "captured_material_uses_its_own_capture_axis_and_current_grants".into(),
                ],
            );
            finish(
                MemoryTimeBuckets {
                    schema_version: NextVersion,
                    kind: "memory_time_buckets".into(),
                    capability: CatalogCapability::V2,
                    effect: MemoryReadEffect::NativeRead,
                    header: h,
                    query: query.clone(),
                    timezone_database: format!(
                        "jiff-0.2.37/jiff-tzdb-0.1.8/iana-{}-bundled",
                        jiff_tzdb::VERSION.ok_or_else(invalid)?
                    ),
                    interval_rule: "possible_overlap_half_open_not_additive".into(),
                    material_axis: CatalogTimeAxis::SourceCaptured,
                    material_scope: "authorized_current_sources_unfiltered".into(),
                    authorized_material_in_scope: data.sources.len() as u64,
                    unknown_time_in_scope: unknown as u64,
                    buckets,
                },
                request.options.max_bytes,
            )
        })
    }
    pub fn memory_theme_members_native(
        &self,
        request: &CatalogReadRequest<'_>,
        theme: &MemoryId,
        cursor: Option<&ThemeCursor>,
    ) -> ApplicationResult<MemoryThemeMembers> {
        self.read(request, |data| {
            if cursor.is_some_and(|c| &c.theme != theme) {
                return Err(AutomationFault::Stale.into());
            }
            let selection = selection(&data, request, cursor.map(|c| &c.header.selection))?;
            let all = rows(&data, request.profile, &selection.evaluated_at)?;
            let selected: BTreeSet<_> = all
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .map(|r| r.id())
                .collect();
            let mut family = family(&data, theme);
            if let Some(family) = &mut family {
                family
                    .members
                    .retain(|m| selected.contains(&m.memory.memory.memory_id));
                family.family.member_count = family.members.len() as u64;
            }
            let count = family.as_ref().map_or(0, |f| f.members.len());
            let h = header(
                &data,
                selection,
                "recorded-shared-entity-families-1",
                theme,
                selected.len(),
                selected.len(),
                vec![
                    "shared_entity_members_are_algorithm_suggestions".into(),
                    "stable_family_id_is_its_recorded_theme_memory_id".into(),
                ],
            );
            if cursor.is_some_and(|c| !same_header(&c.header, &h)) {
                return Err(AutomationFault::Stale.into());
            }
            let remaining: Vec<_> = family
                .as_ref()
                .into_iter()
                .flat_map(|f| &f.members)
                .filter(|m| cursor.is_none_or(|c| m.memory.memory.memory_id > c.after))
                .collect();
            let mut result = MemoryThemeMembers {
                schema_version: NextVersion,
                kind: "memory_theme_members".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                family: family.as_ref().map(|f| f.family.clone()),
                items: Vec::new(),
                next: None,
            };
            let mut used = bytes(&result)? + 2048;
            for member in remaining.iter().take(request.options.limit) {
                let cost = bytes(*member)? + 16;
                if used + cost > request.options.max_bytes {
                    break;
                }
                used += cost;
                result.items.push((*member).clone());
            }
            if result.items.len() < remaining.len() {
                result.next = result.items.last().map(|m| ThemeCursor {
                    header: h,
                    theme: theme.clone(),
                    after: m.memory.memory.memory_id.clone(),
                });
                if result.next.is_none() {
                    return Err(invalid());
                }
            }
            debug_assert!(result.items.len() <= count);
            finish(result, request.options.max_bytes)
        })
    }
    pub fn memory_graph_native(
        &self,
        request: &CatalogReadRequest<'_>,
        query: &GraphQuery,
        cursor: Option<&GraphCursor>,
    ) -> ApplicationResult<MemoryGraphPage> {
        self.read(request, |data| {
            if !(1..=5000).contains(&query.node_limit) || query.edge_limit_per_kind > 20_000 {
                return Err(invalid());
            }
            let selection = selection(&data, request, cursor.map(|c| &c.header.selection))?;
            let all = rows(&data, request.profile, &selection.evaluated_at)?;
            let matched: Vec<_> = all
                .iter()
                .filter(|r| r.matches(request.filter, request.profile, None, false))
                .collect();
            let selected: Vec<_> = matched.iter().take(query.node_limit).copied().collect();
            let mut h = header(
                &data,
                selection,
                "typed-authorized-graph-1",
                query,
                matched.len(),
                selected.len(),
                vec![
                    "edge_candidates_and_retention_describe_only_selected_nodes".into(),
                    "semantic_neighbors_unavailable_no_model_or_embeddings".into(),
                    "same_entity_is_a_recorded_mention_relation_not_independent_verification"
                        .into(),
                ],
            );
            theme_budget(&data, selected.len())?;
            let ids: BTreeSet<_> = selected.iter().map(|r| r.id()).collect();
            let mut edges = Vec::new();
            let mut coverage = Vec::new();
            for kind in [
                GraphEdgeKind::DerivedFrom,
                GraphEdgeKind::Reversion,
                GraphEdgeKind::SameConversation,
                GraphEdgeKind::SameEntity,
                GraphEdgeKind::ThemeCooccurrence,
            ] {
                let mut candidates = 0u64;
                let begin = edges.len();
                if matches!(kind, GraphEdgeKind::DerivedFrom | GraphEdgeKind::Reversion) {
                    let mut add = |edge: MemoryGraphEdge| {
                        candidates += 1;
                        if edges.len() - begin < query.edge_limit_per_kind {
                            edges.push(edge);
                        }
                    };
                    for row in &selected {
                        let d = row.entry.memory.decision.input.dimensions.as_ref().unwrap();
                        let refs: Vec<_> = if kind == GraphEdgeKind::DerivedFrom {
                            d.derived_from.iter().collect()
                        } else {
                            d.reverts_to.iter().collect()
                        };
                        for target in refs {
                            if ids.contains(&target.memory.memory_id)
                                && row.id() != &target.memory.memory_id
                            {
                                add(MemoryGraphEdge {
                                    from: row.id().clone(),
                                    to: target.memory.memory_id.clone(),
                                    relation: kind,
                                    class: GraphEdgeClass::RecordedRelation,
                                    witness: digest(target).to_string(),
                                    score_milli: None,
                                });
                            }
                        }
                    }
                } else {
                    let mut groups: BTreeMap<String, BTreeSet<MemoryId>> = BTreeMap::new();
                    for row in &selected {
                        let values: Vec<String> = match kind {
                            GraphEdgeKind::SameEntity => {
                                row.entities.iter().map(|e| e.entity_id.clone()).collect()
                            }
                            GraphEdgeKind::ThemeCooccurrence => {
                                row.theme_values().map(|(id, _)| id.to_string()).collect()
                            }
                            GraphEdgeKind::SameConversation => row
                                .entry
                                .memory
                                .decision
                                .input
                                .input_refs
                                .iter()
                                .filter_map(|r| data.sources.get(&r.source_id))
                                .filter_map(|s| s.record_fields.as_ref()?.conversation_id.as_ref())
                                .filter(|id| data.conversations.contains_key(*id))
                                .map(ToString::to_string)
                                .collect(),
                            _ => unreachable!(),
                        };
                        for value in values {
                            groups.entry(value).or_default().insert(row.id().clone());
                        }
                        if groups.values().map(BTreeSet::len).sum::<usize>() > 262_144 {
                            return Err(AutomationFault::Capacity.into());
                        }
                    }
                    // Pair count is exact without materializing an unbounded clique.
                    for (witness, members) in groups {
                        let len = members.len() as u64;
                        candidates += len * len.saturating_sub(1) / 2;
                        if edges.len() - begin >= query.edge_limit_per_kind {
                            continue;
                        }
                        let members: Vec<_> = members.into_iter().collect();
                        'pairs: for (i, a) in members.iter().enumerate() {
                            for b in &members[i + 1..] {
                                if edges.len() - begin >= query.edge_limit_per_kind {
                                    break 'pairs;
                                }
                                edges.push(MemoryGraphEdge {
                                    from: a.clone(),
                                    to: b.clone(),
                                    relation: kind,
                                    class: if kind == GraphEdgeKind::ThemeCooccurrence {
                                        GraphEdgeClass::AlgorithmSuggestion
                                    } else {
                                        GraphEdgeClass::RecordedRelation
                                    },
                                    witness: witness.clone(),
                                    score_milli: if kind == GraphEdgeKind::ThemeCooccurrence {
                                        Some(750)
                                    } else {
                                        None
                                    },
                                });
                            }
                        }
                    }
                }
                coverage.push(GraphEdgeCoverage {
                    relation: kind,
                    candidates,
                    retained: (edges.len() - begin) as u64,
                });
            }
            h.complete_in_scope &= coverage.iter().all(|c| c.candidates == c.retained);
            if cursor.is_some_and(|c| !same_header(&c.header, &h)) {
                return Err(AutomationFault::Stale.into());
            }
            let offset = cursor.map_or(0, |c| c.offset);
            if offset > selected.len() + edges.len() {
                return Err(invalid());
            }
            let mut page = MemoryGraphPage {
                schema_version: NextVersion,
                kind: "memory_graph_page".into(),
                capability: CatalogCapability::V2,
                effect: MemoryReadEffect::NativeRead,
                header: h.clone(),
                nodes: Vec::new(),
                edges: Vec::new(),
                edge_coverage: coverage,
                semantic_neighbors_available: false,
                next: None,
            };
            let mut used = bytes(&page)? + 2048;
            let mut position = offset;
            while position < selected.len() + edges.len()
                && page.nodes.len() + page.edges.len() < request.options.limit
            {
                if position < selected.len() {
                    let row = selected[position];
                    let i = &row.entry.memory.decision.input;
                    let f = i.record_fields.as_ref().unwrap();
                    let node = MemoryGraphNode {
                        memory: row.entry.target.clone(),
                        title: f.title.clone(),
                        form: i.dimensions.as_ref().unwrap().kind,
                        knowledge: row.entry.memory.decision.outcome.knowledge,
                        adoption: row.entry.memory.decision.outcome.adoption,
                        validity: row.validity,
                        attention: row.attention,
                        event_time: f.event_time.clone(),
                        entities: row.entities.iter().map(|e| e.entity_id.clone()).collect(),
                        themes: row.theme_values().map(|(id, _)| id.clone()).collect(),
                    };
                    let cost = bytes(&node)? + 16;
                    if used + cost > request.options.max_bytes {
                        break;
                    }
                    used += cost;
                    page.nodes.push(node);
                } else {
                    let edge = &edges[position - selected.len()];
                    let cost = bytes(edge)? + 16;
                    if used + cost > request.options.max_bytes {
                        break;
                    }
                    used += cost;
                    page.edges.push(edge.clone());
                }
                position += 1;
            }
            if position < selected.len() + edges.len() {
                if position == offset {
                    return Err(invalid());
                }
                page.next = Some(GraphCursor {
                    header: h,
                    offset: position,
                });
            }
            finish(page, request.options.max_bytes)
        })
    }
}

#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, hash::sha256, next::*, time::Timestamp};
use enouia_memory_host::automation::catalog::*;
use enouia_memory_vault::{automation::segmented::SegmentedAutomationVault, fault::Faults};
use fixture::*;
use std::{collections::BTreeSet, sync::Arc};
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}
fn request<'a>(p: &'a AttentionProfile, f: &'a CatalogFilter) -> CatalogReadRequest<'a> {
    CatalogReadRequest {
        profile: p,
        filter: f,
        options: CatalogReadOptions {
            limit: 37,
            max_bytes: MAX_DOCUMENT_BYTES,
            max_sensitivity: Sensitivity::Public,
            include_facets: false,
        },
    }
}
fn reader(e: &Env, v: Arc<SegmentedAutomationVault>) -> SegmentedMemoryReader {
    SegmentedMemoryReader::from_authenticated_system(v, e.descriptor.system_actor.clone()).unwrap()
}
fn commit(e: &Env, v: &SegmentedAutomationVault, input: AdmissionInput) -> MemoryReceipt {
    let input = v.prepare(&e.descriptor.system_actor, input).unwrap();
    let response =
        serde_json::to_string(&v.evaluate(&e.descriptor.system_actor, &input).unwrap()).unwrap();
    let binding = GenerationBinding {
        operation_id: input.operation_id.clone(),
        input_hash: digest(&input),
        response_hash: sha256(response.as_bytes()),
    };
    v.apply_batch(
        &e.descriptor.system_actor,
        vec![input],
        vec![response],
        vec![binding],
    )
    .unwrap()
    .remove(0)
}
fn event(input: &mut AdmissionInput, from: &str, until: Option<&str>) {
    let from = Timestamp::parse(from).unwrap();
    let until = until.map(|s| Timestamp::parse(s).unwrap());
    input.dimensions.as_mut().unwrap().occurrence = OccurrenceAssertion::ReportedOccurred;
    input.dimensions.as_mut().unwrap().event_time = Some(FormationTime {
        from: from.clone(),
        until: until.clone(),
    });
    input.record_fields.as_mut().unwrap().event_time = RecordTime {
        from: Some(from),
        until: until.clone(),
        precision: RecordPrecision::Instant,
        bound: if until.is_some() {
            TimeBound::Span
        } else {
            TimeBound::Within
        },
        tz: Some("Etc/UTC".into()),
        basis: TimeBasis::Stated,
    };
}
#[test]
fn timezone_boundaries_uncertainty_dst_skipped_dates_and_layer_axes_are_explicit() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 4);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let sources: Vec<_> = (1..=4).map(|n| e.source(n, Sensitivity::Public)).collect();
    v.capture(
        &e.descriptor.system_actor,
        sources.clone(),
        RecordCatalog::default(),
    )
    .unwrap();
    for (index, s) in sources.iter().enumerate() {
        let mut input = e.input(s, &p, index as u32 + 1);
        match index {
            0 => event(&mut input, "2024-01-01T15:59:59.000Z", None),
            1 => event(&mut input, "2024-01-01T16:00:00.000Z", None),
            2 => event(
                &mut input,
                "2024-01-01T15:00:00.000Z",
                Some("2024-01-02T17:00:00.000Z"),
            ),
            _ => {}
        }
        commit(&e, &v, input);
    }
    let r = reader(&e, v);
    let p = profile();
    let f = CatalogFilter::default();
    let req = request(&p, &f);
    let mut query = TimeBucketQuery {
        axis: CatalogTimeAxis::Event,
        timezone: "Asia/Shanghai".into(),
        granularity: BucketGranularity::Day,
        from_date: "2024-01-01".into(),
        until_date: "2024-01-04".into(),
    };
    let result = r.memory_time_buckets_native(&req, &query, None).unwrap();
    assert_eq!(result.unknown_time_in_scope, 1);
    assert_eq!(
        result
            .buckets
            .iter()
            .map(|b| b.possible_records)
            .collect::<Vec<_>>(),
        vec![2, 2, 1]
    );
    assert!(result.buckets.iter().all(|b| b.captured_material == 0));
    query.timezone = "Etc/UTC".into();
    assert_eq!(
        r.memory_time_buckets_native(&req, &query, None)
            .unwrap()
            .buckets
            .iter()
            .map(|b| b.possible_records)
            .collect::<Vec<_>>(),
        vec![3, 1, 0]
    );
    query.timezone = "Europe/Berlin".into();
    query.from_date = "2024-03-31".into();
    query.until_date = "2024-04-02".into();
    let dst = r.memory_time_buckets_native(&req, &query, None).unwrap();
    assert_eq!(
        dst.buckets[0].until.unix_ms() - dst.buckets[0].from.unix_ms(),
        23 * 3_600_000
    );
    query.from_date = "2024-10-27".into();
    query.until_date = "2024-10-28".into();
    let dst = r.memory_time_buckets_native(&req, &query, None).unwrap();
    assert_eq!(
        dst.buckets[0].until.unix_ms() - dst.buckets[0].from.unix_ms(),
        25 * 3_600_000
    );
    query.timezone = "Pacific/Apia".into();
    query.from_date = "2011-12-30".into();
    query.until_date = "2011-12-31".into();
    let skipped = r.memory_time_buckets_native(&req, &query, None).unwrap();
    assert_eq!(skipped.buckets[0].from, skipped.buckets[0].until);
    query.timezone = "Etc/UTC".into();
    query.from_date = "2024-01-01".into();
    query.until_date = "2025-01-01".into();
    assert_eq!(
        r.memory_time_buckets_native(&req, &query, None)
            .unwrap()
            .buckets
            .len(),
        366
    );
    query.granularity = BucketGranularity::Month;
    assert_eq!(
        r.memory_time_buckets_native(&req, &query, None)
            .unwrap()
            .buckets
            .len(),
        12
    );
    query.timezone = "Unknown/Nowhere".into();
    assert!(r.memory_time_buckets_native(&req, &query, None).is_err());
    query.timezone = "Etc/UTC".into();
    query.from_date = "2026-10-24".into();
    query.until_date = "2026-10-25".into();
    query.granularity = BucketGranularity::Day;
    query.axis = CatalogTimeAxis::Learned;
    let learned = r.memory_time_buckets_native(&req, &query, None).unwrap();
    assert_eq!(learned.buckets[0].possible_records, 4);
    assert_eq!(learned.buckets[0].captured_material, 4);
    r.lock().unwrap();
    assert!(r.memory_time_buckets_native(&req, &query, None).is_err());
}
#[test]
fn hundreds_of_theme_members_and_typed_graph_edges_rebuild_without_hidden_input_or_resurrection() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let sources: Vec<_> = (1..=201)
        .map(|n| {
            e.source(
                n,
                if n % 7 == 0 {
                    Sensitivity::Private
                } else {
                    Sensitivity::Public
                },
            )
        })
        .collect();
    let mut policies = Vec::new();
    let mut receipts = Vec::new();
    let mut template: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    template.conversations.clear();
    template.entities.truncate(1);
    let entity = &mut template.entities[0];
    entity.sensitivity = Sensitivity::Public;
    entity.aliases.clear();
    entity.evidence = e
        .input(&sources[0], &e.policy(1, 1, 64), 1)
        .record_fields
        .unwrap()
        .evidence;
    let entity_id = entity.entity_id.clone();
    for (i, chunk) in sources.chunks(64).enumerate() {
        let mut policy = e.policy(i as u32 + 1, i as u32 * 64 + 1, chunk.len() as u32);
        if i > 0 {
            policy
                .source_ids
                .insert(0, sources[0].record.source_id.clone());
        }
        v.set_policy(&e.descriptor.created_by, policy.clone())
            .unwrap();
        v.capture(
            &e.descriptor.system_actor,
            chunk.to_vec(),
            if i == 0 {
                template.clone()
            } else {
                RecordCatalog::default()
            },
        )
        .unwrap();
        for (j, source) in chunk.iter().enumerate() {
            let mut input = e.input(source, &policy, (i * 64 + j + 1) as u32);
            let evidence = input.record_fields.as_ref().unwrap().evidence.clone();
            input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
                entity_id: entity_id.clone(),
                role: EntityRole::Mentioned,
                evidence,
            }];
            receipts.push(commit(&e, &v, input));
        }
        policies.push(policy);
    }
    let parents = v
        .read(&e.descriptor.system_actor, Sensitivity::Public, |data| {
            data.memories
                .iter()
                .filter(|m| {
                    m.receipt.result == receipts[0].result || m.receipt.result == receipts[1].result
                })
                .map(|m| m.target.clone())
                .collect()
        })
        .unwrap();
    let mut input = e.input(&sources[0], &policies[0], 1);
    input.input_refs.push(sources[1].reference());
    input.context_source_refs = vec![sources[1].reference()];
    input.interpretation = Interpretation::Inference;
    let d = input.dimensions.as_mut().unwrap();
    d.kind = FormationKind::Theme;
    d.change = FormationChange::Synthesis;
    d.derived_from = parents;
    input.record_fields.as_mut().unwrap().title = "Synthetic shared entity family".into();
    input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
        entity_id,
        role: EntityRole::Mentioned,
        evidence: input.record_fields.as_ref().unwrap().evidence.clone(),
    }];
    let theme = commit(&e, &v, input).result.memory_id;
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let req = request(&p, &f);
    let mut cursor = None;
    let mut members = BTreeSet::new();
    loop {
        let page = r
            .memory_theme_members_native(&req, &theme, cursor.as_ref())
            .unwrap();
        assert_eq!(page.family.as_ref().unwrap().member_count, 173);
        assert!(!page.family.as_ref().unwrap().owner_named);
        for member in page.items {
            assert!(members.insert(member.memory.memory.memory_id));
        }
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(members.len(), 173);
    let filter = CatalogFilter {
        theme: Some(theme.clone()),
        ..Default::default()
    };
    assert_eq!(
        r.memory_stats_native(&request(&p, &filter), None)
            .unwrap()
            .matched_in_scope,
        173
    );
    let query = GraphQuery {
        node_limit: 5000,
        edge_limit_per_kind: 20,
    };
    let first = r.memory_graph_native(&req, &query, None).unwrap();
    let stale = first.next.clone().unwrap();
    assert_eq!(first.header.matched_inputs, 174);
    let same_entity = first
        .edge_coverage
        .iter()
        .find(|c| c.relation == GraphEdgeKind::SameEntity)
        .unwrap();
    assert_eq!(same_entity.candidates, 174 * 173 / 2);
    assert_eq!(same_entity.retained, 20);
    let suggestions = first
        .edge_coverage
        .iter()
        .find(|c| c.relation == GraphEdgeKind::ThemeCooccurrence)
        .unwrap();
    assert_eq!(suggestions.candidates, 173 * 172 / 2);
    let mut nodes = BTreeSet::new();
    let mut edges = Vec::new();
    let mut page = first;
    loop {
        for node in page.nodes {
            assert!(nodes.insert(node.memory.memory.memory_id));
        }
        edges.extend(page.edges);
        let Some(next) = page.next else {
            break;
        };
        e.clock.set(NOW + 1);
        page = r.memory_graph_native(&req, &query, Some(&next)).unwrap();
    }
    assert_eq!(nodes.len(), 174);
    assert!(
        edges
            .iter()
            .all(|edge| nodes.contains(&edge.from) && nodes.contains(&edge.to))
    );
    assert!(
        edges
            .iter()
            .any(|e| e.class == GraphEdgeClass::AlgorithmSuggestion)
    );
    for hidden in receipts
        .iter()
        .enumerate()
        .filter(|(i, _)| (i + 1) % 7 == 0)
        .map(|(_, r)| &r.result.memory_id)
    {
        assert!(!nodes.contains(hidden));
    }
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipts[2].result.memory_id.clone()],
    )
    .unwrap();
    assert!(r.memory_graph_native(&req, &query, Some(&stale)).is_err());
    assert_eq!(
        r.memory_theme_members_native(&req, &theme, None)
            .unwrap()
            .family
            .unwrap()
            .member_count,
        172
    );
    drop(r);
    drop(v);
    let v = Arc::new(e.open());
    let r = reader(&e, v.clone());
    let family = r
        .memory_theme_members_native(&req, &theme, None)
        .unwrap()
        .family
        .unwrap();
    assert_eq!(family.theme.memory.memory_id, theme);
    assert_eq!(family.member_count, 172);
    v.delete(
        &e.descriptor.created_by,
        vec![sources[0].record.source_id.clone()],
        vec![],
    )
    .unwrap();
    assert!(
        r.memory_theme_members_native(&req, &theme, None)
            .unwrap()
            .family
            .is_none()
    );
    let empty = r.memory_graph_native(&req, &query, None).unwrap();
    assert!(empty.nodes.is_empty() && empty.edges.is_empty());
    assert!(empty.edge_coverage.iter().all(|c| c.candidates == 0));
}

#[test]
fn conversation_relations_require_a_current_authorized_conversation_and_both_endpoints() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let policy = e.policy(1, 1, 2);
    v.set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.entities.clear();
    let c = &mut catalog.conversations[0];
    c.sensitivity = Sensitivity::Public;
    c.turn_count = 2;
    let mut sources = vec![
        e.source(1, Sensitivity::Public),
        e.source(2, Sensitivity::Public),
    ];
    for source in &mut sources {
        source.record_fields.as_mut().unwrap().conversation_id = Some(c.conversation_id.clone());
    }
    c.turns = (0..2)
        .map(|index| ConversationTurn {
            source: sources[index].reference(),
            branch_id: c.branches[0].clone(),
            turn_id: enouia_memory_contract::ids::TurnId::parse(&id("turn", index as u32 + 1))
                .unwrap(),
            sequence: index as u32 + 1,
            parent_source: if index == 0 {
                None
            } else {
                Some(sources[0].reference())
            },
        })
        .collect();
    v.capture(&e.descriptor.system_actor, sources.clone(), catalog.clone())
        .unwrap();
    let one = commit(&e, &v, e.input(&sources[0], &policy, 1));
    commit(&e, &v, e.input(&sources[1], &policy, 2));
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let mut req = request(&p, &f);
    req.options.limit = 100;
    let query = GraphQuery {
        node_limit: 200,
        edge_limit_per_kind: 100,
    };
    let graph = r.memory_graph_native(&req, &query, None).unwrap();
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.relation == GraphEdgeKind::SameConversation)
    );
    catalog.conversations[0].revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    catalog.conversations[0].sensitivity = Sensitivity::Private;
    sources[1].record.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    catalog.conversations[0].turns[1].source = sources[1].reference();
    v.capture(
        &e.descriptor.system_actor,
        vec![sources[1].clone()],
        catalog,
    )
    .unwrap();
    let graph = r.memory_graph_native(&req, &query, None).unwrap();
    assert_eq!(graph.nodes.len(), 1);
    assert_eq!(graph.nodes[0].memory.memory, one.result);
    assert!(graph.edges.is_empty());
    assert!(graph.edge_coverage.iter().all(|c| c.candidates == 0));
}

#[test]
#[ignore = "explicit 5,000-node native graph traversal; run with --ignored --nocapture"]
fn five_thousand_graph_nodes_progress_with_exact_window_coverage_and_no_omissions() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 5001);
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let mut req = request(&p, &f);
    req.options.max_sensitivity = Sensitivity::Private;
    req.options.limit = 100;
    let query = GraphQuery {
        node_limit: 5000,
        edge_limit_per_kind: 100,
    };
    let mut ids = BTreeSet::new();
    let mut cursor = None;
    let mut pages = 0;
    let mut first_cursor = None;
    loop {
        let page = r
            .memory_graph_native(&req, &query, cursor.as_ref())
            .unwrap();
        assert_eq!(page.header.matched_inputs, 5001);
        assert_eq!(page.header.included_inputs, 5000);
        assert!(!page.header.complete_in_scope);
        assert!(page.edges.is_empty());
        for node in page.nodes {
            assert!(ids.insert(node.memory.memory.memory_id));
        }
        pages += 1;
        cursor = page.next;
        if pages == 1 {
            first_cursor = cursor.clone();
        }
        if cursor.is_none() {
            break;
        }
    }
    let expected: BTreeSet<_> = receipts
        .iter()
        .map(|r| r.result.memory_id.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .take(5000)
        .collect();
    assert_eq!(ids, expected);
    assert!(pages >= 50);
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![ids.first().unwrap().clone()],
    )
    .unwrap();
    assert!(
        r.memory_graph_native(&req, &query, first_cursor.as_ref())
            .is_err()
    );
    let page = r.memory_graph_native(&req, &query, None).unwrap();
    assert_eq!(page.header.matched_inputs, 5000);
    assert!(page.header.complete_in_scope);
    println!("native graph nodes=5000, pages={pages}, deletion invalidates old cursor");
}

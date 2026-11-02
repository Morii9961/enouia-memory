#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*,
    hash::sha256,
    ids::*,
    json::{Revision, canonical_bytes},
    next::*,
    time::Timestamp,
};
use enouia_memory_host::automation::{application::ApplicationFault, catalog::*};
use enouia_memory_vault::{automation::segmented::SegmentedAutomationVault, fault::Faults};
use fixture::*;
use std::{collections::BTreeSet, sync::Arc, time::Instant};
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}
fn request<'a>(
    profile: &'a AttentionProfile,
    filter: &'a CatalogFilter,
    max: Sensitivity,
) -> CatalogReadRequest<'a> {
    CatalogReadRequest {
        profile,
        filter,
        options: CatalogReadOptions {
            limit: 100,
            max_bytes: MAX_DOCUMENT_BYTES,
            max_sensitivity: max,
            include_facets: false,
        },
    }
}
fn reader(e: &Env, v: Arc<SegmentedAutomationVault>) -> SegmentedMemoryReader {
    SegmentedMemoryReader::from_authenticated_system(v, e.descriptor.system_actor.clone()).unwrap()
}
fn all(
    reader: &SegmentedMemoryReader,
    request: &CatalogReadRequest<'_>,
) -> (BTreeSet<MemoryId>, u64, usize) {
    let mut ids = BTreeSet::new();
    let mut cursor = None;
    let mut total = None;
    let mut pages = 0;
    loop {
        let page = reader.memory_page_native(request, cursor.as_ref()).unwrap();
        assert!(page.validate().is_empty());
        assert!(canonical_bytes(&page).unwrap().len() <= request.options.max_bytes);
        assert!(page.items.len() <= 100);
        assert_eq!(
            *total.get_or_insert(page.matched_in_scope),
            page.matched_in_scope
        );
        for item in &page.items {
            assert!(ids.insert(item.base.memory.memory_id.clone()));
        }
        pages += 1;
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len() as u64, total.unwrap());
    (ids, total.unwrap(), pages)
}
#[test]
fn paging_counts_byte_pressure_lock_and_all_cursor_bindings_are_native() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 180);
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let req = request(&p, &f, Sensitivity::Private);
    let (ids, count, pages) = all(&r, &req);
    assert_eq!(count, 180);
    assert!(pages > 1);
    assert_eq!(
        ids,
        receipts
            .iter()
            .map(|r| r.result.memory_id.clone())
            .collect()
    );
    let public = request(&p, &f, Sensitivity::Public);
    assert_eq!(all(&r, &public).1, 180 - 180 / 7);
    let page = r.memory_page_native(&req, None).unwrap();
    let cursor = page.next.unwrap();
    assert!(r.memory_page_native(&public, Some(&cursor)).is_err());
    let mut changed = f.clone();
    changed.text = "散步".into();
    assert!(
        r.memory_page_native(&request(&p, &changed, Sensitivity::Private), Some(&cursor))
            .is_err()
    );
    let mut other = p.clone();
    other.revision = Revision::new(p.revision.get() + 1).unwrap();
    assert!(
        r.memory_page_native(&request(&other, &f, Sensitivity::Private), Some(&cursor))
            .is_err()
    );
    let mut small = request(&p, &f, Sensitivity::Private);
    small.options.max_bytes = 8192;
    assert!(r.memory_page_native(&small, None).unwrap().next.is_some());
    small.options.max_bytes = 1024;
    assert!(r.memory_page_native(&small, None).is_err());
    r.lock().unwrap();
    assert!(matches!(
        r.memory_page_native(&req, None),
        Err(ApplicationFault::Locked)
    ));
    r.unlock(&e.descriptor.system_actor).unwrap();
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipts[0].result.memory_id.clone()],
    )
    .unwrap();
    assert!(r.memory_page_native(&req, Some(&cursor)).is_err());
    assert_eq!(all(&r, &req).1, 179);
    let mut revoked = e.policy(1, 1, 64);
    revoked.revision = Revision::new(2).unwrap();
    revoked.active = false; // fixture policy ID follows its first source
    v.set_policy(&e.descriptor.created_by, revoked).unwrap();
    assert_eq!(all(&r, &req).1, 116);
}
fn commit(e: &Env, v: &SegmentedAutomationVault, input: AdmissionInput) -> MemoryReceipt {
    let request = v.prepare(&e.descriptor.system_actor, input).unwrap();
    let response =
        serde_json::to_string(&v.evaluate(&e.descriptor.system_actor, &request).unwrap()).unwrap();
    let binding = GenerationBinding {
        operation_id: request.operation_id.clone(),
        input_hash: digest(&request),
        response_hash: sha256(response.as_bytes()),
    };
    v.apply_batch(
        &e.descriptor.system_actor,
        vec![request],
        vec![response],
        vec![binding],
    )
    .unwrap()
    .remove(0)
}
#[test]
fn bilingual_alias_entities_projects_themes_time_and_facets_share_authorization() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 4);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let a = e.source(1, Sensitivity::Public);
    let b = e.source(2, Sensitivity::Public);
    let mut c = e.source(3, Sensitivity::Public);
    c.text = "Synthetic item 3: I work on Project Cedar, also called 雪松.".into();
    c.record.content_hash = sha256(c.text.as_bytes());
    let mut hidden = e.source(4, Sensitivity::Private);
    hidden.text = "Synthetic item 4: I know Agent Umbra.".into();
    hidden.record.content_hash = sha256(hidden.text.as_bytes());
    let project_id = ProjectId::parse(&id("prj", 5)).unwrap();
    let project_entity = id("ent", 5);
    let private_entity = id("ent", 6);
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.conversations.clear();
    let mut entity = catalog.entities.remove(0);
    entity.entity_id = project_entity.clone();
    entity.entity_type = EntityType::Project;
    entity.project_id = Some(project_id.clone());
    entity.display_name = "Project Cedar".into();
    entity.sensitivity = Sensitivity::Public;
    let quoted = e.input(&c, &p, 3).record_fields.unwrap().evidence;
    entity.evidence = quoted.clone();
    entity.aliases = vec![EntityAlias {
        name: "雪松".into(),
        evidence: quoted.clone(),
    }];
    let mut secret = entity.clone();
    secret.entity_id = private_entity.clone();
    secret.entity_type = EntityType::Person;
    secret.project_id = None;
    secret.display_name = "Agent Umbra".into();
    secret.sensitivity = Sensitivity::Private;
    secret.evidence = e.input(&hidden, &p, 4).record_fields.unwrap().evidence;
    secret.aliases = vec![EntityAlias {
        name: "classified-alias".into(),
        evidence: secret.evidence.clone(),
    }];
    catalog.entities = vec![entity, secret];
    v.capture(
        &e.descriptor.system_actor,
        vec![a.clone(), b.clone(), c.clone(), hidden.clone()],
        catalog,
    )
    .unwrap();
    let one = commit(&e, &v, e.input(&a, &p, 1));
    let two = commit(&e, &v, e.input(&b, &p, 2));
    let mut project_policy = e.policy(2, 3, 1);
    project_policy.project_id = Some(project_id.clone());
    v.set_policy(&e.descriptor.created_by, project_policy.clone())
        .unwrap();
    let mut input = e.input(&c, &project_policy, 3);
    input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
        entity_id: project_entity.clone(),
        role: EntityRole::Project,
        evidence: quoted,
    }];
    commit(&e, &v, input);
    let mut input = e.input(&hidden, &p, 4);
    let evidence = input.record_fields.as_ref().unwrap().evidence.clone();
    input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
        entity_id: private_entity.clone(),
        role: EntityRole::Mentioned,
        evidence,
    }];
    let private_receipt = commit(&e, &v, input);
    let parents = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |r| {
            r.memories
                .iter()
                .filter(|m| m.receipt.result == one.result || m.receipt.result == two.result)
                .map(|m| m.target.clone())
                .collect::<Vec<_>>()
        })
        .unwrap();
    let mut input = e.input(&a, &p, 1);
    input.input_refs.push(b.reference());
    input.context_source_refs = vec![b.reference()];
    input.interpretation = Interpretation::Inference;
    let d = input.dimensions.as_mut().unwrap();
    d.kind = FormationKind::Theme;
    d.change = FormationChange::Synthesis;
    d.derived_from = parents;
    input.content = "System interpretation: a shared interest in walks.".into();
    input.record_fields.as_mut().unwrap().title = "Shared walks".into();
    let theme = commit(&e, &v, input);
    let r = reader(&e, v.clone());
    let mut profile = profile();
    profile.protected_ids.push(private_receipt.result.memory_id);
    profile.aliases.push(RecallAlias {
        cue: "nightwalk".into(),
        terms: vec!["散步".into(), "moonlight".into()],
    });
    let f = CatalogFilter::default();
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 4);
    let mut f = CatalogFilter {
        text: "nightwalk".into(),
        ..Default::default()
    };
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 2);
    f.text = "雪松".into();
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 1);
    f.text = "classified-alias".into();
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 0);
    f = CatalogFilter {
        entity: Some(private_entity),
        ..Default::default()
    };
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 0);
    f.entity = Some(project_entity);
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 1);
    f = CatalogFilter {
        project: Some(project_id),
        ..Default::default()
    };
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 1);
    f = CatalogFilter {
        theme: Some(theme.result.memory_id),
        ..Default::default()
    };
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 2);
    f = CatalogFilter {
        time_axis: Some(CatalogTimeAxis::Event),
        time_window: Some(FormationTime {
            from: Timestamp::from_unix_ms(NOW - 1000).unwrap(),
            until: Timestamp::from_unix_ms(NOW + 1),
        }),
        ..Default::default()
    };
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 0);
    f.include_unknown_time = true;
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 4);
    f = CatalogFilter::default();
    let req = request(&profile, &f, Sensitivity::Public);
    let stats = r.memory_stats_native(&req, None).unwrap();
    assert_eq!(stats.matched_in_scope, 4);
    assert!(stats.validate().is_empty());
    assert!(
        !String::from_utf8(canonical_bytes(&stats).unwrap())
            .unwrap()
            .contains("Umbra")
    );
    let entities = r
        .memory_facets_native(
            &req,
            CatalogFacetAxis::Entity,
            FacetCountScope::OtherFilters,
            None,
            Some(&stats.selection),
        )
        .unwrap();
    assert_eq!(entities.group.total_values, 1);
    assert_eq!(entities.group.values[0].count, 1);
    let forms = r
        .memory_facets_native(
            &req,
            CatalogFacetAxis::Form,
            FacetCountScope::Matched,
            None,
            Some(&stats.selection),
        )
        .unwrap();
    assert_eq!(forms.group.values.iter().map(|v| v.count).sum::<u64>(), 4);
    v.delete(&e.descriptor.created_by, vec![c.record.source_id], vec![])
        .unwrap();
    assert!(r.memory_stats_native(&req, Some(&stats.selection)).is_err());
    f.text = "雪松".into();
    assert_eq!(all(&r, &request(&profile, &f, Sensitivity::Public)).1, 0);
}
#[test]
#[ignore = "explicit 20,000-record native synthetic traversal; run with --ignored --nocapture"]
fn twenty_thousand_native_records_paginate_exactly_and_survive_reopen() {
    let started = Instant::now();
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 20_000);
    let write_elapsed = started.elapsed();
    let p = profile();
    let f = CatalogFilter::default();
    let r = reader(&e, v.clone());
    let req = request(&p, &f, Sensitivity::Private);
    let (ids, count, pages) = all(&r, &req);
    assert_eq!(count, 20_000);
    assert_eq!(
        ids,
        receipts
            .iter()
            .map(|r| r.result.memory_id.clone())
            .collect()
    );
    for filter in [
        CatalogFilter {
            text: "散步".into(),
            ..Default::default()
        },
        CatalogFilter {
            form: Some(FormationKind::Fact),
            ..Default::default()
        },
        CatalogFilter {
            adoption: Some(AdoptionState::Staged),
            ..Default::default()
        },
    ] {
        let q = request(&p, &filter, Sensitivity::Public);
        let (_, count, _) = all(&r, &q);
        assert_eq!(
            r.memory_stats_native(&q, None).unwrap().matched_in_scope,
            count
        );
    }
    let page = r.memory_page_native(&req, None).unwrap();
    let cursor = page.next.unwrap();
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipts[0].result.memory_id.clone()],
    )
    .unwrap();
    assert!(r.memory_page_native(&req, Some(&cursor)).is_err());
    let cursor = r.memory_page_native(&req, None).unwrap().next.unwrap();
    let mut policy = e.policy(1, 1, 64);
    policy.revision = Revision::new(2).unwrap();
    policy.active = false;
    v.set_policy(&e.descriptor.created_by, policy).unwrap();
    assert!(r.memory_page_native(&req, Some(&cursor)).is_err());
    let expected = r.memory_stats_native(&req, None).unwrap().matched_in_scope;
    drop(r);
    drop(v);
    let reopened = Arc::new(e.open());
    let r = reader(&e, reopened);
    assert_eq!(all(&r, &req).1, expected);
    println!(
        "native records=20000, initial_pages={pages}, write_time={write_elapsed:?}, total_time={:?}, remaining_after_delete_and_revoke={expected}",
        started.elapsed()
    );
}

#[test]
fn entity_aliases_require_the_memorys_own_policy_not_an_unrelated_grant() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let mut own = e.policy(1, 1, 1);
    let foreign = e.policy(2, 2, 1);
    v.set_policy(&e.descriptor.created_by, own.clone()).unwrap();
    v.set_policy(&e.descriptor.created_by, foreign.clone())
        .unwrap();
    let mut a = e.source(1, Sensitivity::Public);
    a.text = "Synthetic item 1 mentions Cedar.".into();
    a.record.content_hash = sha256(a.text.as_bytes());
    let mut b = e.source(2, Sensitivity::Public);
    b.text = "Synthetic item 2 records the alias foreign-alias for Cedar.".into();
    b.record.content_hash = sha256(b.text.as_bytes());
    let evidence = e.input(&b, &foreign, 2).record_fields.unwrap().evidence;
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.conversations.clear();
    let entity = &mut catalog.entities[0];
    entity.display_name = "Cedar".into();
    entity.sensitivity = Sensitivity::Public;
    entity.evidence = evidence.clone();
    entity.aliases = vec![EntityAlias {
        name: "foreign-alias".into(),
        evidence,
    }];
    let entity_id = entity.entity_id.clone();
    v.capture(
        &e.descriptor.system_actor,
        vec![a.clone(), b.clone()],
        catalog,
    )
    .unwrap();
    let mut input = e.input(&a, &own, 1);
    let support = input.record_fields.as_ref().unwrap().evidence.clone();
    input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
        entity_id,
        role: EntityRole::Mentioned,
        evidence: support,
    }];
    let before = v.snapshot().unwrap();
    assert!(
        v.prepare(&e.descriptor.system_actor, input.clone())
            .is_err()
    );
    assert_eq!(v.snapshot().unwrap(), before);
    own.revision = Revision::new(2).unwrap();
    own.source_ids.push(b.record.source_id);
    v.set_policy(&e.descriptor.created_by, own).unwrap();
    commit(&e, &v, input);
    let r = reader(&e, v.clone());
    let p = profile();
    let filter = CatalogFilter {
        text: "foreign-alias".into(),
        ..Default::default()
    };
    assert_eq!(all(&r, &request(&p, &filter, Sensitivity::Public)).1, 1);
    let mut revoked = foreign;
    revoked.revision = Revision::new(2).unwrap();
    revoked.active = false;
    v.set_policy(&e.descriptor.created_by, revoked).unwrap();
    assert_eq!(all(&r, &request(&p, &filter, Sensitivity::Public)).1, 1);
}

#[test]
fn stale_indirect_lineage_cannot_be_used_for_a_fresh_summary() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 3);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let sources: Vec<_> = (1..=3).map(|n| e.source(n, Sensitivity::Public)).collect();
    v.capture(
        &e.descriptor.system_actor,
        sources.clone(),
        RecordCatalog::default(),
    )
    .unwrap();
    let a = commit(&e, &v, e.input(&sources[0], &p, 1));
    let target = v
        .read(&e.descriptor.system_actor, Sensitivity::Public, |r| {
            r.memories[0].target.clone()
        })
        .unwrap();
    let mut theme = e.input(&sources[0], &p, 1);
    theme.interpretation = Interpretation::Inference;
    let d = theme.dimensions.as_mut().unwrap();
    d.kind = FormationKind::Theme;
    d.change = FormationChange::Synthesis;
    d.derived_from = vec![target.clone()];
    let b = commit(&e, &v, theme.clone());
    let b_target = v
        .read(&e.descriptor.system_actor, Sensitivity::Public, |r| {
            r.memories
                .iter()
                .find(|m| m.receipt.result == b.result)
                .unwrap()
                .target
                .clone()
        })
        .unwrap();
    let mut correction = e.input(&sources[2], &p, 3);
    correction.target = Some(target);
    correction.dimensions.as_mut().unwrap().change = FormationChange::Correction;
    let revised = commit(&e, &v, correction);
    assert_eq!(revised.result.memory_id, a.result.memory_id);
    assert_eq!(revised.result.revision.get(), 2);
    assert_eq!(
        v.read(&e.descriptor.system_actor, Sensitivity::Public, |r| r
            .memories
            .len())
            .unwrap(),
        1
    );
    theme.dimensions.as_mut().unwrap().derived_from = vec![b_target];
    let before = v.snapshot().unwrap();
    assert!(v.prepare(&e.descriptor.system_actor, theme).is_err());
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn time_axes_keep_reported_events_unknown_events_learning_and_capture_distinct() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 2);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let a = e.source(1, Sensitivity::Public);
    let mut b = e.source(2, Sensitivity::Public);
    b.text = "合成条目 2：我在六月十日散步。".into();
    b.record.content_hash = sha256(b.text.as_bytes());
    v.capture(
        &e.descriptor.system_actor,
        vec![a.clone(), b.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    commit(&e, &v, e.input(&a, &p, 1));
    let from = Timestamp::parse("2026-06-10T00:00:00.000Z").unwrap();
    let until = Timestamp::parse("2026-06-11T00:00:00.000Z").unwrap();
    let mut input = e.input(&b, &p, 2);
    let d = input.dimensions.as_mut().unwrap();
    d.kind = FormationKind::Episode;
    d.occurrence = OccurrenceAssertion::ReportedOccurred;
    d.event_time = Some(FormationTime {
        from: from.clone(),
        until: Some(until.clone()),
    });
    input.record_fields.as_mut().unwrap().event_time = RecordTime {
        from: Some(from.clone()),
        until: Some(until.clone()),
        precision: RecordPrecision::Day,
        bound: TimeBound::Within,
        tz: Some("Etc/UTC".into()),
        basis: TimeBasis::Stated,
    };
    commit(&e, &v, input);
    let r = reader(&e, v);
    let profile = profile();
    let mut filter = CatalogFilter {
        time_axis: Some(CatalogTimeAxis::Event),
        time_window: Some(FormationTime {
            from,
            until: Some(until),
        }),
        ..Default::default()
    };
    let page = r
        .memory_page_native(&request(&profile, &filter, Sensitivity::Public), None)
        .unwrap();
    assert_eq!(page.matched_in_scope, 1);
    assert_eq!(page.unknown_time_in_scope, 1);
    filter.time_axis = Some(CatalogTimeAxis::Learned);
    assert_eq!(
        all(&r, &request(&profile, &filter, Sensitivity::Public)).1,
        0
    );
    filter.time_axis = Some(CatalogTimeAxis::SourceCaptured);
    assert_eq!(
        all(&r, &request(&profile, &filter, Sensitivity::Public)).1,
        0
    );
    filter.time_window = Some(FormationTime {
        from: Timestamp::from_unix_ms(NOW).unwrap(),
        until: Timestamp::from_unix_ms(NOW + 1),
    });
    assert_eq!(
        all(&r, &request(&profile, &filter, Sensitivity::Public)).1,
        2
    );
    filter.time_axis = Some(CatalogTimeAxis::Event);
    assert_eq!(
        all(&r, &request(&profile, &filter, Sensitivity::Public)).1,
        0
    );
    filter.include_unknown_time = true;
    assert_eq!(
        all(&r, &request(&profile, &filter, Sensitivity::Public)).1,
        1
    );
}

#[test]
fn facet_values_progress_with_exact_counts_and_reads_do_not_publish_artifacts() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 24);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let sources: Vec<_> = (1..=24).map(|n| e.source(n, Sensitivity::Public)).collect();
    let template: RecordCatalog = serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    let mut catalog = RecordCatalog::default();
    for (index, source) in sources.iter().enumerate() {
        let mut entity = template.entities[0].clone();
        entity.entity_id = id("ent", index as u32 + 1);
        entity.display_name = format!("Synthetic entity {}", index + 1);
        entity.reality = EntityReality::Unknown;
        entity.sensitivity = Sensitivity::Public;
        entity.aliases.clear();
        entity.evidence = e
            .input(source, &p, index as u32 + 1)
            .record_fields
            .unwrap()
            .evidence;
        catalog.entities.push(entity);
    }
    v.capture(&e.descriptor.system_actor, sources.clone(), catalog)
        .unwrap();
    for (index, source) in sources.iter().enumerate() {
        let mut input = e.input(source, &p, index as u32 + 1);
        let evidence = input.record_fields.as_ref().unwrap().evidence.clone();
        input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
            entity_id: id("ent", index as u32 + 1),
            role: EntityRole::Mentioned,
            evidence,
        }];
        commit(&e, &v, input);
    }
    fn files(path: &std::path::Path) -> std::collections::BTreeMap<std::path::PathBuf, Vec<u8>> {
        let mut out = std::collections::BTreeMap::new();
        for entry in std::fs::read_dir(path).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                out.extend(files(&path));
            } else {
                // The lifetime lease is intentionally unreadable while held on Windows.
                let content = if path.file_name().unwrap() == "host.lock" {
                    vec![]
                } else {
                    std::fs::read(&path).unwrap()
                };
                out.insert(path, content);
            }
        }
        out
    }
    let before = files(&e.path);
    let r = reader(&e, v.clone());
    let profile = profile();
    let filter = CatalogFilter::default();
    let mut req = request(&profile, &filter, Sensitivity::Public);
    req.options.include_facets = true;
    let page = r.memory_page_native(&req, None).unwrap();
    let embedded = page
        .facets
        .iter()
        .find(|g| g.axis == CatalogFacetAxis::Entity)
        .unwrap();
    assert_eq!(embedded.total_values, 24);
    assert_eq!(embedded.values.len(), 16);
    assert!(embedded.next.is_some());
    req.options.limit = 5;
    let mut cursor = None;
    let mut values = BTreeSet::new();
    loop {
        let facets = r
            .memory_facets_native(
                &req,
                CatalogFacetAxis::Entity,
                FacetCountScope::OtherFilters,
                cursor.as_ref(),
                Some(&page.selection),
            )
            .unwrap();
        assert_eq!(facets.group.total_values, 24);
        for value in facets.group.values {
            assert_eq!(value.count, 1);
            assert!(values.insert(value.value));
        }
        cursor = facets.group.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(values.len(), 24);
    assert_eq!(
        r.memory_stats_native(&req, Some(&page.selection))
            .unwrap()
            .matched_in_scope,
        24
    );
    assert_eq!(files(&e.path), before);
    let first = r
        .memory_facets_native(
            &req,
            CatalogFacetAxis::Entity,
            FacetCountScope::OtherFilters,
            None,
            None,
        )
        .unwrap();
    v.delete(
        &e.descriptor.created_by,
        vec![sources[16].record.source_id.clone()],
        vec![],
    )
    .unwrap();
    assert!(
        r.memory_facets_native(
            &req,
            CatalogFacetAxis::Entity,
            FacetCountScope::OtherFilters,
            first.group.next.as_ref(),
            None
        )
        .is_err()
    );
    assert_eq!(
        r.memory_facets_native(
            &req,
            CatalogFacetAxis::Entity,
            FacetCountScope::OtherFilters,
            None,
            None
        )
        .unwrap()
        .group
        .total_values,
        23
    );
}

#[test]
fn an_entity_quote_cannot_borrow_another_subjects_policy() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let own = e.policy(1, 1, 2);
    let mut foreign = e.policy(2, 2, 1);
    foreign.subject_id = enouia_memory_contract::ids::SubjectId::parse(&id("sub", 99)).unwrap();
    v.set_policy(&e.descriptor.created_by, own.clone()).unwrap();
    v.set_policy(&e.descriptor.created_by, foreign.clone())
        .unwrap();
    let a = e.source(1, Sensitivity::Public);
    let mut b = e.source(2, Sensitivity::Public);
    b.attributed_subject = Some(foreign.subject_id.clone());
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.conversations.clear();
    catalog.entities.truncate(1);
    let entity = &mut catalog.entities[0];
    entity.sensitivity = Sensitivity::Public;
    entity.aliases.clear();
    entity.evidence = e.input(&b, &foreign, 2).record_fields.unwrap().evidence;
    let entity_id = entity.entity_id.clone();
    v.capture(&e.descriptor.system_actor, vec![a.clone(), b], catalog)
        .unwrap();
    let mut input = e.input(&a, &own, 1);
    input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
        entity_id,
        role: EntityRole::Mentioned,
        evidence: input.record_fields.as_ref().unwrap().evidence.clone(),
    }];
    assert!(v.prepare(&e.descriptor.system_actor, input).is_err());
}

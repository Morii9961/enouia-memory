#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*, hash::sha256, ids::*, json::Revision, next::*, time::Timestamp,
};
use enouia_memory_host::automation::{owner::NativeMemoryOwner, qualified_agent::*};
use enouia_memory_vault::{
    automation::segmented::{SegmentedAgentScope, SegmentedAutomationVault},
    fault::Faults,
};
use fixture::*;
use serde_json::Value;
use std::sync::Arc;

fn env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    e
}
fn agent(n: u32) -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", n)).unwrap(),
    }
}
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}
fn scope(e: &Env, ids: &[u32]) -> SegmentedAgentScope {
    let p = e.policy(1, 1, 8);
    SegmentedAgentScope {
        subject_id: p.subject_id,
        project_id: p.project_id,
        policy_ids: vec![p.policy_id],
        source_ids: ids
            .iter()
            .map(|n| SourceId::parse(&id("src", *n)).unwrap())
            .collect(),
        max_sensitivity: Sensitivity::Private,
        source_read: true,
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn target(v: &SegmentedAutomationVault, e: &Env, receipt: &MemoryReceipt) -> TargetRef {
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
        data.memories
            .iter()
            .find(|m| m.memory.memory == receipt.result)
            .unwrap()
            .target
            .clone()
    })
    .unwrap()
}
fn execute(h: &NativeMemoryOwner, action: OwnerAction) {
    let preview = h.plan(action, Sensitivity::Private, None).unwrap();
    let consent = h
        .confirm_from_owner_gesture(
            &preview.plan.operation_id,
            &digest(&preview.plan),
            &preview.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    h.commit(consent).unwrap();
}
fn no_global_sequence(value: &Value) {
    match value {
        Value::Object(map) => {
            assert!(!map.contains_key("sequence"));
            for v in map.values() {
                no_global_sequence(v)
            }
        }
        Value::Array(items) => {
            for v in items {
                no_global_sequence(v)
            }
        }
        _ => {}
    }
}
#[test]
fn qualified_pages_keep_full_fields_owner_marks_scoped_counts_and_no_system_envelope() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 8);
    let owner =
        NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let visible = target(&v, &e, &receipts[0]);
    let hidden = target(&v, &e, &receipts[7]);
    execute(
        &owner,
        OwnerAction::Tag {
            name: "HIDDEN_OWNER_LABEL".into(),
            targets: vec![hidden.clone()],
        },
    );
    execute(
        &owner,
        OwnerAction::Tag {
            name: "visible-owned-tag".into(),
            targets: vec![visible, hidden.clone()],
        },
    );
    let h =
        QualifiedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let r = h
        .reader(&e.descriptor.created_by, agent(77), scope(&e, &[1, 2]))
        .unwrap();
    let before = v.snapshot().unwrap();
    let page = r
        .page(
            &profile(),
            &CatalogFilter::default(),
            None,
            100,
            64 * 1024,
            true,
        )
        .unwrap();
    assert_eq!(page.capability, CAPABILITY);
    assert_eq!(page.principal, agent(77));
    assert_eq!(page.selection.algorithm, CatalogReadAlgorithm::V4);
    assert_eq!(&page.selection.scope_hash, r.scope_hash());
    assert_eq!(page.matched_in_scope, 2);
    assert!(page.count_exact && page.complete_in_scope);
    let first = page
        .items
        .iter()
        .find(|i| i.base.base.memory == receipts[0].result)
        .unwrap();
    assert_eq!(
        first.base.owner_marks.as_ref().unwrap().tags[0].name,
        "visible-owned-tag"
    );
    assert_eq!(
        first.base.base.knowledge,
        KnowledgeState::AttributedStatement
    );
    assert!(!first.record.fields.evidence.is_empty());
    assert_eq!(first.record.fields.title, first.base.title);
    let output = serde_json::to_value(page).unwrap();
    let encoded = output.to_string();
    for denied in [
        hidden.memory.memory_id.as_str(),
        "HIDDEN_OWNER_LABEL",
        "memory-native-read-2",
    ] {
        assert!(!encoded.contains(denied), "denied data in Agent page");
    }
    no_global_sequence(&output);
    for _ in 0..100 {
        r.page(
            &profile(),
            &CatalogFilter::default(),
            None,
            1,
            16 * 1024,
            false,
        )
        .unwrap();
    }
    assert_eq!(v.snapshot().unwrap(), before);
}
#[test]
fn page_cursors_bind_delegation_query_snapshot_and_actual_output_byte_budget() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 8);
    let h =
        QualifiedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let ids: Vec<_> = (1..=8).collect();
    let a = h
        .reader(&e.descriptor.created_by, agent(77), scope(&e, &ids))
        .unwrap();
    let b = h
        .reader(&e.descriptor.created_by, agent(78), scope(&e, &ids))
        .unwrap();
    let p = profile();
    let f = CatalogFilter::default();
    let first = a.page(&p, &f, None, 1, 16 * 1024, false).unwrap();
    let cursor = first.next.unwrap();
    assert!(b.page(&p, &f, Some(&cursor), 1, 16 * 1024, false).is_err());
    let changed = CatalogFilter {
        text: "moonlight".into(),
        ..Default::default()
    };
    assert!(
        a.page(&p, &changed, Some(&cursor), 1, 16 * 1024, false)
            .is_err()
    );
    let mut next = None;
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let page = a
            .page(&p, &f, next.as_ref(), 100, 16 * 1024, false)
            .unwrap();
        assert!(
            enouia_memory_contract::json::canonical_bytes(&page)
                .unwrap()
                .len()
                <= 16 * 1024
        );
        for item in &page.items {
            assert!(seen.insert(item.base.base.memory.memory_id.clone()));
        }
        next = page.next;
        if next.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 8);
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipts[0].result.memory_id.clone()],
    )
    .unwrap();
    assert!(a.page(&p, &f, Some(&cursor), 1, 16 * 1024, false).is_err());
    assert_eq!(
        a.page(&p, &f, None, 100, 64 * 1024, false)
            .unwrap()
            .matched_in_scope,
        7
    );
}
#[test]
fn qualified_readers_refuse_old_roots_wrong_roles_policy_replacement_and_old_generations() {
    let old = Env::new();
    let old_v = Arc::new(old.create(Faults::none()));
    assert!(
        QualifiedAgentHost::from_authenticated_owner(old_v, old.descriptor.created_by.clone())
            .is_err()
    );
    for action in ["revoke", "expiry", "lock", "policy"] {
        let e = env();
        let v = Arc::new(e.create(Faults::none()));
        e.populate(&v, 8);
        let h = QualifiedAgentHost::from_authenticated_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let mut forged = agent(77);
        forged.actor_type = ActorType::System;
        assert!(
            h.reader(&e.descriptor.created_by, forged, scope(&e, &[1]))
                .is_err()
        );
        let r = h
            .reader(&e.descriptor.created_by, agent(77), scope(&e, &[1]))
            .unwrap();
        match action {
            "revoke" => r.revoke(),
            "expiry" => e.clock.set(NOW + 120_000),
            "lock" => {
                h.lock().unwrap();
                h.unlock(&e.descriptor.created_by).unwrap();
            }
            _ => {
                let mut p = e.policy(1, 1, 8);
                p.revision = Revision::new(2).unwrap();
                v.set_policy(&e.descriptor.created_by, p).unwrap();
            }
        }
        assert!(
            r.page(
                &profile(),
                &CatalogFilter::default(),
                None,
                10,
                16 * 1024,
                false
            )
            .is_err()
        );
    }
}
fn publish(e: &Env, v: &SegmentedAutomationVault, input: AdmissionInput) -> MemoryReceipt {
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
#[test]
fn owner_entity_redirects_and_aliases_are_rebuilt_after_agent_source_qualification() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 3);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let sources: Vec<_> = (1..=3).map(|n| e.source(n, Sensitivity::Public)).collect();
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.conversations.clear();
    let template = catalog.entities.remove(0);
    catalog.entities.clear();
    let a = id("ent", 500);
    let b = id("ent", 501);
    let mut inputs: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(n, s)| e.input(s, &p, n as u32 + 1))
        .collect();
    for (n, name, id) in [(0, "Cedar", a.clone()), (1, "Fir", b.clone())] {
        let mut ent = template.clone();
        ent.entity_id = id;
        ent.display_name = name.into();
        ent.aliases.clear();
        ent.sensitivity = Sensitivity::Public;
        ent.evidence = inputs[n].record_fields.as_ref().unwrap().evidence.clone();
        catalog.entities.push(ent);
    }
    v.capture(&e.descriptor.system_actor, sources, catalog)
        .unwrap();
    for (n, input) in inputs.iter_mut().enumerate() {
        input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
            entity_id: if n == 1 { b.clone() } else { a.clone() },
            role: EntityRole::Mentioned,
            evidence: input.record_fields.as_ref().unwrap().evidence.clone(),
        }];
        publish(&e, &v, input.clone());
    }
    let owner =
        NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    execute(
        &owner,
        OwnerAction::EntityMerge {
            from_entity: a.clone(),
            into_entity: b.clone(),
            reason: None,
        },
    );
    let h =
        QualifiedAgentHost::from_authenticated_owner(v, e.descriptor.created_by.clone()).unwrap();
    let mut broad = scope(&e, &[1, 2, 3]);
    broad.policy_ids = vec![p.policy_id.clone()];
    let wide = h
        .reader(&e.descriptor.created_by, agent(77), broad.clone())
        .unwrap();
    let f = CatalogFilter {
        text: "Fir".into(),
        ..Default::default()
    };
    assert_eq!(
        wide.page(&profile(), &f, None, 100, 64 * 1024, true)
            .unwrap()
            .matched_in_scope,
        3
    );
    broad.source_ids = vec![sources_ref(1), sources_ref(3)];
    let narrow = h
        .reader(&e.descriptor.created_by, agent(78), broad)
        .unwrap();
    assert_eq!(
        narrow
            .page(&profile(), &f, None, 100, 64 * 1024, true)
            .unwrap()
            .matched_in_scope,
        0
    );
    let out = narrow
        .page(
            &profile(),
            &CatalogFilter::default(),
            None,
            100,
            64 * 1024,
            true,
        )
        .unwrap();
    assert_eq!(out.matched_in_scope, 2);
    let value = serde_json::to_value(out).unwrap();
    let text = value.to_string();
    assert!(!text.contains("Fir") && !text.contains(&b));
    no_global_sequence(&value);
}
fn sources_ref(n: u32) -> SourceId {
    SourceId::parse(&id("src", n)).unwrap()
}

#[test]
fn real_owner_correction_needs_current_source_policy_and_stays_an_attributed_statement() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let receipts = e.populate(&v, 8);
    let h =
        QualifiedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let old = h
        .reader(&e.descriptor.created_by, agent(77), scope(&e, &[1]))
        .unwrap();
    let target = target(&v, &e, &receipts[0]);
    let mut input = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            data.memories
                .iter()
                .find(|m| m.target == target)
                .unwrap()
                .memory
                .decision
                .input
                .clone()
        })
        .unwrap();
    let mut dimensions = input.dimensions.take().unwrap();
    dimensions.change = FormationChange::Correction;
    dimensions.event_time = None;
    dimensions.valid_time = None;
    dimensions.derived_from.clear();
    dimensions.counter_sources.clear();
    dimensions.reverts_to = None;
    let unknown = RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    };
    let owner =
        NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let preview = owner
        .plan(
            OwnerAction::Correct {
                target,
                source_id: sources_ref(999),
                assertion_text: "I prefer mornings; synthetic native assertion.".into(),
            },
            Sensitivity::Private,
            Some(OwnerCorrectionMetadata {
                title: "Native owner statement".into(),
                content_lang: "en".into(),
                sensitivity: Sensitivity::Public,
                interpretation: Interpretation::ClearSelfReport,
                dimensions,
                event_time: unknown.clone(),
                valid_time: unknown,
                entities: vec![],
            }),
        )
        .unwrap();
    let consent = owner
        .confirm_from_owner_gesture(
            &preview.plan.operation_id,
            &digest(&preview.plan),
            &preview.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    owner.commit(consent).unwrap();
    let f = CatalogFilter {
        text: "synthetic native assertion".into(),
        ..Default::default()
    };
    assert_eq!(
        old.page(&profile(), &f, None, 10, 64 * 1024, false)
            .unwrap()
            .matched_in_scope,
        0
    );
    let current = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |data| {
            data.memories
                .iter()
                .find(|m| m.memory.memory.memory_id == receipts[0].result.memory_id)
                .unwrap()
                .memory
                .decision
                .input
                .clone()
        })
        .unwrap();
    let mut selected = scope(&e, &[999]);
    selected.policy_ids = vec![current.policy.policy_id];
    // Native planning allocates the actual source ID; the request's source ID
    // is only proposed data and cannot be used to manufacture an authorization.
    selected.source_ids = current
        .input_refs
        .iter()
        .map(|r| r.source_id.clone())
        .collect();
    // The correction's reserved policy is Public; a broader requested ceiling
    // must refuse, rather than silently borrowing the original policy's limit.
    assert!(
        h.reader(&e.descriptor.created_by, agent(78), selected.clone())
            .is_err()
    );
    selected.max_sensitivity = Sensitivity::Public;
    assert_eq!(
        selected.subject_id, current.subject_id,
        "selected correction subject"
    );
    assert_eq!(
        selected.project_id, current.project_id,
        "selected correction project"
    );
    let reader = h
        .reader(&e.descriptor.created_by, agent(78), selected)
        .unwrap();
    let page = reader
        .page(&profile(), &f, None, 10, 64 * 1024, false)
        .unwrap();
    assert_eq!(page.matched_in_scope, 1);
    let row = &page.items[0];
    assert_eq!(row.base.base.knowledge, KnowledgeState::AttributedStatement);
    assert!(row.base.owner_marks.as_ref().unwrap().corrected.is_some());
    assert_eq!(
        row.record.fields.formation.prompt_version,
        "owner-native-literal-1"
    );
    assert_eq!(row.record.fields.evidence[0].speaker, SpeakerRole::User);
    assert_eq!(
        row.record.fields.evidence[0].reference.evidence_class,
        EvidenceClass::UserStatement
    );
}

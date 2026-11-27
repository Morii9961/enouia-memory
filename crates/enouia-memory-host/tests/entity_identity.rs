#![cfg(windows)]
//! ADR-MEM-172: entity identity across capture windows. Identical names link
//! automatically as suggestions; Morii confirms or separates records.
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, next::*};
use enouia_memory_host::automation::{
    catalog::*, connector_formation::*, owner::NativeMemoryOwner,
};
use enouia_memory_vault::{
    automation::segmented::{
        EntityIdentities, FormationJobState, IdentityBasis, NativeOwnerPreview,
        SegmentedAutomationVault,
    },
    fault::Faults,
};
use fixture::*;
use std::sync::Arc;

fn owner_env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 8).subject_id);
    e
}
fn writer(v: &Arc<SegmentedAutomationVault>, e: &Env, form: FormationKind) -> ConnectorWriter {
    ConnectorWriter::new(
        v.clone(),
        e.descriptor.created_by.clone(),
        ActorRef {
            actor_type: ActorType::Client,
            actor_id: PrincipalId::parse(&id("prn", 88)).unwrap(),
        },
        ConnectorConfig {
            account_scope: "chatgpt-connector-synthetic".into(),
            default_timezone: "Asia/Shanghai".into(),
            max_sensitivity: Sensitivity::Private,
            allowed_forms: vec![form],
            window_days: 30,
        },
    )
    .unwrap()
}
fn submit(w: &ConnectorWriter, n: u32, text: &str, quote: &str, name: &str, form: FormationKind) {
    let v: serde_json::Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/runtime-owner-capture/cases.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut r: RuntimeCaptureRequest = serde_json::from_value(v["base"].clone()).unwrap();
    r.request_id = RequestId::parse(&id("req", n)).unwrap();
    r.conversation_id = format!("chatgpt-synthetic-{n}");
    r.turns[0].message_id = format!("m{n}");
    r.turns[0].attachments.clear();
    r.turns[0].text = text.into();
    let unknown = RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    };
    let receipt = w
        .submit(
            &ConnectorSubmission {
                request: r,
                analyst_model: "synthetic-chatgpt-model".into(),
                statements: vec![ConnectorStatement {
                    message_id: format!("m{n}"),
                    quote: quote.into(),
                    title: quote.into(),
                    interpretation: Interpretation::ClearSelfReport,
                    kind: form,
                    event_time: unknown.clone(),
                    valid_time: unknown,
                    note: None,
                    entities: vec![ConnectorEntity {
                        name: name.into(),
                        entity_type: EntityType::Person,
                        reality: EntityReality::Real,
                        role: EntityRole::Participant,
                    }],
                }],
            },
            false,
            NOW,
        )
        .unwrap();
    assert_eq!(receipt.jobs[0].state, FormationJobState::Formed);
}
fn execute(h: &NativeMemoryOwner, p: NativeOwnerPreview) {
    let t = h
        .confirm_from_owner_gesture(
            &p.plan.operation_id,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    let outcome = h.commit(t).unwrap();
    // ADR-MEM-226: the outcome passes its own native check, also on replay.
    assert!(outcome.validate().is_empty(), "{}", outcome.action);
    let replayed = h
        .outcome(&p.plan.operation_id, Sensitivity::Private)
        .unwrap()
        .unwrap();
    assert_eq!(replayed, outcome);
}

#[test]
fn identical_names_link_as_suggestions_until_morii_confirms_or_separates() {
    let e = owner_env();
    let v = Arc::new(e.create(Faults::none()));
    // Different allowed forms, so the memories land in different windows.
    let prefer = writer(&v, &e, FormationKind::Preference);
    let fact = writer(&v, &e, FormationKind::Fact);
    submit(
        &prefer,
        1,
        "I like hiking with Lin.",
        "hiking with Lin",
        "Lin",
        FormationKind::Preference,
    );
    submit(
        &fact,
        2,
        "Lin called me today.",
        "Lin called me today",
        "Lin",
        FormationKind::Fact,
    );
    submit(
        &fact,
        3,
        "Lin Xiao sent a gift.",
        "Lin Xiao sent a gift",
        "Lin Xiao",
        FormationKind::Fact,
    );
    let owner = &e.descriptor.created_by;
    assert_eq!(v.capture_windows(owner).unwrap().len(), 2);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), owner.clone()).unwrap();
    let r = SegmentedMemoryReader::from_authenticated_system(
        v.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let filter = CatalogFilter::default();
    let graph = || {
        let page = r
            .memory_graph_native(
                &CatalogReadRequest {
                    profile: &profile,
                    filter: &filter,
                    options: CatalogReadOptions {
                        limit: 64,
                        max_bytes: MAX_DOCUMENT_BYTES,
                        max_sensitivity: Sensitivity::Private,
                        include_facets: false,
                    },
                },
                &GraphQuery {
                    node_limit: 100,
                    edge_limit_per_kind: 100,
                },
                None,
            )
            .unwrap();
        let titles: std::collections::BTreeMap<_, _> = page
            .nodes
            .iter()
            .map(|n| (n.memory.memory.memory_id.clone(), n.title.clone()))
            .collect();
        let mut edges: Vec<(GraphEdgeKind, String, String, usize)> = page
            .edges
            .iter()
            .filter(|edge| {
                matches!(
                    edge.relation,
                    GraphEdgeKind::SameName | GraphEdgeKind::SameEntity
                )
            })
            .map(|edge| {
                let mut pair = [titles[&edge.from].clone(), titles[&edge.to].clone()];
                pair.sort();
                let [a, b] = pair;
                (edge.relation, a, b, edge.owner_relations.len())
            })
            .collect();
        edges.sort();
        edges
    };
    let identities =
        || -> EntityIdentities { h.entity_identities(Sensitivity::Private, 100).unwrap() };
    // ADR-MEM-224: every catalog read still answers once Morii has confirmed
    // or separated records: the page, each memory's detail and neighbors.
    let reads = || {
        let request = CatalogReadRequest {
            profile: &profile,
            filter: &filter,
            options: CatalogReadOptions {
                limit: 64,
                max_bytes: MAX_DOCUMENT_BYTES,
                max_sensitivity: Sensitivity::Private,
                include_facets: false,
            },
        };
        let page = r.memory_page_native(&request, None).unwrap();
        assert_eq!(page.items.len(), 3);
        for item in &page.items {
            let id = &item.base.memory.memory_id;
            r.memory_detail_native(&request, id, None).unwrap();
            r.memory_neighbors_native(&request, id, true, None).unwrap();
        }
        page
    };
    reads();
    // An Agent reader scoped to both windows sees the same suggestion in
    // `related`, anchored on the first "Lin" memory.
    {
        use enouia_memory_host::automation::qualified_agent::QualifiedAgentHost;
        use enouia_memory_vault::automation::segmented::SegmentedAgentScope;
        let windows = v.capture_windows(owner).unwrap();
        let (memories, sources) = v
            .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
                let memories: std::collections::BTreeMap<String, MemoryId> = d
                    .memories
                    .iter()
                    .map(|m| {
                        (
                            m.memory
                                .request
                                .record_fields
                                .as_ref()
                                .unwrap()
                                .title
                                .clone(),
                            m.memory.memory.memory_id.clone(),
                        )
                    })
                    .collect();
                let sources: Vec<SourceId> = d.sources.keys().cloned().collect();
                (memories, sources)
            })
            .unwrap();
        let agents =
            QualifiedAgentHost::from_authenticated_owner(v.clone(), owner.clone()).unwrap();
        let reader = agents
            .reader(
                owner,
                ActorRef {
                    actor_type: ActorType::Agent,
                    actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
                },
                SegmentedAgentScope {
                    subject_id: windows[0].policy.subject_id.clone(),
                    project_id: None,
                    policy_ids: windows.iter().map(|w| w.policy.policy_id.clone()).collect(),
                    source_ids: sources,
                    max_sensitivity: Sensitivity::Private,
                    source_read: false,
                    valid_until: enouia_memory_contract::time::Timestamp::from_unix_ms(
                        NOW + 120_000,
                    )
                    .unwrap(),
                },
            )
            .unwrap();
        let related = reader
            .related(&profile, &memories["hiking with Lin"], None, 64 * 1024)
            .unwrap()
            .unwrap();
        let same_name: Vec<_> = related["suggested_edges"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|edge| edge["relation"] == "same_name")
            .collect();
        assert_eq!(same_name.len(), 1, "{related}");
        assert_eq!(
            same_name[0]["to"],
            serde_json::json!(memories["Lin called me today"])
        );
        assert_eq!(same_name[0]["witness"], "same_name:Lin");
        assert_eq!(related["edge_coverage"].as_array().unwrap().len(), 6);
    }
    // 1. Identical names across windows: one automatic suggestion.
    assert_eq!(
        graph(),
        vec![(
            GraphEdgeKind::SameName,
            "Lin called me today".into(),
            "hiking with Lin".into(),
            0
        )]
    );
    let groups = identities();
    assert_eq!(groups.groups.len(), 2);
    assert_eq!(groups.groups[0].basis, IdentityBasis::SameName);
    assert_eq!(groups.groups[0].records.len(), 2);
    assert_eq!(groups.groups[1].basis, IdentityBasis::Single);
    assert_eq!(groups.groups[1].name, "Lin Xiao");
    let record = |name: &str, n: usize| {
        identities()
            .groups
            .iter()
            .flat_map(|g| g.records.clone())
            .filter(|r| r.display_name == name)
            .nth(n)
            .unwrap()
            .entity_id
    };
    let (lin_a, lin_b, xiao) = (record("Lin", 0), record("Lin", 1), record("Lin Xiao", 0));
    // A different type or an unknown record cannot be planned.
    assert!(
        h.plan(
            OwnerAction::EntityIdentity {
                entities: vec![xiao.clone(), id("ent", 999)],
                same: true,
                reason: None,
            },
            Sensitivity::Private,
            None,
        )
        .is_err()
    );
    // 2. Morii confirms "Lin Xiao" is the person of the first "Lin" memory.
    let first_lin = graph_owner_of(&v, &e, "hiking with Lin");
    let linked = if first_lin == lin_a { &lin_a } else { &lin_b };
    let other_lin = if first_lin == lin_a { &lin_b } else { &lin_a };
    execute(
        &h,
        h.plan(
            OwnerAction::EntityIdentity {
                entities: vec![xiao.clone(), linked.clone()],
                same: true,
                reason: Some("Lin is Lin Xiao".into()),
            },
            Sensitivity::Private,
            None,
        )
        .unwrap(),
    );
    let page = reads();
    assert!(page.items.iter().any(|item| {
        item.entities.iter().any(|e| {
            e.owner_relation
                .as_ref()
                .is_some_and(|p| p.action == "entity_identity")
        })
    }));
    let edges = graph();
    assert!(edges.contains(&(
        GraphEdgeKind::SameEntity,
        "Lin Xiao sent a gift".into(),
        "hiking with Lin".into(),
        1
    )));
    assert!(edges.contains(&(
        GraphEdgeKind::SameName,
        "Lin called me today".into(),
        "hiking with Lin".into(),
        0
    )));
    let groups = identities();
    assert_eq!(groups.groups.len(), 1);
    assert_eq!(groups.groups[0].basis, IdentityBasis::SameName);
    assert_eq!(groups.groups[0].records.len(), 3);
    // 3. Morii separates the other "Lin": the suggestion goes, the link stays.
    execute(
        &h,
        h.plan(
            OwnerAction::EntityIdentity {
                entities: vec![other_lin.clone(), linked.clone()],
                same: false,
                reason: Some("a different Lin".into()),
            },
            Sensitivity::Private,
            None,
        )
        .unwrap(),
    );
    assert_eq!(
        graph(),
        vec![(
            GraphEdgeKind::SameEntity,
            "Lin Xiao sent a gift".into(),
            "hiking with Lin".into(),
            1
        )]
    );
    reads();
    let groups = identities();
    assert_eq!(groups.groups[0].basis, IdentityBasis::Confirmed);
    assert_eq!(groups.groups[0].records.len(), 2);
    assert_eq!(groups.groups[1].basis, IdentityBasis::Single);
    assert!(groups.groups[1].records[0].opted_out);
}

/// The entity record the memory titled `title` mentions.
fn graph_owner_of(v: &SegmentedAutomationVault, e: &Env, title: &str) -> String {
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        d.memories
            .iter()
            .find(|m| m.memory.request.record_fields.as_ref().unwrap().title == title)
            .unwrap()
            .memory
            .request
            .record_fields
            .as_ref()
            .unwrap()
            .entities[0]
            .entity_id
            .clone()
    })
    .unwrap()
}

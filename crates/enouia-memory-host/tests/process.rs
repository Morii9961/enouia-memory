#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, hash::sha256, ids::*, json::Revision, next::*};
use enouia_memory_host::automation::catalog::*;
use enouia_memory_vault::{automation::segmented::SegmentedAutomationVault, fault::Faults};
use fixture::*;
use std::{collections::BTreeSet, sync::Arc};
fn profile() -> AttentionProfile {
    serde_json::from_value(value("attention/profile.json")).unwrap()
}
fn request<'a>(
    p: &'a AttentionProfile,
    f: &'a CatalogFilter,
    max: Sensitivity,
) -> CatalogReadRequest<'a> {
    CatalogReadRequest {
        profile: p,
        filter: f,
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
fn target(e: &Env, v: &SegmentedAutomationVault, id: &MemoryId) -> TargetRef {
    v.read(
        &e.descriptor.system_actor,
        Sensitivity::HighlySensitive,
        |r| {
            r.memories
                .iter()
                .find(|m| &m.memory.memory.memory_id == id)
                .unwrap()
                .target
                .clone()
        },
    )
    .unwrap()
}
fn note(input: &mut AdmissionInput, text: &str) {
    let f = input.record_fields.as_mut().unwrap();
    f.annotations = Some(FormationAnnotations {
        memory: input.result.clone(),
        model_version: input.model_version.clone(),
        prompt_version: f.formation.prompt_version.clone(),
        formation_note: Some(text.into()),
        suggested_tags: vec![],
        tone: None,
        importance_guess_milli: None,
        current_state_note: None,
        suggested_relations: vec![],
    });
}
#[test]
fn activity_and_changes_are_witnessed_scoped_and_do_not_reveal_hidden_publications() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let policy = e.policy(1, 1, 3);
    v.set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let one = e.source(1, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![one.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let first = commit(&e, &v, e.input(&one, &policy, 1));
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let mut req = request(&p, &f, Sensitivity::Public);
    let initial = r.memory_changes_since_native(&req, None).unwrap();
    assert_eq!(initial.status, ChangeStatus::Initialize);
    let unchanged = r
        .memory_changes_since_native(&req, Some(&initial.next_anchor))
        .unwrap();
    assert_eq!(unchanged.status, ChangeStatus::UnchangedInScope);
    assert!(!unchanged.all_projections_invalidated);
    e.clock.set(NOW + 100);
    let hidden = e.source(2, Sensitivity::Private);
    v.capture(
        &e.descriptor.system_actor,
        vec![hidden.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let private = commit(&e, &v, e.input(&hidden, &policy, 2));
    let hidden_change = r
        .memory_changes_since_native(&req, Some(&initial.next_anchor))
        .unwrap();
    assert_eq!(hidden_change.status, ChangeStatus::UnchangedInScope);
    assert!(hidden_change.memories.is_empty() && hidden_change.sources.is_empty());
    let three = e.source(3, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![three.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let third = commit(&e, &v, e.input(&three, &policy, 3));
    let changed = r
        .memory_changes_since_native(&req, Some(&initial.next_anchor))
        .unwrap();
    assert_eq!(changed.status, ChangeStatus::Changed);
    assert_eq!(changed.memories, vec![third.result.clone()]);
    assert_eq!(changed.sources, vec![three.reference()]);
    assert!(changed.all_projections_invalidated);
    assert_eq!(
        r.memory_changes_since_native(&req, Some(&changed.next_anchor))
            .unwrap()
            .status,
        ChangeStatus::UnchangedInScope
    );
    let query = ActivityQuery {
        memory: None,
        event_kind: None,
        include_sources: true,
    };
    req.options.limit = 1;
    let first_page = r.memory_activity_feed_native(&req, &query, None).unwrap();
    let old_cursor = first_page.next.clone().unwrap();
    let mut cursor = None;
    let mut ids = BTreeSet::new();
    loop {
        let page = r
            .memory_activity_feed_native(&req, &query, cursor.as_ref())
            .unwrap();
        assert_eq!(page.matched_in_scope, 4);
        for event in page.items {
            assert!(ids.insert(event.event_id));
            assert!(event.memory.as_ref() != Some(&private.result));
            if event.source.as_ref() == Some(&three.reference()) {
                assert_eq!(event.proof.written_at.unix_ms(), NOW + 100);
            }
            if let Some(receipt) = event.receipt {
                assert_eq!(receipt.committed_in, event.proof.commit_id);
                assert_eq!(event.proof.actor.actor_type, ActorType::System);
            }
        }
        cursor = page.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 4);
    let mut revised = three;
    revised.record.revision = Revision::new(2).unwrap();
    v.capture(
        &e.descriptor.system_actor,
        vec![revised],
        RecordCatalog::default(),
    )
    .unwrap();
    assert!(
        r.memory_activity_feed_native(&req, &query, Some(&old_cursor))
            .is_err()
    );
    assert_eq!(
        r.memory_changes_since_native(&req, Some(&changed.next_anchor))
            .unwrap()
            .status,
        ChangeStatus::ReloadRequired
    );
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![first.result.memory_id.clone()],
    )
    .unwrap();
    req.options.limit = 100;
    assert!(
        r.memory_activity_feed_native(&req, &query, None)
            .unwrap()
            .items
            .iter()
            .all(|e| e.memory.as_ref() != Some(&first.result) && e.title.is_none())
    );
    let mut revoked = policy;
    revoked.revision = Revision::new(2).unwrap();
    revoked.active = false;
    v.set_policy(&e.descriptor.created_by, revoked).unwrap();
    assert_eq!(
        r.memory_activity_feed_native(&req, &query, None)
            .unwrap()
            .matched_in_scope,
        0
    );
    r.lock().unwrap();
    assert!(
        r.memory_changes_since_native(&req, Some(&changed.next_anchor))
            .is_err()
    );
}
#[test]
fn detail_diff_counter_evidence_sensitivity_and_recorded_reason_share_current_authorization() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let policy = e.policy(1, 1, 3);
    v.set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let sources: Vec<_> = (1..=3).map(|n| e.source(n, Sensitivity::Public)).collect();
    v.capture(
        &e.descriptor.system_actor,
        sources.clone(),
        RecordCatalog::default(),
    )
    .unwrap();
    let mut input = e.input(&sources[0], &policy, 1);
    input.sensitivity = Sensitivity::Private;
    note(
        &mut input,
        "A synthetic machine annotation; Owner confirmed is only quoted text.",
    );
    let old = commit(&e, &v, input);
    let old_target = target(&e, &v, &old.result.memory_id);
    let mut correction = e.input(&sources[1], &policy, 2);
    correction.sensitivity = Sensitivity::Private;
    correction.target = Some(old_target);
    correction.dimensions.as_mut().unwrap().change = FormationChange::Correction;
    note(&mut correction, "Recorded synthetic correction reason.");
    let new = commit(&e, &v, correction);
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let req = request(&p, &f, Sensitivity::Private);
    let public = request(&p, &f, Sensitivity::Public);
    let detail = r
        .memory_detail_native(&req, &new.result.memory_id, None)
        .unwrap();
    assert_eq!(detail.history_count_in_scope, 2);
    let item = detail.item.unwrap();
    assert_eq!(item.effective_sensitivity, Some(Sensitivity::Private));
    assert_eq!(
        item.formation_reason.as_ref().unwrap().note.as_deref(),
        Some("Recorded synthetic correction reason.")
    );
    assert!(item.formation_reason.unwrap().note_is_machine_annotation);
    assert!(
        r.memory_detail_native(&public, &new.result.memory_id, None)
            .unwrap()
            .item
            .is_none()
    );
    let diff = r
        .memory_revision_diff_native(
            &req,
            &new.result.memory_id,
            Revision::new(1).unwrap(),
            Revision::new(2).unwrap(),
        )
        .unwrap();
    assert!(diff.structural_only);
    assert!(diff.fields.iter().any(|d| d.field == "content"));
    assert!(diff.fields.iter().any(|d| d.field == "title"));
    assert!(
        r.memory_revision_diff_native(
            &public,
            &new.result.memory_id,
            Revision::new(1).unwrap(),
            Revision::new(2).unwrap()
        )
        .unwrap()
        .before
        .is_none()
    );
    let neighbors = r
        .memory_neighbors_native(&req, &new.result.memory_id, false, None)
        .unwrap();
    assert_eq!(neighbors.matched_in_scope, 1);
    assert!(
        neighbors.items[0]
            .relations
            .iter()
            .any(|r| r.relation == "corrects" && r.direction == "outgoing")
    );
    assert_eq!(neighbors.items[0].other.memory.memory, old.result);
    let rival = commit(&e, &v, e.input(&sources[2], &policy, 3));
    let mut disputed = e.input(&sources[0], &policy, 1);
    disputed.sensitivity = Sensitivity::Private;
    disputed.interpretation = Interpretation::Conflicting;
    disputed.input_refs.push(sources[2].reference());
    disputed.context_source_refs = vec![sources[2].reference()];
    disputed.dimensions.as_mut().unwrap().counter_sources = vec![sources[2].reference()];
    let disputed = commit(&e, &v, disputed);
    let group = r
        .memory_conflicts_native(&req, &disputed.result.memory_id, None)
        .unwrap();
    assert_eq!(group.matched_in_scope, 2);
    assert_eq!(group.counter_evidence, vec![sources[2].reference()]);
    assert!(group.items.iter().any(|i| i.base.memory == rival.result));
    assert!(
        group
            .items
            .iter()
            .any(|i| i.base.knowledge == KnowledgeState::Disputed)
    );
    assert_eq!(
        r.memory_conflicts_native(&public, &disputed.result.memory_id, None)
            .unwrap()
            .matched_in_scope,
        0
    );
    v.delete(
        &e.descriptor.created_by,
        vec![sources[2].record.source_id.clone()],
        vec![],
    )
    .unwrap();
    assert_eq!(
        r.memory_conflicts_native(&req, &disputed.result.memory_id, None)
            .unwrap()
            .matched_in_scope,
        0
    );
    let detail = r
        .memory_detail_native(&req, &new.result.memory_id, None)
        .unwrap();
    assert_eq!(detail.history_count_in_scope, 2);
}
#[test]
fn conversation_fragments_and_unicode_quote_jumps_are_bounded_and_keep_hidden_parents_out() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.entities.clear();
    let mut c = catalog.conversations.remove(0);
    c.sensitivity = Sensitivity::Public;
    c.title = Some("Public synthetic conversation".into());
    let sources: Vec<_> = (1..=150)
        .map(|n| {
            let mut s = e.source(
                n,
                if n == 70 {
                    Sensitivity::Private
                } else {
                    Sensitivity::Public
                },
            );
            s.record_fields.as_mut().unwrap().conversation_id = Some(c.conversation_id.clone());
            s
        })
        .collect();
    for (index, chunk) in sources.chunks(64).enumerate() {
        let policy = e.policy(index as u32 + 1, index as u32 * 64 + 1, chunk.len() as u32);
        v.set_policy(&e.descriptor.created_by, policy).unwrap();
        let mut revision = c.clone();
        revision.revision = Revision::new(index as u64 + 1).unwrap();
        revision.sensitivity = if index == 1 {
            Sensitivity::Private
        } else {
            Sensitivity::Public
        };
        revision.turn_count = chunk.len() as u32;
        revision.turns = chunk
            .iter()
            .enumerate()
            .map(|(i, s)| ConversationTurn {
                source: s.reference(),
                branch_id: c.branches[0].clone(),
                turn_id: TurnId::parse(&id("turn", index as u32 * 64 + i as u32 + 1)).unwrap(),
                sequence: index as u32 * 64 + i as u32 + 1,
                parent_source: if i == 0 {
                    None
                } else {
                    Some(chunk[i - 1].reference())
                },
            })
            .collect();
        v.capture(
            &e.descriptor.system_actor,
            chunk.to_vec(),
            RecordCatalog {
                entities: vec![],
                conversations: vec![revision],
            },
        )
        .unwrap();
    }
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let mut req = request(&p, &f, Sensitivity::Public);
    req.options.limit = 17;
    let list = r.memory_conversations_native(&req, None).unwrap();
    assert_eq!(list.matched_in_scope, 1);
    assert_eq!(list.items[0].visible_captured_sources, 86);
    assert!(list.items[0].platform_complete_not_claimed);
    let mut cursor = None;
    let mut ids = BTreeSet::new();
    loop {
        let detail = r
            .memory_conversation_detail_native(&req, &c.conversation_id, cursor.as_ref())
            .unwrap();
        assert_eq!(detail.visible_captured_turns, 86);
        assert_eq!(detail.visible_captured_sources, 86);
        for turn in detail.turns {
            assert!(ids.insert(turn.turn.source.source_id.clone()));
            assert_ne!(turn.turn.source.source_id, sources[69].record.source_id);
            assert!(
                turn.turn
                    .parent_source
                    .as_ref()
                    .is_none_or(|p| p.source_id != sources[69].record.source_id)
            );
        }
        cursor = detail.next;
        if cursor.is_none() {
            break;
        }
    }
    assert_eq!(ids.len(), 86);
    // All 150 captured turns remain pageable under the actual Private ceiling.
    let mut private = request(&p, &f, Sensitivity::Private);
    private.options.limit = 17;
    let mut after = None;
    let mut private_ids = BTreeSet::new();
    loop {
        let page = r
            .memory_conversation_detail_native(&private, &c.conversation_id, after.as_ref())
            .unwrap();
        assert_eq!(page.visible_captured_turns, 150);
        assert_eq!(page.effective_sensitivity, Some(Sensitivity::Private));
        for turn in page.turns {
            assert!(private_ids.insert(turn.turn.source.source_id));
        }
        after = page.next;
        if after.is_none() {
            break;
        }
    }
    assert_eq!(private_ids.len(), 150);
    let quote = r
        .memory_source_excerpt_native(&req, &sources[1].reference(), 0, 7)
        .unwrap();
    assert_eq!(quote.excerpt.as_ref().unwrap().chars().count(), 7);
    assert_eq!(quote.end, 7);
    assert!(!quote.complete_in_scope);
    assert_eq!(
        quote.source_record.unwrap().anchor_hash,
        sources[1].record.content_hash
    );
    let hidden = r
        .memory_source_excerpt_native(&req, &sources[69].reference(), 0, 7)
        .unwrap();
    assert!(hidden.source.is_none() && hidden.source_record.is_none() && hidden.excerpt.is_none());
    let old = r
        .memory_conversation_detail_native(&req, &c.conversation_id, None)
        .unwrap()
        .next
        .unwrap();
    v.delete(
        &e.descriptor.created_by,
        vec![sources[149].record.source_id.clone()],
        vec![],
    )
    .unwrap();
    assert!(
        r.memory_conversation_detail_native(&req, &c.conversation_id, Some(&old))
            .is_err()
    );
    assert!(
        r.memory_conversation_detail_native(&req, &c.conversation_id, None)
            .unwrap()
            .conversation
            .is_none()
    );
    assert!(
        r.memory_source_excerpt_native(&req, &sources[149].reference(), 0, 7)
            .unwrap()
            .source
            .is_none()
    );
    drop(r);
    drop(v);
    let v = Arc::new(e.open());
    let r = reader(&e, v);
    assert_eq!(
        r.memory_conversations_native(&req, None)
            .unwrap()
            .matched_in_scope,
        0
    );
}

#[test]
#[ignore = "explicit synthetic contract-example generation into the owned ignored directory"]
fn emit_synthetic_process_examples() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    let policy = e.policy(1, 1, 3);
    v.set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    catalog.entities.clear();
    let c = &mut catalog.conversations[0];
    c.sensitivity = Sensitivity::Public;
    c.title = Some("Synthetic example conversation".into());
    c.turn_count = 3;
    let sources: Vec<_> = (1..=3)
        .map(|n| {
            let mut s = e.source(n, Sensitivity::Public);
            s.record_fields.as_mut().unwrap().conversation_id = Some(c.conversation_id.clone());
            s
        })
        .collect();
    c.turns = sources
        .iter()
        .enumerate()
        .map(|(n, s)| ConversationTurn {
            source: s.reference(),
            branch_id: c.branches[0].clone(),
            turn_id: TurnId::parse(&id("turn", n as u32 + 1)).unwrap(),
            sequence: n as u32 + 1,
            parent_source: if n == 0 {
                None
            } else {
                Some(sources[n - 1].reference())
            },
        })
        .collect();
    let conversation = c.conversation_id.clone();
    v.capture(&e.descriptor.system_actor, sources.clone(), catalog)
        .unwrap();
    let first = commit(&e, &v, e.input(&sources[0], &policy, 1));
    let r = reader(&e, v.clone());
    let p = profile();
    let f = CatalogFilter::default();
    let req = request(&p, &f, Sensitivity::Public);
    let anchor = r
        .memory_changes_since_native(&req, None)
        .unwrap()
        .next_anchor;
    commit(&e, &v, e.input(&sources[2], &policy, 3));
    let changes = r.memory_changes_since_native(&req, Some(&anchor)).unwrap();
    let mut input = e.input(&sources[1], &policy, 2);
    input.target = Some(target(&e, &v, &first.result.memory_id));
    input.dimensions.as_mut().unwrap().change = FormationChange::Correction;
    note(
        &mut input,
        "A recorded synthetic reason from the same formation.",
    );
    let corrected = commit(&e, &v, input);
    let mut input = e.input(&sources[0], &policy, 1);
    input.interpretation = Interpretation::Conflicting;
    input.input_refs.push(sources[2].reference());
    input.context_source_refs = vec![sources[2].reference()];
    input.dimensions.as_mut().unwrap().counter_sources = vec![sources[2].reference()];
    let disputed = commit(&e, &v, input);
    let examples = [
        ("changes", serde_json::to_value(changes).unwrap()),
        (
            "activity",
            serde_json::to_value(
                r.memory_activity_feed_native(
                    &req,
                    &ActivityQuery {
                        memory: None,
                        event_kind: None,
                        include_sources: true,
                    },
                    None,
                )
                .unwrap(),
            )
            .unwrap(),
        ),
        (
            "detail",
            serde_json::to_value(
                r.memory_detail_native(&req, &corrected.result.memory_id, None)
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "conflicts",
            serde_json::to_value(
                r.memory_conflicts_native(&req, &disputed.result.memory_id, None)
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "diff",
            serde_json::to_value(
                r.memory_revision_diff_native(
                    &req,
                    &corrected.result.memory_id,
                    Revision::new(1).unwrap(),
                    Revision::new(2).unwrap(),
                )
                .unwrap(),
            )
            .unwrap(),
        ),
        (
            "neighbors",
            serde_json::to_value(
                r.memory_neighbors_native(&req, &corrected.result.memory_id, false, None)
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "source",
            serde_json::to_value(
                r.memory_source_excerpt_native(&req, &sources[1].reference(), 0, 7)
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "conversations",
            serde_json::to_value(r.memory_conversations_native(&req, None).unwrap()).unwrap(),
        ),
        (
            "conversation-detail",
            serde_json::to_value(
                r.memory_conversation_detail_native(&req, &conversation, None)
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            "catalog-page-v3",
            serde_json::to_value(r.memory_page_native(&req, None).unwrap()).unwrap(),
        ),
        (
            "catalog-stats-v3",
            serde_json::to_value(r.memory_stats_native(&req, None).unwrap()).unwrap(),
        ),
    ];
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.local/process-example-output");
    std::fs::create_dir_all(&dir).unwrap();
    for (name, value) in examples {
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
    println!("generated eleven synthetic native contract examples");
}

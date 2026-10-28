#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, hash::sha256, ids::*, next::*};
use enouia_memory_host::automation::{catalog::*, owner::NativeMemoryOwner};
use enouia_memory_vault::{
    OsIdSource,
    automation::segmented::SegmentedAutomationVault,
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;
use std::sync::Arc;
fn env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    e
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
fn seed(e: &Env, v: &SegmentedAutomationVault) -> (TargetRef, AdmissionInput) {
    let p = e.policy(1, 1, 2);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let source = e.source(1, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![source.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let input = e.input(&source, &p, 1);
    let r = commit(e, v, input.clone());
    let target = v
        .read(
            &e.descriptor.system_actor,
            Sensitivity::HighlySensitive,
            |d| {
                d.memories
                    .iter()
                    .find(|m| m.memory.memory == r.result)
                    .unwrap()
                    .target
                    .clone()
            },
        )
        .unwrap();
    (target, input)
}
fn unknown() -> RecordTime {
    RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    }
}
fn meta(i: &AdmissionInput) -> OwnerCorrectionMetadata {
    let mut d = i.dimensions.clone().unwrap();
    d.change = FormationChange::Correction;
    d.event_time = None;
    d.valid_time = None;
    d.derived_from.clear();
    d.counter_sources.clear();
    d.reverts_to = None;
    OwnerCorrectionMetadata {
        title: "Synthetic owner correction".into(),
        content_lang: "en".into(),
        sensitivity: Sensitivity::Public,
        interpretation: Interpretation::ClearSelfReport,
        dimensions: d,
        event_time: unknown(),
        valid_time: unknown(),
        entities: vec![],
    }
}
fn correct(target: TargetRef) -> OwnerAction {
    OwnerAction::Correct {
        target,
        source_id: SourceId::parse(&id("src", 999)).unwrap(),
        assertion_text: "I prefer morning walks; this is a synthetic owner assertion.".into(),
    }
}
fn execute(
    h: &NativeMemoryOwner,
    p: enouia_memory_vault::automation::segmented::NativeOwnerPreview,
) -> NativeOwnerOutcome {
    let t = h
        .confirm_from_owner_gesture(
            &p.plan.operation_id,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    h.commit(t).unwrap()
}
#[test]
fn owner_correction_is_literal_attributed_and_one_witnessed_atomic_commit() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (target, i) = seed(&e, &v);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let before = v.snapshot().unwrap();
    let p = h
        .plan(
            correct(target.clone()),
            Sensitivity::Private,
            Some(meta(&i)),
        )
        .unwrap();
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(p.original_sources_retained && !p.physical_purge);
    let proposal = p.proposed_correction.as_ref().unwrap();
    assert_eq!(proposal.input.actor, e.descriptor.system_actor);
    assert_eq!(proposal.outcome.adoption, AdoptionState::Adopted);
    assert_eq!(proposal.input.target.as_ref(), Some(&target));
    let operation = p.plan.operation_id.clone();
    let t = h
        .confirm_from_owner_gesture(
            &operation,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    assert_eq!(v.snapshot().unwrap(), before);
    assert!(
        h.confirm_from_owner_gesture(
            &operation,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp
        )
        .is_err()
    );
    let outcome = h.commit(t).unwrap();
    assert_eq!(outcome.memory_result.as_ref().unwrap().revision.get(), 2);
    assert_eq!(
        h.outcome(&operation, Sensitivity::Private).unwrap(),
        Some(outcome.clone())
    );
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        let current = d
            .memories
            .iter()
            .find(|m| m.memory.memory.memory_id == target.memory.memory_id)
            .unwrap();
        assert_eq!(current.receipt.committed_in, outcome.committed_in);
        assert_eq!(current.memory.request.actor, e.descriptor.system_actor);
        let s = d
            .sources
            .get(&current.memory.request.input_refs[0].source_id)
            .unwrap();
        assert_eq!(
            s.record.source_kind,
            enouia_memory_contract::source::SourceKind::ManualAssertion
        );
        assert_eq!(s.attributed_subject, e.descriptor.owner_subject_id);
        assert_eq!(s.text, current.memory.request.content);
        assert!(!s.validate().is_empty());
        // Generic System capture validation continues to reject even a real manual assertion.
    })
    .unwrap();
    drop(h);
    drop(v);
    let reopened = Arc::new(e.open());
    let h = NativeMemoryOwner::from_authenticated_owner(reopened, e.descriptor.created_by.clone())
        .unwrap();
    assert_eq!(
        h.outcome(&operation, Sensitivity::Private).unwrap(),
        Some(outcome)
    );
}
#[test]
fn wrong_actor_subject_stale_hash_expiration_and_lock_cannot_create_owner_actions() {
    let mut e = env();
    e.descriptor.owner_subject_id = Some(SubjectId::parse(&id("sub", 999)).unwrap());
    let v = Arc::new(e.create(Faults::none()));
    let (target, i) = seed(&e, &v);
    assert!(
        NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.system_actor.clone())
            .is_err()
    );
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let before = v.snapshot().unwrap();
    assert!(
        h.plan(
            correct(target.clone()),
            Sensitivity::Private,
            Some(meta(&i))
        )
        .is_err()
    );
    assert_eq!(v.snapshot().unwrap(), before);
    let action = OwnerAction::Attention {
        target: target.clone(),
        annotation: AttentionAnnotation::Protect,
        until: None,
    };
    assert!(
        v.plan_owner_action(
            &e.descriptor.system_actor,
            action.clone(),
            Sensitivity::Private,
            None
        )
        .is_err()
    );
    let p = h.plan(action.clone(), Sensitivity::Private, None).unwrap();
    assert!(
        h.confirm_from_owner_gesture(
            &p.plan.operation_id,
            &sha256(b"forged"),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp
        )
        .is_err()
    );
    e.clock.set(NOW + 300_000);
    assert!(
        h.confirm_from_owner_gesture(
            &p.plan.operation_id,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp
        )
        .is_err()
    );
    let p = h.plan(action, Sensitivity::Private, None).unwrap();
    let token = h
        .confirm_from_owner_gesture(
            &p.plan.operation_id,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
    h.lock().unwrap();
    assert!(h.commit(token).is_err());
    h.unlock(&e.descriptor.created_by).unwrap();
    assert_eq!(v.snapshot().unwrap(), before);
    let p = h
        .plan(
            OwnerAction::Attention {
                target,
                annotation: AttentionAnnotation::Pin,
                until: None,
            },
            Sensitivity::Private,
            None,
        )
        .unwrap();
    let mut policy = e.policy(2, 2, 1);
    policy.subject_id = e.policy(1, 1, 2).subject_id;
    v.set_policy(&e.descriptor.created_by, policy).unwrap();
    assert!(
        h.confirm_from_owner_gesture(
            &p.plan.operation_id,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedWindowsApp
        )
        .is_err()
    );
}
#[test]
fn lost_owner_reply_requires_forward_recovery_and_returns_the_original_outcome() {
    for point in [FaultPoint::HostIntentPublished, FaultPoint::AfterCurrent] {
        let e = env();
        let faults = Faults::armed();
        let v = Arc::new(e.create(faults.clone()));
        let (target, _) = seed(&e, &v);
        let h =
            NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
                .unwrap();
        let p = h
            .plan(
                OwnerAction::Attention {
                    target,
                    annotation: AttentionAnnotation::Protect,
                    until: None,
                },
                Sensitivity::Private,
                None,
            )
            .unwrap();
        let op = p.plan.operation_id.clone();
        let token = h
            .confirm_from_owner_gesture(
                &op,
                &digest(&p.plan),
                &p.effects_hash,
                TrustedSurface::TrustedWindowsApp,
            )
            .unwrap();
        faults.arm(point, 1, FaultAction::Fail(112));
        assert!(h.commit(token).is_err());
        assert!(h.outcome(&op, Sensitivity::Private).is_err());
        drop(h);
        drop(v);
        faults.disarm();
        assert!(
            SegmentedAutomationVault::open(
                &e.root,
                e.descriptor.host_id.clone(),
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none()
            )
            .is_err()
        );
        let v = Arc::new(
            SegmentedAutomationVault::recover(
                &e.root,
                e.descriptor.host_id.clone(),
                &e.descriptor.system_actor,
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none(),
            )
            .unwrap(),
        );
        let h =
            NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
                .unwrap();
        let outcome = h.outcome(&op, Sensitivity::Private).unwrap().unwrap();
        let snap = v.snapshot().unwrap();
        assert_eq!(h.outcome(&op, Sensitivity::Private).unwrap(), Some(outcome));
        assert_eq!(v.snapshot().unwrap(), snap);
    }
}

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
#[test]
fn actual_owner_marks_and_removal_propagate_without_global_sequence_or_regeneration() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (target, i) = seed(&e, &v);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let r = SegmentedMemoryReader::from_authenticated_system(
        v.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    let p = profile();
    let f = CatalogFilter::default();
    let q = request(&p, &f, Sensitivity::Private);
    let initial = r.memory_page_native(&q, None).unwrap();
    assert!(
        initial.items[0]
            .owner_marks
            .as_ref()
            .unwrap()
            .corrected
            .is_none()
    );
    execute(
        &h,
        h.plan(
            OwnerAction::Attention {
                target: target.clone(),
                annotation: AttentionAnnotation::Protect,
                until: None,
            },
            Sensitivity::Private,
            None,
        )
        .unwrap(),
    );
    let protected = r.memory_page_native(&q, None).unwrap();
    assert_eq!(protected.items[0].base.attention, AttentionState::Protected);
    assert_eq!(
        protected.items[0]
            .owner_marks
            .as_ref()
            .unwrap()
            .attention
            .as_ref()
            .unwrap()
            .proof
            .owner,
        e.descriptor.created_by
    );
    execute(
        &h,
        h.plan(
            OwnerAction::Tag {
                name: "Owner synthetic tag".into(),
                targets: vec![target.clone()],
            },
            Sensitivity::Private,
            None,
        )
        .unwrap(),
    );
    let corrected = execute(
        &h,
        h.plan(
            correct(target.clone()),
            Sensitivity::Private,
            Some(meta(&i)),
        )
        .unwrap(),
    );
    let page = r.memory_page_native(&q, None).unwrap();
    let item = &page.items[0];
    assert_eq!(
        item.owner_marks
            .as_ref()
            .unwrap()
            .corrected
            .as_ref()
            .unwrap()
            .operation_id,
        corrected.operation_id
    );
    assert_eq!(
        item.owner_marks.as_ref().unwrap().tags[0].name,
        "Owner synthetic tag"
    );
    assert!(item.base.receipt.as_ref().unwrap().sequence.is_none());
    assert!(
        !serde_json::to_string(&page)
            .unwrap()
            .contains("\"sequence\"")
    );
    let feed = r
        .memory_activity_feed_native(
            &q,
            &ActivityQuery {
                memory: None,
                event_kind: None,
                include_sources: true,
            },
            None,
        )
        .unwrap();
    let correction = feed
        .items
        .iter()
        .find(|e| e.event_kind == NativeActivityKind::Correction)
        .unwrap();
    assert_eq!(correction.proof.actor, e.descriptor.created_by);
    assert!(correction.owner_action.as_ref().unwrap().action == "correct");
    assert!(correction.receipt.as_ref().unwrap().sequence.is_none());
    let live = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.memories[0].target.clone()
        })
        .unwrap();
    let preview = h
        .plan(
            OwnerAction::Forget {
                target: live,
                impact_preview_hash: sha256(b"untrusted client preview"),
            },
            Sensitivity::Private,
            None,
        )
        .unwrap();
    match &preview.plan.action {
        OwnerAction::Forget {
            impact_preview_hash,
            ..
        } => assert_ne!(*impact_preview_hash, sha256(b"untrusted client preview")),
        _ => unreachable!(),
    }
    assert_eq!(preview.affected_count_in_scope, 1);
    assert!(preview.original_sources_retained && !preview.physical_purge);
    execute(&h, preview);
    assert_eq!(r.memory_page_native(&q, None).unwrap().matched_in_scope, 0);
    assert!(
        r.memory_activity_feed_native(
            &q,
            &ActivityQuery {
                memory: None,
                event_kind: None,
                include_sources: false
            },
            None
        )
        .unwrap()
        .items
        .is_empty()
    );
    assert!(
        h.outcome(&corrected.operation_id, Sensitivity::Private)
            .unwrap()
            .unwrap()
            .memory_result
            .is_none()
    );
    // A machine cannot turn the retained owner source into a new manual assertion.
    let mut prior = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.sources
                .values()
                .find(|s| {
                    s.record.source_kind
                        == enouia_memory_contract::source::SourceKind::ManualAssertion
                })
                .map(|s| (*s).clone())
        })
        .unwrap()
        .unwrap();
    prior.record.source_id = SourceId::parse(&id("src", 998)).unwrap();
    assert!(
        v.capture(
            &e.descriptor.system_actor,
            vec![prior],
            RecordCatalog::default()
        )
        .is_err()
    );
}
#[test]
fn owner_withdrawal_rejects_a_literal_new_id_from_the_same_original_evidence_after_reopen() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (target, i) = seed(&e, &v);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    execute(
        &h,
        h.plan(
            OwnerAction::Withdraw {
                target,
                reason: Some("Synthetic withdrawal".into()),
            },
            Sensitivity::Private,
            None,
        )
        .unwrap(),
    );
    let input = v.prepare(&e.descriptor.system_actor, i.clone()).unwrap();
    let response =
        serde_json::to_string(&v.evaluate(&e.descriptor.system_actor, &input).unwrap()).unwrap();
    let binding = GenerationBinding {
        operation_id: input.operation_id.clone(),
        input_hash: digest(&input),
        response_hash: sha256(response.as_bytes()),
    };
    assert!(
        v.apply_batch(
            &e.descriptor.system_actor,
            vec![input],
            vec![response],
            vec![binding]
        )
        .is_err()
    );
    drop(h);
    drop(v);
    let v = e.open();
    let input = v.prepare(&e.descriptor.system_actor, i).unwrap();
    let response =
        serde_json::to_string(&v.evaluate(&e.descriptor.system_actor, &input).unwrap()).unwrap();
    let binding = GenerationBinding {
        operation_id: input.operation_id.clone(),
        input_hash: digest(&input),
        response_hash: sha256(response.as_bytes()),
    };
    assert!(
        v.apply_batch(
            &e.descriptor.system_actor,
            vec![input],
            vec![response],
            vec![binding]
        )
        .is_err()
    );
}
#[test]
fn owner_entry_refuses_old_layouts_and_confirmation_expires_at_the_commit_barrier() {
    let e = Env::new();
    let v = Arc::new(e.create(Faults::none()));
    assert!(
        NativeMemoryOwner::from_authenticated_owner(v, e.descriptor.created_by.clone()).is_err()
    );
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (target, _) = seed(&e, &v);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let p = h
        .plan(
            OwnerAction::Attention {
                target,
                annotation: AttentionAnnotation::Pin,
                until: None,
            },
            Sensitivity::Private,
            None,
        )
        .unwrap();
    let before = v.snapshot().unwrap();
    let token = h
        .confirm_from_owner_gesture(
            &p.plan.operation_id,
            &digest(&p.plan),
            &p.effects_hash,
            TrustedSurface::TrustedLocalCli,
        )
        .unwrap();
    e.clock.set(NOW + 300_000);
    assert!(h.commit(token).is_err());
    assert_eq!(v.snapshot().unwrap(), before);
}
fn entity_fixture(
    e: &Env,
    v: &SegmentedAutomationVault,
) -> (String, String, Vec<(TargetRef, AdmissionInput)>) {
    entity_fixture_for(e, v, false)
}
fn entity_fixture_for(
    e: &Env,
    v: &SegmentedAutomationVault,
    conversation: bool,
) -> (String, String, Vec<(TargetRef, AdmissionInput)>) {
    let policy = e.policy(1, 1, 3);
    v.set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let mut sources: Vec<_> = (1..=3).map(|n| e.source(n, Sensitivity::Public)).collect();
    let mut catalog: RecordCatalog =
        serde_json::from_value(value("ui-records/catalog.json")).unwrap();
    if !conversation {
        catalog.conversations.clear();
    }
    let template = catalog.entities.remove(0);
    catalog.entities.clear();
    let mut inputs: Vec<_> = sources
        .iter()
        .enumerate()
        .map(|(n, s)| e.input(s, &policy, n as u32 + 1))
        .collect();
    let a = id("ent", 500);
    let b = id("ent", 501);
    for (n, name, entity_id) in [(0, "Cedar", a.clone()), (1, "Fir", b.clone())] {
        let mut record = template.clone();
        record.entity_id = entity_id;
        record.display_name = name.into();
        record.aliases.clear();
        record.sensitivity = Sensitivity::Public;
        record.evidence = inputs[n].record_fields.as_ref().unwrap().evidence.clone();
        catalog.entities.push(record);
    }
    if conversation {
        let c = &mut catalog.conversations[0];
        c.sensitivity = Sensitivity::Public;
        c.participants = vec![ConversationParticipant {
            entity_id: Some(a.clone()),
            label: "Cedar".into(),
            speaker: SpeakerRole::User,
        }];
        for source in &mut sources {
            source.record_fields.as_mut().unwrap().conversation_id =
                Some(c.conversation_id.clone());
        }
        c.turns = sources
            .iter()
            .enumerate()
            .map(|(n, s)| ConversationTurn {
                source: s.reference(),
                branch_id: c.branches[0].clone(),
                turn_id: TurnId::parse(&id("turn", n as u32 + 1)).unwrap(),
                sequence: n as u32 + 1,
                parent_source: None,
            })
            .collect();
        c.turn_count = 3;
    }
    v.capture(&e.descriptor.system_actor, sources, catalog)
        .unwrap();
    let mut records = vec![];
    for (n, input) in inputs.iter_mut().enumerate() {
        let q = input.record_fields.as_ref().unwrap().evidence.clone();
        input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
            entity_id: if n == 1 { b.clone() } else { a.clone() },
            role: EntityRole::Mentioned,
            evidence: q,
        }];
        let receipt = commit(e, v, input.clone());
        let target = v
            .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
                d.memories
                    .iter()
                    .find(|m| m.memory.memory == receipt.result)
                    .unwrap()
                    .target
                    .clone()
            })
            .unwrap();
        records.push((target, input.clone()));
    }
    (a, b, records)
}
#[test]
fn owner_entity_merge_and_selected_evidence_split_drive_filters_aliases_and_typed_edges() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (a, b, records) = entity_fixture(&e, &v);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let r = SegmentedMemoryReader::from_authenticated_system(
        v.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    let p = profile();
    let mut f = CatalogFilter {
        entity: Some(a.clone()),
        ..Default::default()
    };
    assert_eq!(
        r.memory_page_native(&request(&p, &f, Sensitivity::Public), None)
            .unwrap()
            .matched_in_scope,
        2
    );
    execute(
        &h,
        h.plan(
            OwnerAction::EntityMerge {
                from_entity: a.clone(),
                into_entity: b.clone(),
                reason: None,
            },
            Sensitivity::Public,
            None,
        )
        .unwrap(),
    );
    f.entity = Some(b.clone());
    assert_eq!(
        r.memory_page_native(&request(&p, &f, Sensitivity::Public), None)
            .unwrap()
            .matched_in_scope,
        3
    );
    f = CatalogFilter {
        text: "Cedar".into(),
        ..Default::default()
    };
    assert_eq!(
        r.memory_page_native(&request(&p, &f, Sensitivity::Public), None)
            .unwrap()
            .matched_in_scope,
        3
    );
    f = CatalogFilter::default();
    let graph = r
        .memory_graph_native(
            &request(&p, &f, Sensitivity::Public),
            &GraphQuery {
                node_limit: 100,
                edge_limit_per_kind: 100,
            },
            None,
        )
        .unwrap();
    assert!(
        graph
            .edges
            .iter()
            .any(|e| e.relation == GraphEdgeKind::SameEntity && !e.owner_relations.is_empty())
    );
    execute(
        &h,
        h.plan(
            OwnerAction::EntitySplit {
                from_entity: b.clone(),
                into_entity: a.clone(),
                evidence: records[2]
                    .1
                    .record_fields
                    .as_ref()
                    .unwrap()
                    .evidence
                    .clone(),
                reason: None,
            },
            Sensitivity::Public,
            None,
        )
        .unwrap(),
    );
    f.entity = Some(a);
    assert_eq!(
        r.memory_page_native(&request(&p, &f, Sensitivity::Public), None)
            .unwrap()
            .matched_in_scope,
        1
    );
    f.entity = Some(b);
    assert_eq!(
        r.memory_page_native(&request(&p, &f, Sensitivity::Public), None)
            .unwrap()
            .matched_in_scope,
        2
    );
}

#[test]
fn native_theme_overlays_cover_more_than_128_members_survive_reopen_and_dissolve_only_the_projection()
 {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let sources: Vec<_> = (1..=132)
        .map(|n| e.source(n, Sensitivity::Public))
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

    let r = SegmentedMemoryReader::from_authenticated_system(
        v.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let p = profile();
    let f = CatalogFilter::default();
    let req = request(&p, &f, Sensitivity::Public);
    let original = r
        .memory_theme_members_native(&req, &theme, None)
        .unwrap()
        .family
        .unwrap();
    assert_eq!(original.member_count, 132);
    let preview = h
        .plan(
            OwnerAction::ThemeRename {
                theme: original.theme.clone(),
                name: "Synthetic owner family".into(),
                members: vec![],
            },
            Sensitivity::Public,
            None,
        )
        .unwrap();
    match &preview.plan.action {
        OwnerAction::ThemeRename { members, .. } => assert_eq!(members.len(), 128),
        _ => unreachable!(),
    }
    assert_eq!(preview.affected_count_in_scope, 133);
    execute(&h, preview);
    execute(
        &h,
        h.plan(
            OwnerAction::ThemePin {
                theme: original.theme.clone(),
                members: vec![],
            },
            Sensitivity::Public,
            None,
        )
        .unwrap(),
    );
    let renamed = r
        .memory_theme_members_native(&req, &theme, None)
        .unwrap()
        .family
        .unwrap();
    assert!(renamed.owner_named && renamed.pinned);
    assert_eq!(renamed.name, "Synthetic owner family");
    assert_eq!(renamed.owner_proofs.len(), 2);
    // Removing one member leaves Jaccard >= 0.5, so native recomputation keeps the real owner overlays.
    let target = v
        .read(&e.descriptor.system_actor, Sensitivity::Public, |d| {
            d.memories
                .iter()
                .find(|m| m.memory.memory == receipts[10].result)
                .unwrap()
                .target
                .clone()
        })
        .unwrap();
    execute(
        &h,
        h.plan(
            OwnerAction::Withdraw {
                target,
                reason: None,
            },
            Sensitivity::Public,
            None,
        )
        .unwrap(),
    );
    let after = r
        .memory_theme_members_native(&req, &theme, None)
        .unwrap()
        .family
        .unwrap();
    assert_eq!(after.member_count, 131);
    assert_eq!(after.name, "Synthetic owner family");
    // Core-5 still forbids correction revisions of a Theme.
    assert!(
        h.plan(
            correct(original.theme.clone()),
            Sensitivity::Public,
            Some(meta(&e.input(&sources[0], &policies[0], 1)))
        )
        .is_err()
    );
    drop(h);
    drop(r);
    drop(v);
    let v = Arc::new(e.open());
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let r = SegmentedMemoryReader::from_authenticated_system(
        v.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    assert!(
        r.memory_theme_members_native(&req, &theme, None)
            .unwrap()
            .family
            .unwrap()
            .pinned
    );
    execute(
        &h,
        h.plan(
            OwnerAction::ThemeDissolve {
                theme: original.theme,
                members: vec![],
            },
            Sensitivity::Public,
            None,
        )
        .unwrap(),
    );
    assert!(
        r.memory_theme_members_native(&req, &theme, None)
            .unwrap()
            .family
            .is_none()
    );
    assert!(
        r.memory_detail_native(&req, &theme, None)
            .unwrap()
            .item
            .is_some()
    );
}

#[test]
#[ignore = "emits synthetic native owner contract examples; no private data"]
fn emit_synthetic_owner_examples() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (target, i) = seed(&e, &v);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let metadata = meta(&i);
    let preview = h
        .plan(
            correct(target),
            Sensitivity::Private,
            Some(metadata.clone()),
        )
        .unwrap();
    let plan = preview.plan.clone();
    let outcome = execute(&h, preview);
    let (record, effects) = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            let a = &d.owner_actions[0];
            (a.record.clone(), a.effects.clone())
        })
        .unwrap();
    let r = SegmentedMemoryReader::from_authenticated_system(
        v.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    let p = profile();
    let f = CatalogFilter::default();
    let q = request(&p, &f, Sensitivity::Private);
    let dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.local/owner-example-output");
    std::fs::create_dir_all(&dir).unwrap();
    let values = [
        ("descriptor", serde_json::to_value(&e.descriptor).unwrap()),
        ("plan", serde_json::to_value(plan).unwrap()),
        ("record", serde_json::to_value(&record).unwrap()),
        ("effects", serde_json::to_value(effects).unwrap()),
        ("metadata", serde_json::to_value(metadata).unwrap()),
        ("outcome", serde_json::to_value(outcome).unwrap()),
        (
            "proof",
            serde_json::to_value(OwnerActionProof::from_record(&record)).unwrap(),
        ),
        (
            "page",
            serde_json::to_value(r.memory_page_native(&q, None).unwrap()).unwrap(),
        ),
        (
            "graph",
            serde_json::to_value(
                r.memory_graph_native(
                    &q,
                    &GraphQuery {
                        node_limit: 100,
                        edge_limit_per_kind: 100,
                    },
                    None,
                )
                .unwrap(),
            )
            .unwrap(),
        ),
        (
            "activity",
            serde_json::to_value(
                r.memory_activity_feed_native(
                    &q,
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
    ];
    for (name, value) in values {
        std::fs::write(
            dir.join(format!("{name}.json")),
            serde_json::to_vec_pretty(&value).unwrap(),
        )
        .unwrap();
    }
    for file in std::fs::read_dir(e.path.join("vault/automation-segments")).unwrap() {
        let stored: SegmentedCommit =
            serde_json::from_slice(&std::fs::read(file.unwrap().path()).unwrap()).unwrap();
        let name = match stored.delta {
            SegmentedDelta::Genesis { .. } => Some("genesis"),
            SegmentedDelta::OwnerAction { .. } => Some("owner-segment"),
            _ => None,
        };
        if let Some(name) = name {
            std::fs::write(
                dir.join(format!("{name}.json")),
                serde_json::to_vec_pretty(&stored).unwrap(),
            )
            .unwrap();
        }
    }
}

#[test]
fn owner_entity_merge_refuses_foreign_policy_aliases_and_commit_rechecks_permission_expiry() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (a, b, _) = entity_fixture(&e, &v);
    let narrow = e.policy(3, 1, 1);
    v.set_policy(&e.descriptor.created_by, narrow.clone())
        .unwrap();
    let source = e.source(1, Sensitivity::Public);
    let mut input = e.input(&source, &narrow, 101);
    input.record_fields.as_mut().unwrap().entities = vec![EntityMention {
        entity_id: a.clone(),
        role: EntityRole::Mentioned,
        evidence: input.record_fields.as_ref().unwrap().evidence.clone(),
    }];
    commit(&e, &v, input);
    // B is globally readable through the original policy, but A's new memory cannot borrow it.
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let before = v.snapshot().unwrap();
    assert!(
        h.plan(
            OwnerAction::EntityMerge {
                from_entity: a,
                into_entity: b,
                reason: None
            },
            Sensitivity::Private,
            None
        )
        .is_err()
    );
    assert_eq!(v.snapshot().unwrap(), before);
    drop(h);
    drop(v);
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    seed(&e, &v);
    let mut p = e.policy(2, 1, 2);
    p.valid_until = enouia_memory_contract::time::Timestamp::from_unix_ms(NOW + 100).unwrap();
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let receipt = commit(&e, &v, e.input(&e.source(1, Sensitivity::Public), &p, 102));
    let target = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.memories
                .iter()
                .find(|m| m.memory.memory == receipt.result)
                .unwrap()
                .target
                .clone()
        })
        .unwrap();
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let preview = h
        .plan(
            OwnerAction::Attention {
                target,
                annotation: AttentionAnnotation::Protect,
                until: None,
            },
            Sensitivity::Private,
            None,
        )
        .unwrap();
    let token = h
        .confirm_from_owner_gesture(
            &preview.plan.operation_id,
            &digest(&preview.plan),
            &preview.effects_hash,
            TrustedSurface::TrustedLocalCli,
        )
        .unwrap();
    let before = v.snapshot().unwrap();
    e.clock.set(NOW + 101);
    assert!(h.commit(token).is_err());
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn owner_entity_projection_updates_authorized_conversation_participants_without_rewriting_turns() {
    let e = env();
    let v = Arc::new(e.create(Faults::none()));
    let (a, b, records) = entity_fixture_for(&e, &v, true);
    let h = NativeMemoryOwner::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
        .unwrap();
    let r = SegmentedMemoryReader::from_authenticated_system(
        v.clone(),
        e.descriptor.system_actor.clone(),
    )
    .unwrap();
    let p = profile();
    let f = CatalogFilter::default();
    let req = request(&p, &f, Sensitivity::Public);
    execute(
        &h,
        h.plan(
            OwnerAction::EntityMerge {
                from_entity: a.clone(),
                into_entity: b.clone(),
                reason: None,
            },
            Sensitivity::Public,
            None,
        )
        .unwrap(),
    );
    let page = r.memory_conversations_native(&req, None).unwrap();
    let c = &page.items[0].conversation;
    assert_eq!(c.participants[0].entity_id, Some(b.clone()));
    assert_eq!(c.participants[0].label, "Fir");
    assert_eq!(c.owner_relations.len(), 1);
    assert!(c.turns.is_none());
    execute(
        &h,
        h.plan(
            OwnerAction::EntitySplit {
                from_entity: b.clone(),
                into_entity: a.clone(),
                evidence: records[2]
                    .1
                    .record_fields
                    .as_ref()
                    .unwrap()
                    .evidence
                    .clone(),
                reason: None,
            },
            Sensitivity::Public,
            None,
        )
        .unwrap(),
    );
    let page = r.memory_conversations_native(&req, None).unwrap();
    let c = &page.items[0].conversation;
    let ids: std::collections::BTreeSet<_> = c
        .participants
        .iter()
        .filter_map(|p| p.entity_id.clone())
        .collect();
    assert_eq!(ids, std::collections::BTreeSet::from([a, b]));
    assert_eq!(c.owner_relations.len(), 2);
    assert_eq!(page.items[0].visible_captured_turns, 3);
}

#[test]
fn owner_roots_cannot_be_adopted_or_rebound_by_editing_only_the_descriptor() {
    for already_owner in [false, true] {
        let e = if already_owner { env() } else { Env::new() };
        let v = Arc::new(e.create(Faults::none()));
        seed(&e, &v);
        drop(v);
        let mut descriptor = e.descriptor.clone();
        descriptor.layout = AutomationLayout::SegmentedOwnerV1;
        descriptor.owner_subject_id = Some(SubjectId::parse(&id("sub", 999)).unwrap());
        assert!(descriptor.validate().is_empty());
        // Deliberately damage only this test's fresh synthetic descriptor, never an existing root.
        std::fs::write(
            e.path
                .join(enouia_memory_contract::layout::vault_descriptor()),
            enouia_memory_contract::json::canonical_bytes(&descriptor).unwrap(),
        )
        .unwrap();
        assert!(
            SegmentedAutomationVault::open(
                &e.root,
                e.descriptor.host_id.clone(),
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none()
            )
            .is_err()
        );
    }
}

use super::formation_cases::{DimensionalModel, dims, form};
use super::*;
use enouia_memory_host::automation::read_model::simulate_synthetic_recall;
fn profile() -> AttentionProfile {
    serde_json::from_value(support::fixture("../attention/profile.json")).unwrap()
}
fn env() -> Env {
    super::formation_cases::env()
}
#[test]
fn pages_are_current_authorized_snapshot_bound_and_preserve_real_formation_meaning() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    for n in 1..=2 {
        form(
            &e,
            &host,
            &jobs,
            n,
            &format!("A private episode {n}."),
            dims(FormationKind::Episode, None, FormationChange::Initial),
            None,
        );
    }
    let p = profile();
    let f = MemoryReadFilter::default();
    let first = host
        .memory_page_native(&p, &f, None, 1, MAX_DOCUMENT_BYTES, Sensitivity::Private)
        .unwrap();
    assert_eq!(first.items.len(), 1);
    assert_eq!(first.matched_in_scope, 2);
    assert!(!first.complete_in_scope);
    let cursor = first.next.as_ref().unwrap();
    let next = host
        .memory_page_native(
            &p,
            &f,
            Some(cursor),
            1,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private,
        )
        .unwrap();
    assert!(next.complete_in_scope);
    assert_ne!(next.items[0].memory, first.items[0].memory);
    assert!(first.items[0].receipt.is_some());
    assert_eq!(first.items[0].system_actor.actor_type, ActorType::System);
    assert!(parse_document::<MemoryReadPage>(&canonical_bytes(&first).unwrap()).is_ok());
    let mut changed = f.clone();
    changed.form = Some(FormationKind::Plan);
    assert!(
        host.memory_page_native(
            &p,
            &changed,
            Some(cursor),
            1,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private
        )
        .is_err()
    );
    let mut changed_profile = p.clone();
    changed_profile.fade_after_days = 31;
    assert!(
        host.memory_page_native(
            &changed_profile,
            &f,
            Some(cursor),
            1,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private
        )
        .is_err()
    );
    let public = host
        .memory_page_native(&p, &f, None, 32, MAX_DOCUMENT_BYTES, Sensitivity::Public)
        .unwrap();
    assert_eq!(public.matched_in_scope, 0);
    assert_eq!(public.unknown_event_time_in_scope, 0);
    assert!(public.items.is_empty());
    assert!(
        host.memory_detail_native(
            &p,
            &first.items[0].memory.memory_id,
            None,
            true,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Public
        )
        .unwrap()
        .is_none()
    );
    host.capture_native(e.source(3, "A new head invalidates the cursor.", SpeakerRole::User))
        .unwrap();
    assert!(
        host.memory_page_native(
            &p,
            &f,
            Some(cursor),
            1,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private
        )
        .is_err()
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn selected_history_sources_and_learned_time_come_from_actual_currently_authorized_storage() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    e.clock.set(NOW + 5000);
    let first = form(
        &e,
        &host,
        &jobs,
        1,
        "During debugging I prefer details.",
        dims(FormationKind::Preference, None, FormationChange::Initial),
        None,
    );
    e.clock.set(NOW + 6000);
    form(
        &e,
        &host,
        &jobs,
        2,
        "Correction: use shorter debugging answers.",
        dims(FormationKind::Preference, None, FormationChange::Correction),
        Some(first.target()),
    );
    let p = profile();
    let state = host.inspect_native().unwrap();
    let before = std::fs::read(e.root.path().join("vault/CURRENT")).unwrap();
    let detail = host
        .memory_detail_native(
            &p,
            &first.memory.memory_id,
            Some(&state.snapshot()),
            false,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private,
        )
        .unwrap()
        .unwrap();
    assert_eq!(detail.history.len(), 2);
    assert!(detail.sources.iter().all(|s| s.text.is_none()));
    assert_eq!(
        detail.item.learned_at,
        Timestamp::from_unix_ms(NOW + 6000).unwrap()
    );
    assert_eq!(
        detail.item.source_captured_at,
        Timestamp::from_unix_ms(NOW).unwrap()
    );
    assert_eq!(
        detail.history[0].learned_at,
        Timestamp::from_unix_ms(NOW + 5000).unwrap()
    );
    let source = host
        .memory_detail_native(
            &p,
            &first.memory.memory_id,
            None,
            true,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private,
        )
        .unwrap()
        .unwrap();
    assert!(
        source
            .sources
            .iter()
            .any(|s| s.text.as_deref() == Some("During debugging I prefer details."))
    );
    assert!(
        source
            .sources
            .iter()
            .all(|s| s.record.evidence_class == EvidenceClass::UserStatement)
    );
    assert!(parse_document::<MemoryReadDetail>(&canonical_bytes(&source).unwrap()).is_ok());
    assert_eq!(
        std::fs::read(e.root.path().join("vault/CURRENT")).unwrap(),
        before
    );
    assert!(
        host.memory_detail_native(
            &p,
            &first.memory.memory_id,
            None,
            true,
            1024,
            Sensitivity::Private
        )
        .is_err()
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn local_lineage_only_names_visible_endpoints_and_deletion_never_leaks_old_details() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let parent = form(
        &e,
        &host,
        &jobs,
        1,
        "I made a pottery cup.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let key = host
        .submit_synthesis(e.input(1, None), vec![parent.target()])
        .unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    let p = profile();
    let detail = host
        .memory_detail_native(
            &p,
            &parent.memory.memory_id,
            None,
            false,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private,
        )
        .unwrap()
        .unwrap();
    assert_eq!(detail.neighbors.len(), 1);
    assert_eq!(detail.neighbors[0].to, parent.target());
    assert_eq!(detail.neighbors[0].kind, MemoryLinkKind::DerivedFrom);
    host.delete_source_native(
        &e.descriptor.created_by,
        parent.decision.input.input_refs[0].source_id.clone(),
    )
    .unwrap();
    assert!(
        host.memory_detail_native(
            &p,
            &parent.memory.memory_id,
            None,
            true,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private
        )
        .unwrap()
        .is_none()
    );
    assert!(
        host.memory_page_native(
            &p,
            &MemoryReadFilter::default(),
            None,
            32,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private
        )
        .unwrap()
        .items
        .is_empty()
    );
    host.lock().unwrap();
    assert!(
        host.memory_page_native(
            &p,
            &MemoryReadFilter::default(),
            None,
            32,
            MAX_DOCUMENT_BYTES,
            Sensitivity::Private
        )
        .is_err()
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn simulations_have_distinct_effects_and_neither_saves_a_preview_or_experiment() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let memory = form(
        &e,
        &host,
        &jobs,
        1,
        "I made a pottery cup.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let state = host.inspect_native().unwrap();
    let p = profile();
    let q = RecallQuery {
        text: "ceramics".into(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(NOW).unwrap(),
        intent: RecallIntent::Current,
        limit: 8,
        max_bytes: 16384,
    };
    let native = host
        .simulate_native_recall(&p, &q, Sensitivity::Private)
        .unwrap();
    assert_eq!(native.effect, MemoryReadEffect::NativeRead);
    let pure = simulate_synthetic_recall(
        &p,
        &q,
        &state.snapshot(),
        &[(memory, Timestamp::from_unix_ms(NOW).unwrap())],
    )
    .unwrap();
    assert_eq!(pure.effect, MemoryReadEffect::SyntheticFixture);
    assert_eq!(pure.trace.selected, native.trace.selected);
    assert!(parse_document::<MemoryReadSimulation>(&canonical_bytes(&native).unwrap()).is_ok());
    assert_eq!(host.inspect_native().unwrap(), state);
    assert!(!e.root.path().join("indexes/core6-attention.json").exists());
    drop(host);
    thread.join().unwrap();
}

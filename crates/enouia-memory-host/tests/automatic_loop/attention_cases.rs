use super::formation_cases::{DimensionalModel, dims, form};
use super::*;
use enouia_memory_vault::automation::ATTENTION_PROJECTION;
fn env() -> Env {
    let mut e = super::formation_cases::env();
    e.policy.valid_until = Timestamp::parse("2035-01-01T00:00:00.000Z").unwrap();
    e
}
fn profile() -> AttentionProfile {
    serde_json::from_value(support::fixture("../attention/profile.json")).unwrap()
}
fn query(e: &Env, text: &str) -> RecallQuery {
    use enouia_memory_contract::foundation::Clock;
    RecallQuery {
        text: text.into(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(e.clock.now_unix_ms()).unwrap(),
        intent: RecallIntent::Current,
        limit: 8,
        max_bytes: 16384,
    }
}
#[test]
fn authorized_alias_selection_beats_lexical_gap_without_writes_or_reinforcement() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    form(
        &e,
        &host,
        &jobs,
        1,
        "I made a pottery cup.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let before = host.inspect_native().unwrap();
    let pointer = std::fs::read(e.root.path().join("vault/CURRENT")).unwrap();
    assert!(
        host.context("ceramics", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    let q = query(&e, "ceramics");
    let read = host
        .select_context_native(&profile(), &q, Sensitivity::Private)
        .unwrap();
    assert_eq!(read.trace.selected.len(), 1);
    assert!(
        read.trace.selected[0]
            .reasons
            .contains(&RecallReason::Alias)
    );
    assert_eq!(read.trace.rendered_bytes, read.text.len());
    assert_eq!(
        host.select_context_native(&profile(), &q, Sensitivity::Private)
            .unwrap()
            .trace,
        read.trace
    );
    assert_eq!(host.inspect_native().unwrap(), before);
    assert_eq!(
        std::fs::read(e.root.path().join("vault/CURRENT")).unwrap(),
        pointer
    );
    assert!(!e.root.path().join(ATTENTION_PROJECTION).exists());
    drop(host);
    thread.join().unwrap();
}
#[test]
fn conditions_permissions_and_rendered_byte_budget_precede_output_and_counts() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let mut d = dims(FormationKind::Preference, None, FormationChange::Initial);
    d.conditions = vec!["During debugging".into()];
    form(
        &e,
        &host,
        &jobs,
        1,
        "I prefer detailed explanations.",
        d,
        None,
    );
    let p = profile();
    let mut q = query(&e, "explanations");
    let read = host
        .select_context_native(&p, &q, Sensitivity::Private)
        .unwrap();
    assert!(read.text.is_empty());
    assert_eq!(read.trace.excluded[0].1, RecallExclusionReason::Conditions);
    q.conditions = vec!["During debugging".into()];
    let read = host
        .select_context_native(&p, &q, Sensitivity::Private)
        .unwrap();
    assert!(read.text.contains("During debugging"));
    q.max_bytes = read.text.len() - 1;
    assert!(
        host.select_context_native(&p, &q, Sensitivity::Private)
            .unwrap()
            .text
            .is_empty()
    );
    let public = host
        .select_context_native(&p, &q, Sensitivity::Public)
        .unwrap();
    assert_eq!(public.trace.considered, 0);
    assert!(
        public.trace.selected.is_empty()
            && public.trace.excluded.is_empty()
            && public.text.is_empty()
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn fading_keeps_protected_episodes_and_historical_rescue_without_completing_open_loops() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let open = form(
        &e,
        &host,
        &jobs,
        1,
        "I still need to choose a class.",
        dims(
            FormationKind::OpenLoop,
            Some(FormationState::Open),
            FormationChange::Initial,
        ),
        None,
    );
    let episode = form(
        &e,
        &host,
        &jobs,
        2,
        "I made my first cup.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let before = host.inspect_native().unwrap();
    e.clock.set(NOW + 200 * 86_400_000);
    let p = profile();
    let projection = host.maintain_attention_native(&p).unwrap();
    assert_eq!(
        projection
            .rows
            .iter()
            .find(|r| r.memory == open.memory)
            .unwrap()
            .state,
        AttentionState::Archived
    );
    assert_eq!(
        projection
            .rows
            .iter()
            .find(|r| r.memory == episode.memory)
            .unwrap()
            .state,
        AttentionState::Protected
    );
    let mut q = query(&e, "class");
    assert!(
        host.select_context_native(&p, &q, Sensitivity::Private)
            .unwrap()
            .text
            .is_empty()
    );
    q.intent = RecallIntent::Historical;
    assert!(
        host.select_context_native(&p, &q, Sensitivity::Private)
            .unwrap()
            .trace
            .selected[0]
            .reasons
            .contains(&RecallReason::HistoricalRescue)
    );
    assert!(
        !host
            .select_context_native(&p, &query(&e, "cup"), Sensitivity::Private)
            .unwrap()
            .text
            .is_empty()
    );
    assert_eq!(host.inspect_native().unwrap(), before);
    drop(host);
    thread.join().unwrap();
}
#[test]
fn stale_or_forged_cache_cannot_reveal_deleted_content_or_revive_historical_grants() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    form(
        &e,
        &host,
        &jobs,
        1,
        "A private pottery memory.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let p = profile();
    host.maintain_attention_native(&p).unwrap();
    let cache = e.root.path().join(ATTENTION_PROJECTION);
    std::fs::write(&cache, b"corrupt cache with no authority").unwrap();
    assert!(
        !host
            .select_context_native(&p, &query(&e, "pottery"), Sensitivity::Private)
            .unwrap()
            .text
            .is_empty()
    );
    let id = e
        .source(1, "A private pottery memory.", SpeakerRole::User)
        .record
        .source_id;
    host.delete_source_native(&e.descriptor.created_by, id)
        .unwrap();
    let mut q = query(&e, "pottery");
    q.intent = RecallIntent::Historical;
    q.at = Timestamp::from_unix_ms(NOW - 1000).unwrap();
    let read = host
        .select_context_native(&p, &q, Sensitivity::Private)
        .unwrap();
    assert!(read.text.is_empty());
    assert_eq!(read.trace.considered, 0);
    let projection = host.maintain_attention_native(&p).unwrap();
    assert!(projection.rows.is_empty());
    host.lock().unwrap();
    assert!(
        host.select_context_native(&p, &q, Sensitivity::Private)
            .is_err()
    );
    assert!(host.maintain_attention_native(&p).is_err());
    drop(host);
    thread.join().unwrap();
}
#[test]
fn native_periodic_maintenance_is_single_cancellable_and_pauses_while_locked() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    form(
        &e,
        &host,
        &jobs,
        1,
        "An unfinished class choice.",
        dims(
            FormationKind::OpenLoop,
            Some(FormationState::Open),
            FormationChange::Initial,
        ),
        None,
    );
    let host = Arc::new(host);
    let mut p = profile();
    p.maintenance_interval_ms = 100;
    let worker = host.start_maintenance(p.clone()).unwrap();
    assert!(host.start_maintenance(p).is_err());
    let cache = e.root.path().join(ATTENTION_PROJECTION);
    let until = Instant::now() + Duration::from_secs(5);
    while !cache.exists() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let pointer = std::fs::read(e.root.path().join("vault/CURRENT")).unwrap();
    e.clock.set(NOW + 200 * 86_400_000);
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        if let Ok(bytes) = std::fs::read(&cache)
            && let Ok(value) = parse_document::<AttentionProjection>(&bytes)
            && value.rows[0].state == AttentionState::Archived
        {
            break;
        }
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(10));
    }
    host.lock().unwrap();
    let saved = std::fs::read(&cache).unwrap();
    e.clock.set(NOW + 201 * 86_400_000);
    std::thread::sleep(Duration::from_millis(220));
    assert_eq!(std::fs::read(&cache).unwrap(), saved);
    worker.join().unwrap();
    assert_eq!(
        std::fs::read(e.root.path().join("vault/CURRENT")).unwrap(),
        pointer
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn explicit_event_window_uses_event_time_and_unknown_dates_never_borrow_capture_time() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let mut d = dims(FormationKind::Episode, None, FormationChange::Initial);
    d.event_time = Some(FormationTime {
        from: Timestamp::parse("2026-03-05T00:00:00.000Z").unwrap(),
        until: None,
    });
    form(&e, &host, &jobs, 1, "I made a cup in March.", d, None);
    form(
        &e,
        &host,
        &jobs,
        2,
        "Some time ago I made another cup.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let mut q = query(&e, "cup");
    q.intent = RecallIntent::Historical;
    q.event_window = Some(FormationTime {
        from: Timestamp::parse("2026-03-01T00:00:00.000Z").unwrap(),
        until: Some(Timestamp::parse("2026-04-01T00:00:00.000Z").unwrap()),
    });
    let read = host
        .select_context_native(&profile(), &q, Sensitivity::Private)
        .unwrap();
    assert_eq!(read.trace.selected.len(), 1);
    assert!(read.text.contains("March"));
    assert_eq!(read.trace.excluded.len(), 1);
    drop(host);
    thread.join().unwrap();
}

#[test]
fn old_ranking_clock_cannot_make_expired_current_state_valid_again() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let mut d = dims(FormationKind::Fact, None, FormationChange::Initial);
    d.valid_time = Some(FormationTime {
        from: Timestamp::from_unix_ms(NOW).unwrap(),
        until: Timestamp::from_unix_ms(NOW + 1000),
    });
    form(&e, &host, &jobs, 1, "A temporary venue report.", d, None);
    let mut q = query(&e, "venue");
    e.clock.set(NOW + 2000);
    let read = host
        .select_context_native(&profile(), &q, Sensitivity::Private)
        .unwrap();
    assert!(read.text.is_empty());
    assert_eq!(read.trace.excluded[0].1, RecallExclusionReason::Validity);
    assert_eq!(
        read.trace.authorized_at,
        Timestamp::from_unix_ms(NOW + 2000).unwrap()
    );
    q.intent = RecallIntent::Historical;
    assert!(
        !host
            .select_context_native(&profile(), &q, Sensitivity::Private)
            .unwrap()
            .text
            .is_empty()
    );
    drop(host);
    thread.join().unwrap();
}

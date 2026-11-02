//! Separate Agent authority, real witnessed V2 data, no Provider or OS identity claim.
use super::formation_cases::{DimensionalModel, dims, form};
use super::*;
use enouia_memory_contract::foundation::Clock;
use enouia_memory_vault::automation::AgentReadScope;
fn env() -> Env {
    super::formation_cases::env()
}
fn profile() -> AttentionProfile {
    serde_json::from_value(support::fixture("../attention/profile.json")).unwrap()
}
fn agent() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn scope(e: &Env, sources: &[u8], source_read: bool) -> AgentReadScope {
    AgentReadScope {
        subject_id: e.policy.subject_id.clone(),
        project_id: e.policy.project_id.clone(),
        source_ids: sources
            .iter()
            .map(|n| SourceId::parse(&id("src", *n)).unwrap())
            .collect(),
        max_sensitivity: Sensitivity::Private,
        source_read,
        valid_until: Timestamp::from_unix_ms(e.clock.now_unix_ms() + 120_000).unwrap(),
    }
}
#[test]
fn agent_v2_pages_filter_before_counts_and_preserve_formation_receipts_without_writes() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let first = form(
        &e,
        &host,
        &jobs,
        1,
        "I made a synthetic pottery cup.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let hidden = form(
        &e,
        &host,
        &jobs,
        2,
        "Hidden synthetic source episode.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let before = host.inspect_native().unwrap().current();
    {
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        let page = reader
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                32,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        assert_eq!(page.matched_in_scope, 1);
        assert_eq!(page.unknown_event_time_in_scope, 1);
        assert_eq!(page.items.len(), 1);
        assert_eq!(page.items[0].memory, first.memory);
        assert_eq!(page.items[0].system_actor, e.descriptor.system_actor);
        assert_eq!(
            page.items[0].receipt.as_ref().unwrap().committed_in,
            first.committed_in
        );
        let json = serde_json::to_value(&page).unwrap();
        assert_eq!(json["capability"], "memory-agent-read-1");
        assert_eq!(json["effect"], "agent_read");
        assert_eq!(json["principal"], serde_json::to_value(agent()).unwrap());
        assert!(json.get("maintenance_worker_alive").is_none());
        assert!(!json.to_string().contains(hidden.memory.memory_id.as_str()));
        assert!(
            parse_document::<MemoryReadPage>(&canonical_bytes(&page).unwrap()).is_err(),
            "no relabeling as System native result"
        );
        assert!(
            reader
                .detail(
                    &profile(),
                    &hidden.memory.memory_id,
                    None,
                    false,
                    MAX_DOCUMENT_BYTES
                )
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .detail(
                    &profile(),
                    &MemoryId::parse(&id("mem", 99)).unwrap(),
                    None,
                    false,
                    MAX_DOCUMENT_BYTES
                )
                .unwrap()
                .is_none()
        );
        let detail = reader
            .detail(
                &profile(),
                &first.memory.memory_id,
                None,
                false,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap()
            .unwrap();
        assert!(!detail.source_read);
        assert!(detail.sources.is_empty());
        assert!(
            detail
                .history
                .iter()
                .all(|i| i.memory.memory_id == first.memory.memory_id)
        );
        assert!(
            reader
                .detail(
                    &profile(),
                    &first.memory.memory_id,
                    None,
                    true,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
    }
    assert_eq!(host.inspect_native().unwrap().current(), before);
    drop(host);
    thread.join().unwrap();
}
#[test]
fn agent_v2_source_read_is_independent_and_cursors_bind_grant_and_snapshot() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let first = form(
        &e,
        &host,
        &jobs,
        1,
        "One synthetic event.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    form(
        &e,
        &host,
        &jobs,
        2,
        "Second synthetic event.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    {
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1, 2], true))
            .unwrap();
        let first_page = reader
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                1,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        assert_eq!(first_page.matched_in_scope, 2);
        assert!(!first_page.complete_in_scope);
        let cursor = first_page.next.as_ref().unwrap();
        let second = reader
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                Some(cursor),
                1,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        assert!(second.complete_in_scope);
        let other = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1, 2], true))
            .unwrap();
        assert!(
            other
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    Some(cursor),
                    1,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
        assert!(
            host.memory_page_native(
                &profile(),
                &MemoryReadFilter::default(),
                Some(cursor),
                1,
                MAX_DOCUMENT_BYTES,
                Sensitivity::Private
            )
            .is_err()
        );
        let detail = reader
            .detail(
                &profile(),
                &first.memory.memory_id,
                Some(&first_page.snapshot),
                true,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap()
            .unwrap();
        assert!(detail.source_read);
        assert_eq!(detail.sources.len(), 1);
        assert_eq!(
            detail.sources[0].text.as_deref(),
            Some("One synthetic event.")
        );
        host.capture_native(e.source(
            3,
            "New snapshot, no implicit resumption.",
            SpeakerRole::User,
        ))
        .unwrap();
        assert!(
            reader
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    Some(cursor),
                    1,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
        assert!(
            reader
                .detail(
                    &profile(),
                    &first.memory.memory_id,
                    Some(&first_page.snapshot),
                    false,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
    }
    drop(host);
    thread.join().unwrap();
}
#[test]
fn agent_v2_revocation_lock_unlock_expiry_and_policy_change_never_revive_readers() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    {
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        reader.revoke();
        assert!(
            reader
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    None,
                    1,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
        let old = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        host.unlock(&e.descriptor.system_actor).unwrap();
        assert!(
            old.page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                1,
                MAX_DOCUMENT_BYTES
            )
            .is_err()
        );
        let old = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        host.lock().unwrap();
        assert!(
            old.page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                1,
                MAX_DOCUMENT_BYTES
            )
            .is_err()
        );
        host.unlock(&e.descriptor.system_actor).unwrap();
        assert!(
            old.page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                1,
                MAX_DOCUMENT_BYTES
            )
            .is_err()
        );
        let expired = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        e.clock.set(NOW + 120_000);
        assert!(
            expired
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    None,
                    1,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
        e.clock.set(NOW + 100_000);
        assert!(
            expired
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    None,
                    1,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
        e.clock.set(NOW + 120_000);
        let rollback = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        rollback
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                1,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        e.clock.set(NOW + 120_500);
        rollback
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                1,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        e.clock.set(NOW + 120_100);
        assert!(
            rollback
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    None,
                    1,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
        e.clock.set(NOW + 121_000);
        let stale = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        let mut policy = e.policy.clone();
        policy.revision = Revision::new(2).unwrap();
        host.set_policy_native(&e.descriptor.created_by, policy)
            .unwrap();
        assert!(
            stale
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    None,
                    1,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
    }
}
#[test]
fn agent_v2_admission_rejects_impersonation_foreign_scope_and_cross_vault_permit() {
    let e = env();
    let vault = e.vault(Faults::none());
    for bad in [
        e.descriptor.created_by.clone(),
        e.descriptor.system_actor.clone(),
    ] {
        assert!(
            vault
                .authorize_agent_read(&e.descriptor.created_by, bad, scope(&e, &[1], false))
                .is_err()
        );
    }
    assert!(
        vault
            .authorize_agent_read(&agent(), agent(), scope(&e, &[1], false))
            .is_err()
    );
    for mode in [
        "subject",
        "project",
        "source",
        "sensitivity",
        "duplicate",
        "empty",
        "expiry",
    ] {
        let mut s = scope(&e, &[1], false);
        match mode {
            "subject" => s.subject_id = SubjectId::parse(&id("sub", 99)).unwrap(),
            "project" => s.project_id = Some(ProjectId::parse(&id("prj", 99)).unwrap()),
            "source" => s.source_ids = vec![SourceId::parse(&id("src", 99)).unwrap()],
            "sensitivity" => s.max_sensitivity = Sensitivity::HighlySensitive,
            "duplicate" => s.source_ids.push(s.source_ids[0].clone()),
            "empty" => s.source_ids.clear(),
            _ => s.valid_until = Timestamp::from_unix_ms(NOW + 300_001).unwrap(),
        }
        assert!(
            vault
                .authorize_agent_read(&e.descriptor.created_by, agent(), s)
                .is_err(),
            "{mode}"
        );
    }
    let permit = vault
        .authorize_agent_read(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
        .unwrap();
    let other = env();
    let other_vault = other.vault(Faults::none());
    assert!(
        other_vault
            .authorized_agent_read_data(&permit, None)
            .is_err()
    );
}
#[test]
fn agent_v2_hidden_dependencies_and_deletion_do_not_leak_edges_history_or_counts() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let parent = form(
        &e,
        &host,
        &jobs,
        1,
        "A synthetic pottery episode.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let hidden = form(
        &e,
        &host,
        &jobs,
        2,
        "A separate private synthetic episode.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let key = host
        .submit_synthesis(e.input(1, None), vec![parent.target(), hidden.target()])
        .unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    let theme = host
        .inspect_native()
        .unwrap()
        .data
        .memories
        .last()
        .unwrap()
        .clone();
    {
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], true))
            .unwrap();
        let page = reader
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                32,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        assert_eq!(page.matched_in_scope, 1);
        assert!(
            !String::from_utf8(canonical_bytes(&page).unwrap())
                .unwrap()
                .contains(theme.memory.memory_id.as_str())
        );
        let detail = reader
            .detail(
                &profile(),
                &parent.memory.memory_id,
                None,
                true,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap()
            .unwrap();
        assert!(detail.neighbors.is_empty());
        let broad = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1, 2], true))
            .unwrap();
        let detail = broad
            .detail(
                &profile(),
                &theme.memory.memory_id,
                None,
                false,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap()
            .unwrap();
        assert_eq!(detail.item.knowledge, KnowledgeState::Hypothesis);
        assert_eq!(detail.item.adoption, AdoptionState::Staged);
        host.delete_source_native(
            &e.descriptor.created_by,
            SourceId::parse(&id("src", 1)).unwrap(),
        )
        .unwrap();
        let empty = reader
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                32,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        assert_eq!(empty.matched_in_scope, 0);
        assert_eq!(empty.unknown_event_time_in_scope, 0);
        assert!(
            reader
                .detail(
                    &profile(),
                    &parent.memory.memory_id,
                    None,
                    true,
                    MAX_DOCUMENT_BYTES
                )
                .unwrap()
                .is_none()
        );
    }
    drop(host);
    thread.join().unwrap();
}

#[test]
fn agent_v2_history_sensitivity_budget_and_old_layout_are_independent_gates() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let old = form(
        &e,
        &host,
        &jobs,
        1,
        "Old synthetic episode body.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let updated = form(
        &e,
        &host,
        &jobs,
        2,
        "Updated synthetic episode body.",
        dims(FormationKind::Episode, None, FormationChange::Update),
        Some(old.target()),
    );
    {
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[2], true))
            .unwrap();
        let detail = reader
            .detail(
                &profile(),
                &updated.memory.memory_id,
                None,
                true,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap()
            .unwrap();
        assert_eq!(detail.item.memory, updated.memory);
        assert!(
            detail
                .history
                .iter()
                .all(|m| m.memory.revision == updated.memory.revision)
        );
        assert_eq!(detail.sources.len(), 1);
        assert_eq!(
            detail.sources[0].record.source_id,
            SourceId::parse(&id("src", 2)).unwrap()
        );
        let mut s = scope(&e, &[1, 2], true);
        s.max_sensitivity = Sensitivity::Public;
        let public = host
            .agent_reader(&e.descriptor.created_by, agent(), s)
            .unwrap();
        let empty = public
            .page(
                &profile(),
                &MemoryReadFilter::default(),
                None,
                32,
                MAX_DOCUMENT_BYTES,
            )
            .unwrap();
        assert_eq!(empty.matched_in_scope, 0);
        assert!(
            public
                .detail(
                    &profile(),
                    &updated.memory.memory_id,
                    None,
                    true,
                    MAX_DOCUMENT_BYTES
                )
                .unwrap()
                .is_none()
        );
        assert!(
            reader
                .page(
                    &profile(),
                    &MemoryReadFilter::default(),
                    None,
                    0,
                    MAX_DOCUMENT_BYTES
                )
                .is_err()
        );
        assert!(
            reader
                .page(&profile(), &MemoryReadFilter::default(), None, 1, 2047)
                .is_err()
        );
        assert!(
            reader
                .detail(&profile(), &updated.memory.memory_id, None, true, 2048)
                .is_err(),
            "required data is not silently truncated"
        );
    }
    drop(host);
    thread.join().unwrap();
    let old = Env::new();
    let vault = old.vault(Faults::none());
    assert!(
        vault
            .authorize_agent_read(
                &old.descriptor.created_by,
                agent(),
                scope(&old, &[1], false)
            )
            .is_err(),
        "HostV1 automation layout must not gain this capability"
    );
}

#[test]
fn agent_v2_search_axes_and_recall_trace_use_current_authorization_without_touching_memory() {
    use enouia_memory_host::automation::agent_read::{AgentSearchFilter, SearchTimeAxis};
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let mut d = dims(FormationKind::Episode, None, FormationChange::Initial);
    d.valid_time = Some(FormationTime {
        from: Timestamp::from_unix_ms(NOW).unwrap(),
        until: Timestamp::from_unix_ms(NOW + 5000),
    });
    let memory = form(&e, &host, &jobs, 1, "A synthetic pottery source.", d, None);
    let before = host.inspect_native().unwrap().current();
    {
        let reader = host
            .agent_reader(&e.descriptor.created_by, agent(), scope(&e, &[1], false))
            .unwrap();
        let mut f = AgentSearchFilter {
            window: Some(FormationTime {
                from: Timestamp::from_unix_ms(NOW - 1).unwrap(),
                until: Timestamp::from_unix_ms(NOW + 1),
            }),
            ..Default::default()
        };
        let event = reader
            .search(&profile(), &f, None, 32, MAX_DOCUMENT_BYTES)
            .unwrap();
        assert_eq!(event.page.matched_in_scope, 0);
        assert_eq!(event.unknown_time_in_scope, 1);
        f.include_unknown_time = true;
        assert_eq!(
            reader
                .search(&profile(), &f, None, 32, MAX_DOCUMENT_BYTES)
                .unwrap()
                .page
                .matched_in_scope,
            1
        );
        for axis in [
            SearchTimeAxis::Learned,
            SearchTimeAxis::SourceCaptured,
            SearchTimeAxis::Valid,
        ] {
            f.time_axis = axis;
            let found = reader
                .search(&profile(), &f, None, 32, MAX_DOCUMENT_BYTES)
                .unwrap();
            assert_eq!(found.page.matched_in_scope, 1);
            assert_eq!(found.unknown_time_in_scope, 0);
        }
        let q = RecallQuery {
            text: "pottery".into(),
            conditions: vec![],
            event_window: None,
            at: Timestamp::from_unix_ms(NOW).unwrap(),
            intent: RecallIntent::Current,
            limit: 8,
            max_bytes: 4096,
        };
        let request = RequestId::parse(&id("req", 11)).unwrap();
        let recall = reader.recall(&profile(), &q, request.clone()).unwrap();
        assert_eq!(recall.items.len(), 1);
        assert_eq!(recall.items[0].memory, memory.memory);
        assert_eq!(
            canonical_bytes(&reader.context_trace(&request).unwrap().unwrap()).unwrap(),
            canonical_bytes(&recall).unwrap()
        );
        e.clock.set(NOW + 6000);
        let expired = reader
            .recall(&profile(), &q, RequestId::parse(&id("req", 12)).unwrap())
            .unwrap();
        assert!(
            expired.items.is_empty(),
            "old ranking time never revives validity"
        );
        host.capture_native(e.source(
            2,
            "A fresh snapshot invalidates old Context.",
            SpeakerRole::User,
        ))
        .unwrap();
        assert!(
            reader
                .context_trace(&RequestId::parse(&id("req", 12)).unwrap())
                .is_err()
        );
    }
    assert_ne!(host.inspect_native().unwrap().current(), before);
    drop(host);
    thread.join().unwrap();
}

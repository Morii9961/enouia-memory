#![cfg(any(windows, target_os = "linux"))]
#[path = "support/owner_read_grant_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, json::Revision, next::*, time::Timestamp};
use enouia_memory_host::automation::qualified_agent::QualifiedAgentHost;
use fixture::*;

#[test]
fn historical_binding_survives_formation_expiry_and_disable_only_through_explicit_grant() {
    let (e, v, mut p, target) = correction_rig(SpeakerRole::User);
    let grant = read_grant(&e, &p);
    let mut forged = grant.clone();
    forged.policies[0].revision = Revision::new(2).unwrap();
    assert!(
        v.plan_owner_read_grant(&e.descriptor.created_by, forged)
            .is_err()
    );
    assert!(
        v.plan_owner_read_grant(&grant.principal, grant.clone())
            .is_err()
    );
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    let before = v.snapshot().unwrap();
    assert_eq!(plan.snapshot, before);
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    let published = v.snapshot().unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    assert_eq!(v.snapshot().unwrap(), published);
    p.active = false;
    p.revision = Revision::new(2).unwrap();
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    e.clock.set(p.valid_until.unix_ms());
    let mut s = scope(&p);
    s.source_ids = grant.source_ids.clone();
    s.valid_until = Timestamp::from_unix_ms(p.valid_until.unix_ms() + 120_000).unwrap();
    assert!(
        v.authorize_initialized_agent_read(
            &e.descriptor.created_by,
            grant.principal.clone(),
            s.clone()
        )
        .is_err()
    );
    let reader = v
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s,
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&reader, Some(&target.target.memory.memory_id), |d| {
            assert_eq!(
                d.current.memories[0].memory.decision.input.policy,
                grant.policies[0]
            );
            (
                d.current.memories.len(),
                d.history.len(),
                d.current.sources.len(),
            )
        })
        .unwrap(),
        (1, 1, 2)
    );
    // Independent reads confer no formation authority under either revision.
    let source = e.source(8, Sensitivity::Public);
    assert!(
        v.prepare(&e.descriptor.system_actor, e.input(&source, &p, 8))
            .is_err()
    );
    let status = v.owner_read_status(&e.descriptor.created_by).unwrap();
    assert_eq!(status.policies.len(), 2);
    assert_eq!(status.grants, vec![grant.clone()]);
    let head = v.snapshot().unwrap();
    drop(v);
    let reopened = e.open();
    assert!(reopened.read_agent(&reader, None, |_| ()).is_err());
    assert_eq!(
        reopened
            .owner_read_grant(&e.descriptor.created_by, &grant.grant_id)
            .unwrap(),
        grant
    );
    assert_eq!(reopened.snapshot().unwrap(), head);
}

#[test]
fn renewal_and_withdrawal_invalidate_old_permits_and_never_revive_deleted_evidence() {
    let (e, v, p, target) = correction_rig(SpeakerRole::User);
    let mut grant = read_grant(&e, &p);
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    let mut s = scope(&p);
    s.source_ids = grant.source_ids.clone();
    let first = v
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s.clone(),
        )
        .unwrap();
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let reader = host
        .reader_with_owner_grant(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s.clone(),
        )
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let request = RequestId::parse(&id("req", 900)).unwrap();
    let query = RecallQuery {
        intent: RecallIntent::Browse,
        text: String::new(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(NOW).unwrap(),
        limit: 4,
        max_bytes: 16384,
    };
    reader.recall(&profile, &query, request.clone()).unwrap();
    assert!(reader.context_trace(&request).unwrap().is_some());
    let mut session = enouia_memory_host::mcp::opening::Server::new(&reader, &profile).unwrap();
    session.handle(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"1"}}}));
    grant.revision = Revision::new(2).unwrap();
    grant.valid_until = Timestamp::from_unix_ms(grant.valid_until.unix_ms() + 86_400_000).unwrap();
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    assert!(v.read_agent(&first, None, |_| ()).is_err());
    assert!(reader.context_trace(&request).is_err());
    session.handle(&serde_json::json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}));
    assert!(session.closed());
    drop(session);
    drop(reader);
    drop(host);
    let second = v
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s.clone(),
        )
        .unwrap();
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![target.target.memory.memory_id],
    )
    .unwrap();
    assert_eq!(
        v.read_agent(&second, None, |d| d.current.memories.len())
            .unwrap(),
        0
    );
    grant.revision = Revision::new(3).unwrap();
    grant.active = false;
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    assert!(v.check_agent_context_live(&second).is_err());
    assert!(
        v.authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s.clone()
        )
        .is_err()
    );
    drop(v);
    let reopened = e.open();
    assert!(
        !reopened
            .owner_read_grant(&e.descriptor.created_by, &grant.grant_id)
            .unwrap()
            .active
    );
    grant.revision = Revision::new(4).unwrap();
    grant.active = true;
    let plan = reopened
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    reopened
        .commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    let new = reopened
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal,
            &grant.grant_id,
            s,
        )
        .unwrap();
    assert_eq!(
        reopened
            .read_agent(&new, None, |d| d.current.memories.len())
            .unwrap(),
        0
    );
}

#[test]
fn explicit_grant_keeps_scope_expiry_source_text_and_plan_freshness_gates() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let mut grant = read_grant(&e, &p);
    grant.source_read = false;
    grant.valid_until = Timestamp::from_unix_ms(NOW + 60_000).unwrap();
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    let mut stale = plan.clone();
    stale.snapshot.policy_watermark = Revision::new(99).unwrap();
    assert!(
        v.commit_owner_read_grant(&e.descriptor.created_by, &stale)
            .is_err()
    );
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    let mut s = scope(&p);
    s.source_ids = grant.source_ids.clone();
    s.source_read = false;
    s.valid_until = grant.valid_until.clone();
    for change in 0..6 {
        let mut wrong = s.clone();
        let mut principal = grant.principal.clone();
        match change {
            0 => wrong.subject_id = SubjectId::parse(&id("sub", 99)).unwrap(),
            1 => wrong.project_id = Some(ProjectId::parse(&id("prj", 99)).unwrap()),
            2 => wrong.max_sensitivity = Sensitivity::HighlySensitive,
            3 => wrong
                .source_ids
                .push(SourceId::parse(&id("src", 99)).unwrap()),
            4 => wrong.source_read = true,
            _ => principal.actor_id = PrincipalId::parse(&id("prn", 99)).unwrap(),
        }
        assert!(
            v.authorize_owner_grant_read(
                &e.descriptor.created_by,
                principal,
                &grant.grant_id,
                wrong
            )
            .is_err()
        );
    }
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let reader = host
        .reader_with_owner_grant(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s.clone(),
        )
        .unwrap();
    assert!(
        reader
            .source(&grant.source_ids[0], None, 0, None, 8192)
            .is_err()
    );
    let mut too_long = s.clone();
    too_long.valid_until = Timestamp::from_unix_ms(NOW + 60_001).unwrap();
    assert!(
        host.reader_with_owner_grant(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            too_long
        )
        .is_err()
    );
    e.clock.set(NOW + 60_000);
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    assert!(
        reader
            .detail(
                &profile,
                &MemoryId::parse(&id("mem", 1)).unwrap(),
                None,
                false,
                8192
            )
            .is_err()
    );
    assert!(
        v.authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal,
            &grant.grant_id,
            s
        )
        .is_err()
    );
}

#[test]
fn backup_replays_the_grant_and_stays_quarantined_before_owner_activation() {
    use enouia_memory_vault::{
        OsIdSource,
        automation::segmented::{SegmentedAutomationVault, backup::*},
        fault::Faults,
    };
    use std::sync::Arc;
    let (e, v, mut p, _) = correction_rig(SpeakerRole::User);
    let grant = read_grant(&e, &p);
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    p.active = false;
    p.revision = Revision::new(2).unwrap();
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let before = v.snapshot().unwrap();
    drop(v);
    let export = Env::new();
    let target = Env::new();
    export_owner(&e.root, &export.root, e.clock.clone(), Arc::new(OsIdSource)).unwrap();
    restore_owner(&export.root, &target.root, e.clock.as_ref()).unwrap();
    assert!(
        SegmentedAutomationVault::open(
            &target.root,
            e.descriptor.host_id.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    assert!(
        SegmentedAutomationVault::recover(
            &target.root,
            e.descriptor.host_id.clone(),
            &e.descriptor.system_actor,
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    activate_owner(&target.root, &export.root, e.clock.as_ref()).unwrap();
    let restored = SegmentedAutomationVault::open(
        &target.root,
        e.descriptor.host_id.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
        Faults::none(),
    )
    .unwrap();
    assert_eq!(restored.snapshot().unwrap(), before);
    assert_eq!(
        restored
            .owner_read_grant(&e.descriptor.created_by, &grant.grant_id)
            .unwrap(),
        grant
    );
    let mut s = scope(&p);
    s.source_ids = grant.source_ids.clone();
    let permit = restored
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal,
            &grant.grant_id,
            s,
        )
        .unwrap();
    assert_eq!(
        restored
            .read_agent(&permit, None, |d| d.current.memories.len())
            .unwrap(),
        1
    );
}

#[test]
fn selected_historical_revisions_do_not_launder_lineage_or_private_sources() {
    let (e, v, mut p, target) = correction_rig(SpeakerRole::User);
    let source = e.source(2, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![source.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let mut input = e.input(&source, &p, 2);
    let parent = e.source(8, Sensitivity::Public).reference();
    input.input_refs.push(parent.clone());
    input.context_source_refs.push(parent);
    input.interpretation = Interpretation::Inference;
    input
        .dimensions
        .as_mut()
        .unwrap()
        .derived_from
        .push(target.target);
    publish(&e, &v, input);
    let mut grant = read_grant(&e, &p);
    grant.source_ids = vec![source.record.source_id.clone()];
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    let mut s = scope(&p);
    s.source_ids = grant.source_ids.clone();
    let narrow = v
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s.clone(),
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&narrow, None, |d| d.current.memories.len())
            .unwrap(),
        0
    );
    p.revision = Revision::new(2).unwrap();
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let private = e.source(4, Sensitivity::Private);
    v.capture(
        &e.descriptor.system_actor,
        vec![private.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    publish(&e, &v, e.input(&private, &p, 4));
    grant.revision = Revision::new(2).unwrap();
    grant.policies.push(p.reference());
    grant.source_ids.push(private.record.source_id);
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    v.commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    p.revision = Revision::new(3).unwrap();
    p.active = false;
    v.set_policy(&e.descriptor.created_by, p).unwrap();
    s.source_ids = grant.source_ids.clone();
    let private = v
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal.clone(),
            &grant.grant_id,
            s.clone(),
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&private, None, |d| d.current.memories.len())
            .unwrap(),
        1
    );
    s.max_sensitivity = Sensitivity::Public;
    let public = v
        .authorize_owner_grant_read(
            &e.descriptor.created_by,
            grant.principal,
            &grant.grant_id,
            s,
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&public, None, |d| (
            d.current.memories.len(),
            d.current.sources.len()
        ))
        .unwrap(),
        (0, 1)
    );
}

#[test]
fn unknown_grant_publication_recovers_the_original_revision_without_new_commit() {
    use enouia_memory_vault::{
        OsIdSource,
        automation::{AutomationFault, segmented::SegmentedAutomationVault},
        fault::{FaultAction, FaultPoint, Faults},
    };
    use std::sync::Arc;
    let faults = Faults::armed();
    let (e, v, p, _) = correction_rig_with(SpeakerRole::User, faults.clone());
    let grant = read_grant(&e, &p);
    let plan = v
        .plan_owner_read_grant(&e.descriptor.created_by, grant.clone())
        .unwrap();
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(112));
    assert!(matches!(
        v.commit_owner_read_grant(&e.descriptor.created_by, &plan),
        Err(AutomationFault::OutcomeUnknown)
    ));
    assert!(v.owner_read_status(&e.descriptor.created_by).is_err());
    drop(v);
    let recovered = SegmentedAutomationVault::recover(
        &e.root,
        e.descriptor.host_id.clone(),
        &e.descriptor.system_actor,
        e.clock.clone(),
        Arc::new(OsIdSource),
        Faults::none(),
    )
    .unwrap();
    assert_eq!(
        recovered
            .owner_read_grant(&e.descriptor.created_by, &grant.grant_id)
            .unwrap(),
        grant
    );
    let head = recovered.snapshot().unwrap();
    recovered
        .commit_owner_read_grant(&e.descriptor.created_by, &plan)
        .unwrap();
    assert_eq!(recovered.snapshot().unwrap(), head);
}

#[test]
fn grant_backed_reader_renews_in_place_and_keeps_its_recall_context() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let grant = read_grant(&e, &p);
    let plan = v.plan_owner_read_grant(owner, grant.clone()).unwrap();
    v.commit_owner_read_grant(owner, &plan).unwrap();
    let mut s = scope(&p);
    s.source_ids = grant.source_ids.clone();
    let raw = v
        .authorize_owner_grant_read(owner, grant.principal.clone(), &grant.grant_id, s.clone())
        .unwrap();
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let reader = host
        .reader_with_owner_grant(owner, grant.principal.clone(), &grant.grant_id, s)
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    let request = RequestId::parse(&id("req", 901)).unwrap();
    let query = RecallQuery {
        intent: RecallIntent::Browse,
        text: String::new(),
        conditions: vec![],
        event_window: None,
        at: Timestamp::from_unix_ms(NOW).unwrap(),
        limit: 4,
        max_bytes: 16384,
    };
    reader.recall(&profile, &query, request.clone()).unwrap();
    let hash = reader.scope_hash().clone();
    // ADR-MEM-163: past the permit's own end the grant renews the host reader in
    // place. The scope hash is unchanged, so its recall context and cursors
    // survive; a raw permit that nobody renews still ends.
    e.clock.set(NOW + 200_000);
    assert!(reader.context_trace(&request).unwrap().is_some());
    assert_eq!(reader.scope_hash(), &hash);
    assert!(v.check_agent_read_live(&raw).is_err());
}

#[test]
fn grant_coverage_lists_later_windows_and_sources_and_refreshes_only_through_a_plan() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let grant = read_grant(&e, &p);
    let plan = v.plan_owner_read_grant(owner, grant.clone()).unwrap();
    v.commit_owner_read_grant(owner, &plan).unwrap();
    // Nothing new yet: both captured sources of the only policy are listed.
    let coverage = v.owner_read_grant_coverage(owner, &grant.grant_id).unwrap();
    assert_eq!(coverage.grant, grant);
    assert!(coverage.uncovered_policies.is_empty());
    assert!(coverage.uncovered_sources.is_empty());
    assert!(coverage.refreshed.is_none());
    assert!(
        v.owner_read_grant_coverage(&grant.principal, &grant.grant_id)
            .is_err()
    );
    // A later window of the same subject, and a later capture in the old one.
    let window = e.policy(2, 20, 4);
    v.set_policy(owner, window.clone()).unwrap();
    let later = [
        e.source(20, Sensitivity::Public),
        e.source(3, Sensitivity::Public),
    ];
    v.capture(
        &e.descriptor.system_actor,
        later.to_vec(),
        RecordCatalog::default(),
    )
    .unwrap();
    let before = v.snapshot().unwrap();
    let coverage = v.owner_read_grant_coverage(owner, &grant.grant_id).unwrap();
    assert_eq!(v.snapshot().unwrap(), before, "coverage writes nothing");
    assert_eq!(coverage.uncovered_policies, vec![window.reference()]);
    let mut expected = vec![
        later[0].record.source_id.clone(),
        later[1].record.source_id.clone(),
    ];
    expected.sort();
    assert_eq!(coverage.uncovered_sources, expected);
    let refreshed = coverage.refreshed.unwrap();
    assert_eq!(refreshed.revision.get(), 2);
    assert_eq!(refreshed.policies, vec![p.reference(), window.reference()]);
    assert_eq!(refreshed.source_ids.len(), 4);
    assert_eq!(
        (
            &refreshed.principal,
            refreshed.max_sensitivity,
            &refreshed.valid_until,
            refreshed.allow_requests
        ),
        (
            &grant.principal,
            grant.max_sensitivity,
            &grant.valid_until,
            grant.allow_requests
        )
    );
    // The grant itself is unchanged until Morii plans and confirms it.
    assert_eq!(v.owner_read_grant(owner, &grant.grant_id).unwrap(), grant);
    let plan = v.plan_owner_read_grant(owner, refreshed.clone()).unwrap();
    v.commit_owner_read_grant(owner, &plan).unwrap();
    let coverage = v.owner_read_grant_coverage(owner, &grant.grant_id).unwrap();
    assert_eq!(coverage.grant, refreshed);
    assert!(coverage.uncovered_policies.is_empty() && coverage.uncovered_sources.is_empty());
    assert!(coverage.refreshed.is_none());
    // A revoked grant reports its gap but offers no refresh.
    let mut revoked = refreshed.clone();
    revoked.revision = Revision::new(3).unwrap();
    revoked.active = false;
    let plan = v.plan_owner_read_grant(owner, revoked).unwrap();
    v.commit_owner_read_grant(owner, &plan).unwrap();
    v.capture(
        &e.descriptor.system_actor,
        vec![e.source(21, Sensitivity::Public)],
        RecordCatalog::default(),
    )
    .unwrap();
    let coverage = v.owner_read_grant_coverage(owner, &grant.grant_id).unwrap();
    assert_eq!(coverage.uncovered_sources.len(), 1);
    assert!(coverage.refreshed.is_none());
}

#[test]
fn a_following_grant_reads_and_files_requests_for_later_windows_until_revoked() {
    use enouia_memory_vault::automation::segmented::{
        AgentNoteRequest, RuntimeCaptureScope, SegmentedAgentScope,
    };
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let commit = |g: OwnerReadGrant| {
        let plan = v.plan_owner_read_grant(owner, g).unwrap();
        v.commit_owner_read_grant(owner, &plan).unwrap();
    };
    let mut follow = read_grant(&e, &p);
    follow.allow_requests = true;
    follow.follow_new_sources = true;
    let mut fixed = read_grant(&e, &p);
    fixed.grant_id = PolicyId::parse(&id("pol", 9101)).unwrap();
    fixed.principal = actor(ActorType::Agent, 89);
    fixed.allow_requests = true;
    commit(follow.clone());
    commit(fixed.clone());
    let listed = |g: &OwnerReadGrant| {
        let mut s = scope(&p);
        s.source_ids = g.source_ids.clone();
        s
    };
    let permit = |g: &OwnerReadGrant, s: SegmentedAgentScope| {
        v.authorize_owner_grant_read(owner, g.principal.clone(), &g.grant_id, s)
    };
    let following = permit(&follow, listed(&follow)).unwrap();
    let pinned = permit(&fixed, listed(&fixed)).unwrap();
    let count = |r: &enouia_memory_vault::automation::segmented::SegmentedAgentPermit| {
        v.read_agent(r, None, |d| d.current.memories.len()).unwrap()
    };
    let before = count(&following);
    assert_eq!(count(&pinned), before);
    // A later capture window of the same subject: one Runtime-captured turn
    // and one formed memory.
    let window = e.policy(2, 20, 4);
    v.set_policy(owner, window.clone()).unwrap();
    let capture_permit = v
        .authorize_owner_runtime_capture(
            owner,
            actor(ActorType::Client, 77),
            RuntimeCaptureScope {
                policy_id: window.policy_id.clone(),
                source_ids: window.source_ids.clone(),
                max_sensitivity: Sensitivity::Private,
                platform: "enouia-runtime".into(),
                account_scope: "account-synthetic".into(),
                default_timezone: "Asia/Shanghai".into(),
                valid_until: scope(&window).valid_until,
            },
        )
        .unwrap();
    let mut capture: RuntimeCaptureRequest =
        serde_json::from_value(value("runtime-owner-capture/cases.json")["base"].clone()).unwrap();
    capture.request_id = RequestId::parse(&id("req", 2)).unwrap();
    capture.conversation_id = "synthetic-later".into();
    capture.turns[0].message_id = "m2".into();
    capture.turns[0].text = "A later synthetic turn.".into();
    capture.turns[0].attachments.clear();
    let captured = v.capture_runtime(&capture_permit, &capture, false).unwrap();
    let later = e.source(23, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![later.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    publish(&e, &v, e.input(&later, &window, 23));
    // A live connection sees it after its next check; a fixed grant does not.
    following.renew(&v).unwrap();
    pinned.renew(&v).unwrap();
    assert_eq!(count(&following), before + 1);
    assert_eq!(count(&pinned), before);
    // New permits may name the later source only under the following grant.
    let mut wider = listed(&follow);
    wider.policy_ids.push(window.policy_id.clone());
    wider.source_ids.push(later.record.source_id.clone());
    assert!(permit(&follow, wider.clone()).is_ok());
    let mut wider_fixed = wider.clone();
    wider_fixed.source_ids = wider.source_ids.clone();
    assert!(permit(&fixed, wider_fixed).is_err());
    // Coverage has nothing to add for a following grant.
    let coverage = v
        .owner_read_grant_coverage(owner, &follow.grant_id)
        .unwrap();
    assert!(coverage.uncovered_policies.is_empty() && coverage.uncovered_sources.is_empty());
    assert!(coverage.refreshed.is_none());
    assert!(
        !v.owner_read_grant_coverage(owner, &fixed.grant_id)
            .unwrap()
            .uncovered_sources
            .is_empty()
    );
    // Agent requests on the later window are admitted only by the following grant.
    let agents =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let note = |g: &OwnerReadGrant, n: u32| {
        agents
            .note_writer_with_owner_grant(owner, g.principal.clone(), &g.grant_id, listed(g))
            .and_then(|w| {
                w.submit(&AgentNoteRequest {
                    request_id: RequestId::parse(&id("req", n)).unwrap(),
                    source: captured.sources[0].clone(),
                    quote_hint: "A later synthetic turn.".into(),
                    note: "Optional synthetic interpretation; not Owner confirmation.".into(),
                })
            })
    };
    assert_eq!(note(&follow, 30).unwrap().state, AgentNoteState::HintSaved);
    assert!(note(&fixed, 31).is_err());
    // Revoking the grant ends the live connection at its next check.
    let mut revoked = follow.clone();
    revoked.revision = Revision::new(2).unwrap();
    revoked.active = false;
    commit(revoked);
    assert!(following.renew(&v).is_err());
    assert!(v.read_agent(&following, None, |_| ()).is_err());
}

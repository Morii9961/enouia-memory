#![cfg(windows)]
#[path = "support/agent_forget_fixture.rs"]
mod fixture;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod schema_support;
use enouia_memory_contract::{common::*, hash::sha256, ids::*, json::Revision, next::*};
use enouia_memory_host::automation::{
    application::ApplicationFault, qualified_agent::QualifiedAgentHost,
};
use enouia_memory_vault::{
    OsIdSource,
    automation::{AutomationFault, segmented::*},
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;
use serde_json::json;
use std::sync::Arc;
fn fault(result: Result<AgentForgetReceipt, ApplicationFault>) -> AutomationFault {
    match result {
        Err(ApplicationFault::Vault(f)) => f,
        other => panic!("unexpected {:?}", other.map(|r| r.state)),
    }
}
fn initialize_forget_requests(
    server: &mut enouia_memory_host::mcp::forget_requests::Server<'_, '_>,
) {
    assert!(server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-client","version":"1"}}})).unwrap().get("result").is_some());
    assert!(
        server
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none()
    );
}

#[test]
fn concurrent_report_replay_returns_one_receipt_and_one_commit() {
    let (e, v, p, request) = forget_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let requester = host
        .forget_requester(owner, actor(ActorType::Agent, 88), scope(&p))
        .unwrap();
    let before = v.snapshot().unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let thread = scope.spawn(|| {
            barrier.wait();
            requester.submit(&request).unwrap()
        });
        barrier.wait();
        let b = requester.submit(&request).unwrap();
        (thread.join().unwrap(), b)
    });
    assert_eq!(a, b);
    let after = v.snapshot().unwrap();
    assert_ne!(after, before);
    assert_eq!(requester.submit(&request).unwrap(), a);
    assert_eq!(v.snapshot().unwrap(), after);
}

#[test]
fn deletion_revocation_and_scope_precede_original_receipt_replay() {
    for invalidation in 0..7 {
        let (e, v, p, request) = forget_rig(SpeakerRole::User);
        let owner = &e.descriptor.created_by;
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let agent = actor(ActorType::Agent, 88);
        let requester = host
            .forget_requester(owner, agent.clone(), scope(&p))
            .unwrap();
        requester.submit(&request).unwrap();
        revise(&e, &v, &p, &request);
        assert!(requester.submit(&request).is_ok());
        let mut narrow = scope(&p);
        narrow
            .source_ids
            .retain(|id| id != &request.source.source_id);
        let narrowed = host.forget_requester(owner, agent, narrow).unwrap();
        let mut caller = &requester;
        match invalidation {
            0 => requester.revoke(),
            1 => {
                host.lock().unwrap();
                host.unlock(owner).unwrap();
            }
            2 => e.clock.set(NOW + 120_001),
            3 => {
                v.delete(owner, vec![request.source.source_id.clone()], vec![])
                    .unwrap();
            }
            4 => {
                v.delete(owner, vec![], vec![request.target.memory.memory_id.clone()])
                    .unwrap();
            }
            // A fresh grant that excludes the evidence source cannot replay.
            5 => caller = &narrowed,
            _ => {
                let mut replacement = p.clone();
                replacement.revision = Revision::new(2).unwrap();
                v.set_policy(owner, replacement).unwrap();
            }
        }
        let after = v.snapshot().unwrap();
        assert!(
            caller.submit(&request).is_err(),
            "invalidation {invalidation}"
        );
        assert_eq!(v.snapshot().unwrap(), after);
    }
}

#[test]
fn report_capacity_is_per_target_and_saved_receipts_still_replay() {
    let (e, v, p, mut request) = forget_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let requester = host
        .forget_requester(owner, actor(ActorType::Agent, 88), scope(&p))
        .unwrap();
    let original = request.clone();
    let first = requester.submit(&original).unwrap();
    for n in 1..MAX_AGENT_FORGET_REQUESTS_PER_TARGET {
        request.request_id = RequestId::parse(&id("req", 100 + n as u32)).unwrap();
        requester.submit(&request).unwrap();
    }
    let full = v.snapshot().unwrap();
    request.request_id = RequestId::parse(&id("req", 999)).unwrap();
    assert!(matches!(
        fault(requester.submit(&request)),
        AutomationFault::Capacity
    ));
    assert_eq!(requester.submit(&original).unwrap(), first);
    assert_eq!(v.snapshot().unwrap(), full);
}

#[test]
fn uncertain_report_publication_latches_the_original_request_and_recovers_one_receipt() {
    for (point, persisted) in [
        (FaultPoint::StagingWritten, false),
        (FaultPoint::AfterCurrent, true),
    ] {
        let faults = Faults::armed();
        let (e, v, p, request) = forget_rig_with(SpeakerRole::User, faults.clone());
        let owner = &e.descriptor.created_by;
        let principal = actor(ActorType::Agent, 88);
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let requester = host
            .forget_requester(owner, principal.clone(), scope(&p))
            .unwrap();
        faults.arm(point, 1, FaultAction::Fail(121));
        assert!(matches!(
            fault(requester.submit(&request)),
            AutomationFault::OutcomeUnknown
        ));
        assert_eq!(
            digest(requester.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
        let mut changed = request.clone();
        changed.forget.push('!');
        assert!(matches!(
            fault(requester.submit(&changed)),
            AutomationFault::OutcomeUnknown
        ));
        assert_eq!(
            digest(requester.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
        drop(requester);
        drop(host);
        drop(v);
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
        if persisted {
            // A System revision between recovery and explicit replay must not
            // hide the report that was published before the uncertainty.
            revise(&e, &v, &p, &request);
        }
        let recovered = v.snapshot().unwrap();
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let requester = host.forget_requester(owner, principal, scope(&p)).unwrap();
        let receipt = requester.submit(&request).unwrap();
        let after = v.snapshot().unwrap();
        assert_eq!(after == recovered, persisted);
        assert_eq!(receipt.target, request.target);
        assert_eq!(requester.submit(&request).unwrap(), receipt);
        assert_eq!(v.snapshot().unwrap(), after);
        assert!(requester.original_unknown().unwrap().is_none());
    }
}

#[test]
fn explicit_forget_server_pairs_authority_and_returns_only_forget_request_saved_receipts() {
    use enouia_memory_host::mcp::{forget_requests, initialized};
    let (e, v, p, request) = forget_rig_with(SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let writer = host
        .forget_requester(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let other = host
        .forget_requester(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 89),
            scope(&p),
        )
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    assert!(forget_requests::Server::new(&reader, &other, &profile).is_none());
    let (foreign_e, foreign_v, foreign_p, _) = forget_rig_with(SpeakerRole::User, Faults::none());
    let foreign_host = QualifiedAgentHost::from_authenticated_initialized_owner(
        foreign_v,
        foreign_e.descriptor.created_by.clone(),
    )
    .unwrap();
    let foreign = foreign_host
        .forget_requester(
            &foreign_e.descriptor.created_by,
            principal,
            scope(&foreign_p),
        )
        .unwrap();
    assert_eq!(&foreign_e.descriptor.vault_id, reader.vault_id()); // Same synthetic logical ID, different lease.
    assert!(forget_requests::Server::new(&reader, &foreign, &profile).is_none());
    let mut server = forget_requests::Server::new(&reader, &writer, &profile).unwrap();
    initialize_forget_requests(&mut server);
    let listed = server
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    assert_eq!(tools[5]["name"], "memory_forget_request");
    assert_eq!(tools[5]["annotations"]["readOnlyHint"], false);
    assert!(!tools[5]["inputSchema"].to_string().contains("../"));
    assert!(!tools[5]["outputSchema"].to_string().contains("../"));
    let read_packet =
        schema_support::fixture("../agent-initialized-read/manifest.json")["valid"][0].clone();
    let read = server.handle(&json!({"jsonrpc":"2.0","id":"read-before-note","method":"tools/call","params":{"name":"memory_search","arguments":read_packet}})).unwrap();
    assert_eq!(
        read["result"]["structuredContent"]["capability"],
        "memory-agent-initialized-read-1"
    );
    assert!(read["result"]["structuredContent"]["error"].is_null());
    let mut call = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_forget_request","arguments":{"schema_version":2,"kind":"agent_forget_request","capability":forget_requests::CAPABILITY,"request":request}}});
    let before = v.snapshot().unwrap();
    for n in 0..5 {
        let mut bad = call.clone();
        bad["id"] = json!(20 + n);
        match n {
            0 => bad["params"]["arguments"]["owner"] = json!(true),
            1 => bad["params"]["arguments"]["schema_version"] = json!(1),
            2 => {
                bad["params"]["arguments"]["capability"] = json!("memory-agent-initialized-read-1")
            }
            3 => {
                bad["params"]["arguments"]["request"]["principal"] = json!(e.descriptor.created_by)
            }
            _ => bad["params"]["arguments"]["request"]["source"]["source_revision"] = json!(0),
        }
        assert!(server.handle(&bad).unwrap().get("error").is_some());
    }
    assert_eq!(v.snapshot().unwrap(), before);
    let mut refused = call.clone();
    refused["id"] = json!("known-refusal");
    refused["params"]["arguments"]["request"]["quote_hint"] = json!("No such quote.");
    let refusal = server.handle(&refused).unwrap();
    let refusal_data = &refusal["result"]["structuredContent"];
    assert_eq!(refusal_data["error"]["code"], "permission_denied");
    assert!(refusal_data["receipt"].is_null());
    assert!(!server.closed());
    assert!(
        schema_support::schema::SchemaStore::load()
            .validate("next/agent-forget-mcp-v1.schema.json", refusal_data)
            .is_empty()
    );
    let response = server.handle(&call).unwrap();
    let data = &response["result"]["structuredContent"];
    assert_eq!(data["receipt"]["state"], "forget_request_saved");
    assert!(data["error"].is_null());
    assert!(
        schema_support::schema::SchemaStore::load()
            .validate("next/agent-forget-mcp-v1.schema.json", data)
            .is_empty()
    );
    assert!(data.get("scope_hash").is_none() && data.get("principal").is_none());
    assert!(data.get("source").is_none() && data.get("memory_id").is_none());
    let saved = v.snapshot().unwrap();
    assert_ne!(saved, before);
    revise(&e, &v, &p, &request);
    let saved = v.snapshot().unwrap();
    call["id"] = json!(4);
    assert_eq!(
        server.handle(&call).unwrap()["result"]["structuredContent"],
        *data
    );
    assert_eq!(v.snapshot().unwrap(), saved);
    assert!(server.handle(&call).unwrap().get("error").is_some()); // Duplicate MCP ID never writes.
    let mut old = initialized::Server::new(&reader, &profile).unwrap();
    old.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"old-client","version":"1"}}}));
    old.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    assert_eq!(
        old.handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .unwrap()["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(old.handle(&call).unwrap().get("error").is_some());
    assert_eq!(v.snapshot().unwrap(), saved);
}

#[test]
fn forget_server_unknown_returns_no_tool_result_and_keeps_the_entire_original_mcp_value() {
    use enouia_memory_host::mcp::forget_requests;
    for point in [FaultPoint::StagingWritten, FaultPoint::AfterCurrent] {
        let faults = Faults::armed();
        let (e, v, p, request) = forget_rig_with(SpeakerRole::User, faults.clone());
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v,
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let writer = host
            .forget_requester(&e.descriptor.created_by, principal, scope(&p))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let mut server = forget_requests::Server::new(&reader, &writer, &profile).unwrap();
        initialize_forget_requests(&mut server);
        let message = json!({"jsonrpc":"2.0","id":"original-note","method":"tools/call","params":{"name":"memory_forget_request","arguments":{"schema_version":2,"kind":"agent_forget_request","capability":forget_requests::CAPABILITY,"request":request}}});
        faults.arm(point, 1, FaultAction::Fail(118));
        assert!(server.handle(&message).is_none());
        assert!(server.outcome_unknown() && server.closed());
        assert_eq!(server.original_unknown(), Some(&message));
        let mut changed = message.clone();
        changed["id"] = json!("replacement");
        assert!(server.handle(&changed).is_none());
        assert_eq!(server.original_unknown(), Some(&message));
        assert_eq!(
            digest(writer.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
    }
}

#[test]
fn saved_request_replays_after_system_revision_and_reopen_without_forgetting() {
    let (e, v, p, request) = forget_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let principal = actor(ActorType::Agent, 88);
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let requester = host
        .forget_requester(owner, principal.clone(), scope(&p))
        .unwrap();
    let before = v.snapshot().unwrap();
    let receipt = requester.submit(&request).unwrap();
    assert_eq!(receipt.state, AgentForgetState::ForgetRequestSaved);
    let saved = v.snapshot().unwrap();
    assert_ne!(saved, before);
    assert_eq!(requester.submit(&request).unwrap(), receipt);
    assert_eq!(v.snapshot().unwrap(), saved);
    let mut changed = request.clone();
    changed.forget.push('!');
    assert!(matches!(
        fault(requester.submit(&changed)),
        AutomationFault::Conflict
    ));
    let current = revise(&e, &v, &p, &request);
    let revised = v.snapshot().unwrap();
    assert_eq!(requester.submit(&request).unwrap(), receipt);
    assert_eq!(v.snapshot().unwrap(), revised);
    let mut fresh = request.clone();
    fresh.request_id = RequestId::parse(&id("req", 300)).unwrap();
    assert!(matches!(
        fault(requester.submit(&fresh)),
        AutomationFault::Stale
    ));
    fresh.target = current;
    let second = requester.submit(&fresh).unwrap();
    drop(requester);
    drop(host);
    drop(v);
    let v = Arc::new(e.open());
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let requester = host.forget_requester(owner, principal, scope(&p)).unwrap();
    assert_eq!(requester.submit(&request).unwrap(), receipt);
    assert_eq!(requester.submit(&fresh).unwrap(), second);
}

#[test]
fn forget_recording_requires_independent_live_authority_real_user_source_and_exact_target() {
    for variant in 0..10 {
        let (e, v, p, mut request) = forget_rig(if variant == 0 {
            SpeakerRole::Assistant
        } else {
            SpeakerRole::User
        });
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let owner = &e.descriptor.created_by;
        let mut unreadable = scope(&p);
        unreadable.source_read = false;
        assert!(
            host.forget_requester(owner, actor(ActorType::Agent, 88), unreadable)
                .is_err()
        );
        assert!(
            host.forget_requester(
                &e.descriptor.system_actor,
                actor(ActorType::Agent, 88),
                scope(&p)
            )
            .is_err()
        );
        let requester = host
            .forget_requester(owner, actor(ActorType::Agent, 88), scope(&p))
            .unwrap();
        match variant {
            0 => {}
            1 => request.target.hash = sha256(b"wrong target"),
            2 => request.quote_hint = "not in this source".into(),
            3 => request.forget = " ".into(),
            4 => request.forget = "x".repeat(2001),
            5 => {
                request.source.source_revision =
                    enouia_memory_contract::json::Revision::new(2).unwrap()
            }
            6 => requester.revoke(),
            7 => {
                host.lock().unwrap();
            }
            8 => e.clock.set(NOW + 120_001),
            _ => {
                let mut p = p.clone();
                p.active = false;
                p.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
                v.set_policy(owner, p).unwrap();
            }
        }
        let before = v.snapshot().unwrap();
        assert!(requester.submit(&request).is_err(), "variant {variant}");
        assert_eq!(v.snapshot().unwrap(), before);
    }
}

#[test]
fn saving_a_forget_request_neither_changes_recall_inputs_nor_dispatches_any_analysis() {
    let (e, v, p, request) = forget_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let agent = actor(ActorType::Agent, 88);
    let requester = host
        .forget_requester(owner, agent.clone(), scope(&p))
        .unwrap();
    let receipt = requester.submit(&request).unwrap();
    let correction = AgentCorrectionRequest {
        request_id: request.request_id.clone(),
        target: request.target.clone(),
        source: request.source.clone(),
        quote_hint: request.quote_hint.clone(),
        correction: request.forget.clone(),
    };
    let supersede = AgentSupersedeRequest {
        request_id: request.request_id.clone(),
        target: request.target.clone(),
        source: request.source.clone(),
        quote_hint: request.quote_hint.clone(),
        supersede: request.forget.clone(),
    };
    // Independent key spaces without saving either other kind of report.
    assert_ne!(
        receipt.request_key,
        host.correction_reporter(owner, agent.clone(), scope(&p))
            .unwrap()
            .prepare(&correction)
            .unwrap()
            .report_key
    );
    assert_ne!(
        receipt.request_key,
        host.supersede_reporter(owner, agent, scope(&p))
            .unwrap()
            .prepare(&supersede)
            .unwrap()
            .report_key
    );
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.memories.len(), 1);
        assert_eq!(d.memories[0].target, request.target);
        assert_eq!(d.sources.len(), 2);
        assert!(d.owner_actions.is_empty());
    })
    .unwrap();
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
    struct NoAnalysis;
    impl CaptureAnalyzer for NoAnalysis {
        fn start(
            &mut self,
            _: &QueuedCapture,
            _: &CapturedSource,
            _: &[AgentNoteRecord],
            _: &CaptureStopToken,
        ) -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
            panic!("forget recording dispatched capture")
        }
    }
    impl CorrectionAnalyzer for NoAnalysis {
        fn start(
            &mut self,
            _: &AgentCorrectionRecord,
            _: &CorrectionTarget,
            _: &CapturedSource,
            _: &CaptureStopToken,
        ) -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
            panic!("forget recording dispatched correction")
        }
    }
    impl SupersedeAnalyzer for NoAnalysis {
        fn start(
            &mut self,
            _: &AgentSupersedeRecord,
            _: &CorrectionTarget,
            _: &CapturedSource,
            _: &CaptureStopToken,
        ) -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
            panic!("forget recording dispatched supersede")
        }
    }
    let mut f = CapturedFormation::open(v.clone(), &e.descriptor.system_actor).unwrap();
    let stop = CaptureStopToken::default();
    assert!(!f.step_with_stop(&mut NoAnalysis, &stop).unwrap());
    assert!(!f.step_corrections(&mut NoAnalysis, &stop).unwrap());
    assert!(!f.step_supersedes(&mut NoAnalysis, &stop).unwrap());
    assert!(f.status().is_empty());
}

#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod schema_support;
use enouia_memory_contract::{common::*, ids::*, json::Revision, next::*, time::Timestamp};
use enouia_memory_host::automation::qualified_agent::QualifiedAgentHost;
use enouia_memory_vault::{
    OsIdSource,
    automation::segmented::*,
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;
use serde_json::json;
use std::sync::Arc;

fn initialize_notes(server: &mut enouia_memory_host::mcp::notes::Server<'_, '_>) {
    assert!(server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-client","version":"1"}}})).unwrap().get("result").is_some());
    assert!(
        server
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none()
    );
}

fn actor(role: ActorType, n: u32) -> ActorRef {
    ActorRef {
        actor_type: role,
        actor_id: PrincipalId::parse(&id("prn", n)).unwrap(),
    }
}
fn scope(p: &AutomationPolicy) -> SegmentedAgentScope {
    SegmentedAgentScope {
        subject_id: p.subject_id.clone(),
        project_id: p.project_id.clone(),
        policy_ids: vec![p.policy_id.clone()],
        source_ids: p.source_ids.clone(),
        max_sensitivity: Sensitivity::Private,
        source_read: true,
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn rig(
    text: &str,
    speaker: SpeakerRole,
    faults: Faults,
) -> (
    Env,
    Arc<SegmentedAutomationVault>,
    AutomationPolicy,
    AgentNoteRequest,
) {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    let v = Arc::new(e.create(faults));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let permit = v
        .authorize_owner_runtime_capture(
            &e.descriptor.created_by,
            actor(ActorType::Client, 77),
            RuntimeCaptureScope {
                policy_id: p.policy_id.clone(),
                source_ids: p.source_ids.clone(),
                max_sensitivity: Sensitivity::Private,
                platform: "enouia-runtime".into(),
                account_scope: "account-synthetic".into(),
                default_timezone: "Asia/Shanghai".into(),
                valid_until: scope(&p).valid_until,
            },
        )
        .unwrap();
    let mut capture: RuntimeCaptureRequest =
        serde_json::from_value(value("runtime-owner-capture/cases.json")["base"].clone()).unwrap();
    capture.turns[0].text = text.into();
    capture.turns[0].speaker = speaker;
    capture.turns[0].assistant_model =
        (speaker == SpeakerRole::Assistant).then(|| "synthetic-model".into());
    capture.turns[0].attachments.clear();
    let captured = v.capture_runtime(&permit, &capture, false).unwrap();
    let request = AgentNoteRequest {
        request_id: RequestId::parse(&id("req", 12)).unwrap(),
        source: captured.sources[0].clone(),
        quote_hint: text.into(),
        note: "Optional synthetic interpretation; not Owner confirmation.".into(),
    };
    (e, v, p, request)
}

#[test]
fn native_drafts_bind_exact_original_quote_actual_speaker_and_agent_provenance() {
    for speaker in [SpeakerRole::User, SpeakerRole::Assistant] {
        let (e, v, p, mut request) = rig("前缀「ＡＢＣ　喜欢安静」后缀", speaker, Faults::none());
        request.quote_hint = "\"ABC 喜欢安静\"".into();
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let writer = host
            .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let before = v.snapshot().unwrap();
        let draft = writer.prepare(&request).unwrap();
        assert_eq!(draft.capability, AGENT_NOTE_CAPABILITY);
        assert_eq!(draft.principal, principal);
        assert_eq!(draft.evidence.quote, "「ＡＢＣ　喜欢安静」");
        assert_eq!(draft.evidence.start, 2);
        assert_eq!(draft.evidence.speaker, speaker);
        assert_eq!(
            draft.evidence.reference.evidence_class,
            if speaker == SpeakerRole::User {
                EvidenceClass::UserStatement
            } else {
                EvidenceClass::ModelClaim
            }
        );
        assert_eq!(writer.prepare(&request).unwrap().note_key, draft.note_key);
        assert_eq!(
            writer.prepare(&request).unwrap().request_hash,
            draft.request_hash
        );
        request.note.push('!');
        let changed = writer.prepare(&request).unwrap();
        assert_eq!(changed.note_key, draft.note_key);
        assert_ne!(changed.request_hash, draft.request_hash);
        let other = host
            .note_writer(
                &e.descriptor.created_by,
                actor(ActorType::Agent, 89),
                scope(&p),
            )
            .unwrap();
        assert_ne!(other.prepare(&request).unwrap().note_key, draft.note_key);
        assert_eq!(v.snapshot().unwrap(), before);
        assert!(!e.path.join("sync-state/agent-notes-1.json").exists());
        assert!(
            v.queued_captures(&e.descriptor.system_actor)
                .unwrap()
                .is_empty()
        );
        v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            assert!(d.memories.is_empty())
        })
        .unwrap();
    }
}

#[test]
fn note_delegation_is_explicit_and_lock_revoke_policy_expiry_and_instance_bound() {
    for invalidation in 0..6 {
        let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
        let owner = &e.descriptor.created_by;
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let mut unreadable = scope(&p);
        unreadable.source_read = false;
        assert!(
            host.note_writer(owner, principal.clone(), unreadable)
                .is_err()
        );
        assert!(
            host.note_writer(&e.descriptor.system_actor, principal.clone(), scope(&p))
                .is_err()
        );
        assert!(host.note_writer(owner, owner.clone(), scope(&p)).is_err());
        let read_host =
            QualifiedAgentHost::from_authenticated_owner(v.clone(), owner.clone()).unwrap();
        assert!(
            read_host
                .note_writer(owner, principal.clone(), scope(&p))
                .is_err()
        );
        let writer = host
            .note_writer(owner, principal.clone(), scope(&p))
            .unwrap();
        assert!(writer.prepare(&request).is_ok());
        assert!(writer.check_connection().is_ok());
        match invalidation {
            0 => writer.revoke(),
            1 => {
                host.lock().unwrap();
                host.unlock(owner).unwrap();
            }
            2 => {
                let mut replacement = p.clone();
                replacement.revision = Revision::new(2).unwrap();
                v.set_policy(owner, replacement).unwrap();
            }
            3 => e.clock.set(NOW + 120_001),
            4 => {
                v.delete(owner, vec![request.source.source_id.clone()], vec![])
                    .unwrap();
            }
            _ => {
                let permit = v.authorize_agent_note(owner, principal, scope(&p)).unwrap();
                let (_, other, _, _) = rig("Other source.", SpeakerRole::User, Faults::none());
                assert!(
                    other
                        .prepare_agent_note(&permit, &request, |_, _| panic!(
                            "foreign permit reached source"
                        ))
                        .is_err()
                );
                writer.revoke();
            }
        }
        assert!(writer.prepare(&request).is_err());
        // A deleted source refuses per call; other delegated sources stay live.
        assert_eq!(writer.check_connection().is_err(), invalidation != 4);
    }
}

#[test]
fn note_bounds_ambiguity_and_exact_revision_refuse_before_publication() {
    let (e, v, p, request) = rig("alpha alpha", SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let writer = host
        .note_writer(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    let before = v.snapshot().unwrap();
    for variant in 0..8 {
        let mut r = request.clone();
        match variant {
            0 => r.quote_hint = "alpha".into(),
            1 => r.quote_hint = "missing".into(),
            2 => r.quote_hint = "x".repeat(501),
            3 => r.note = "x".repeat(2001),
            4 => r.note = " ".into(),
            5 => r.source.source_revision = Revision::new(2).unwrap(),
            6 => r.source.source_id = SourceId::parse(&id("src", 99)).unwrap(),
            _ => r.quote_hint.clear(),
        }
        assert!(writer.prepare(&r).is_err());
    }
    let mut narrowed = scope(&p);
    narrowed
        .source_ids
        .retain(|id| id != &request.source.source_id);
    let narrow = host
        .note_writer(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            narrowed,
        )
        .unwrap();
    assert!(narrow.prepare(&request).is_err());
    let permit = v
        .authorize_agent_note(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    assert!(
        v.prepare_agent_note(&permit, &request, |source, _| {
            let mut evidence = quoted_evidence(source, &source.text).unwrap();
            evidence.reference.evidence_class = EvidenceClass::UserConfirmation;
            Some(evidence)
        })
        .is_err()
    );
    assert_eq!(v.snapshot().unwrap(), before);
}

#[test]
fn saved_hints_replay_original_receipts_without_creating_sources_jobs_or_memories() {
    let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
    let principal = actor(ActorType::Agent, 88);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let writer = host
        .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let draft = writer.prepare(&request).unwrap();
    let before = v.snapshot().unwrap();
    let receipt = writer.submit(&request).unwrap();
    assert_eq!(receipt.note_key, draft.note_key);
    assert_eq!(receipt.request_hash, draft.request_hash);
    assert_eq!(receipt.state, AgentNoteState::HintSaved);
    let after = v.snapshot().unwrap();
    assert_ne!(after, before);
    assert_eq!(writer.submit(&request).unwrap(), receipt);
    assert_eq!(v.snapshot().unwrap(), after);
    let mut changed = request.clone();
    changed.note.push('!');
    assert!(matches!(
        writer.submit(&changed),
        Err(
            enouia_memory_host::automation::application::ApplicationFault::Vault(
                enouia_memory_vault::automation::AutomationFault::Conflict
            )
        )
    ));
    assert_eq!(v.snapshot().unwrap(), after);
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.sources.len(), 1);
        assert!(d.memories.is_empty());
    })
    .unwrap();
    drop(writer);
    drop(host);
    drop(v);
    let v = Arc::new(e.open());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let writer = host
        .note_writer(&e.descriptor.created_by, principal, scope(&p))
        .unwrap();
    assert_eq!(writer.submit(&request).unwrap(), receipt);
    assert_eq!(v.snapshot().unwrap(), after);
    v.delete(
        &e.descriptor.created_by,
        vec![request.source.source_id.clone()],
        vec![],
    )
    .unwrap();
    let deleted = v.snapshot().unwrap();
    assert!(writer.submit(&request).is_err());
    assert_eq!(v.snapshot().unwrap(), deleted);
}

#[test]
fn saved_hint_capacity_is_bounded_but_original_receipts_still_replay() {
    let (e, v, p, mut request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let writer = host
        .note_writer(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    let original = request.clone();
    let first = writer.submit(&original).unwrap();
    for n in 1..MAX_AGENT_NOTES_PER_SOURCE {
        request.request_id = RequestId::parse(&id("req", 100 + n as u32)).unwrap();
        writer.submit(&request).unwrap();
    }
    let full = v.snapshot().unwrap();
    request.request_id = RequestId::parse(&id("req", 999)).unwrap();
    assert!(matches!(
        writer.submit(&request),
        Err(
            enouia_memory_host::automation::application::ApplicationFault::Vault(
                enouia_memory_vault::automation::AutomationFault::Capacity
            )
        )
    ));
    assert_eq!(writer.submit(&original).unwrap(), first);
    assert_eq!(v.snapshot().unwrap(), full);
}

#[test]
fn uncertain_hint_publication_retains_the_original_packet_and_recovers_one_receipt() {
    use enouia_memory_host::automation::application::ApplicationFault;
    use enouia_memory_vault::automation::AutomationFault;
    for (point, persisted) in [
        (FaultPoint::StagingWritten, false),
        (FaultPoint::AfterCurrent, true),
    ] {
        let faults = Faults::armed();
        let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, faults.clone());
        let principal = actor(ActorType::Agent, 88);
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let writer = host
            .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        faults.arm(point, 1, FaultAction::Fail(116));
        assert!(matches!(
            writer.submit(&request),
            Err(ApplicationFault::Vault(AutomationFault::OutcomeUnknown))
        ));
        assert_eq!(
            digest(writer.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
        let mut changed = request.clone();
        changed.note.push('!');
        assert!(matches!(
            writer.submit(&changed),
            Err(ApplicationFault::Vault(AutomationFault::OutcomeUnknown))
        ));
        assert_eq!(
            digest(writer.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
        drop(writer);
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
        let recovered = v.snapshot().unwrap();
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let writer = host
            .note_writer(&e.descriptor.created_by, principal, scope(&p))
            .unwrap();
        let receipt = writer.submit(&request).unwrap();
        let after = v.snapshot().unwrap();
        assert_eq!(after == recovered, persisted);
        assert_eq!(writer.submit(&request).unwrap(), receipt);
        assert_eq!(v.snapshot().unwrap(), after);
        assert_eq!(receipt.state, AgentNoteState::HintSaved);
        assert!(writer.original_unknown().unwrap().is_none());
    }
}

#[test]
fn concurrent_native_hint_replay_returns_one_receipt_and_one_commit() {
    let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let writer = host
        .note_writer(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    let before = v.snapshot().unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let thread = scope.spawn(|| {
            barrier.wait();
            writer.submit(&request).unwrap()
        });
        barrier.wait();
        let b = writer.submit(&request).unwrap();
        (thread.join().unwrap(), b)
    });
    assert_eq!(a, b);
    let after = v.snapshot().unwrap();
    assert_ne!(after, before);
    assert_eq!(writer.submit(&request).unwrap(), a);
    assert_eq!(v.snapshot().unwrap(), after);
}

#[test]
fn explicit_note_server_advertises_six_tools_and_returns_only_saved_hint_receipts() {
    use enouia_memory_host::mcp::{initialized, notes};
    let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, Faults::none());
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
        .note_writer(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let other = host
        .note_writer(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 89),
            scope(&p),
        )
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    assert!(notes::Server::new(&reader, &other, &profile).is_none());
    let (foreign_e, foreign_v, foreign_p, _) =
        rig("A foreign source.", SpeakerRole::User, Faults::none());
    let foreign_host = QualifiedAgentHost::from_authenticated_initialized_owner(
        foreign_v,
        foreign_e.descriptor.created_by.clone(),
    )
    .unwrap();
    let foreign = foreign_host
        .note_writer(
            &foreign_e.descriptor.created_by,
            principal,
            scope(&foreign_p),
        )
        .unwrap();
    assert_eq!(foreign.vault_id(), reader.vault_id()); // Same synthetic logical ID, different lease.
    assert!(notes::Server::new(&reader, &foreign, &profile).is_none());
    let mut server = notes::Server::new(&reader, &writer, &profile).unwrap();
    initialize_notes(&mut server);
    let listed = server
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    assert_eq!(tools[5]["name"], "memory_note");
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
    let mut call = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_note","arguments":{"schema_version":2,"kind":"agent_note_request","capability":notes::CAPABILITY,"request":request}}});
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
            .validate("next/agent-note-mcp-v1.schema.json", refusal_data)
            .is_empty()
    );
    let response = server.handle(&call).unwrap();
    let data = &response["result"]["structuredContent"];
    assert_eq!(data["receipt"]["state"], "hint_saved");
    assert!(data["error"].is_null());
    assert!(
        schema_support::schema::SchemaStore::load()
            .validate("next/agent-note-mcp-v1.schema.json", data)
            .is_empty()
    );
    assert!(data.get("scope_hash").is_none() && data.get("principal").is_none());
    assert!(data.get("source").is_none() && data.get("memory_id").is_none());
    let saved = v.snapshot().unwrap();
    assert_ne!(saved, before);
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
fn note_server_unknown_returns_no_tool_result_and_keeps_the_entire_original_mcp_value() {
    use enouia_memory_host::mcp::notes;
    for point in [FaultPoint::StagingWritten, FaultPoint::AfterCurrent] {
        let faults = Faults::armed();
        let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User, faults.clone());
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
            .note_writer(&e.descriptor.created_by, principal, scope(&p))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let mut server = notes::Server::new(&reader, &writer, &profile).unwrap();
        initialize_notes(&mut server);
        let message = json!({"jsonrpc":"2.0","id":"original-note","method":"tools/call","params":{"name":"memory_note","arguments":{"schema_version":2,"kind":"agent_note_request","capability":notes::CAPABILITY,"request":request}}});
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

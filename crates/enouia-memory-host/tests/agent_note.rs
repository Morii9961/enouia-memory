#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, json::Revision, next::*, time::Timestamp};
use enouia_memory_host::automation::qualified_agent::QualifiedAgentHost;
use enouia_memory_vault::{automation::segmented::*, fault::Faults};
use fixture::*;
use std::sync::Arc;

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
) -> (
    Env,
    Arc<SegmentedAutomationVault>,
    AutomationPolicy,
    AgentNoteRequest,
) {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    let v = Arc::new(e.create(Faults::none()));
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
        let (e, v, p, mut request) = rig("前缀「ＡＢＣ　喜欢安静」后缀", speaker);
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
        let (e, v, p, request) = rig("A synthetic preference.", SpeakerRole::User);
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
                let (_, other, _, _) = rig("Other source.", SpeakerRole::User);
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
    }
}

#[test]
fn note_bounds_ambiguity_and_exact_revision_refuse_before_publication() {
    let (e, v, p, request) = rig("alpha alpha", SpeakerRole::User);
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

#![cfg(windows)]
#[path = "support/agent_note_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, hash::sha256, ids::*, json::Revision, next::*};
use enouia_memory_host::automation::qualified_agent::QualifiedAgentHost;
use enouia_memory_vault::{automation::segmented::*, fault::Faults};
use fixture::*;
use std::sync::Arc;

fn correction_rig(
    speaker: SpeakerRole,
) -> (
    Env,
    Arc<SegmentedAutomationVault>,
    AutomationPolicy,
    AgentCorrectionRequest,
) {
    let (e, v, p, note) = rig("前缀「ＡＢＣ　纠正原话」后缀", speaker, Faults::none());
    let source = e.source(8, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![source.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let input = v
        .prepare(&e.descriptor.system_actor, e.input(&source, &p, 8))
        .unwrap();
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
    .unwrap();
    let target = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.memories[0].target.clone()
        })
        .unwrap();
    let request = AgentCorrectionRequest {
        request_id: note.request_id,
        target,
        source: note.source,
        quote_hint: "\"ABC 纠正原话\"".into(),
        correction: "Untrusted synthetic correction interpretation.".into(),
    };
    (e, v, p, request)
}

#[test]
fn correction_draft_binds_current_target_actual_user_quote_and_separate_agent_identity_without_writes()
 {
    let (e, v, p, mut request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reporter = host
        .correction_reporter(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let before = v.snapshot().unwrap();
    let draft = reporter.prepare(&request).unwrap();
    assert_eq!(draft.capability, AGENT_CORRECTION_CAPABILITY);
    assert_eq!(draft.principal, principal);
    assert_eq!(draft.target, request.target);
    assert_eq!(draft.snapshot, before);
    assert_eq!(draft.evidence.quote, "「ＡＢＣ　纠正原话」");
    assert_eq!(draft.evidence.start, 2);
    assert_eq!(draft.evidence.speaker, SpeakerRole::User);
    assert_eq!(
        draft.evidence.reference.evidence_class,
        EvidenceClass::UserStatement
    );
    assert_eq!(
        reporter.prepare(&request).unwrap().report_key,
        draft.report_key
    );
    request.correction.push('!');
    let changed = reporter.prepare(&request).unwrap();
    assert_eq!(changed.report_key, draft.report_key);
    assert_ne!(changed.request_hash, draft.request_hash);
    let other = host
        .correction_reporter(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 89),
            scope(&p),
        )
        .unwrap();
    assert_ne!(
        other.prepare(&request).unwrap().report_key,
        draft.report_key
    );
    assert_eq!(v.snapshot().unwrap(), before);
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.memories.len(), 1);
        assert_eq!(d.memories[0].target, request.target);
        assert!(d.owner_actions.is_empty());
    })
    .unwrap();
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn correction_requires_both_target_closure_and_real_current_user_source() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let reporter = host
        .correction_reporter(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    for variant in 0..10 {
        let mut r = request.clone();
        match variant {
            0 => r.target.hash = sha256(b"wrong target"),
            1 => r.target.memory.revision = Revision::new(2).unwrap(),
            2 => r.target.memory.memory_id = MemoryId::parse(&id("mem", 99)).unwrap(),
            3 => r.source.source_revision = Revision::new(2).unwrap(),
            4 => r.quote_hint = "missing".into(),
            5 => r.quote_hint = "x".repeat(501),
            6 => r.correction = "x".repeat(2001),
            7 => r.correction = " ".into(),
            8 => r.quote_hint.clear(),
            _ => {
                // A permitted System fixture source is not Runtime user-correction evidence.
                r.source = e.source(8, Sensitivity::Public).reference();
                r.quote_hint = e.source(8, Sensitivity::Public).text;
            }
        }
        assert!(reporter.prepare(&r).is_err(), "variant {variant}");
    }
    for excluded in [
        request.source.source_id.clone(),
        SourceId::parse(&id("src", 8)).unwrap(),
    ] {
        let mut narrow = scope(&p);
        narrow.source_ids.retain(|id| id != &excluded);
        let scoped = host
            .correction_reporter(
                &e.descriptor.created_by,
                actor(ActorType::Agent, 88),
                narrow,
            )
            .unwrap();
        assert!(scoped.prepare(&request).is_err());
    }
    let permit = v
        .authorize_agent_correction(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    assert!(
        v.prepare_agent_correction(&permit, &request, |source, _| {
            let mut q = quoted_evidence(source, &source.text).unwrap();
            q.reference.evidence_class = EvidenceClass::UserConfirmation;
            Some(q)
        })
        .is_err()
    );
    let (e2, v2, p2, r2) = correction_rig(SpeakerRole::Assistant);
    let host2 = QualifiedAgentHost::from_authenticated_initialized_owner(
        v2.clone(),
        e2.descriptor.created_by.clone(),
    )
    .unwrap();
    assert!(
        host2
            .correction_reporter(
                &e2.descriptor.created_by,
                actor(ActorType::Agent, 88),
                scope(&p2)
            )
            .unwrap()
            .prepare(&r2)
            .is_err()
    );
    assert!(
        v2.prepare_agent_correction(&permit, &r2, |_, _| panic!("foreign grant reached source"))
            .is_err()
    );
}

#[test]
fn correction_delegation_requires_owner_setup_and_live_generation_policy_source_and_target() {
    for invalidation in 0..6 {
        let (e, v, p, request) = correction_rig(SpeakerRole::User);
        let owner = &e.descriptor.created_by;
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let agent = actor(ActorType::Agent, 88);
        let mut unreadable = scope(&p);
        unreadable.source_read = false;
        assert!(
            host.correction_reporter(owner, agent.clone(), unreadable)
                .is_err()
        );
        assert!(
            host.correction_reporter(&e.descriptor.system_actor, agent.clone(), scope(&p))
                .is_err()
        );
        assert!(
            host.correction_reporter(owner, owner.clone(), scope(&p))
                .is_err()
        );
        let old_host =
            QualifiedAgentHost::from_authenticated_owner(v.clone(), owner.clone()).unwrap();
        assert!(
            old_host
                .correction_reporter(owner, agent.clone(), scope(&p))
                .is_err()
        );
        let reporter = host.correction_reporter(owner, agent, scope(&p)).unwrap();
        assert!(reporter.prepare(&request).is_ok());
        match invalidation {
            0 => reporter.revoke(),
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
                v.delete(owner, vec![], vec![request.target.memory.memory_id.clone()])
                    .unwrap();
            }
        }
        assert!(
            reporter.prepare(&request).is_err(),
            "invalidation {invalidation}"
        );
    }
}

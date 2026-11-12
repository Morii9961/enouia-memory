#![cfg(any(windows, target_os = "linux"))]
//! Characterize the existing Owner-root read gates before changing authority.
//! Every identity, choice, source and clock in this file is synthetic.
#[path = "support/agent_correction_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*,
    ids::{PolicyId, ProjectId, SubjectId},
    json::Revision,
    next::RecordCatalog,
    time::Timestamp,
};
use enouia_memory_host::automation::qualified_agent::QualifiedAgentHost;
use fixture::*;

#[test]
fn extending_the_same_policy_does_not_renew_existing_memory_authority() {
    let (e, v, p, target) = correction_rig(SpeakerRole::User);
    let old = v
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    let original = v
        .read_agent(&old, None, |d| d.current.memories[0].memory.clone())
        .unwrap();
    assert_eq!(original.decision.input.policy, p.reference());
    assert_eq!(original.memory, target.target.memory);
    let mut renewed = p.clone();
    renewed.revision = Revision::new(2).unwrap();
    renewed.valid_until = Timestamp::from_unix_ms(p.valid_until.unix_ms() + 86_400_000).unwrap();
    v.set_policy(&e.descriptor.created_by, renewed.clone())
        .unwrap();
    assert_ne!(renewed.reference(), original.decision.input.policy);
    assert!(v.read_agent(&old, None, |_| ()).is_err());
    let fresh = v
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&renewed),
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&fresh, Some(&target.target.memory.memory_id), |d| (
            d.current.memories.len(),
            d.history.len(),
            d.current.sources.len()
        ))
        .unwrap(),
        (0, 0, 2)
    );
    // The same exact-reference gate also applies to the native System view.
    assert_eq!(
        v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| d
            .memories
            .len())
            .unwrap(),
        0
    );
    let head = v.snapshot().unwrap();
    drop(v);
    let reopened = e.open();
    assert!(reopened.read_agent(&old, None, |_| ()).is_err());
    let fresh = reopened
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&renewed),
        )
        .unwrap();
    assert_eq!(
        reopened
            .read_agent(&fresh, None, |d| (
                d.current.memories.len(),
                d.current.sources.len()
            ))
            .unwrap(),
        (0, 2)
    );
    assert_eq!(reopened.snapshot().unwrap(), head);
}

#[test]
fn a_new_read_only_policy_id_covers_sources_but_does_not_rebind_memories() {
    let (e, v, p, _) = correction_rig(SpeakerRole::User);
    let original = v
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    let mut replacement = p.clone();
    replacement.policy_id = PolicyId::parse(&id("pol", 9000)).unwrap();
    replacement.allow_create = false;
    replacement.allow_revise_created = false;
    replacement.target_ids.clear();
    replacement.valid_until =
        Timestamp::from_unix_ms(p.valid_until.unix_ms() + 86_400_000).unwrap();
    v.set_policy(&e.descriptor.created_by, replacement.clone())
        .unwrap();
    let mut delegated = scope(&replacement);
    let replacement_permit = v
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            delegated.clone(),
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&replacement_permit, None, |d| (
            d.current.memories.len(),
            d.current.sources.len()
        ))
        .unwrap(),
        (0, 2)
    );
    // An unrelated policy does not revoke the original exact policy permit.
    assert_eq!(
        v.read_agent(&original, None, |d| d.current.memories.len())
            .unwrap(),
        1
    );
    e.clock.set(p.valid_until.unix_ms());
    delegated.valid_until = Timestamp::from_unix_ms(p.valid_until.unix_ms() + 120_000).unwrap();
    let mut expired = delegated.clone();
    expired.policy_ids = vec![p.policy_id.clone()];
    assert!(
        v.authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            expired
        )
        .is_err()
    );
    let replacement_permit = v
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            delegated,
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&replacement_permit, None, |d| (
            d.current.memories.len(),
            d.current.sources.len()
        ))
        .unwrap(),
        (0, 2)
    );
    let head = v.snapshot().unwrap();
    drop(v);
    let reopened = e.open();
    assert_eq!(
        reopened
            .read(&e.descriptor.system_actor, Sensitivity::Private, |d| (
                d.memories.len(),
                d.sources.len()
            ))
            .unwrap(),
        (0, 2)
    );
    assert_eq!(reopened.snapshot().unwrap(), head);
}

#[test]
fn disabling_and_reenabling_a_policy_does_not_restore_its_old_memory_binding() {
    let (e, v, mut p, _) = correction_rig(SpeakerRole::User);
    let old = v
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    p.revision = Revision::new(2).unwrap();
    p.active = false;
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    assert!(v.read_agent(&old, None, |_| ()).is_err());
    assert!(
        v.authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p)
        )
        .is_err()
    );
    p.revision = Revision::new(3).unwrap();
    p.active = true;
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    assert!(v.read_agent(&old, None, |_| ()).is_err());
    let fresh = v
        .authorize_initialized_agent_read(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    assert_eq!(
        v.read_agent(&fresh, None, |d| (
            d.current.memories.len(),
            d.current.sources.len()
        ))
        .unwrap(),
        (0, 2)
    );
}

#[test]
fn owner_scope_source_text_and_deletion_gates_remain_independent() {
    let (e, v, p, target) = correction_rig(SpeakerRole::User);
    let private = e.source(2, Sensitivity::Private);
    v.capture(
        &e.descriptor.system_actor,
        vec![private.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    publish(&e, &v, e.input(&private, &p, 2));
    let principal = actor(ActorType::Agent, 88);
    for change in 0..4 {
        let mut wrong = scope(&p);
        match change {
            0 => wrong.subject_id = SubjectId::parse(&id("sub", 99)).unwrap(),
            1 => wrong.project_id = Some(ProjectId::parse(&id("prj", 99)).unwrap()),
            2 => wrong.max_sensitivity = Sensitivity::HighlySensitive,
            _ => wrong.valid_until = Timestamp::from_unix_ms(NOW + 300_001).unwrap(),
        }
        assert!(
            v.authorize_initialized_agent_read(&e.descriptor.created_by, principal.clone(), wrong)
                .is_err()
        );
    }
    let mut public = scope(&p);
    public.max_sensitivity = Sensitivity::Public;
    let public = v
        .authorize_initialized_agent_read(&e.descriptor.created_by, principal.clone(), public)
        .unwrap();
    assert_eq!(
        v.read_agent(&public, None, |d| (
            d.current.memories.len(),
            d.current.sources.len()
        ))
        .unwrap(),
        (1, 1)
    );
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let mut narrow = scope(&p);
    narrow.source_ids = vec![private.record.source_id.clone()];
    let text = host
        .reader(&e.descriptor.created_by, principal.clone(), narrow.clone())
        .unwrap();
    assert!(
        text.source(&private.record.source_id, None, 0, None, 8192)
            .unwrap()
            .is_some()
    );
    assert!(
        text.source(&target.source.source_id, None, 0, None, 8192)
            .unwrap()
            .is_none()
    );
    narrow.source_read = false;
    let without_text = host
        .reader(&e.descriptor.created_by, principal.clone(), narrow)
        .unwrap();
    assert!(
        without_text
            .source(&private.record.source_id, None, 0, None, 8192)
            .is_err()
    );
    // Deleting evidence suppresses the target even for an already issued permit.
    v.delete(
        &e.descriptor.created_by,
        vec![e.source(8, Sensitivity::Public).record.source_id],
        vec![],
    )
    .unwrap();
    assert_eq!(
        v.read_agent(&public, None, |d| d.current.memories.len())
            .unwrap(),
        0
    );
    drop(without_text);
    drop(text);
    drop(host);
    let head = v.snapshot().unwrap();
    drop(v);
    let reopened = e.open();
    assert!(reopened.read_agent(&public, None, |_| ()).is_err());
    assert!(
        reopened
            .authorize_initialized_agent_read(
                &e.descriptor.created_by,
                principal.clone(),
                scope(&p)
            )
            .is_err()
    );
    let mut selected = scope(&p);
    selected.source_ids = vec![private.record.source_id];
    let current = reopened
        .authorize_initialized_agent_read(&e.descriptor.created_by, principal, selected)
        .unwrap();
    assert_eq!(
        reopened
            .read_agent(&current, None, |d| d.current.memories.len())
            .unwrap(),
        1
    );
    assert_eq!(reopened.snapshot().unwrap(), head);
}

#![cfg(windows)]
#[path = "support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, foundation::Clock, ids::*, json::Revision};
use enouia_memory_contract::{hash::sha256, next::*};
use enouia_memory_vault::automation::segmented::SegmentedAutomationVault;
use enouia_memory_vault::{automation::segmented::SegmentedAgentScope, fault::Faults};
use fixture::*;

fn apply(e: &Env, v: &SegmentedAutomationVault, input: AdmissionInput) -> MemoryReceipt {
    let request = v.prepare(&e.descriptor.system_actor, input).unwrap();
    let response =
        serde_json::to_string(&v.evaluate(&e.descriptor.system_actor, &request).unwrap()).unwrap();
    let binding = GenerationBinding {
        operation_id: request.operation_id.clone(),
        input_hash: digest(&request),
        response_hash: sha256(response.as_bytes()),
    };
    v.apply_batch(
        &e.descriptor.system_actor,
        vec![request],
        vec![response],
        vec![binding],
    )
    .unwrap()
    .remove(0)
}

#[test]
fn lineage_cannot_launder_an_undelegated_source_and_reads_do_not_publish() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let p = e.policy(1, 1, 2);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let a = e.source(1, Sensitivity::Public);
    let b = e.source(2, Sensitivity::Public);
    v.capture(
        &e.descriptor.system_actor,
        vec![a.clone(), b.clone()],
        RecordCatalog::default(),
    )
    .unwrap();
    let receipt = apply(&e, &v, e.input(&a, &p, 1));
    let target = v
        .read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
            d.memories[0].target.clone()
        })
        .unwrap();
    let mut derived = e.input(&b, &p, 2);
    derived.interpretation = Interpretation::Inference;
    derived.input_refs.push(a.reference());
    derived.context_source_refs.push(a.reference());
    derived.dimensions.as_mut().unwrap().derived_from = vec![target];
    apply(&e, &v, derived);
    let mut delegated = scope(&e, 1);
    delegated.source_ids = vec![b.record.source_id.clone()];
    let permit = v
        .authorize_agent_read(&e.descriptor.created_by, agent(), delegated)
        .unwrap();
    assert_eq!(
        v.read_agent(&permit, Some(&receipt.result.memory_id), |d| (
            d.current.memories.len(),
            d.history.len()
        ))
        .unwrap(),
        (0, 0)
    );
    let all = v
        .authorize_agent_read(&e.descriptor.created_by, agent(), scope(&e, 2))
        .unwrap();
    let before = v.snapshot().unwrap();
    for _ in 0..100 {
        assert_eq!(
            v.read_agent(&all, None, |d| d.current.memories.len())
                .unwrap(),
            2
        );
    }
    assert_eq!(before, v.snapshot().unwrap());
    v.delete(
        &e.descriptor.created_by,
        vec![],
        vec![receipt.result.memory_id],
    )
    .unwrap();
    assert_eq!(
        v.read_agent(&all, None, |d| d.current.memories.len())
            .unwrap(),
        0
    );
}

#[test]
fn admission_ceilings_are_independent_of_the_native_system_reader() {
    let e = Env::new();
    let v = e.create(Faults::none());
    e.populate(&v, 8);
    for change in 0..6 {
        let mut s = scope(&e, 8);
        match change {
            0 => s.policy_ids.push(s.policy_ids[0].clone()),
            1 => s.source_ids.push(s.source_ids[0].clone()),
            2 => s.max_sensitivity = Sensitivity::HighlySensitive,
            3 => {
                s.valid_until =
                    enouia_memory_contract::time::Timestamp::from_unix_ms(NOW + 300_001).unwrap()
            }
            4 => s.subject_id = SubjectId::parse(&id("sub", 99)).unwrap(),
            _ => s.policy_ids.clear(),
        }
        assert!(
            v.authorize_agent_read(&e.descriptor.created_by, agent(), s)
                .is_err()
        );
    }
    let mut public = scope(&e, 8);
    public.max_sensitivity = Sensitivity::Public;
    let p = v
        .authorize_agent_read(&e.descriptor.created_by, agent(), public)
        .unwrap();
    assert_eq!(
        v.read_agent(&p, None, |d| d.current.memories.len())
            .unwrap(),
        7
    );
    p.revoke();
    assert!(v.read_agent(&p, None, |_| ()).is_err());
    assert_eq!(
        v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| d
            .memories
            .len())
            .unwrap(),
        8
    );
}
fn agent() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn scope(e: &Env, count: u32) -> SegmentedAgentScope {
    let p = e.policy(1, 1, count.min(64));
    SegmentedAgentScope {
        subject_id: p.subject_id,
        project_id: p.project_id,
        policy_ids: (1..=count)
            .step_by(64)
            .map(|n| PolicyId::parse(&id("pol", 1000 + n)).unwrap())
            .collect(),
        source_ids: (1..=count)
            .map(|n| SourceId::parse(&id("src", n)).unwrap())
            .collect(),
        max_sensitivity: Sensitivity::Private,
        source_read: true,
        valid_until: enouia_memory_contract::time::Timestamp::from_unix_ms(
            e.clock.now_unix_ms() + 120_000,
        )
        .unwrap(),
    }
}
#[test]
fn agent_gate_filters_scopes_before_counts_and_rechecks_policy_deletion_and_instance() {
    let e = Env::new();
    let v = e.create(Faults::none());
    let receipts = e.populate(&v, 180);
    let mut narrow = scope(&e, 1);
    narrow.source_read = false;
    let permit = v
        .authorize_agent_read(&e.descriptor.created_by, agent(), narrow)
        .unwrap();
    let one = v
        .read_agent(&permit, None, |d| {
            assert_eq!(d.current.memories.len(), 1);
            assert_eq!(d.current.sources.len(), 1);
            d.current.memories[0].memory.memory.clone()
        })
        .unwrap();
    assert_eq!(one, receipts[0].result);
    let all = v
        .authorize_agent_read(&e.descriptor.created_by, agent(), scope(&e, 180))
        .unwrap();
    assert_eq!(
        v.read_agent(&all, None, |d| d.current.memories.len())
            .unwrap(),
        180
    );
    let other = Env::new();
    let other_v = other.create(Faults::none());
    assert!(other_v.read_agent(&permit, None, |_| ()).is_err());
    v.delete(
        &e.descriptor.created_by,
        vec![SourceId::parse(&id("src", 1)).unwrap()],
        vec![],
    )
    .unwrap();
    assert_eq!(
        v.read_agent(&permit, None, |d| d.current.memories.len())
            .unwrap(),
        0
    );
    assert_eq!(
        v.read_agent(&all, None, |d| d.current.memories.len())
            .unwrap(),
        179
    );
    let mut policy = e.policy(1, 1, 64);
    policy.revision = Revision::new(2).unwrap();
    policy.active = false;
    v.set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    assert!(v.read_agent(&all, None, |_| ()).is_err());
    policy.revision = Revision::new(3).unwrap();
    policy.active = true;
    v.set_policy(&e.descriptor.created_by, policy).unwrap();
    assert!(v.read_agent(&all, None, |_| ()).is_err());
}
#[test]
fn native_registration_rejects_impersonation_outside_sources_and_expired_or_rolled_back_grants() {
    let e = Env::new();
    let v = e.create(Faults::none());
    e.populate(&v, 8);
    assert!(
        v.authorize_agent_read(&agent(), agent(), scope(&e, 8))
            .is_err()
    );
    assert!(
        v.authorize_agent_read(
            &e.descriptor.created_by,
            e.descriptor.system_actor.clone(),
            scope(&e, 8)
        )
        .is_err()
    );
    let mut wrong = scope(&e, 8);
    wrong
        .source_ids
        .push(SourceId::parse(&id("src", 100)).unwrap());
    assert!(
        v.authorize_agent_read(&e.descriptor.created_by, agent(), wrong)
            .is_err()
    );
    let p = v
        .authorize_agent_read(&e.descriptor.created_by, agent(), scope(&e, 8))
        .unwrap();
    e.clock.set(NOW + 120_000);
    assert!(v.read_agent(&p, None, |_| ()).is_err());
    e.clock.set(NOW);
    assert!(v.read_agent(&p, None, |_| ()).is_err());
    let p = v
        .authorize_agent_read(&e.descriptor.created_by, agent(), scope(&e, 8))
        .unwrap();
    e.clock.set(NOW - 1);
    assert!(v.read_agent(&p, None, |_| ()).is_err());
    e.clock.set(NOW);
    assert!(v.read_agent(&p, None, |_| ()).is_err());
}

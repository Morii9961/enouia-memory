#![allow(dead_code)]
#[path = "agent_correction_fixture.rs"]
mod correction;
pub(crate) use correction::*;
use enouia_memory_contract::{
    common::ActorType, ids::PolicyId, json::Revision, next::*, time::Timestamp,
};

pub(crate) fn read_grant(e: &Env, p: &AutomationPolicy) -> OwnerReadGrant {
    OwnerReadGrant {
        grant_id: PolicyId::parse(&id("pol", 9100)).unwrap(),
        revision: Revision::new(1).unwrap(),
        vault_id: e.descriptor.vault_id.clone(),
        principal: actor(ActorType::Agent, 88),
        subject_id: p.subject_id.clone(),
        project_id: p.project_id.clone(),
        policies: vec![p.reference()],
        source_ids: vec![
            e.source(1, enouia_memory_contract::common::Sensitivity::Public)
                .record
                .source_id,
            e.source(8, enouia_memory_contract::common::Sensitivity::Public)
                .record
                .source_id,
        ],
        max_sensitivity: enouia_memory_contract::common::Sensitivity::Private,
        source_read: true,
        active: true,
        valid_from: Timestamp::from_unix_ms(NOW).unwrap(),
        valid_until: Timestamp::from_unix_ms(p.valid_until.unix_ms() + 86_400_000).unwrap(),
    }
}

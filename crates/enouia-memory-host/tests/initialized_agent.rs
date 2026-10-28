#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, next::*, time::Timestamp};
use enouia_memory_host::automation::{
    application::ApplicationFault, segmented_agent::SegmentedAgentHost,
};
use enouia_memory_vault::{
    OsIdSource,
    automation::{
        AUTOMATION_WITNESS, AutomationFault, AutomationVault,
        segmented::{RuntimeCaptureScope, SegmentedAgentScope},
    },
    fault::Faults,
};
use fixture::*;
use std::{collections::BTreeMap, path::Path, sync::Arc};

fn principal(role: ActorType) -> ActorRef {
    ActorRef {
        actor_type: role,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn runtime_scope(p: &AutomationPolicy) -> RuntimeCaptureScope {
    RuntimeCaptureScope {
        policy_id: p.policy_id.clone(),
        source_ids: p.source_ids.clone(),
        max_sensitivity: Sensitivity::Private,
        platform: "enouia-runtime".into(),
        account_scope: "account-synthetic".into(),
        default_timezone: "Asia/Shanghai".into(),
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn agent_scope(p: &AutomationPolicy) -> SegmentedAgentScope {
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
fn business_files(root: &Path) -> BTreeMap<String, Vec<u8>> {
    fn walk(root: &Path, dir: &Path, files: &mut BTreeMap<String, Vec<u8>>) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(root, &path, files);
            } else {
                let name = path
                    .strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .replace('\\', "/");
                // Exact native synchronization objects are not business data.
                if name != "indexes/host.lock" && name != "vault/LOCK" {
                    files.insert(name, std::fs::read(&path).unwrap());
                }
            }
        }
    }
    let mut files = BTreeMap::new();
    walk(root, root, &mut files);
    files
}
#[test]
fn old_factories_refuse_legal_owner_roots_before_snapshot_policy_or_record_decoding() {
    for corrupt_witness in [false, true] {
        let mut e = Env::new();
        let p = e.policy(1, 1, 8);
        e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
        e.descriptor.owner_subject_id = Some(p.subject_id.clone());
        let v = Arc::new(e.create(Faults::none()));
        v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        if corrupt_witness {
            // Prove refusal precedes current snapshot/policy inspection. Only
            // this owned disposable root is corrupted, after legal publication.
            std::fs::write(e.path.join(AUTOMATION_WITNESS), b"not-json").unwrap();
        }
        let before = business_files(&e.path);
        assert!(matches!(
            SegmentedAgentHost::from_authenticated_owner(
                v.clone(),
                e.descriptor.created_by.clone()
            ),
            Err(ApplicationFault::Vault(AutomationFault::Denied))
        ));
        assert!(matches!(
            v.authorize_agent_read(
                &e.descriptor.created_by,
                principal(ActorType::Agent),
                agent_scope(&p)
            ),
            Err(AutomationFault::Denied)
        ));
        assert!(matches!(
            v.authorize_runtime_capture(
                &e.descriptor.created_by,
                principal(ActorType::Client),
                runtime_scope(&p)
            ),
            Err(AutomationFault::Denied)
        ));
        // Existing non-segmented reader refuses at descriptor, even while the
        // sole legitimate writer lease is held; it must not attempt another.
        assert!(matches!(
            AutomationVault::open(
                &e.root,
                e.descriptor.host_id.clone(),
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none()
            ),
            Err(AutomationFault::Denied)
        ));
        assert_eq!(business_files(&e.path), before);
    }
}
#[test]
fn old_runtime_event_roots_retain_independent_agent_and_client_delegations() {
    let e = Env::new();
    let p = e.policy(1, 1, 8);
    let v = Arc::new(e.create(Faults::none()));
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let before = business_files(&e.path);
    let host =
        SegmentedAgentHost::from_authenticated_owner(v.clone(), e.descriptor.created_by.clone())
            .unwrap();
    let reader = host
        .reader(
            &e.descriptor.created_by,
            principal(ActorType::Agent),
            agent_scope(&p),
        )
        .unwrap();
    assert_eq!(reader.principal().actor_type, ActorType::Agent);
    assert!(
        v.authorize_runtime_capture(
            &e.descriptor.created_by,
            principal(ActorType::Client),
            runtime_scope(&p)
        )
        .is_ok()
    );
    assert!(matches!(
        v.authorize_runtime_capture(
            &e.descriptor.created_by,
            principal(ActorType::Agent),
            runtime_scope(&p)
        ),
        Err(AutomationFault::Denied)
    ));
    assert!(
        host.reader(
            &e.descriptor.created_by,
            principal(ActorType::System),
            agent_scope(&p)
        )
        .is_err()
    );
    assert_eq!(business_files(&e.path), before);
}

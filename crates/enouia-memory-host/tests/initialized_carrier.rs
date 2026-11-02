#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{
    common::*,
    hash::sha256,
    ids::*,
    next::*,
    source::{Completeness, SourceKind},
    time::Timestamp,
};
use enouia_memory_host::automation::{
    application::ApplicationFault, qualified_agent::QualifiedAgentHost,
};
use enouia_memory_vault::{
    OsIdSource,
    automation::{AutomationFault, AutomationVault, segmented::SegmentedAgentScope},
    fault::Faults,
};
use fixture::*;
use std::{collections::BTreeMap, sync::Arc};

fn agent() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Agent,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn scope(e: &Env) -> SegmentedAgentScope {
    let p = e.policy(1, 1, 1);
    SegmentedAgentScope {
        subject_id: p.subject_id,
        project_id: p.project_id,
        policy_ids: vec![p.policy_id],
        source_ids: p.source_ids,
        max_sensitivity: Sensitivity::Private,
        source_read: true,
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn imported(
    e: &Env,
    partial: bool,
    attached: bool,
) -> (
    CapturedSource,
    BTreeMap<enouia_memory_contract::hash::Sha256Hex, Vec<u8>>,
) {
    let mut source = e.source(200, Sensitivity::Private);
    source.record.source_kind = SourceKind::ImportedDocument;
    source.record.import_id = Some(ImportId::parse(&id("imp", 200)).unwrap());
    source.record.raw_object_hash = Some(sha256(source.text.as_bytes()));
    source.record.locator = Locator::ByteRange {
        start: 0,
        end: source.text.len() as u64,
    };
    source.record.completeness = if partial {
        Completeness::Partial
    } else {
        Completeness::Complete
    };
    let mut objects = BTreeMap::from([(
        sha256(source.text.as_bytes()),
        source.text.as_bytes().to_vec(),
    )]);
    if attached {
        let fixture: CapturedSource =
            serde_json::from_value(value("ui-records/source-attachments.json")).unwrap();
        source.attachments = fixture.attachments;
        let blob = b"synthetic".to_vec();
        for a in &mut source.attachments {
            a.source_id = source.record.source_id.clone();
            a.object_hash = Some(sha256(&blob));
            a.size_bytes = Some(blob.len() as u64);
        }
        objects.insert(sha256(&blob), blob);
        source.record.attachment_refs = source
            .attachments
            .iter()
            .map(|a| a.attachment_id.clone())
            .collect();
    }
    assert!(source.validate().is_empty(), "{:?}", source.validate());
    (source, objects)
}

#[test]
fn imported_carriers_refuse_old_factories_and_live_permits_before_projection_even_if_unselected_or_deleted()
 {
    for (partial, attached) in [(false, false), (true, false), (false, true)] {
        let mut e = Env::new();
        e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
        e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
        let vault = Arc::new(e.create(Faults::none()));
        e.populate(&vault, 1);
        vault
            .set_policy(&e.descriptor.created_by, e.policy(2, 200, 1))
            .unwrap();
        let host = QualifiedAgentHost::from_authenticated_owner(
            vault.clone(),
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let reader = host
            .reader(&e.descriptor.created_by, agent(), scope(&e))
            .unwrap();
        let permit = vault
            .authorize_qualified_agent_read(&e.descriptor.created_by, agent(), scope(&e))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let mut server =
            enouia_memory_host::mcp::qualified::Server::new(&reader, &profile).unwrap();
        assert_eq!(
            reader
                .page(&profile, &CatalogFilter::default(), None, 10, 262144, false)
                .unwrap()
                .matched_in_scope,
            1
        );
        let (source, objects) = imported(&e, partial, attached);
        let before = vault.snapshot().unwrap();
        // A real legal native import, including archived originals; no receipt
        // or imported state is fabricated by the Agent test.
        vault
            .capture_import(
                &e.descriptor.system_actor,
                vec![source.clone()],
                RecordCatalog::default(),
                &objects,
            )
            .unwrap();
        let published = vault.snapshot().unwrap();
        assert_ne!(published, before);
        assert!(matches!(
            QualifiedAgentHost::from_authenticated_owner(
                vault.clone(),
                e.descriptor.created_by.clone()
            ),
            Err(ApplicationFault::Vault(AutomationFault::Denied))
        ));
        assert!(matches!(
            vault.authorize_qualified_agent_read(&e.descriptor.created_by, agent(), scope(&e)),
            Err(AutomationFault::Denied)
        ));
        let mut projected = false;
        assert!(matches!(
            vault.read_agent(&permit, None, |_| {
                projected = true;
            }),
            Err(AutomationFault::Denied)
        ));
        assert!(!projected);
        let refused = server.handle(&serde_json::json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"memory_search","arguments":{"unadopted":"must not decode"}}})).unwrap();
        assert_eq!(refused["error"]["message"], "session_closed");
        assert!(server.closed());
        assert_eq!(
            server.counts.search
                + server.counts.read
                + server.counts.source
                + server.counts.related
                + server.counts.recall,
            0
        );
        assert!(enouia_memory_host::mcp::qualified::Server::new(&reader, &profile).is_none());
        assert!(
            reader
                .page(&profile, &CatalogFilter::default(), None, 10, 262144, false)
                .is_err()
        );
        assert_eq!(vault.snapshot().unwrap(), published);
        vault
            .delete(
                &e.descriptor.created_by,
                vec![source.record.source_id],
                vec![],
            )
            .unwrap();
        assert!(vault.check_original_agent_carrier().is_err());
        assert!(
            vault
                .authorize_qualified_agent_read(&e.descriptor.created_by, agent(), scope(&e))
                .is_err()
        );
    }
}

#[test]
fn original_nonsegmented_capture_cannot_admit_typed_attachments_after_dependency_upgrade() {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::HostV3;
    let vault = AutomationVault::create(
        &e.root,
        e.descriptor.clone(),
        e.policy(1, 1, 1),
        e.clock.clone(),
        Arc::new(OsIdSource),
        Faults::none(),
    )
    .unwrap();
    let mut source: CapturedSource =
        serde_json::from_value(value("ui-records/source-attachments.json")).unwrap();
    source.record_fields.as_mut().unwrap().conversation_id = None;
    source.record.original_conversation_id = None;
    source.record.original_message_id = None;
    source.record.branch_id = enouia_memory_contract::json::Knowable::Known(None);
    assert!(source.validate().is_empty(), "{:?}", source.validate());
    let before = std::fs::read(
        e.path
            .join(enouia_memory_contract::layout::current_pointer()),
    )
    .unwrap();
    assert!(matches!(
        vault.capture(&e.descriptor.system_actor, source.clone()),
        Err(AutomationFault::Denied)
    ));
    assert_eq!(
        std::fs::read(
            e.path
                .join(enouia_memory_contract::layout::current_pointer())
        )
        .unwrap(),
        before
    );
    // The original empty typed-carrier field still works with its exact old
    // source contract; the dependency does not disable RuntimeEvent capture.
    source.attachments.clear();
    source.record.attachment_refs.clear();
    vault.capture(&e.descriptor.system_actor, source).unwrap();
}

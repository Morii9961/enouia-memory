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
use enouia_memory_host::automation::qualified_agent::{INITIALIZED_CAPABILITY, QualifiedAgentHost};
use enouia_memory_vault::{automation::segmented::SegmentedAgentScope, fault::Faults};
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
fn initialized_native_reads_filter_attachment_copies_and_keep_real_formation_and_context() {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    let policy = e.policy(2, 200, 1);
    e.descriptor.owner_subject_id = Some(policy.subject_id.clone());
    let vault = Arc::new(e.create(Faults::none()));
    vault
        .set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let (mut source, mut objects) = imported(&e, false, true);
    let mut hidden = source.attachments[0].clone();
    hidden.attachment_id = AttachmentId::parse(&id("att", 999)).unwrap();
    hidden.original_name = Some("HIDDEN_ATTACHMENT_MARKER".into());
    hidden.sensitivity = Sensitivity::HighlySensitive;
    let bytes = b"hidden synthetic attachment".to_vec();
    hidden.object_hash = Some(sha256(&bytes));
    hidden.size_bytes = Some(bytes.len() as u64);
    objects.insert(sha256(&bytes), bytes);
    source
        .record
        .attachment_refs
        .push(hidden.attachment_id.clone());
    source.record.extensions.insert(
        "memory-init.metadata".into(),
        serde_json::json!({"attachments":[hidden]}),
    );
    source.attachments.push(hidden.clone());
    assert!(source.validate().is_empty());
    vault
        .capture_import(
            &e.descriptor.system_actor,
            vec![source.clone()],
            RecordCatalog::default(),
            &objects,
        )
        .unwrap();
    let request = vault
        .prepare(&e.descriptor.system_actor, e.input(&source, &policy, 200))
        .unwrap();
    let response = serde_json::to_string(
        &vault
            .evaluate(&e.descriptor.system_actor, &request)
            .unwrap(),
    )
    .unwrap();
    let binding = GenerationBinding {
        operation_id: request.operation_id.clone(),
        input_hash: digest(&request),
        response_hash: sha256(response.as_bytes()),
    };
    let receipts = vault
        .apply_batch(
            &e.descriptor.system_actor,
            vec![request],
            vec![response],
            vec![binding],
        )
        .unwrap();
    assert!(
        QualifiedAgentHost::from_authenticated_owner(
            vault.clone(),
            e.descriptor.created_by.clone()
        )
        .is_err()
    );
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        vault.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let mut selected = scope(&e);
    selected.policy_ids = vec![policy.policy_id];
    selected.source_ids = vec![source.record.source_id.clone()];
    let reader = host
        .reader(&e.descriptor.created_by, agent(), selected.clone())
        .unwrap();
    assert_eq!(reader.capability(), INITIALIZED_CAPABILITY);
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    assert!(enouia_memory_host::mcp::qualified::Server::new(&reader, &profile).is_none());
    let before = vault.snapshot().unwrap();
    let page = reader
        .page(&profile, &CatalogFilter::default(), None, 10, 262144, false)
        .unwrap();
    assert_eq!(page.capability, INITIALIZED_CAPABILITY);
    assert_eq!(page.matched_in_scope, 1);
    let projected = reader
        .source(&source.record.source_id, None, 0, None, 8192)
        .unwrap()
        .unwrap();
    assert_eq!(projected["attachments"].as_array().unwrap().len(), 1);
    assert_eq!(
        projected["record"]["attachment_refs"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(projected["record"]["extensions"], serde_json::json!({}));
    assert_eq!(projected["effective_sensitivity"], "private");
    assert_eq!(projected["attachments_complete_in_scope"], true);
    let detail = reader
        .detail(&profile, &receipts[0].result.memory_id, None, true, 262144)
        .unwrap()
        .unwrap();
    for v in [&projected, &detail] {
        let text = serde_json::to_string(v).unwrap();
        assert!(!text.contains("HIDDEN_ATTACHMENT_MARKER"));
        assert!(!text.contains(hidden.attachment_id.as_str()));
        assert!(!text.contains(hidden.object_hash.as_ref().unwrap().as_str()));
    }
    let related = reader
        .related(&profile, &receipts[0].result.memory_id, None, 262144)
        .unwrap()
        .unwrap();
    assert_eq!(related["capability"], INITIALIZED_CAPABILITY);
    selected.source_read = false;
    let no_source = host
        .reader(&e.descriptor.created_by, agent(), selected)
        .unwrap();
    assert!(
        no_source
            .source(&source.record.source_id, None, 0, None, 8192)
            .is_err()
    );
    let query = RecallQuery {
        text: "".into(),
        conditions: vec![],
        event_window: None,
        at: enouia_memory_contract::time::Timestamp::from_unix_ms(NOW).unwrap(),
        intent: RecallIntent::Browse,
        limit: 8,
        max_bytes: 16384,
    };
    let req = RequestId::parse(&id("req", 77)).unwrap();
    let recall = reader.recall(&profile, &query, req.clone()).unwrap();
    assert_eq!(recall["trace"]["selected"].as_array().unwrap().len(), 1);
    assert_eq!(reader.context_trace(&req).unwrap().unwrap(), recall);
    assert_eq!(vault.snapshot().unwrap(), before);
    vault
        .delete(
            &e.descriptor.created_by,
            vec![source.record.source_id],
            vec![],
        )
        .unwrap();
    assert_eq!(
        reader
            .page(&profile, &CatalogFilter::default(), None, 10, 262144, false)
            .unwrap()
            .matched_in_scope,
        0
    );
    assert!(reader.context_trace(&req).is_err());
}

#[test]
fn initialized_native_partial_sources_keep_exact_policy_subject_role_and_generation_gates() {
    let old = Env::new();
    let old_vault = Arc::new(old.create(Faults::none()));
    assert!(
        QualifiedAgentHost::from_authenticated_initialized_owner(
            old_vault,
            old.descriptor.created_by.clone()
        )
        .is_err()
    );
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    let policy = e.policy(2, 200, 1);
    e.descriptor.owner_subject_id = Some(policy.subject_id.clone());
    let vault = Arc::new(e.create(Faults::none()));
    vault
        .set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let (source, objects) = imported(&e, true, false);
    vault
        .capture_import(
            &e.descriptor.system_actor,
            vec![source.clone()],
            RecordCatalog::default(),
            &objects,
        )
        .unwrap();
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        vault.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let mut selected = scope(&e);
    selected.policy_ids = vec![policy.policy_id.clone()];
    selected.source_ids = vec![source.record.source_id.clone()];
    let mut wrong = selected.clone();
    wrong.subject_id = SubjectId::parse(&id("sub", 999)).unwrap();
    assert!(
        host.reader(&e.descriptor.created_by, agent(), wrong)
            .is_err()
    );
    let mut wrong = selected.clone();
    wrong.max_sensitivity = Sensitivity::HighlySensitive;
    assert!(
        host.reader(&e.descriptor.created_by, agent(), wrong)
            .is_err()
    );
    let mut claimed = agent();
    claimed.actor_type = ActorType::System;
    assert!(
        host.reader(&e.descriptor.created_by, claimed, selected.clone())
            .is_err()
    );
    let reader = host
        .reader(&e.descriptor.created_by, agent(), selected.clone())
        .unwrap();
    assert_eq!(
        reader
            .source(&source.record.source_id, None, 0, None, 8192)
            .unwrap()
            .unwrap()["record"]["completeness"],
        "partial"
    );
    host.lock().unwrap();
    host.unlock(&e.descriptor.created_by).unwrap();
    assert!(
        reader
            .source(&source.record.source_id, None, 0, None, 8192)
            .is_err()
    );
    let reader = host
        .reader(&e.descriptor.created_by, agent(), selected)
        .unwrap();
    let mut changed = policy;
    changed.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    vault.set_policy(&e.descriptor.created_by, changed).unwrap();
    assert!(
        reader
            .source(&source.record.source_id, None, 0, None, 8192)
            .is_err()
    );
}

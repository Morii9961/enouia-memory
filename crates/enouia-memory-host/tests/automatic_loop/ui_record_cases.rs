use super::*;

fn value(n: &str) -> Value {
    support::fixture(&format!("../ui-records/{n}.json"))
}
fn env() -> Env {
    let mut e = formation_cases::env();
    e.descriptor.layout = AutomationLayout::HostV3;
    e
}
#[test]
fn new_layout_atomically_stores_fields_catalog_and_real_receipt_across_reopen() {
    let e = env();
    let vault = e.vault(Faults::none());
    let source: CapturedSource = serde_json::from_value(value("source")).unwrap();
    let catalog: RecordCatalog = serde_json::from_value(value("catalog")).unwrap();
    let before = vault.inspect().unwrap();
    // Missing v3 source fields/catalog cannot advance a new root.
    let mut missing = source.clone();
    missing.record_fields = None;
    assert!(vault.capture(&e.descriptor.system_actor, missing).is_err());
    assert_eq!(vault.inspect().unwrap(), before);
    let captured = vault
        .capture_with_records(
            &e.descriptor.system_actor,
            source.clone(),
            Some(catalog.clone()),
        )
        .unwrap();
    assert_eq!(
        vault
            .capture_with_records(&e.descriptor.system_actor, source, Some(catalog.clone()))
            .unwrap(),
        captured
    );
    let seed: AdmissionInput = serde_json::from_value(value("input")).unwrap();
    let request = vault.prepare(&e.descriptor.system_actor, seed).unwrap();
    let current = vault.inspect().unwrap();
    let mut forged_data = current.data.clone();
    let record: OwnerActionRecord = serde_json::from_value(value("owner-record")).unwrap();
    forged_data
        .owner_actions
        .as_mut()
        .unwrap()
        .records
        .push(record);
    forged_data
        .owner_actions
        .as_mut()
        .unwrap()
        .withdrawal_watermark = Revision::new(2).unwrap();
    // Even a structurally owner-labelled record cannot ride a System capture.
    assert!(
        AutomationCommit::assemble(
            &e.descriptor,
            Some(&before),
            e.descriptor.system_actor.clone(),
            AutomationChange::Capture,
            forged_data.clone(),
            CommitId::from_random([45; 16]),
            vault.current_time().unwrap()
        )
        .is_err()
    );
    // The reserved owner transaction has no executable entry in this slice.
    assert!(
        AutomationCommit::assemble(
            &e.descriptor,
            Some(&current),
            e.descriptor.created_by.clone(),
            AutomationChange::OwnerAction,
            forged_data,
            CommitId::from_random([46; 16]),
            vault.current_time().unwrap()
        )
        .is_err()
    );
    assert_eq!(vault.inspect().unwrap(), current);
    let mut result = request.clone();
    result.content = "Synthetic subject says: I prefer quiet workspaces.".into();
    result.interpretation = Interpretation::ClearSelfReport;
    let decision = with_stored_context(
        &current.data,
        &current.snapshot(),
        &e.descriptor.system_actor,
        &vault.current_time().unwrap(),
        |ctx| evaluate(&result, ctx),
    )
    .unwrap();
    let response = serde_json::to_string(&decision).unwrap();
    let binding = GenerationBinding {
        operation_id: request.operation_id.clone(),
        input_hash: digest(&request),
        response_hash: sha256(response.as_bytes()),
    };
    let key = admission_batch_key(std::slice::from_ref(&request));
    let mut wrong = decision.clone();
    let wrong_fields = wrong.input.record_fields.as_mut().unwrap();
    wrong_fields.evidence[0].quote = wrong_fields.evidence[0].quote.replace("quiet", "wrong");
    for entity in &mut wrong_fields.entities {
        for evidence in &mut entity.evidence {
            evidence.quote = evidence.quote.replace("quiet", "wrong");
        }
    }
    assert!(wrong.input.validate().is_empty());
    let bad_response = serde_json::to_string(&wrong).unwrap();
    let bad_binding = GenerationBinding {
        response_hash: sha256(bad_response.as_bytes()),
        ..binding.clone()
    };
    assert!(
        vault
            .apply_batch(
                &e.descriptor.system_actor,
                key.clone(),
                vec![request.clone()],
                vec![wrong],
                vec![bad_binding],
                vec![bad_response]
            )
            .is_err()
    );
    assert_eq!(vault.inspect().unwrap(), current);
    let receipts = vault
        .apply_batch(
            &e.descriptor.system_actor,
            key.clone(),
            vec![request],
            vec![decision],
            vec![binding.clone()],
            vec![response],
        )
        .unwrap();
    assert_eq!(receipts.len(), 1);
    let state = vault.inspect().unwrap();
    assert_eq!(state.data.record_catalog, Some(catalog));
    let fields = state.data.memories[0]
        .decision
        .input
        .record_fields
        .as_ref()
        .unwrap();
    assert_eq!(fields.title, "Quiet workspaces");
    assert_eq!(
        fields.annotations.as_ref().unwrap().memory,
        receipts[0].result
    );
    let conversation = &state.data.record_catalog.as_ref().unwrap().conversations[0];
    assert_eq!(
        fields.evidence[0].reference.source_id,
        conversation.turns[0].source.source_id
    );
    // Owner-labelled machine output is not accepted as an owner action.
    let mut fake = state.data.memories[0].decision.input.clone();
    fake.actor = e.descriptor.created_by.clone();
    assert!(
        with_stored_context(
            &state.data,
            &state.snapshot(),
            &e.descriptor.system_actor,
            &vault.current_time().unwrap(),
            |ctx| evaluate(&fake, ctx)
        )
        .is_err()
    );
    drop(vault);
    let reopened = e.open();
    assert_eq!(reopened.inspect().unwrap(), state);
    assert_eq!(
        reopened
            .replay_job(
                &e.descriptor.system_actor,
                &key,
                std::slice::from_ref(&binding)
            )
            .unwrap()
            .unwrap(),
        receipts
    );
    reopened
        .delete_source(
            &e.descriptor.created_by,
            conversation.turns[0].source.source_id.clone(),
        )
        .unwrap();
    assert!(
        reopened
            .replay_job(&e.descriptor.system_actor, &key, &[binding])
            .is_err()
    );
    assert!(
        reopened
            .recall(
                &e.descriptor.system_actor,
                "workspaces",
                Sensitivity::Private,
                8
            )
            .unwrap()
            .is_empty()
    );
}
#[test]
fn legacy_layouts_refuse_new_fields_and_old_descriptor_reader_refuses_v3() {
    for layout in [AutomationLayout::HostV1, AutomationLayout::HostV2] {
        let mut e = formation_cases::env();
        e.descriptor.layout = layout;
        if layout == AutomationLayout::HostV1 {
            e.policy.rule_version = RuleVersion::V2;
            e.policy.allowed_forms.clear();
        }
        let vault = e.vault(Faults::none());
        let before = vault.inspect().unwrap();
        let source: CapturedSource = serde_json::from_value(value("source")).unwrap();
        assert!(
            vault
                .capture_with_records(
                    &e.descriptor.system_actor,
                    source,
                    Some(serde_json::from_value(value("catalog")).unwrap())
                )
                .is_err()
        );
        assert_eq!(vault.inspect().unwrap(), before);
    }
    #[derive(serde::Deserialize)]
    #[serde(rename_all = "kebab-case")]
    #[allow(dead_code)]
    enum OldLayout {
        AutomationHostV1,
        AutomationHostV2,
    }
    assert!(serde_json::from_value::<OldLayout>(json!("automation-host-v3")).is_err());
    assert!(enouia_memory_contract::store::parse_store::<enouia_memory_contract::store::VaultDescriptor>(&canonical_bytes(&value("descriptor")).unwrap()).is_err());
}

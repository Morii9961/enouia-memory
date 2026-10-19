//! Workspace-channel history planning on synthetic archived data only.
use super::*;
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::ids::CommitId;
use enouia_memory_contract::policy::PolicyRecord;
use enouia_memory_contract::ports::{CommitRequest, StagedRecord};

const RAW: &[u8] =
    include_bytes!("../../../../tests/fixtures/history/initialization-synthetic.json");

fn imported(env: &Env, bytes: &[u8]) -> Value {
    imported_with_timeout(env, bytes, Duration::from_secs(60))
}

fn imported_with_timeout(env: &Env, bytes: &[u8], timeout: Duration) -> Value {
    std::fs::write(env.base.join("history.json"), bytes).unwrap();
    let token = env.pick(PickKind::ImportFile, "history.json");
    let started = env.ok(
        "import_start",
        json!({"importToken": token, "accountAlias": "synthetic-history"}),
    );
    let id = OperationId::parse(started["operationId"].as_str().unwrap()).unwrap();
    let done = env.ws.operations().wait(&id, timeout).unwrap();
    assert_eq!(done["state"], "succeeded", "{done}");
    env.ok("import_list", json!({}))["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["importId"] == done["result"]["importId"])
        .unwrap()
        .clone()
}

fn args(import: &Value) -> Value {
    json!({"importId": import["importId"], "importRevision": import["revision"],
        "messagesPerBatch": 2, "maxPathReferences": 1000, "cursor": null, "limit": 2})
}

#[test]
fn local_history_admission_is_atomic_replayable_and_survives_reopen() {
    use enouia_memory_contract::history::HistoryMaterialization;
    use enouia_memory_contract::session::ProviderBinding;
    let env = Env::new("history-admission");
    let import = imported(&env, RAW);
    let plan = env.ok("history_plan", args(&import));
    let key = b"synthetic-workspace-history-admission";
    let open = env.ws.open().unwrap();
    let input = enouia_memory_vault::service::SessionStart {
        owner: open.owner.clone(),
        surface: ClientSurface::WindowsApp,
        sensitivity: Sensitivity::Private,
        policy_id: env.ws.policy(&open).unwrap(),
    };
    let admission = HistoryMaterialization {
        import_id: serde_json::from_value(import["importId"].clone()).unwrap(),
        import_revision: serde_json::from_value(import["revision"].clone()).unwrap(),
        input_object_hash: serde_json::from_value(plan["identity"]["inputObjectHash"].clone())
            .unwrap(),
        preview_plan_hash: serde_json::from_value(plan["planHash"].clone()).unwrap(),
        preview_commit_id: serde_json::from_value(plan["identity"]["commitId"].clone()).unwrap(),
        messages_per_batch: 2,
        max_path_references: 1000,
        batch_indices: vec![1, 0],
        binding: ProviderBinding {
            provider: "openai".into(),
            model: "synthetic-history-model".into(),
            adapter_version: "text-http-1".into(),
        },
        subject_id: SubjectId::parse(&open.owner.actor_id.as_str().replacen("prn_", "sub_", 1))
            .unwrap(),
        max_candidates: 8,
        max_pending: 100,
        max_reserved_tokens: 10000,
        max_reserved_cost_microusd: None,
        submission_key_hash: sha256(key),
    };
    let before = open.vault.pin_current().unwrap();
    let first =
        session::start_history_materialization(&open.vault, &input, key, &admission).unwrap();
    let after = open.vault.pin_current().unwrap();
    assert_eq!(after.sequence, before.sequence + 1);
    let saved = session::get(&open.vault, &after, &first.id.0).unwrap();
    assert_eq!(saved.history_materialization, Some(admission.clone()));
    assert!(saved.extraction_jobs.is_empty() && saved.provider_invocations.is_empty());
    assert!(
        session::events(&open.vault, &after, &first.id.0, &first.id.1)
            .unwrap()
            .is_empty()
    );
    let replay =
        session::start_history_materialization(&open.vault, &input, key, &admission).unwrap();
    assert_eq!(replay.id, first.id);
    assert_eq!(open.vault.pin_current().unwrap().commit_id, after.commit_id);
    let mut changed = admission.clone();
    changed.batch_indices.reverse();
    assert_eq!(
        session::start_history_materialization(&open.vault, &input, key, &changed)
            .unwrap_err()
            .code(),
        MemoryErrorCode::IdempotencyConflict
    );
    assert_eq!(open.vault.pin_current().unwrap().commit_id, after.commit_id);
    let mut stale = admission.clone();
    stale.submission_key_hash = sha256(b"synthetic-new-stale-admission");
    assert_eq!(
        session::start_history_materialization(
            &open.vault,
            &input,
            b"synthetic-new-stale-admission",
            &stale
        )
        .unwrap_err()
        .code(),
        MemoryErrorCode::RevisionConflict
    );
    assert_eq!(open.vault.pin_current().unwrap().commit_id, after.commit_id);
    env.ws.shutdown();
    env.ws.open_root(&env.base.join("vault")).unwrap();
    let reopened = env.ws.open().unwrap();
    assert_eq!(
        session::start_history_materialization(&reopened.vault, &input, key, &admission)
            .unwrap()
            .id,
        first.id
    );
    assert_eq!(
        reopened.vault.pin_current().unwrap().commit_id,
        after.commit_id
    );
    let mut bad_key = admission;
    bad_key.submission_key_hash = sha256(b"different-key");
    assert_eq!(
        session::start_history_materialization(&reopened.vault, &input, key, &bad_key)
            .unwrap_err()
            .code(),
        MemoryErrorCode::InvalidRequest
    );
    assert_eq!(
        reopened.vault.pin_current().unwrap().commit_id,
        after.commit_id
    );
}

// Only deliberately modified, validated synthetic records, through a real
// Vault commit. This is fault/revocation input, not a product write endpoint.
fn rewrite<T: Record>(
    env: &Env,
    kind: RecordKind,
    id: &str,
    change: impl FnOnce(&mut Value),
) -> Value {
    let open = env.ws.open().unwrap();
    let pin = open.vault.pin_current().unwrap();
    let entry = open.vault.record_entry(&pin, kind, id).unwrap().unwrap();
    let mut value: Value = serde_json::from_slice(
        &open
            .vault
            .read_record(&pin, &RecordRef::new(kind, id, entry.revision))
            .unwrap(),
    )
    .unwrap();
    let revision = Revision::new(entry.revision.get() + 1).unwrap();
    value["revision"] = json!(revision);
    change(&mut value);
    let bytes = enouia_memory_contract::json::canonical_bytes(&value).unwrap();
    enouia_memory_contract::parse_record::<T>(&bytes).unwrap();
    let operation = if kind == RecordKind::Policy {
        OperationKind::PolicyChange
    } else {
        OperationKind::Import
    };
    let hash = sha256(&bytes);
    open.vault
        .commit(CommitRequest {
            commit_id: CommitId::from_random(env.ws.config.ids.random_16()),
            expected_commit_id: Some(pin.commit_id),
            principal: open.owner.clone(),
            operation_kind: operation,
            idempotency: IdempotencyScope {
                principal_id: open.owner.actor_id.clone(),
                operation_kind: operation,
                key_hash: hash.clone(),
            },
            request_payload_hash: hash,
            expected_revisions: vec![(kind, id.to_owned(), Some(entry.revision))],
            records: vec![StagedRecord {
                record_kind: kind,
                record_id: id.to_owned(),
                revision,
                bytes,
            }],
            objects: vec![],
        })
        .unwrap();
    value
}

#[test]
fn history_plan_binds_archived_sources_preserves_roles_and_pages_without_writing() {
    let env = Env::new("history-plan-coverage");
    let import = imported(&env, RAW);
    let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
    let first = env.ok("history_plan", args(&import));
    assert_eq!(first["identity"]["importId"], import["importId"]);
    assert_eq!(first["identity"]["importRevision"], import["revision"]);
    assert_eq!(first["identity"]["inputObjectHash"], json!(sha256(RAW)));
    assert_eq!(first["summary"]["dated"], 6);
    assert_eq!(first["summary"]["undated"], 1);
    assert_eq!(first["summary"]["unparseable"], 1);
    assert_eq!(first["summary"]["incompleteMessages"], 1);
    assert_eq!(first["summary"]["unavailableAttachments"], 1);
    assert_eq!(first["summary"]["earliest"], "2023-01-01T00:00:00.000Z");
    assert_eq!(first["coverageIsCompleteHistory"], false);
    assert_eq!(first["readOnly"], true);
    assert_eq!(first["batches"].as_array().unwrap().len(), 2);
    assert_eq!(env.ok("history_plan", args(&import)), first);
    let mut all = first["batches"].as_array().unwrap().clone();
    let mut cursor = first["nextCursor"].clone();
    while !cursor.is_null() {
        let mut request = args(&import);
        request["cursor"] = cursor;
        let page = env.ok("history_plan", request);
        assert_eq!(page["planHash"], first["planHash"]);
        all.extend(page["batches"].as_array().unwrap().clone());
        cursor = page["nextCursor"].clone();
    }
    assert_eq!(
        all.len() as u64,
        first["summary"]["batches"].as_u64().unwrap()
    );
    let mut saw_assistant = false;
    for (index, batch) in all.iter().enumerate() {
        assert_eq!(batch["batchIndex"], index);
        for message in batch["messages"].as_array().unwrap() {
            let source = env.ok(
                "source_excerpt",
                json!({"sourceId": message["sourceId"],
                "sourceRevision": message["sourceRevision"], "startByte": null, "maxBytes": 4096}),
            );
            assert_eq!(source["speakerRole"], message["speakerRole"]);
            if message["speakerRole"] == "assistant" {
                saw_assistant = true;
                assert_eq!(message["evidenceClass"], "model_claim");
            }
        }
    }
    assert!(saw_assistant);
    assert!(
        !first.to_string().contains("纸质地图"),
        "no raw source text in the planning result"
    );
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
        before
    );
    assert_eq!(env.ok("session_list", json!({}))["items"], json!([]));
    assert_eq!(
        env.ok("candidate_list", json!({"cursor": null,"limit": 20}))["total"],
        0
    );
}

#[test]
fn history_plan_cursors_bind_parameters_current_head_and_each_vault_opening() {
    let env = Env::new("history-plan-cursors");
    let import = imported(&env, RAW);
    let first = env.ok("history_plan", args(&import));
    let mut request = args(&import);
    request["cursor"] = first["nextCursor"].clone();
    let mut changed = request.clone();
    changed["messagesPerBatch"] = json!(1);
    assert_eq!(
        env.err("history_plan", changed)["rules"][0],
        "history.plan_stale"
    );
    env.ok("session_new", json!({}));
    assert_eq!(
        env.err("history_plan", request.clone())["rules"][0],
        "history.plan_stale"
    );
    let fresh = env.ok("history_plan", args(&import));
    request["cursor"] = fresh["nextCursor"].clone();
    env.ok("vault_lock", json!({}));
    assert_eq!(
        env.err("history_plan", args(&import))["code"],
        "vault_locked"
    );
    env.ok("vault_unlock", json!({}));
    assert_eq!(
        env.err("history_plan", request)["rules"][0],
        "history.plan_stale"
    );
    assert_ne!(
        env.ok("history_plan", args(&import))["planHash"],
        fresh["planHash"]
    );
    let other = Env::new("history-plan-other");
    assert_eq!(
        other.err("history_plan", args(&import))["code"],
        "not_found"
    );
}

#[test]
fn history_plan_rejects_stale_imports_duplicates_reference_overflow_and_paths() {
    let env = Env::new("history-plan-refusals");
    let import = imported(&env, RAW);
    let mut request = args(&import);
    request["importRevision"] = json!(999);
    assert_eq!(
        env.err("history_plan", request)["rules"][0],
        "history.import_revision"
    );
    let mut request = args(&import);
    request["maxPathReferences"] = json!(1);
    assert_eq!(
        env.err("history_plan", request)["rules"][0],
        "history.reference_budget"
    );
    let mut request = args(&import);
    request["path"] = json!("synthetic-not-a-real-path");
    assert_eq!(env.err("history_plan", request)["code"], "invalid_request");
    let duplicate = imported(&env, RAW);
    assert!(!duplicate["duplicateOf"].is_null());
    assert_eq!(
        env.err("history_plan", args(&duplicate))["rules"][0],
        "history.import_incomplete"
    );
    let request = json!({"schemaVersion": 1,"requestId":"req_00000000-0000-4000-8000-000000000123",
        "command":"history_plan","idempotencyKey":null,"arguments":args(&import)});
    assert!(HostSurface::Workspace.allows(&request));
    assert!(!HostSurface::QuickSearch.allows(&request));
}

#[test]
fn history_plan_applies_current_source_read_policy_without_an_egress_grant() {
    let env = Env::new("history-plan-policy");
    let import = imported(&env, RAW);
    env.ok("history_plan", args(&import));
    let id = {
        let open = env.ws.open().unwrap();
        let pin = open.vault.pin_current().unwrap();
        latest_records::<PolicyRecord>(&open.vault, &pin, RecordKind::Policy).unwrap()[0]
            .policy_id
            .to_string()
    };
    rewrite::<PolicyRecord>(&env, RecordKind::Policy, &id, |value| {
        value["scopes"]
            .as_array_mut()
            .unwrap()
            .retain(|scope| scope != "source:read");
    });
    assert_eq!(
        env.err("history_plan", args(&import))["rules"][0],
        "history.source_read"
    );
}

#[test]
fn history_plan_refuses_old_archive_bindings_after_an_import_revises_a_source() {
    let env = Env::new("history-plan-source-revision");
    let original = imported(&env, RAW);
    env.ok("history_plan", args(&original));
    let mut changed: Value = serde_json::from_slice(RAW).unwrap();
    changed[1]["mapping"]["p2"]["message"]["content"]["parts"][0] =
        json!("Synthetic revised preference");
    imported(&env, &serde_json::to_vec(&changed).unwrap());
    let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
    assert_eq!(
        env.err("history_plan", args(&original))["code"],
        "broken_provenance"
    );
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
        before
    );
}

#[test]
fn history_plan_refuses_adapter_drift_and_bounds_the_archived_object_before_reading() {
    let env = Env::new("history-plan-manifest");
    let import = imported(&env, RAW);
    let id = import["importId"].as_str().unwrap();
    let changed = rewrite::<ImportManifest>(&env, RecordKind::Import, id, |value| {
        value["adapter"]["version"] = json!("999")
    });
    let mut request = args(&import);
    request["importRevision"] = changed["revision"].clone();
    assert_eq!(
        env.err("history_plan", request)["rules"][0],
        "import.adapter_changed"
    );
    // Use a genuine oversized archived Markdown input. Vault correctly
    // refuses changing the byte size of an already archived import.
    // This is fixture setup, not a planning/lifecycle deadline. Two measured
    // runs hit the ordinary 60s import wait while still running, before any
    // planning assertion. Give this genuine 64 MiB archive its own bounded
    // setup window; retain the production limits and concurrent-test barriers.
    let started = std::time::Instant::now();
    let oversized = imported_with_timeout(
        &env,
        &vec![b' '; 64 * 1024 * 1024 + 1],
        Duration::from_secs(180),
    );
    eprintln!("oversized synthetic archive setup: {:?}", started.elapsed());
    let request = args(&oversized);
    assert_eq!(
        env.err("history_plan", request)["rules"][0],
        "history.input_budget"
    );
}

#[test]
fn history_plan_never_revives_raw_after_a_source_dependency_is_purged() {
    let env = Env::new("history-plan-purge");
    let import = imported(&env, RAW);
    env.ok("history_plan", args(&import));
    let candidate = {
        let open = env.ws.open().unwrap();
        let pin = open.vault.pin_current().unwrap();
        let source = latest_records::<SourceRecord>(&open.vault, &pin, RecordKind::Source)
            .unwrap()
            .into_iter()
            .find(|source| source.speaker_role == enouia_memory_contract::common::SpeakerRole::User)
            .unwrap();
        let proposal = Proposal::create(
            ProposedType::Fact,
            "Synthetic history review",
            Map::from_iter([
                ("claim_key".into(), json!("synthetic.history")),
                (
                    "subject_ids".into(),
                    json!([open.owner.actor_id.as_str().replacen("prn_", "sub_", 1)]),
                ),
            ]),
            vec![EvidenceSpec::content(source.source_id, source.revision)],
        );
        let Proposed::Stored(stored) = propose(
            &open.vault,
            &proposal,
            &Origin::owner(open.owner.clone()),
            b"synthetic-history-review",
        )
        .unwrap() else {
            panic!("candidate")
        };
        stored.id
    };
    let plan = env.ok("review_plan",json!({"decisions":[{"candidateId":candidate,"revision":1,"action":"accept","editedContent":null,"mergeTarget":null}]}));
    env.ok(
        "review_confirm",
        json!({"planId":plan["planId"],"diffHash":plan["diffHash"]}),
    );
    let memory = env.ok(
        "memory_list",
        json!({"includeInactive":false,"cursor":null,"limit":20}),
    )["items"][0]["memoryId"]
        .clone();
    let plan = env.ok(
        "forget_plan",
        json!({"memoryId":memory,"mode":"purge","withDependents":true}),
    );
    env.ok(
        "review_confirm",
        json!({"planId":plan["planId"],"diffHash":plan["diffHash"]}),
    );
    let response = env.send("history_plan", args(&import));
    assert_eq!(response["kind"], "memory_error");
    assert!(response["result"].is_null());
}

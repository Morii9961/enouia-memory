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
// Every port is a trap: local H2 admission, status and resume must not touch it.
struct NoHistoryPorts;
impl enouia_memory_contract::ports::SecretStore for NoHistoryPorts {
    fn read(
        &self,
        _: &str,
    ) -> Result<enouia_memory_contract::ports::SecretBytes, enouia_memory_contract::MemoryError>
    {
        panic!("local history tasks must never read credentials")
    }
}
impl enouia_memory_provider::transport::Transport for NoHistoryPorts {
    fn exchange(
        &self,
        _: &enouia_memory_provider::codec::WireRequest,
        _: &enouia_memory_contract::ports::SecretBytes,
        _: &dyn enouia_memory_contract::foundation::Cancellation,
        _: Duration,
        _: &mut dyn FnMut(&[u8]) -> Result<(), enouia_memory_contract::MemoryError>,
    ) -> Result<(), enouia_memory_contract::MemoryError> {
        panic!("local history tasks must never send")
    }
}
fn local_model(env: &Env) -> NativeProvider {
    use enouia_memory_contract::provider::{Capability, ProviderCapabilities, TokenCounting};
    use enouia_memory_provider::codec::Api;
    let provider = NativeProvider {
        transport: Arc::new(NoHistoryPorts),
        secrets: Arc::new(NoHistoryPorts),
        destinations: vec![NativeDestination {
            api: Api::OpenAiResponses,
            capabilities: ProviderCapabilities {
                schema_version: enouia_memory_contract::json::SchemaVersion,
                binding: Api::OpenAiResponses.binding("synthetic-model").unwrap(),
                text_input: Capability::Supported,
                image_input: Capability::Unsupported,
                streaming: Capability::Unsupported,
                tool_calling: Capability::Unsupported,
                cancellation: Capability::Supported,
                token_counting: TokenCounting::Estimated,
                context_window_tokens: Some(128000),
                max_output_tokens: Some(4096),
                verified_at: Some(env.ws.open().unwrap().vault.now().unwrap()),
            },
        }],
        quota: enouia_memory_provider::vault::Quota::default(),
        limits: enouia_memory_provider::client::Limits::default(),
    };
    env.ws.set_native_provider(Some(provider.clone())).unwrap();
    provider
}
fn local_args(env: &Env, import: &Value) -> Value {
    let plan = env.ok("history_plan", args(import));
    json!({"importId":import["importId"],"importRevision":import["revision"],
        "inputObjectHash":plan["identity"]["inputObjectHash"],"planHash":plan["planHash"],
        "previewCommitId":plan["identity"]["commitId"],"messagesPerBatch":2,"maxPathReferences":1000,
        "batchIndices":[1,0],"api":"openai","model":"synthetic-model","maxCandidates":2,
        "maxPending":100,"maxReservedTokens":10000,"maxReservedCostMicrousd":null})
}

fn batch_setup(env: &Env) -> (Value, Value, NativeProvider) {
    batch_setup_pending(env, 100)
}

fn batch_setup_pending(env: &Env, max_pending: u64) -> (Value, Value, NativeProvider) {
    let import = imported(env, RAW);
    let provider = local_model(env);
    let grant = env.ok(
        "egress_grant_plan",
        json!({"apis":["openai"],"maxSensitivity":"private","extraction":true}),
    );
    let policy = env.ok(
        "provider_confirm",
        json!({"planId":grant["planId"],"diffHash":grant["diffHash"]}),
    );
    let mut request = local_args(env, &import);
    request["maxPending"] = json!(max_pending);
    let task = local_keyed(env, &request, "synthetic-batch-task");
    let args = json!({"taskId":task["taskId"],"batchIndex":1,"policyId":policy["policyId"],"outputTokens":256});
    (task, args, provider)
}

#[test]
fn history_batch_prepare_reuses_saved_child_and_replays_keys_after_reopen_without_ports() {
    let env = Env::new("history-batch-prepare-replay");
    let (task, args, _) = batch_setup(&env);
    let original = task["batches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["batchIndex"] == 1)
        .unwrap();
    let open = env.ws.open().unwrap();
    let count = open
        .vault
        .record_entries(&open.vault.pin_current().unwrap(), RecordKind::Session)
        .unwrap()
        .len();
    let reply = env.send_keyed(
        "history_batch_prepare",
        args.clone(),
        "synthetic-prepare-key",
    );
    assert!(reply["error"].is_null(), "{reply}");
    let first = &reply["result"];
    assert_eq!(first["runId"], original["runId"]);
    assert_eq!(first["sessionId"], original["sessionId"]);
    assert_eq!(first["taskId"], task["taskId"]);
    assert_eq!(first["outputTokens"], 256);
    assert!(first["body"].as_str().unwrap().contains("source_index"));
    let head = open.vault.pin_current().unwrap();
    let replay = env.send_keyed(
        "history_batch_prepare",
        args.clone(),
        "synthetic-prepare-key",
    );
    assert!(replay["error"].is_null(), "{replay}");
    assert_eq!(replay["result"]["wireHash"], first["wireHash"]);
    assert_eq!(open.vault.pin_current().unwrap(), head);
    env.ws.shutdown();
    env.ws.open_root(&env.base.join("vault")).unwrap();
    local_model(&env);
    let reopened = env.send_keyed(
        "history_batch_prepare",
        args.clone(),
        "synthetic-prepare-key",
    );
    assert!(reopened["error"].is_null(), "{reopened}");
    assert_eq!(reopened["result"]["wireHash"], first["wireHash"]);
    assert_eq!(open.vault.pin_current().unwrap(), head);
    let mut changed = args.clone();
    changed["batchIndex"] = json!(0);
    let conflict = env.send_keyed("history_batch_prepare", changed, "synthetic-prepare-key");
    assert_eq!(conflict["error"]["code"], "idempotency_conflict");
    assert_eq!(
        open.vault
            .record_entries(&head, RecordKind::Session)
            .unwrap()
            .len(),
        count
    );
    for kind in [RecordKind::Candidate, RecordKind::Memory] {
        assert!(open.vault.record_entries(&head, kind).unwrap().is_empty());
    }
    let state = env.ok("history_task_get", json!({"taskId":task["taskId"]}));
    assert!(
        state["batches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["invocationState"].is_null())
    );
    assert!(!HostSurface::QuickSearch.allows(&json!({"command":"history_batch_prepare"})));
    assert_eq!(
        env.err(
            "external_send",
            json!({"callId":first["callId"],"wireHash":first["wireHash"]})
        )["code"],
        "not_found"
    );
}

#[test]
fn history_batch_prepare_refuses_unselected_paused_model_drift_source_policy_and_lock() {
    let env = Env::new("history-batch-prepare-guards");
    let (task, args, mut provider) = batch_setup(&env);
    let open = env.ws.open().unwrap();
    let head = open.vault.pin_current().unwrap();
    let mut bad = args.clone();
    bad["batchIndex"] = json!(100);
    assert_eq!(
        env.err("history_batch_prepare", bad)["code"],
        "invalid_request"
    );
    provider.destinations[0].capabilities.binding.model = "other-synthetic-model".into();
    env.ws.set_native_provider(Some(provider)).unwrap();
    assert_eq!(
        env.err("history_batch_prepare", args.clone())["code"],
        "revision_conflict"
    );
    assert_eq!(open.vault.pin_current().unwrap(), head);
    local_model(&env);
    let batch = task["batches"]
        .as_array()
        .unwrap()
        .iter()
        .find(|b| b["batchIndex"] == 1)
        .unwrap();
    env.ok(
        "extraction_pause",
        json!({"sessionId":batch["sessionId"],"runId":batch["runId"],"paused":true}),
    );
    assert_eq!(
        env.err("history_batch_prepare", args.clone())["code"],
        "permission_denied"
    );
    env.ok(
        "extraction_pause",
        json!({"sessionId":batch["sessionId"],"runId":batch["runId"],"paused":false}),
    );
    let policy = env.ws.policy(&open).unwrap().to_string();
    rewrite::<PolicyRecord>(&env, RecordKind::Policy, &policy, |p| {
        p["scopes"]
            .as_array_mut()
            .unwrap()
            .retain(|s| s != "source:read");
    });
    assert_eq!(
        env.err("history_batch_prepare", args.clone())["code"],
        "permission_denied"
    );
    env.ok("vault_lock", json!({}));
    assert_eq!(
        env.err("history_batch_prepare", args)["code"],
        "vault_locked"
    );
}

struct BatchPorts {
    reads: AtomicU64,
    calls: AtomicU64,
    text: Option<String>,
}
impl enouia_memory_contract::ports::SecretStore for BatchPorts {
    fn read(
        &self,
        _: &str,
    ) -> Result<enouia_memory_contract::ports::SecretBytes, enouia_memory_contract::MemoryError>
    {
        self.reads.fetch_add(1, Ordering::SeqCst);
        Ok(enouia_memory_contract::ports::SecretBytes::new(
            b"synthetic-key".to_vec(),
        ))
    }
}
impl enouia_memory_provider::transport::Transport for BatchPorts {
    fn exchange(
        &self,
        _: &enouia_memory_provider::codec::WireRequest,
        _: &enouia_memory_contract::ports::SecretBytes,
        _: &dyn enouia_memory_contract::foundation::Cancellation,
        _: Duration,
        sink: &mut dyn FnMut(&[u8]) -> Result<(), enouia_memory_contract::MemoryError>,
    ) -> Result<(), enouia_memory_contract::MemoryError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        sink(&serde_json::to_vec(&json!({"status":"completed","output":[{"type":"message","role":"assistant","content":[{"type":"output_text","text":self.text.as_deref().unwrap_or("{\"candidates\":[]}")}]}],"usage":{"input_tokens":50,"output_tokens":10}})).unwrap())
    }
}

#[test]
fn history_batch_prepare_only_sends_after_exact_confirmation_and_never_reopens_an_invocation() {
    let env = Env::new("history-batch-prepare-send");
    let (task, args, mut provider) = batch_setup(&env);
    let ports = Arc::new(BatchPorts {
        reads: AtomicU64::new(0),
        calls: AtomicU64::new(0),
        text: None,
    });
    provider.secrets = ports.clone();
    provider.transport = ports.clone();
    env.ws.set_native_provider(Some(provider)).unwrap();
    let prepared = env.ok("history_batch_prepare", args.clone());
    assert_eq!(ports.reads.load(Ordering::SeqCst), 0);
    assert_eq!(ports.calls.load(Ordering::SeqCst), 0);
    let wrong = env.err(
        "external_send",
        json!({"callId":prepared["callId"],"wireHash":sha256(b"wrong-body")}),
    );
    assert_eq!(wrong["code"], "permission_denied");
    assert_eq!(ports.calls.load(Ordering::SeqCst), 0);
    let done = env.wait(&env.ok(
        "external_send",
        json!({"callId":prepared["callId"],"wireHash":prepared["wireHash"]}),
    ));
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(ports.calls.load(Ordering::SeqCst), 1);
    let head = env.ws.open().unwrap().vault.pin_current().unwrap();
    assert_eq!(
        env.err("history_batch_prepare", args)["code"],
        "permission_denied"
    );
    assert_eq!(env.ws.open().unwrap().vault.pin_current().unwrap(), head);
    let state = env.ok("history_task_get", json!({"taskId":task["taskId"]}));
    assert_eq!(state["batches"][0]["invocationState"], "completed");
    assert_eq!(ports.calls.load(Ordering::SeqCst), 1);
}
fn local_keyed(env: &Env, args: &Value, key: &str) -> Value {
    let reply = env.send_keyed("history_materialize", args.clone(), key);
    assert!(reply["error"].is_null(), "{reply}");
    let done = env.wait(&reply["result"]);
    assert_eq!(done["state"], "succeeded", "{done}");
    done["result"].clone()
}
#[test]
fn local_tasks_replay_after_reopen_and_status_never_creates_or_sends() {
    let env = Env::new("history-local-replay");
    let import = imported(&env, RAW);
    local_model(&env);
    let args = local_args(&env, &import);
    let first = local_keyed(&env, &args, "synthetic-local-history-key");
    assert_eq!(first["materializationComplete"], true);
    assert_eq!(first["batches"][0]["batchIndex"], 1);
    assert_eq!(first["batches"][1]["batchIndex"], 0);
    assert!(
        first["batches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["state"] == "ready" && b["invocationState"].is_null() && b["applied"] == 0)
    );
    let open = env.ws.open().unwrap();
    let head = open.vault.pin_current().unwrap();
    assert_eq!(
        open.vault
            .record_entries(&head, RecordKind::Session)
            .unwrap()
            .len(),
        4
    );
    for kind in [
        RecordKind::Candidate,
        RecordKind::Memory,
        RecordKind::Dispatch,
    ] {
        assert!(open.vault.record_entries(&head, kind).unwrap().is_empty());
    }
    assert_eq!(
        local_keyed(&env, &args, "synthetic-local-history-key"),
        first
    );
    let list = env.ok("history_task_list", json!({"cursor":null,"limit":20}));
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    assert_eq!(list["items"][0]["taskId"], first["taskId"]);
    let detail = env.ok("history_task_get", json!({"taskId":first["taskId"]}));
    assert_eq!(detail, first);
    assert_eq!(open.vault.pin_current().unwrap(), head);
    let mut changed = args.clone();
    changed["batchIndices"] = json!([0, 1]);
    assert_eq!(
        env.send_keyed(
            "history_materialize",
            changed.clone(),
            "synthetic-local-history-key"
        )["error"]["code"],
        "idempotency_conflict"
    );
    assert_eq!(
        env.send_keyed(
            "history_materialize",
            args.clone(),
            "synthetic-fresh-stale-key"
        )["error"]["code"],
        "revision_conflict"
    );
    env.ws.shutdown();
    env.ws.open_root(&env.base.join("vault")).unwrap();
    env.ws.set_native_provider(None).unwrap();
    assert_eq!(
        local_keyed(&env, &args, "synthetic-local-history-key")["taskId"],
        first["taskId"]
    );
    assert_eq!(
        env.send_keyed(
            "history_materialize",
            changed,
            "synthetic-local-history-key"
        )["error"]["code"],
        "idempotency_conflict"
    );
    let resumed = env.wait(&env.ok("history_resume", json!({"taskId":first["taskId"]})));
    assert_eq!(resumed["state"], "succeeded", "{resumed}");
    assert_eq!(resumed["result"]["batches"], first["batches"]);
    assert_eq!(env.ws.open().unwrap().vault.pin_current().unwrap(), head);
    env.ok("vault_lock", json!({}));
    assert_eq!(
        env.err("history_task_get", json!({"taskId":first["taskId"]}))["code"],
        "vault_locked"
    );
    env.ok("vault_unlock", json!({}));
    assert_eq!(
        env.ok("history_task_get", json!({"taskId":first["taskId"]}))["batches"],
        first["batches"]
    );
}
#[test]
fn local_task_refusals_leave_no_admission_or_children() {
    let env = Env::new("history-local-refusals");
    let import = imported(&env, RAW);
    let args = local_args(&env, &import);
    let open = env.ws.open().unwrap();
    let before = open.vault.pin_current().unwrap();
    assert_eq!(
        env.err("history_materialize", args.clone())["code"],
        "provider_unavailable"
    );
    local_model(&env);
    for (field, value, code) in [
        ("model", json!("different-model"), "revision_conflict"),
        ("planHash", json!(sha256(b"other")), "revision_conflict"),
        ("batchIndices", json!([999]), "invalid_request"),
        ("importRevision", json!(99), "revision_conflict"),
    ] {
        let mut changed = args.clone();
        changed[field] = value;
        assert_eq!(
            env.err("history_materialize", changed)["code"],
            code,
            "{field}"
        );
        assert_eq!(open.vault.pin_current().unwrap(), before);
    }
    assert!(
        env.ok("history_task_list", json!({"cursor":null,"limit":20}))["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}

#[test]
fn admitted_task_without_coordinator_is_discovered_and_explicitly_resumed_once() {
    let env = Env::new("history-local-admission-only");
    let import = imported(&env, RAW);
    let args = local_args(&env, &import);
    let open = env.ws.open().unwrap();
    let key = b"workspace-history-admission:synthetic-admission-only-key";
    let admission: enouia_memory_contract::history::HistoryMaterialization=serde_json::from_value(json!({
        "import_id":args["importId"],"import_revision":args["importRevision"],
        "input_object_hash":args["inputObjectHash"],"preview_plan_hash":args["planHash"],
        "preview_commit_id":args["previewCommitId"],"messages_per_batch":2,"max_path_references":1000,
        "batch_indices":[1,0],"binding":{"provider":"openai","model":"synthetic-model","adapter_version":"text-http-1"},
        "subject_id":open.owner.actor_id.as_str().replacen("prn_","sub_",1),
        "max_candidates":2,"max_pending":100,"max_reserved_tokens":10000,
        "max_reserved_cost_microusd":null,"submission_key_hash":sha256(key)})).unwrap();
    let input = enouia_memory_vault::service::SessionStart {
        owner: open.owner.clone(),
        surface: ClientSurface::WindowsApp,
        sensitivity: Sensitivity::Private,
        policy_id: env.ws.policy(&open).unwrap(),
    };
    let id = session::start_history_materialization(&open.vault, &input, key, &admission)
        .unwrap()
        .id
        .0;
    let before = open.vault.pin_current().unwrap();
    env.ws.shutdown();
    env.ws.open_root(&env.base.join("vault")).unwrap();
    let list = env.ok("history_task_list", json!({"cursor":null,"limit":1}));
    assert_eq!(list["items"][0]["taskId"], json!(id));
    let partial = env.ok("history_task_get", json!({"taskId":id}));
    assert_eq!(partial["materializationComplete"], false);
    assert!(partial["coordinatorSessionId"].is_null());
    assert_eq!(partial["missingBatchIndices"], json!([1, 0]));
    assert_eq!(env.ws.open().unwrap().vault.pin_current().unwrap(), before);
    let done = env.wait(&env.ok("history_resume", json!({"taskId":id})));
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["result"]["materializationComplete"], true);
    let final_head = env.ws.open().unwrap().vault.pin_current().unwrap();
    let replay = local_keyed(&env, &args, "synthetic-admission-only-key");
    assert_eq!(replay["taskId"], json!(id));
    assert_eq!(replay["batches"], done["result"]["batches"]);
    assert_eq!(
        env.ws.open().unwrap().vault.pin_current().unwrap(),
        final_head
    );
}

#[test]
fn saved_local_task_status_and_resume_obey_current_source_policy() {
    let env = Env::new("history-local-current-policy");
    let import = imported(&env, RAW);
    local_model(&env);
    let first = local_keyed(
        &env,
        &local_args(&env, &import),
        "synthetic-current-policy-key",
    );
    let open = env.ws.open().unwrap();
    let policy = env.ws.policy(&open).unwrap().to_string();
    rewrite::<PolicyRecord>(&env, RecordKind::Policy, &policy, |value| {
        value["scopes"]
            .as_array_mut()
            .unwrap()
            .retain(|scope| scope != "source:read");
    });
    let denied_head = open.vault.pin_current().unwrap();
    assert_eq!(
        env.err("history_task_get", json!({"taskId":first["taskId"]}))["code"],
        "permission_denied"
    );
    let resumed = env.wait(&env.ok("history_resume", json!({"taskId":first["taskId"]})));
    assert_eq!(resumed["state"], "failed", "{resumed}");
    assert_eq!(resumed["error"]["code"], "permission_denied");
    assert_eq!(open.vault.pin_current().unwrap(), denied_head);
    assert_eq!(
        env.ok("history_task_list", json!({"cursor":null,"limit":20}))["items"][0]["taskId"],
        first["taskId"]
    );
}

// Produce saved synthetic responses only through the ordinary inspected-send
// path; all following preview/application runs with the native Provider removed.
fn saved_batch(env: &Env, candidates: bool) -> (Value, Value, Arc<BatchPorts>) {
    saved_batch_pending(env, candidates, 100)
}

fn saved_batch_pending(
    env: &Env,
    candidates: bool,
    max_pending: u64,
) -> (Value, Value, Arc<BatchPorts>) {
    let (task, args, mut provider) = batch_setup_pending(env, max_pending);
    let open = env.ws.open().unwrap();
    let id =
        enouia_memory_contract::ids::SessionId::parse(task["taskId"].as_str().unwrap()).unwrap();
    let batch = crate::materialization::selected_batch(&open, &id, 1).unwrap();
    let job = enouia_memory_provider::extraction::job(
        &open.vault,
        &open.owner,
        &batch.session_id,
        &batch.run_id,
    )
    .unwrap();
    let pin = open.vault.pin_current().unwrap();
    let first = &job.sources[0];
    let source: SourceRecord = parse(
        &open
            .vault
            .read_record(
                &pin,
                &RecordRef::new(
                    RecordKind::Source,
                    first.source.source_id.as_str(),
                    first.source.source_revision,
                ),
            )
            .unwrap(),
    )
    .unwrap();
    let text = enouia_memory_import::locate::resolve(RAW, &source.locator).unwrap();
    let quote = std::str::from_utf8(&text[first.start as usize..first.end as usize]).unwrap();
    let suggestions = if candidates {
        json!([
            {"kind":"fact","content":"合成候选一","quote":quote,"source_index":0,"claim_key":"synthetic_first"},
            {"kind":"fact","content":"合成候选二","quote":quote,"source_index":0,"claim_key":"synthetic_second"}
        ])
    } else {
        json!([])
    };
    let ports = Arc::new(BatchPorts {
        reads: AtomicU64::new(0),
        calls: AtomicU64::new(0),
        text: Some(json!({"candidates":suggestions}).to_string()),
    });
    provider.secrets = ports.clone();
    provider.transport = ports.clone();
    env.ws.set_native_provider(Some(provider)).unwrap();
    let prepared = env.ok("history_batch_prepare", args);
    let done = env.wait(&env.ok(
        "external_send",
        json!({"callId":prepared["callId"],"wireHash":prepared["wireHash"]}),
    ));
    assert_eq!(done["state"], "succeeded", "{done}");
    env.ws.set_native_provider(None).unwrap();
    let selected = json!({"taskId":task["taskId"],"batchIndex":1});
    (task, selected, ports)
}

#[test]
fn history_batch_application_binds_response_cursor_and_key_across_reopen_without_ports() {
    let env = Env::new("history-batch-apply-replay");
    let (task, selected, ports) = saved_batch(&env, true);
    let open = env.ws.open().unwrap();
    let head = open.vault.pin_current().unwrap();
    let preview = env.ok("history_batch_preview", selected.clone());
    assert_eq!(preview["items"].as_array().unwrap().len(), 2);
    assert_eq!(preview["items"][0]["application"], "awaiting_cursor");
    assert!(
        !preview["items"][0]["selectedText"]
            .as_str()
            .unwrap()
            .is_empty()
    );
    assert_eq!(open.vault.pin_current().unwrap(), head);
    let mut request = selected.clone();
    request["responseHash"] = preview["responseHash"].clone();
    request["cursor"] = json!(0);
    let applied = env.send_keyed(
        "history_batch_apply",
        request.clone(),
        "synthetic-apply-one",
    );
    assert!(applied["error"].is_null(), "{applied}");
    assert_eq!(applied["result"]["job"]["cursor"], 1);
    let head = open.vault.pin_current().unwrap();
    let replay = env.send_keyed(
        "history_batch_apply",
        request.clone(),
        "synthetic-apply-one",
    );
    assert_eq!(replay["result"], applied["result"], "{replay}");
    assert_eq!(open.vault.pin_current().unwrap(), head);
    env.ws.shutdown();
    env.ws.open_root(&env.base.join("vault")).unwrap();
    let replay = env.send_keyed(
        "history_batch_apply",
        request.clone(),
        "synthetic-apply-one",
    );
    assert_eq!(replay["result"], applied["result"], "{replay}");
    assert_eq!(open.vault.pin_current().unwrap(), head);
    // Even a different key with an old cursor cannot consume item two.
    let old = env.send_keyed(
        "history_batch_apply",
        request.clone(),
        "synthetic-apply-old",
    );
    assert_eq!(old["result"]["job"]["cursor"], 1, "{old}");
    request["cursor"] = json!(1);
    assert_eq!(
        env.send_keyed(
            "history_batch_apply",
            request.clone(),
            "synthetic-apply-one"
        )["error"]["code"],
        "idempotency_conflict"
    );
    let second = env.send_keyed(
        "history_batch_apply",
        request.clone(),
        "synthetic-apply-two",
    );
    assert_eq!(second["result"]["job"]["cursor"], 2, "{second}");
    assert_eq!(second["result"]["job"]["state"], "completed");
    let shown = env.ok("history_batch_preview", selected.clone());
    assert!(
        shown["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["candidate"]["status"] == "pending")
    );
    assert!(
        open.vault
            .record_entries(&open.vault.pin_current().unwrap(), RecordKind::Memory)
            .unwrap()
            .is_empty()
    );
    let status = env.ok("history_task_get", json!({"taskId":task["taskId"]}));
    assert_eq!(status["batches"][0]["applied"], 2);
    assert_eq!(ports.calls.load(Ordering::SeqCst), 1);
    assert_eq!(ports.reads.load(Ordering::SeqCst), 1);
    assert!(!HostSurface::QuickSearch.allows(&json!({"command":"history_batch_apply"})));
    assert!(!HostSurface::QuickSearch.allows(&json!({"command":"history_batch_preview"})));
}

#[test]
fn history_batch_application_refuses_changed_response_future_cursor_pause_policy_and_lock() {
    let env = Env::new("history-batch-apply-guards");
    let (_, selected, ports) = saved_batch(&env, true);
    let preview = env.ok("history_batch_preview", selected.clone());
    let mut request = selected.clone();
    request["responseHash"] = json!(sha256(b"wrong"));
    request["cursor"] = json!(0);
    let open = env.ws.open().unwrap();
    let head = open.vault.pin_current().unwrap();
    assert_eq!(
        env.err("history_batch_apply", request.clone())["code"],
        "revision_conflict"
    );
    request["responseHash"] = preview["responseHash"].clone();
    request["cursor"] = json!(1);
    assert_eq!(
        env.err("history_batch_apply", request.clone())["code"],
        "revision_conflict"
    );
    assert_eq!(open.vault.pin_current().unwrap(), head);
    request["cursor"] = json!(0);
    request["batchIndex"] = json!(100);
    assert_eq!(
        env.err("history_batch_apply", request.clone())["code"],
        "invalid_request"
    );
    request["batchIndex"] = json!(1);
    let pause = json!({"sessionId":preview["sessionId"],"runId":preview["runId"],"paused":true});
    env.ok("extraction_pause", pause.clone());
    assert_eq!(
        env.err("history_batch_apply", request.clone())["code"],
        "cancelled"
    );
    let mut resume = pause;
    resume["paused"] = json!(false);
    env.ok("extraction_pause", resume);
    let policy = env.ws.policy(&open).unwrap().to_string();
    rewrite::<PolicyRecord>(&env, RecordKind::Policy, &policy, |p| {
        p["scopes"]
            .as_array_mut()
            .unwrap()
            .retain(|s| s != "source:read")
    });
    assert_eq!(
        env.err("history_batch_preview", selected)["code"],
        "permission_denied"
    );
    assert_eq!(
        env.err("history_batch_apply", request.clone())["code"],
        "permission_denied"
    );
    env.ok("vault_lock", json!({}));
    assert_eq!(
        env.err("history_batch_apply", request)["code"],
        "vault_locked"
    );
    assert_eq!(ports.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn history_batch_application_completes_an_empty_saved_response_without_candidates() {
    let env = Env::new("history-batch-apply-empty");
    let (_, selected, ports) = saved_batch(&env, false);
    let preview = env.ok("history_batch_preview", selected.clone());
    assert!(preview["items"].as_array().unwrap().is_empty());
    let mut request = selected;
    request["responseHash"] = preview["responseHash"].clone();
    request["cursor"] = json!(0);
    let applied = env.send_keyed(
        "history_batch_apply",
        request.clone(),
        "synthetic-empty-apply",
    );
    assert_eq!(applied["result"]["job"]["state"], "completed", "{applied}");
    let open = env.ws.open().unwrap();
    let head = open.vault.pin_current().unwrap();
    let replay = env.send_keyed("history_batch_apply", request, "synthetic-empty-apply");
    assert_eq!(replay["result"], applied["result"], "{replay}");
    assert_eq!(open.vault.pin_current().unwrap(), head);
    assert!(
        open.vault
            .record_entries(&head, RecordKind::Candidate)
            .unwrap()
            .is_empty()
    );
    assert_eq!(ports.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn history_batch_application_recovers_admission_without_a_candidate_after_backlog_refusal() {
    let env = Env::new("history-batch-apply-admission-recovery");
    let (_, selected, ports) = saved_batch_pending(&env, true, 1);
    let blocker = env.ok(
        "remember",
        json!({"text":"合成待审占位","claimKey":"synthetic.backlog"}),
    );
    let preview = env.ok("history_batch_preview", selected.clone());
    let mut request = selected.clone();
    request["responseHash"] = preview["responseHash"].clone();
    request["cursor"] = json!(0);
    let refused = env.send_keyed(
        "history_batch_apply",
        request.clone(),
        "synthetic-admission-recovery",
    );
    assert_eq!(refused["error"]["code"], "budget_exceeded", "{refused}");
    let open = env.ws.open().unwrap();
    let receipt = open
        .vault
        .find_receipt(&IdempotencyScope {
            principal_id: open.owner.actor_id.clone(),
            operation_kind: OperationKind::SessionAppend,
            key_hash: sha256(b"history-batch-apply:synthetic-admission-recovery"),
        })
        .unwrap();
    assert!(
        receipt.is_some(),
        "the apply request is admitted before the backlog refusal"
    );
    assert_eq!(
        env.ok("history_batch_preview", selected)["job"]["cursor"],
        0
    );
    // Free the synthetic pending slot through the ordinary owner review flow.
    let plan = env.ok("review_plan", json!({"decisions":[{"candidateId":blocker["candidateId"],"revision":blocker["revision"],"action":"reject","editedContent":null,"mergeTarget":null}]}));
    env.ok(
        "review_confirm",
        json!({"planId":plan["planId"],"diffHash":plan["diffHash"]}),
    );
    env.ws.shutdown();
    env.ws.open_root(&env.base.join("vault")).unwrap();
    let recovered = env.send_keyed(
        "history_batch_apply",
        request,
        "synthetic-admission-recovery",
    );
    assert_eq!(recovered["result"]["job"]["cursor"], 1, "{recovered}");
    assert_eq!(
        recovered["result"]["job"]["candidates"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(ports.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn history_compare_is_read_only_and_preserves_originals_and_current_edits_without_ports() {
    let env = Env::new("history-compare");
    let (task, template, provider) = batch_setup(&env);
    let mut selected = Vec::new();
    let mut used_ports = Vec::new();
    for index in [0, 1] {
        let open = env.ws.open().unwrap();
        let task_id = serde_json::from_value(task["taskId"].clone()).unwrap();
        let batch = crate::materialization::selected_batch(&open, &task_id, index).unwrap();
        let job = enouia_memory_provider::extraction::job(
            &open.vault,
            &open.owner,
            &batch.session_id,
            &batch.run_id,
        )
        .unwrap();
        let first = &job.sources[0];
        let pin = open.vault.pin_current().unwrap();
        let source: SourceRecord = parse(
            &open
                .vault
                .read_record(
                    &pin,
                    &RecordRef::new(
                        RecordKind::Source,
                        first.source.source_id.as_str(),
                        first.source.source_revision,
                    ),
                )
                .unwrap(),
        )
        .unwrap();
        let text = enouia_memory_import::locate::resolve(RAW, &source.locator).unwrap();
        let quote = std::str::from_utf8(&text[first.start as usize..first.end as usize]).unwrap();
        let ports = Arc::new(BatchPorts {
            reads: AtomicU64::new(0),
            calls: AtomicU64::new(0),
            text: Some(
                json!({"candidates":[{"kind":"fact",
                "content":format!("Synthetic comparison statement {index}"),
                "quote":quote,"source_index":0,"claim_key":"synthetic_compare"}]})
                .to_string(),
            ),
        });
        let mut configured = provider.clone();
        configured.secrets = ports.clone();
        configured.transport = ports.clone();
        env.ws.set_native_provider(Some(configured)).unwrap();
        let mut args = template.clone();
        args["batchIndex"] = json!(index);
        let prepared = env.ok("history_batch_prepare", args);
        let done = env.wait(&env.ok(
            "external_send",
            json!({"callId":prepared["callId"],"wireHash":prepared["wireHash"]}),
        ));
        assert_eq!(done["state"], "succeeded", "{done}");
        selected.push(json!({"taskId":task["taskId"],"batchIndex":index}));
        used_ports.push(ports);
    }
    env.ws.set_native_provider(None).unwrap();
    let args = json!({"batches":selected});
    let open = env.ws.open().unwrap();
    let head = open.vault.pin_current().unwrap();
    let initial = env.ok("history_compare", args.clone());
    assert_eq!(open.vault.pin_current().unwrap(), head);
    assert_eq!(initial["batches"].as_array().unwrap().len(), 2);
    assert_eq!(initial["runs"].as_array().unwrap().len(), 2);
    assert_eq!(initial["topics"].as_array().unwrap().len(), 1);
    let topic = &initial["topics"][0];
    assert_eq!(topic["entries"].as_array().unwrap().len(), 2);
    assert_eq!(topic["comparisons"].as_array().unwrap().len(), 1);
    for (i, selected) in selected.iter().enumerate() {
        let preview = env.ok("history_batch_preview", selected.clone());
        assert_eq!(initial["batches"][i]["sessionId"], preview["sessionId"]);
        assert_eq!(initial["runs"][i]["response_hash"], preview["responseHash"]);
        let entry = topic["entries"]
            .as_array()
            .unwrap()
            .iter()
            .find(|e| e["sessionId"] == preview["sessionId"])
            .unwrap();
        for field in [
            "content",
            "quote",
            "selectedText",
            "occurredAt",
            "capturedAt",
            "speakerRole",
            "evidenceClass",
        ] {
            assert_eq!(entry[field], preview["items"][0][field], "{field}");
        }
    }
    assert!(
        open.vault
            .record_entries(&head, RecordKind::Candidate)
            .unwrap()
            .is_empty()
    );
    assert!(
        open.vault
            .record_entries(&head, RecordKind::Memory)
            .unwrap()
            .is_empty()
    );
    let preview = env.ok("history_batch_preview", selected[0].clone());
    let mut apply = selected[0].clone();
    apply["responseHash"] = preview["responseHash"].clone();
    apply["cursor"] = json!(0);
    let applied = env.ok("history_batch_apply", apply);
    let candidate = applied["job"]["candidates"][0].as_str().unwrap();
    rewrite::<enouia_memory_contract::candidate::CandidateRecord>(
        &env,
        RecordKind::Candidate,
        candidate,
        |c| {
            c["proposed_content"] = json!("Synthetic owner edited content");
            c["proposed_details"]["claim_key"] = json!("synthetic_owner_edited_key");
        },
    );
    let head = open.vault.pin_current().unwrap();
    let edited = env.ok("history_compare", args.clone());
    assert_eq!(open.vault.pin_current().unwrap(), head);
    let entry = edited["topics"][0]["entries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["candidate"].is_object())
        .unwrap();
    assert_eq!(entry["content"], "Synthetic comparison statement 0");
    assert_eq!(entry["details"]["claim_key"], "synthetic_compare");
    assert_eq!(
        entry["candidate"]["content"],
        "Synthetic owner edited content"
    );
    assert_eq!(
        entry["candidate"]["details"]["claim_key"],
        "synthetic_owner_edited_key"
    );
    let brief_args = json!({"batches": selected,
        "comparisonCommitId": edited["base"]["commitId"], "maxBytes": 131072});
    let brief = env.ok("history_brief", brief_args.clone());
    assert_eq!(brief["base"], edited["base"]);
    assert_eq!(brief["batches"], edited["batches"]);
    let markdown = brief["markdown"].as_str().unwrap();
    assert_eq!(brief["contentHash"], json!(sha256(markdown.as_bytes())));
    assert!(markdown.contains("Synthetic comparison statement 0"));
    assert!(markdown.contains("Synthetic owner edited content"));
    assert!(markdown.contains("uncited_span_occurrences"));
    assert_eq!(open.vault.pin_current().unwrap(), head);
    let mut small = brief_args.clone();
    small["maxBytes"] = json!(1);
    assert_eq!(env.err("history_brief", small)["code"], "budget_exceeded");
    let mut stale = brief_args.clone();
    stale["comparisonCommitId"] = initial["base"]["commitId"].clone();
    assert_eq!(env.err("history_brief", stale)["code"], "revision_conflict");
    assert_eq!(open.vault.pin_current().unwrap(), head);
    if let Some(path) = std::env::var_os("MEMORY_HISTORY_BRIEF_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(
                &json!({"comparison":edited,"args":brief_args,"response":brief}),
            )
            .unwrap(),
        )
        .unwrap();
    }
    if let Some(path) = std::env::var_os("MEMORY_HISTORY_COMPARE_FIXTURE") {
        std::fs::write(
            path,
            serde_json::to_vec_pretty(&json!({"batches":selected,"response":edited})).unwrap(),
        )
        .unwrap();
    }
    assert!(
        entry["flags"]
            .as_array()
            .unwrap()
            .contains(&json!("OwnerEdited"))
    );
    assert_eq!(entry["candidate"]["status"], "pending");
    assert!(
        open.vault
            .record_entries(&head, RecordKind::Memory)
            .unwrap()
            .is_empty()
    );
    env.ws.shutdown();
    env.ws.open_root(&env.base.join("vault")).unwrap();
    assert_eq!(env.ok("history_compare", args.clone()), edited);
    assert_eq!(
        env.err(
            "history_compare",
            json!({"batches":[{"taskId":task["taskId"],"batchIndex":999}]})
        )["code"],
        "invalid_request"
    );
    let open = env.ws.open().unwrap();
    let policy = env.ws.policy(&open).unwrap().to_string();
    rewrite::<PolicyRecord>(&env, RecordKind::Policy, &policy, |p| {
        p["scopes"]
            .as_array_mut()
            .unwrap()
            .retain(|s| s != "source:read");
    });
    assert_eq!(
        env.err("history_compare", args.clone())["code"],
        "permission_denied"
    );
    env.ok("vault_lock", json!({}));
    assert_eq!(env.err("history_compare", args)["code"], "vault_locked");
    assert_eq!(env.err("history_brief", brief_args)["code"], "vault_locked");
    assert!(!HostSurface::QuickSearch.allows(&json!({"command":"history_compare"})));
    for ports in used_ports {
        assert_eq!(ports.reads.load(Ordering::SeqCst), 1);
        assert_eq!(ports.calls.load(Ordering::SeqCst), 1);
    }
}

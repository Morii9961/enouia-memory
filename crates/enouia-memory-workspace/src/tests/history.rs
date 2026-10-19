//! Synthetic native-host consumption, not model or personal-history acceptance.
use super::*;
use crate::history::{BranchSelection, HistoryPlanRequest};
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::ids::{CommitId, ImportId, VaultId};
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::policy::{PolicyRecord, Scope};
use enouia_memory_contract::ports::{CommitRequest, StagedRecord};

const RAW: &[u8] =
    include_bytes!("../../../../tests/fixtures/history/initialization-synthetic.json");

fn import(env: &Env, raw: &[u8]) -> ImportManifest {
    let path = env.base.join("synthetic-history.json");
    std::fs::write(&path, raw).unwrap();
    let open = env.ws.open().unwrap();
    let options = env.ws.import_options(&open, "synthetic-history").unwrap();
    enouia_memory_import::import_file(
        &open.vault,
        &path,
        &options,
        &enouia_memory_import::pipeline::NeverCancel,
    )
    .unwrap()
    .manifest
}

fn request(env: &Env, manifest: &ImportManifest) -> HistoryPlanRequest {
    let open = env.ws.open().unwrap();
    HistoryPlanRequest {
        vault_id: open.vault.descriptor().vault_id.clone(),
        commit_id: open.vault.pin_current().unwrap().commit_id,
        import_id: manifest.import_id.clone(),
        import_revision: manifest.revision,
        unit_indices: (0..manifest.coverage.len() as u32).collect(),
        branches: BranchSelection::All,
        messages_per_batch: 2,
        max_path_references: 128,
    }
}

fn observe(env: &Env, request: &HistoryPlanRequest) -> Value {
    env.ws
        .plan_history(HostSurface::Workspace, request)
        .unwrap()
}

fn rewrite(env: &Env, kind: RecordKind, id: &str, revision: Revision, value: &Value) {
    let open = env.ws.open().unwrap();
    let key = canonical_bytes(value).unwrap();
    let operation = if kind == RecordKind::Policy {
        OperationKind::PolicyChange
    } else {
        OperationKind::Migration
    };
    open.vault
        .commit(CommitRequest {
            commit_id: CommitId::from_random(env.ws.config.ids.random_16()),
            expected_commit_id: Some(open.vault.pin_current().unwrap().commit_id),
            principal: open.owner.clone(),
            operation_kind: operation,
            idempotency: IdempotencyScope {
                principal_id: open.owner.actor_id.clone(),
                operation_kind: operation,
                key_hash: sha256(&key),
            },
            request_payload_hash: sha256(&key),
            expected_revisions: vec![(kind, id.to_owned(), Revision::new(revision.get() - 1))],
            records: vec![StagedRecord {
                record_kind: kind,
                record_id: id.to_owned(),
                revision,
                bytes: key,
            }],
            objects: vec![],
        })
        .unwrap();
}

#[test]
fn history_host_binds_archived_branches_unknowns_and_coverage_without_writing() {
    let env = Env::new("history-host-coverage");
    let manifest = import(&env, RAW);
    let request = request(&env, &manifest);
    let before = env
        .ws
        .open()
        .unwrap()
        .vault
        .read_manifest(&env.ws.open().unwrap().vault.pin_current().unwrap())
        .unwrap();
    let result = observe(&env, &request);
    assert_eq!(result["coverage"]["dated"], 6);
    assert_eq!(result["coverage"]["undated"], 1);
    assert_eq!(result["coverage"]["unparseable"], 1);
    assert_eq!(result["coverage"]["incompleteMessages"], 1);
    assert_eq!(result["coverage"]["unavailableAttachments"], 1);
    assert_eq!(result["coverage"]["earliest"], "2023-01-01T00:00:00.000Z");
    assert_eq!(result["messages"].as_array().unwrap().len(), 7);
    assert_eq!(result["batches"].as_array().unwrap().len(), 6);
    let messages = result["messages"].as_array().unwrap();
    let unknown = result["undated"][0].as_u64().unwrap() as usize;
    assert_eq!(messages[unknown]["occurredAt"], Value::Null);
    assert_eq!(messages[unknown]["timePrecision"], "unknown");
    let open = env.ws.open().unwrap();
    let pin = open.vault.pin_current().unwrap();
    let upstream = |index: &Value| {
        let row = &messages[index.as_u64().unwrap() as usize];
        let source: SourceRecord = parse(
            &open
                .vault
                .read_record(
                    &pin,
                    &RecordRef::new(RecordKind::Source, row["sourceId"].as_str().unwrap(), one()),
                )
                .unwrap(),
        )
        .unwrap();
        source.original_message_id.unwrap()
    };
    for batch in result["batches"].as_array().unwrap() {
        let ids: Vec<_> = batch["messages"]
            .as_array()
            .unwrap()
            .iter()
            .map(upstream)
            .collect();
        assert!(!(ids.contains(&"edit".to_owned()) && ids.contains(&"u2".to_owned())));
        if ids.contains(&"edit".to_owned()) || ids.contains(&"u2".to_owned()) {
            assert_eq!(upstream(&batch["preceding"]), "a1");
        }
    }
    env.tick();
    let repeated = observe(&env, &request);
    assert_eq!(result["planId"], repeated["planId"]);
    assert_eq!(result["batches"], repeated["batches"]);
    assert_eq!(result["commitId"], json!(pin.commit_id));
    let after = open.vault.read_manifest(&pin).unwrap();
    assert_eq!(
        canonical_bytes(&before).unwrap(),
        canonical_bytes(&after).unwrap()
    );
    assert_eq!(
        open.vault.pin_current().unwrap().commit_id,
        request.commit_id
    );
    assert_eq!(env.ok("operation_list", json!({}))["items"], json!([]));
    for private in [
        "Quiet Harbor",
        "file-service",
        "忽略审核",
        "synthetic-project",
        "synthetic-history.json",
    ] {
        assert!(!result.to_string().contains(private), "{private}");
    }
    assert!(open.vault.verify(&pin).unwrap().is_clean());
}

#[test]
fn history_host_selection_is_explicit_and_result_identity_changes_with_scope() {
    let env = Env::new("history-host-selection");
    let manifest = import(&env, RAW);
    let mut req = request(&env, &manifest);
    let all = observe(&env, &req);
    req.branches = BranchSelection::ExportSelected;
    let selected = observe(&env, &req);
    assert_eq!(selected["batches"].as_array().unwrap().len(), 3);
    assert!(
        selected["batches"]
            .as_array()
            .unwrap()
            .iter()
            .all(|b| b["selectedInExport"] == true)
    );
    assert_ne!(selected["planId"], all["planId"]);
    // Branch filtering affects batches; coverage still describes selected units.
    assert_eq!(selected["coverage"], all["coverage"]);
    req.unit_indices = vec![1, 3];
    let subset = observe(&env, &req);
    assert_eq!(subset["messages"].as_array().unwrap().len(), 2);
    assert!(
        subset["messages"]
            .as_array()
            .unwrap()
            .iter()
            .all(|m| m["unitIndex"] == 1)
    );
    assert_eq!(subset["coverage"]["unparseable"], 1);
    assert_eq!(subset["totalUnits"], 4);
}

#[test]
fn history_host_refuses_unknown_fields_bad_selection_budgets_and_wrong_bindings() {
    let env = Env::new("history-host-invalid");
    let manifest = import(&env, RAW);
    let original = request(&env, &manifest);
    let mut packet = serde_json::to_value(&original).unwrap();
    packet["surface"] = json!("workspace");
    assert!(serde_json::from_value::<HistoryPlanRequest>(packet).is_err());
    let mut cases = vec![];
    for units in [vec![], vec![0, 0], vec![1, 0], vec![4], (0..129).collect()] {
        let mut req = original.clone();
        req.unit_indices = units;
        cases.push(req);
    }
    for n in [0, 257] {
        let mut req = original.clone();
        req.messages_per_batch = n;
        cases.push(req);
    }
    for n in [0, 16_385] {
        let mut req = original.clone();
        req.max_path_references = n;
        cases.push(req);
    }
    for req in cases {
        assert_eq!(
            env.ws
                .plan_history(HostSurface::Workspace, &req)
                .unwrap_err()
                .code,
            MemoryErrorCode::InvalidRequest
        );
    }
    let mut req = original.clone();
    req.max_path_references = 1;
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::BudgetExceeded
    );
    let mut req = original.clone();
    req.vault_id = VaultId::from_random([42; 16]);
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::RevisionConflict
    );
    let mut req = original.clone();
    req.import_id = ImportId::from_random([42; 16]);
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::NotFound
    );
    let mut req = original.clone();
    req.import_revision = one();
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::NotFound
    );
    assert_eq!(
        env.ws
            .open()
            .unwrap()
            .vault
            .pin_current()
            .unwrap()
            .commit_id,
        original.commit_id
    );
}

#[test]
fn history_host_rejects_overlay_no_vault_locked_and_moved_head() {
    let env = Env::new("history-host-lifecycle");
    let manifest = import(&env, RAW);
    let req = request(&env, &manifest);
    assert_eq!(
        env.ws
            .plan_history(HostSurface::QuickSearch, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::PermissionDenied
    );
    let empty = Workspace::new(Config::default());
    assert_eq!(
        empty
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
    env.ok("vault_lock", json!({}));
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
    env.ok("vault_unlock", json!({}));
    observe(&env, &req);
    env.ok(
        "remember",
        json!({"text":"Synthetic changed head", "claimKey":"synthetic.changed_head"}),
    );
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::RevisionConflict
    );
    observe(&env, &request(&env, &manifest));
    env.ws.shutdown();
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &req)
            .unwrap_err()
            .code,
        MemoryErrorCode::VaultLocked
    );
}

#[test]
fn history_host_rechecks_source_read_policy_and_source_metadata() {
    let env = Env::new("history-host-policy");
    let manifest = import(&env, RAW);
    let req = request(&env, &manifest);
    let result = observe(&env, &req);
    let open = env.ws.open().unwrap();
    let pin = open.vault.pin_current().unwrap();
    let source_id = result["messages"][0]["sourceId"].as_str().unwrap();
    let mut source: Value = serde_json::from_slice(
        &open
            .vault
            .read_record(&pin, &RecordRef::new(RecordKind::Source, source_id, one()))
            .unwrap(),
    )
    .unwrap();
    let original_time = source["original_time"].clone();
    source["revision"] = json!(2);
    source["original_time"] = json!("synthetic changed time");
    rewrite(
        &env,
        RecordKind::Source,
        source_id,
        Revision::new(2).unwrap(),
        &source,
    );
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &request(&env, &manifest))
            .unwrap_err()
            .code,
        MemoryErrorCode::BrokenProvenance
    );
    // Restore matching metadata at a new revision; bindings must report rev 3.
    source["revision"] = json!(3);
    source["original_time"] = original_time;
    rewrite(
        &env,
        RecordKind::Source,
        source_id,
        Revision::new(3).unwrap(),
        &source,
    );
    assert_eq!(
        observe(&env, &request(&env, &manifest))["messages"][0]["sourceRevision"],
        3
    );
    let pin = open.vault.pin_current().unwrap();
    let mut policy: PolicyRecord = latest_records(&open.vault, &pin, RecordKind::Policy)
        .unwrap()
        .remove(0);
    policy.scopes.retain(|s| *s != Scope::SourceRead);
    policy.revision = Revision::new(2).unwrap();
    rewrite(
        &env,
        RecordKind::Policy,
        policy.policy_id.as_str(),
        policy.revision,
        &serde_json::to_value(&policy).unwrap(),
    );
    assert_eq!(
        env.ws
            .plan_history(HostSurface::Workspace, &request(&env, &manifest))
            .unwrap_err()
            .code,
        MemoryErrorCode::PermissionDenied
    );
}

#[test]
fn history_host_reports_bad_graph_without_inventing_batches() {
    let env = Env::new("history-host-bad-graph");
    let mut raw: Value = serde_json::from_slice(RAW).unwrap();
    raw[0]["mapping"]["u1"]["parent"] = json!("u2");
    let manifest = import(&env, &serde_json::to_vec(&raw).unwrap());
    let mut req = request(&env, &manifest);
    req.unit_indices = vec![0];
    let result = observe(&env, &req);
    assert_eq!(result["batches"], json!([]));
    assert_eq!(result["issues"][0]["kind"], "broken_parent_chain");
    assert_eq!(result["messages"].as_array().unwrap().len(), 4);
}

#[test]
fn history_host_checks_freshness_after_a_concurrent_source_change() {
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let paused = Arc::new(PausedClock {
        ready: ready_tx,
        release: Mutex::new(None),
    });
    let env = Env::with_clock("history-host-freshness", Some(paused.clone()));
    let manifest = import(&env, RAW);
    let req = request(&env, &manifest);
    let source_id = observe(&env, &req)["messages"][0]["sourceId"]
        .as_str()
        .unwrap()
        .to_owned();
    *paused.release.lock().unwrap() = Some(release_rx);
    std::thread::scope(|scope| {
        let read = scope.spawn(|| env.ws.plan_history(HostSurface::Workspace, &req));
        ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        let open = env.ws.open().unwrap();
        let pin = open.vault.pin_current().unwrap();
        let mut source: Value = serde_json::from_slice(
            &open
                .vault
                .read_record(&pin, &RecordRef::new(RecordKind::Source, &source_id, one()))
                .unwrap(),
        )
        .unwrap();
        source["revision"] = json!(2);
        rewrite(
            &env,
            RecordKind::Source,
            &source_id,
            Revision::new(2).unwrap(),
            &source,
        );
        release_tx.send(()).unwrap();
        assert_eq!(
            read.join().unwrap().unwrap_err().code,
            MemoryErrorCode::RevisionConflict
        );
    });
}

#[test]
fn history_host_distinguishes_memory_forgetting_from_source_purge() {
    use enouia_memory_contract::candidate::ProposedType;
    use enouia_memory_contract::commit::{DeleteMode, DeleteScope};
    for physical in [false, true] {
        let env = Env::new(if physical {
            "history-host-purge"
        } else {
            "history-host-logical"
        });
        let manifest = import(&env, RAW);
        let req = request(&env, &manifest);
        let result = observe(&env, &req);
        let source_id = enouia_memory_contract::ids::SourceId::parse(
            result["messages"][0]["sourceId"].as_str().unwrap(),
        )
        .unwrap();
        let open = env.ws.open().unwrap();
        let subject =
            SubjectId::parse(&open.owner.actor_id.as_str().replacen("prn_", "sub_", 1)).unwrap();
        let proposal = Proposal::create(
            ProposedType::Fact,
            "Synthetic selected history claim",
            json!({"claim_key":"synthetic.history_delete", "subject_ids":[subject]})
                .as_object()
                .unwrap()
                .clone(),
            vec![EvidenceSpec::content(source_id.clone(), one())],
        );
        let proposed = env
            .ws
            .proposed(&open, &proposal, b"synthetic-history-delete", &source_id)
            .unwrap();
        let plan = env.ok(
            "review_plan",
            json!({"decisions":[{
                "candidateId":proposed["candidateId"], "revision":proposed["revision"],
                "action":"accept", "editedContent":null, "mergeTarget":null
            }]}),
        );
        env.ok(
            "review_confirm",
            json!({"planId":plan["planId"],"diffHash":plan["diffHash"]}),
        );
        let memory = canonical_memories(
            &open.vault,
            &open.vault.pin_current().unwrap(),
            Canonical::AllStatuses,
        )
        .unwrap()
        .remove(0);
        let proposal = delete_proposal(
            &memory,
            if physical {
                DeleteMode::Purge
            } else {
                DeleteMode::LogicalDelete
            },
            DeleteScope::WithDependents,
        );
        let proposed = env
            .ws
            .proposed(
                &open,
                &proposal,
                b"synthetic-history-delete-confirm",
                &source_id,
            )
            .unwrap();
        let review = enouia_memory_govern::plan(
            &open.vault,
            &[Decision::Accept {
                candidate_id: CandidateId::parse(proposed["candidateId"].as_str().unwrap())
                    .unwrap(),
                revision: one(),
            }],
            &open.owner,
            SURFACE,
        )
        .unwrap();
        confirm(
            &open.vault,
            &review,
            &OwnerConfirmation {
                owner: open.owner.clone(),
                surface: SURFACE,
            },
        )
        .unwrap();
        if physical {
            let req = request(&env, &manifest);
            // The committed tombstone blocks planning even before files go.
            assert!(env.ws.plan_history(HostSurface::Workspace, &req).is_err());
            complete_purge(
                &open.vault,
                &review.ids[0].delete_id,
                &open.owner,
                b"synthetic-history-purge",
            )
            .unwrap();
        }
        let req = request(&env, &manifest);
        let before = open.vault.pin_current().unwrap().commit_id;
        let observation = env.ws.plan_history(HostSurface::Workspace, &req);
        if physical {
            assert!(observation.is_err());
        } else {
            assert_eq!(observation.unwrap()["coverage"], result["coverage"]);
        }
        assert_eq!(open.vault.pin_current().unwrap().commit_id, before);
    }
}

#[test]
fn history_host_result_budget_is_an_error_without_partial_observation() {
    let env = Env::new("history-host-output-budget");
    let mut mapping = Map::new();
    for i in 0..1000 {
        mapping.insert(format!("m{i:04}"), json!({"parent": if i == 0 { Value::Null } else { json!(format!("m{:04}", i-1)) },
            "message":{"id":format!("m{i:04}"), "author":{"role":"user"},
                "create_time":1_704_067_200+i, "content":{"parts":["Synthetic bounded observation"]}}}));
    }
    let manifest = import(
        &env,
        &serde_json::to_vec(
            &json!([{"id":"synthetic-output-budget", "mapping":mapping, "current_node":"m0999"}]),
        )
        .unwrap(),
    );
    let mut req = request(&env, &manifest);
    req.messages_per_batch = 256;
    req.max_path_references = 1024;
    let error = env
        .ws
        .plan_history(HostSurface::Workspace, &req)
        .unwrap_err();
    assert_eq!(error.code, MemoryErrorCode::BudgetExceeded);
    assert_eq!(error.rules, ["history.plan_budget"]);
    assert_eq!(
        env.ws
            .open()
            .unwrap()
            .vault
            .pin_current()
            .unwrap()
            .commit_id,
        req.commit_id
    );
}

#[test]
fn history_host_lifecycle_gate_excludes_lock_and_switch_until_observation_returns() {
    for action in ["lock", "switch", "shutdown"] {
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let paused = Arc::new(PausedClock {
            ready: ready_tx,
            release: Mutex::new(None),
        });
        let env = Env::with_clock("history-host-lifecycle-gate", Some(paused.clone()));
        let manifest = import(&env, RAW);
        let req = request(&env, &manifest);
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        *paused.release.lock().unwrap() = Some(release_rx);
        std::thread::scope(|scope| {
            let read = scope.spawn(|| env.ws.plan_history(HostSurface::Workspace, &req));
            ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            let close = scope.spawn(|| {
                close_for_test(&env, action);
                done_tx.send(()).unwrap();
            });
            let closed_early = done_rx.recv_timeout(Duration::from_millis(150)).is_ok();
            release_tx.send(()).unwrap();
            let result = read.join().unwrap().unwrap();
            close.join().unwrap();
            assert!(!closed_early, "{action} passed an active observation");
            assert_eq!(result["commitId"], json!(req.commit_id));
        });
        assert!(env.ws.plan_history(HostSurface::Workspace, &req).is_err());
    }
}

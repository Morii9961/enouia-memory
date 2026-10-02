//! The workspace Core through its page channel: every request is the JSON
//! the frontend sends, every response is checked against the workspace IPC
//! contract. Synthetic data in a temporary root only.

use super::*;
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::workspace::validate_response;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNTER: AtomicU64 = AtomicU64::new(0);
const T0: i64 = 1_790_000_000_000;

struct Env {
    base: PathBuf,
    clock: Arc<FakeClock>,
    ws: Workspace,
    n: AtomicU64,
}

impl Drop for Env {
    fn drop(&mut self) {
        self.ws.shutdown();
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl Env {
    fn new(name: &str) -> Self {
        let tmp = std::env::temp_dir().join("enouia-memory-workspace-tests");
        std::fs::create_dir_all(&tmp).unwrap();
        let tmp = std::fs::canonicalize(&tmp).unwrap();
        let tmp = PathBuf::from(tmp.to_string_lossy().trim_start_matches(r"\\?\"));
        let base = tmp.join(format!(
            "{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("vault")).unwrap();
        let clock = Arc::new(FakeClock::new(T0));
        let ws = Workspace::new(Config {
            clock: clock.clone(),
            ids: Arc::new(SequentialIdSource::new(0xb000)),
        });
        let env = Self {
            base,
            clock,
            ws,
            n: AtomicU64::new(0),
        };
        let token = env.pick(PickKind::VaultRoot, "vault");
        let status = env.ok(
            "vault_create",
            json!({"rootToken": token, "confirmPhrase": "create new vault"}),
        );
        assert_eq!(status["vault"]["state"], "open");
        env
    }

    fn pick(&self, kind: PickKind, relative: &str) -> String {
        self.ws
            .register_pick(kind, &self.base.join(relative))
            .unwrap()
            .token
    }

    fn tick(&self) {
        self.clock.set(self.clock.now_unix_ms() + 1_000);
    }

    /// Send one request as the page would and check the envelope.
    fn send(&self, command: &str, arguments: Value) -> Value {
        self.tick();
        let n = self.n.fetch_add(1, Ordering::SeqCst);
        let key = wire::is_write(command).then(|| format!("test-key-{n:016}"));
        let request = json!({
            "schemaVersion": 1,
            "requestId": format!("req_00000000-0000-4000-8000-{n:012x}"),
            "command": command, "idempotencyKey": key, "arguments": arguments,
        });
        let response = self.ws.call(&request);
        validate_response(command, &response)
            .unwrap_or_else(|e| panic!("{command}: {e}: {response}"));
        let text = response.to_string();
        let root = self.base.to_string_lossy().replace('\\', "\\\\");
        assert!(!text.contains(&root), "{command} leaked a path: {text}");
        response
    }

    fn ok(&self, command: &str, arguments: Value) -> Value {
        let response = self.send(command, arguments);
        assert_eq!(response["error"], Value::Null, "{command}: {response}");
        response["result"].clone()
    }

    fn err(&self, command: &str, arguments: Value) -> Value {
        let response = self.send(command, arguments);
        assert_eq!(response["kind"], "memory_error", "{command}: {response}");
        response["error"].clone()
    }

    fn wait(&self, started: &Value) -> Value {
        let id = OperationId::parse(started["operationId"].as_str().unwrap()).unwrap();
        self.ws
            .operations()
            .wait(&id, Duration::from_secs(60))
            .unwrap()
    }

    /// Owner statement -> pending candidate -> plan -> confirmed memory.
    fn remember(&self, text: &str, claim: &str) -> String {
        let proposed = self.ok("remember", json!({"text": text, "claimKey": claim}));
        assert_eq!(proposed["state"], "pending");
        let plan = self.ok(
            "review_plan",
            json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
        );
        self.ok(
            "review_confirm",
            json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
        );
        let list = self.ok(
            "memory_list",
            json!({"includeInactive": false, "cursor": null, "limit": 100}),
        );
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["snippet"] == text)
            .unwrap()["memoryId"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

#[test]
fn w01_import_review_ask_inspect_correct_and_resume() {
    let env = Env::new("w01");
    // Import: the page names a token, never a path.
    std::fs::write(
        env.base.join("notes.md"),
        "# MoriMeta\n\n设计决定：采用 Professional Darkroom 风格。\n",
    )
    .unwrap();
    let token = env.pick(PickKind::ImportFile, "notes.md");
    let preview = env.ok("import_preview", json!({"importToken": token}));
    assert_eq!(preview["displayName"], "notes.md");
    assert_eq!(preview["recognized"], true);
    let started = env.ok(
        "import_start",
        json!({"importToken": token, "accountAlias": "acct-main"}),
    );
    let done = env.wait(&started);
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["result"]["status"], "completed", "{done}");
    // The token was single-use.
    let again = env.err(
        "import_start",
        json!({"importToken": token, "accountAlias": "acct-main"}),
    );
    assert_eq!(again["rules"][0], "workspace.token_unknown");
    let imports = env.ok("import_list", json!({}));
    assert_eq!(imports["items"].as_array().unwrap().len(), 1);

    // Review: a pending candidate becomes a memory only through a plan
    // confirmed with the exact diff hash.
    let proposed = env.ok(
        "remember",
        json!({"text": "MoriMeta 的设计决定是 Professional Darkroom。", "claimKey": "project.morimeta.design"}),
    );
    let page = env.ok("candidate_list", json!({"cursor": null, "limit": null}));
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["candidateId"], proposed["candidateId"]);
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(listed["total"], 0, "a candidate is not a memory");
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    assert_eq!(plan["confirmCode"].as_str().unwrap().len(), 8);
    let wrong = env.err(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": "0".repeat(64)}),
    );
    assert_eq!(wrong["code"], "revision_conflict");
    assert_eq!(
        env.ok(
            "memory_list",
            json!({"includeInactive": false, "cursor": null, "limit": null})
        )["total"],
        0,
        "a wrong hash writes nothing"
    );
    let confirmed = env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    let replay = env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    assert_eq!(confirmed, replay, "a retried confirm answers the same");
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    let memory_id = listed["items"][0]["memoryId"].as_str().unwrap().to_owned();
    assert_eq!(listed["items"][0]["status"], "active");

    // Look up and open the source.
    let found = env.ok(
        "memory_search",
        json!({"query": "MoriMeta", "includeHistorical": false, "cursor": null, "limit": null}),
    );
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["memoryId"] == json!(memory_id))
    );
    let detail = env.ok("memory_read", json!({"memoryId": memory_id}));
    let evidence = &detail["evidence"][0];
    assert_eq!(evidence["available"], true);
    let excerpt = env.ok(
        "source_excerpt",
        json!({"sourceId": evidence["sourceId"], "sourceRevision": evidence["sourceRevision"], "startByte": null, "maxBytes": 8192}),
    );
    assert_eq!(
        excerpt["excerpt"],
        "MoriMeta 的设计决定是 Professional Darkroom。"
    );
    assert_eq!(excerpt["untrusted"], true);

    // Ask in a session: input saved, context compiled, local Mock answers
    // with sources; the inspector shows the saved capsule and the request.
    let created = env.ok("session_new", json!({}));
    let (sid, bid) = (created["sessionId"].clone(), created["branchId"].clone());
    let turn = env.ok(
        "session_ask",
        json!({"sessionId": sid, "branchId": bid, "text": "MoriMeta 设计决定"}),
    );
    assert_eq!(turn["destination"], "local_mock");
    assert!(!turn["sources"].as_array().unwrap().is_empty(), "{turn}");
    let inspected = env.ok("context_inspect", json!({"capsuleId": turn["capsuleId"]}));
    assert_eq!(inspected["delivery"], "dispatched");
    assert!(
        inspected["inspection"]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["record_id"] == json!(memory_id) && d["decision"] == "included")
    );
    let request = env.ok(
        "dispatch_inspect",
        json!({"dispatchId": turn["dispatchId"]}),
    );
    assert_eq!(request["verified"], true);
    assert_eq!(request["tools"], 0);
    let preview = env.ok(
        "context_preview",
        json!({"query": "MoriMeta", "sessionId": null, "branchId": null}),
    );
    let previewed = env.ok(
        "context_inspect",
        json!({"capsuleId": preview["capsuleId"]}),
    );
    assert_eq!(previewed["delivery"], "preview_not_sent");

    // Correct: a revision is proposed, reviewed, and then current.
    let fix = env.ok(
        "correction_propose",
        json!({"memoryId": memory_id, "revision": 1, "text": "MoriMeta 的设计决定是 Darkroom 2。"}),
    );
    let candidates = env.ok("candidate_list", json!({"cursor": null, "limit": null}));
    let row = &candidates["items"][0];
    assert_eq!(row["proposalKind"], "revise");
    assert_eq!(
        row["target"]["content"],
        "MoriMeta 的设计决定是 Professional Darkroom。"
    );
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": fix["candidateId"], "revision": fix["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    let detail = env.ok("memory_read", json!({"memoryId": memory_id}));
    assert_eq!(detail["record"]["revision"], 2);
    assert_eq!(
        detail["record"]["content"],
        "MoriMeta 的设计决定是 Darkroom 2。"
    );

    // Resume: a checkpoint, then a restart of the Core keeps everything.
    env.ok(
        "session_checkpoint",
        json!({"sessionId": sid, "branchId": bid, "summary": "讨论了 MoriMeta 的设计决定。"}),
    );
    env.ws.shutdown();
    let token = env.pick(PickKind::VaultRoot, "vault");
    env.ok("vault_open", json!({"rootToken": token}));
    let detail = env.ok("session_detail", json!({"sessionId": sid, "branchId": bid}));
    assert_eq!(detail["turns"][0]["state"], "completed");
    assert_eq!(detail["checkpoints"][0]["status"], "provisional");
    let sessions = env.ok("session_list", json!({}));
    assert_eq!(sessions["items"][0]["sessionId"], sid);
}

#[test]
fn w02_long_work_cancel_paging_and_index_busy() {
    let env = Env::new("w02");
    for i in 0..5 {
        env.remember(&format!("合成事实 {i}"), &format!("synthetic.fact_{i}"));
    }
    // Paging: two pages of two and one; a later commit makes a cursor stale.
    let first = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": 2}),
    );
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    let second = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": first["nextCursor"], "limit": 2}),
    );
    assert_eq!(second["items"].as_array().unwrap().len(), 2);
    assert_ne!(first["items"][0], second["items"][0]);
    env.remember("合成事实 5", "synthetic.fact_5");
    let stale = env.err(
        "memory_list",
        json!({"includeInactive": false, "cursor": second["nextCursor"], "limit": 2}),
    );
    assert_eq!(stale["rules"][0], "workspace.cursor_stale");

    // While the index is held (a rebuild), search answers at once.
    let open = env.ws.open().unwrap();
    {
        let _held = open.index.lock().unwrap();
        let busy = env.err(
            "memory_search",
            json!({"query": "合成", "includeHistorical": false, "cursor": null, "limit": null}),
        );
        assert_eq!(busy["code"], "index_not_ready");
        assert_eq!(busy["rules"][0], "index.busy");
        let status = env.ok("workspace_status", json!({}));
        let index = status["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["component"] == "memory_index")
            .unwrap();
        assert_eq!(index["state"], "recovering");
        assert_eq!(status["vault"]["health"], "healthy");
    }
    // A rebuild runs off the calling thread and reaches the head.
    let started = env.ok("index_rebuild", json!({}));
    let done = env.wait(&started);
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["result"]["reachedHead"], true);
    assert!(done["progress"]["done"].as_u64().unwrap() > 0);
    let found = env.ok(
        "memory_search",
        json!({"query": "合成", "includeHistorical": false, "cursor": null, "limit": null}),
    );
    assert_eq!(found["items"].as_array().unwrap().len(), 6);

    // Cancellation is visible and never reported as success.
    let id = OperationId::from_random([7; 16]);
    env.ws
        .operations()
        .spawn(id.clone(), "test_wait", |ticket| {
            while !ticket.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(WorkspaceError::new(
                MemoryErrorCode::Cancelled,
                &["test.cancelled"],
            ))
        });
    let cancelled = env.ok("operation_cancel", json!({"operationId": id}));
    assert_eq!(cancelled["cancelRequested"], true);
    let finished = env
        .ws
        .operations()
        .wait(&id, Duration::from_secs(10))
        .unwrap();
    assert_eq!(finished["state"], "cancelled");
    let unknown = env.err(
        "operation_get",
        json!({"operationId": OperationId::from_random([9; 16])}),
    );
    assert_eq!(unknown["code"], "not_found");

    // Vault verification and backup export run as operations; the export
    // previews as the same Vault.
    let verified = env.wait(&env.ok("vault_verify", json!({})));
    assert_eq!(verified["result"]["clean"], true, "{verified}");
    std::fs::create_dir_all(env.base.join("backup")).unwrap();
    let token = env.pick(PickKind::BackupDestination, "backup");
    let exported = env.wait(&env.ok("backup_export", json!({"destinationToken": token})));
    assert_eq!(exported["state"], "succeeded", "{exported}");
    let status = env.ok("workspace_status", json!({}));
    assert_eq!(status["lastBackup"]["state"], "succeeded");
    let token = env.pick(PickKind::ExportFolder, "backup");
    let restore = env.ok("restore_preview", json!({"exportToken": token}));
    assert_eq!(restore["sameVaultAsOpen"], true);
    assert_eq!(restore["commitId"], exported["result"]["commitId"]);
}

#[test]
fn w03_lock_unlock_and_shutdown() {
    let env = Env::new("w03");
    env.remember("锁定前保存的事实", "synthetic.before_lock");
    let locked = env.ok("vault_lock", json!({}));
    assert_eq!(locked["vault"]["state"], "locked");
    let refused = env.err(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(refused["code"], "vault_locked");
    let refused = env.err(
        "remember",
        json!({"text": "不会保存", "claimKey": "synthetic.locked"}),
    );
    assert_eq!(refused["code"], "vault_locked");
    let status = env.ok("workspace_status", json!({}));
    let activity = status["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["component"] == "activity")
        .unwrap();
    assert_eq!(activity["mode"], "independent_not_managed");
    let unlocked = env.ok("vault_unlock", json!({}));
    assert_eq!(unlocked["vault"]["state"], "open");
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(listed["total"], 1);
    // A plan made before a lock cannot be confirmed after it.
    let proposed = env.ok(
        "remember",
        json!({"text": "计划中的事实", "claimKey": "synthetic.planned"}),
    );
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    env.ok("vault_lock", json!({}));
    env.ok("vault_unlock", json!({}));
    let gone = env.err(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    assert_eq!(gone["rules"][0], "workspace.plan_unknown");
    env.ws.shutdown();
    let none = env.err(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(none["rules"][0], "workspace.no_vault");
}

#[test]
fn w04_page_cannot_name_paths_commands_or_identities() {
    let env = Env::new("w04");
    let root = env.base.join("vault").to_string_lossy().into_owned();
    let cases = [
        json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "vault_open", "idempotencyKey": null, "arguments": {"rootToken": root}}),
        json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "run_shell", "idempotencyKey": null, "arguments": {"cmd": "dir"}}),
        json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "memory_list", "idempotencyKey": null, "arguments": {"includeInactive": false, "cursor": null, "limit": null, "principal": "prn_x"}}),
        json!({"schemaVersion": 2, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "workspace_status", "idempotencyKey": null, "arguments": {}}),
        json!("not an object"),
    ];
    for case in cases {
        let response = env.ws.call(&case);
        assert_eq!(response["kind"], "memory_error", "{case}");
        assert!(!response.to_string().contains(&root));
    }
    // A token of another kind is refused; an import token cannot open a Vault.
    std::fs::write(env.base.join("a.md"), "# a\n").unwrap();
    let token = env.pick(PickKind::ImportFile, "a.md");
    let wrong = env.err("vault_open", json!({"rootToken": token}));
    assert_eq!(wrong["rules"][0], "workspace.token_kind");
    // Source text is returned as data, marked untrusted; scripts stay text.
    let id = env.remember(
        "<script>alert(1)</script><img src=\"https://example.invalid/x.png\">",
        "synthetic.injection",
    );
    let detail = env.ok("memory_read", json!({"memoryId": id}));
    let excerpt = env.ok(
        "source_excerpt",
        json!({"sourceId": detail["evidence"][0]["sourceId"], "sourceRevision": 1, "startByte": null, "maxBytes": 16}),
    );
    assert_eq!(excerpt["truncated"], true);
    assert_eq!(excerpt["untrusted"], true);
    assert_eq!(excerpt["byteEnd"], 16);
}

#[test]
fn forget_and_purge_go_through_a_plan() {
    let env = Env::new("forget");
    let keep = env.remember("保留的事实", "synthetic.keep");
    let gone = env.remember("要忘记的事实", "synthetic.forget");
    let purged = env.remember("要彻底删除的事实", "synthetic.purge");
    let impact = env.ok(
        "delete_preview",
        json!({"memoryId": purged, "withDependents": false}),
    );
    assert!(!impact["targets"].as_array().unwrap().is_empty());
    for (id, mode) in [(&gone, "forget"), (&purged, "purge")] {
        let plan = env.ok(
            "forget_plan",
            json!({"memoryId": id, "mode": mode, "withDependents": false}),
        );
        assert_eq!(plan["purge"], mode == "purge");
        let done = env.ok(
            "review_confirm",
            json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
        );
        assert_eq!(done["purge"].is_object(), mode == "purge", "{done}");
    }
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": true, "cursor": null, "limit": null}),
    );
    let ids: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["memoryId"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![keep.as_str()]);
    let missing = env.err("memory_read", json!({"memoryId": gone}));
    assert_eq!(missing["code"], "not_found");
    let verified = env.wait(&env.ok("vault_verify", json!({})));
    assert_eq!(verified["result"]["clean"], true, "{verified}");
}

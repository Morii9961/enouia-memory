//! Actual in-process Host, governance, policies and audit over fresh synthetic roots.
use super::*;
use crate::mcp::{Grant, ReadPort, Server};
use enouia_memory_contract::candidate::ProposedType;
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::foundation::{Clock, FakeClock};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::*;
use enouia_memory_contract::json::{Revision, SchemaVersion, canonical_bytes};
use enouia_memory_contract::policy::{PolicyRecord, Scope};
use enouia_memory_contract::ports::{
    CommitRequest, IdempotencyScope, SequentialIdSource, StagedRecord,
};
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_govern::{
    Decision, EvidenceSpec, Origin, OwnerConfirmation, Proposal, Proposed, confirm, plan, propose,
};
use enouia_memory_vault::host_transaction::HostCoordinator;
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{RootPolicy, verify_data_root};
use std::sync::atomic::{AtomicU64, Ordering};

const NOW: i64 = 1_790_000_000_000;
static ROOTS: AtomicU64 = AtomicU64::new(0);
fn actor(kind: ActorType) -> ActorRef {
    ActorRef {
        actor_type: kind,
        actor_id: PrincipalId::parse(if kind == ActorType::Owner {
            "prn_00000001-0000-4000-8000-000000000001"
        } else {
            "prn_00000002-0000-4000-8000-000000000002"
        })
        .unwrap(),
    }
}
fn owner() -> ActorRef {
    actor(ActorType::Owner)
}
fn agent() -> ActorRef {
    actor(ActorType::Agent)
}
fn device() -> DeviceId {
    DeviceId::parse("dev_00000001-0000-4000-8000-000000000001").unwrap()
}

struct Env {
    base: PathBuf,
    root: PathBuf,
    clock: Arc<FakeClock>,
    config: Config,
    vault: Vault,
    memory: MemoryId,
    source: SourceId,
}
impl Env {
    fn new() -> Self {
        let base = std::env::temp_dir().join("enouia-agent-read-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let root = base.join(format!(
            "reads-{}-{}",
            std::process::id(),
            ROOTS.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&root).unwrap();
        let verified = verify_data_root(&root, &RootPolicy::default()).unwrap();
        let clock = Arc::new(FakeClock::new(NOW));
        let config = Config {
            clock: clock.clone(),
            ids: Arc::new(SequentialIdSource::new(0xf000)),
        };
        let genesis = new_genesis(
            config.ids.as_ref(),
            owner(),
            TrustedSurface::TrustedWindowsApp,
            &Timestamp::from_unix_ms(NOW).unwrap(),
        )
        .unwrap();
        let vault = Vault::create(
            &verified,
            genesis,
            config.clock.clone(),
            config.ids.clone(),
            VaultOptions::default(),
        )
        .unwrap();
        enouia_memory_vault::platform::restrict_to_owner(verified.path()).unwrap();
        let pin = vault.pin_current().unwrap();
        let entry = vault
            .record_entries(&pin, RecordKind::Policy)
            .unwrap()
            .remove(0);
        let source = vault
            .record_manual_assertion(
                &ManualAssertionInput {
                    text: "synthetic readable 中文 evidence".into(),
                    operator: owner(),
                    trusted_surface: TrustedSurface::TrustedWindowsApp,
                    confirmation: ConfirmationMethod::TypedConfirmation,
                    sensitivity: Sensitivity::Private,
                    access_policy_id: PolicyId::parse(&entry.record_id).unwrap(),
                    time_precision: TimePrecision::Millisecond,
                },
                b"synthetic-source",
            )
            .unwrap()
            .id;
        let proposal = Proposal::create(ProposedType::Fact, "synthetic readable 中文 evidence",
            serde_json::from_value(json!({"claim_key":"agent.readable", "subject_ids":["sub_00000001-0000-4000-8000-000000000001"],"new_project":{"display_name":"synthetic agent project","aliases":[]}})).unwrap(),
            vec![EvidenceSpec::content(source.clone(), Revision::new(1).unwrap())]);
        let Proposed::Stored(candidate) = propose(
            &vault,
            &proposal,
            &Origin::owner(owner()),
            b"synthetic-candidate",
        )
        .unwrap() else {
            panic!("stored")
        };
        let shown = plan(
            &vault,
            &[Decision::Accept {
                candidate_id: candidate.id,
                revision: Revision::new(1).unwrap(),
            }],
            &owner(),
            TrustedSurface::TrustedWindowsApp,
        )
        .unwrap();
        confirm(
            &vault,
            &shown,
            &OwnerConfirmation {
                owner: owner(),
                surface: TrustedSurface::TrustedWindowsApp,
            },
        )
        .unwrap();
        let memory = shown.ids[0].memory_id.clone();
        Self {
            base,
            root,
            clock,
            config,
            vault,
            memory,
            source,
        }
    }
    fn host(&self) -> HostService {
        let verified = verify_data_root(&self.root, &RootPolicy::default()).unwrap();
        drop(HostCoordinator::bootstrap(&self.vault, &verified, device()).unwrap());
        HostService::open(&verified, device(), self.config.clone()).unwrap()
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        // Delete only this test's already resolved direct child of its fixed
        // synthetic temporary base. Never enumerate or adopt an input root.
        if self.root.parent() == Some(self.base.as_path()) {
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }
}
fn grant(scopes: Vec<Scope>) -> Grant {
    Grant::from_authenticated_agent(agent(), scopes).unwrap()
}

fn policy(
    vault: &Vault,
    scopes: &[Scope],
    max: Sensitivity,
    revision: u64,
    revoked: bool,
) -> PolicyId {
    project_policy(vault, scopes, max, revision, revoked, None)
}
fn project_policy(
    vault: &Vault,
    scopes: &[Scope],
    max: Sensitivity,
    revision: u64,
    revoked: bool,
    project: Option<ProjectId>,
) -> PolicyId {
    let pin = vault.pin_current().unwrap();
    let entry = vault
        .record_entries(&pin, RecordKind::Policy)
        .unwrap()
        .into_iter()
        .find(|e| e.revision.get() == 1)
        .unwrap();
    let mut p: Value = serde_json::from_slice(
        &vault
            .read_record(
                &pin,
                &RecordRef::new(RecordKind::Policy, &entry.record_id, entry.revision),
            )
            .unwrap(),
    )
    .unwrap();
    let id = PolicyId::parse("pol_000000aa-0000-4000-8000-000000000001").unwrap();
    let approval = ApprovalId::from_random(vault.random_id_bytes());
    let now = vault.now().unwrap();
    p["policy_id"] = json!(id);
    p["revision"] = json!(revision);
    p["origin"] = json!("owner_grant");
    p["approval_id"] = json!(approval);
    p["principals"] = json!([{"actor_type":"agent","actor_id":agent().actor_id}]);
    p["scopes"] = json!(scopes);
    p["resources"]["record_kinds"] = json!(["memory", "source", "attachment"]);
    p["resources"]["max_sensitivity"] = json!(max);
    p["resources"]["all_projects"] = json!(project.is_none());
    p["resources"]["project_ids"] = json!(project.into_iter().collect::<Vec<_>>());
    p["status"] = json!(if revoked { "revoked" } else { "active" });
    p["revoked_at"] = if revoked { json!(now) } else { Value::Null };
    p["updated_at"] = json!(now);
    let parsed: PolicyRecord = serde_json::from_value(p.clone()).unwrap();
    assert!(parsed.validate().is_empty());
    let hash = sha256(&canonical_bytes(&p).unwrap());
    let a = json!({"schema_version":1,"approval_id":approval,"approved_by":owner(),
        "trusted_surface":"trusted_windows_app","approval_nonce":format!("synthetic-grant-{revision:016}"),
        "approved_diff_hash":hash,"issued_at":now,"expires_at":null,
        "binding":{"kind":"policy_grant","policy_id":id,"policy_revision":revision,"grant_hash":hash}});
    let operation = OperationKind::PolicyChange;
    vault
        .commit(CommitRequest {
            commit_id: CommitId::from_random(vault.random_id_bytes()),
            expected_commit_id: Some(pin.commit_id),
            principal: owner(),
            operation_kind: operation,
            idempotency: IdempotencyScope {
                principal_id: owner().actor_id,
                operation_kind: operation,
                key_hash: sha256(format!("synthetic-policy-{revision}").as_bytes()),
            },
            request_payload_hash: hash,
            expected_revisions: vec![(
                RecordKind::Policy,
                id.to_string(),
                Revision::new(revision - 1),
            )],
            records: vec![
                StagedRecord {
                    record_kind: RecordKind::Policy,
                    record_id: id.to_string(),
                    revision: Revision::new(revision).unwrap(),
                    bytes: canonical_bytes(&p).unwrap(),
                },
                StagedRecord {
                    record_kind: RecordKind::Approval,
                    record_id: approval.to_string(),
                    revision: Revision::new(1).unwrap(),
                    bytes: canonical_bytes(&a).unwrap(),
                },
            ],
            objects: vec![],
        })
        .unwrap();
    id
}
fn initialized(host: &HostService, scopes: Vec<Scope>) -> Server<ReadPort<'_>> {
    let mut server = host.mcp_read_server(grant(scopes)).unwrap();
    let r = server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{
        "protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"0"}}})).unwrap();
    assert!(r["result"].is_object(), "{r}");
    server.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    server
}
fn call(server: &mut Server<ReadPort<'_>>, id: usize, operation: &str, args: Value) -> Value {
    let packet = json!({"schemaVersion":1,"requestId":format!("req_00000001-0000-4000-8000-{id:012x}"),"operation":operation,"idempotencyKey":null,"arguments":args});
    let response = server.handle(&json!({"jsonrpc":"2.0","id":id,"method":"tools/call","params":{"name":operation,"arguments":packet}})).unwrap();
    response["result"]["structuredContent"].clone()
}
fn read(memory: &MemoryId) -> Value {
    json!({"memoryId":memory,"revision":null})
}
fn search_args() -> Value {
    json!({"query":"readable","types":[],"projectIds":[],"includeHistorical":false,"timeMode":{"asOf":null,"knownAtCommitId":null},"cursor":null,"limit":20})
}
fn source_args(source: &SourceId) -> Value {
    json!({"sourceId":source,"sourceRevision":null,"range":null,"maxBytes":8192})
}

#[test]
fn scoped_agent_reads_default_deny_and_do_not_open_a_second_host() {
    let env = Env::new();
    let host = env.host();
    let before = host.vault.pin_current().unwrap();
    let mut s = initialized(&host, vec![Scope::MemoryRead]);
    let catalog = s
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    assert_eq!(catalog["result"]["tools"].as_array().unwrap().len(), 2);
    assert_eq!(
        call(&mut s, 3, "memory_search", search_args())["result"]["items"],
        json!([])
    );
    assert_eq!(
        call(&mut s, 4, "memory_read", read(&env.memory))["error"]["code"],
        "not_found"
    );
    let unknown = MemoryId::parse("mem_000000ff-0000-4000-8000-000000000001").unwrap();
    assert_eq!(
        call(&mut s, 5, "memory_read", read(&unknown))["error"]["code"],
        "not_found"
    );
    assert_eq!(host.vault.pin_current().unwrap(), before);
    assert!(
        host.mcp_read_server(grant(vec![Scope::MemoryPropose]))
            .is_err()
    );
}

#[test]
fn scoped_agent_reads_use_persisted_policy_and_separate_source_permission() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryRead, Scope::SourceRead],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    let mut s = initialized(&host, vec![Scope::MemoryRead, Scope::SourceRead]);
    let found = call(&mut s, 2, "memory_search", search_args());
    assert_eq!(
        found["result"]["items"].as_array().unwrap().len(),
        1,
        "{found}"
    );
    let record = call(&mut s, 3, "memory_read", read(&env.memory));
    assert_eq!(
        record["result"]["record"]["memory_id"],
        json!(env.memory),
        "{record}"
    );
    let source = call(&mut s, 4, "memory_source", source_args(&env.source));
    assert_eq!(
        source["result"]["excerpt"], "synthetic readable 中文 evidence",
        "{source}"
    );
    let mut limited = initialized(&host, vec![Scope::MemoryRead]);
    assert_eq!(
        call(&mut limited, 2, "memory_source", source_args(&env.source))["error"]["code"],
        "permission_denied"
    );
    let log =
        enouia_memory_vault::audit::AuditLog::new(host.vault.managed_root().clone(), 1024 * 1024);
    let (events, torn) = log.read_all().unwrap();
    assert_eq!(torn, 0);
    assert_eq!(events.len(), 3);
    assert!(events.iter().all(|e| e.actor == agent()));
    let text = serde_json::to_string(&events).unwrap();
    assert!(!text.contains("readable"));
    assert!(!text.contains("evidence"));
}

#[test]
fn scoped_agent_reads_refuse_current_policy_revocation_and_sensitivity_limits() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryRead],
        Sensitivity::Normal,
        1,
        false,
    );
    let host = env.host();
    let mut s = initialized(&host, vec![Scope::MemoryRead]);
    assert_eq!(
        call(&mut s, 2, "memory_read", read(&env.memory))["error"]["code"],
        "not_found"
    );
    policy(
        &host.vault,
        &[Scope::MemoryRead],
        Sensitivity::Private,
        2,
        false,
    );
    assert!(call(&mut s, 3, "memory_read", read(&env.memory))["result"].is_object());
    policy(
        &host.vault,
        &[Scope::MemoryRead],
        Sensitivity::Private,
        3,
        true,
    );
    assert_eq!(
        call(&mut s, 4, "memory_read", read(&env.memory))["error"]["code"],
        "not_found"
    );
}

#[test]
fn scoped_agent_reads_expire_and_cannot_cross_window_or_unlock_boundaries() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryRead],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    let mut s = initialized(&host, vec![Scope::MemoryRead]);
    let native =
        || NativeAdmission::from_authenticated_owner(&owner(), HostSurface::Workspace).unwrap();
    let hello = ClientHello {
        schema_version: SchemaVersion,
        protocol_version: 1,
        workspace_version: 1,
        vault_id: host.vault.vault_id().clone(),
        compatibility_hash: crate::compatibility_hash(),
    };
    let (_, window) = host.connect(native(), &hello).unwrap();
    assert!(matches!(
        host.gate.dispatch_agent(&window, || ()),
        Err(SessionFault::ScopeDenied)
    ));
    let connection = host.gate.connect_agent().unwrap();
    let packet = json!({"schemaVersion":1,"requestId":"req_00000001-0000-4000-8000-000000000001","command":"workspace_status","idempotencyKey":null,"arguments":{}});
    assert!(matches!(
        host.call(&connection, &packet),
        Err(ServiceFault::Session(SessionFault::ScopeDenied))
    ));
    host.lock(native()).unwrap();
    host.unlock(native()).unwrap();
    assert_eq!(
        call(&mut s, 2, "memory_read", read(&env.memory))["error"]["code"],
        "backend_unavailable"
    );
    let mut fresh = initialized(&host, vec![Scope::MemoryRead]);
    assert!(call(&mut fresh, 2, "memory_read", read(&env.memory))["result"].is_object());
    env.clock
        .set(env.clock.now_unix_ms() + crate::SESSION_TTL_MS);
    assert_eq!(
        call(&mut fresh, 3, "memory_read", read(&env.memory))["error"]["code"],
        "backend_unavailable"
    );
}

#[test]
fn scoped_agent_source_bytes_are_utf8_bounded_and_audit_failure_is_closed() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::SourceRead],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    let mut s = initialized(&host, vec![Scope::SourceRead]);
    let mut a = source_args(&env.source);
    a["maxBytes"] = json!(21);
    let r = call(&mut s, 2, "memory_source", a.clone());
    assert_eq!(r["result"]["excerpt"], "synthetic readable ", "{r}");
    assert_eq!(r["result"]["truncated"], true);
    a["range"] = json!({"start":20,"end":24});
    assert_eq!(
        call(&mut s, 3, "memory_source", a)["error"]["code"],
        "invalid_request"
    );
    let audit = env.root.join("vault/audit");
    std::fs::rename(&audit, env.root.join("vault/audit-saved")).unwrap();
    std::fs::write(&audit, b"synthetic blocked audit directory").unwrap();
    let r = call(&mut s, 4, "memory_source", source_args(&env.source));
    assert_eq!(r["error"]["code"], "audit_unavailable", "{r}");
    assert!(r["result"].is_null());
}

fn stored_memory(vault: &Vault, id: &MemoryId) -> enouia_memory_contract::memory::CanonicalMemory {
    let pin = vault.pin_current().unwrap();
    let entry = vault
        .record_entry(&pin, RecordKind::Memory, id.as_str())
        .unwrap()
        .unwrap();
    enouia_memory_contract::record::parse_record(
        &vault
            .read_record(
                &pin,
                &RecordRef::new(RecordKind::Memory, id.as_str(), entry.revision),
            )
            .unwrap(),
    )
    .unwrap()
}
fn accept(vault: &Vault, proposal: &Proposal, key: &[u8]) {
    let Proposed::Stored(candidate) =
        propose(vault, proposal, &Origin::owner(owner()), key).unwrap()
    else {
        panic!("stored")
    };
    let shown = plan(
        vault,
        &[Decision::Accept {
            candidate_id: candidate.id,
            revision: Revision::new(1).unwrap(),
        }],
        &owner(),
        TrustedSurface::TrustedWindowsApp,
    )
    .unwrap();
    confirm(
        vault,
        &shown,
        &OwnerConfirmation {
            owner: owner(),
            surface: TrustedSurface::TrustedWindowsApp,
        },
    )
    .unwrap();
}

#[test]
fn scoped_agent_history_cannot_bypass_current_sensitivity() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryRead],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    let mut s = initialized(&host, vec![Scope::MemoryRead]);
    let old_pin = host.vault.pin_current().unwrap();
    assert!(call(&mut s, 2, "memory_read", read(&env.memory))["result"].is_object());
    let mut revision=Proposal::create(ProposedType::Fact,"synthetic readable restricted revision",
        serde_json::from_value(json!({"claim_key":"agent.readable","subject_ids":["sub_00000001-0000-4000-8000-000000000001"]})).unwrap(),
        vec![EvidenceSpec::content(env.source.clone(),Revision::new(1).unwrap())]);
    revision.kind = enouia_memory_contract::candidate::ProposalKind::Revise;
    revision.target_memory_id = Some(env.memory.clone());
    revision.expected_revision = Revision::new(1);
    revision.sensitivity = Some(Sensitivity::HighlySensitive);
    accept(&host.vault, &revision, b"synthetic-private-revision");
    let mut historical = read(&env.memory);
    historical["revision"] = json!(1);
    let denied = call(&mut s, 3, "memory_read", historical);
    assert_eq!(denied["error"]["code"], "not_found", "{denied}");
    assert!(denied["result"].is_null());
    let mut search = search_args();
    search["timeMode"]["knownAtCommitId"] = json!(old_pin.commit_id);
    let denied = call(&mut s, 4, "memory_search", search);
    assert_eq!(denied["error"]["code"], "permission_denied", "{denied}");
    assert!(!denied.to_string().contains("readable"));
}

#[test]
fn scoped_agent_project_and_principal_do_not_authorize_sources_by_inference() {
    let env = Env::new();
    let memory = stored_memory(&env.vault, &env.memory);
    project_policy(
        &env.vault,
        &[Scope::MemoryRead, Scope::SourceRead],
        Sensitivity::Private,
        1,
        false,
        memory.project_id,
    );
    let host = env.host();
    let mut s = initialized(&host, vec![Scope::MemoryRead, Scope::SourceRead]);
    assert!(call(&mut s, 2, "memory_read", read(&env.memory))["result"].is_object());
    assert_eq!(
        call(&mut s, 3, "memory_source", source_args(&env.source))["error"]["code"],
        "not_found"
    );
    let mut other = agent();
    other.actor_id = PrincipalId::parse("prn_00000003-0000-4000-8000-000000000003").unwrap();
    let mut other = host
        .mcp_read_server(Grant::from_authenticated_agent(other, vec![Scope::MemoryRead]).unwrap())
        .unwrap();
    other.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic","version":"0"}}}));
    other.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    assert_eq!(
        call(&mut other, 2, "memory_read", read(&env.memory))["error"]["code"],
        "not_found"
    );
}

#[test]
fn scoped_agent_deletion_blocks_old_pins_and_source_dependencies() {
    let env = Env::new();
    policy(
        &env.vault,
        &[Scope::MemoryRead, Scope::SourceRead],
        Sensitivity::Private,
        1,
        false,
    );
    let host = env.host();
    let mut s = initialized(&host, vec![Scope::MemoryRead, Scope::SourceRead]);
    let before = host.vault.pin_current().unwrap();
    let memory = stored_memory(&host.vault, &env.memory);
    let proposal = enouia_memory_govern::delete::delete_proposal(
        &memory,
        enouia_memory_contract::commit::DeleteMode::Purge,
        enouia_memory_contract::commit::DeleteScope::WithDependents,
    );
    accept(&host.vault, &proposal, b"synthetic-delete");
    assert_eq!(
        call(&mut s, 2, "memory_read", read(&env.memory))["error"]["code"],
        "not_found"
    );
    assert_eq!(
        call(&mut s, 3, "memory_source", source_args(&env.source))["error"]["code"],
        "not_found"
    );
    let mut search = search_args();
    search["timeMode"]["knownAtCommitId"] = json!(before.commit_id);
    let found = call(&mut s, 4, "memory_search", search);
    assert_eq!(found["result"]["items"], json!([]), "{found}");
}

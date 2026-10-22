//! Host + real Workspace over isolated synthetic Windows Vaults.
#![cfg(windows)]
use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
use enouia_memory_contract::foundation::{Clock, FakeClock};
use enouia_memory_contract::host::HostFault;
use enouia_memory_contract::host::session::ClientHello;
use enouia_memory_contract::ids::{DeviceId, PrincipalId};
use enouia_memory_contract::json::SchemaVersion;
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_contract::workspace::{HostSurface, is_write, validate_response};
use enouia_memory_host::service::{HostService, ServiceFault};
use enouia_memory_host::{Connection, NativeAdmission, SessionFault, compatibility_hash};
use enouia_memory_vault::host_recovery::{INTENT_FILE, WITNESS_FILE};
use enouia_memory_vault::host_transaction::HostCoordinator;
use enouia_memory_vault::service::new_genesis;
use enouia_memory_vault::{Fault, RootPolicy, Vault, VaultOptions, VerifiedRoot, verify_data_root};
use enouia_memory_workspace::{Config, Workspace};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

static ROOTS: AtomicU64 = AtomicU64::new(0);
const NOW: i64 = 1_790_000_000_000;
struct Env {
    root: PathBuf,
    clock: Arc<FakeClock>,
    config: Config,
    vault: Vault,
    sequence: AtomicU64,
}
impl Env {
    fn new(adopt: bool) -> Self {
        let base = std::env::temp_dir().join("enouia-memory-host-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let root = base.join(format!(
            "service-{}-{}",
            std::process::id(),
            ROOTS.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&root).unwrap();
        let verified = verify_data_root(&root, &RootPolicy::default()).unwrap();
        let clock = Arc::new(FakeClock::new(NOW));
        let config = Config {
            clock: clock.clone(),
            ids: Arc::new(SequentialIdSource::new(0xd000)),
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
        if adopt {
            drop(HostCoordinator::bootstrap(&vault, &verified, identity()).unwrap());
        }
        Self {
            root,
            clock,
            config,
            vault,
            sequence: AtomicU64::new(0),
        }
    }
    fn root(&self) -> VerifiedRoot {
        verify_data_root(&self.root, &RootPolicy::default()).unwrap()
    }
    fn host(&self) -> HostService {
        HostService::open(&self.root(), identity(), self.config.clone()).unwrap()
    }
    fn connect(&self, host: &HostService, surface: HostSurface) -> Connection {
        host.connect(
            native(surface),
            &ClientHello {
                schema_version: SchemaVersion,
                protocol_version: 1,
                workspace_version: 1,
                vault_id: self.vault.vault_id().clone(),
                compatibility_hash: compatibility_hash(),
            },
        )
        .unwrap()
        .1
    }
    fn packet(&self, command: &str, arguments: Value) -> Value {
        self.clock.set(self.clock.now_unix_ms() + 1000);
        let n = self.sequence.fetch_add(1, Ordering::SeqCst);
        json!({"schemaVersion":1,"requestId":format!("req_00000001-0000-4000-8000-{n:012x}"),
            "command":command,"idempotencyKey":is_write(command).then(||format!("host-key-{n:016x}")),"arguments":arguments})
    }
    fn ok(&self, host: &HostService, connection: &Connection, command: &str, args: Value) -> Value {
        let response = host.call(connection, &self.packet(command, args)).unwrap();
        validate_response(command, &response).unwrap();
        assert_eq!(response["error"], Value::Null, "{response}");
        response["result"].clone()
    }
}
impl Drop for Env {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}
fn owner() -> ActorRef {
    ActorRef {
        actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
        actor_type: ActorType::Owner,
    }
}
fn identity() -> DeviceId {
    DeviceId::parse("dev_00000001-0000-4000-8000-000000000001").unwrap()
}
fn native(surface: HostSurface) -> NativeAdmission {
    NativeAdmission::from_authenticated_owner(&owner(), surface).unwrap()
}
fn list() -> Value {
    json!({"includeInactive":false,"cursor":null,"limit":20})
}

#[test]
fn host_workspace_proposes_then_owner_reviews_and_scoped_overlay_reads() {
    let env = Env::new(true);
    let host = env.host();
    let connection = env.connect(&host, HostSurface::Workspace);
    let proposed = env.ok(
        &host,
        &connection,
        "remember",
        json!({"text":"合成：项目使用本地存储。","claimKey":"host.storage"}),
    );
    assert_eq!(proposed["state"], "pending");
    assert!(
        env.ok(&host, &connection, "memory_list", list())["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let plan = env.ok(&host, &connection, "review_plan", json!({"decisions":[{
        "candidateId":proposed["candidateId"],"revision":proposed["revision"],"action":"accept","editedContent":null,"mergeTarget":null
    }]}));
    env.ok(
        &host,
        &connection,
        "review_confirm",
        json!({"planId":plan["planId"],"diffHash":plan["diffHash"]}),
    );
    assert_eq!(
        env.ok(&host, &connection, "memory_list", list())["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let overlay = env.connect(&host, HostSurface::QuickSearch);
    let found = env.ok(
        &host,
        &overlay,
        "memory_search",
        json!({"query":"本地存储","includeHistorical":false,"cursor":null,"limit":20}),
    );
    assert_eq!(found["items"].as_array().unwrap().len(), 1);
    let before = std::fs::read(env.root.join("vault/CURRENT")).unwrap();
    let denied = host.call(
        &overlay,
        &env.packet(
            "remember",
            json!({"text":"合成写入不能来自 overlay","claimKey":"denied"}),
        ),
    );
    assert!(matches!(
        denied,
        Err(ServiceFault::Session(SessionFault::ScopeDenied))
    ));
    assert_eq!(
        std::fs::read(env.root.join("vault/CURRENT")).unwrap(),
        before
    );
}

#[test]
fn ui_lock_retains_the_unique_lease_and_unlock_requires_a_fresh_connection() {
    let env = Env::new(true);
    let host = env.host();
    let old = env.connect(&host, HostSurface::Workspace);
    let request = env.packet(
        "remember",
        json!({"text":"合成：跨重连保留原始请求。","claimKey":"reconnect"}),
    );
    let original = host.call(&old, &request).unwrap();
    assert!(original["error"].is_null());
    env.ok(&host, &old, "vault_lock", json!({}));
    assert!(matches!(
        host.call(&old, &env.packet("workspace_status", json!({}))),
        Err(ServiceFault::Session(SessionFault::Locked))
    ));
    let competing = HostCoordinator::open(&env.vault, &env.root(), identity())
        .err()
        .unwrap();
    assert_eq!(competing.fault, Fault::Host(HostFault::Busy));
    assert!(HostService::open(&env.root(), identity(), env.config.clone()).is_err());
    assert!(matches!(
        host.unlock(native(HostSurface::QuickSearch)),
        Err(ServiceFault::Session(SessionFault::Unauthenticated))
    ));
    host.unlock(native(HostSurface::Workspace)).unwrap();
    assert!(matches!(
        host.call(&old, &request),
        Err(ServiceFault::Session(SessionFault::Revoked))
    ));
    let fresh = env.connect(&host, HostSurface::Workspace);
    let current = std::fs::read(env.root.join("vault/CURRENT")).unwrap();
    let replay = host.call(&fresh, &request).unwrap();
    assert_eq!(
        replay["result"]["candidateId"],
        original["result"]["candidateId"]
    );
    assert_eq!(
        std::fs::read(env.root.join("vault/CURRENT")).unwrap(),
        current
    );
    drop(host);
    let reopened = env.host();
    assert!(matches!(
        reopened.call(&fresh, &request),
        Err(ServiceFault::Session(SessionFault::Revoked))
    ));
    let connection = env.connect(&reopened, HostSurface::Workspace);
    assert_eq!(
        reopened.call(&connection, &request).unwrap()["result"]["candidateId"],
        original["result"]["candidateId"]
    );
}

#[test]
fn native_control_owns_root_changes_and_bad_witness_blocks_before_core_replay() {
    let env = Env::new(true);
    let host = env.host();
    let connection = env.connect(&host, HostSurface::Workspace);
    for command in ["vault_open", "vault_create", "vault_unlock"] {
        let args = match command {
            "vault_open" => json!({"rootToken":"tok_0123456789abcdef0123456789abcdef"}),
            "vault_create" => {
                json!({"rootToken":"tok_0123456789abcdef0123456789abcdef","confirmPhrase":"create new vault"})
            }
            _ => json!({}),
        };
        assert!(matches!(
            host.call(&connection, &env.packet(command, args)),
            Err(ServiceFault::NativeControlRequired)
        ));
    }
    let request = env.packet(
        "remember",
        json!({"text":"合成：见证损坏时不重放。","claimKey":"witness"}),
    );
    assert!(host.call(&connection, &request).unwrap()["error"].is_null());
    let before = std::fs::read(env.root.join("vault/CURRENT")).unwrap();
    std::fs::write(env.root.join(WITNESS_FILE), b"{synthetic corrupt witness}").unwrap();
    assert!(matches!(
        host.call(&connection, &request),
        Err(ServiceFault::Vault(_))
    ));
    assert!(
        host.connect(
            native(HostSurface::Workspace),
            &ClientHello {
                schema_version: SchemaVersion,
                protocol_version: 1,
                workspace_version: 1,
                vault_id: env.vault.vault_id().clone(),
                compatibility_hash: compatibility_hash(),
            }
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(env.root.join("vault/CURRENT")).unwrap(),
        before
    );
}

#[test]
fn ordinary_roots_and_unbound_workspace_handles_never_become_a_host_implicitly() {
    let env = Env::new(false);
    let current = std::fs::read(env.root.join("vault/CURRENT")).unwrap();
    assert!(HostService::open(&env.root(), identity(), env.config.clone()).is_err());
    assert!(!env.root.join(INTENT_FILE).exists());
    let unbound = Vault::open(
        &env.root(),
        None,
        env.config.clock.clone(),
        env.config.ids.clone(),
        VaultOptions::default(),
    )
    .unwrap();
    assert!(Workspace::from_host_vault(env.config.clone(), Arc::new(unbound)).is_err());
    assert_eq!(
        std::fs::read(env.root.join("vault/CURRENT")).unwrap(),
        current
    );
}

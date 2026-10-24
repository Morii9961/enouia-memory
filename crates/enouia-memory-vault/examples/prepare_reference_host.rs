//! Prepare a NEW synthetic Host v2 root for manual reference-shell acceptance.
//! No input Vault, imported data, process launch or cleanup is supported.
//! Run: cargo run -p enouia-memory-vault --example prepare_reference_host --locked
//! The printed local metadata and root must not be committed.

use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
use enouia_memory_contract::foundation::Clock;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::{DeviceId, PrincipalId};
use enouia_memory_contract::ports::IdSource;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_vault::host_transaction::HostCoordinator;
use enouia_memory_vault::service::new_genesis;
use enouia_memory_vault::{
    OsIdSource, RootPolicy, SystemClock, Vault, VaultOptions, verify_data_root,
};
use std::path::PathBuf;
use std::sync::Arc;

fn main() {
    assert_eq!(
        std::env::args_os().len(),
        1,
        "no input paths or IDs accepted"
    );
    let path = std::env::temp_dir().join(format!(
        "enouia-reference-acceptance-{}",
        sha256(&OsIdSource.random_16()).as_str()
    ));
    // Never reuse, delete or bootstrap an existing directory.
    std::fs::create_dir(&path).expect("create a fresh synthetic directory");
    let canonical = std::fs::canonicalize(path).expect("canonical synthetic path");
    let path = PathBuf::from(canonical.to_string_lossy().trim_start_matches(r"\\?\"));
    let root = verify_data_root(&path, &RootPolicy::default()).expect("local synthetic root");
    enouia_memory_vault::platform::restrict_to_owner(root.path()).expect("owner-only ACL");
    let owner = ActorRef {
        actor_type: ActorType::Owner,
        actor_id: PrincipalId::from_random(OsIdSource.random_16()),
    };
    let now = Timestamp::from_unix_ms(SystemClock.now_unix_ms()).expect("current time");
    let vault = Vault::create(
        &root,
        new_genesis(&OsIdSource, owner, TrustedSurface::TrustedLocalCli, &now)
            .expect("synthetic genesis"),
        Arc::new(SystemClock),
        Arc::new(OsIdSource),
        VaultOptions::default(),
    )
    .expect("new synthetic Vault");
    let identity = DeviceId::from_random(OsIdSource.random_16());
    drop(HostCoordinator::bootstrap(&vault, &root, identity.clone()).expect("synthetic Host v2"));
    drop(HostCoordinator::open(&vault, &root, identity.clone()).expect("release and reacquire"));
    println!(
        "{}",
        serde_json::json!({
            "synthetic": true,
            "root": path,
            "hostId": identity,
            "vaultId": vault.vault_id(),
        })
    );
}

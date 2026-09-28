//! MV-1 measurement (MEMORY_ARCHITECTURE §6): the complete-catalog manifest
//! grows with the number of records, so every commit rewrites a manifest
//! proportional to the Vault. This example commits synthetic manual
//! assertions one per commit into a temporary root and prints, at each
//! checkpoint, the catalog size, the head manifest size, total manifest
//! bytes written, and the latency of the last commits, with and without the
//! full-history cross-record validation.
//!
//! Run: `cargo run --release -p enouia-memory-vault --example measure -- 2000`
//! Synthetic data only; the temporary root is removed afterwards.

use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::ids::{PolicyId, PrincipalId};
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{RootPolicy, Vault, VaultOptions, verify_data_root};
use std::sync::Arc;
use std::time::Instant;

fn run(total: usize, validate: bool) {
    let base = std::env::temp_dir().join(format!(
        "enouia-memory-measure-{}-{validate}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(&base).unwrap();
    let canonical = std::fs::canonicalize(&base).unwrap();
    let path = std::path::PathBuf::from(canonical.to_string_lossy().trim_start_matches(r"\\?\"));
    let root = verify_data_root(&path, &RootPolicy::default()).expect("temp root");
    let clock = Arc::new(FakeClock::new(1_800_000_000_000));
    let ids = Arc::new(SequentialIdSource::new(1));
    let owner = ActorRef {
        actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
        actor_type: ActorType::Owner,
    };
    let now = enouia_memory_contract::time::Timestamp::from_unix_ms(1_800_000_000_000).unwrap();
    let genesis = new_genesis(
        ids.as_ref(),
        owner.clone(),
        TrustedSurface::TrustedLocalCli,
        &now,
    )
    .unwrap();
    let options = VaultOptions {
        validate_record_set: validate,
        ..VaultOptions::default()
    };
    let vault = Vault::create(&root, genesis, clock.clone(), ids, options).unwrap();
    let head = vault.read_manifest(&vault.pin_current().unwrap()).unwrap();
    let policy = head
        .catalog
        .iter()
        .find(|e| e.record_kind == RecordKind::Policy)
        .map(|e| PolicyId::parse(&e.record_id).unwrap())
        .unwrap();
    println!("validate_record_set={validate}");
    println!("records  head_manifest_bytes  manifest_bytes_total  last_50_commits_ms_avg");
    let mut manifest_total: u64 = 0;
    let mut window = Instant::now();
    for n in 1..=total {
        clock.set(1_800_000_000_000 + n as i64 * 1_000);
        let input = ManualAssertionInput {
            text: format!("（合成）第 {n} 条测量用陈述。"),
            operator: owner.clone(),
            trusted_surface: TrustedSurface::TrustedLocalCli,
            confirmation: ConfirmationMethod::TypedConfirmation,
            sensitivity: Sensitivity::Private,
            access_policy_id: policy.clone(),
            time_precision: TimePrecision::Millisecond,
        };
        vault
            .record_manual_assertion(&input, format!("m{n}").as_bytes())
            .unwrap();
        let pin = vault.pin_current().unwrap();
        let size = std::fs::metadata(
            path.join("vault/commits")
                .join(format!("{}.json", pin.commit_id)),
        )
        .unwrap()
        .len();
        manifest_total += size;
        if n % 50 == 0 {
            let avg = window.elapsed().as_secs_f64() * 1000.0 / 50.0;
            if n % 250 == 0 || n == total {
                println!(
                    "{:>7}  {:>19}  {:>20}  {:>22.1}",
                    n + 1,
                    size,
                    manifest_total,
                    avg
                );
            }
            window = Instant::now();
        }
    }
    drop(vault);
    let _ = std::fs::remove_dir_all(&base);
}

fn main() {
    let total: usize = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(1000);
    run(total, false);
    run(total.min(1000), true);
}

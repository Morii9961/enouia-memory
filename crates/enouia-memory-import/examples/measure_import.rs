//! MV-2 scale measurement: import a synthetic ChatGPT-shaped export of
//! `conversations × messages` into a temporary Vault and print the time per
//! batch commit and the head manifest size (ADR-MEM-37 trigger: 1 MiB head
//! manifest or 1 s per commit). Synthetic data only; the root is removed.
//!
//! Run: `cargo run --release -p enouia-memory-import --example measure_import -- 200 20 50`

use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::ids::{PolicyId, PrincipalId};
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_import::pipeline::NeverCancel;
use enouia_memory_import::{ImportOptions, import_file};
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{RootPolicy, Vault, VaultOptions, verify_data_root};
use serde_json::{Map, json};
use std::sync::Arc;
use std::time::Instant;

fn main() {
    let args: Vec<usize> = std::env::args()
        .skip(1)
        .filter_map(|a| a.parse().ok())
        .collect();
    let (conversations, messages, batch) = (
        *args.first().unwrap_or(&200),
        *args.get(1).unwrap_or(&20),
        *args.get(2).unwrap_or(&50),
    );
    let base = std::env::temp_dir().join(format!("enouia-import-measure-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    std::fs::create_dir_all(base.join("root")).unwrap();
    let canonical = std::fs::canonicalize(&base).unwrap();
    let base = std::path::PathBuf::from(canonical.to_string_lossy().trim_start_matches(r"\\?\"));

    let mut export = Vec::new();
    for c in 0..conversations {
        let mut mapping = Map::new();
        for m in 0..messages {
            let id = format!("c{c}m{m}");
            let parent = (m > 0).then(|| format!("c{c}m{}", m - 1));
            mapping.insert(id.clone(), json!({"id": id, "parent": parent, "children": [], "message": {
                "id": id, "author": {"role": if m % 2 == 0 { "user" } else { "assistant" }},
                "create_time": 1_719_900_000.0 + (c * 1000 + m) as f64,
                "content": {"content_type": "text", "parts": [format!("（合成）第 {c} 段对话的第 {m} 条消息，带一些普通长度的正文用于测量。")]}}}));
        }
        export.push(
            json!({"conversation_id": format!("conv-{c}"), "mapping": mapping,
                           "current_node": format!("c{c}m{}", messages - 1)}),
        );
    }
    let bytes = serde_json::to_vec(&export).unwrap();
    let input = base.join("conversations.json");
    std::fs::write(&input, &bytes).unwrap();

    let root = verify_data_root(&base.join("root"), &RootPolicy::default()).unwrap();
    let clock = Arc::new(FakeClock::new(1_790_000_000_000));
    let ids = Arc::new(SequentialIdSource::new(1));
    let owner = ActorRef {
        actor_id: PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap(),
        actor_type: ActorType::Owner,
    };
    let now = enouia_memory_contract::time::Timestamp::from_unix_ms(1_790_000_000_000).unwrap();
    let genesis = new_genesis(
        ids.as_ref(),
        owner.clone(),
        TrustedSurface::TrustedLocalCli,
        &now,
    )
    .unwrap();
    let validate = std::env::var_os("MEASURE_NO_SET_VALIDATION").is_none();
    let vault = Vault::create(
        &root,
        genesis,
        clock,
        ids,
        VaultOptions {
            validate_record_set: validate,
            ..VaultOptions::default()
        },
    )
    .unwrap();
    println!("validate_record_set={validate}");
    let pin = vault.pin_current().unwrap();
    let policy = vault
        .read_manifest(&pin)
        .unwrap()
        .catalog
        .iter()
        .find(|e| e.record_kind == RecordKind::Policy)
        .map(|e| PolicyId::parse(&e.record_id).unwrap())
        .unwrap();
    let mut options = ImportOptions::new(owner, "acct-main", policy);
    options.batch_units = batch;

    println!(
        "input: {conversations} conversations x {messages} messages = {} sources, {} bytes, batch {batch} conversations",
        conversations * messages,
        bytes.len()
    );
    let started = Instant::now();
    let report = import_file(&vault, &input, &options, &NeverCancel).unwrap();
    let elapsed = started.elapsed().as_secs_f64();
    let pin = vault.pin_current().unwrap();
    let head = std::fs::metadata(
        base.join("root/vault/commits")
            .join(format!("{}.json", pin.commit_id)),
    )
    .unwrap()
    .len();
    println!(
        "status {:?}; commits {}; total {elapsed:.1} s; {:.2} s per commit; head manifest {head} bytes",
        report.manifest.status,
        report.commits,
        elapsed / report.commits as f64
    );
    let stored = vault.stored_commit(&pin).unwrap();
    let mut per_kind: std::collections::BTreeMap<String, (usize, u64)> = Default::default();
    for r in stored.record_segments.iter().chain(&stored.object_segments) {
        let key = r
            .record_kind
            .map_or("objects".to_owned(), |k| k.name().to_owned());
        let e = per_kind.entry(key).or_default();
        e.0 += 1;
        e.1 += r.entry_count;
    }
    println!("segments (count, entries) per kind: {per_kind:?}");
    drop(vault);

    // A later single-record commit from a fresh process (cold caches): the
    // cost every CLI command pays after a large import.
    let clock = Arc::new(FakeClock::new(1_790_000_100_000));
    let ids = Arc::new(SequentialIdSource::new(900_000));
    let started = Instant::now();
    let vault = Vault::open(
        &root,
        None,
        clock,
        ids,
        VaultOptions {
            validate_record_set: validate,
            ..VaultOptions::default()
        },
    )
    .unwrap();
    vault
        .record_manual_assertion(
            &ManualAssertionInput {
                text: "（合成）导入之后的一句话。".into(),
                operator: options.owner.clone(),
                trusted_surface: TrustedSurface::TrustedLocalCli,
                confirmation: ConfirmationMethod::TypedConfirmation,
                sensitivity: Sensitivity::Private,
                access_policy_id: options.access_policy_id.clone(),
                time_precision: TimePrecision::Millisecond,
            },
            b"measure-after-import",
        )
        .unwrap();
    let pin = vault.pin_current().unwrap();
    let head = std::fs::metadata(
        base.join("root/vault/commits")
            .join(format!("{}.json", pin.commit_id)),
    )
    .unwrap()
    .len();
    println!(
        "one assertion after reopening: {:.2} s; head manifest {head} bytes",
        started.elapsed().as_secs_f64()
    );
    drop(vault);
    let _ = std::fs::remove_dir_all(&base);
}

//! `enouia-memory`: the minimal controlled local entry point for MV-1
//! (IMPLEMENTATION_PLAN §5). Every command takes an explicit data root; the
//! CLI never creates or guesses a default location. Output is one JSON
//! object on stdout with IDs, counts, states, and error codes only.
//!
//! Owner-confirmed actions (new Vault, recovery adoption, restore, manual
//! assertion) require an explicit confirmation argument on this trusted local
//! surface; nothing here calls a model, the network, or a sync service.

use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::ids::{CommitId, PolicyId, PrincipalId};
use enouia_memory_contract::ports::IdSource;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_contract::store::{VaultDescriptor, parse_store};
use enouia_memory_vault::backup::{export_pinned, network_allowed, restore_export, verify_export};
use enouia_memory_vault::fault::Faults;
use enouia_memory_vault::health::HealthState;
use enouia_memory_vault::platform::{inspect_acl, restrict_to_owner};
use enouia_memory_vault::service::{ManualAssertionInput, new_genesis};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, SystemClock, Vault, VaultError, VaultOptions, verify_data_root,
};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

const USAGE: &str = "usage: enouia-memory <command> ...
  check-root <dir>
  init <dir> --confirm-new-vault
  status <dir>
  verify <dir>
  recovery <dir>
  adopt <dir> <commit_id> --confirm <commit_id>
  assert <dir> --text <text> --confirm-text <text> [--key <submission-key>]
  export <dir> <empty-destination>
  verify-export <export-dir>
  restore <export-dir> <empty-target> --confirm-restore
  acl <dir>
  protect <dir> --confirm-owner-only";

enum Failure {
    Usage(String),
    Rejected(String),
    Vault(VaultError),
}

impl From<VaultError> for Failure {
    fn from(error: VaultError) -> Self {
        Self::Vault(error)
    }
}

type Outcome = Result<Value, Failure>;

fn verified(dir: &str) -> Result<enouia_memory_vault::VerifiedRoot, Failure> {
    verify_data_root(Path::new(dir), &RootPolicy::default())
        .map_err(|r| Failure::Rejected(format!("{r:?}")))
}

fn open(dir: &str) -> Result<Vault, Failure> {
    Ok(Vault::open(
        &verified(dir)?,
        None,
        Arc::new(SystemClock),
        Arc::new(OsIdSource),
        VaultOptions::default(),
    )?)
}

fn flag<'a>(args: &'a [String], name: &str) -> Option<&'a str> {
    args.windows(2)
        .find(|w| w[0] == name)
        .map(|w| w[1].as_str())
}

fn has(args: &[String], name: &str) -> bool {
    args.iter().any(|a| a == name)
}

fn arg(args: &[String], index: usize) -> Result<&str, Failure> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| Failure::Usage(USAGE.to_owned()))
}

fn state(state: HealthState) -> &'static str {
    match state {
        HealthState::Healthy => "healthy",
        HealthState::Degraded => "degraded",
        HealthState::Unavailable => "unavailable",
        HealthState::Recovering => "recovering",
    }
}

fn owner_of(vault: &Vault) -> ActorRef {
    vault.descriptor().created_by.clone()
}

fn run(args: &[String]) -> Outcome {
    let command = arg(args, 1)?;
    match command {
        "check-root" => {
            let root = verified(arg(args, 2)?)?;
            Ok(
                json!({"root": "accepted", "filesystem": root.filesystem, "free_bytes": root.free_bytes}),
            )
        }
        "init" => {
            let dir = arg(args, 2)?;
            if !has(args, "--confirm-new-vault") {
                return Err(Failure::Usage("init needs --confirm-new-vault".into()));
            }
            let root = verified(dir)?;
            let ids = Arc::new(OsIdSource);
            let owner = ActorRef {
                actor_id: PrincipalId::from_random(ids.random_16()),
                actor_type: ActorType::Owner,
            };
            let now = enouia_memory_contract::ports::commit_time(
                &SystemClock,
                None,
                enouia_memory_contract::foundation::ComponentId::Vault,
            )
            .map_err(|e| Failure::Vault(e.into()))?;
            let genesis = new_genesis(
                ids.as_ref(),
                owner.clone(),
                TrustedSurface::TrustedLocalCli,
                &now,
            )?;
            let vault = Vault::create(
                &root,
                genesis,
                Arc::new(SystemClock),
                ids,
                VaultOptions::default(),
            )?;
            let pin = vault.pin_current()?;
            Ok(
                json!({"vault_id": vault.vault_id(), "owner": owner.actor_id, "commit_id": pin.commit_id, "sequence": pin.sequence}),
            )
        }
        "status" => {
            let vault = open(arg(args, 2)?)?;
            let health = vault.health();
            Ok(json!({
                "vault_id": vault.vault_id(),
                "state": state(health.state),
                "error_code": health.error_code,
                "head_commit_id": health.head_commit_id,
                "head_sequence": health.head_sequence,
                "last_commit_at": health.last_commit_at,
                "policy_epoch": health.policy_epoch,
                "deletion_epoch": health.deletion_epoch,
                "notes": health.notes.iter().map(|n| format!("{n:?}")).collect::<Vec<_>>(),
                "network_allowed": network_allowed(&vault)?,
            }))
        }
        "verify" => {
            let vault = open(arg(args, 2)?)?;
            let pin = vault.pin_current()?;
            let report = vault.verify(&pin)?;
            Ok(json!({
                "commit_id": pin.commit_id,
                "clean": report.is_clean(),
                "records_checked": report.records_checked,
                "objects_checked": report.objects_checked,
                "missing_records": report.missing_records,
                "corrupt_records": report.corrupt_records,
                "missing_objects": report.missing_objects,
                "corrupt_objects": report.corrupt_objects,
            }))
        }
        "recovery" => {
            let vault = open(arg(args, 2)?)?;
            let report = vault.recovery_report()?;
            Ok(json!({
                "current": format!("{:?}", report.current),
                "candidates": report.candidates.iter().map(|c| json!({
                    "commit_id": c.commit_id, "sequence": c.sequence,
                    "evidence": c.evidence, "complete": c.complete,
                })).collect::<Vec<_>>(),
            }))
        }
        "adopt" => {
            let vault = open(arg(args, 2)?)?;
            let id = arg(args, 3)?;
            if flag(args, "--confirm") != Some(id) {
                return Err(Failure::Usage(
                    "adopt needs --confirm <the same commit_id>".into(),
                ));
            }
            let commit = CommitId::parse(id).map_err(|_| Failure::Usage("bad commit_id".into()))?;
            let receipt = vault.adopt_recovery_point(
                &commit,
                &owner_of(&vault),
                TrustedSurface::TrustedLocalCli,
            )?;
            Ok(
                json!({"recovery_id": receipt.recovery_id, "adopted_commit_id": receipt.adopted_commit_id, "evidence": receipt.evidence}),
            )
        }
        "assert" => {
            let vault = open(arg(args, 2)?)?;
            let text =
                flag(args, "--text").ok_or_else(|| Failure::Usage("assert needs --text".into()))?;
            if flag(args, "--confirm-text") != Some(text) {
                return Err(Failure::Usage(
                    "assert needs --confirm-text with the exact same text".into(),
                ));
            }
            let pin = vault.pin_current()?;
            let manifest = vault.read_manifest(&pin)?;
            let policy = manifest
                .catalog
                .iter()
                .find(|e| e.record_kind == RecordKind::Policy)
                .and_then(|e| PolicyId::parse(&e.record_id).ok())
                .ok_or_else(|| Failure::Rejected("no policy".into()))?;
            let key = match flag(args, "--key") {
                Some(key) => key.as_bytes().to_vec(),
                None => OsIdSource.random_16().to_vec(),
            };
            let written = vault.record_manual_assertion(
                &ManualAssertionInput {
                    text: text.to_owned(),
                    operator: owner_of(&vault),
                    trusted_surface: TrustedSurface::TrustedLocalCli,
                    confirmation: ConfirmationMethod::TypedConfirmation,
                    sensitivity: Sensitivity::Private,
                    access_policy_id: policy,
                    time_precision: TimePrecision::Millisecond,
                },
                &key,
            )?;
            let replayed = matches!(
                written.outcome,
                enouia_memory_contract::ports::CommitOutcome::Replayed { .. }
            );
            Ok(json!({"source_id": written.id, "replayed": replayed}))
        }
        "export" => {
            let vault = open(arg(args, 2)?)?;
            let destination = verified(arg(args, 3)?)?;
            let pin = vault.pin_current()?;
            let export = export_pinned(&vault, &pin, &destination)?;
            Ok(
                json!({"commit_id": export.commit_id, "sequence": export.sequence, "files": export.files.len()}),
            )
        }
        "verify-export" => {
            let export = verify_export(Path::new(arg(args, 2)?))?;
            Ok(
                json!({"valid": true, "vault_id": export.vault_id, "commit_id": export.commit_id, "files": export.files.len()}),
            )
        }
        "restore" => {
            let export_dir = PathBuf::from(arg(args, 2)?);
            let target = verified(arg(args, 3)?)?;
            if !has(args, "--confirm-restore") {
                return Err(Failure::Usage("restore needs --confirm-restore".into()));
            }
            let descriptor_bytes = std::fs::read(export_dir.join("vault").join("vault.json"))
                .map_err(|_| Failure::Rejected("export has no descriptor".into()))?;
            let descriptor: VaultDescriptor = parse_store(&descriptor_bytes)
                .map_err(|_| Failure::Rejected("export descriptor invalid".into()))?;
            let restored = restore_export(
                &export_dir,
                &target,
                &descriptor.created_by,
                TrustedSurface::TrustedLocalCli,
                Arc::new(SystemClock),
                Arc::new(OsIdSource),
                VaultOptions {
                    faults: Faults::none(),
                    ..VaultOptions::default()
                },
            )?;
            Ok(json!({
                "vault_id": restored.state.vault_id,
                "restored_commit_id": restored.state.restored_commit_id,
                "network_disabled_until_reconciled": restored.state.network_disabled_until_reconciled,
            }))
        }
        "acl" => {
            let root = verified(arg(args, 2)?)?;
            let report = inspect_acl(root.path()).map_err(|e| Failure::Vault(e.into()))?;
            Ok(
                json!({"owner_only": report.is_owner_only(), "protected": report.protected, "broad_grants": report.broad_grants()}),
            )
        }
        "protect" => {
            let root = verified(arg(args, 2)?)?;
            if !has(args, "--confirm-owner-only") {
                return Err(Failure::Usage("protect needs --confirm-owner-only".into()));
            }
            restrict_to_owner(root.path()).map_err(|e| Failure::Vault(e.into()))?;
            let report = inspect_acl(root.path()).map_err(|e| Failure::Vault(e.into()))?;
            Ok(json!({"owner_only": report.is_owner_only()}))
        }
        _ => Err(Failure::Usage(USAGE.to_owned())),
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    match run(&args) {
        Ok(value) => {
            println!("{value}");
            ExitCode::SUCCESS
        }
        Err(Failure::Usage(text)) => {
            eprintln!("{text}");
            ExitCode::from(1)
        }
        Err(Failure::Rejected(reason)) => {
            println!("{}", json!({"error": "root_rejected", "reason": reason}));
            ExitCode::from(2)
        }
        Err(Failure::Vault(error)) => {
            println!(
                "{}",
                json!({"error": error.error.code, "retryable": error.error.retryable, "fault": format!("{:?}", error.fault)})
            );
            ExitCode::from(2)
        }
    }
}

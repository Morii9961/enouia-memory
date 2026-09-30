//! `enouia-memory`: the minimal controlled local entry point for MV-1
//! (IMPLEMENTATION_PLAN §5). Every command takes an explicit data root; the
//! CLI never creates or guesses a default location. Output is one JSON
//! object on stdout with IDs, counts, states, and error codes only.
//!
//! Owner-confirmed actions (new Vault, recovery adoption, restore, manual
//! assertion) require an explicit confirmation argument on this trusted local
//! surface; nothing here calls a model, the network, or a sync service.
//!
//! Review commands (MV-3) print the exact plan (every record they would
//! write, the text included) with an 8-character code from the plan hash,
//! then read the typed code from stdin; only an exact match commits. Their
//! last line is the result.

use enouia_memory_contract::candidate::ProposedType;
use enouia_memory_contract::commit::{DeleteMode, DeleteScope};
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::{
    CandidateId, CommitId, ImportId, MemoryId, PolicyId, PrincipalId,
};
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::memory::CanonicalMemory;
use enouia_memory_contract::ports::IdSource;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::record::RecordRef;
use enouia_memory_contract::source::ConfirmationMethod;
use enouia_memory_contract::store::{VaultDescriptor, parse_store};
use enouia_memory_govern::delete::{
    complete_purge, delete_proposal, deletion_ledger, ledger_bytes, parse_ledger, purge_preview,
    reconcile_deletions,
};
use enouia_memory_govern::{
    Canonical, Decision, EvidenceSpec, Origin, OwnerConfirmation, Proposal, Proposed, ReviewPlan,
    canonical_memories, confirm, pending_candidates, plan, propose,
};
use enouia_memory_index::{Index, SearchRequest, search};
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
  sweep <dir> --confirm-sweep
  import <dir> <file> --account <alias> --confirm-import
  resume-import <dir> <import_id>
  import-audit <dir> <import_id>
  acl <dir>
  protect <dir> --confirm-owner-only
  candidates <dir>
  memories <dir> [--all]
  remember <dir> --text <text> --claim <claim-key> [--subject <sub_id>]   (plan, then type its code)
  review <dir> <candidate_id> <revision> accept|reject|edit [--text <text>]
  forget <dir> <memory_id>                                (plan, then type its code)
  purge-preview <dir> <memory_id> [--with-dependents]
  purge <dir> <memory_id> [--with-dependents]             (plan, then type its code)
  deletion-ledger <dir> <out-file>
  reconcile <dir> <ledger-file>                           (summary, then type its code)
  index <dir>
  index-rebuild <dir>
  search <dir> <query> [--limit <n>] [--cursor <c>] [--historical]";

enum Failure {
    Usage(String),
    /// The owner did not type the plan's confirmation code.
    NotConfirmed,
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

fn genesis_policy(vault: &Vault) -> Result<PolicyId, Failure> {
    let pin = vault.pin_current()?;
    vault
        .read_manifest(&pin)?
        .catalog
        .iter()
        .find(|e| e.record_kind == RecordKind::Policy)
        .and_then(|e| PolicyId::parse(&e.record_id).ok())
        .ok_or_else(|| Failure::Rejected("no policy".into()))
}

fn import_options(
    vault: &Vault,
    account: &str,
) -> Result<enouia_memory_import::ImportOptions, Failure> {
    Ok(enouia_memory_import::ImportOptions::new(
        owner_of(vault),
        account,
        genesis_policy(vault)?,
    ))
}

/// Counts, cursor, status, and warning codes only: never titles or text.
fn import_json(m: &enouia_memory_contract::import::ImportManifest) -> Value {
    json!({
        "import_id": m.import_id, "status": m.status, "input_kind": m.input_kind,
        "duplicate_of": m.duplicate_of, "cursor": m.cursor, "counts": m.counts,
        "conversations_covered": m.coverage.len(),
        "members_quarantined": m.members.iter().filter(|x| x.disposition == enouia_memory_contract::import::MemberDisposition::Quarantined).count(),
        "warnings": m.warnings.iter().map(|w| w.code.clone()).collect::<Vec<_>>(),
    })
}

fn owner_of(vault: &Vault) -> ActorRef {
    vault.descriptor().created_by.clone()
}

fn confirmation(vault: &Vault) -> OwnerConfirmation {
    OwnerConfirmation {
        owner: owner_of(vault),
        surface: TrustedSurface::TrustedLocalCli,
    }
}

/// Print what would be written with a short code derived from its hash,
/// then read the typed code from stdin. Only an exact match confirms.
fn typed_code(shown: Value, code: &str) -> Result<(), Failure> {
    use std::io::Write;
    println!("{}", json!({"plan": shown, "confirm_code": code}));
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| Failure::Vault(e.into()))?;
    if line.trim() == code {
        Ok(())
    } else {
        Err(Failure::NotConfirmed)
    }
}

fn plan_and_confirm(vault: &Vault, decisions: &[Decision]) -> Result<ReviewPlan, Failure> {
    let owner = owner_of(vault);
    let shown = plan(vault, decisions, &owner, TrustedSurface::TrustedLocalCli)?;
    typed_code(
        json!({
            "operation": shown.operation_kind,
            "expires_at": shown.expires_at,
            "records": shown.diff["records"],
            "objects": shown.diff["objects"],
        }),
        &shown.diff_hash.as_str()[..8],
    )?;
    confirm(vault, &shown, &confirmation(vault))?;
    Ok(shown)
}

fn memory_id(text: &str) -> Result<MemoryId, Failure> {
    MemoryId::parse(text).map_err(|_| Failure::Usage("bad memory id".into()))
}

/// The latest stored revision of a memory, tombstoned or not.
fn stored_memory(vault: &Vault, id: &MemoryId) -> Result<CanonicalMemory, Failure> {
    let pin = vault.pin_current()?;
    let entry = vault
        .record_entry(&pin, RecordKind::Memory, id.as_str())?
        .ok_or_else(|| Failure::Rejected("memory not found".into()))?;
    let bytes = vault.read_record(
        &pin,
        &RecordRef::new(RecordKind::Memory, id.as_str(), entry.revision),
    )?;
    enouia_memory_contract::parse_record(&bytes)
        .map_err(|_| Failure::Rejected("memory unreadable".into()))
}

fn delete_candidate(
    vault: &Vault,
    memory: &CanonicalMemory,
    mode: DeleteMode,
    scope: DeleteScope,
) -> Result<Decision, Failure> {
    let id = match propose(
        vault,
        &delete_proposal(memory, mode, scope),
        &Origin::owner(owner_of(vault)),
        &OsIdSource.random_16(),
    )? {
        Proposed::Stored(written) => written.id,
        Proposed::DuplicateOf(id) => id,
    };
    let pin = vault.pin_current()?;
    let revision = vault
        .record_entry(&pin, RecordKind::Candidate, id.as_str())?
        .map(|e| e.revision)
        .ok_or_else(|| Failure::Rejected("candidate missing".into()))?;
    Ok(Decision::Accept {
        candidate_id: id,
        revision,
    })
}

fn scope_of(args: &[String]) -> DeleteScope {
    if has(args, "--with-dependents") {
        DeleteScope::WithDependents
    } else {
        DeleteScope::AllRevisions
    }
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
                "damaged_segments": report.damaged_segments,
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
        "import" => {
            let vault = open(arg(args, 2)?)?;
            let file = PathBuf::from(arg(args, 3)?);
            let account = flag(args, "--account")
                .ok_or_else(|| Failure::Usage("import needs --account <alias>".into()))?;
            if !has(args, "--confirm-import") {
                return Err(Failure::Usage("import needs --confirm-import".into()));
            }
            let report = enouia_memory_import::import_file(
                &vault,
                &file,
                &import_options(&vault, account)?,
                &enouia_memory_import::pipeline::NeverCancel,
            )?;
            Ok(import_json(&report.manifest))
        }
        "resume-import" => {
            let vault = open(arg(args, 2)?)?;
            let id = ImportId::parse(arg(args, 3)?)
                .map_err(|_| Failure::Usage("bad import_id".into()))?;
            let account = flag(args, "--account").unwrap_or("acct-main");
            let report = enouia_memory_import::resume_import(
                &vault,
                &id,
                &import_options(&vault, account)?,
                &enouia_memory_import::pipeline::NeverCancel,
            )?;
            Ok(import_json(&report.manifest))
        }
        "import-audit" => {
            let vault = open(arg(args, 2)?)?;
            let id = ImportId::parse(arg(args, 3)?)
                .map_err(|_| Failure::Usage("bad import_id".into()))?;
            let audit = enouia_memory_import::audit_import(&vault, &id)?;
            Ok(json!({
                "import_id": audit.import_id, "consistent": audit.is_consistent(),
                "raw_present": audit.raw_present, "sources_citing": audit.sources_citing,
                "revisions_checked": audit.revisions_checked, "coverage_total": audit.coverage_total,
                "mismatched": audit.mismatched.len(), "unresolvable": audit.unresolvable.len(),
            }))
        }
        "sweep" => {
            let vault = open(arg(args, 2)?)?;
            if !has(args, "--confirm-sweep") {
                return Err(Failure::Usage("sweep needs --confirm-sweep".into()));
            }
            let report = vault.sweep_unreferenced()?;
            Ok(
                json!({"sweep_id": report.sweep_id, "files_checked": report.files_checked, "quarantined": report.quarantined.len()}),
            )
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
        "candidates" => {
            let vault = open(arg(args, 2)?)?;
            let pending = pending_candidates(&vault, &vault.pin_current()?)?;
            let list: Vec<Value> = pending
                .iter()
                .map(|c| {
                    json!({
                        "candidate_id": c.candidate_id, "revision": c.revision,
                        "proposal_kind": c.proposal_kind, "proposed_type": c.proposed_type,
                        "origin_kind": c.origin_kind, "sensitivity": c.sensitivity,
                        "conflicts": c.conflicts.len(), "content": c.proposed_content,
                    })
                })
                .collect();
            Ok(json!({"pending": list}))
        }
        "memories" => {
            let vault = open(arg(args, 2)?)?;
            let which = if has(args, "--all") {
                Canonical::AllStatuses
            } else {
                Canonical::Active
            };
            let list: Vec<Value> = canonical_memories(&vault, &vault.pin_current()?, which)?
                .iter()
                .map(|m| {
                    json!({
                        "memory_id": m.memory_id, "revision": m.revision, "type": m.memory_type(),
                        "status": m.status, "conflict_group_id": m.conflict_group_id,
                        "content": m.content,
                    })
                })
                .collect();
            Ok(json!({"memories": list}))
        }
        "remember" => {
            // The quick save of the design: the exact words become a manual
            // assertion, and a memory only after the typed code.
            let vault = open(arg(args, 2)?)?;
            let text = flag(args, "--text")
                .ok_or_else(|| Failure::Usage("remember needs --text".into()))?;
            let claim = flag(args, "--claim")
                .ok_or_else(|| Failure::Usage("remember needs --claim".into()))?;
            let pin = vault.pin_current()?;
            let policy = vault
                .record_entries(&pin, RecordKind::Policy)?
                .first()
                .and_then(|e| PolicyId::parse(&e.record_id).ok())
                .ok_or_else(|| Failure::Rejected("no policy".into()))?;
            let source = vault
                .record_manual_assertion(
                    &ManualAssertionInput {
                        text: text.to_owned(),
                        operator: owner_of(&vault),
                        trusted_surface: TrustedSurface::TrustedLocalCli,
                        confirmation: ConfirmationMethod::TypedConfirmation,
                        sensitivity: Sensitivity::Private,
                        access_policy_id: policy,
                        time_precision: TimePrecision::Millisecond,
                    },
                    &OsIdSource.random_16(),
                )?
                .id;
            // The subject defaults to the owner: the owner principal's UUID
            // under the subject prefix.
            let subject = match flag(args, "--subject") {
                Some(s) => s.to_owned(),
                None => owner_of(&vault)
                    .actor_id
                    .as_str()
                    .replacen("prn_", "sub_", 1),
            };
            let subject = enouia_memory_contract::ids::SubjectId::parse(&subject)
                .map_err(|_| Failure::Usage("bad subject id".into()))?;
            let mut details = serde_json::Map::new();
            details.insert("claim_key".into(), json!(claim));
            details.insert("subject_ids".into(), json!([subject]));
            let proposal = Proposal::create(
                ProposedType::Fact,
                text,
                details,
                vec![EvidenceSpec::content(
                    source,
                    Revision::new(1).expect("one"),
                )],
            );
            let id = match propose(
                &vault,
                &proposal,
                &Origin::owner(owner_of(&vault)),
                &OsIdSource.random_16(),
            )? {
                Proposed::Stored(written) => written.id,
                Proposed::DuplicateOf(id) => id,
            };
            let shown = plan_and_confirm(
                &vault,
                &[Decision::Accept {
                    candidate_id: id,
                    revision: Revision::new(1).expect("one"),
                }],
            )?;
            Ok(json!({"memory_id": shown.ids[0].memory_id, "commit_id": shown.commit_id}))
        }
        "review" => {
            let vault = open(arg(args, 2)?)?;
            let candidate_id = CandidateId::parse(arg(args, 3)?)
                .map_err(|_| Failure::Usage("bad candidate id".into()))?;
            let revision = arg(args, 4)?
                .parse::<u64>()
                .ok()
                .and_then(Revision::new)
                .ok_or_else(|| Failure::Usage("bad revision".into()))?;
            let decision = match arg(args, 5)? {
                "accept" => Decision::Accept {
                    candidate_id,
                    revision,
                },
                "reject" => Decision::Reject {
                    candidate_id,
                    revision,
                    reason_code: None,
                },
                "edit" => Decision::EditAccept {
                    candidate_id,
                    revision,
                    content: flag(args, "--text")
                        .ok_or_else(|| Failure::Usage("edit needs --text".into()))?
                        .to_owned(),
                    details: None,
                },
                _ => return Err(Failure::Usage(USAGE.to_owned())),
            };
            let shown = plan_and_confirm(&vault, &[decision])?;
            Ok(json!({"commit_id": shown.commit_id, "review_id": shown.ids[0].review_id}))
        }
        "forget" => {
            let vault = open(arg(args, 2)?)?;
            let memory = stored_memory(&vault, &memory_id(arg(args, 3)?)?)?;
            let decision = delete_candidate(
                &vault,
                &memory,
                DeleteMode::LogicalDelete,
                DeleteScope::AllRevisions,
            )?;
            let shown = plan_and_confirm(&vault, &[decision])?;
            Ok(json!({"delete_id": shown.ids[0].delete_id, "commit_id": shown.commit_id}))
        }
        "purge-preview" => {
            let vault = open(arg(args, 2)?)?;
            let impact = purge_preview(
                &vault,
                &memory_id(arg(args, 3)?)?,
                DeleteMode::Purge,
                scope_of(args),
            )?;
            let targets: Vec<Value> = impact
                .targets
                .iter()
                .map(|(k, i, r)| json!({"record_kind": k, "record_id": i, "revision": r}))
                .collect();
            let shared: Vec<Value> = impact
                .shared_raw
                .iter()
                .map(|(h, n)| json!({"object_hash": h, "other_sources": n}))
                .collect();
            Ok(json!({
                "targets": targets,
                "object_hashes": impact.object_hashes,
                "losing_provenance": impact.losing_provenance,
                "shared_raw": shared,
            }))
        }
        "purge" => {
            let vault = open(arg(args, 2)?)?;
            // A forgotten memory can still be purged.
            let memory = stored_memory(&vault, &memory_id(arg(args, 3)?)?)?;
            let decision = delete_candidate(&vault, &memory, DeleteMode::Purge, scope_of(args))?;
            let shown = plan_and_confirm(&vault, &[decision])?;
            let done = complete_purge(
                &vault,
                &shown.ids[0].delete_id,
                &owner_of(&vault),
                shown.nonce.as_bytes(),
            )?;
            Ok(json!({
                "delete_id": shown.ids[0].delete_id,
                "receipt_id": done.receipt_id,
                "files_removed": done.files.removed.len(),
                "orphans_removed": done.files.orphans_removed.len(),
                "overall_state": "backup_purge_pending",
            }))
        }
        "deletion-ledger" => {
            let vault = open(arg(args, 2)?)?;
            let out = PathBuf::from(arg(args, 3)?);
            if out.starts_with(vault.managed_root().root()) {
                return Err(Failure::Usage(
                    "write the ledger outside the data root".into(),
                ));
            }
            let ledger = deletion_ledger(&vault)?;
            std::fs::write(&out, ledger_bytes(&ledger)).map_err(|e| Failure::Vault(e.into()))?;
            Ok(json!({"entries": ledger.len()}))
        }
        "reconcile" => {
            let vault = open(arg(args, 2)?)?;
            let bytes = std::fs::read(arg(args, 3)?).map_err(|e| Failure::Vault(e.into()))?;
            let ledger = parse_ledger(&bytes)?;
            let code = sha256(&bytes).as_str()[..8].to_owned();
            let ids: Vec<String> = ledger.iter().map(|t| t.delete_id.to_string()).collect();
            typed_code(
                json!({"ledger_entries": ledger.len(), "delete_ids": ids}),
                &code,
            )?;
            let done = reconcile_deletions(&vault, &ledger, &confirmation(&vault))?;
            Ok(json!({
                "applied": done.applied,
                "already": done.already,
                "not_present": done.not_present,
                "network_allowed": network_allowed(&vault)?,
            }))
        }
        "index" => {
            let vault = open(arg(args, 2)?)?;
            let mut index = Index::open(&vault)?;
            let report = index.update(&vault, &|| false)?;
            Ok(json!({
                "commits_applied": report.commits_applied,
                "revisions_indexed": report.revisions_indexed,
                "rows_purged": report.rows_purged,
                "sequence": index.watermark()?.map(|w| w.sequence),
            }))
        }
        "index-rebuild" => {
            let vault = open(arg(args, 2)?)?;
            let (index, report) = Index::rebuild(&vault)?;
            Ok(json!({
                "revisions_indexed": report.revisions_indexed,
                "sequence": index.watermark()?.map(|w| w.sequence),
            }))
        }
        "search" => {
            // The owner's own search on the trusted surface: the index is
            // brought to the head first (a lagging index is never mixed).
            let vault = open(arg(args, 2)?)?;
            let query = arg(args, 3)?;
            let mut index = Index::open(&vault)?;
            index.update(&vault, &|| false)?;
            let limit = match flag(args, "--limit") {
                Some(n) => Some(
                    n.parse::<u32>()
                        .map_err(|_| Failure::Usage("bad --limit".into()))?,
                ),
                None => None,
            };
            let request = SearchRequest {
                query: query.to_owned(),
                include_historical: has(args, "--historical"),
                cursor: flag(args, "--cursor").map(str::to_owned),
                limit,
                ..SearchRequest::default()
            };
            let page = search(&index, &vault, &owner_of(&vault), &request)?;
            let items: Vec<Value> = page
                .items
                .iter()
                .map(|h| {
                    json!({
                        "memory_id": h.memory_id, "revision": h.revision, "type": h.memory_type,
                        "project_id": h.project_id, "currency": h.currency, "snippet": h.snippet,
                        "evidence_count": h.evidence_count,
                    })
                })
                .collect();
            let projects: Vec<Value> = page
                .projects
                .iter()
                .map(|p| json!({"project_id": p.project_id, "name": p.display_name, "exact": p.exact}))
                .collect();
            Ok(json!({
                "items": items,
                "projects": projects,
                "next_cursor": page.next_cursor,
                "partial": page.partial,
                "ranking": enouia_memory_index::RANKING_VERSION,
            }))
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
        Err(Failure::NotConfirmed) => {
            println!("{}", json!({"error": "not_confirmed"}));
            ExitCode::from(3)
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

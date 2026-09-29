//! Backup exit (MV-1.4): a pinned-commit export, its independent
//! verification, and restore into a new, empty, verified root.
//!
//! The export is the plaintext tree that an encrypted backup tool stores
//! (PRIVACY_RECOVERY §4: restic, pinned version, owner-held secret). It holds
//! the descriptor, every stored commit on the chain and every catalog
//! segment they reference, every record revision the head catalogs (with
//! all earlier revisions), every object of the head, and sealed audit
//! segments. It never holds `CURRENT`, the lock, staging, orphans, the
//! journal, idempotency entries (rebuilt on restore), indexes, config, or
//! credentials. Records are immutable, so a pinned export is consistent
//! without stopping writers.
//!
//! A restore verifies the whole export before touching the target, refuses a
//! non-empty target (a bad snapshot never overwrites a healthy Vault), and
//! starts with network, Provider, MCP, and sync disabled until the owner
//! reconciles newer deletions (`config/restore-state.json`).

use crate::audit::AUDIT_DIR;
use crate::error::{Fault, Result, VaultError};
use crate::fault::Faults;
use crate::fs::ManagedRoot;
use crate::root::VerifiedRoot;
use crate::store::{Vault, VaultOptions};
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::catalog::{CatalogSegment, LayoutFormat, StoredCommit};
use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
use enouia_memory_contract::foundation::Clock;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::OperationId;
use enouia_memory_contract::json::{SchemaVersion, canonical_bytes};
use enouia_memory_contract::layout;
use enouia_memory_contract::ports::{CommitPin, IdSource};
use enouia_memory_contract::store::{
    CurrentPointer, EXPORT_MANIFEST_FILE, ExportFile, ExportManifest, IdempotencyEntry,
    PublishRecord, RecoveryEvidence, RecoveryReceipt, RestoreState, StoreDocument, VaultDescriptor,
    idempotency_scope_hash, is_safe_vault_path, parse_store,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::sync::Arc;

fn bad_export(what: &'static str) -> VaultError {
    let mut error = VaultError::new(MemoryErrorCode::StorageFailed, Fault::Corrupt(what));
    error.error.retryable = false;
    error
}

fn empty_dir(path: &Path) -> Result<bool> {
    Ok(std::fs::read_dir(path)?.next().is_none())
}

/// Export the pinned commit into `destination`, a verified, empty directory
/// outside the Vault root. Every copied file is verified against the catalog
/// or object hash first; the manifest is written last.
pub fn export_pinned(
    vault: &Vault,
    pin: &CommitPin,
    destination: &VerifiedRoot,
) -> Result<ExportManifest> {
    let source_root = vault.managed_root().root();
    if destination.path().starts_with(source_root) || source_root.starts_with(destination.path()) {
        return Err(VaultError::invalid(vec!["export.destination_overlaps"]));
    }
    if !empty_dir(destination.path())? {
        return Err(VaultError::invalid(vec!["export.destination_not_empty"]));
    }
    let head = vault.stored_commit(pin)?;
    let out = ManagedRoot::new(destination.path(), Faults::none());
    let mut files: BTreeMap<String, ExportFile> = BTreeMap::new();
    let copy =
        |files: &mut BTreeMap<String, ExportFile>, rel: String, bytes: Vec<u8>| -> Result<()> {
            out.write_new(&rel, &bytes)?;
            files.insert(
                rel.clone(),
                ExportFile {
                    path: rel,
                    sha256: sha256(&bytes),
                    size_bytes: bytes.len() as u64,
                },
            );
            Ok(())
        };
    let descriptor = vault
        .managed_root()
        .read(&layout::vault_descriptor())?
        .ok_or_else(|| VaultError::corrupt("vault descriptor"))?;
    copy(&mut files, layout::vault_descriptor(), descriptor)?;
    // The chain and its segments, every revision the head names, and every
    // object of the head; each verified against its catalog hash first.
    for pinned in vault.pinned_files(pin)? {
        let bytes = vault
            .managed_root()
            .read(&pinned.file)?
            .ok_or_else(|| VaultError::corrupt("pinned file missing"))?;
        if sha256(&bytes) != pinned.sha256 {
            return Err(VaultError::corrupt("pinned file hash"));
        }
        copy(&mut files, pinned.file, bytes)?;
    }
    let audit = vault.managed_root().list(AUDIT_DIR)?;
    if audit.len() > 1 {
        for segment in &audit[..audit.len() - 1] {
            let rel = format!("{AUDIT_DIR}/{segment}");
            if let Some(bytes) = vault.managed_root().read(&rel)? {
                copy(&mut files, rel, bytes)?;
            }
        }
    }
    let manifest_sha256 = files
        .get(&layout::commit_manifest(head.commit_id.as_str()))
        .map(|f| f.sha256.clone())
        .expect("head manifest exported");
    let export = ExportManifest {
        schema_version: SchemaVersion,
        export_format: LayoutFormat,
        vault_id: vault.vault_id().clone(),
        commit_id: head.commit_id.clone(),
        sequence: head.sequence,
        policy_epoch: head.policy_epoch,
        deletion_epoch: head.deletion_epoch,
        manifest_sha256,
        created_at: vault.now()?,
        files: files.into_values().collect(),
    };
    let violations = export.validate();
    if !violations.is_empty() {
        return Err(VaultError::invalid(
            violations.iter().map(|v| v.rule).collect(),
        ));
    }
    let bytes = canonical_bytes(&export).expect("export manifest serializes");
    std::fs::write(destination.path().join(EXPORT_MANIFEST_FILE), &bytes)?;
    // FlushFileBuffers needs a handle with write access.
    let file = std::fs::OpenOptions::new()
        .write(true)
        .open(destination.path().join(EXPORT_MANIFEST_FILE))?;
    file.sync_all()?;
    Ok(export)
}

fn walk(dir: &Path, base: &Path, out: &mut Vec<String>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let meta = std::fs::symlink_metadata(entry.path())?;
        if crate::platform::is_reparse_point(&meta) {
            return Err(bad_export("export reparse point"));
        }
        let rel = entry
            .path()
            .strip_prefix(base)
            .map_err(|_| bad_export("export path"))?
            .to_string_lossy()
            .replace('\\', "/");
        if meta.is_dir() {
            walk(&entry.path(), base, out)?;
        } else {
            out.push(rel);
        }
    }
    Ok(())
}

/// Verify an export completely without trusting it: manifest shape and
/// rules, exact file set, every hash and size, the commit chain back to
/// genesis, and every cataloged revision present with its catalog hash.
pub fn verify_export(export_dir: &Path) -> Result<ExportManifest> {
    let bytes = std::fs::read(export_dir.join(EXPORT_MANIFEST_FILE))
        .map_err(|_| bad_export("export manifest missing"))?;
    let manifest: ExportManifest =
        parse_store(&bytes).map_err(|_| bad_export("export manifest invalid"))?;
    let mut present = Vec::new();
    walk(export_dir, export_dir, &mut present)?;
    let listed: BTreeSet<&str> = manifest.files.iter().map(|f| f.path.as_str()).collect();
    for file in &present {
        if file != EXPORT_MANIFEST_FILE && !listed.contains(file.as_str()) {
            return Err(bad_export("export unlisted file"));
        }
    }
    let mut contents: BTreeMap<&str, Vec<u8>> = BTreeMap::new();
    for file in &manifest.files {
        if !is_safe_vault_path(&file.path) {
            return Err(bad_export("export path"));
        }
        let mut path = export_dir.to_path_buf();
        path.extend(file.path.split('/'));
        let bytes = std::fs::read(&path).map_err(|_| bad_export("export file missing"))?;
        if bytes.len() as u64 != file.size_bytes || sha256(&bytes) != file.sha256 {
            return Err(bad_export("export file hash"));
        }
        contents.insert(file.path.as_str(), bytes);
    }
    let descriptor: VaultDescriptor = parse_store(
        contents
            .get(layout::vault_descriptor().as_str())
            .ok_or_else(|| bad_export("export descriptor"))?,
    )
    .map_err(|_| bad_export("export descriptor"))?;
    if descriptor.vault_id != manifest.vault_id {
        return Err(bad_export("export vault id"));
    }
    let head_file = layout::commit_manifest(manifest.commit_id.as_str());
    let head_bytes = contents
        .get(head_file.as_str())
        .ok_or_else(|| bad_export("export head"))?;
    if sha256(head_bytes) != manifest.manifest_sha256 {
        return Err(bad_export("export head hash"));
    }
    let mut next = Some(manifest.commit_id.clone());
    let mut expected_sequence = manifest.sequence;
    let mut chain: Vec<StoredCommit> = Vec::new();
    let mut segments: BTreeMap<Sha256Hex, CatalogSegment> = BTreeMap::new();
    while let Some(id) = next {
        let bytes = contents
            .get(layout::commit_manifest(id.as_str()).as_str())
            .ok_or_else(|| bad_export("export chain gap"))?;
        let commit: StoredCommit =
            parse_store(bytes).map_err(|_| bad_export("export stored commit"))?;
        if commit.commit_id != id
            || commit.sequence != expected_sequence
            || commit.vault_id != manifest.vault_id
            || canonical_bytes(&commit).ok().as_deref() != Some(bytes.as_slice())
        {
            return Err(bad_export("export chain"));
        }
        if chain.is_empty()
            && (commit.policy_epoch != manifest.policy_epoch
                || commit.deletion_epoch != manifest.deletion_epoch)
        {
            return Err(bad_export("export epochs"));
        }
        for reference in commit.record_segments.iter().chain(&commit.object_segments) {
            if segments.contains_key(&reference.segment_hash) {
                continue;
            }
            let bytes = contents
                .get(layout::catalog_segment(&reference.segment_hash).as_str())
                .ok_or_else(|| bad_export("export segment missing"))?;
            let segment: CatalogSegment =
                parse_store(bytes).map_err(|_| bad_export("export segment"))?;
            if sha256(bytes) != reference.segment_hash
                || segment.segment_kind != reference.segment_kind
                || segment.record_kind != reference.record_kind
                || segment.prefix != reference.prefix
                || segment.len() as u64 != reference.entry_count
            {
                return Err(bad_export("export segment"));
            }
            segments.insert(reference.segment_hash.clone(), segment);
        }
        expected_sequence = expected_sequence.saturating_sub(1);
        next = commit.parent_commit_id.clone();
        if next.is_none()
            && (commit.sequence != 1 || descriptor.genesis_commit_id != commit.commit_id)
        {
            return Err(bad_export("export genesis"));
        }
        chain.push(commit);
    }
    // The head's complete view must be a valid manifest.
    chain[0]
        .materialize(|hash| segments.get(hash))
        .map_err(|_| bad_export("export head catalog"))?;
    // Purge tombstones in the export: what they name is intentionally absent.
    let mut tombstones = Vec::new();
    for segment in segments
        .values()
        .filter(|s| s.record_kind == Some(enouia_memory_contract::record::RecordKind::Tombstone))
    {
        for entry in &segment.records {
            let file = crate::store::entry_file(
                enouia_memory_contract::record::RecordKind::Tombstone,
                entry,
                1,
            )
            .ok_or_else(|| bad_export("export tombstone"))?;
            let bytes = contents
                .get(file.as_str())
                .ok_or_else(|| bad_export("export tombstone"))?;
            if sha256(bytes) != entry.content_hash {
                return Err(bad_export("export tombstone"));
            }
            let tombstone: enouia_memory_contract::commit::Tombstone =
                enouia_memory_contract::record::parse_record(bytes)
                    .map_err(|_| bad_export("export tombstone"))?;
            tombstones.push(tombstone);
        }
    }
    let purged = crate::purge::Purged::from_tombstones(&tombstones);
    // Every revision and object any exported segment names is present.
    let has = |file: Option<String>, hash: &Sha256Hex| {
        file.and_then(|f| contents.get(f.as_str()))
            .is_some_and(|bytes| &sha256(bytes) == hash)
    };
    for segment in segments.values() {
        if let Some(kind) = segment.record_kind {
            for entry in &segment.records {
                for revision in 1..=entry.revision.get() {
                    if purged.record(kind, &entry.record_id, revision) {
                        continue;
                    }
                    let hash = entry.hash_of(revision).expect("cataloged revision");
                    if !has(crate::store::entry_file(kind, entry, revision), hash) {
                        return Err(bad_export("export record missing"));
                    }
                }
            }
        }
    }
    for reference in &chain[0].object_segments {
        for item in &segments[&reference.segment_hash].objects {
            if purged.object(&item.object_hash) {
                continue;
            }
            if !has(crate::store::object_file_of(item), &item.object_hash) {
                return Err(bad_export("export object missing"));
            }
        }
    }
    Ok(manifest)
}

/// What a completed restore produced.
pub struct Restored {
    pub vault: Vault,
    pub state: RestoreState,
    pub receipt: RecoveryReceipt,
}

/// Restore a verified export into `target` (verified, empty). The export is
/// verified first; nothing is written to the target if it fails.
pub fn restore_export(
    export_dir: &Path,
    target: &VerifiedRoot,
    approved_by: &ActorRef,
    trusted_surface: TrustedSurface,
    clock: Arc<dyn Clock + Send + Sync>,
    ids: Arc<dyn IdSource + Send + Sync>,
    options: VaultOptions,
) -> Result<Restored> {
    if approved_by.actor_type != ActorType::Owner {
        return Err(VaultError::invalid(vec!["recovery.owner_required"]));
    }
    let export = verify_export(export_dir)?;
    if !empty_dir(target.path())? {
        return Err(VaultError::new(
            MemoryErrorCode::InvalidRequest,
            Fault::AlreadyInitialized,
        ));
    }
    let out = ManagedRoot::new(target.path(), Faults::none());
    for file in &export.files {
        let mut path = export_dir.to_path_buf();
        path.extend(file.path.split('/'));
        let bytes = std::fs::read(&path)?;
        if sha256(&bytes) != file.sha256 {
            return Err(bad_export("export changed during restore"));
        }
        out.write_new(&file.path, &bytes)?;
    }
    // Rebuild idempotency entries from the chain.
    let mut next = Some(export.commit_id.clone());
    while let Some(id) = next {
        let bytes = out
            .read(&layout::commit_manifest(id.as_str()))?
            .ok_or_else(|| bad_export("restored chain"))?;
        let commit: StoredCommit = parse_store(&bytes).map_err(|_| bad_export("restored chain"))?;
        if let Some(key) = &commit.idempotency_key_hash {
            let entry = IdempotencyEntry {
                schema_version: SchemaVersion,
                vault_id: export.vault_id.clone(),
                principal_id: commit.principal.actor_id.clone(),
                operation_kind: commit.operation_kind,
                key_hash: key.clone(),
                request_payload_hash: commit.request_payload_hash.clone(),
                commit_id: commit.commit_id.clone(),
                sequence: commit.sequence,
            };
            let file = layout::idempotency_entry(&idempotency_scope_hash(
                &entry.principal_id,
                entry.operation_kind,
                key,
            ));
            out.write_new(&file, &canonical_bytes(&entry).expect("entry"))?;
        }
        next = commit.parent_commit_id.clone();
    }
    let now = enouia_memory_contract::ports::commit_time(
        clock.as_ref(),
        None,
        enouia_memory_contract::foundation::ComponentId::Backup,
    )?;
    let receipt = RecoveryReceipt {
        schema_version: SchemaVersion,
        recovery_id: OperationId::from_random(ids.random_16()),
        vault_id: export.vault_id.clone(),
        adopted_commit_id: export.commit_id.clone(),
        adopted_sequence: export.sequence,
        manifest_sha256: export.manifest_sha256.clone(),
        previous_current_sha256: None,
        evidence: RecoveryEvidence::RestoredExport,
        approved_by: approved_by.clone(),
        trusted_surface,
        created_at: now.clone(),
    };
    out.write_new(
        &layout::recovery_receipt(receipt.recovery_id.as_str()),
        &canonical_bytes(&receipt).expect("receipt"),
    )?;
    let state = RestoreState {
        schema_version: SchemaVersion,
        vault_id: export.vault_id.clone(),
        restored_commit_id: export.commit_id.clone(),
        export_manifest_sha256: sha256(&std::fs::read(export_dir.join(EXPORT_MANIFEST_FILE))?),
        restored_at: now.clone(),
        network_disabled_until_reconciled: true,
        reconciled_at: None,
    };
    out.write_new(
        &layout::restore_state(),
        &canonical_bytes(&state).expect("state"),
    )?;
    let pointer = CurrentPointer {
        schema_version: SchemaVersion,
        vault_id: export.vault_id.clone(),
        commit_id: export.commit_id.clone(),
        sequence: export.sequence,
        manifest_sha256: export.manifest_sha256.clone(),
    };
    out.write_atomic(
        &layout::current_pointer(),
        &canonical_bytes(&pointer).expect("pointer"),
    )?;
    let journal = PublishRecord {
        schema_version: SchemaVersion,
        vault_id: export.vault_id.clone(),
        commit_id: export.commit_id.clone(),
        sequence: export.sequence,
        manifest_sha256: export.manifest_sha256.clone(),
        published_at: now,
    };
    out.append_line(
        &layout::publish_journal(),
        &serde_json::to_vec(&journal).expect("line"),
    )?;
    let vault = Vault::open(target, None, clock, ids, options)?;
    let pin = vault.pin_current()?;
    let report = vault.verify(&pin)?;
    if !report.is_clean() {
        return Err(bad_export("restored commit incomplete"));
    }
    Ok(Restored {
        vault,
        state,
        receipt,
    })
}

/// Whether a restored Vault may use network, Provider, MCP, or sync yet.
pub fn network_allowed(vault: &Vault) -> Result<bool> {
    match vault.managed_root().read(&layout::restore_state())? {
        None => Ok(true),
        Some(bytes) => {
            let state: RestoreState =
                parse_store(&bytes).map_err(|_| VaultError::corrupt("restore state"))?;
            Ok(!state.network_disabled_until_reconciled && state.reconciled_at.is_some())
        }
    }
}

/// Record that the owner reconciled a restored Vault with the deletions and
/// revocations made after its export (B02); network use may resume. A Vault
/// that was not restored has nothing to reconcile.
pub fn mark_reconciled(vault: &Vault) -> Result<()> {
    let Some(bytes) = vault.managed_root().read(&layout::restore_state())? else {
        return Ok(());
    };
    let mut state: RestoreState =
        parse_store(&bytes).map_err(|_| VaultError::corrupt("restore state"))?;
    state.reconciled_at = Some(vault.now()?);
    state.network_disabled_until_reconciled = false;
    let violations = state.validate();
    if !violations.is_empty() {
        return Err(VaultError::invalid(
            violations.iter().map(|v| v.rule).collect(),
        ));
    }
    vault.managed_root().write_atomic(
        &layout::restore_state(),
        &canonical_bytes(&state).expect("state"),
    )
}

/// Hash of an export manifest file, as recorded in a restore state.
pub fn export_manifest_hash(export_dir: &Path) -> Result<Sha256Hex> {
    Ok(sha256(&std::fs::read(
        export_dir.join(EXPORT_MANIFEST_FILE),
    )?))
}

//! Post-import reconciliation (IMPORT_REVIEW §2.7, §7; MV-2.4): count
//! closure and a full re-derivation of every source the import wrote from the
//! archived bytes. Read-only; works the same on a restored Vault, which is
//! how backup/restore source location is checked.

use crate::locate::resolve;
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::{ImportId, SourceId};
use enouia_memory_contract::import::{ImportManifest, ImportStatus};
use enouia_memory_contract::parse_record;
use enouia_memory_contract::ports::CommitPin;
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_contract::source::SourceRecord;
use enouia_memory_vault::{Fault, Vault, VaultError};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportAudit {
    pub import_id: ImportId,
    pub status: ImportStatus,
    pub raw_present: bool,
    /// Distinct sources with at least one revision citing this import.
    pub sources_citing: u64,
    /// Source revisions citing this import that were re-derived.
    pub revisions_checked: u64,
    /// Sum of the manifest's per-conversation `message_count`.
    pub coverage_total: u64,
    /// `sources_created + sources_revised` from the manifest counts.
    pub counted_written: u64,
    pub mismatched: Vec<SourceId>,
    pub unresolvable: Vec<SourceId>,
}

impl ImportAudit {
    /// Everything reconciles: bytes present, every cited revision resolves
    /// to its content hash, and (when completed) the counts close.
    pub fn is_consistent(&self) -> bool {
        self.raw_present
            && self.mismatched.is_empty()
            && self.unresolvable.is_empty()
            && (self.status != ImportStatus::Completed
                || (self.coverage_total == self.sources_citing
                    && self.counted_written == self.sources_citing))
    }
}

/// Every commit on the published chain, head first.
fn chain(vault: &Vault) -> Result<Vec<CommitPin>, VaultError> {
    let mut pins = vec![vault.pin_current()?];
    loop {
        let manifest = vault.read_manifest(pins.last().expect("non-empty"))?;
        let Some(parent) = manifest.parent_commit_id else {
            break;
        };
        pins.push(CommitPin {
            commit_id: parent,
            sequence: manifest.sequence - 1,
            policy_epoch: 0,
            deletion_epoch: 0,
        });
    }
    Ok(pins)
}

pub fn audit_import(vault: &Vault, import_id: &ImportId) -> Result<ImportAudit, VaultError> {
    let pins = chain(vault)?;
    let head = vault.read_manifest(&pins[0])?;
    let entry = head
        .catalog
        .iter()
        .find(|e| e.record_kind == RecordKind::Import && e.record_id == import_id.as_str())
        .ok_or_else(|| VaultError::new(MemoryErrorCode::NotFound, Fault::NotFound))?;
    let manifest: ImportManifest = parse_record(&vault.read_record(
        &pins[0],
        &RecordRef::new(RecordKind::Import, import_id.as_str(), entry.revision),
    )?)
    .map_err(|_| VaultError::corrupt("import"))?;
    let raw = vault
        .read_object(&pins[0], &manifest.input_object_hash)
        .ok();
    let mut audit = ImportAudit {
        import_id: import_id.clone(),
        status: manifest.status,
        raw_present: raw
            .as_ref()
            .is_some_and(|b| sha256(b) == manifest.input_object_hash),
        sources_citing: 0,
        revisions_checked: 0,
        coverage_total: manifest.coverage.iter().map(|c| c.message_count).sum(),
        counted_written: manifest.counts.sources_created + manifest.counts.sources_revised,
        mismatched: Vec::new(),
        unresolvable: Vec::new(),
    };
    let mut seen = BTreeSet::new();
    let mut citing = BTreeSet::new();
    for pin in &pins {
        let commit = vault.read_manifest(pin)?;
        for e in commit
            .catalog
            .iter()
            .filter(|e| e.record_kind == RecordKind::Source && e.changed)
        {
            if !seen.insert((e.record_id.clone(), e.revision)) {
                continue;
            }
            let reference = RecordRef::new(e.record_kind, &e.record_id, e.revision);
            let source: SourceRecord = parse_record(&vault.read_record(pin, &reference)?)
                .map_err(|_| VaultError::corrupt("source"))?;
            if source.import_id.as_ref() != Some(import_id) {
                continue;
            }
            citing.insert(source.source_id.clone());
            audit.revisions_checked += 1;
            match raw.as_deref().map(|r| resolve(r, &source.locator)) {
                Some(Ok(content)) if sha256(&content) == source.content_hash => {}
                Some(Ok(_)) => audit.mismatched.push(source.source_id.clone()),
                _ => audit.unresolvable.push(source.source_id.clone()),
            }
        }
    }
    audit.sources_citing = citing.len() as u64;
    Ok(audit)
}

//! Owner-invoked sweep of unreferenced files (MV-1.2, "校验后清理孤儿").
//!
//! A writer that crashed before publishing may leave record revisions,
//! objects, a manifest, an idempotency entry, or an atomic-replace temporary
//! file behind. None of them is ever read (readers follow `CURRENT` and the
//! catalog), but they take space and confuse manual inspection. The sweep
//! runs under the writer lock, only while `CURRENT` verifies, and moves every
//! file that the published chain does not reference into
//! `vault/orphans/<sweep_id>/`. It never deletes, and it never touches
//! records, audit, journal, recovery receipts, or anything outside `vault/`.

use crate::error::Result;
use crate::store::Vault;
use enouia_memory_contract::ids::OperationId;
use enouia_memory_contract::layout;
use enouia_memory_contract::record::RecordRef;
use enouia_memory_contract::store::{IdempotencyEntry, parse_store};
use std::collections::BTreeSet;

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct SweepReport {
    pub sweep_id: Option<OperationId>,
    pub files_checked: usize,
    /// Managed names moved to `vault/orphans/<sweep_id>/`.
    pub quarantined: Vec<String>,
}

/// Directories whose files must all be referenced by the published chain.
const SWEPT: &[&str] = &[
    "vault/records",
    "vault/session-events",
    "vault/raw/objects",
    "vault/assets/objects",
    "vault/session-content/objects",
    "vault/commits",
    "vault/idempotency",
];

impl Vault {
    fn files_under(&self, dir: &str, out: &mut Vec<String>) -> Result<()> {
        for name in self.managed_root().list(dir)? {
            let rel = format!("{dir}/{name}");
            let mut path = self.managed_root().root().to_path_buf();
            path.extend(rel.split('/'));
            if path.is_dir() {
                self.files_under(&rel, out)?;
            } else {
                out.push(rel);
            }
        }
        Ok(())
    }

    /// Every managed file the published chain references.
    fn referenced_files(&self) -> Result<BTreeSet<String>> {
        let pin = self.pin_current()?;
        let head = self.read_manifest(&pin)?;
        let mut keep = BTreeSet::new();
        let mut current = head.clone();
        loop {
            keep.insert(layout::commit_manifest(current.commit_id.as_str()));
            for entry in &current.catalog {
                let reference = RecordRef::new(entry.record_kind, &entry.record_id, entry.revision);
                keep.insert(self.stored_file(&reference)?);
                if entry.record_kind == enouia_memory_contract::record::RecordKind::Identity
                    && let Some(markdown) =
                        layout::record_path(entry.record_kind, &entry.record_id, entry.revision)
                {
                    keep.insert(markdown);
                }
            }
            let Some(parent) = current.parent_commit_id.clone() else {
                break;
            };
            let at = enouia_memory_contract::ports::CommitPin {
                commit_id: parent,
                sequence: current.sequence - 1,
                policy_epoch: 0,
                deletion_epoch: 0,
            };
            current = self.read_manifest(&at)?;
        }
        for object in &head.objects {
            keep.insert(self.object_file_for(&head, object)?);
        }
        for name in self.managed_root().list("vault/idempotency")? {
            let rel = format!("vault/idempotency/{name}");
            let Some(bytes) = self.managed_root().read(&rel)? else {
                continue;
            };
            if let Ok(entry) = parse_store::<IdempotencyEntry>(&bytes)
                && self
                    .find_receipt(&enouia_memory_contract::ports::IdempotencyScope {
                        principal_id: entry.principal_id.clone(),
                        operation_kind: entry.operation_kind,
                        key_hash: entry.key_hash.clone(),
                    })?
                    .is_some_and(|(commit_id, _, _)| commit_id == entry.commit_id)
            {
                keep.insert(rel);
            }
        }
        Ok(keep)
    }

    /// Quarantine unreferenced files and interrupted atomic-replace
    /// temporaries. Refused while `CURRENT` cannot be verified, because
    /// unpublished manifests are then recovery evidence.
    pub fn sweep_unreferenced(&self) -> Result<SweepReport> {
        let _guard = self.writer_guard()?;
        let keep = self.referenced_files()?;
        let mut candidates = Vec::new();
        for dir in SWEPT {
            self.files_under(dir, &mut candidates)?;
        }
        for name in self.managed_root().list("vault")? {
            if name.contains(".tmp-") {
                candidates.push(format!("vault/{name}"));
            }
        }
        let mut report = SweepReport {
            files_checked: candidates.len(),
            ..SweepReport::default()
        };
        let orphans: Vec<String> = candidates
            .into_iter()
            .filter(|f| !keep.contains(f))
            .collect();
        if orphans.is_empty() {
            return Ok(report);
        }
        let sweep_id = OperationId::from_random(self.random_id_bytes());
        for file in orphans {
            let target = format!(
                "{}/{}/{}",
                layout::orphans_dir(),
                sweep_id,
                file.replace('/', "_")
            );
            self.managed_root().rename(&file, &target)?;
            report.quarantined.push(file);
        }
        report.sweep_id = Some(sweep_id);
        Ok(report)
    }
}

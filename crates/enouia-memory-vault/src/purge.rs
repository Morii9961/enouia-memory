//! Intentional purge (MV-3.4, PRIVACY_RECOVERY §6).
//!
//! A purge is decided by a published tombstone with mode `purge`: its
//! targets (record IDs, optionally one revision) and object hashes name
//! exactly what is destroyed. The catalog keeps the hashes; from then on
//! every read of that content returns `intentionally_purged`, verification
//! and exports count it as purged instead of missing, sweeps leave it to
//! the purge, and commit validation treats it as absent. `purge_files`
//! deletes the bytes (and any quarantined copy) under the writer lock; it is
//! idempotent, so an interrupted purge is finished by running it again.

use crate::error::{Fault, Result, VaultError};
use crate::store::{Vault, entry_file, object_file_of};
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::catalog::StoredCommit;
use enouia_memory_contract::commit::{DeleteMode, Tombstone};
use enouia_memory_contract::hash::Sha256Hex;
use enouia_memory_contract::json::Revision;
use enouia_memory_contract::layout;
use enouia_memory_contract::record::{AnyRecord, RecordKind};
use std::collections::BTreeSet;

/// What purge tombstones cover.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Purged {
    whole: BTreeSet<(RecordKind, String)>,
    revisions: BTreeSet<(RecordKind, String, u64)>,
    objects: BTreeSet<Sha256Hex>,
}

impl Purged {
    pub fn record(&self, kind: RecordKind, id: &str, revision: u64) -> bool {
        self.whole.contains(&(kind, id.to_owned()))
            || self.revisions.contains(&(kind, id.to_owned(), revision))
    }

    pub fn object(&self, hash: &Sha256Hex) -> bool {
        self.objects.contains(hash)
    }

    pub fn is_empty(&self) -> bool {
        self.whole.is_empty() && self.revisions.is_empty() && self.objects.is_empty()
    }

    /// The purge coverage of these tombstones (other modes are ignored).
    pub fn from_tombstones<'a>(tombstones: impl IntoIterator<Item = &'a Tombstone>) -> Self {
        let mut purged = Self::default();
        for tombstone in tombstones {
            if tombstone.mode == DeleteMode::Purge {
                purged.add(tombstone);
            }
        }
        purged
    }

    fn add(&mut self, tombstone: &Tombstone) {
        for target in &tombstone.targets {
            match target.revision {
                None => {
                    self.whole
                        .insert((target.record_kind, target.record_id.clone()));
                }
                Some(revision) => {
                    self.revisions.insert((
                        target.record_kind,
                        target.record_id.clone(),
                        revision.get(),
                    ));
                }
            }
        }
        self.objects.extend(tombstone.object_hashes.iter().cloned());
    }

    /// Does a quarantined file name carry purged content?
    fn names(&self, name: &str) -> bool {
        self.whole.iter().any(|(_, id)| name.contains(id.as_str()))
            || self
                .revisions
                .iter()
                .any(|(_, id, _)| name.contains(id.as_str()))
            || self.objects.iter().any(|h| name.contains(h.as_str()))
    }
}

pub fn purged_error() -> VaultError {
    let mut error = VaultError::new(
        MemoryErrorCode::IntentionallyPurged,
        Fault::Corrupt("intentionally purged"),
    );
    error.error.retryable = false;
    error
}

/// What `purge_files` removed.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PurgeReport {
    /// Managed files of purged content that were deleted.
    pub removed: Vec<String>,
    /// Quarantined copies (in `vault/orphans/`) that were deleted.
    pub orphans_removed: Vec<String>,
}

impl Vault {
    fn purge_tombstones(&self, commit: &StoredCommit) -> Result<Vec<Tombstone>> {
        let mut out = Vec::new();
        for entry in self.entries_of(commit, RecordKind::Tombstone)? {
            if let AnyRecord::Tombstone(tombstone) =
                self.record_at(RecordKind::Tombstone, &entry, 1)?
                && tombstone.mode == DeleteMode::Purge
            {
                out.push(tombstone);
            }
        }
        Ok(out)
    }

    /// Everything purged as of `commit` or of the head (a purge reaches back
    /// into every earlier commit's view).
    pub fn purged_for(&self, commit: &StoredCommit) -> Result<Purged> {
        let mut purged = Purged::default();
        for tombstone in self.purge_tombstones(commit)? {
            purged.add(&tombstone);
        }
        if let Ok(head) = self.load_head()
            && head.commit_id != commit.commit_id
        {
            for tombstone in self.purge_tombstones(&head)? {
                purged.add(&tombstone);
            }
        }
        Ok(purged)
    }

    /// Everything purged at the head.
    pub fn purged(&self) -> Result<Purged> {
        let head = self.load_head()?;
        self.purged_for(&head)
    }

    /// Managed files that hold purged content at `commit`.
    pub(crate) fn purged_files(
        &self,
        commit: &StoredCommit,
        purged: &Purged,
    ) -> Result<BTreeSet<String>> {
        let mut files = BTreeSet::new();
        if purged.is_empty() {
            return Ok(files);
        }
        for reference in &commit.record_segments {
            let kind = reference.record_kind.expect("record segment");
            for entry in &self.segment(reference)?.records {
                for revision in 1..=entry.revision.get() {
                    if !purged.record(kind, &entry.record_id, revision) {
                        continue;
                    }
                    if let Some(file) = entry_file(kind, entry, revision) {
                        files.insert(file);
                    }
                    if kind == RecordKind::Identity
                        && let Some(markdown) = layout::record_path(
                            kind,
                            &entry.record_id,
                            Revision::new(revision).expect("revision"),
                        )
                    {
                        files.insert(markdown);
                    }
                }
            }
        }
        for item in self.objects_of(commit)? {
            if purged.object(&item.object_hash)
                && let Some(file) = object_file_of(&item)
            {
                files.insert(file);
            }
        }
        Ok(files)
    }

    fn orphan_files(&self, dir: &str, out: &mut Vec<String>) -> Result<()> {
        for name in self.managed_root().list(dir)? {
            let rel = format!("{dir}/{name}");
            let mut path = self.managed_root().root().to_path_buf();
            path.extend(rel.split('/'));
            if path.is_dir() {
                self.orphan_files(&rel, out)?;
            } else {
                out.push(rel);
            }
        }
        Ok(())
    }

    /// Delete the bytes of everything the published purge tombstones name,
    /// and every quarantined copy of it. Runs under the writer lock and only
    /// while `CURRENT` verifies. Safe to repeat.
    pub fn purge_files(&self) -> Result<PurgeReport> {
        let _guard = self.writer_guard()?;
        let head = self.load_head()?;
        let purged = self.purged_for(&head)?;
        let mut report = PurgeReport::default();
        for file in self.purged_files(&head, &purged)? {
            if self.managed_root().remove_purged(&file)? {
                report.removed.push(file);
            }
        }
        let mut orphans = Vec::new();
        self.orphan_files(&layout::orphans_dir(), &mut orphans)?;
        for file in orphans {
            let name = file.rsplit('/').next().unwrap_or_default();
            if purged.names(name) && self.managed_root().remove_purged(&file)? {
                report.orphans_removed.push(file);
            }
        }
        Ok(report)
    }
}

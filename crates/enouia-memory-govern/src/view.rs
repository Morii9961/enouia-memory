//! The canonical view: the latest revision of every approved memory at a
//! pinned commit, minus tombstoned ones. Candidates, whatever their status,
//! are never part of it (M01). Reading an older pin gives what the system
//! knew then (`known_at`, M06). Relevance ranking is MV-4.

use crate::util::{Result, all_latest};
use enouia_memory_contract::commit::Tombstone;
use enouia_memory_contract::memory::{CanonicalMemory, MemoryStatus};
use enouia_memory_contract::ports::CommitPin;
use enouia_memory_contract::record::RecordKind;
use enouia_memory_vault::Vault;

/// Which memories a caller wants from the view.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Canonical {
    /// Status `active` only: the default for retrieval and model requests.
    Active,
    /// Every status (history views). Tombstoned records stay excluded.
    AllStatuses,
}

pub fn canonical_memories(
    vault: &Vault,
    pin: &CommitPin,
    which: Canonical,
) -> Result<Vec<CanonicalMemory>> {
    let tombstones: Vec<Tombstone> = all_latest(vault, pin, RecordKind::Tombstone)?;
    let memories: Vec<CanonicalMemory> = all_latest(vault, pin, RecordKind::Memory)?;
    let mut out: Vec<CanonicalMemory> = memories
        .into_iter()
        .filter(|m| which == Canonical::AllStatuses || m.status == MemoryStatus::Active)
        .filter(|m| !tombstoned(&tombstones, m))
        .collect();
    out.sort_by(|a, b| a.memory_id.as_str().cmp(b.memory_id.as_str()));
    Ok(out)
}

/// A tombstone covers this memory revision (or every revision of it).
pub fn tombstoned(tombstones: &[Tombstone], memory: &CanonicalMemory) -> bool {
    tombstones.iter().any(|t| {
        t.targets.iter().any(|target| {
            target.record_kind == RecordKind::Memory
                && target.record_id == memory.memory_id.as_str()
                && target.revision.is_none_or(|r| r == memory.revision)
        })
    })
}

//! Minimal host ports owned by Memory. Any host (the Enouia Runtime Windows
//! client, a CLI, a future Memory Host) implements these in its own adapter
//! layer. Memory never depends on a host's internal crates, so this repository
//! builds and tests without any other checkout.

use crate::error::MemoryError;
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::atomic::{AtomicI64, Ordering};

/// Milliseconds since the Unix epoch, UTC. Business time is always injected.
pub trait Clock {
    fn now_unix_ms(&self) -> i64;
}

/// Controllable clock for deterministic tests.
#[derive(Debug)]
pub struct FakeClock {
    now_ms: AtomicI64,
}

impl FakeClock {
    pub const fn new(now_ms: i64) -> Self {
        Self {
            now_ms: AtomicI64::new(now_ms),
        }
    }

    pub fn set(&self, now_ms: i64) {
        self.now_ms.store(now_ms, Ordering::SeqCst);
    }
}

impl Clock for FakeClock {
    fn now_unix_ms(&self) -> i64 {
        self.now_ms.load(Ordering::SeqCst)
    }
}

/// Cooperative cancellation for long operations and Provider calls.
pub trait Cancellation {
    fn is_cancelled(&self) -> bool;
}

/// OS-level, process-exclusive Vault writer lock. The guard owns the lock until
/// dropped. An in-process mutex or the mere existence of a lock file is not
/// ownership; a live lock is never broken by age or PID guesses.
pub trait WriterLock {
    type Guard;

    fn try_acquire(&self, lock_path: &Path) -> Result<Self::Guard, MemoryError>;
}

/// Stage, flush, and atomically replace one managed file on the same volume.
/// A single-file primitive only: several calls are not a transaction (the
/// transaction is the commit catalog plus one `CURRENT`, see ADR-MEM-04/21).
pub trait AtomicFile {
    fn read(&self, path: &Path) -> Result<Option<Vec<u8>>, MemoryError>;
    fn replace_durable(&self, path: &Path, bytes: &[u8]) -> Result<(), MemoryError>;
}

/// Memory health/error components. A host maps these onto its own health DTOs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentId {
    Core,
    Vault,
    MemoryIndex,
    Context,
    Session,
    Provider,
    Importer,
    Backup,
    Audit,
    Policy,
}

impl ComponentId {
    pub const ALL: [Self; 10] = [
        Self::Core,
        Self::Vault,
        Self::MemoryIndex,
        Self::Context,
        Self::Session,
        Self::Provider,
        Self::Importer,
        Self::Backup,
        Self::Audit,
        Self::Policy,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fake_clock_can_advance_and_regress() {
        let clock = FakeClock::new(10);
        assert_eq!(clock.now_unix_ms(), 10);
        clock.set(-5);
        assert_eq!(clock.now_unix_ms(), -5);
    }
}

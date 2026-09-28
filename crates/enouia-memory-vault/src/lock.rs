//! OS-level single-writer lock (MV-1.1, V01). The lock is an exclusive
//! `LockFileEx` (via `File::try_lock`) on `vault/LOCK`, held by an open
//! handle: the OS releases it when the owning process exits for any reason,
//! so a crashed writer never leaves a stale lock, and a live lock is never
//! broken by age or PID guesses. The file's existence means nothing.

use crate::error::{Fault, Result, VaultError};
use crate::fs::ManagedRoot;
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::layout;
use std::fs::{File, TryLockError};
use std::time::{Duration, Instant};

/// Owns the writer lock until dropped.
#[derive(Debug)]
pub struct WriterGuard {
    _file: File,
}

/// Acquire the Vault writer lock, waiting at most `wait` (zero = one try).
pub fn acquire(root: &ManagedRoot, wait: Duration) -> Result<WriterGuard> {
    let file = root.open_lock_file(&layout::lock_file())?;
    let deadline = Instant::now() + wait;
    loop {
        match file.try_lock() {
            Ok(()) => return Ok(WriterGuard { _file: file }),
            Err(TryLockError::WouldBlock) => {
                if Instant::now() >= deadline {
                    return Err(VaultError::new(MemoryErrorCode::Busy, Fault::WriterBusy));
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(TryLockError::Error(e)) => return Err(e.into()),
        }
    }
}

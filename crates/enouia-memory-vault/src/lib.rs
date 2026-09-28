//! Enouia Memory Vault store (MV-1): data root verification, managed file
//! operations, the OS single-writer lock, and the transactional commit /
//! recovery protocol over the contracts in `enouia-memory-contract`.
//!
//! Scope: synthetic data in isolated roots. Nothing here creates the default
//! data root, reads real memories, or talks to a network, model, or sync
//! service. Durability claims are limited to process crashes (V09).

pub mod audit;
pub mod backup;
pub mod error;
pub mod fault;
pub mod fs;
pub mod health;
pub mod lock;
pub mod platform;
pub mod restic;
pub mod root;
pub mod service;
pub mod store;
pub mod sweep;

pub use error::{Fault, VaultError};
pub use root::{RootPolicy, RootRejection, VerifiedRoot, verify_data_root};
pub use store::{GenesisRequest, Vault, VaultOptions};

use enouia_memory_contract::foundation::Clock;
use enouia_memory_contract::ports::IdSource;

/// IDs from the operating system CSPRNG.
#[derive(Clone, Copy, Debug, Default)]
pub struct OsIdSource;

impl IdSource for OsIdSource {
    fn random_16(&self) -> [u8; 16] {
        platform::random_bytes::<16>().expect("operating system RNG unavailable")
    }
}

/// Wall clock (UTC milliseconds). Commit order never relies on it alone:
/// a clock earlier than the head pauses writes with `clock_regression`.
#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_unix_ms(&self) -> i64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }
}

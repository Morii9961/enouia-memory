//! Vault health (MV-1.3), in the Runtime vocabulary
//! `Healthy / Degraded / Unavailable / Recovering`. Reading health never
//! writes, repairs, or advances anything, and reports no paths or content.

use crate::error::Fault;
use crate::store::{CurrentState, Vault};
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::ids::CommitId;
use enouia_memory_contract::layout;
use enouia_memory_contract::store::{PublishRecord, parse_store};
use enouia_memory_contract::time::Timestamp;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthState {
    Healthy,
    Degraded,
    Unavailable,
    Recovering,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HealthNote {
    /// Unpublished staging from an interrupted writer (cleared by the next commit).
    StagingLeftover,
    /// The publish journal is behind `CURRENT` (recovery evidence is missing).
    JournalBehind,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultHealth {
    pub state: HealthState,
    pub error_code: Option<MemoryErrorCode>,
    pub head_commit_id: Option<CommitId>,
    pub head_sequence: Option<u64>,
    pub last_commit_at: Option<Timestamp>,
    pub policy_epoch: Option<u64>,
    pub deletion_epoch: Option<u64>,
    pub notes: Vec<HealthNote>,
}

impl VaultHealth {
    fn without_head(state: HealthState, code: MemoryErrorCode) -> Self {
        Self {
            state,
            error_code: Some(code),
            head_commit_id: None,
            head_sequence: None,
            last_commit_at: None,
            policy_epoch: None,
            deletion_epoch: None,
            notes: Vec::new(),
        }
    }
}

impl Vault {
    pub fn health(&self) -> VaultHealth {
        let pin = match self.pin_current() {
            Ok(pin) => pin,
            Err(error) => {
                let recovering = matches!(error.fault, Fault::Recovering(_) | Fault::Corrupt(_))
                    || self
                        .recovery_report()
                        .is_ok_and(|r| r.current != CurrentState::Valid);
                return if recovering {
                    VaultHealth::without_head(
                        HealthState::Recovering,
                        MemoryErrorCode::VaultRecovering,
                    )
                } else {
                    VaultHealth::without_head(HealthState::Unavailable, error.code())
                };
            }
        };
        let manifest = match self.stored_commit(&pin) {
            Ok(manifest) => manifest,
            Err(error) => {
                return VaultHealth::without_head(HealthState::Unavailable, error.code());
            }
        };
        let mut notes = Vec::new();
        let root = self.managed_root();
        if root.list("vault/staging").is_ok_and(|s| !s.is_empty()) {
            notes.push(HealthNote::StagingLeftover);
        }
        let journal_head = root
            .read(&layout::publish_journal())
            .ok()
            .flatten()
            .and_then(|bytes| {
                bytes
                    .split(|&b| b == b'\n')
                    .filter_map(|l| parse_store::<PublishRecord>(l).ok())
                    .map(|r| r.sequence)
                    .max()
            });
        if journal_head.is_none_or(|s| s < pin.sequence) {
            notes.push(HealthNote::JournalBehind);
        }
        VaultHealth {
            state: if notes.is_empty() {
                HealthState::Healthy
            } else {
                HealthState::Degraded
            },
            error_code: None,
            head_commit_id: Some(pin.commit_id.clone()),
            head_sequence: Some(pin.sequence),
            last_commit_at: Some(manifest.created_at.clone()),
            policy_epoch: Some(pin.policy_epoch),
            deletion_epoch: Some(pin.deletion_epoch),
            notes,
        }
    }
}

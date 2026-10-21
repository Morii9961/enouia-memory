//! Opt-in Windows Host/data publication. Native identity is injected by a
//! trusted caller; this library is not an authenticated client transport.
//!
//! Immutable authority bytes are selected by the SAME atomic CURRENT as data.
//! A durable publication intent is written first; interrupted adoption fails closed.
//! Never silently adopt the earlier authority-only pilot or a legacy backup.

use crate::error::{Result, VaultError};
use crate::host_authority::{AUTHORITY_FILE, ControlChange};
use crate::platform;
use crate::{RootPolicy, Vault, VerifiedRoot, verify_data_root};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::host::{
    AuthorityState, Checkpoint, HostFault, MAX_HOST_DOCUMENT_BYTES, parse_document,
};
use enouia_memory_contract::ids::DeviceId;
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::layout;
use enouia_memory_contract::ports::{CommitOutcome, CommitPin, CommitRequest};
use enouia_memory_contract::store::{CurrentPointer, VaultDescriptor, parse_store};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs::{File, TryLockError};

pub const MODE_FILE: &str = "config/host-mode.json";
const AUTHORITY_DIR: &str = "vault/host-authority";

fn authority_file(hash: &Sha256Hex) -> String {
    format!("{AUTHORITY_DIR}/{hash}.json")
}

impl Vault {
    /// Legacy mutations must check while holding writer exclusion. A marker,
    /// an authority directory or an authority-bearing CURRENT is sufficient to
    /// refuse, even if another part of interrupted Host adoption is missing.
    pub(crate) fn require_legacy_mode(&self) -> Result<()> {
        let root = self.managed_root();
        if root.exists(MODE_FILE)?
            || root.exists(crate::host_recovery::INTENT_FILE)?
            || root.exists(crate::host_recovery::WITNESS_FILE)?
            || root.exists(AUTHORITY_DIR)?
            || root.read(&layout::current_pointer())?.is_some_and(|bytes| {
                // Inspect presence, including malformed/null claimed values.
                serde_json::from_slice::<serde_json::Value>(&bytes)
                    .ok()
                    .is_some_and(|v| v.get("host_authority_hash").is_some())
            })
        {
            return Err(HostFault::Fenced.into());
        }
        Ok(())
    }

    /// Verify the complete immutable data closure of one pinned commit, not
    /// merely its manifest. The caller MUST own writer exclusion throughout.
    /// Re-read every actual file, even when catalog metadata is cached.
    pub(crate) fn checkpoint_for_locked(&self, pin: &CommitPin) -> Result<Checkpoint> {
        let descriptor = self
            .managed_root()
            .read(&layout::vault_descriptor())?
            .ok_or(HostFault::InvalidState)?;
        let parsed: VaultDescriptor =
            parse_store(&descriptor).map_err(|_| HostFault::InvalidState)?;
        if &parsed != self.descriptor() {
            return Err(HostFault::Fenced.into());
        }
        let mut inventory = BTreeMap::new();
        inventory.insert(
            layout::vault_descriptor(),
            json!({
                "sha256": sha256(&descriptor), "size_bytes": descriptor.len(),
            }),
        );
        for file in self.pinned_files(pin)? {
            let bytes = self
                .managed_root()
                .read(&file.file)?
                .ok_or(HostFault::CheckpointMismatch)?;
            if sha256(&bytes) != file.sha256 {
                return Err(HostFault::CheckpointMismatch.into());
            }
            inventory.insert(
                file.file,
                json!({
                    "sha256": file.sha256, "size_bytes": bytes.len(),
                }),
            );
        }
        let bytes = canonical_bytes(&json!({
            "snapshot_format": "host-data-closure-v1",
            "vault_id": self.vault_id(),
            "commit_id": pin.commit_id,
            "sequence": pin.sequence,
            "policy_epoch": pin.policy_epoch,
            "deletion_epoch": pin.deletion_epoch,
            "files": inventory,
        }))
        .map_err(|_| HostFault::InvalidState)?;
        Ok(Checkpoint {
            commit_id: pin.commit_id.clone(),
            commit_sequence: Revision::new(pin.sequence).ok_or(HostFault::CounterExhausted)?,
            snapshot_hash: sha256(&bytes),
            policy_epoch: pin.policy_epoch,
            deletion_epoch: pin.deletion_epoch,
        })
    }

    /// A fresh pinned-data checkpoint for explicit native inspection. Not an
    /// export/recovery proof for mutable audit, secrets or platform metadata.
    pub fn verified_data_checkpoint(&self) -> Result<Checkpoint> {
        let _guard = self.lock()?;
        self.checkpoint_for_locked(&self.pin_current()?)
    }

    pub(crate) fn write_authority(&self, state: &AuthorityState) -> Result<Sha256Hex> {
        let bytes = canonical_bytes(state).map_err(|_| HostFault::InvalidState)?;
        parse_document::<AuthorityState>(&bytes).map_err(|_| HostFault::InvalidState)?;
        let hash = sha256(&bytes);
        let path = authority_file(&hash);
        match self
            .managed_root()
            .read_bounded(&path, MAX_HOST_DOCUMENT_BYTES)?
        {
            Some(existing) if existing == bytes => {}
            Some(_) => return Err(HostFault::InvalidState.into()),
            None => self.managed_root().write_new(&path, &bytes)?,
        }
        Ok(hash)
    }

    pub(crate) fn mode_bytes(&self) -> Vec<u8> {
        canonical_bytes(&json!({
            "schema_version": 1, "mode": "host-current-v2",
            "vault_id": self.vault_id(),
        }))
        .expect("mode marker")
    }

    fn host_state_locked(&self) -> Result<AuthorityState> {
        if self
            .managed_root()
            .read_bounded(MODE_FILE, 1024)?
            .as_deref()
            != Some(self.mode_bytes().as_slice())
        {
            return Err(HostFault::InvalidState.into());
        }
        let bytes = self
            .managed_root()
            .read_bounded(&layout::current_pointer(), 4096)?
            .ok_or(HostFault::Unavailable)?;
        // Direct decoding rejects duplicate fields before generic validation.
        let pointer: CurrentPointer =
            serde_json::from_slice(&bytes).map_err(|_| HostFault::InvalidState)?;
        parse_store::<CurrentPointer>(&bytes).map_err(|_| HostFault::InvalidState)?;
        let hash = pointer
            .host_authority_hash
            .as_ref()
            .ok_or(HostFault::InvalidState)?;
        let bytes = self
            .managed_root()
            .read_bounded(&authority_file(hash), MAX_HOST_DOCUMENT_BYTES)?
            .ok_or(HostFault::Unavailable)?;
        if &sha256(&bytes) != hash {
            return Err(HostFault::InvalidState.into());
        }
        let state: AuthorityState = parse_document(&bytes).map_err(|_| HostFault::InvalidState)?;
        if &state.vault_id != self.vault_id() {
            return Err(HostFault::Fenced.into());
        }
        let checkpoint = self.checkpoint_for_locked(&self.pin_current()?)?;
        if state.checkpoint != checkpoint {
            return Err(HostFault::CheckpointMismatch.into());
        }
        self.check_publication_locked(&pointer, &state)?;
        Ok(state)
    }
}

/// Holds the existing OS Host lease for its lifetime. Every operation also
/// obtains the Vault writer lock and re-verifies data and authority from disk.
/// This is a coordinator, not yet the Host executable or an IPC/MCP adapter.
pub struct HostCoordinator<'a> {
    pub(super) vault: &'a Vault,
    pub(super) identity: DeviceId,
    _lease: File,
}

impl<'a> HostCoordinator<'a> {
    pub(super) fn acquire(
        vault: &'a Vault,
        root: &VerifiedRoot,
        identity: DeviceId,
    ) -> Result<Self> {
        if !cfg!(windows) {
            return Err(HostFault::Unavailable.into());
        }
        let verified = verify_data_root(root.path(), &RootPolicy::default())
            .map_err(|_| HostFault::InvalidState)?;
        if verified.path() != vault.managed_root().root()
            || !verified.filesystem.eq_ignore_ascii_case("NTFS")
        {
            return Err(HostFault::InvalidState.into());
        }
        if !platform::inspect_acl(verified.path())?.is_owner_only() {
            return Err(HostFault::PermissionDenied.into());
        }
        let lease = vault
            .managed_root()
            .open_lock_file(&layout::host_lock_file())?;
        match lease.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(HostFault::Busy.into()),
            Err(TryLockError::Error(error)) => return Err(error.into()),
        }
        Ok(Self {
            vault,
            identity,
            _lease: lease,
        })
    }

    /// Explicit irreversible MODE ADOPTION in a synthetic root. Native owner
    /// approval belongs to the calling control plane; there is no UI command.
    /// A partial bootstrap leaves an intent and refuses legacy recovery/writing.
    pub fn bootstrap(vault: &'a Vault, root: &VerifiedRoot, identity: DeviceId) -> Result<Self> {
        let host = Self::acquire(vault, root, identity)?;
        let _guard = vault.lock()?;
        vault.require_legacy_mode()?;
        if vault.managed_root().exists(AUTHORITY_FILE)? {
            return Err(HostFault::Conflict.into());
        }
        let pin = vault.pin_current()?;
        let checkpoint = vault.checkpoint_for_locked(&pin)?;
        let state =
            AuthorityState::initial(vault.vault_id().clone(), host.identity.clone(), checkpoint)?;
        let commit = vault.stored_commit(&pin)?;
        vault
            .publish_host(&commit, &state, true)
            .map_err(|_| VaultError::from(HostFault::OutcomeUnknown))?;
        Ok(host)
    }

    /// Never bootstrap, repair, choose a recovery point or overwrite on open.
    pub fn open(vault: &'a Vault, root: &VerifiedRoot, identity: DeviceId) -> Result<Self> {
        let host = Self::acquire(vault, root, identity)?;
        host.inspect()?;
        Ok(host)
    }

    pub fn inspect(&self) -> Result<AuthorityState> {
        let _guard = self.vault.lock()?;
        self.vault.host_state_locked()
    }

    pub fn commit(&self, request: CommitRequest) -> Result<CommitOutcome> {
        let _guard = self.vault.lock()?;
        let state = self.vault.host_state_locked()?;
        let binding = enouia_memory_contract::host::PrimaryBinding {
            host_id: self.identity.clone(),
            primary_epoch: state.primary.primary_epoch,
        };
        state.check_writer(self.vault.vault_id(), &binding)?;
        self.vault.commit_locked(request, Some(&state))
    }

    /// Authenticated transport MUST NOT accept writer/target identity from a
    /// client packet. Both are checked against this native coordinator identity.
    pub fn apply(&self, change: ControlChange<'_>) -> Result<AuthorityState> {
        let _guard = self.vault.lock()?;
        let state = self.vault.host_state_locked()?;
        let next = match change {
            ControlChange::Quiesce {
                writer,
                plan,
                confirmed_hash,
            } => {
                if writer.host_id != self.identity {
                    return Err(HostFault::Fenced.into());
                }
                state.quiesce(writer, plan, confirmed_hash)?
            }
            ControlChange::Promote {
                target,
                plan,
                confirmed_hash,
                verified_target,
            } => {
                if target != &self.identity {
                    return Err(HostFault::Fenced.into());
                }
                if verified_target != &state.checkpoint {
                    return Err(HostFault::CheckpointMismatch.into());
                }
                state.promote(target, plan, confirmed_hash, &state.checkpoint)?
            }
            ControlChange::Abort {
                writer,
                plan,
                confirmed_hash,
            } => {
                if writer.host_id != self.identity {
                    return Err(HostFault::Fenced.into());
                }
                state.abort(writer, plan, confirmed_hash)?
            }
        };
        if next != state {
            let commit = self.vault.stored_commit(&self.vault.pin_current()?)?;
            self.vault
                .publish_host(&commit, &next, false)
                .map_err(|_| VaultError::from(HostFault::OutcomeUnknown))?;
        }
        Ok(next)
    }
}

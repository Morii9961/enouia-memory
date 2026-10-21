//! MV-8.1b: opt-in, authority-only native persistence (ADR-MEM-67).
//!
//! This is NOT the full authority/data transaction adapter. No data-advance
//! operation exists here; CURRENT and this file must not be published separately
//! and called atomic. Existing Core/CLI do not adopt this control file.

use crate::fault::{FaultPoint, Faults};
use crate::fs::ManagedRoot;
use crate::lock::{self, WriterGuard};
use crate::platform;
use crate::{RootPolicy, VerifiedRoot, verify_data_root};
use enouia_memory_contract::hash::Sha256Hex;
use enouia_memory_contract::host::{
    AuthorityState, Checkpoint, HostFault, HostResult, MAX_HOST_DOCUMENT_BYTES, PrimaryBinding,
    TransferPlan, parse_document,
};
use enouia_memory_contract::ids::{DeviceId, VaultId};
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::layout;
use enouia_memory_contract::store::{VaultDescriptor, parse_store};
use std::fs::{File, TryLockError};
use std::time::Duration;

pub const AUTHORITY_FILE: &str = "config/host-authority.json";

/// Native, independently verified complete snapshot. A caller-supplied hash or
/// a fixture reader is not production evidence; the actual verifier is deferred.
pub trait CheckpointReader {
    fn verified_checkpoint(&self, vault: &VaultId) -> HostResult<Checkpoint>;
}

pub enum ControlChange<'a> {
    Quiesce {
        writer: &'a PrimaryBinding,
        plan: &'a TransferPlan,
        confirmed_hash: &'a Sha256Hex,
    },
    Promote {
        target: &'a DeviceId,
        plan: &'a TransferPlan,
        confirmed_hash: &'a Sha256Hex,
        verified_target: &'a Checkpoint,
    },
    Abort {
        writer: &'a PrimaryBinding,
        plan: &'a TransferPlan,
        confirmed_hash: &'a Sha256Hex,
    },
}

/// Owns both OS leases until drop. Trusted native callers must obtain owner
/// approval and identity themselves; no page, CLI or MCP entry point is exposed.
pub struct ControlStore<R> {
    root: ManagedRoot,
    vault_id: VaultId,
    reader: R,
    _host_lease: File,
    _writer_lease: WriterGuard,
}

impl<R: CheckpointReader> ControlStore<R> {
    fn acquire(root: &VerifiedRoot, reader: R, faults: Faults) -> HostResult<Self> {
        if !cfg!(windows) {
            return Err(HostFault::Unavailable);
        }
        let verified = verify_data_root(root.path(), &RootPolicy::default())
            .map_err(|_| HostFault::InvalidState)?;
        if !verified.filesystem.eq_ignore_ascii_case("NTFS") {
            return Err(HostFault::Unavailable);
        }
        let acl =
            platform::inspect_acl(verified.path()).map_err(|_| HostFault::PermissionDenied)?;
        if !acl.is_owner_only() {
            return Err(HostFault::PermissionDenied);
        }
        let root = ManagedRoot::new(verified.path(), faults);
        let host_lease = root
            .open_lock_file(&layout::host_lock_file())
            .map_err(|_| HostFault::StorageFailed)?;
        match host_lease.try_lock() {
            Ok(()) => {}
            Err(TryLockError::WouldBlock) => return Err(HostFault::Busy),
            Err(TryLockError::Error(_)) => return Err(HostFault::StorageFailed),
        }
        let writer_lease = lock::acquire(&root, Duration::ZERO).map_err(|error| {
            if matches!(error.fault, crate::Fault::WriterBusy) {
                HostFault::Busy
            } else {
                HostFault::StorageFailed
            }
        })?;
        if root
            .exists(crate::host_transaction::MODE_FILE)
            .map_err(|_| HostFault::StorageFailed)?
            || root
                .exists("vault/host-authority")
                .map_err(|_| HostFault::StorageFailed)?
            || root
                .exists(crate::host_recovery::INTENT_FILE)
                .map_err(|_| HostFault::StorageFailed)?
            || root
                .exists(crate::host_recovery::WITNESS_FILE)
                .map_err(|_| HostFault::StorageFailed)?
        {
            return Err(HostFault::Conflict);
        }
        let descriptor = root
            .read_bounded(&layout::vault_descriptor(), 64 * 1024)
            .map_err(|_| HostFault::StorageFailed)?
            .ok_or(HostFault::InvalidState)?;
        let descriptor: VaultDescriptor =
            parse_store(&descriptor).map_err(|_| HostFault::InvalidState)?;
        Ok(Self {
            root,
            vault_id: descriptor.vault_id,
            reader,
            _host_lease: host_lease,
            _writer_lease: writer_lease,
        })
    }

    /// Explicit native bootstrap only. Never called by open or missing-file
    /// recovery. Refuses any existing file, even malformed/empty bytes.
    pub fn bootstrap(
        root: &VerifiedRoot,
        state: AuthorityState,
        reader: R,
        faults: Faults,
    ) -> HostResult<Self> {
        let store = Self::acquire(root, reader, faults)?;
        if store
            .root
            .read_bounded(AUTHORITY_FILE, MAX_HOST_DOCUMENT_BYTES)
            .map_err(|_| HostFault::StorageFailed)?
            .is_some()
        {
            return Err(HostFault::Conflict);
        }
        if state.vault_id != store.vault_id {
            return Err(HostFault::Fenced);
        }
        let checkpoint = store.reader.verified_checkpoint(&store.vault_id)?;
        let initial = AuthorityState::initial(
            store.vault_id.clone(),
            state.primary.host_id.clone(),
            checkpoint,
        )?;
        if initial != state {
            return Err(HostFault::InvalidState);
        }
        store.publish(&state)?;
        Ok(store)
    }

    pub fn open(root: &VerifiedRoot, reader: R, faults: Faults) -> HostResult<Self> {
        let store = Self::acquire(root, reader, faults)?;
        store.inspect()?;
        Ok(store)
    }

    /// Fresh bounded disk read and fresh native checkpoint on every operation.
    pub fn inspect(&self) -> HostResult<AuthorityState> {
        let bytes = self
            .root
            .read_bounded(AUTHORITY_FILE, MAX_HOST_DOCUMENT_BYTES)
            .map_err(|_| HostFault::StorageFailed)?
            .ok_or(HostFault::Unavailable)?;
        let state: AuthorityState = parse_document(&bytes).map_err(|_| HostFault::InvalidState)?;
        if state.vault_id != self.vault_id {
            return Err(HostFault::Fenced);
        }
        if self.reader.verified_checkpoint(&self.vault_id)? != state.checkpoint {
            return Err(HostFault::CheckpointMismatch);
        }
        Ok(state)
    }

    fn publish(&self, state: &AuthorityState) -> HostResult<()> {
        let bytes = canonical_bytes(state).map_err(|_| HostFault::InvalidState)?;
        // Revalidate the exact bytes that will be published, including budget.
        parse_document::<AuthorityState>(&bytes).map_err(|_| HostFault::InvalidState)?;
        self.root
            .write_atomic(AUTHORITY_FILE, &bytes)
            .map_err(|_| HostFault::OutcomeUnknown)?;
        self.root
            .faults()
            .io(FaultPoint::HostAuthorityPublished)
            .map_err(|_| HostFault::OutcomeUnknown)
    }

    /// Only authority changes with the SAME checkpoint can be published. The
    /// API deliberately cannot stage/commit data or advance its checkpoint.
    pub fn apply(&mut self, change: ControlChange<'_>) -> HostResult<AuthorityState> {
        let current = self.inspect()?;
        let next = match change {
            ControlChange::Quiesce {
                writer,
                plan,
                confirmed_hash,
            } => current.quiesce(writer, plan, confirmed_hash)?,
            ControlChange::Promote {
                target,
                plan,
                confirmed_hash,
                verified_target,
            } => current.promote(target, plan, confirmed_hash, verified_target)?,
            ControlChange::Abort {
                writer,
                plan,
                confirmed_hash,
            } => current.abort(writer, plan, confirmed_hash)?,
        };
        if next.checkpoint != current.checkpoint {
            return Err(HostFault::CheckpointMismatch);
        }
        if next != current {
            self.publish(&next)?;
        }
        Ok(next)
    }
}

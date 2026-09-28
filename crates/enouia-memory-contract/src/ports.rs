//! Ports that MV-1+ implementations must satisfy, plus the pure rules every
//! implementation shares (idempotency classification, clock regression).
//!
//! Host ports live in `foundation`: `Clock`, `Cancellation`, `WriterLock`
//! (the OS-level single-writer lock), `AtomicFile`. An in-process mutex is not a
//! writer lock. Nothing here performs I/O; no Gateway, sync, or multi-agent
//! framework is defined at this stage.

use crate::commit::{CommitManifest, ObjectKind, OperationKind, OperationReceipt};
use crate::common::ActorRef;
use crate::context::{Destination, Purpose};
use crate::error::{MemoryError, MemoryErrorCode};
use crate::foundation::{Cancellation, Clock, ComponentId};
use crate::hash::Sha256Hex;
use crate::ids::{CommitId, PrincipalId, RequestId};
use crate::ipc::Operation;
use crate::json::Revision;
use crate::policy::{PolicyDecision, ResourceContext, Scope};
use crate::provider::{ProviderCapabilities, ProviderRequest, ProviderResponse};
use crate::record::{RecordKind, RecordRef};
use crate::time::Timestamp;
use std::fmt;

/// Source of randomness for typed IDs. Production uses the OS CSPRNG (MV-1);
/// tests use a deterministic sequence. IDs never derive from time or content.
pub trait IdSource {
    fn random_16(&self) -> [u8; 16];
}

/// Deterministic test IdSource: counter bytes, still formatted as UUID v4.
pub struct SequentialIdSource(std::sync::atomic::AtomicU64);

impl SequentialIdSource {
    pub const fn new(start: u64) -> Self {
        Self(std::sync::atomic::AtomicU64::new(start))
    }
}

impl IdSource for SequentialIdSource {
    fn random_16(&self) -> [u8; 16] {
        let n = self.0.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut bytes = [0u8; 16];
        bytes[8..].copy_from_slice(&n.to_be_bytes());
        bytes
    }
}

/// A commit a reader has pinned for its whole read, with the policy/deletion
/// epochs observed at pin time. Fresh epochs are re-checked before returning
/// content or dispatching to a Provider, even for an old pin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CommitPin {
    pub commit_id: CommitId,
    pub sequence: u64,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
}

/// Read side of the Vault. Implementations never guess a newest commit when
/// `CURRENT` cannot be verified: they return `VaultRecovering`.
pub trait VaultReader {
    fn pin_current(&self) -> Result<CommitPin, MemoryError>;
    fn read_manifest(&self, pin: &CommitPin) -> Result<CommitManifest, MemoryError>;
    fn read_record(&self, pin: &CommitPin, record: &RecordRef) -> Result<Vec<u8>, MemoryError>;
    fn read_object(&self, pin: &CommitPin, hash: &Sha256Hex) -> Result<Vec<u8>, MemoryError>;
}

/// Idempotency is scoped by authenticated principal + canonical operation.
/// The store scopes by the commit `OperationKind` (ADR-MEM-36): local writes
/// such as imports have no IPC operation, and each IPC write maps to one kind.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct IdempotencyScope {
    pub principal_id: PrincipalId,
    pub operation_kind: OperationKind,
    pub key_hash: Sha256Hex,
}

#[derive(Debug)]
pub struct StagedRecord {
    pub record_kind: RecordKind,
    pub record_id: String,
    pub revision: Revision,
    /// Canonical bytes from `json::canonical_bytes`, already validated.
    pub bytes: Vec<u8>,
}

#[derive(Debug)]
pub struct StagedObject {
    pub kind: ObjectKind,
    pub hash: Sha256Hex,
    pub bytes: Vec<u8>,
}

/// One transaction. All writes become visible together through a single
/// `CURRENT` publication, or none do. Genesis is not a `CommitRequest`: it is
/// the separate, owner-confirmed initialization of an empty Vault.
#[derive(Debug)]
pub struct CommitRequest {
    /// `Some`: the head the caller saw; any other head is `RevisionConflict`.
    /// `None`: only `expected_revisions` guard the write.
    pub expected_commit_id: Option<CommitId>,
    pub principal: ActorRef,
    pub operation_kind: OperationKind,
    /// Required: every non-genesis commit records its idempotency key hash.
    pub idempotency: IdempotencyScope,
    pub request_payload_hash: Sha256Hex,
    /// Every record this transaction relies on, with the revision the caller saw
    /// (`None` = must not exist yet). Any mismatch fails with `RevisionConflict`.
    pub expected_revisions: Vec<(RecordKind, String, Option<Revision>)>,
    pub records: Vec<StagedRecord>,
    pub objects: Vec<StagedObject>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CommitOutcome {
    /// Durable at the declared persistence level before returning.
    Committed {
        commit_id: CommitId,
        receipt: OperationReceipt,
    },
    /// Same scope and payload seen before: the original receipt, nothing applied.
    Replayed {
        commit_id: CommitId,
        receipt: OperationReceipt,
    },
}

/// Single-writer port. Implementations hold the OS writer lock for the whole
/// call, verify `expected_commit_id`, and publish exactly one `CURRENT`.
/// Index updates happen after release and never undo a committed write.
pub trait VaultWriter {
    fn commit(&self, request: CommitRequest) -> Result<CommitOutcome, MemoryError>;
    fn find_receipt(
        &self,
        scope: &IdempotencyScope,
    ) -> Result<Option<(CommitId, Sha256Hex, OperationReceipt)>, MemoryError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RetryDisposition {
    New,
    Replay,
    Conflict,
}

/// Pure idempotency rule: same scope + same payload replays the stored receipt;
/// same scope + different payload is `IdempotencyConflict`, never an overwrite.
pub fn classify_retry(
    stored_payload_hash: Option<&Sha256Hex>,
    request_payload_hash: &Sha256Hex,
) -> RetryDisposition {
    match stored_payload_hash {
        None => RetryDisposition::New,
        Some(stored) if stored == request_payload_hash => RetryDisposition::Replay,
        Some(_) => RetryDisposition::Conflict,
    }
}

/// Commit times come from the injected Clock. A clock earlier than the parent
/// commit pauses writing with `clock_regression` instead of forging order.
pub fn commit_time(
    clock: &dyn Clock,
    parent_created_at: Option<&Timestamp>,
    component: ComponentId,
) -> Result<Timestamp, MemoryError> {
    let now = Timestamp::from_unix_ms(clock.now_unix_ms()).ok_or(MemoryError::new(
        MemoryErrorCode::ClockRegression,
        component,
    ))?;
    match parent_created_at {
        Some(parent) if &now < parent => Err(MemoryError::new(
            MemoryErrorCode::ClockRegression,
            component,
        )),
        _ => Ok(now),
    }
}

/// Access audit. If `record` fails, external reads and Provider dispatch fail
/// closed with `AuditUnavailable`; only local health/recovery may proceed.
pub trait AuditSink {
    fn record(&self, event: &crate::commit::AuditEvent) -> Result<(), MemoryError>;
}

/// One authorization question, fully resolved by the server: the principal
/// comes from transport authentication, and every target is a pinned record
/// revision with its project and sensitivity read from the Vault, never from
/// caller arguments or a hidden table keyed by request ID.
#[derive(Clone, Debug)]
pub struct AccessRequest {
    pub principal: ActorRef,
    pub operation: Operation,
    pub scope: Scope,
    pub purpose: Option<Purpose>,
    /// Full destination including the exact Provider binding.
    pub destination: Option<Destination>,
    pub targets: Vec<ResourceContext>,
    pub request_id: RequestId,
}

/// Default deny. `decide` must answer from versioned PolicyRecords at the
/// current policy epoch (`policy::evaluate` is the reference semantics) and
/// allow only when every target is allowed.
pub trait PolicyGate {
    fn decide(&self, request: &AccessRequest) -> PolicyDecision;
    fn current_epochs(&self) -> Result<(u64, u64), MemoryError>;
}

/// Inference Provider port. Receives only what a DispatchRecord describes.
pub trait ProviderPort {
    fn capabilities(&self) -> ProviderCapabilities;
    fn send(
        &self,
        request: &ProviderRequest,
        cancellation: &dyn Cancellation,
    ) -> Result<ProviderResponse, MemoryError>;
}

/// Secret bytes read from the OS secret store (DPAPI/credential manager).
/// Not Serialize, not Clone, redacted Debug: cannot enter Vault, logs, IPC, or backups.
pub struct SecretBytes(Vec<u8>);

impl SecretBytes {
    pub fn new(bytes: Vec<u8>) -> Self {
        Self(bytes)
    }

    pub fn expose(&self) -> &[u8] {
        &self.0
    }
}

impl fmt::Debug for SecretBytes {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretBytes(<redacted>)")
    }
}

pub trait SecretStore {
    fn read(&self, name: &str) -> Result<SecretBytes, MemoryError>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupReceipt {
    pub snapshot_id: String,
    pub commit_id: CommitId,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub completed_at: Timestamp,
    /// Repository reachable and sampled objects verified. Not a restore test.
    pub verified: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RestorePlan {
    pub snapshot_id: String,
    pub commit_id: CommitId,
    /// Restores start with network, Provider, MCP, and sync disabled until
    /// newer tombstones and device revocations are reconciled.
    pub network_disabled_until_reconciled: bool,
}

/// Backup of one pinned commit (restic adapter planned for MV-1). The
/// implementation holds a lease that blocks GC/purge of needed objects.
pub trait BackupPort {
    fn backup_pinned(&self, pin: &CommitPin) -> Result<BackupReceipt, MemoryError>;
    fn restore_preview(&self, snapshot_id: &str) -> Result<RestorePlan, MemoryError>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::foundation::FakeClock;

    fn hash(c: char) -> Sha256Hex {
        Sha256Hex::parse(&c.to_string().repeat(64)).unwrap()
    }

    #[test]
    fn retries_replay_or_conflict_never_overwrite() {
        assert_eq!(classify_retry(None, &hash('a')), RetryDisposition::New);
        assert_eq!(
            classify_retry(Some(&hash('a')), &hash('a')),
            RetryDisposition::Replay
        );
        assert_eq!(
            classify_retry(Some(&hash('a')), &hash('b')),
            RetryDisposition::Conflict
        );
    }

    #[test]
    fn clock_regression_blocks_commit_time() {
        let parent = Timestamp::parse("2026-09-28T08:00:00.000Z").unwrap();
        let clock = FakeClock::new(parent.unix_ms() - 1);
        let error = commit_time(&clock, Some(&parent), ComponentId::Vault).unwrap_err();
        assert_eq!(error.code, MemoryErrorCode::ClockRegression);
        clock.set(parent.unix_ms());
        assert_eq!(
            commit_time(&clock, Some(&parent), ComponentId::Vault).unwrap(),
            parent
        );
    }

    #[test]
    fn secrets_are_redacted_in_debug() {
        let secret = SecretBytes::new(b"sentinel-secret".to_vec());
        assert!(!format!("{secret:?}").contains("sentinel"));
    }

    #[test]
    fn sequential_ids_are_valid_v4() {
        let source = SequentialIdSource::new(7);
        let id = crate::ids::MemoryId::from_random(source.random_16());
        assert!(crate::ids::MemoryId::parse(id.as_str()).is_ok());
    }
}

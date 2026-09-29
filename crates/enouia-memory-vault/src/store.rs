//! The Vault store (MV-1.2, segmented in MV-3.0): immutable revisions, a
//! commit that references immutable catalog segments, one atomically
//! replaced `CURRENT`, idempotent receipts, pinned reads, and owner-driven
//! recovery.
//!
//! Transaction order (MEMORY_ARCHITECTURE §6), each step flushed:
//!
//! 1. take the OS writer lock; clear unpublished staging; load and verify the
//!    head named by `CURRENT` (never guess one);
//! 2. idempotency: a published receipt for the same scope replays, a
//!    different payload conflicts; then check the expected head/revisions;
//! 3. validate every staged record and object; build the changed catalog
//!    segments; validate the delta on a scoped set (ADR-MEM-39);
//! 4. write an intent marker in `staging/<operation>/`, then records,
//!    objects, and new segments once at their final (still unreferenced)
//!    names, then the stored commit and the idempotency entry;
//! 5. replace `CURRENT` (the only publication step), append the journal;
//! 6. release the lock. Indexes are not part of the transaction.
//!
//! A crash before step 5 leaves the old `CURRENT`; everything written so far
//! is unreferenced and never read. Durability is "flushed with write-through"
//! on the local volume; power-loss behavior is not claimed (V09).

use crate::error::{Fault, Result, VaultError};
use crate::fault::{FaultPoint, Faults};
use crate::fs::ManagedRoot;
use crate::lock::{self, WriterGuard};
use crate::root::VerifiedRoot;
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::catalog::{
    CatalogSegment, LAYOUT_FORMAT_VERSION, LayoutFormat, ObjectItem, RecordEntry, SEGMENT_CAPACITY,
    SegmentKind, SegmentRef, StoredCommit, leaf_for, object_kind_order, record_key, split,
};
use enouia_memory_contract::commit::{
    CommitManifest, FormatVersion, ObjectKind, OperationKind, OperationReceipt, ReceiptResult,
};
use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
use enouia_memory_contract::delta::{Need, delta_needs, is_bulk, validate_delta};
use enouia_memory_contract::foundation::{Clock, ComponentId};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{CommitId, DeleteId, DeviceId, OperationId, ReviewId, VaultId};
use enouia_memory_contract::json::{Revision, SchemaVersion, canonical_bytes};
use enouia_memory_contract::layout;
use enouia_memory_contract::ports::{
    CommitOutcome, CommitPin, CommitRequest, IdSource, IdempotencyScope, StagedObject,
    StagedRecord, commit_time,
};
use enouia_memory_contract::record::{AnyRecord, RecordKind, RecordRef, parse_any};
use enouia_memory_contract::set::{RecordSet, check_commit_step};
use enouia_memory_contract::store::{
    CurrentPointer, IdempotencyEntry, PublishRecord, RecoveryEvidence, RecoveryReceipt,
    StoreDocument, VaultDescriptor, idempotency_scope_hash, parse_store,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct VaultOptions {
    /// How long a writer waits for the OS lock before returning `busy`.
    pub lock_wait: Duration,
    /// Validate every commit against the cross-record rules on a scoped set
    /// (ADR-MEM-39). Off only for measurements.
    pub validate_record_set: bool,
    /// Leaf capacity of catalog segments for new commits.
    pub segment_capacity: u64,
    pub faults: Faults,
}

impl Default for VaultOptions {
    fn default() -> Self {
        Self {
            lock_wait: Duration::from_secs(2),
            validate_record_set: true,
            segment_capacity: SEGMENT_CAPACITY,
            faults: Faults::none(),
        }
    }
}

/// The owner-confirmed creation of an empty Vault (MEMORY_ARCHITECTURE §6).
#[derive(Debug)]
pub struct GenesisRequest {
    pub vault_id: VaultId,
    pub commit_id: CommitId,
    pub device_id: DeviceId,
    pub owner: ActorRef,
    pub trusted_surface: TrustedSurface,
    pub request_payload_hash: Sha256Hex,
    /// Policy records only: at least the genesis default-deny policy.
    pub records: Vec<StagedRecord>,
}

/// State of `CURRENT` as found on disk.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CurrentState {
    Valid,
    Missing,
    Unreadable,
    ForeignVault,
    ManifestUnverifiable,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryCandidate {
    pub commit_id: CommitId,
    pub sequence: u64,
    pub manifest_sha256: Sha256Hex,
    pub evidence: RecoveryEvidence,
    /// Every segment, record revision, and object of the commit is present
    /// and verified.
    pub complete: bool,
}

/// What an owner needs to choose a recovery point. Nothing is chosen here.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecoveryReport {
    pub current: CurrentState,
    pub current_sha256: Option<Sha256Hex>,
    /// Newest first: journal-evidenced commits, then complete unpublished ones.
    pub candidates: Vec<RecoveryCandidate>,
}

/// Scope of damage found by a full verification of one commit (V06).
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct IntegrityReport {
    /// Record revisions checked: every revision the commit names.
    pub records_checked: usize,
    pub objects_checked: usize,
    pub missing_records: Vec<RecordRef>,
    pub corrupt_records: Vec<RecordRef>,
    pub missing_objects: Vec<Sha256Hex>,
    pub corrupt_objects: Vec<Sha256Hex>,
    /// Catalog segments that are missing, altered, or do not match their
    /// reference; their records and objects could not be listed.
    pub damaged_segments: Vec<Sha256Hex>,
}

impl IntegrityReport {
    pub fn is_clean(&self) -> bool {
        self.missing_records.is_empty()
            && self.corrupt_records.is_empty()
            && self.missing_objects.is_empty()
            && self.corrupt_objects.is_empty()
            && self.damaged_segments.is_empty()
    }
}

/// Result of re-checking a pinned read against the newest head before
/// returning content or dispatching it (V08).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Freshness {
    pub head: CommitPin,
    /// The policy epoch moved since the pin: authorization must be re-decided.
    pub policy_changed: bool,
}

/// One file of a pinned state and the hash it must have.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PinnedFile {
    pub file: String,
    pub sha256: Sha256Hex,
}

type DocKey = (RecordKind, String, u64);

pub struct Vault {
    root: ManagedRoot,
    descriptor: VaultDescriptor,
    device_id: DeviceId,
    clock: Arc<dyn Clock + Send + Sync>,
    ids: Arc<dyn IdSource + Send + Sync>,
    options: VaultOptions,
    commits: Mutex<BTreeMap<String, (Arc<StoredCommit>, Sha256Hex)>>,
    segments: Mutex<BTreeMap<Sha256Hex, Arc<CatalogSegment>>>,
    records: Mutex<BTreeMap<DocKey, AnyRecord>>,
}

fn id_field(kind: RecordKind) -> Option<&'static str> {
    Some(match kind {
        RecordKind::Source => "source_id",
        RecordKind::Attachment => "attachment_id",
        RecordKind::Project => "project_id",
        RecordKind::Memory => "memory_id",
        RecordKind::Candidate => "candidate_id",
        RecordKind::Review => "review_id",
        RecordKind::Identity => "identity_id",
        RecordKind::Session => "session_id",
        RecordKind::SessionEvent => "event_id",
        RecordKind::Checkpoint => "checkpoint_id",
        RecordKind::Tombstone => "delete_id",
        RecordKind::PurgeReceipt => "receipt_id",
        RecordKind::Approval => "approval_id",
        RecordKind::Policy => "policy_id",
        RecordKind::Import => "import_id",
        _ => return None,
    })
}

/// Final file of a stored record revision. A session event's file lives
/// under its session, which its catalog entry names as its group.
fn record_file(
    kind: RecordKind,
    id: &str,
    revision: Revision,
    session: Option<&str>,
) -> Option<String> {
    match kind {
        RecordKind::Identity => Some(layout::identity_sidecar(id, revision)),
        RecordKind::SessionEvent => session.map(|s| layout::session_event(s, id)),
        _ => layout::record_path(kind, id, revision),
    }
}

pub(crate) fn entry_file(kind: RecordKind, entry: &RecordEntry, revision: u64) -> Option<String> {
    record_file(
        kind,
        &entry.record_id,
        Revision::new(revision)?,
        entry.group_ids.first().map(String::as_str),
    )
}

/// Group IDs a record contributes to its catalog entry (ADR-MEM-39).
fn groups_of(record: &AnyRecord) -> Vec<String> {
    match record {
        AnyRecord::Source(s) => s.import_id.iter().map(|i| i.to_string()).collect(),
        AnyRecord::SessionEvent(e) => vec![e.session_id.to_string()],
        _ => Vec::new(),
    }
}

pub(crate) fn object_file_of(item: &ObjectItem) -> Option<String> {
    match (&item.stored_with, item.object_kind) {
        (Some(r), ObjectKind::IdentityMarkdown) => {
            layout::record_path(RecordKind::Identity, &r.record_id, r.revision)
        }
        (_, kind) => layout::object_path(kind, &item.object_hash),
    }
}

/// The commit header as a `CommitManifest` without catalog or objects: what
/// cross-record rules read about a commit (review and tombstone IDs, epochs).
fn stub_of(commit: &StoredCommit) -> CommitManifest {
    CommitManifest {
        schema_version: SchemaVersion,
        commit_id: commit.commit_id.clone(),
        format_version: FormatVersion,
        parent_commit_id: commit.parent_commit_id.clone(),
        sequence: commit.sequence,
        vault_id: commit.vault_id.clone(),
        writer_device_id: commit.writer_device_id.clone(),
        principal: commit.principal.clone(),
        operation_id: commit.operation_id.clone(),
        operation_kind: commit.operation_kind,
        idempotency_key_hash: commit.idempotency_key_hash.clone(),
        request_payload_hash: commit.request_payload_hash.clone(),
        created_at: commit.created_at.clone(),
        catalog: Vec::new(),
        objects: Vec::new(),
        review_ids: commit.review_ids.clone(),
        tombstone_ids: commit.tombstone_ids.clone(),
        policy_epoch: commit.policy_epoch,
        deletion_epoch: commit.deletion_epoch,
        receipt: OperationReceipt {
            operation_id: commit.operation_id.clone(),
            result: ReceiptResult::Committed,
            records: Vec::new(),
        },
    }
}

/// A staged record after validation, with its final file name.
struct Prepared {
    kind: RecordKind,
    id: String,
    revision: Revision,
    file: String,
    bytes: Vec<u8>,
    record: AnyRecord,
}

/// A new commit before it is written: the stored commit and the segments it
/// adds (hash, canonical bytes).
struct Built {
    commit: StoredCommit,
    segments: Vec<(Sha256Hex, Vec<u8>)>,
}

impl Vault {
    /// Create a new Vault on a verified, empty root. Refuses any root that
    /// already has content: an existing directory is never re-initialized.
    pub fn create(
        root: &VerifiedRoot,
        request: GenesisRequest,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        options: VaultOptions,
    ) -> Result<Self> {
        if std::fs::read_dir(root.path())?.next().is_some() {
            return Err(VaultError::new(
                MemoryErrorCode::InvalidRequest,
                Fault::AlreadyInitialized,
            ));
        }
        if request.owner.actor_type != ActorType::Owner {
            return Err(VaultError::invalid(vec!["commit.genesis_owner"]));
        }
        let managed = ManagedRoot::new(root.path(), options.faults.clone());
        let _guard = lock::acquire(&managed, options.lock_wait)?;
        // Re-check under the lock: a concurrent genesis may have won.
        if managed.exists(&layout::vault_descriptor())?
            || managed.exists(&layout::current_pointer())?
        {
            return Err(VaultError::new(
                MemoryErrorCode::InvalidRequest,
                Fault::AlreadyInitialized,
            ));
        }
        let created_at = commit_time(clock.as_ref(), None, ComponentId::Vault)?;
        let descriptor = VaultDescriptor {
            schema_version: SchemaVersion,
            vault_id: request.vault_id.clone(),
            format_version: LayoutFormat,
            genesis_commit_id: request.commit_id.clone(),
            genesis_device_id: request.device_id.clone(),
            created_by: request.owner.clone(),
            created_at: created_at.clone(),
        };
        let vault = Self {
            root: managed,
            descriptor,
            device_id: request.device_id.clone(),
            clock,
            ids,
            options,
            commits: Mutex::new(BTreeMap::new()),
            segments: Mutex::new(BTreeMap::new()),
            records: Mutex::new(BTreeMap::new()),
        };
        if request.records.is_empty()
            || request
                .records
                .iter()
                .any(|r| r.record_kind != RecordKind::Policy)
        {
            return Err(VaultError::invalid(vec!["store.genesis_records"]));
        }
        let prepared = vault.prepare_records(&request.records, None)?;
        let operation_id = OperationId::from_random(vault.ids.random_16());
        let header = StoredCommit {
            schema_version: SchemaVersion,
            commit_id: request.commit_id.clone(),
            format_version: LayoutFormat,
            parent_commit_id: None,
            sequence: 1,
            vault_id: request.vault_id.clone(),
            writer_device_id: request.device_id.clone(),
            principal: request.owner.clone(),
            operation_id: operation_id.clone(),
            operation_kind: OperationKind::Genesis,
            idempotency_key_hash: None,
            request_payload_hash: request.request_payload_hash.clone(),
            created_at,
            segment_capacity: vault.options.segment_capacity,
            record_segments: Vec::new(),
            object_segments: Vec::new(),
            review_ids: Vec::new(),
            tombstone_ids: Vec::new(),
            policy_epoch: 1,
            deletion_epoch: 0,
            receipt: OperationReceipt {
                operation_id,
                result: ReceiptResult::Committed,
                records: Vec::new(),
            },
        };
        let built = vault.build(header, None, &prepared, &[])?;
        vault.check_delta(&built.commit, None, &prepared)?;
        vault.write_transaction(&built, &prepared, &[], None)?;
        vault.root.write_new(
            &layout::vault_descriptor(),
            &canonical_bytes(&vault.descriptor).expect("descriptor"),
        )?;
        vault.publish(&built.commit)?;
        Ok(vault)
    }

    /// Open an existing Vault. Opening does not require a valid `CURRENT`, so
    /// that health and owner recovery remain available; reads and writes then
    /// fail with `vault_recovering`.
    pub fn open(
        root: &VerifiedRoot,
        device_id: Option<DeviceId>,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        options: VaultOptions,
    ) -> Result<Self> {
        let managed = ManagedRoot::new(root.path(), options.faults.clone());
        let bytes = managed
            .read(&layout::vault_descriptor())?
            .ok_or_else(|| VaultError::new(MemoryErrorCode::NotFound, Fault::NotInitialized))?;
        let descriptor: VaultDescriptor = parse_store(&bytes).map_err(|_| {
            // Another layout (a newer one, or the pre-segmented format 1)
            // is not readable by this build, never "corrupt".
            let other = serde_json::from_slice::<Value>(&bytes).is_ok_and(|v| {
                v["schema_version"].as_i64().is_some_and(|s| s > 1)
                    || v["format_version"]
                        .as_u64()
                        .is_some_and(|f| f != LAYOUT_FORMAT_VERSION)
            });
            if other {
                VaultError::new(
                    MemoryErrorCode::UnsupportedSchema,
                    Fault::Contract(vec!["unsupported_schema"]),
                )
            } else {
                VaultError::corrupt("vault descriptor")
            }
        })?;
        Ok(Self {
            root: managed,
            device_id: device_id.unwrap_or_else(|| descriptor.genesis_device_id.clone()),
            descriptor,
            clock,
            ids,
            options,
            commits: Mutex::new(BTreeMap::new()),
            segments: Mutex::new(BTreeMap::new()),
            records: Mutex::new(BTreeMap::new()),
        })
    }

    pub fn vault_id(&self) -> &VaultId {
        &self.descriptor.vault_id
    }

    pub fn descriptor(&self) -> &VaultDescriptor {
        &self.descriptor
    }

    pub fn managed_root(&self) -> &ManagedRoot {
        &self.root
    }

    /// Current time from the injected clock (UTC milliseconds).
    pub fn now(&self) -> Result<enouia_memory_contract::time::Timestamp> {
        Ok(commit_time(self.clock.as_ref(), None, ComponentId::Vault)?)
    }

    /// Sixteen bytes from the injected ID source, for new typed IDs.
    pub fn random_id_bytes(&self) -> [u8; 16] {
        self.ids.random_16()
    }

    // ---------------------------------------------------------------- reads

    fn load_commit(
        &self,
        commit_id: &CommitId,
        expected: Option<&Sha256Hex>,
    ) -> Result<(Arc<StoredCommit>, Sha256Hex)> {
        if let Some((commit, hash)) = self.commits.lock().expect("cache").get(commit_id.as_str()) {
            if expected.is_some_and(|e| e != hash) {
                return Err(VaultError::corrupt("manifest hash"));
            }
            return Ok((commit.clone(), hash.clone()));
        }
        let bytes = self
            .root
            .read(&layout::commit_manifest(commit_id.as_str()))?
            .ok_or_else(|| VaultError::corrupt("manifest missing"))?;
        let hash = sha256(&bytes);
        if expected.is_some_and(|e| e != &hash) {
            return Err(VaultError::corrupt("manifest hash"));
        }
        let commit: StoredCommit =
            parse_store(&bytes).map_err(|_| VaultError::corrupt("manifest unparseable"))?;
        if &commit.commit_id != commit_id
            || commit.vault_id != self.descriptor.vault_id
            || canonical_bytes(&commit).ok().as_deref() != Some(bytes.as_slice())
        {
            return Err(VaultError::corrupt("manifest identity"));
        }
        let commit = Arc::new(commit);
        self.commits
            .lock()
            .expect("cache")
            .insert(commit_id.to_string(), (commit.clone(), hash.clone()));
        Ok((commit, hash))
    }

    /// A catalog segment, verified by its content hash and matched to the
    /// reference that names it.
    fn segment(&self, reference: &SegmentRef) -> Result<Arc<CatalogSegment>> {
        let hash = &reference.segment_hash;
        let cached = self.segments.lock().expect("cache").get(hash).cloned();
        let segment = match cached {
            Some(segment) => segment,
            None => {
                let bytes = self
                    .root
                    .read(&layout::catalog_segment(hash))?
                    .ok_or_else(|| VaultError::corrupt("segment missing"))?;
                if &sha256(&bytes) != hash {
                    return Err(VaultError::corrupt("segment hash"));
                }
                let segment: CatalogSegment =
                    parse_store(&bytes).map_err(|_| VaultError::corrupt("segment invalid"))?;
                if canonical_bytes(&segment).ok().as_deref() != Some(bytes.as_slice()) {
                    return Err(VaultError::corrupt("segment bytes"));
                }
                let segment = Arc::new(segment);
                self.segments
                    .lock()
                    .expect("cache")
                    .insert(hash.clone(), segment.clone());
                segment
            }
        };
        if segment.segment_kind != reference.segment_kind
            || segment.record_kind != reference.record_kind
            || segment.prefix != reference.prefix
            || segment.len() as u64 != reference.entry_count
        {
            return Err(VaultError::corrupt("segment reference"));
        }
        Ok(segment)
    }

    fn current_bytes(&self) -> Result<Option<Vec<u8>>> {
        self.root.read(&layout::current_pointer())
    }

    fn load_head(&self) -> Result<Arc<StoredCommit>> {
        let bytes = self
            .current_bytes()?
            .ok_or_else(|| VaultError::recovering("CURRENT missing"))?;
        let pointer: CurrentPointer =
            parse_store(&bytes).map_err(|_| VaultError::recovering("CURRENT unreadable"))?;
        if pointer.vault_id != self.descriptor.vault_id {
            return Err(VaultError::recovering("CURRENT names another vault"));
        }
        let (commit, _) = self
            .load_commit(&pointer.commit_id, Some(&pointer.manifest_sha256))
            .map_err(|_| VaultError::recovering("CURRENT manifest unverifiable"))?;
        if commit.sequence != pointer.sequence {
            return Err(VaultError::recovering("CURRENT sequence"));
        }
        Ok(commit)
    }

    fn pin_of(commit: &StoredCommit) -> CommitPin {
        CommitPin {
            commit_id: commit.commit_id.clone(),
            sequence: commit.sequence,
            policy_epoch: commit.policy_epoch,
            deletion_epoch: commit.deletion_epoch,
        }
    }

    /// Pin the published head for a whole read.
    pub fn pin_current(&self) -> Result<CommitPin> {
        Ok(Self::pin_of(&*self.load_head()?))
    }

    /// The stored commit a pin names: header, epochs, and segment references.
    pub fn stored_commit(&self, pin: &CommitPin) -> Result<Arc<StoredCommit>> {
        let (commit, _) = self.load_commit(&pin.commit_id, None)?;
        if commit.sequence != pin.sequence {
            return Err(VaultError::corrupt("pin sequence"));
        }
        Ok(commit)
    }

    /// The complete `CommitManifest` v1 view of a pinned commit (every
    /// record's latest revision and every object), read from its segments.
    pub fn read_manifest(&self, pin: &CommitPin) -> Result<CommitManifest> {
        let commit = self.stored_commit(pin)?;
        let mut loaded: BTreeMap<Sha256Hex, Arc<CatalogSegment>> = BTreeMap::new();
        for reference in commit.record_segments.iter().chain(&commit.object_segments) {
            loaded.insert(reference.segment_hash.clone(), self.segment(reference)?);
        }
        commit
            .materialize(|hash| loaded.get(hash).map(Arc::as_ref))
            .map_err(|_| VaultError::corrupt("manifest segments"))
    }

    /// The segment of `kind` whose prefix `key` extends, if any.
    fn record_segment(
        &self,
        commit: &StoredCommit,
        kind: RecordKind,
        key: &str,
    ) -> Result<Option<Arc<CatalogSegment>>> {
        match commit
            .record_segments
            .iter()
            .find(|r| r.record_kind == Some(kind) && key.starts_with(r.prefix.as_str()))
        {
            Some(reference) => Ok(Some(self.segment(reference)?)),
            None => Ok(None),
        }
    }

    fn find_record(
        &self,
        commit: &StoredCommit,
        kind: RecordKind,
        id: &str,
    ) -> Result<Option<RecordEntry>> {
        let Some(key) = record_key(kind, id) else {
            return Ok(None);
        };
        let Some(segment) = self.record_segment(commit, kind, &key)? else {
            return Ok(None);
        };
        Ok(segment
            .records
            .binary_search_by(|e| e.record_id.as_str().cmp(id))
            .ok()
            .map(|i| segment.records[i].clone()))
    }

    fn entries_of(&self, commit: &StoredCommit, kind: RecordKind) -> Result<Vec<RecordEntry>> {
        let mut out = Vec::new();
        for reference in commit
            .record_segments
            .iter()
            .filter(|r| r.record_kind == Some(kind))
        {
            out.extend(self.segment(reference)?.records.iter().cloned());
        }
        Ok(out)
    }

    fn find_object(&self, commit: &StoredCommit, hash: &Sha256Hex) -> Result<Option<ObjectItem>> {
        let Some(reference) = commit
            .object_segments
            .iter()
            .find(|r| hash.as_str().starts_with(r.prefix.as_str()))
        else {
            return Ok(None);
        };
        Ok(self
            .segment(reference)?
            .objects
            .iter()
            .find(|o| &o.object_hash == hash)
            .cloned())
    }

    fn objects_of(&self, commit: &StoredCommit) -> Result<Vec<ObjectItem>> {
        let mut out = Vec::new();
        for reference in &commit.object_segments {
            out.extend(self.segment(reference)?.objects.iter().cloned());
        }
        Ok(out)
    }

    /// The catalog entry of one record at a pinned commit.
    pub fn record_entry(
        &self,
        pin: &CommitPin,
        kind: RecordKind,
        id: &str,
    ) -> Result<Option<RecordEntry>> {
        self.find_record(&*self.stored_commit(pin)?, kind, id)
    }

    /// Every catalog entry of one record kind at a pinned commit.
    pub fn record_entries(&self, pin: &CommitPin, kind: RecordKind) -> Result<Vec<RecordEntry>> {
        self.entries_of(&*self.stored_commit(pin)?, kind)
    }

    fn not_found() -> VaultError {
        VaultError::new(MemoryErrorCode::NotFound, Fault::NotFound)
    }

    fn corrupt_record() -> VaultError {
        let mut error = VaultError::new(
            MemoryErrorCode::StorageFailed,
            Fault::Corrupt("record hash"),
        );
        error.error.retryable = false;
        error
    }

    /// Stored bytes of one record revision in the pinned catalog, verified
    /// against the catalog hash. Only the revision the pin catalogs as the
    /// record's latest is served.
    pub fn read_record(&self, pin: &CommitPin, reference: &RecordRef) -> Result<Vec<u8>> {
        let commit = self.stored_commit(pin)?;
        let entry = self
            .find_record(&commit, reference.record_kind, &reference.record_id)?
            .filter(|e| e.revision == reference.revision)
            .ok_or_else(Self::not_found)?;
        let file = entry_file(reference.record_kind, &entry, entry.revision.get())
            .ok_or_else(Self::corrupt_record)?;
        self.read_verified(&file, &entry.content_hash)
    }

    /// Stored bytes of any revision the pinned catalog names for a record:
    /// its latest or an earlier one (history views, full re-validation).
    pub fn read_revision(&self, pin: &CommitPin, reference: &RecordRef) -> Result<Vec<u8>> {
        let commit = self.stored_commit(pin)?;
        let entry = self
            .find_record(&commit, reference.record_kind, &reference.record_id)?
            .ok_or_else(Self::not_found)?;
        let revision = reference.revision.get();
        let hash = entry.hash_of(revision).ok_or_else(Self::not_found)?;
        let file =
            entry_file(reference.record_kind, &entry, revision).ok_or_else(Self::corrupt_record)?;
        self.read_verified(&file, hash)
    }

    fn read_verified(&self, file: &str, hash: &Sha256Hex) -> Result<Vec<u8>> {
        let bytes = self.root.read(file)?.ok_or_else(Self::corrupt_record)?;
        if &sha256(&bytes) != hash {
            return Err(Self::corrupt_record());
        }
        Ok(bytes)
    }

    /// Bytes of an object reachable at the pinned commit, verified by hash.
    pub fn read_object(&self, pin: &CommitPin, hash: &Sha256Hex) -> Result<Vec<u8>> {
        let commit = self.stored_commit(pin)?;
        let item = self
            .find_object(&commit, hash)?
            .ok_or_else(Self::not_found)?;
        let file = object_file_of(&item).ok_or_else(Self::corrupt_record)?;
        self.read_verified(&file, hash)
    }

    /// Parse one cataloged revision (cached after its first verified read).
    fn record_at(&self, kind: RecordKind, entry: &RecordEntry, revision: u64) -> Result<AnyRecord> {
        let key = (kind, entry.record_id.clone(), revision);
        if let Some(record) = self.records.lock().expect("cache").get(&key) {
            return Ok(record.clone());
        }
        let hash = entry.hash_of(revision).ok_or_else(Self::corrupt_record)?;
        let file = entry_file(kind, entry, revision).ok_or_else(Self::corrupt_record)?;
        let bytes = self.read_verified(&file, hash)?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| VaultError::corrupt("record unparseable"))?;
        let record = parse_any(kind, &value).map_err(|_| VaultError::corrupt("record invalid"))?;
        self.records
            .lock()
            .expect("cache")
            .insert(key, record.clone());
        Ok(record)
    }

    /// Every file of a pinned state with the hash it must have: the stored
    /// commits of the chain and every segment they reference, every revision
    /// the pinned catalog names, and every object of the pinned commit.
    pub fn pinned_files(&self, pin: &CommitPin) -> Result<Vec<PinnedFile>> {
        let head = self.stored_commit(pin)?;
        let mut files: BTreeMap<String, Sha256Hex> = BTreeMap::new();
        let mut next = Some(head.commit_id.clone());
        while let Some(id) = next {
            let (commit, hash) = self.load_commit(&id, None)?;
            files.insert(layout::commit_manifest(id.as_str()), hash);
            for reference in commit.record_segments.iter().chain(&commit.object_segments) {
                files.insert(
                    layout::catalog_segment(&reference.segment_hash),
                    reference.segment_hash.clone(),
                );
            }
            next = commit.parent_commit_id.clone();
        }
        for reference in &head.record_segments {
            let kind = reference.record_kind.expect("record segment");
            for entry in &self.segment(reference)?.records {
                for revision in 1..=entry.revision.get() {
                    let file =
                        entry_file(kind, entry, revision).ok_or_else(Self::corrupt_record)?;
                    let hash = entry.hash_of(revision).expect("cataloged revision");
                    files.insert(file, hash.clone());
                }
            }
        }
        for item in self.objects_of(&head)? {
            let file = object_file_of(&item).ok_or_else(Self::corrupt_record)?;
            files.insert(file, item.object_hash.clone());
        }
        Ok(files
            .into_iter()
            .map(|(file, sha256)| PinnedFile { file, sha256 })
            .collect())
    }

    /// Verify every segment, record revision, and object of a commit and
    /// report the damage range. Reports only; never repairs or deletes (V06).
    pub fn verify(&self, pin: &CommitPin) -> Result<IntegrityReport> {
        let commit = self.stored_commit(pin)?;
        let mut report = IntegrityReport::default();
        let check = |file: Option<String>, hash: &Sha256Hex| -> Result<Option<bool>> {
            let bytes = file.map(|f| self.root.read(&f)).transpose()?.flatten();
            Ok(bytes.map(|bytes| &sha256(&bytes) == hash))
        };
        for reference in &commit.record_segments {
            let Ok(segment) = self.segment(reference) else {
                report.damaged_segments.push(reference.segment_hash.clone());
                continue;
            };
            let kind = segment.record_kind.expect("record segment");
            for entry in &segment.records {
                for revision in 1..=entry.revision.get() {
                    report.records_checked += 1;
                    let hash = entry.hash_of(revision).expect("cataloged revision");
                    let reference = RecordRef::new(
                        kind,
                        &entry.record_id,
                        Revision::new(revision).expect("revision"),
                    );
                    match check(entry_file(kind, entry, revision), hash)? {
                        None => report.missing_records.push(reference),
                        Some(false) => report.corrupt_records.push(reference),
                        Some(true) => {}
                    }
                }
            }
        }
        for reference in &commit.object_segments {
            let Ok(segment) = self.segment(reference) else {
                report.damaged_segments.push(reference.segment_hash.clone());
                continue;
            };
            for item in &segment.objects {
                report.objects_checked += 1;
                match check(object_file_of(item), &item.object_hash)? {
                    None => report.missing_objects.push(item.object_hash.clone()),
                    Some(false) => report.corrupt_objects.push(item.object_hash.clone()),
                    Some(true) => {}
                }
            }
        }
        Ok(report)
    }

    /// Re-check a pinned read against the newest head: references deleted
    /// since the pin are refused (non-disclosing `not_found`), and a moved
    /// policy epoch is reported so authorization is decided again (V08).
    pub fn check_fresh(&self, pin: &CommitPin, references: &[RecordRef]) -> Result<Freshness> {
        let head = self.load_head()?;
        let pinned = self.stored_commit(pin)?;
        if head.deletion_epoch > pin.deletion_epoch {
            let known: BTreeSet<String> = self
                .entries_of(&pinned, RecordKind::Tombstone)?
                .into_iter()
                .map(|e| e.record_id)
                .collect();
            for entry in self
                .entries_of(&head, RecordKind::Tombstone)?
                .into_iter()
                .filter(|e| !known.contains(&e.record_id))
            {
                if let AnyRecord::Tombstone(tombstone) =
                    self.record_at(RecordKind::Tombstone, &entry, 1)?
                {
                    let hit = references.iter().any(|r| {
                        tombstone.targets.iter().any(|t| {
                            t.record_kind == r.record_kind
                                && t.record_id == r.record_id
                                && t.revision.is_none_or(|rev| rev == r.revision)
                        })
                    });
                    if hit {
                        return Err(Self::not_found());
                    }
                }
            }
        }
        Ok(Freshness {
            policy_changed: head.policy_epoch != pin.policy_epoch,
            head: Self::pin_of(&head),
        })
    }

    // --------------------------------------------------------------- writes

    fn prepare_records(
        &self,
        staged: &[StagedRecord],
        head: Option<&StoredCommit>,
    ) -> Result<Vec<Prepared>> {
        let mut seen = BTreeSet::new();
        let mut out = Vec::new();
        for record in staged {
            let Some(field) = id_field(record.record_kind) else {
                return Err(VaultError::invalid(vec!["store.kind_not_stored"]));
            };
            let value: Value = serde_json::from_slice(&record.bytes)
                .map_err(|_| VaultError::invalid(vec!["malformed"]))?;
            let parsed = parse_any(record.record_kind, &value)
                .map_err(|e| VaultError::invalid(e.rules()))?;
            if canonical_bytes(&value).ok().as_deref() != Some(record.bytes.as_slice()) {
                return Err(VaultError::invalid(vec!["store.non_canonical"]));
            }
            let id = value[field].as_str().unwrap_or_default().to_owned();
            let revision = if record.record_kind.is_revisioned() {
                value["revision"]
                    .as_u64()
                    .and_then(Revision::new)
                    .ok_or_else(|| VaultError::invalid(vec!["shape"]))?
            } else {
                Revision::new(1).expect("one")
            };
            if id != record.record_id || revision != record.revision {
                return Err(VaultError::invalid(vec!["store.staged_identity"]));
            }
            if !seen.insert((record.record_kind, id.clone())) {
                return Err(VaultError::invalid(vec!["commit.catalog_duplicate"]));
            }
            let existing = match head {
                Some(head) => self.find_record(head, record.record_kind, &id)?,
                None => None,
            };
            let expected_revision = match &existing {
                Some(entry) if record.record_kind.is_revisioned() => entry.revision.get() + 1,
                Some(_) => 0, // single-revision records are never rewritten
                None => 1,
            };
            if revision.get() != expected_revision {
                return Err(VaultError::new(
                    MemoryErrorCode::RevisionConflict,
                    Fault::RevisionMismatch,
                ));
            }
            let session = value["session_id"].as_str();
            let file = record_file(record.record_kind, &id, revision, session)
                .ok_or_else(|| VaultError::invalid(vec!["store.kind_not_stored"]))?;
            out.push(Prepared {
                kind: record.record_kind,
                id,
                revision,
                file,
                bytes: record.bytes.clone(),
                record: parsed,
            });
        }
        Ok(out)
    }

    fn check_objects(&self, objects: &[StagedObject], prepared: &[Prepared]) -> Result<()> {
        let identity_hashes: BTreeSet<&Sha256Hex> = prepared
            .iter()
            .filter_map(|p| match &p.record {
                AnyRecord::Identity(identity) => Some(&identity.content_hash),
                _ => None,
            })
            .collect();
        for object in objects {
            if sha256(&object.bytes) != object.hash {
                return Err(VaultError::invalid(vec!["store.object_hash"]));
            }
            if object.kind == ObjectKind::IdentityMarkdown
                && !identity_hashes.contains(&object.hash)
            {
                return Err(VaultError::invalid(vec!["store.identity_markdown_orphan"]));
            }
        }
        for object in objects
            .iter()
            .filter(|o| o.kind == ObjectKind::SessionContent)
        {
            let referenced = prepared.iter().any(|p| match &p.record {
                AnyRecord::SessionEvent(event) => event
                    .content_ref
                    .as_ref()
                    .is_some_and(|c| c.object_hash == object.hash),
                _ => false,
            });
            if !referenced {
                return Err(VaultError::invalid(vec!["store.session_content_orphan"]));
            }
        }
        for hash in identity_hashes {
            let staged = objects
                .iter()
                .any(|o| o.kind == ObjectKind::IdentityMarkdown && &o.hash == hash);
            if !staged {
                return Err(VaultError::invalid(vec!["store.identity_markdown_missing"]));
            }
        }
        Ok(())
    }

    fn encode_segment(segment: CatalogSegment, out: &mut Vec<(Sha256Hex, Vec<u8>)>) -> SegmentRef {
        let bytes = canonical_bytes(&segment).expect("segment serializes");
        let hash = sha256(&bytes);
        let reference = SegmentRef {
            segment_kind: segment.segment_kind,
            record_kind: segment.record_kind,
            prefix: segment.prefix.clone(),
            entry_count: segment.len() as u64,
            segment_hash: hash.clone(),
        };
        out.push((hash, bytes));
        reference
    }

    /// Fill `header` with the catalog of `head` plus `prepared` and
    /// `objects`: only the leaves they touch are rewritten (and split when
    /// they outgrow the capacity); every other segment is shared by hash.
    fn build(
        &self,
        mut header: StoredCommit,
        head: Option<&StoredCommit>,
        prepared: &[Prepared],
        objects: &[StagedObject],
    ) -> Result<Built> {
        let capacity = header.segment_capacity;
        let mut new_segments = Vec::new();
        let mut record_refs: BTreeMap<(&'static str, String), SegmentRef> = BTreeMap::new();
        for reference in head
            .map(|h| h.record_segments.as_slice())
            .unwrap_or_default()
        {
            let kind = reference.record_kind.expect("record segment");
            record_refs.insert((kind.name(), reference.prefix.clone()), reference.clone());
        }
        let mut by_kind: BTreeMap<&'static str, (RecordKind, Vec<&Prepared>)> = BTreeMap::new();
        for p in prepared {
            by_kind
                .entry(p.kind.name())
                .or_insert((p.kind, Vec::new()))
                .1
                .push(p);
        }
        for (name, (kind, changes)) in by_kind {
            let leaves: BTreeSet<String> = record_refs
                .keys()
                .filter(|(k, _)| *k == name)
                .map(|(_, p)| p.clone())
                .collect();
            let mut touched: BTreeMap<String, Vec<&Prepared>> = BTreeMap::new();
            for p in changes {
                let key = record_key(kind, &p.id)
                    .ok_or_else(|| VaultError::invalid(vec!["ref.kind_prefix"]))?;
                touched.entry(leaf_for(&leaves, &key)).or_default().push(p);
            }
            for (leaf, changes) in touched {
                let mut entries: BTreeMap<String, RecordEntry> = BTreeMap::new();
                if let Some(old) = record_refs.remove(&(name, leaf.clone())) {
                    for e in &self.segment(&old)?.records {
                        entries.insert(e.record_id.clone(), e.clone());
                    }
                }
                for p in changes {
                    let hash = sha256(&p.bytes);
                    let mut groups: BTreeSet<String> = groups_of(&p.record).into_iter().collect();
                    let entry = match entries.remove(&p.id) {
                        Some(mut old) => {
                            groups.extend(old.group_ids.drain(..));
                            old.prior_hashes.push(old.content_hash.clone());
                            RecordEntry {
                                record_id: p.id.clone(),
                                revision: p.revision,
                                content_hash: hash,
                                prior_hashes: old.prior_hashes,
                                group_ids: groups.into_iter().collect(),
                            }
                        }
                        None => RecordEntry {
                            record_id: p.id.clone(),
                            revision: p.revision,
                            content_hash: hash,
                            prior_hashes: Vec::new(),
                            group_ids: groups.into_iter().collect(),
                        },
                    };
                    if entry.prior_hashes.len() as u64 != entry.revision.get() - 1 {
                        return Err(VaultError::new(
                            MemoryErrorCode::RevisionConflict,
                            Fault::RevisionMismatch,
                        ));
                    }
                    entries.insert(p.id.clone(), entry);
                }
                let list: Vec<RecordEntry> = entries.into_values().collect();
                let keys: Vec<String> = list
                    .iter()
                    .map(|e| record_key(kind, &e.record_id).expect("cataloged id"))
                    .collect();
                let key_refs: Vec<&str> = keys.iter().map(String::as_str).collect();
                for (prefix, range) in split(&leaf, &key_refs, capacity) {
                    let segment = CatalogSegment {
                        schema_version: SchemaVersion,
                        segment_kind: SegmentKind::Records,
                        record_kind: Some(kind),
                        prefix: prefix.clone(),
                        records: list[range].to_vec(),
                        objects: Vec::new(),
                    };
                    let reference = Self::encode_segment(segment, &mut new_segments);
                    record_refs.insert((name, prefix), reference);
                }
            }
        }
        header.record_segments = record_refs.into_values().collect();

        let mut object_refs: BTreeMap<String, SegmentRef> = head
            .map(|h| h.object_segments.as_slice())
            .unwrap_or_default()
            .iter()
            .map(|r| (r.prefix.clone(), r.clone()))
            .collect();
        let leaves: BTreeSet<String> = object_refs.keys().cloned().collect();
        let mut touched: BTreeMap<String, Vec<&StagedObject>> = BTreeMap::new();
        for object in objects {
            touched
                .entry(leaf_for(&leaves, object.hash.as_str()))
                .or_default()
                .push(object);
        }
        for (leaf, added) in touched {
            let mut items: BTreeMap<(Sha256Hex, u8), ObjectItem> = BTreeMap::new();
            if let Some(old) = object_refs.remove(&leaf) {
                for item in &self.segment(&old)?.objects {
                    items.insert(
                        (
                            item.object_hash.clone(),
                            object_kind_order(item.object_kind),
                        ),
                        item.clone(),
                    );
                }
            }
            for object in added {
                let stored_with = match object.kind {
                    ObjectKind::IdentityMarkdown => prepared.iter().find_map(|p| match &p.record {
                        AnyRecord::Identity(identity) if identity.content_hash == object.hash => {
                            Some(RecordRef::new(RecordKind::Identity, &p.id, p.revision))
                        }
                        _ => None,
                    }),
                    _ => None,
                };
                items
                    .entry((object.hash.clone(), object_kind_order(object.kind)))
                    .or_insert(ObjectItem {
                        object_hash: object.hash.clone(),
                        object_kind: object.kind,
                        size_bytes: object.bytes.len() as u64,
                        stored_with,
                    });
            }
            let list: Vec<ObjectItem> = items.into_values().collect();
            // Sorted by hash; two kinds of one hash share a key and a leaf.
            let keys: Vec<&str> = list.iter().map(|o| o.object_hash.as_str()).collect();
            for (prefix, range) in split(&leaf, &keys, capacity) {
                let segment = CatalogSegment {
                    schema_version: SchemaVersion,
                    segment_kind: SegmentKind::Objects,
                    record_kind: None,
                    prefix: prefix.clone(),
                    records: Vec::new(),
                    objects: list[range].to_vec(),
                };
                let reference = Self::encode_segment(segment, &mut new_segments);
                object_refs.insert(prefix, reference);
            }
        }
        header.object_segments = object_refs.into_values().collect();

        let mut refs: Vec<RecordRef> = prepared
            .iter()
            .map(|p| RecordRef::new(p.kind, &p.id, p.revision))
            .collect();
        refs.sort_by(|a, b| {
            (a.record_kind.name(), &a.record_id, a.revision).cmp(&(
                b.record_kind.name(),
                &b.record_id,
                b.revision,
            ))
        });
        header.receipt.records = refs;
        header.review_ids = prepared
            .iter()
            .filter(|p| p.kind == RecordKind::Review)
            .map(|p| ReviewId::parse(&p.id).expect("parsed review id"))
            .collect();
        header.tombstone_ids = prepared
            .iter()
            .filter(|p| p.kind == RecordKind::Tombstone)
            .map(|p| DeleteId::parse(&p.id).expect("parsed delete id"))
            .collect();
        let violations = header.validate();
        if !violations.is_empty() {
            return Err(VaultError::invalid(
                violations.iter().map(|v| v.rule).collect(),
            ));
        }
        Ok(Built {
            commit: header,
            segments: new_segments,
        })
    }

    /// Validate the delta on a scoped set (ADR-MEM-39) and the chain step.
    fn check_delta(
        &self,
        commit: &StoredCommit,
        head: Option<&StoredCommit>,
        prepared: &[Prepared],
    ) -> Result<()> {
        let stub = stub_of(commit);
        let delta: Vec<AnyRecord> = prepared.iter().map(|p| p.record.clone()).collect();
        let mut own = RecordSet::default();
        for record in &delta {
            own.push(record.clone());
        }
        let head_stub = head.map(stub_of);
        let mut violations = check_commit_step(&own, head_stub.as_ref(), &stub);
        if self.options.validate_record_set {
            let before = match head {
                Some(head) => self.scoped_set(head, &delta)?,
                None => RecordSet::default(),
            };
            violations.extend(validate_delta(&before, &delta, &[stub]));
        }
        if violations.is_empty() {
            Ok(())
        } else {
            let mut rules: Vec<&'static str> = violations.iter().map(|v| v.rule).collect();
            rules.sort();
            rules.dedup();
            Err(VaultError::invalid(rules))
        }
    }

    /// Every small-kind revision at `head`, plus the bulk documents and
    /// commits that `delta_needs` lists for `delta`.
    fn scoped_set(&self, head: &StoredCommit, delta: &[AnyRecord]) -> Result<RecordSet> {
        let mut set = RecordSet::default();
        let mut loaded: BTreeSet<DocKey> = BTreeSet::new();
        let mut push = |set: &mut RecordSet, kind: RecordKind, entry: &RecordEntry, revision| {
            if loaded.insert((kind, entry.record_id.clone(), revision)) {
                set.push(self.record_at(kind, entry, revision)?);
            }
            Ok::<(), VaultError>(())
        };
        let kinds: BTreeSet<RecordKind> = head
            .record_segments
            .iter()
            .filter_map(|r| r.record_kind)
            .filter(|k| !is_bulk(*k))
            .collect();
        for kind in kinds {
            for entry in self.entries_of(head, kind)? {
                for revision in 1..=entry.revision.get() {
                    push(&mut set, kind, &entry, revision)?;
                }
            }
        }
        for need in delta_needs(&set, delta) {
            match need {
                Need::Revision(kind, id, revision) if is_bulk(kind) => {
                    if let Some(entry) = self.find_record(head, kind, &id)?
                        && revision <= entry.revision.get()
                    {
                        push(&mut set, kind, &entry, revision)?;
                    }
                }
                Need::AllRevisions(kind, id) if is_bulk(kind) => {
                    if let Some(entry) = self.find_record(head, kind, &id)? {
                        for revision in 1..=entry.revision.get() {
                            push(&mut set, kind, &entry, revision)?;
                        }
                    }
                }
                Need::ImportSources(import) => {
                    for entry in self.entries_of(head, RecordKind::Source)? {
                        if entry.group_ids.contains(&import) {
                            for revision in 1..=entry.revision.get() {
                                push(&mut set, RecordKind::Source, &entry, revision)?;
                            }
                        }
                    }
                }
                Need::SessionEvents(session) => {
                    // Event files are grouped by session; the catalog decides
                    // which of them are published.
                    for name in self.root.list(&format!("vault/session-events/{session}"))? {
                        let Some(id) = name.strip_suffix(".json") else {
                            continue;
                        };
                        if let Some(entry) = self.find_record(head, RecordKind::SessionEvent, id)?
                            && entry.group_ids.first() == Some(&session)
                        {
                            push(&mut set, RecordKind::SessionEvent, &entry, 1)?;
                        }
                    }
                }
                Need::Commit(id) => {
                    if let Ok(id) = CommitId::parse(&id)
                        && let Ok((commit, _)) = self.load_commit(&id, None)
                        && self.on_chain(head, &commit.commit_id, commit.sequence)?
                    {
                        set.commits.push(stub_of(&commit));
                    }
                }
                _ => {}
            }
        }
        Ok(set)
    }

    /// Move a file found at a name this transaction must write to the
    /// orphans area. Such a file was never published at that name.
    fn quarantine(&self, file: &str, operation: &OperationId) -> Result<()> {
        let target = format!(
            "{}/{}/{}",
            layout::orphans_dir(),
            operation,
            file.replace('/', "_")
        );
        self.root.rename(file, &target)
    }

    /// Write one immutable file at its final, still unreferenced name.
    fn place(&self, file: &str, bytes: &[u8], operation: &OperationId) -> Result<()> {
        match self.root.read(file)? {
            Some(existing) if existing == bytes => Ok(()),
            Some(_) => {
                self.quarantine(file, operation)?;
                self.root.write_new(file, bytes)
            }
            None => self.root.write_new(file, bytes),
        }
    }

    fn write_transaction(
        &self,
        built: &Built,
        prepared: &[Prepared],
        objects: &[StagedObject],
        idempotency: Option<(&IdempotencyScope, &Sha256Hex)>,
    ) -> Result<()> {
        let faults = &self.options.faults;
        let commit = &built.commit;
        let operation = &commit.operation_id;
        // An intent marker makes an interrupted transaction visible to
        // health until the next writer clears it. Records, objects, and
        // segments are then written once, flushed, directly at their final
        // names: until CURRENT names a commit that references them, nothing
        // reads them.
        let staging = layout::staging_dir(operation.as_str());
        self.root.write_new(
            &format!("{staging}/intent"),
            commit.commit_id.as_str().as_bytes(),
        )?;
        let mut object_placements = Vec::new();
        for object in objects {
            let file = match layout::object_path(object.kind, &object.hash) {
                Some(path) => path,
                None => prepared
                    .iter()
                    .find_map(|p| match &p.record {
                        AnyRecord::Identity(identity) if identity.content_hash == object.hash => {
                            layout::record_path(RecordKind::Identity, &p.id, p.revision)
                        }
                        _ => None,
                    })
                    .ok_or_else(|| VaultError::invalid(vec!["store.identity_markdown_orphan"]))?,
            };
            object_placements.push((file, object.bytes.as_slice()));
        }
        faults.io(FaultPoint::StagingWritten)?;
        for p in prepared {
            self.place(&p.file, &p.bytes, operation)?;
        }
        faults.io(FaultPoint::RecordsPlaced)?;
        for (file, bytes) in &object_placements {
            self.place(file, bytes, operation)?;
        }
        faults.io(FaultPoint::ObjectsPlaced)?;
        for (hash, bytes) in &built.segments {
            self.place(&layout::catalog_segment(hash), bytes, operation)?;
        }
        faults.io(FaultPoint::SegmentsPlaced)?;
        let bytes = canonical_bytes(commit).expect("commit serializes");
        self.root
            .write_new(&layout::commit_manifest(commit.commit_id.as_str()), &bytes)?;
        faults.io(FaultPoint::ManifestWritten)?;
        if let Some((scope, payload)) = idempotency {
            let entry = IdempotencyEntry {
                schema_version: SchemaVersion,
                vault_id: self.descriptor.vault_id.clone(),
                principal_id: scope.principal_id.clone(),
                operation_kind: scope.operation_kind,
                key_hash: scope.key_hash.clone(),
                request_payload_hash: payload.clone(),
                commit_id: commit.commit_id.clone(),
                sequence: commit.sequence,
            };
            let file = layout::idempotency_entry(&idempotency_scope_hash(
                &scope.principal_id,
                scope.operation_kind,
                &scope.key_hash,
            ));
            self.root
                .write_atomic(&file, &canonical_bytes(&entry).expect("entry"))?;
        }
        faults.io(FaultPoint::IdempotencyWritten)?;
        Ok(())
    }

    /// Replace `CURRENT` (the publication) and append the journal line.
    fn publish(&self, commit: &StoredCommit) -> Result<()> {
        let faults = &self.options.faults;
        let (_, hash) = self.load_commit(&commit.commit_id, None)?;
        let pointer = CurrentPointer {
            schema_version: SchemaVersion,
            vault_id: self.descriptor.vault_id.clone(),
            commit_id: commit.commit_id.clone(),
            sequence: commit.sequence,
            manifest_sha256: hash.clone(),
        };
        faults.io(FaultPoint::BeforeCurrent)?;
        self.root.write_atomic(
            &layout::current_pointer(),
            &canonical_bytes(&pointer).expect("pointer"),
        )?;
        faults.io(FaultPoint::AfterCurrent)?;
        self.append_journal(commit, &hash);
        faults.io(FaultPoint::AfterJournal)?;
        let _ = self
            .root
            .remove_staging(&layout::staging_dir(commit.operation_id.as_str()));
        Ok(())
    }

    /// The journal is recovery evidence, not the commit: a failed append is
    /// not reported as a failed commit (that would invite a duplicate retry).
    fn append_journal(&self, commit: &StoredCommit, hash: &Sha256Hex) {
        let Ok(published_at) = commit_time(self.clock.as_ref(), None, ComponentId::Vault) else {
            return;
        };
        let record = PublishRecord {
            schema_version: SchemaVersion,
            vault_id: self.descriptor.vault_id.clone(),
            commit_id: commit.commit_id.clone(),
            sequence: commit.sequence,
            manifest_sha256: hash.clone(),
            published_at,
        };
        let line = serde_json::to_vec(&record).expect("journal line");
        let _ = self.root.append_line(&layout::publish_journal(), &line);
    }

    fn clear_staging(&self) -> Result<()> {
        for name in self.root.list("vault/staging")? {
            self.root.remove_staging(&format!("vault/staging/{name}"))?;
        }
        Ok(())
    }

    /// The writer lock, for maintenance operations in this crate.
    pub(crate) fn writer_guard(&self) -> Result<WriterGuard> {
        self.lock()
    }

    fn lock(&self) -> Result<WriterGuard> {
        let guard = lock::acquire(&self.root, self.options.lock_wait)?;
        self.options.faults.io(FaultPoint::AfterLock)?;
        Ok(guard)
    }

    /// Is `commit_id` at `sequence` on the published chain ending at `head`?
    fn on_chain(&self, head: &StoredCommit, commit_id: &CommitId, sequence: u64) -> Result<bool> {
        if sequence > head.sequence {
            return Ok(false);
        }
        let mut current = Arc::new(head.clone());
        while current.sequence > sequence {
            let Some(parent) = current.parent_commit_id.clone() else {
                return Ok(false);
            };
            current = self.load_commit(&parent, None)?.0;
        }
        Ok(&current.commit_id == commit_id)
    }

    fn published_receipt(
        &self,
        head: &StoredCommit,
        scope: &IdempotencyScope,
    ) -> Result<Option<(CommitId, Sha256Hex, OperationReceipt)>> {
        let file = layout::idempotency_entry(&idempotency_scope_hash(
            &scope.principal_id,
            scope.operation_kind,
            &scope.key_hash,
        ));
        let Some(bytes) = self.root.read(&file)? else {
            return Ok(None);
        };
        let Ok(entry) = parse_store::<IdempotencyEntry>(&bytes) else {
            return Ok(None); // a torn entry from a crashed attempt
        };
        if entry.vault_id != self.descriptor.vault_id
            || !self.on_chain(head, &entry.commit_id, entry.sequence)?
        {
            return Ok(None);
        }
        let (commit, _) = self.load_commit(&entry.commit_id, None)?;
        if commit.idempotency_key_hash.as_ref() != Some(&scope.key_hash)
            || commit.principal.actor_id != scope.principal_id
            || commit.operation_kind != scope.operation_kind
        {
            return Err(VaultError::corrupt("idempotency entry"));
        }
        Ok(Some((
            commit.commit_id.clone(),
            commit.request_payload_hash.clone(),
            commit.receipt.clone(),
        )))
    }

    pub fn find_receipt(
        &self,
        scope: &IdempotencyScope,
    ) -> Result<Option<(CommitId, Sha256Hex, OperationReceipt)>> {
        let head = self.load_head()?;
        self.published_receipt(&head, scope)
    }

    /// Apply one transaction (see the module documentation for the order).
    pub fn commit(&self, request: CommitRequest) -> Result<CommitOutcome> {
        if request.operation_kind == OperationKind::Genesis
            || request.operation_kind == OperationKind::RestoreAdopt
        {
            return Err(VaultError::invalid(vec!["store.operation_kind"]));
        }
        if request.idempotency.principal_id != request.principal.actor_id
            || request.idempotency.operation_kind != request.operation_kind
        {
            return Err(VaultError::invalid(vec!["store.idempotency_scope"]));
        }
        let _guard = self.lock()?;
        self.clear_staging()?;
        let head = self.load_head()?;
        match self.published_receipt(&head, &request.idempotency)? {
            Some((commit_id, payload, receipt)) if payload == request.request_payload_hash => {
                return Ok(CommitOutcome::Replayed { commit_id, receipt });
            }
            Some(_) => {
                return Err(VaultError::new(
                    MemoryErrorCode::IdempotencyConflict,
                    Fault::IdempotencyConflict,
                ));
            }
            None => {}
        }
        if request
            .expected_commit_id
            .as_ref()
            .is_some_and(|expected| expected != &head.commit_id)
        {
            return Err(VaultError::new(
                MemoryErrorCode::RevisionConflict,
                Fault::HeadMoved,
            ));
        }
        for (kind, id, expected) in &request.expected_revisions {
            let actual = self.find_record(&head, *kind, id)?.map(|e| e.revision);
            if actual != *expected {
                return Err(VaultError::new(
                    MemoryErrorCode::RevisionConflict,
                    Fault::RevisionMismatch,
                ));
            }
        }
        if request.records.is_empty() && request.objects.is_empty() {
            return Err(VaultError::invalid(vec!["store.empty_commit"]));
        }
        let manifest_file = layout::commit_manifest(request.commit_id.as_str());
        if self.root.exists(&manifest_file)? {
            // Published under this ID: a reuse. Otherwise it is the commit
            // of an attempt that crashed before publication: quarantine it.
            let published = match self.load_commit(&request.commit_id, None) {
                Ok((existing, _)) => {
                    self.on_chain(&head, &existing.commit_id, existing.sequence)?
                }
                Err(_) => false,
            };
            if published {
                return Err(VaultError::invalid(vec!["store.commit_id_reused"]));
            }
            self.commits
                .lock()
                .expect("cache")
                .remove(request.commit_id.as_str());
            let operation = OperationId::from_random(self.ids.random_16());
            self.quarantine(&manifest_file, &operation)?;
        }
        let prepared = self.prepare_records(&request.records, Some(&head))?;
        self.check_objects(&request.objects, &prepared)?;
        let tombstone_epochs: BTreeSet<u64> = prepared
            .iter()
            .filter_map(|p| match &p.record {
                AnyRecord::Tombstone(t) => Some(t.deletion_epoch),
                _ => None,
            })
            .collect();
        let deletion_epoch = match tombstone_epochs.len() {
            0 => head.deletion_epoch,
            1 if tombstone_epochs.contains(&(head.deletion_epoch + 1)) => head.deletion_epoch + 1,
            _ => return Err(VaultError::invalid(vec!["store.deletion_epoch"])),
        };
        let policy_epoch =
            head.policy_epoch + u64::from(prepared.iter().any(|p| p.kind == RecordKind::Policy));
        let created_at = commit_time(
            self.clock.as_ref(),
            Some(&head.created_at),
            ComponentId::Vault,
        )
        .map_err(|_| VaultError::new(MemoryErrorCode::ClockRegression, Fault::ClockRegression))?;
        // An import past `archiving` is only committed together with (or
        // after) the exact bytes it received (IMPORT_REVIEW §2, I01).
        for p in &prepared {
            if let AnyRecord::Import(import) = &p.record
                && !matches!(
                    import.status,
                    enouia_memory_contract::import::ImportStatus::Planned
                        | enouia_memory_contract::import::ImportStatus::Archiving
                )
            {
                let staged = request
                    .objects
                    .iter()
                    .any(|o| o.hash == import.input_object_hash && o.kind == ObjectKind::Raw);
                let stored = self
                    .find_object(&head, &import.input_object_hash)?
                    .is_some_and(|o| o.object_kind == ObjectKind::Raw);
                if !staged && !stored {
                    return Err(VaultError::invalid(vec!["store.import_raw_missing"]));
                }
            }
        }
        let operation_id = OperationId::from_random(self.ids.random_16());
        let header = StoredCommit {
            schema_version: SchemaVersion,
            commit_id: request.commit_id.clone(),
            format_version: LayoutFormat,
            parent_commit_id: Some(head.commit_id.clone()),
            sequence: head.sequence + 1,
            vault_id: self.descriptor.vault_id.clone(),
            writer_device_id: self.device_id.clone(),
            principal: request.principal.clone(),
            operation_id: operation_id.clone(),
            operation_kind: request.operation_kind,
            idempotency_key_hash: Some(request.idempotency.key_hash.clone()),
            request_payload_hash: request.request_payload_hash.clone(),
            created_at,
            segment_capacity: self.options.segment_capacity,
            record_segments: Vec::new(),
            object_segments: Vec::new(),
            review_ids: Vec::new(),
            tombstone_ids: Vec::new(),
            policy_epoch,
            deletion_epoch,
            receipt: OperationReceipt {
                operation_id,
                result: ReceiptResult::Committed,
                records: Vec::new(),
            },
        };
        let built = self.build(header, Some(&head), &prepared, &request.objects)?;
        self.check_delta(&built.commit, Some(&head), &prepared)?;
        self.write_transaction(
            &built,
            &prepared,
            &request.objects,
            Some((&request.idempotency, &request.request_payload_hash)),
        )?;
        self.publish(&built.commit)?;
        Ok(CommitOutcome::Committed {
            commit_id: built.commit.commit_id.clone(),
            receipt: built.commit.receipt.clone(),
        })
    }

    // ------------------------------------------------------------- recovery

    fn journal(&self) -> Result<Vec<PublishRecord>> {
        let Some(bytes) = self.root.read(&layout::publish_journal())? else {
            return Ok(Vec::new());
        };
        Ok(bytes
            .split(|&b| b == b'\n')
            .filter(|line| !line.is_empty())
            .filter_map(|line| parse_store::<PublishRecord>(line).ok())
            .filter(|r| r.vault_id == self.descriptor.vault_id)
            .collect())
    }

    fn complete(&self, commit_id: &CommitId, hash: &Sha256Hex) -> bool {
        let Ok((commit, _)) = self.load_commit(commit_id, Some(hash)) else {
            return false;
        };
        let pin = Self::pin_of(&commit);
        self.verify(&pin).is_ok_and(|r| r.is_clean())
    }

    /// Inspect `CURRENT` and list recovery points with their evidence. The
    /// Vault does not choose; the owner does (`adopt_recovery_point`).
    pub fn recovery_report(&self) -> Result<RecoveryReport> {
        let bytes = self.current_bytes()?;
        let current = match &bytes {
            None => CurrentState::Missing,
            Some(bytes) => match parse_store::<CurrentPointer>(bytes) {
                Err(_) => CurrentState::Unreadable,
                Ok(pointer) if pointer.vault_id != self.descriptor.vault_id => {
                    CurrentState::ForeignVault
                }
                Ok(_) => match self.load_head() {
                    Ok(_) => CurrentState::Valid,
                    Err(_) => CurrentState::ManifestUnverifiable,
                },
            },
        };
        let mut candidates = Vec::new();
        let mut seen = BTreeSet::new();
        let mut journal = self.journal()?;
        journal.sort_by_key(|r| std::cmp::Reverse(r.sequence));
        for record in journal {
            if !seen.insert(record.commit_id.to_string()) {
                continue;
            }
            candidates.push(RecoveryCandidate {
                complete: self.complete(&record.commit_id, &record.manifest_sha256),
                commit_id: record.commit_id,
                sequence: record.sequence,
                manifest_sha256: record.manifest_sha256,
                evidence: RecoveryEvidence::PublishJournal,
            });
        }
        for name in self.root.list("vault/commits")? {
            let Some(id) = name
                .strip_suffix(".json")
                .and_then(|n| CommitId::parse(n).ok())
            else {
                continue;
            };
            if seen.contains(id.as_str()) {
                continue;
            }
            let Ok((commit, hash)) = self.load_commit(&id, None) else {
                continue;
            };
            if self.complete(&id, &hash) {
                candidates.push(RecoveryCandidate {
                    commit_id: id,
                    sequence: commit.sequence,
                    manifest_sha256: hash,
                    evidence: RecoveryEvidence::VerifiedUnpublished,
                    complete: true,
                });
            }
        }
        candidates.sort_by(|a, b| {
            (b.evidence == RecoveryEvidence::PublishJournal, b.sequence)
                .cmp(&(a.evidence == RecoveryEvidence::PublishJournal, a.sequence))
        });
        Ok(RecoveryReport {
            current,
            current_sha256: bytes.as_deref().map(sha256),
            candidates,
        })
    }

    /// The owner's explicit choice of a recovery point when `CURRENT` cannot
    /// be verified. Only a complete candidate from the report is accepted; a
    /// receipt is written before `CURRENT` is replaced.
    pub fn adopt_recovery_point(
        &self,
        commit_id: &CommitId,
        approved_by: &ActorRef,
        trusted_surface: TrustedSurface,
    ) -> Result<RecoveryReceipt> {
        if approved_by.actor_type != ActorType::Owner {
            return Err(VaultError::invalid(vec!["recovery.owner_required"]));
        }
        let _guard = self.lock()?;
        let report = self.recovery_report()?;
        if report.current == CurrentState::Valid {
            return Err(VaultError::invalid(vec!["recovery.current_valid"]));
        }
        let candidate = report
            .candidates
            .iter()
            .find(|c| &c.commit_id == commit_id && c.complete)
            .ok_or_else(|| VaultError::invalid(vec!["recovery.candidate"]))?;
        let receipt = RecoveryReceipt {
            schema_version: SchemaVersion,
            recovery_id: OperationId::from_random(self.ids.random_16()),
            vault_id: self.descriptor.vault_id.clone(),
            adopted_commit_id: candidate.commit_id.clone(),
            adopted_sequence: candidate.sequence,
            manifest_sha256: candidate.manifest_sha256.clone(),
            previous_current_sha256: report.current_sha256.clone(),
            evidence: candidate.evidence,
            approved_by: approved_by.clone(),
            trusted_surface,
            created_at: commit_time(self.clock.as_ref(), None, ComponentId::Vault)?,
        };
        self.root.write_new(
            &layout::recovery_receipt(receipt.recovery_id.as_str()),
            &canonical_bytes(&receipt).expect("receipt"),
        )?;
        let (commit, _) = self.load_commit(commit_id, Some(&candidate.manifest_sha256))?;
        self.publish(&commit)?;
        Ok(receipt)
    }
}

impl enouia_memory_contract::ports::VaultReader for Vault {
    fn pin_current(&self) -> std::result::Result<CommitPin, enouia_memory_contract::MemoryError> {
        Vault::pin_current(self).map_err(Into::into)
    }

    fn read_manifest(
        &self,
        pin: &CommitPin,
    ) -> std::result::Result<CommitManifest, enouia_memory_contract::MemoryError> {
        Vault::read_manifest(self, pin).map_err(Into::into)
    }

    fn read_record(
        &self,
        pin: &CommitPin,
        record: &RecordRef,
    ) -> std::result::Result<Vec<u8>, enouia_memory_contract::MemoryError> {
        Vault::read_record(self, pin, record).map_err(Into::into)
    }

    fn read_object(
        &self,
        pin: &CommitPin,
        hash: &Sha256Hex,
    ) -> std::result::Result<Vec<u8>, enouia_memory_contract::MemoryError> {
        Vault::read_object(self, pin, hash).map_err(Into::into)
    }
}

impl enouia_memory_contract::ports::VaultWriter for Vault {
    fn commit(
        &self,
        request: CommitRequest,
    ) -> std::result::Result<CommitOutcome, enouia_memory_contract::MemoryError> {
        Vault::commit(self, request).map_err(Into::into)
    }

    fn find_receipt(
        &self,
        scope: &IdempotencyScope,
    ) -> std::result::Result<
        Option<(CommitId, Sha256Hex, OperationReceipt)>,
        enouia_memory_contract::MemoryError,
    > {
        Vault::find_receipt(self, scope).map_err(Into::into)
    }
}

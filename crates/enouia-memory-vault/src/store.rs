//! The Vault store (MV-1.2): immutable revisions, a complete commit manifest,
//! one atomically replaced `CURRENT`, idempotent receipts, pinned reads, and
//! owner-driven recovery.
//!
//! Transaction order (MEMORY_ARCHITECTURE §6), each step flushed:
//!
//! 1. take the OS writer lock; clear unpublished staging; load and verify the
//!    head named by `CURRENT` (never guess one);
//! 2. idempotency: a published receipt for the same scope replays, a
//!    different payload conflicts; then check the expected head/revisions;
//! 3. validate every staged record and object, then the whole record set;
//! 4. write an intent marker in `staging/<operation>/`, then records and
//!    objects once at their final (still unreferenced) names, then the
//!    commit manifest and the idempotency entry;
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
use enouia_memory_contract::commit::{
    CatalogEntry, CommitManifest, FormatVersion, ObjectEntry, ObjectKind, OperationKind,
    OperationReceipt, ReceiptResult,
};
use enouia_memory_contract::common::{ActorRef, ActorType, TrustedSurface};
use enouia_memory_contract::foundation::{Clock, ComponentId};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{CommitId, DeleteId, DeviceId, OperationId, ReviewId, VaultId};
use enouia_memory_contract::json::{Revision, SchemaVersion, canonical_bytes};
use enouia_memory_contract::layout;
use enouia_memory_contract::ports::{
    CommitOutcome, CommitPin, CommitRequest, IdSource, IdempotencyScope, StagedObject,
    StagedRecord, commit_time,
};
use enouia_memory_contract::record::{AnyRecord, RecordKind, RecordRef, parse_any, parse_record};
use enouia_memory_contract::set::{RecordSet, validate_set};
use enouia_memory_contract::store::{
    CurrentPointer, IdempotencyEntry, PublishRecord, RecoveryEvidence, RecoveryReceipt,
    VaultDescriptor, idempotency_scope_hash, parse_store,
};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct VaultOptions {
    /// How long a writer waits for the OS lock before returning `busy`.
    pub lock_wait: Duration,
    /// Run every cross-record rule (`set::validate_set`) over the complete
    /// history before publishing. Costs a full read of new revisions only;
    /// earlier ones are cached after their first verified read.
    pub validate_record_set: bool,
    pub faults: Faults,
}

impl Default for VaultOptions {
    fn default() -> Self {
        Self {
            lock_wait: Duration::from_secs(2),
            validate_record_set: true,
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
    /// Every catalog record and object of the commit is present and verified.
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
    pub records_checked: usize,
    pub objects_checked: usize,
    pub missing_records: Vec<RecordRef>,
    pub corrupt_records: Vec<RecordRef>,
    pub missing_objects: Vec<Sha256Hex>,
    pub corrupt_objects: Vec<Sha256Hex>,
}

impl IntegrityReport {
    pub fn is_clean(&self) -> bool {
        self.missing_records.is_empty()
            && self.corrupt_records.is_empty()
            && self.missing_objects.is_empty()
            && self.corrupt_objects.is_empty()
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

struct Head {
    manifest: Arc<CommitManifest>,
}

type DocKey = (RecordKind, String, u64);
/// The manifests of a chain (head first) and every document they catalog.
type History = (Vec<Arc<CommitManifest>>, BTreeMap<DocKey, Sha256Hex>);

pub struct Vault {
    root: ManagedRoot,
    descriptor: VaultDescriptor,
    device_id: DeviceId,
    clock: Arc<dyn Clock + Send + Sync>,
    ids: Arc<dyn IdSource + Send + Sync>,
    options: VaultOptions,
    manifests: Mutex<BTreeMap<String, (Arc<CommitManifest>, Sha256Hex)>>,
    records: Mutex<BTreeMap<DocKey, AnyRecord>>,
    event_sessions: Mutex<BTreeMap<String, String>>,
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

/// Final file of a stored record revision.
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

/// The record kind's wire name (`snake_case`), used for canonical ordering.
fn kind_name(kind: RecordKind) -> &'static str {
    match kind {
        RecordKind::Source => "source",
        RecordKind::Attachment => "attachment",
        RecordKind::Project => "project",
        RecordKind::Memory => "memory",
        RecordKind::Candidate => "candidate",
        RecordKind::Review => "review",
        RecordKind::Identity => "identity",
        RecordKind::Session => "session",
        RecordKind::SessionEvent => "session_event",
        RecordKind::Checkpoint => "checkpoint",
        RecordKind::Commit => "commit",
        RecordKind::Tombstone => "tombstone",
        RecordKind::PurgeReceipt => "purge_receipt",
        RecordKind::AuditEvent => "audit_event",
        RecordKind::Capsule => "capsule",
        RecordKind::Inspection => "inspection",
        RecordKind::Dispatch => "dispatch",
        RecordKind::ProviderCapabilities => "provider_capabilities",
        RecordKind::Approval => "approval",
        RecordKind::Policy => "policy",
        RecordKind::Import => "import",
    }
}

fn kind_order(kind: ObjectKind) -> u8 {
    match kind {
        ObjectKind::Raw => 0,
        ObjectKind::Asset => 1,
        ObjectKind::SessionContent => 2,
        ObjectKind::IdentityMarkdown => 3,
    }
}

fn push_record(set: &mut RecordSet, record: AnyRecord) {
    match record {
        AnyRecord::Source(r) => set.sources.push(r),
        AnyRecord::Attachment(r) => set.attachments.push(r),
        AnyRecord::Project(r) => set.projects.push(r),
        AnyRecord::Memory(r) => set.memories.push(*r),
        AnyRecord::Candidate(r) => set.candidates.push(*r),
        AnyRecord::Review(r) => set.reviews.push(r),
        AnyRecord::Identity(r) => set.identities.push(r),
        AnyRecord::Session(r) => set.sessions.push(r),
        AnyRecord::SessionEvent(r) => set.session_events.push(r),
        AnyRecord::Checkpoint(r) => set.checkpoints.push(r),
        AnyRecord::Commit(r) => set.commits.push(r),
        AnyRecord::Tombstone(r) => set.tombstones.push(r),
        AnyRecord::PurgeReceipt(r) => set.purge_receipts.push(r),
        AnyRecord::AuditEvent(r) => set.audit_events.push(r),
        AnyRecord::Capsule(r) => set.capsules.push(*r),
        AnyRecord::Inspection(r) => set.inspections.push(r),
        AnyRecord::Dispatch(r) => set.dispatches.push(r),
        AnyRecord::ProviderCapabilities(_) => {}
        AnyRecord::Approval(r) => set.approvals.push(*r),
        AnyRecord::Policy(r) => set.policies.push(*r),
        AnyRecord::Import(r) => set.imports.push(*r),
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
            format_version: FormatVersion,
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
            manifests: Mutex::new(BTreeMap::new()),
            records: Mutex::new(BTreeMap::new()),
            event_sessions: Mutex::new(BTreeMap::new()),
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
        let mut manifest = CommitManifest {
            schema_version: SchemaVersion,
            commit_id: request.commit_id.clone(),
            format_version: FormatVersion,
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
            catalog: Vec::new(),
            objects: Vec::new(),
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
        vault.fill_catalog(&mut manifest, None, &prepared, &[]);
        vault.check_manifest_and_set(&manifest, None, &prepared)?;
        vault.write_transaction(&manifest, &prepared, &[], None)?;
        vault.root.write_new(
            &layout::vault_descriptor(),
            &canonical_bytes(&vault.descriptor).expect("descriptor"),
        )?;
        vault.publish(&manifest)?;
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
            // A newer layout is read-only for this build, never "corrupt".
            let newer = serde_json::from_slice::<Value>(&bytes).is_ok_and(|v| {
                v["schema_version"].as_i64().is_some_and(|s| s > 1)
                    || v["format_version"].as_u64().is_some_and(|f| f > 1)
            });
            if newer {
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
            manifests: Mutex::new(BTreeMap::new()),
            records: Mutex::new(BTreeMap::new()),
            event_sessions: Mutex::new(BTreeMap::new()),
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

    fn load_manifest(
        &self,
        commit_id: &CommitId,
        expected: Option<&Sha256Hex>,
    ) -> Result<(Arc<CommitManifest>, Sha256Hex)> {
        if let Some((manifest, hash)) = self
            .manifests
            .lock()
            .expect("cache")
            .get(commit_id.as_str())
        {
            if expected.is_some_and(|e| e != hash) {
                return Err(VaultError::corrupt("manifest hash"));
            }
            return Ok((manifest.clone(), hash.clone()));
        }
        let bytes = self
            .root
            .read(&layout::commit_manifest(commit_id.as_str()))?
            .ok_or_else(|| VaultError::corrupt("manifest missing"))?;
        let hash = sha256(&bytes);
        if expected.is_some_and(|e| e != &hash) {
            return Err(VaultError::corrupt("manifest hash"));
        }
        let manifest: CommitManifest =
            parse_record(&bytes).map_err(|_| VaultError::corrupt("manifest unparseable"))?;
        if &manifest.commit_id != commit_id
            || manifest.vault_id != self.descriptor.vault_id
            || canonical_bytes(&manifest).ok().as_deref() != Some(bytes.as_slice())
        {
            return Err(VaultError::corrupt("manifest identity"));
        }
        let manifest = Arc::new(manifest);
        self.manifests
            .lock()
            .expect("cache")
            .insert(commit_id.to_string(), (manifest.clone(), hash.clone()));
        Ok((manifest, hash))
    }

    fn current_bytes(&self) -> Result<Option<Vec<u8>>> {
        self.root.read(&layout::current_pointer())
    }

    fn load_head(&self) -> Result<Head> {
        let bytes = self
            .current_bytes()?
            .ok_or_else(|| VaultError::recovering("CURRENT missing"))?;
        let pointer: CurrentPointer =
            parse_store(&bytes).map_err(|_| VaultError::recovering("CURRENT unreadable"))?;
        if pointer.vault_id != self.descriptor.vault_id {
            return Err(VaultError::recovering("CURRENT names another vault"));
        }
        let (manifest, _) = self
            .load_manifest(&pointer.commit_id, Some(&pointer.manifest_sha256))
            .map_err(|_| VaultError::recovering("CURRENT manifest unverifiable"))?;
        if manifest.sequence != pointer.sequence {
            return Err(VaultError::recovering("CURRENT sequence"));
        }
        Ok(Head { manifest })
    }

    fn pin_of(manifest: &CommitManifest) -> CommitPin {
        CommitPin {
            commit_id: manifest.commit_id.clone(),
            sequence: manifest.sequence,
            policy_epoch: manifest.policy_epoch,
            deletion_epoch: manifest.deletion_epoch,
        }
    }

    /// Pin the published head for a whole read.
    pub fn pin_current(&self) -> Result<CommitPin> {
        Ok(Self::pin_of(&self.load_head()?.manifest))
    }

    pub fn read_manifest(&self, pin: &CommitPin) -> Result<CommitManifest> {
        let (manifest, _) = self.load_manifest(&pin.commit_id, None)?;
        if manifest.sequence != pin.sequence {
            return Err(VaultError::corrupt("pin sequence"));
        }
        Ok((*manifest).clone())
    }

    fn session_of_event(&self, event_id: &str) -> Result<Option<String>> {
        if let Some(session) = self.event_sessions.lock().expect("cache").get(event_id) {
            return Ok(Some(session.clone()));
        }
        for session in self.root.list("vault/session-events")? {
            if self
                .root
                .exists(&layout::session_event(&session, event_id))?
            {
                self.event_sessions
                    .lock()
                    .expect("cache")
                    .insert(event_id.to_owned(), session.clone());
                return Ok(Some(session));
            }
        }
        Ok(None)
    }

    fn file_of(&self, reference: &RecordRef) -> Result<Option<String>> {
        let session = if reference.record_kind == RecordKind::SessionEvent {
            match self.session_of_event(&reference.record_id)? {
                Some(session) => Some(session),
                None => return Ok(None),
            }
        } else {
            None
        };
        Ok(record_file(
            reference.record_kind,
            &reference.record_id,
            reference.revision,
            session.as_deref(),
        ))
    }

    fn catalog_entry<'a>(
        manifest: &'a CommitManifest,
        reference: &RecordRef,
    ) -> Option<&'a CatalogEntry> {
        manifest.catalog.iter().find(|e| {
            e.record_kind == reference.record_kind
                && e.record_id == reference.record_id
                && e.revision == reference.revision
        })
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
    /// against the catalog hash. Only revisions named by the pin are served.
    pub fn read_record(&self, pin: &CommitPin, reference: &RecordRef) -> Result<Vec<u8>> {
        let manifest = self.read_manifest(pin)?;
        let entry = Self::catalog_entry(&manifest, reference).ok_or_else(Self::not_found)?;
        self.read_verified(reference, &entry.content_hash)
    }

    fn read_verified(&self, reference: &RecordRef, hash: &Sha256Hex) -> Result<Vec<u8>> {
        let file = self.file_of(reference)?.ok_or_else(Self::corrupt_record)?;
        let bytes = self.root.read(&file)?.ok_or_else(Self::corrupt_record)?;
        if &sha256(&bytes) != hash {
            return Err(Self::corrupt_record());
        }
        Ok(bytes)
    }

    fn object_file(
        &self,
        manifest: &CommitManifest,
        entry: &ObjectEntry,
    ) -> Result<Option<String>> {
        if let Some(path) = layout::object_path(entry.object_kind, &entry.object_hash) {
            return Ok(Some(path));
        }
        // Identity Markdown lives beside the identity revision that named it
        // first; that revision may be older than the pinned catalog's, so
        // the chain is searched back to genesis (oldest match wins).
        let mut found = None;
        let mut current = Arc::new(manifest.clone());
        loop {
            for catalog in current
                .catalog
                .iter()
                .filter(|e| e.record_kind == RecordKind::Identity)
            {
                let reference =
                    RecordRef::new(RecordKind::Identity, &catalog.record_id, catalog.revision);
                let record = self.record_at(&reference, &catalog.content_hash)?;
                if let AnyRecord::Identity(identity) = record
                    && identity.content_hash == entry.object_hash
                {
                    found = layout::record_path(
                        RecordKind::Identity,
                        &catalog.record_id,
                        catalog.revision,
                    );
                }
            }
            let Some(parent) = current.parent_commit_id.clone() else {
                break;
            };
            current = self.load_manifest(&parent, None)?.0;
        }
        Ok(found)
    }

    /// Managed file name of a stored record revision (for exports).
    pub fn stored_file(&self, reference: &RecordRef) -> Result<String> {
        self.file_of(reference)?.ok_or_else(Self::corrupt_record)
    }

    /// Managed file name of an object listed by `manifest` (for exports).
    pub fn object_file_for(
        &self,
        manifest: &CommitManifest,
        entry: &ObjectEntry,
    ) -> Result<String> {
        self.object_file(manifest, entry)?
            .ok_or_else(Self::corrupt_record)
    }

    /// Bytes of an object reachable at the pinned commit, verified by hash.
    pub fn read_object(&self, pin: &CommitPin, hash: &Sha256Hex) -> Result<Vec<u8>> {
        let manifest = self.read_manifest(pin)?;
        let entry = manifest
            .objects
            .iter()
            .find(|o| &o.object_hash == hash)
            .ok_or_else(Self::not_found)?;
        let file = self
            .object_file(&manifest, entry)?
            .ok_or_else(Self::corrupt_record)?;
        let bytes = self.root.read(&file)?.ok_or_else(Self::corrupt_record)?;
        if &sha256(&bytes) != hash {
            return Err(Self::corrupt_record());
        }
        Ok(bytes)
    }

    fn record_at(&self, reference: &RecordRef, hash: &Sha256Hex) -> Result<AnyRecord> {
        let key = (
            reference.record_kind,
            reference.record_id.clone(),
            reference.revision.get(),
        );
        if let Some(record) = self.records.lock().expect("cache").get(&key) {
            return Ok(record.clone());
        }
        let bytes = self.read_verified(reference, hash)?;
        let value: Value = serde_json::from_slice(&bytes)
            .map_err(|_| VaultError::corrupt("record unparseable"))?;
        let record = parse_any(reference.record_kind, &value)
            .map_err(|_| VaultError::corrupt("record invalid"))?;
        self.records
            .lock()
            .expect("cache")
            .insert(key, record.clone());
        Ok(record)
    }

    /// Verify every record and object of a commit and report the damage
    /// range. Reports only; never repairs or deletes (V06).
    pub fn verify(&self, pin: &CommitPin) -> Result<IntegrityReport> {
        let manifest = self.read_manifest(pin)?;
        let mut report = IntegrityReport::default();
        for entry in &manifest.catalog {
            report.records_checked += 1;
            let reference = RecordRef::new(entry.record_kind, &entry.record_id, entry.revision);
            let file = self.file_of(&reference)?;
            match file.map(|f| self.root.read(&f)).transpose()?.flatten() {
                None => report.missing_records.push(reference),
                Some(bytes) if sha256(&bytes) != entry.content_hash => {
                    report.corrupt_records.push(reference)
                }
                Some(_) => {}
            }
        }
        for entry in &manifest.objects {
            report.objects_checked += 1;
            let file = self.object_file(&manifest, entry).unwrap_or_default();
            match file.map(|f| self.root.read(&f)).transpose()?.flatten() {
                None => report.missing_objects.push(entry.object_hash.clone()),
                Some(bytes) if sha256(&bytes) != entry.object_hash => {
                    report.corrupt_objects.push(entry.object_hash.clone())
                }
                Some(_) => {}
            }
        }
        Ok(report)
    }

    /// Re-check a pinned read against the newest head: references deleted
    /// since the pin are refused (non-disclosing `not_found`), and a moved
    /// policy epoch is reported so authorization is decided again (V08).
    pub fn check_fresh(&self, pin: &CommitPin, references: &[RecordRef]) -> Result<Freshness> {
        let head = self.load_head()?;
        let pinned = self.read_manifest(pin)?;
        if head.manifest.deletion_epoch > pin.deletion_epoch {
            let known: BTreeSet<&str> = pinned
                .catalog
                .iter()
                .filter(|e| e.record_kind == RecordKind::Tombstone)
                .map(|e| e.record_id.as_str())
                .collect();
            for entry in head.manifest.catalog.iter().filter(|e| {
                e.record_kind == RecordKind::Tombstone && !known.contains(e.record_id.as_str())
            }) {
                let reference = RecordRef::new(entry.record_kind, &entry.record_id, entry.revision);
                if let AnyRecord::Tombstone(tombstone) =
                    self.record_at(&reference, &entry.content_hash)?
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
            policy_changed: head.manifest.policy_epoch != pin.policy_epoch,
            head: Self::pin_of(&head.manifest),
        })
    }

    // --------------------------------------------------------------- writes

    fn prepare_records(
        &self,
        staged: &[StagedRecord],
        head: Option<&CommitManifest>,
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
            let current = head.and_then(|h| {
                h.catalog
                    .iter()
                    .find(|e| e.record_kind == record.record_kind && e.record_id == id)
            });
            let expected_revision = match current {
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

    fn fill_catalog(
        &self,
        manifest: &mut CommitManifest,
        head: Option<&CommitManifest>,
        prepared: &[Prepared],
        objects: &[StagedObject],
    ) {
        // Canonical order: record kind name, then record ID.
        let mut catalog: BTreeMap<(&'static str, String), CatalogEntry> = BTreeMap::new();
        if let Some(head) = head {
            for entry in &head.catalog {
                let mut entry = entry.clone();
                entry.changed = false;
                catalog.insert(
                    (kind_name(entry.record_kind), entry.record_id.clone()),
                    entry,
                );
            }
        }
        for p in prepared {
            catalog.insert(
                (kind_name(p.kind), p.id.clone()),
                CatalogEntry {
                    record_kind: p.kind,
                    record_id: p.id.clone(),
                    revision: p.revision,
                    content_hash: sha256(&p.bytes),
                    changed: true,
                },
            );
        }
        manifest.catalog = catalog.into_values().collect();
        let mut all: BTreeMap<(String, u8), ObjectEntry> = BTreeMap::new();
        for entry in head.map(|h| h.objects.as_slice()).unwrap_or_default() {
            all.insert(
                (entry.object_hash.to_string(), kind_order(entry.object_kind)),
                entry.clone(),
            );
        }
        for object in objects {
            all.insert(
                (object.hash.to_string(), kind_order(object.kind)),
                ObjectEntry {
                    object_hash: object.hash.clone(),
                    size_bytes: object.bytes.len() as u64,
                    object_kind: object.kind,
                },
            );
        }
        manifest.objects = all.into_values().collect();
        let mut refs: Vec<RecordRef> = prepared
            .iter()
            .map(|p| RecordRef::new(p.kind, &p.id, p.revision))
            .collect();
        refs.sort_by(|a, b| {
            (kind_name(a.record_kind), &a.record_id, a.revision).cmp(&(
                kind_name(b.record_kind),
                &b.record_id,
                b.revision,
            ))
        });
        manifest.receipt.records = refs;
        manifest.review_ids = prepared
            .iter()
            .filter(|p| p.kind == RecordKind::Review)
            .map(|p| ReviewId::parse(&p.id).expect("parsed review id"))
            .collect();
        manifest.tombstone_ids = prepared
            .iter()
            .filter(|p| p.kind == RecordKind::Tombstone)
            .map(|p| DeleteId::parse(&p.id).expect("parsed delete id"))
            .collect();
    }

    /// Every document ever cataloged on the chain ending at `head`.
    fn history(&self, head: &CommitManifest) -> Result<History> {
        let mut chain = Vec::new();
        let mut docs = BTreeMap::new();
        let mut next = Some(head.commit_id.clone());
        while let Some(id) = next {
            let (manifest, _) = self.load_manifest(&id, None)?;
            for entry in &manifest.catalog {
                docs.entry((
                    entry.record_kind,
                    entry.record_id.clone(),
                    entry.revision.get(),
                ))
                .or_insert_with(|| entry.content_hash.clone());
            }
            next = manifest.parent_commit_id.clone();
            chain.push(manifest);
        }
        Ok((chain, docs))
    }

    fn check_manifest_and_set(
        &self,
        manifest: &CommitManifest,
        head: Option<&CommitManifest>,
        prepared: &[Prepared],
    ) -> Result<()> {
        let violations = manifest.validate();
        if !violations.is_empty() {
            return Err(VaultError::invalid(
                violations.iter().map(|v| v.rule).collect(),
            ));
        }
        if !self.options.validate_record_set {
            return Ok(());
        }
        let mut set = RecordSet::default();
        if let Some(head) = head {
            let (chain, docs) = self.history(head)?;
            for ((kind, id, rev), hash) in &docs {
                let reference =
                    RecordRef::new(*kind, id, Revision::new(*rev).expect("stored revision"));
                push_record(&mut set, self.record_at(&reference, hash)?);
            }
            for manifest in chain {
                set.commits.push((*manifest).clone());
            }
        }
        for p in prepared {
            push_record(&mut set, p.record.clone());
        }
        set.commits.push(manifest.clone());
        let violations = validate_set(&set);
        if violations.is_empty() {
            Ok(())
        } else {
            let mut rules: Vec<&'static str> = violations.iter().map(|v| v.rule).collect();
            rules.sort();
            rules.dedup();
            Err(VaultError::invalid(rules))
        }
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
        manifest: &CommitManifest,
        prepared: &[Prepared],
        objects: &[StagedObject],
        idempotency: Option<(&IdempotencyScope, &Sha256Hex)>,
    ) -> Result<()> {
        let faults = &self.options.faults;
        let operation = &manifest.operation_id;
        // An intent marker makes an interrupted transaction visible to
        // health until the next writer clears it. Records and objects are
        // then written once, flushed, directly at their final names: until
        // CURRENT names a manifest that catalogs them, nothing reads them.
        let staging = layout::staging_dir(operation.as_str());
        self.root.write_new(
            &format!("{staging}/intent"),
            manifest.commit_id.as_str().as_bytes(),
        )?;
        let placements: Vec<(String, &[u8])> = prepared
            .iter()
            .map(|p| (p.file.clone(), p.bytes.as_slice()))
            .collect();
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
        for (file, bytes) in &placements {
            self.place(file, bytes, operation)?;
        }
        faults.io(FaultPoint::RecordsPlaced)?;
        for (file, bytes) in &object_placements {
            self.place(file, bytes, operation)?;
        }
        faults.io(FaultPoint::ObjectsPlaced)?;
        let bytes = canonical_bytes(manifest).expect("manifest serializes");
        self.root.write_new(
            &layout::commit_manifest(manifest.commit_id.as_str()),
            &bytes,
        )?;
        faults.io(FaultPoint::ManifestWritten)?;
        if let Some((scope, payload)) = idempotency {
            let entry = IdempotencyEntry {
                schema_version: SchemaVersion,
                vault_id: self.descriptor.vault_id.clone(),
                principal_id: scope.principal_id.clone(),
                operation_kind: scope.operation_kind,
                key_hash: scope.key_hash.clone(),
                request_payload_hash: payload.clone(),
                commit_id: manifest.commit_id.clone(),
                sequence: manifest.sequence,
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
    fn publish(&self, manifest: &CommitManifest) -> Result<()> {
        let faults = &self.options.faults;
        let (_, hash) = self.load_manifest(&manifest.commit_id, None)?;
        let pointer = CurrentPointer {
            schema_version: SchemaVersion,
            vault_id: self.descriptor.vault_id.clone(),
            commit_id: manifest.commit_id.clone(),
            sequence: manifest.sequence,
            manifest_sha256: hash.clone(),
        };
        faults.io(FaultPoint::BeforeCurrent)?;
        self.root.write_atomic(
            &layout::current_pointer(),
            &canonical_bytes(&pointer).expect("pointer"),
        )?;
        faults.io(FaultPoint::AfterCurrent)?;
        self.append_journal(manifest, &hash);
        faults.io(FaultPoint::AfterJournal)?;
        let _ = self
            .root
            .remove_staging(&layout::staging_dir(manifest.operation_id.as_str()));
        Ok(())
    }

    /// The journal is recovery evidence, not the commit: a failed append is
    /// not reported as a failed commit (that would invite a duplicate retry).
    fn append_journal(&self, manifest: &CommitManifest, hash: &Sha256Hex) {
        let Ok(published_at) = commit_time(self.clock.as_ref(), None, ComponentId::Vault) else {
            return;
        };
        let record = PublishRecord {
            schema_version: SchemaVersion,
            vault_id: self.descriptor.vault_id.clone(),
            commit_id: manifest.commit_id.clone(),
            sequence: manifest.sequence,
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
    fn on_chain(&self, head: &CommitManifest, commit_id: &CommitId, sequence: u64) -> Result<bool> {
        if sequence > head.sequence {
            return Ok(false);
        }
        let mut current = Arc::new(head.clone());
        while current.sequence > sequence {
            let Some(parent) = current.parent_commit_id.clone() else {
                return Ok(false);
            };
            current = self.load_manifest(&parent, None)?.0;
        }
        Ok(&current.commit_id == commit_id)
    }

    fn published_receipt(
        &self,
        head: &CommitManifest,
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
        let (manifest, _) = self.load_manifest(&entry.commit_id, None)?;
        if manifest.idempotency_key_hash.as_ref() != Some(&scope.key_hash)
            || manifest.principal.actor_id != scope.principal_id
            || manifest.operation_kind != scope.operation_kind
        {
            return Err(VaultError::corrupt("idempotency entry"));
        }
        Ok(Some((
            manifest.commit_id.clone(),
            manifest.request_payload_hash.clone(),
            manifest.receipt.clone(),
        )))
    }

    pub fn find_receipt(
        &self,
        scope: &IdempotencyScope,
    ) -> Result<Option<(CommitId, Sha256Hex, OperationReceipt)>> {
        let head = self.load_head()?;
        self.published_receipt(&head.manifest, scope)
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
        match self.published_receipt(&head.manifest, &request.idempotency)? {
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
            .is_some_and(|expected| expected != &head.manifest.commit_id)
        {
            return Err(VaultError::new(
                MemoryErrorCode::RevisionConflict,
                Fault::HeadMoved,
            ));
        }
        for (kind, id, expected) in &request.expected_revisions {
            let actual = head
                .manifest
                .catalog
                .iter()
                .find(|e| e.record_kind == *kind && &e.record_id == id)
                .map(|e| e.revision);
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
            // Published under this ID: a reuse. Otherwise it is the manifest
            // of an attempt that crashed before publication: quarantine it.
            let published = match self.load_manifest(&request.commit_id, None) {
                Ok((existing, _)) => {
                    self.on_chain(&head.manifest, &existing.commit_id, existing.sequence)?
                }
                Err(_) => false,
            };
            if published {
                return Err(VaultError::invalid(vec!["store.commit_id_reused"]));
            }
            self.manifests
                .lock()
                .expect("cache")
                .remove(request.commit_id.as_str());
            let operation = OperationId::from_random(self.ids.random_16());
            self.quarantine(&manifest_file, &operation)?;
        }
        let prepared = self.prepare_records(&request.records, Some(&head.manifest))?;
        self.check_objects(&request.objects, &prepared)?;
        let tombstone_epochs: BTreeSet<u64> = prepared
            .iter()
            .filter_map(|p| match &p.record {
                AnyRecord::Tombstone(t) => Some(t.deletion_epoch),
                _ => None,
            })
            .collect();
        let deletion_epoch = match tombstone_epochs.len() {
            0 => head.manifest.deletion_epoch,
            1 if tombstone_epochs.contains(&(head.manifest.deletion_epoch + 1)) => {
                head.manifest.deletion_epoch + 1
            }
            _ => return Err(VaultError::invalid(vec!["store.deletion_epoch"])),
        };
        let policy_epoch = head.manifest.policy_epoch
            + u64::from(prepared.iter().any(|p| p.kind == RecordKind::Policy));
        let created_at = commit_time(
            self.clock.as_ref(),
            Some(&head.manifest.created_at),
            ComponentId::Vault,
        )
        .map_err(|_| VaultError::new(MemoryErrorCode::ClockRegression, Fault::ClockRegression))?;
        let operation_id = OperationId::from_random(self.ids.random_16());
        let mut manifest = CommitManifest {
            schema_version: SchemaVersion,
            commit_id: request.commit_id.clone(),
            format_version: FormatVersion,
            parent_commit_id: Some(head.manifest.commit_id.clone()),
            sequence: head.manifest.sequence + 1,
            vault_id: self.descriptor.vault_id.clone(),
            writer_device_id: self.device_id.clone(),
            principal: request.principal.clone(),
            operation_id: operation_id.clone(),
            operation_kind: request.operation_kind,
            idempotency_key_hash: Some(request.idempotency.key_hash.clone()),
            request_payload_hash: request.request_payload_hash.clone(),
            created_at,
            catalog: Vec::new(),
            objects: Vec::new(),
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
        self.fill_catalog(
            &mut manifest,
            Some(&head.manifest),
            &prepared,
            &request.objects,
        );
        // An import past `archiving` is only committed together with (or
        // after) the exact bytes it received (IMPORT_REVIEW §2, I01).
        for p in &prepared {
            if let AnyRecord::Import(import) = &p.record
                && !matches!(
                    import.status,
                    enouia_memory_contract::import::ImportStatus::Planned
                        | enouia_memory_contract::import::ImportStatus::Archiving
                )
                && !manifest.objects.iter().any(|o| {
                    o.object_hash == import.input_object_hash && o.object_kind == ObjectKind::Raw
                })
            {
                return Err(VaultError::invalid(vec!["store.import_raw_missing"]));
            }
        }
        self.check_manifest_and_set(&manifest, Some(&head.manifest), &prepared)?;
        self.write_transaction(
            &manifest,
            &prepared,
            &request.objects,
            Some((&request.idempotency, &request.request_payload_hash)),
        )?;
        self.publish(&manifest)?;
        Ok(CommitOutcome::Committed {
            commit_id: manifest.commit_id.clone(),
            receipt: manifest.receipt.clone(),
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
        let Ok((manifest, _)) = self.load_manifest(commit_id, Some(hash)) else {
            return false;
        };
        let pin = Self::pin_of(&manifest);
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
            let Ok((manifest, hash)) = self.load_manifest(&id, None) else {
                continue;
            };
            if self.complete(&id, &hash) {
                candidates.push(RecoveryCandidate {
                    commit_id: id,
                    sequence: manifest.sequence,
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
        let (manifest, _) = self.load_manifest(commit_id, Some(&candidate.manifest_sha256))?;
        self.publish(&manifest)?;
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

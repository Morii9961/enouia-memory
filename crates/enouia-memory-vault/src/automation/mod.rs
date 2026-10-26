//! Core-3 explicit V2 synthetic authority. No legacy-format adoption or default
//! writer activation. One Host lease, short Writer transactions, exact witness.
mod attention;
mod read_model;
pub use read_model::AuthorizedReadData;
mod publication;
pub use attention::{ATTENTION_PROJECTION, AuthorizedRecallSet};
pub use publication::{AUTOMATION_INTENT, AUTOMATION_WITNESS};

use crate::{
    RootPolicy, VerifiedRoot,
    fault::{FaultPoint, Faults},
    fs::ManagedRoot,
    lock, platform, verify_data_root,
};
use enouia_memory_contract::{
    ContractError,
    common::{ActorRef, ActorType, Sensitivity},
    foundation::Clock,
    hash::Sha256Hex,
    host::PrimaryBinding,
    ids::*,
    json::{Revision, canonical_bytes},
    layout,
    next::*,
    ports::IdSource,
    time::Timestamp,
};
use std::{
    collections::BTreeMap,
    fs::File,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Debug)]
pub enum AutomationFault {
    Contract(ContractError),
    Denied,
    Fenced,
    Stale,
    Conflict,
    Busy,
    Capacity,
    Unavailable,
    RecoveryRequired,
    OutcomeUnknown,
    StorageFailed,
}
pub type Result<T> = std::result::Result<T, AutomationFault>;
impl From<ContractError> for AutomationFault {
    fn from(e: ContractError) -> Self {
        Self::Contract(e)
    }
}
impl From<crate::VaultError> for AutomationFault {
    fn from(e: crate::VaultError) -> Self {
        if matches!(e.fault, crate::Fault::WriterBusy) {
            Self::Busy
        } else {
            Self::StorageFailed
        }
    }
}

pub struct AutomationVault {
    root: ManagedRoot,
    descriptor: AutomationDescriptor,
    binding: PrimaryBinding,
    _lease: File,
    clock: Arc<dyn Clock + Send + Sync>,
    ids: Arc<dyn IdSource + Send + Sync>,
    poisoned: AtomicBool,
}
impl AutomationVault {
    fn acquire(
        root: &VerifiedRoot,
        descriptor: AutomationDescriptor,
        host: DeviceId,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        let verified = verify_data_root(root.path(), &RootPolicy::default())
            .map_err(|_| AutomationFault::Denied)?;
        if !cfg!(windows)
            || !verified.filesystem.eq_ignore_ascii_case("NTFS")
            || !platform::inspect_acl(verified.path())
                .map_err(|_| AutomationFault::Denied)?
                .is_owner_only()
            || !descriptor.validate().is_empty()
        {
            return Err(AutomationFault::Denied);
        }
        if host != descriptor.host_id {
            return Err(AutomationFault::Fenced);
        }
        let root = ManagedRoot::new(verified.path(), faults);
        let lease = root.open_lock_file(&layout::host_lock_file())?;
        lease.try_lock().map_err(|_| AutomationFault::Busy)?;
        Ok(Self {
            root,
            descriptor,
            binding: PrimaryBinding {
                host_id: host,
                primary_epoch: Revision::new(1).unwrap(),
            },
            _lease: lease,
            clock,
            ids,
            poisoned: AtomicBool::new(false),
        })
    }
    pub fn create(
        root: &VerifiedRoot,
        descriptor: AutomationDescriptor,
        policy: AutomationPolicy,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        if std::fs::read_dir(root.path())
            .map_err(|_| AutomationFault::Unavailable)?
            .next()
            .is_some()
        {
            return Err(AutomationFault::Conflict);
        }
        let data = AutomationData {
            policy,
            record_catalog: (descriptor.layout == AutomationLayout::HostV3)
                .then(RecordCatalog::default),
            owner_actions: (descriptor.layout == AutomationLayout::HostV3)
                .then(OwnerActionLedger::default),
            policy_watermark: Revision::new(1).unwrap(),
            deletion_watermark: Revision::new(1).unwrap(),
            sources: Vec::new(),
            memories: Vec::new(),
            receipts: Vec::new(),
            deleted_sources: Vec::new(),
            deleted_memories: Vec::new(),
        };
        let now =
            Timestamp::from_unix_ms(clock.now_unix_ms()).ok_or(AutomationFault::Unavailable)?;
        let commit = AutomationCommit::assemble(
            &descriptor,
            None,
            descriptor.created_by.clone(),
            AutomationChange::Genesis,
            data,
            CommitId::from_random(ids.random_16()),
            now,
        )?;
        platform::restrict_to_owner(root.path()).map_err(|_| AutomationFault::Denied)?;
        let vault = Self::acquire(
            root,
            descriptor.clone(),
            descriptor.host_id.clone(),
            clock,
            ids,
            faults,
        )?;
        let _writer = lock::acquire(&vault.root, Duration::ZERO)?;
        vault
            .root
            .write_new(&layout::vault_descriptor(), &canonical_bytes(&descriptor)?)?;
        vault.publish(None, &commit, 0)?;
        drop(_writer);
        Ok(vault)
    }
    /// No repair, adoption or old-version fallback.
    pub fn open(
        root: &VerifiedRoot,
        host: DeviceId,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        let managed = ManagedRoot::new(root.path(), faults.clone());
        // Version refusal before creating a lease file or decoding any records.
        let bytes = managed
            .read_bounded(&layout::vault_descriptor(), 16 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        let descriptor = parse_document::<AutomationDescriptor>(&bytes)?;
        let vault = Self::acquire(root, descriptor, host, clock, ids, faults)?;
        vault.inspect()?;
        Ok(vault)
    }
    pub fn descriptor(&self) -> &AutomationDescriptor {
        &self.descriptor
    }
    pub fn current_time(&self) -> Result<Timestamp> {
        self.now()
    }
    fn now(&self) -> Result<Timestamp> {
        Timestamp::from_unix_ms(self.clock.now_unix_ms()).ok_or(AutomationFault::Unavailable)
    }
    fn system(&self, actor: &ActorRef) -> Result<()> {
        if actor == &self.descriptor.system_actor && actor.actor_type == ActorType::System {
            Ok(())
        } else {
            Err(AutomationFault::Denied)
        }
    }
    fn owner(&self, actor: &ActorRef) -> Result<()> {
        if actor == &self.descriptor.created_by && actor.actor_type == ActorType::Owner {
            Ok(())
        } else {
            Err(AutomationFault::Denied)
        }
    }
    fn available(&self) -> Result<()> {
        if self.poisoned.load(Ordering::SeqCst) {
            Err(AutomationFault::RecoveryRequired)
        } else {
            Ok(())
        }
    }
    pub fn inspect(&self) -> Result<AutomationCommit> {
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        self.current_locked().map(|(commit, _)| commit)
    }
    /// Native selected-source request. Ambiguous seed only; actual generation
    /// runs later without this writer lock and is reevaluated at application.
    pub fn prepare(&self, actor: &ActorRef, mut input: AdmissionInput) -> Result<AdmissionInput> {
        self.system(actor)?;
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (before, _) = self.current_locked()?;
        let policy = &before.data.policy;
        let mut selected = Vec::new();
        let mut sensitivity = Sensitivity::Public;
        for reference in &input.input_refs {
            let source = before
                .data
                .sources
                .iter()
                .rev()
                .find(|s| s.record.source_id == reference.source_id)
                .ok_or(AutomationFault::Stale)?;
            if source.record.revision != reference.source_revision {
                return Err(AutomationFault::Stale);
            }
            sensitivity = sensitivity.max(source.record.sensitivity);
            selected.push(source.clone());
        }
        let first = selected.first().ok_or(AutomationFault::Conflict)?;
        input.snapshot = before.snapshot();
        input.actor = actor.clone();
        input.policy = policy.reference();
        input.subject_id = policy.subject_id.clone();
        input.project_id = policy.project_id.clone();
        input.access_policy_id = policy.access_policy_id.clone();
        input.egress_policy_id = policy.egress_policy_id.clone();
        input.operation_id = OperationId::from_random(self.ids.random_16());
        if let Some(target) = &input.target {
            let current = before
                .data
                .memories
                .iter()
                .rev()
                .find(|m| m.memory.memory_id == target.memory.memory_id)
                .ok_or(AutomationFault::Stale)?;
            if current.target() != *target {
                return Err(AutomationFault::Stale);
            }
            sensitivity = sensitivity.max(current.decision.input.sensitivity);
            input.result = MemoryRevisionRef {
                memory_id: target.memory.memory_id.clone(),
                revision: Revision::new(target.memory.revision.get() + 1)
                    .ok_or(AutomationFault::Capacity)?,
            };
        } else {
            input.result = MemoryRevisionRef {
                memory_id: MemoryId::from_random(self.ids.random_16()),
                revision: Revision::new(1).unwrap(),
            };
        }
        input.sensitivity = sensitivity;
        input.evidence = vec![source_evidence(first)];
        input.context_source_refs = input.input_refs[1..].to_vec();
        input.content =
            serde_json::to_string(&selected).map_err(|_| AutomationFault::Unavailable)?;
        input.interpretation = Interpretation::Ambiguous;
        if let Some(fields) = &mut input.record_fields
            && let Some(annotations) = &mut fields.annotations
        {
            annotations.memory = input.result.clone();
        }
        if let Some(revert) = input
            .dimensions
            .as_ref()
            .and_then(|d| d.reverts_to.as_ref())
        {
            let historical = before
                .data
                .memories
                .iter()
                .find(|m| m.target() == *revert)
                .ok_or(AutomationFault::Stale)?;
            input.content = historical.decision.input.content.clone();
            input.interpretation = historical.decision.input.interpretation;
        }
        with_stored_context(
            &before.data,
            &before.snapshot(),
            actor,
            &self.now()?,
            |context| evaluate(&input, context),
        )?;
        Ok(input)
    }
    /// Expand exact current parents to original sources. No model call or parent
    /// deletion; the eventual result remains a separately attributed hypothesis.
    pub fn prepare_synthesis(
        &self,
        actor: &ActorRef,
        mut input: AdmissionInput,
        parents: Vec<TargetRef>,
    ) -> Result<AdmissionInput> {
        self.system(actor)?;
        self.available()?;
        {
            let _writer = lock::acquire(&self.root, Duration::ZERO)?;
            let (before, _) = self.current_locked()?;
            if self.descriptor.layout != AutomationLayout::HostV2
                || parents.is_empty()
                || parents.len() > 8
            {
                return Err(AutomationFault::Denied);
            }
            let mut refs = std::collections::BTreeSet::new();
            for parent in &parents {
                let memory = before
                    .data
                    .memories
                    .iter()
                    .rev()
                    .find(|m| m.memory.memory_id == parent.memory.memory_id)
                    .ok_or(AutomationFault::Stale)?;
                if memory.target() != *parent {
                    return Err(AutomationFault::Stale);
                }
                self.eligible(&before, memory, before.data.policy.max_sensitivity, true)?;
                refs.extend(memory.decision.input.input_refs.iter().cloned());
            }
            input.input_refs = refs.into_iter().collect();
            input.target = None;
            input.dimensions = Some(FormationDimensions {
                kind: FormationKind::Theme,
                occurrence: OccurrenceAssertion::Unknown,
                state: None,
                change: FormationChange::Synthesis,
                conditions: vec![],
                event_time: None,
                valid_time: None,
                counter_sources: vec![],
                derived_from: parents,
                reverts_to: None,
            });
        }
        self.prepare(actor, input)
    }
    fn commit(
        &self,
        before: &AutomationCommit,
        data: AutomationData,
        actor: ActorRef,
        change: AutomationChange,
        chain_bytes: usize,
    ) -> Result<AutomationCommit> {
        let after = AutomationCommit::assemble(
            &self.descriptor,
            Some(before),
            actor,
            change,
            data,
            CommitId::from_random(self.ids.random_16()),
            self.now()?,
        )?;
        self.publish(Some(before.current()), &after, chain_bytes)?;
        Ok(after)
    }
    pub fn capture(&self, actor: &ActorRef, source: CapturedSource) -> Result<AutomationCurrent> {
        self.capture_with_records(actor, source, None)
    }
    /// Atomically append source and revisioned entity/conversation catalog.
    pub fn capture_with_records(
        &self,
        actor: &ActorRef,
        source: CapturedSource,
        catalog: Option<RecordCatalog>,
    ) -> Result<AutomationCurrent> {
        self.system(actor)?;
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (before, bytes) = self.current_locked()?;
        if let Some(old) = before
            .data
            .sources
            .iter()
            .find(|s| s.reference() == source.reference())
        {
            return if old == &source
                && catalog
                    .as_ref()
                    .is_none_or(|c| before.data.record_catalog.as_ref() == Some(c))
            {
                Ok(before.current())
            } else {
                Err(AutomationFault::Conflict)
            };
        }
        let mut data = before.data.clone();
        data.sources.push(source);
        if let Some(catalog) = catalog {
            data.record_catalog = Some(catalog);
        }
        Ok(self
            .commit(
                &before,
                data,
                actor.clone(),
                AutomationChange::Capture,
                bytes,
            )?
            .current())
    }
    pub fn set_policy(
        &self,
        owner: &ActorRef,
        policy: AutomationPolicy,
    ) -> Result<AutomationCurrent> {
        self.owner(owner)?;
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (before, bytes) = self.current_locked()?;
        if before.data.policy == policy {
            return Ok(before.current());
        }
        let mut data = before.data.clone();
        data.policy = policy;
        data.policy_watermark =
            Revision::new(data.policy_watermark.get() + 1).ok_or(AutomationFault::Capacity)?;
        Ok(self
            .commit(
                &before,
                data,
                owner.clone(),
                AutomationChange::Policy,
                bytes,
            )?
            .current())
    }
    pub fn delete_source(&self, owner: &ActorRef, id: SourceId) -> Result<AutomationCurrent> {
        self.owner(owner)?;
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (before, bytes) = self.current_locked()?;
        if before.data.deleted_sources.contains(&id) {
            return Ok(before.current());
        }
        let mut data = before.data.clone();
        data.deleted_sources.push(id);
        data.deletion_watermark =
            Revision::new(data.deletion_watermark.get() + 1).ok_or(AutomationFault::Capacity)?;
        Ok(self
            .commit(
                &before,
                data,
                owner.clone(),
                AutomationChange::DeleteSource,
                bytes,
            )?
            .current())
    }
    pub fn delete_memory(&self, owner: &ActorRef, id: MemoryId) -> Result<AutomationCurrent> {
        self.owner(owner)?;
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (before, bytes) = self.current_locked()?;
        if before.data.deleted_memories.contains(&id) {
            return Ok(before.current());
        }
        let mut data = before.data.clone();
        data.deleted_memories.push(id);
        data.deletion_watermark =
            Revision::new(data.deletion_watermark.get() + 1).ok_or(AutomationFault::Capacity)?;
        Ok(self
            .commit(
                &before,
                data,
                owner.clone(),
                AutomationChange::DeleteMemory,
                bytes,
            )?
            .current())
    }
    fn receipts(
        &self,
        before: &AutomationCommit,
        actor: &ActorRef,
        key: &Sha256Hex,
        bindings: &[GenerationBinding],
    ) -> Result<Option<Vec<MemoryReceipt>>> {
        self.system(actor)?;
        let policy = &before.data.policy;
        let now = self.now()?;
        if !policy.active || now < policy.valid_from || now >= policy.valid_until {
            return Err(AutomationFault::Denied);
        }
        let receipts: Vec<_> = before
            .data
            .receipts
            .iter()
            .filter(|r| &r.job_key == key)
            .cloned()
            .collect();
        if receipts.is_empty() {
            return Ok(None);
        }
        if receipts.len() != bindings.len()
            || receipts.iter().any(|r| r.binding_hash != digest(&bindings))
        {
            return Err(AutomationFault::Conflict);
        }
        for receipt in &receipts {
            let memory = before
                .data
                .memories
                .iter()
                .find(|m| m.memory == receipt.result)
                .ok_or(AutomationFault::Unavailable)?;
            self.eligible(before, memory, policy.max_sensitivity, true)?;
        }
        Ok(Some(receipts))
    }
    pub fn replay_job(
        &self,
        actor: &ActorRef,
        key: &Sha256Hex,
        bindings: &[GenerationBinding],
    ) -> Result<Option<Vec<MemoryReceipt>>> {
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (before, _) = self.current_locked()?;
        self.receipts(&before, actor, key, bindings)
    }
    pub fn apply_batch(
        &self,
        actor: &ActorRef,
        job_key: Sha256Hex,
        requests: Vec<AdmissionInput>,
        decisions: Vec<DecisionDraft>,
        bindings: Vec<GenerationBinding>,
        responses: Vec<String>,
    ) -> Result<Vec<MemoryReceipt>> {
        self.system(actor)?;
        self.available()?;
        if requests.is_empty()
            || requests.len() > MAX_AUTOMATION_BATCH
            || requests.len() != decisions.len()
            || requests.len() != bindings.len()
            || requests.len() != responses.len()
            || admission_batch_key(&requests) != job_key
        {
            return Err(AutomationFault::Conflict);
        }
        for (((request, decision), binding), response) in requests
            .iter()
            .zip(&decisions)
            .zip(&bindings)
            .zip(&responses)
        {
            if binding.input_hash != digest(request)
                || binding.operation_id != request.operation_id
                || binding.response_hash
                    != enouia_memory_contract::hash::sha256(response.as_bytes())
                || parse_document::<DecisionDraft>(response.as_bytes())? != *decision
            {
                return Err(AutomationFault::Conflict);
            }
            check_generation_binding(request, &decision.input)?;
        }
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (before, bytes) = self.current_locked()?;
        if let Some(receipts) = self.receipts(&before, actor, &job_key, &bindings)? {
            return Ok(receipts);
        }
        if requests.iter().any(|r| r.snapshot != before.snapshot()) {
            return Err(AutomationFault::Stale);
        }
        let id = CommitId::from_random(self.ids.random_16());
        let sequence = Revision::new(before.sequence.get() + 1).ok_or(AutomationFault::Capacity)?;
        let binding_hash = digest(&bindings);
        let mut data = before.data.clone();
        for (((request, decision), binding), response_json) in requests
            .into_iter()
            .zip(decisions)
            .zip(bindings)
            .zip(responses)
        {
            let memory = MachineMemory {
                schema_version: NextVersion,
                kind: MachineKind::MachineMemory,
                memory: decision.input.result.clone(),
                request,
                decision,
                response_hash: binding.response_hash.clone(),
                response_json,
                committed_in: id.clone(),
            };
            let receipt = MemoryReceipt {
                job_key: job_key.clone(),
                binding_hash: binding_hash.clone(),
                operation_id: binding.operation_id,
                request_hash: binding.input_hash,
                response_hash: binding.response_hash,
                decision_hash: digest(&memory.decision),
                committed_in: id.clone(),
                sequence,
                result: memory.memory.clone(),
            };
            data.memories.push(memory);
            data.receipts.push(receipt);
        }
        let after = AutomationCommit::assemble(
            &self.descriptor,
            Some(&before),
            actor.clone(),
            AutomationChange::AdmitBatch,
            data,
            id,
            self.now()?,
        )?;
        self.publish(Some(before.current()), &after, bytes)?;
        Ok(after.data.receipts[before.data.receipts.len()..].to_vec())
    }
    fn eligible(
        &self,
        commit: &AutomationCommit,
        memory: &MachineMemory,
        max: Sensitivity,
        receipt: bool,
    ) -> Result<()> {
        let p = &commit.data.policy;
        let input = &memory.decision.input;
        if !p.active
            || self.now()? < p.valid_from
            || self.now()? >= p.valid_until
            || input.policy != p.reference()
            || input.actor != self.descriptor.system_actor
            || input.subject_id != p.subject_id
            || input.project_id != p.project_id
            || input.access_policy_id != p.access_policy_id
            || input.egress_policy_id != p.egress_policy_id
            || input.sensitivity > max.min(p.max_sensitivity)
            || commit
                .data
                .deleted_memories
                .contains(&memory.memory.memory_id)
            || (!receipt && memory.decision.outcome.adoption != AdoptionState::Adopted)
        {
            return Err(AutomationFault::Denied);
        }
        for reference in &input.input_refs {
            let source = commit
                .data
                .sources
                .iter()
                .rev()
                .find(|s| s.record.source_id == reference.source_id)
                .ok_or(AutomationFault::Denied)?;
            if !p.source_ids.contains(&reference.source_id)
                || commit.data.deleted_sources.contains(&reference.source_id)
                || source.record.revision != reference.source_revision
                || source.record.access_policy_id != p.access_policy_id
                || source.record.sensitivity > max.min(p.max_sensitivity)
                || source.record.sensitivity > input.sensitivity
            {
                return Err(AutomationFault::Denied);
            }
        }
        if let Some(dimensions) = &input.dimensions {
            if !receipt
                && dimensions.valid_time.as_ref().is_some_and(|t| {
                    self.now().map_or(true, |now| {
                        now < t.from || t.until.as_ref().is_some_and(|end| &now >= end)
                    })
                })
            {
                return Err(AutomationFault::Denied);
            }
            for parent in &dimensions.derived_from {
                let current = commit
                    .data
                    .memories
                    .iter()
                    .rev()
                    .find(|m| m.memory.memory_id == parent.memory.memory_id)
                    .ok_or(AutomationFault::Denied)?;
                if current.target() != *parent {
                    return Err(AutomationFault::Denied);
                }
                self.eligible(commit, current, max, true)?;
            }
        }
        Ok(())
    }
    pub fn recall(
        &self,
        actor: &ActorRef,
        query: &str,
        max: Sensitivity,
        limit: usize,
    ) -> Result<Vec<MachineMemory>> {
        self.system(actor)?;
        self.available()?;
        if query.len() > 2048 || !(1..=32).contains(&limit) {
            return Err(AutomationFault::Capacity);
        }
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (commit, _) = self.current_locked()?;
        let now = self.now()?;
        if !commit.data.policy.active
            || now < commit.data.policy.valid_from
            || now >= commit.data.policy.valid_until
        {
            return Err(AutomationFault::Denied);
        }
        let mut latest = BTreeMap::new();
        for memory in &commit.data.memories {
            latest.insert(&memory.memory.memory_id, memory);
        }
        let terms: Vec<_> = query.split_whitespace().map(str::to_lowercase).collect();
        Ok(latest
            .values()
            .filter(|memory| self.eligible(&commit, memory, max, false).is_ok())
            .filter(|memory| {
                terms.is_empty()
                    || terms
                        .iter()
                        .any(|term| memory.decision.input.content.to_lowercase().contains(term))
            })
            .take(limit)
            .map(|memory| (*memory).clone())
            .collect())
    }
}

fn source_evidence(source: &CapturedSource) -> enouia_memory_contract::common::EvidenceRef {
    enouia_memory_contract::common::EvidenceRef {
        source_id: source.record.source_id.clone(),
        source_revision: source.record.revision,
        locator: source.record.locator.clone(),
        object_hash: source.record.anchor_hash().clone(),
        evidence_class: source.record.evidence_class,
        supports: "content".into(),
    }
}

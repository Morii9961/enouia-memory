//! Explicit ADR-MEM-94 synthetic root. No implicit opening, migration or routes.
mod agent;
mod catalog;
pub use agent::{AuthorizedSegmentedAgentRead, SegmentedAgentPermit, SegmentedAgentScope};
mod runtime_capture;
mod state;
use super::*;
use enouia_memory_contract::{
    common::{ActorRef, Sensitivity},
    hash::sha256,
    host::{AuthorityState, Checkpoint},
};
pub use runtime_capture::{NativeRuntimeCapture, RuntimeCapturePermit, RuntimeCaptureScope};
pub use state::SegmentedMemory;
use state::State;
use std::{collections::BTreeMap, sync::Mutex};

pub struct AuthorizedSegmentedRead<'a> {
    pub withdrawal_watermark: Revision,
    pub snapshot: SnapshotBinding,
    pub now: Timestamp,
    pub memories: Vec<&'a SegmentedMemory>,
    pub sources: BTreeMap<SourceId, &'a CapturedSource>,
    pub entities: BTreeMap<String, &'a EntityRecord>,
}
pub struct SegmentedAutomationVault {
    native: AutomationVault,
    state: Mutex<State>,
}
fn next(v: Revision) -> Result<Revision> {
    Revision::new(v.get() + 1).ok_or(AutomationFault::Capacity)
}
impl SegmentedAutomationVault {
    pub fn descriptor(&self) -> &AutomationDescriptor {
        &self.native.descriptor
    }
    fn path(hash: &Sha256Hex) -> String {
        format!("vault/automation-segments/{hash}.json")
    }
    fn pointer(&self, path: &str) -> Result<Option<AutomationCurrent>> {
        self.native
            .root
            .read_bounded(path, 16 * 1024)?
            .map(|b| parse_document(&b).map_err(AutomationFault::from))
            .transpose()
    }
    fn intent(&self) -> Result<AutomationPublication> {
        let b = self
            .native
            .root
            .read_bounded(AUTOMATION_INTENT, 32 * 1024)?
            .ok_or(AutomationFault::RecoveryRequired)?;
        Ok(parse_document(&b)?)
    }
    fn current(&self, state: &State) -> Result<()> {
        self.native.available()?;
        let b = self
            .native
            .root
            .read_bounded(&layout::vault_descriptor(), 16 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        if parse_document::<AutomationDescriptor>(&b)? != self.native.descriptor {
            return Err(AutomationFault::Fenced);
        }
        let head = state.head.as_ref().ok_or(AutomationFault::Unavailable)?;
        let pointer = head.current();
        let intent = self.intent()?;
        if self.pointer(&layout::current_pointer())? != Some(pointer.clone())
            || self.pointer(AUTOMATION_WITNESS)? != Some(pointer.clone())
            || intent.after != pointer
            || intent.before != head.parent
        {
            return Err(AutomationFault::RecoveryRequired);
        }
        head.authority
            .check_writer(&self.native.descriptor.vault_id, &self.native.binding)
            .map_err(|_| AutomationFault::Fenced)?;
        Ok(())
    }
    pub fn create(
        root: &VerifiedRoot,
        descriptor: AutomationDescriptor,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        if descriptor.layout != AutomationLayout::SegmentedV1
            || !descriptor.validate().is_empty()
            || std::fs::read_dir(root.path())
                .map_err(|_| AutomationFault::Unavailable)?
                .next()
                .is_some()
        {
            return Err(AutomationFault::Denied);
        }
        platform::restrict_to_owner(root.path()).map_err(|_| AutomationFault::Denied)?;
        let native = AutomationVault::acquire(
            root,
            descriptor.clone(),
            descriptor.host_id.clone(),
            clock,
            ids,
            faults,
        )?;
        let vault = Self {
            native,
            state: Mutex::new(State::default()),
        };
        let _writer = lock::acquire(&vault.native.root, Duration::ZERO)?;
        vault
            .native
            .root
            .write_new(&layout::vault_descriptor(), &canonical_bytes(&descriptor)?)?;
        let mut state = vault
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        vault.commit(
            &mut state,
            descriptor.created_by,
            SegmentedDelta::Genesis {
                owner_actions: OwnerActionLedger::default(),
            },
        )?;
        drop(state);
        drop(_writer);
        Ok(vault)
    }
    fn load(
        root: &VerifiedRoot,
        host: DeviceId,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
        recover: bool,
    ) -> Result<Self> {
        let managed = ManagedRoot::new(root.path(), faults.clone());
        let b = managed
            .read_bounded(&layout::vault_descriptor(), 16 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        let descriptor: AutomationDescriptor = parse_document(&b)?;
        if descriptor.layout != AutomationLayout::SegmentedV1 {
            return Err(AutomationFault::Denied);
        }
        let native = AutomationVault::acquire(root, descriptor, host, clock, ids, faults)?;
        let vault = Self {
            native,
            state: Mutex::new(State::default()),
        };
        let _writer = lock::acquire(&vault.native.root, Duration::ZERO)?;
        let intent = vault.intent()?;
        let current = vault.pointer(&layout::current_pointer())?;
        let witness = vault.pointer(AUTOMATION_WITNESS)?;
        let complete =
            current == Some(intent.after.clone()) && witness == Some(intent.after.clone());
        if !complete
            && (!recover
                || !(current == intent.before || current == Some(intent.after.clone()))
                || witness != intent.before)
        {
            return Err(AutomationFault::RecoveryRequired);
        }
        let mut pointer = Some(intent.after.clone());
        let mut chain = Vec::new();
        let mut bytes = 0usize;
        while let Some(head) = pointer {
            if chain.len() >= MAX_SEGMENTED_COMMITS
                || head.vault_id != vault.native.descriptor.vault_id
            {
                return Err(AutomationFault::Capacity);
            }
            let b = vault
                .native
                .root
                .read_bounded(&Self::path(&head.manifest_sha256), MAX_SEGMENT_BYTES)?
                .ok_or(AutomationFault::Unavailable)?;
            bytes = bytes
                .checked_add(b.len())
                .ok_or(AutomationFault::Capacity)?;
            if bytes > MAX_SEGMENTED_CHAIN_BYTES || sha256(&b) != head.manifest_sha256 {
                return Err(AutomationFault::Unavailable);
            }
            let c: SegmentedCommit = parse_document(&b)?;
            if canonical_bytes(&c)? != b || c.current() != head {
                return Err(AutomationFault::Unavailable);
            }
            pointer = c.parent.clone();
            chain.push((c, b.len()));
        }
        let mut state = State::default();
        for (c, size) in chain.into_iter().rev() {
            let admitted = state.check(&vault.native.descriptor, &c)?;
            state.apply(c, admitted, size);
        }
        if state
            .head
            .as_ref()
            .is_none_or(|h| h.parent != intent.before)
        {
            return Err(AutomationFault::RecoveryRequired);
        }
        if !complete {
            vault
                .native
                .root
                .write_atomic(&layout::current_pointer(), &canonical_bytes(&intent.after)?)?;
            vault
                .native
                .root
                .write_atomic(AUTOMATION_WITNESS, &canonical_bytes(&intent.after)?)?;
        }
        vault.current(&state)?;
        *vault
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)? = state;
        drop(_writer);
        Ok(vault)
    }
    pub fn open(
        root: &VerifiedRoot,
        host: DeviceId,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        Self::load(root, host, clock, ids, faults, false)
    }
    /// Explicit native-primary forward recovery only; never repairs on open/read.
    pub fn recover(
        root: &VerifiedRoot,
        host: DeviceId,
        system: &ActorRef,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        // Authenticate the exact native System before any recovery write.
        let b = ManagedRoot::new(root.path(), faults.clone())
            .read_bounded(&layout::vault_descriptor(), 16 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        let descriptor: AutomationDescriptor = parse_document(&b)?;
        if descriptor.system_actor != *system || system.actor_type != ActorType::System {
            return Err(AutomationFault::Denied);
        }
        Self::load(root, host, clock, ids, faults, true)
    }
    fn commit(
        &self,
        state: &mut State,
        actor: ActorRef,
        delta: SegmentedDelta,
    ) -> Result<AutomationCurrent> {
        self.commit_checked(state, actor, delta, false)
    }
    /// Shares all transaction admission/size checks with native no-write preflight.
    fn commit_checked(
        &self,
        state: &mut State,
        actor: ActorRef,
        delta: SegmentedDelta,
        preflight: bool,
    ) -> Result<AutomationCurrent> {
        let before = state.head.as_ref();
        let now = self.native.now()?;
        let sequence = before.map_or(Ok(Revision::new(1).unwrap()), |h| next(h.sequence))?;
        if sequence.get() as usize > MAX_SEGMENTED_COMMITS {
            return Err(AutomationFault::Capacity);
        }
        let parent = before.map(SegmentedCommit::current);
        let id = CommitId::from_random(self.native.ids.random_16());
        let pw = before.map_or(Revision::new(1).unwrap(), |h| h.policy_watermark);
        let dw = before.map_or(Revision::new(1).unwrap(), |h| h.deletion_watermark);
        let pw = if matches!(delta, SegmentedDelta::Policy { .. }) {
            next(pw)?
        } else {
            pw
        };
        let dw = if matches!(delta, SegmentedDelta::Delete { .. }) {
            next(dw)?
        } else {
            dw
        };
        let checkpoint = Checkpoint {
            commit_id: id.clone(),
            commit_sequence: sequence,
            snapshot_hash: digest(&(&parent, &delta)),
            policy_epoch: pw.get(),
            deletion_epoch: dw.get(),
        };
        let authority = if let Some(h) = before {
            h.authority
                .advance(&self.native.binding, &h.checkpoint(), checkpoint)
        } else {
            AuthorityState::initial(
                self.native.descriptor.vault_id.clone(),
                self.native.descriptor.host_id.clone(),
                checkpoint,
            )
        }
        .map_err(|_| AutomationFault::Fenced)?;
        let c = SegmentedCommit {
            schema_version: NextVersion,
            kind: SegmentedKind::Commit,
            vault_id: self.native.descriptor.vault_id.clone(),
            commit_id: id,
            sequence,
            parent: parent.clone(),
            actor,
            created_at: now,
            policy_watermark: pw,
            deletion_watermark: dw,
            delta,
            authority,
        };
        let admitted = state.check(&self.native.descriptor, &c)?;
        let bytes = canonical_bytes(&c)?;
        if state
            .chain_bytes
            .checked_add(bytes.len())
            .is_none_or(|n| n > MAX_SEGMENTED_CHAIN_BYTES)
        {
            return Err(AutomationFault::Capacity);
        }
        let pointer = c.current();
        if preflight {
            return Ok(pointer);
        }
        let intent = AutomationPublication {
            schema_version: NextVersion,
            kind: PublicationKind::AutomationPublication,
            before: parent,
            after: pointer.clone(),
        };
        let publish = || -> Result<()> {
            let path = Self::path(&pointer.manifest_sha256);
            match self.native.root.read_bounded(&path, MAX_SEGMENT_BYTES)? {
                Some(existing) if existing == bytes => {}
                Some(_) => return Err(AutomationFault::Conflict),
                None => self.native.root.write_new(&path, &bytes)?,
            }
            let faults = self.native.root.faults();
            faults
                .io(FaultPoint::StagingWritten)
                .map_err(|_| AutomationFault::StorageFailed)?;
            self.native
                .root
                .write_atomic(AUTOMATION_INTENT, &canonical_bytes(&intent)?)?;
            faults
                .io(FaultPoint::HostIntentPublished)
                .map_err(|_| AutomationFault::StorageFailed)?;
            faults
                .io(FaultPoint::BeforeCurrent)
                .map_err(|_| AutomationFault::StorageFailed)?;
            self.native
                .root
                .write_atomic(&layout::current_pointer(), &canonical_bytes(&pointer)?)?;
            faults
                .io(FaultPoint::AfterCurrent)
                .map_err(|_| AutomationFault::StorageFailed)?;
            self.native
                .root
                .write_atomic(AUTOMATION_WITNESS, &canonical_bytes(&pointer)?)?;
            faults
                .io(FaultPoint::HostWitnessPublished)
                .map_err(|_| AutomationFault::StorageFailed)?;
            Ok(())
        };
        if publish().is_err() {
            self.native.poisoned.store(true, Ordering::SeqCst);
            return Err(AutomationFault::OutcomeUnknown);
        }
        state.apply(c, admitted, bytes.len());
        Ok(pointer)
    }
    pub fn snapshot(&self) -> Result<SnapshotBinding> {
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        state.snapshot()
    }
    pub fn set_policy(
        &self,
        owner: &ActorRef,
        policy: AutomationPolicy,
    ) -> Result<AutomationCurrent> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if state.policies.get(&policy.policy_id) == Some(&policy) {
            return Ok(state.head.as_ref().unwrap().current());
        }
        self.commit(
            &mut state,
            owner.clone(),
            SegmentedDelta::Policy {
                policy: Box::new(policy),
            },
        )
    }
    pub fn capture(
        &self,
        system: &ActorRef,
        sources: Vec<CapturedSource>,
        catalog: RecordCatalog,
    ) -> Result<AutomationCurrent> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if !sources.is_empty()
            && sources
                .iter()
                .all(|s| state.source(&s.reference()) == Some(s))
        {
            if catalog.entities.iter().all(|e| {
                state
                    .entities
                    .get(&e.entity_id)
                    .is_some_and(|v| v.contains(e))
            }) && catalog.conversations.iter().all(|c| {
                state
                    .conversations
                    .get(&c.conversation_id)
                    .is_some_and(|v| v.contains(c))
            }) {
                return Ok(state.head.as_ref().unwrap().current());
            }
            return Err(AutomationFault::Conflict);
        }
        self.commit(
            &mut state,
            system.clone(),
            SegmentedDelta::Capture { sources, catalog },
        )
    }
    pub fn prepare(&self, system: &ActorRef, mut input: AdmissionInput) -> Result<AdmissionInput> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let p = state
            .policies
            .get(&input.policy.policy_id)
            .ok_or(AutomationFault::Denied)?;
        input.snapshot = state.snapshot()?;
        input.actor = system.clone();
        input.policy = p.reference();
        input.subject_id = p.subject_id.clone();
        input.project_id = p.project_id.clone();
        input.access_policy_id = p.access_policy_id.clone();
        input.egress_policy_id = p.egress_policy_id.clone();
        input.operation_id = OperationId::from_random(self.native.ids.random_16());
        input.result = if let Some(target) = &input.target {
            let old = state
                .memories
                .get(&target.memory.memory_id)
                .and_then(|v| v.last())
                .ok_or(AutomationFault::Stale)?;
            if old.target != *target {
                return Err(AutomationFault::Stale);
            }
            MemoryRevisionRef {
                memory_id: target.memory.memory_id.clone(),
                revision: next(target.memory.revision)?,
            }
        } else {
            MemoryRevisionRef {
                memory_id: MemoryId::from_random(self.native.ids.random_16()),
                revision: Revision::new(1).unwrap(),
            }
        };
        if let Some(a) = input
            .record_fields
            .as_mut()
            .and_then(|f| f.annotations.as_mut())
        {
            a.memory = input.result.clone();
        }
        state.context(&input, system, &self.native.now()?, |ctx| {
            evaluate(&input, ctx)
        })??;
        state.record_fields(&input)?;
        Ok(input)
    }
    pub fn evaluate(&self, system: &ActorRef, input: &AdmissionInput) -> Result<DecisionDraft> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        state
            .context(input, system, &self.native.now()?, |ctx| {
                evaluate(input, ctx)
            })?
            .map_err(AutomationFault::from)
    }
    pub fn apply_batch(
        &self,
        system: &ActorRef,
        requests: Vec<AdmissionInput>,
        responses: Vec<String>,
        bindings: Vec<GenerationBinding>,
    ) -> Result<Vec<MemoryReceipt>> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let key = admission_batch_key(&requests);
        let hash = digest(&bindings);
        if let Some((expected, receipts)) = state.jobs.get(&key) {
            if *expected != hash
                || requests.len() != receipts.len()
                || requests
                    .iter()
                    .zip(receipts)
                    .any(|(r, receipt)| digest(r) != receipt.request_hash)
                || responses.len() != bindings.len()
                || responses
                    .iter()
                    .zip(&bindings)
                    .any(|(s, b)| sha256(s.as_bytes()) != b.response_hash)
            {
                return Err(AutomationFault::Conflict);
            }
            let allowed = state.authorized(
                &self.native.descriptor,
                Sensitivity::HighlySensitive,
                self.native.now()?,
            )?;
            if receipts
                .iter()
                .any(|r| !allowed.memories.iter().any(|m| m.memory.memory == r.result))
            {
                return Err(AutomationFault::Denied);
            }
            return Ok(receipts.clone());
        }
        self.commit(
            &mut state,
            system.clone(),
            SegmentedDelta::Admit {
                job_key: key.clone(),
                requests,
                responses,
                bindings,
            },
        )?;
        Ok(state.jobs[&key].1.clone())
    }
    pub fn delete(
        &self,
        owner: &ActorRef,
        sources: Vec<SourceId>,
        memories: Vec<MemoryId>,
    ) -> Result<AutomationCurrent> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        self.commit(
            &mut state,
            owner.clone(),
            SegmentedDelta::Delete { sources, memories },
        )
    }
    pub fn read<T>(
        &self,
        system: &ActorRef,
        max: Sensitivity,
        f: impl FnOnce(AuthorizedSegmentedRead<'_>) -> T,
    ) -> Result<T> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        Ok(f(state.authorized(
            &self.native.descriptor,
            max,
            self.native.now()?,
        )?))
    }
}

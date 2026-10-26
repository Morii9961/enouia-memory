use super::*;
use enouia_memory_contract::hash::sha256;

pub const AUTOMATION_INTENT: &str = "config/automation-publication-intent.json";
pub const AUTOMATION_WITNESS: &str = "config/automation-publication-witness.json";

impl AutomationVault {
    fn pointer(&self, path: &str) -> Result<Option<AutomationCurrent>> {
        self.root
            .read_bounded(path, 16 * 1024)?
            .map(|bytes| parse_document(&bytes).map_err(AutomationFault::from))
            .transpose()
    }
    fn intent(&self) -> Result<AutomationPublication> {
        let bytes = self
            .root
            .read_bounded(AUTOMATION_INTENT, 32 * 1024)?
            .ok_or(AutomationFault::RecoveryRequired)?;
        Ok(parse_document(&bytes)?)
    }
    fn commit_path(hash: &Sha256Hex) -> String {
        format!("vault/automation-commits/{hash}.json")
    }
    fn chain(&self, head: &AutomationCurrent) -> Result<(AutomationCommit, usize)> {
        let mut next = Some(head.clone());
        let mut commits = Vec::new();
        let mut size = 0usize;
        while let Some(pointer) = next {
            if commits.len() >= MAX_AUTOMATION_CHAIN || pointer.vault_id != self.descriptor.vault_id
            {
                return Err(AutomationFault::Capacity);
            }
            let bytes = self
                .root
                .read_bounded(
                    &Self::commit_path(&pointer.manifest_sha256),
                    MAX_AUTOMATION_COMMIT_BYTES,
                )?
                .ok_or(AutomationFault::Unavailable)?;
            size = size
                .checked_add(bytes.len())
                .ok_or(AutomationFault::Capacity)?;
            if size > MAX_AUTOMATION_CHAIN_BYTES || sha256(&bytes) != pointer.manifest_sha256 {
                return Err(AutomationFault::Unavailable);
            }
            let commit = parse_document::<AutomationCommit>(&bytes)?;
            if canonical_bytes(&commit)? != bytes || commit.current() != pointer {
                return Err(AutomationFault::Unavailable);
            }
            next = commit.parent.clone();
            commits.push(commit);
        }
        let mut before = None;
        for commit in commits.iter().rev() {
            validate_delta(&self.descriptor, before, commit)?;
            before = Some(commit);
        }
        let commit = commits
            .into_iter()
            .next()
            .ok_or(AutomationFault::Unavailable)?;
        commit
            .authority
            .check_writer(&self.descriptor.vault_id, &self.binding)
            .map_err(|_| AutomationFault::Fenced)?;
        Ok((commit, size))
    }
    pub(super) fn current_locked(&self) -> Result<(AutomationCommit, usize)> {
        let descriptor = self
            .root
            .read_bounded(&layout::vault_descriptor(), 16 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        if parse_document::<AutomationDescriptor>(&descriptor)? != self.descriptor {
            return Err(AutomationFault::Fenced);
        }
        let pointer = self
            .pointer(&layout::current_pointer())?
            .ok_or(AutomationFault::RecoveryRequired)?;
        let intent = self.intent()?;
        if intent.after != pointer || self.pointer(AUTOMATION_WITNESS)? != Some(pointer.clone()) {
            return Err(AutomationFault::RecoveryRequired);
        }
        let (commit, bytes) = self.chain(&pointer)?;
        if commit.parent != intent.before {
            return Err(AutomationFault::RecoveryRequired);
        }
        Ok((commit, bytes))
    }
    pub(super) fn publish(
        &self,
        before: Option<AutomationCurrent>,
        after: &AutomationCommit,
        chain_bytes: usize,
    ) -> Result<()> {
        let bytes = canonical_bytes(after)?;
        if after.sequence.get() as usize > MAX_AUTOMATION_CHAIN
            || chain_bytes + bytes.len() > MAX_AUTOMATION_CHAIN_BYTES
        {
            return Err(AutomationFault::Capacity);
        }
        let pointer = after.current();
        let intent = AutomationPublication {
            schema_version: NextVersion,
            kind: PublicationKind::AutomationPublication,
            before,
            after: pointer.clone(),
        };
        if !intent.validate().is_empty() {
            return Err(AutomationFault::Unavailable);
        }
        let publish = || -> Result<()> {
            let path = Self::commit_path(&pointer.manifest_sha256);
            match self.root.read_bounded(&path, MAX_AUTOMATION_COMMIT_BYTES)? {
                Some(existing) if existing == bytes => {}
                Some(_) => return Err(AutomationFault::Conflict),
                None => self.root.write_new(&path, &bytes)?,
            }
            self.root
                .faults()
                .io(FaultPoint::StagingWritten)
                .map_err(|_| AutomationFault::StorageFailed)?;
            self.root
                .write_atomic(AUTOMATION_INTENT, &canonical_bytes(&intent)?)?;
            self.root
                .faults()
                .io(FaultPoint::HostIntentPublished)
                .map_err(|_| AutomationFault::StorageFailed)?;
            self.root
                .faults()
                .io(FaultPoint::BeforeCurrent)
                .map_err(|_| AutomationFault::StorageFailed)?;
            self.root
                .write_atomic(&layout::current_pointer(), &canonical_bytes(&pointer)?)?;
            self.root
                .faults()
                .io(FaultPoint::AfterCurrent)
                .map_err(|_| AutomationFault::StorageFailed)?;
            self.root
                .write_atomic(AUTOMATION_WITNESS, &canonical_bytes(&pointer)?)?;
            self.root
                .faults()
                .io(FaultPoint::HostWitnessPublished)
                .map_err(|_| AutomationFault::StorageFailed)?;
            Ok(())
        };
        if publish().is_err() {
            self.poisoned.store(true, Ordering::SeqCst);
            return Err(AutomationFault::OutcomeUnknown);
        }
        Ok(())
    }
    /// Complete ONLY the exact validated pending publication of this native
    /// primary. No record edits, new operation, rollback or owner impersonation.
    pub fn recover(
        root: &VerifiedRoot,
        host: DeviceId,
        system: &ActorRef,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: Arc<dyn IdSource + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        let managed = ManagedRoot::new(root.path(), faults.clone());
        let bytes = managed
            .read_bounded(&layout::vault_descriptor(), 16 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        let descriptor = parse_document::<AutomationDescriptor>(&bytes)?;
        if descriptor.layout == AutomationLayout::SegmentedV1 {
            return Err(AutomationFault::Denied);
        }
        let vault = Self::acquire(root, descriptor, host, clock, ids, faults)?;
        vault.system(system)?;
        let guard = lock::acquire(&vault.root, Duration::ZERO)?;
        let intent = vault.intent()?;
        let (after, _) = vault.chain(&intent.after)?;
        if after.parent != intent.before {
            return Err(AutomationFault::RecoveryRequired);
        }
        let current = vault.pointer(&layout::current_pointer())?;
        let witness = vault.pointer(AUTOMATION_WITNESS)?;
        if current == Some(intent.after.clone()) && witness == Some(intent.after.clone()) {
            // Already complete: return without publishing anything again.
        } else if (current == intent.before || current == Some(intent.after.clone()))
            && witness == intent.before
        {
            if let Some(before) = &intent.before {
                vault.chain(before)?;
            }
            vault
                .root
                .write_atomic(&layout::current_pointer(), &canonical_bytes(&intent.after)?)?;
            vault
                .root
                .write_atomic(AUTOMATION_WITNESS, &canonical_bytes(&intent.after)?)?;
        } else {
            return Err(AutomationFault::RecoveryRequired);
        }
        vault.current_locked()?;
        drop(guard);
        Ok(vault)
    }
}

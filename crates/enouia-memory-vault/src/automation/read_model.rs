use super::*;
pub struct AuthorizedReadData {
    pub snapshot: SnapshotBinding,
    pub now: Timestamp,
    pub current: Vec<MachineMemory>,
    pub history: Vec<MachineMemory>,
    pub sources: Vec<CapturedSource>,
    pub receipts: Vec<MemoryReceipt>,
    pub learned_times: BTreeMap<CommitId, Timestamp>,
    pub record_catalog: Option<RecordCatalog>,
}
impl AutomationVault {
    /// Same live native gates for rows, selected history, source text and links.
    /// An inaccessible selected identity is indistinguishable from absence.
    pub fn authorized_read_data(
        &self,
        actor: &ActorRef,
        max: Sensitivity,
        selected: Option<&MemoryId>,
    ) -> Result<AuthorizedReadData> {
        self.system(actor)?;
        self.read_data_with(
            AutomationLayout::HostV2,
            max,
            selected,
            |_, _| Ok(()),
            |_, _| true,
        )
    }

    pub(super) fn read_data_with(
        &self,
        expected_layout: AutomationLayout,
        max: Sensitivity,
        selected: Option<&MemoryId>,
        verify: impl FnOnce(&AutomationCommit, &Timestamp) -> Result<()>,
        allowed: impl Fn(&AutomationCommit, &MachineMemory) -> bool,
    ) -> Result<AuthorizedReadData> {
        self.available()?;
        if self.descriptor.layout != expected_layout {
            return Err(AutomationFault::Denied);
        }
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (commit, _) = self.current_locked()?;
        let now = self.now()?;
        if now < commit.created_at
            || !commit.data.policy.active
            || now < commit.data.policy.valid_from
            || now >= commit.data.policy.valid_until
        {
            return Err(AutomationFault::Denied);
        }
        verify(&commit, &now)?;
        let mut latest = BTreeMap::new();
        for m in &commit.data.memories {
            latest.insert(&m.memory.memory_id, m);
        }
        let current: Vec<_> = latest
            .values()
            .filter(|m| self.eligible(&commit, m, max, true).is_ok() && allowed(&commit, m))
            .map(|m| (*m).clone())
            .collect();
        let history =
            if selected.is_some_and(|id| current.iter().any(|m| &m.memory.memory_id == id)) {
                commit
                    .data
                    .memories
                    .iter()
                    .filter(|m| {
                        Some(&m.memory.memory_id) == selected
                            && self.eligible(&commit, m, max, true).is_ok()
                            && allowed(&commit, m)
                    })
                    .cloned()
                    .collect()
            } else {
                vec![]
            };
        let refs: std::collections::BTreeSet<_> = current
            .iter()
            .chain(&history)
            .flat_map(|m| m.decision.input.input_refs.iter().cloned())
            .collect();
        let sources = commit
            .data
            .sources
            .iter()
            .filter(|s| refs.contains(&s.reference()))
            .cloned()
            .collect();
        let visible: std::collections::BTreeSet<_> = current
            .iter()
            .chain(&history)
            .map(|m| (&m.memory.memory_id, m.memory.revision))
            .collect();
        let receipts = commit
            .data
            .receipts
            .iter()
            .filter(|r| visible.contains(&(&r.result.memory_id, r.result.revision)))
            .cloned()
            .collect();
        // CURRENT's full chain was already hash/set/delta validated. Resolve
        // actual containing commit time; refuse ambiguous provenance, never use
        // source capture/import time as learned time.
        let mut learned_times = BTreeMap::new();
        let mut pointer = Some(commit.current());
        while let Some(head) = pointer {
            let bytes = self
                .root
                .read_bounded(
                    &format!("vault/automation-commits/{}.json", head.manifest_sha256),
                    MAX_AUTOMATION_COMMIT_BYTES,
                )?
                .ok_or(AutomationFault::Unavailable)?;
            let state = parse_document::<AutomationCommit>(&bytes)?;
            if state.current() != head
                || learned_times
                    .insert(state.commit_id.clone(), state.created_at.clone())
                    .is_some()
            {
                return Err(AutomationFault::Unavailable);
            }
            pointer = state.parent;
        }
        Ok(AuthorizedReadData {
            snapshot: commit.snapshot(),
            now,
            current,
            history,
            sources,
            receipts,
            learned_times,
            record_catalog: commit.data.record_catalog.clone(),
        })
    }
}

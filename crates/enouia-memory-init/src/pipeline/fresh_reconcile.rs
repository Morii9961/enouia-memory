use super::*;

impl Initializer {
    /// Reconcile a stopped compact run after its first capture and all of that
    /// batch's admissions completed, but its next batch published only a policy.
    /// The trusted caller authenticates the registered Owner before opening.
    pub fn reconcile_fresh_source_overlap(
        &mut self,
        run: &Sha256Hex,
        plan_check: &[Plan],
        expected_head: &CommitId,
        expected_journal: &Sha256Hex,
    ) -> Result<JournalCapacity> {
        if self.poisoned {
            return Err(AutomationFault::OutcomeUnknown);
        }
        let hashes = self
            .compact_plan_objects
            .as_ref()
            .ok_or(AutomationFault::Conflict)?;
        let [first, pending, ..] = self.journal.plans.as_slice() else {
            return Err(AutomationFault::Conflict);
        };
        if self.journal.fresh_run.as_ref() != Some(run)
            || plan_check.len() != self.journal.plans.len()
            || !self.journal.plans.iter().zip(plan_check).all(|(old, new)| {
                old.input_watermark == new.input_watermark && old.analysis_hash == new.analysis_hash
            })
            || !first.captured
            || first.work.is_empty()
            || first.work.iter().any(|w| {
                w.state != "committed"
                    || w.receipt.is_none()
                    || w.binding.is_none()
                    || w.request.is_none()
                    || w.response.is_none()
            })
            || pending.policy.revision != revision()
            || pending.policy.access_policy_id == first.policy.access_policy_id
        {
            return Err(AutomationFault::Conflict);
        }
        let old_journal = self
            .managed
            .read_bounded(JOURNAL, 16 * 1024 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        if &sha256(&old_journal) != expected_journal {
            return Err(AutomationFault::Stale);
        }
        let snapshot = self.vault.snapshot()?;
        if &snapshot.base_commit_id != expected_head {
            return Err(AutomationFault::Stale);
        }
        let raw = self
            .managed
            .read_bounded(
                &enouia_memory_contract::layout::current_pointer(),
                16 * 1024,
            )?
            .ok_or(AutomationFault::Unavailable)?;
        let current: AutomationCurrent = parse_document(&raw)?;
        let raw = self
            .managed
            .read_bounded(
                &format!("vault/automation-segments/{}.json", current.manifest_sha256),
                MAX_SEGMENT_BYTES,
            )?
            .ok_or(AutomationFault::Unavailable)?;
        let head: SegmentedCommit = parse_document(&raw)?;
        // Genesis, the first policy and capture, the retained receipts, then
        // exactly one pending policy. Native open has already replayed the chain.
        if head.current() != current
            || head.snapshot() != snapshot
            || head.sequence.get() != first.work.len() as u64 + 4
            || !matches!(&head.delta, SegmentedDelta::Policy { policy } if **policy == pending.policy)
        {
            return Err(AutomationFault::Conflict);
        }
        self.vault
            .read(&self.system, Sensitivity::HighlySensitive, |data| {
                if data.snapshot != snapshot
                    || !data.owner_actions.is_empty()
                    || data.sources.len() != first.sources.len()
                    || data.memories.len() != first.work.len()
                    || first
                        .sources
                        .iter()
                        .any(|s| data.sources.get(&s.record.source_id) != Some(&s))
                    || self.journal.plans.iter().any(|p| {
                        !p.policy.active
                            || data.now < p.policy.valid_from
                            || data.now >= p.policy.valid_until
                    })
                {
                    return Err(AutomationFault::Conflict);
                }
                for work in &first.work {
                    let request = work.request.as_ref().unwrap();
                    let response = work.response.as_ref().unwrap();
                    let binding = work.binding.as_ref().unwrap();
                    if !data.memories.iter().any(|m| {
                        Some(&m.receipt) == work.receipt.as_ref()
                            && &m.memory.request == request
                            && &m.memory.response_json == response
                            && binding.operation_id == request.operation_id
                            && binding.input_hash == m.receipt.request_hash
                            && binding.response_hash == m.receipt.response_hash
                    }) {
                        return Err(AutomationFault::Conflict);
                    }
                }
                Ok(())
            })??;

        let mut replacement = self.journal.clone();
        // The pending policy already occupies revision 1. Reuse the same ID
        // and publish its next revision only in the ordinary capture step.
        replacement.plans[1].policy.revision = Revision::new(2).unwrap();
        reuse_fresh_sources(&mut replacement.plans, 1)?;
        if !replacement.plans[1].sources.iter().any(|s| {
            first
                .sources
                .iter()
                .any(|old| old.reference() == s.reference())
        }) {
            return Err(AutomationFault::Conflict);
        }
        let next = &mut replacement.plans[1];
        next.analysis_artifact["source_reconciliation"] = json!({
            "previous_journal": expected_journal, "previous_head": expected_head,
        });
        next.object_hashes.push(expected_journal.clone());
        let descriptor = self.vault.descriptor();
        let objects = fresh_plan_suffix_objects(&replacement.plans, descriptor, run, 1)?;
        let untouched_hash = &hashes[0];
        let untouched_path = enouia_memory_contract::layout::raw_object(untouched_hash);
        let untouched = self
            .managed
            .read_bounded(&untouched_path, 16 * 1024 * 1024)?
            .ok_or(AutomationFault::Unavailable)?;
        if &sha256(&untouched) != untouched_hash {
            return Err(AutomationFault::Conflict);
        }
        let new_hashes: Vec<_> = std::iter::once(untouched_hash.clone())
            .chain(objects.iter().map(|(hash, _)| hash.clone()))
            .collect();
        let capacity = journal_capacity(
            replacement.clone(),
            &new_hashes,
            std::iter::once(untouched.len()).chain(objects.iter().map(|(_, bytes)| bytes.len())),
        )?;
        if !capacity.fits {
            return Err(AutomationFault::Capacity);
        }
        let objects = objects
            .into_iter()
            .chain([(expected_journal.clone(), old_journal)])
            .collect();
        if self.vault.snapshot()? != snapshot {
            return Err(AutomationFault::Stale);
        }
        self.vault.archive_import_objects(&self.system, &objects)?;
        if self.vault.snapshot()? != snapshot {
            return Err(AutomationFault::Stale);
        }
        self.journal = replacement;
        self.compact_plan_objects = Some(new_hashes);
        if self.save().is_err() {
            self.poisoned = true;
            return Err(AutomationFault::OutcomeUnknown);
        }
        Ok(capacity)
    }
}

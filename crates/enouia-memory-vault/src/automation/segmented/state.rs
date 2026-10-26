use super::*;
use enouia_memory_contract::common::SourceRevisionRef;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone)]
pub struct SegmentedMemory {
    pub memory: MachineMemory,
    pub receipt: MemoryReceipt,
    pub learned_at: Timestamp,
    pub target: TargetRef,
    pub search_text: String,
}
#[derive(Default)]
pub(super) struct State {
    pub head: Option<SegmentedCommit>,
    pub published: Vec<super::process::PublishedChange>,
    pub publication_positions: BTreeMap<CommitId, usize>,
    pub policies: BTreeMap<PolicyId, AutomationPolicy>,
    pub sources: BTreeMap<SourceId, Vec<CapturedSource>>,
    pub memories: BTreeMap<MemoryId, Vec<SegmentedMemory>>,
    pub entities: BTreeMap<String, Vec<EntityRecord>>,
    pub conversations: BTreeMap<SessionId, Vec<ConversationRecord>>,
    pub deleted_sources: BTreeSet<SourceId>,
    pub deleted_memories: BTreeSet<MemoryId>,
    pub operations: BTreeMap<OperationId, (Sha256Hex, Sha256Hex)>,
    pub jobs: BTreeMap<Sha256Hex, (Sha256Hex, Vec<MemoryReceipt>)>,
    pub locators: BTreeMap<Sha256Hex, SourceId>,
    pub source_revisions: usize,
    pub memory_revisions: usize,
    pub catalog_revisions: usize,
    pub chain_bytes: usize,
}
fn deny() -> AutomationFault {
    AutomationFault::Denied
}
impl State {
    pub fn snapshot(&self) -> Result<SnapshotBinding> {
        self.head
            .as_ref()
            .map(SegmentedCommit::snapshot)
            .ok_or(AutomationFault::Unavailable)
    }
    pub fn source(&self, r: &SourceRevisionRef) -> Option<&CapturedSource> {
        self.sources
            .get(&r.source_id)?
            .last()
            .filter(|s| s.record.revision == r.source_revision)
    }
    pub fn policy(&self, input: &AdmissionInput) -> Option<&AutomationPolicy> {
        self.policies
            .get(&input.policy.policy_id)
            .filter(|p| p.reference() == input.policy)
    }
    pub fn context<T>(
        &self,
        input: &AdmissionInput,
        actor: &ActorRef,
        now: &Timestamp,
        f: impl FnOnce(&EvaluationContext<'_>) -> T,
    ) -> Result<T> {
        let policy = self.policy(input).ok_or_else(deny)?;
        let snapshot = self.snapshot()?;
        let sources: Vec<_> = input
            .input_refs
            .iter()
            .map(|r| {
                let source = self.source(r).ok_or_else(deny)?;
                Ok(SourceView {
                    record: &source.record,
                    attributed_subject: source.attributed_subject.clone(),
                    deleted: self.deleted_sources.contains(&r.source_id),
                    readable: policy.source_ids.contains(&r.source_id)
                        && source.record.sensitivity <= policy.max_sensitivity
                        && source.record.access_policy_id == policy.access_policy_id,
                })
            })
            .collect::<Result<_>>()?;
        let mut ids = BTreeSet::from([input.result.memory_id.clone()]);
        if let Some(t) = &input.target {
            ids.insert(t.memory.memory_id.clone());
        }
        if let Some(d) = &input.dimensions {
            ids.extend(d.derived_from.iter().map(|t| t.memory.memory_id.clone()));
            if let Some(t) = &d.reverts_to {
                ids.insert(t.memory.memory_id.clone());
            }
        }
        let targets: Vec<_> = ids
            .iter()
            .filter_map(|id| self.memories.get(id))
            .map(|history| {
                let latest = history.last().unwrap();
                let m = &latest.memory;
                let i = &m.decision.input;
                TargetView {
                    created_under_policy: Some(m.request.policy.policy_id.clone()),
                    target: latest.target.clone(),
                    legacy_owner_record: false,
                    subject_id: i.subject_id.clone(),
                    project_id: i.project_id.clone(),
                    sensitivity: i.sensitivity,
                    access_policy_id: i.access_policy_id.clone(),
                    egress_policy_id: i.egress_policy_id.clone(),
                    writable: i.subject_id == policy.subject_id
                        && i.project_id == policy.project_id,
                    dimensions: i.dimensions.clone(),
                    input_refs: i.input_refs.clone(),
                    policy: i.policy.clone(),
                    deleted: self.deleted_memories.contains(&m.memory.memory_id),
                    history: history
                        .iter()
                        .map(|v| HistoricalMemoryView {
                            target: v.target.clone(),
                            content: v.memory.decision.input.content.clone(),
                            interpretation: v.memory.decision.input.interpretation,
                            dimensions: v.memory.decision.input.dimensions.clone(),
                            input_refs: v.memory.decision.input.input_refs.clone(),
                        })
                        .collect(),
                }
            })
            .collect();
        Ok(f(&EvaluationContext {
            system_actor: actor,
            policy,
            snapshot: &snapshot,
            now,
            sources: &sources,
            targets: &targets,
        }))
    }
    pub(super) fn source_in_policy(
        &self,
        r: &enouia_memory_contract::common::SourceRevisionRef,
        p: &AutomationPolicy,
        max: Sensitivity,
    ) -> bool {
        !self.deleted_sources.contains(&r.source_id)
            && p.source_ids.contains(&r.source_id)
            && self.source(r).is_some_and(|s| {
                s.record.access_policy_id == p.access_policy_id
                    && s.record.sensitivity <= max.min(p.max_sensitivity)
                    && s.attributed_subject
                        .as_ref()
                        .is_none_or(|subject| subject == &p.subject_id)
            })
    }
    pub(super) fn entity_in_policy(
        &self,
        id: &str,
        p: &AutomationPolicy,
        max: Sensitivity,
    ) -> bool {
        self.entities
            .get(id)
            .and_then(|v| v.last())
            .is_some_and(|e| {
                e.sensitivity <= max.min(p.max_sensitivity)
                    && e.access_policy_id == p.access_policy_id
                    && e.evidence
                        .iter()
                        .chain(e.aliases.iter().flat_map(|a| &a.evidence))
                        .all(|q| {
                            self.source_in_policy(
                                &enouia_memory_contract::common::SourceRevisionRef {
                                    source_id: q.reference.source_id.clone(),
                                    source_revision: q.reference.source_revision,
                                },
                                p,
                                e.sensitivity,
                            )
                        })
            })
    }
    pub(super) fn record_fields(&self, input: &AdmissionInput) -> Result<()> {
        let fields = input.record_fields.as_ref().ok_or_else(deny)?;
        let policy = self.policy(input).ok_or_else(deny)?;
        if fields
            .entities
            .iter()
            .any(|e| !self.entity_in_policy(&e.entity_id, policy, input.sensitivity))
        {
            return Err(deny());
        }
        // Original-source closure alone cannot make a stale/cyclic current
        // derivation valid. Bound its exact current parent closure before write.
        let mut pending: Vec<_> = input
            .dimensions
            .as_ref()
            .into_iter()
            .flat_map(|d| d.derived_from.iter().cloned())
            .collect();
        let mut visited = BTreeSet::new();
        while let Some(target) = pending.pop() {
            if target.memory.memory_id == input.result.memory_id
                || self.deleted_memories.contains(&target.memory.memory_id)
            {
                return Err(deny());
            }
            if !visited.insert(target.memory.memory_id.clone()) {
                continue;
            }
            if visited.len() > MAX_SOURCES {
                return Err(AutomationFault::Capacity);
            }
            let current = self
                .memories
                .get(&target.memory.memory_id)
                .and_then(|v| v.last())
                .ok_or_else(deny)?;
            if current.target != target || current.memory.decision.input.policy != input.policy {
                return Err(deny());
            }
            pending.extend(
                current
                    .memory
                    .decision
                    .input
                    .dimensions
                    .as_ref()
                    .into_iter()
                    .flat_map(|d| d.derived_from.iter().cloned()),
            );
        }

        let selected: Vec<_> = input
            .input_refs
            .iter()
            .map(|r| self.source(r).cloned().ok_or_else(deny))
            .collect::<Result<_>>()?;
        if !fields.evidence.iter().all(|q| q.resolves(&selected))
            || [&fields.event_time, &fields.valid_time]
                .into_iter()
                .any(|t| {
                    t.basis == TimeBasis::SourceTime
                        && !selected.iter().any(|s| {
                            s.record.occurred_at == t.from
                                && fields
                                    .evidence
                                    .iter()
                                    .any(|q| q.reference.source_id == s.record.source_id)
                        })
                })
            || fields.entities.iter().any(|e| {
                self.entities
                    .get(&e.entity_id)
                    .and_then(|v| v.last())
                    .is_none_or(|v| {
                        v.sensitivity > input.sensitivity
                            || v.access_policy_id != input.access_policy_id
                            || (e.role == EntityRole::Project && v.project_id != input.project_id)
                    })
            })
        {
            return Err(deny());
        }
        if let Some(target) = input
            .dimensions
            .as_ref()
            .and_then(|d| d.reverts_to.as_ref())
        {
            let old = self
                .memories
                .get(&target.memory.memory_id)
                .and_then(|v| v.iter().find(|v| v.target == *target))
                .ok_or_else(deny)?;
            if old
                .memory
                .decision
                .input
                .record_fields
                .as_ref()
                .is_none_or(|old| !fields.same_claim(old))
            {
                return Err(deny());
            }
        }
        Ok(())
    }
    pub fn check(
        &self,
        descriptor: &AutomationDescriptor,
        c: &SegmentedCommit,
    ) -> Result<Vec<SegmentedMemory>> {
        if self.publication_positions.contains_key(&c.commit_id)
            || !c.validate().is_empty()
            || c.vault_id != descriptor.vault_id
            || c.authority.primary.host_id != descriptor.host_id
            || c.authority.primary.primary_epoch.get() != 1
        {
            return Err(deny());
        }
        let (pw, dw) = self.head.as_ref().map_or((1, 1), |h| {
            (h.policy_watermark.get(), h.deletion_watermark.get())
        });
        let policy_change = matches!(c.delta, SegmentedDelta::Policy { .. });
        let delete = matches!(c.delta, SegmentedDelta::Delete { .. });
        if let Some(before) = &self.head {
            if c.parent != Some(before.current())
                || c.created_at < before.created_at
                || c.policy_watermark.get() != pw + u64::from(policy_change)
                || c.deletion_watermark.get() != dw + u64::from(delete)
                || before
                    .authority
                    .advance(
                        &before.authority.primary,
                        &before.checkpoint(),
                        c.checkpoint(),
                    )
                    .ok()
                    .as_ref()
                    != Some(&c.authority)
            {
                return Err(AutomationFault::Stale);
            }
        } else if !matches!(c.delta, SegmentedDelta::Genesis { .. })
            || c.parent.is_some()
            || pw != c.policy_watermark.get()
            || dw != c.deletion_watermark.get()
            || enouia_memory_contract::host::AuthorityState::initial(
                c.vault_id.clone(),
                descriptor.host_id.clone(),
                c.checkpoint(),
            )
            .ok()
            .as_ref()
                != Some(&c.authority)
        {
            return Err(deny());
        }
        let mut admitted = Vec::new();
        match &c.delta {
            SegmentedDelta::Genesis { .. } => {
                if self.head.is_some() || c.actor != descriptor.created_by {
                    return Err(deny());
                }
            }
            SegmentedDelta::Policy { policy: p } => {
                if c.actor != descriptor.created_by
                    || p.vault_id != descriptor.vault_id
                    || p.system_actor != descriptor.system_actor
                    || p.revision.get()
                        != self
                            .policies
                            .get(&p.policy_id)
                            .map_or(1, |v| v.revision.get() + 1)
                    || (!self.policies.contains_key(&p.policy_id)
                        && self.policies.len() >= MAX_SEGMENTED_POLICIES)
                {
                    return Err(deny());
                }
            }
            SegmentedDelta::Capture { sources, catalog } => {
                if c.actor != descriptor.system_actor
                    || self.source_revisions + sources.len() > MAX_SEGMENTED_REVISIONS
                    || self.catalog_revisions + catalog.entities.len() + catalog.conversations.len()
                        > MAX_SEGMENTED_REVISIONS
                {
                    return Err(deny());
                }
                let mut seen = BTreeSet::new();
                let mut locators = BTreeSet::new();
                for source in sources {
                    let id = &source.record.source_id;
                    let key = digest(&source.record.locator);
                    if !seen.insert(id)
                        || !locators.insert(key.clone())
                        || self.deleted_sources.contains(id)
                        || self.locators.get(&key).is_some_and(|old| old != id)
                        || self
                            .sources
                            .get(id)
                            .and_then(|v| v.last())
                            .is_some_and(|v| v.record.locator != source.record.locator)
                        || source.record.revision.get()
                            != self
                                .sources
                                .get(id)
                                .and_then(|v| v.last())
                                .map_or(1, |v| v.record.revision.get() + 1)
                        || !self.policies.values().any(|p| {
                            p.active
                                && c.created_at >= p.valid_from
                                && c.created_at < p.valid_until
                                && p.source_ids.contains(id)
                                && source.record.sensitivity <= p.max_sensitivity
                                && source.record.access_policy_id == p.access_policy_id
                                && source
                                    .attributed_subject
                                    .as_ref()
                                    .is_none_or(|s| s == &p.subject_id)
                        })
                    {
                        return Err(deny());
                    }
                }
                self.check_catalog(sources, catalog, &c.created_at)?;
            }
            SegmentedDelta::Admit {
                job_key,
                requests,
                responses,
                bindings,
            } => {
                if c.actor != descriptor.system_actor
                    || self.memory_revisions + requests.len() > MAX_SEGMENTED_REVISIONS
                    || self.jobs.contains_key(job_key)
                {
                    return Err(deny());
                }
                let mut ids = BTreeSet::new();
                let mut operations = BTreeSet::new();
                for ((request, response), binding) in requests.iter().zip(responses).zip(bindings) {
                    if !ids.insert(&request.result.memory_id)
                        || !operations.insert(&request.operation_id)
                        || self.operations.contains_key(&request.operation_id)
                    {
                        return Err(AutomationFault::Conflict);
                    }
                    self.record_fields(request)?;
                    self.context(request, &descriptor.system_actor, &c.created_at, |ctx| {
                        evaluate(request, ctx)
                    })??;
                    let decision: DecisionDraft = parse_document(response.as_bytes())?;
                    if self.context(
                        &decision.input,
                        &descriptor.system_actor,
                        &c.created_at,
                        |ctx| evaluate(&decision.input, ctx),
                    )?? != decision
                    {
                        return Err(deny());
                    }
                    self.record_fields(&decision.input)?;
                    let memory = MachineMemory {
                        schema_version: NextVersion,
                        kind: MachineKind::MachineMemory,
                        memory: decision.input.result.clone(),
                        request: request.clone(),
                        decision,
                        response_hash: binding.response_hash.clone(),
                        response_json: response.clone(),
                        committed_in: c.commit_id.clone(),
                    };
                    let receipt = MemoryReceipt {
                        job_key: job_key.clone(),
                        binding_hash: digest(bindings),
                        operation_id: request.operation_id.clone(),
                        request_hash: digest(request),
                        response_hash: binding.response_hash.clone(),
                        decision_hash: digest(&memory.decision),
                        committed_in: c.commit_id.clone(),
                        sequence: c.sequence,
                        result: memory.memory.clone(),
                    };
                    let search_text = format!(
                        "{}\n{}",
                        memory.decision.input.content,
                        memory.decision.input.record_fields.as_ref().unwrap().title
                    )
                    .to_lowercase();
                    admitted.push(SegmentedMemory {
                        target: memory.target(),
                        memory,
                        receipt,
                        learned_at: c.created_at.clone(),
                        search_text,
                    });
                }
            }
            SegmentedDelta::Delete { sources, memories } => {
                if c.actor != descriptor.created_by
                    || sources.len() + memories.len() > 128
                    || sources.iter().collect::<BTreeSet<_>>().len() != sources.len()
                    || memories.iter().collect::<BTreeSet<_>>().len() != memories.len()
                    || sources.iter().any(|id| {
                        !self.sources.contains_key(id) || self.deleted_sources.contains(id)
                    })
                    || memories.iter().any(|id| {
                        !self.memories.contains_key(id) || self.deleted_memories.contains(id)
                    })
                {
                    return Err(deny());
                }
            }
            SegmentedDelta::OwnerAction { .. } => return Err(deny()),
        }
        Ok(admitted)
    }
    pub fn apply(&mut self, c: SegmentedCommit, admitted: Vec<SegmentedMemory>, bytes: usize) {
        self.publication_positions
            .insert(c.commit_id.clone(), self.published.len());
        self.published
            .push(super::process::PublishedChange::from_commit(&c));
        match &c.delta {
            SegmentedDelta::Policy { policy } => {
                self.policies
                    .insert(policy.policy_id.clone(), policy.as_ref().clone());
            }
            SegmentedDelta::Capture { sources, catalog } => {
                self.source_revisions += sources.len();
                self.catalog_revisions += catalog.entities.len() + catalog.conversations.len();
                for s in sources {
                    self.locators
                        .insert(digest(&s.record.locator), s.record.source_id.clone());
                    self.sources
                        .entry(s.record.source_id.clone())
                        .or_default()
                        .push(s.clone());
                }
                for e in &catalog.entities {
                    self.entities
                        .entry(e.entity_id.clone())
                        .or_default()
                        .push(e.clone());
                }
                for v in &catalog.conversations {
                    self.conversations
                        .entry(v.conversation_id.clone())
                        .or_default()
                        .push(v.clone());
                }
            }
            SegmentedDelta::Admit {
                job_key, bindings, ..
            } => {
                self.memory_revisions += admitted.len();
                self.jobs.insert(
                    job_key.clone(),
                    (
                        digest(bindings),
                        admitted.iter().map(|m| m.receipt.clone()).collect(),
                    ),
                );
                for m in admitted {
                    self.operations.insert(
                        m.receipt.operation_id.clone(),
                        (job_key.clone(), m.receipt.request_hash.clone()),
                    );
                    self.memories
                        .entry(m.memory.memory.memory_id.clone())
                        .or_default()
                        .push(m);
                }
            }
            SegmentedDelta::Delete { sources, memories } => {
                self.deleted_sources.extend(sources.iter().cloned());
                self.deleted_memories.extend(memories.iter().cloned());
            }
            _ => {}
        }
        self.chain_bytes += bytes;
        self.head = Some(c);
    }
}

//! Authorized process/history view. Internal commit positions never cross this API.
use super::*;
use enouia_memory_contract::common::SourceRevisionRef;
use std::collections::BTreeSet;
#[derive(Clone)]
pub struct PublicationStamp {
    pub commit_id: CommitId,
    pub commit_hash: Sha256Hex,
    pub actor: ActorRef,
    pub at: Timestamp,
}
pub(super) struct PublishedChange {
    pub snapshot: SnapshotBinding,
    pub stamp: PublicationStamp,
    pub sources: Vec<SourceRevisionRef>,
    pub memories: Vec<MemoryRevisionRef>,
    pub reset: bool,
}
impl PublishedChange {
    pub fn from_commit(c: &SegmentedCommit) -> Self {
        let mut sources = Vec::new();
        let mut memories = Vec::new();
        let reset = match &c.delta {
            SegmentedDelta::Capture {
                sources: incoming,
                catalog,
            } => {
                sources = incoming.iter().map(CapturedSource::reference).collect();
                incoming.iter().any(|s| s.record.revision.get() > 1)
                    || catalog.entities.iter().any(|e| e.revision.get() > 1)
                    || catalog.conversations.iter().any(|e| e.revision.get() > 1)
            }
            SegmentedDelta::Admit { requests, .. } => {
                memories = requests.iter().map(|r| r.result.clone()).collect();
                requests.iter().any(|r| r.result.revision.get() > 1)
            }
            SegmentedDelta::OwnerAction { effects, .. } => {
                memories = effects
                    .correction
                    .iter()
                    .map(|c| c.decision.input.result.clone())
                    .collect();
                true
            }
            // Queue state is not a record change; authorized views stay valid.
            SegmentedDelta::Enqueue { .. } => false,
            _ => true,
        };
        Self {
            snapshot: c.snapshot(),
            stamp: PublicationStamp {
                commit_id: c.commit_id.clone(),
                commit_hash: c.current().manifest_sha256,
                actor: c.actor.clone(),
                at: c.created_at.clone(),
            },
            sources,
            memories,
            reset,
        }
    }
}
pub struct AuthorizedCapture<'a> {
    pub source: &'a CapturedSource,
    pub stamp: &'a PublicationStamp,
}
pub struct AuthorizedMachineEvent<'a> {
    pub entry: &'a SegmentedMemory,
    pub stamp: &'a PublicationStamp,
}
pub struct AuthorizedOwnerEvent<'a> {
    pub record: &'a OwnerActionRecord,
    pub stamp: &'a PublicationStamp,
}
pub struct AuthorizedProcessRead<'a> {
    pub owner_events: Vec<AuthorizedOwnerEvent<'a>>,
    pub current: AuthorizedSegmentedRead<'a>,
    pub history: Vec<&'a SegmentedMemory>,
    pub captures: Vec<AuthorizedCapture<'a>>,
    pub machine_events: Vec<AuthorizedMachineEvent<'a>>,
    pub permission_hash: Sha256Hex,
    pub reset_required: bool,
    pub changed_memories: Vec<&'a SegmentedMemory>,
    pub changed_sources: Vec<&'a CapturedSource>,
}
impl State {
    fn history_eligible(
        &self,
        entry: &SegmentedMemory,
        scope: &AuthorizedSegmentedRead<'_>,
        max: Sensitivity,
        targets: &BTreeMap<MemoryId, &TargetRef>,
    ) -> bool {
        let i = &entry.memory.decision.input;
        let Some(p) = self.policy(i) else {
            return false;
        };
        targets.contains_key(&entry.memory.memory.memory_id)
            && p.active
            && scope.now >= p.valid_from
            && scope.now < p.valid_until
            && i.subject_id == p.subject_id
            && i.project_id == p.project_id
            && i.access_policy_id == p.access_policy_id
            && i.egress_policy_id == p.egress_policy_id
            && i.sensitivity <= max.min(p.max_sensitivity)
            && i.input_refs
                .iter()
                .all(|r| self.source_in_policy(r, p, i.sensitivity))
            && i.record_fields.as_ref().is_some_and(|f| {
                f.entities
                    .iter()
                    .all(|e| self.entity_in_policy(&e.entity_id, p, i.sensitivity))
            })
            && i.dimensions.as_ref().is_some_and(|d| {
                d.derived_from.iter().all(|target| {
                    targets
                        .get(&target.memory.memory_id)
                        .is_some_and(|t| **t == *target)
                })
            })
    }
    pub(super) fn process<'a>(
        &'a self,
        scope: AuthorizedSegmentedRead<'a>,
        max: Sensitivity,
        after: Option<&SnapshotBinding>,
        include_history: bool,
    ) -> AuthorizedProcessRead<'a> {
        let grants: Vec<_> = self
            .policies
            .values()
            .filter(|p| p.active && scope.now >= p.valid_from && scope.now < p.valid_until)
            .map(AutomationPolicy::reference)
            .collect();
        let permission_hash = digest(&(
            &scope.snapshot.vault_id,
            scope.snapshot.primary_epoch,
            scope.withdrawal_watermark,
            max,
            grants,
        ));
        let targets: BTreeMap<_, _> = scope
            .memories
            .iter()
            .map(|m| (m.memory.memory.memory_id.clone(), &m.target))
            .collect();
        let history: Vec<_> = if include_history {
            self.memories
                .values()
                .flatten()
                .filter(|m| self.history_eligible(m, &scope, max, &targets))
                .collect()
        } else {
            Vec::new()
        };
        let historic: BTreeSet<_> = history
            .iter()
            .map(|m| (&m.memory.memory.memory_id, m.memory.memory.revision))
            .collect();
        let captures = if include_history {
            self.published
                .iter()
                .flat_map(|event| {
                    event.sources.iter().filter_map(|r| {
                        scope
                            .sources
                            .get(&r.source_id)
                            .filter(|s| s.record.revision == r.source_revision)
                            .map(|source| AuthorizedCapture {
                                source,
                                stamp: &event.stamp,
                            })
                    })
                })
                .collect()
        } else {
            Vec::new()
        };
        let eligible: BTreeMap<_, _> = history
            .iter()
            .map(|m| {
                (
                    (m.memory.memory.memory_id.clone(), m.memory.memory.revision),
                    *m,
                )
            })
            .collect();
        let machine_events = if include_history {
            self.published
                .iter()
                .flat_map(|event| {
                    event
                        .memories
                        .iter()
                        .filter(|r| historic.contains(&(&r.memory_id, r.revision)))
                        .map(|r| AuthorizedMachineEvent {
                            entry: eligible[&(r.memory_id.clone(), r.revision)],
                            stamp: &event.stamp,
                        })
                })
                .collect()
        } else {
            Vec::new()
        };
        let base = after.and_then(|a| {
            self.publication_positions
                .get(&a.base_commit_id)
                .copied()
                .filter(|n| &self.published[*n].snapshot == a)
        });
        let reset_required = after.is_none()
            || base.is_none()
            || after.is_some_and(|a| {
                a.policy_watermark != scope.snapshot.policy_watermark
                    || a.deletion_watermark != scope.snapshot.deletion_watermark
                    || a.primary_epoch != scope.snapshot.primary_epoch
            })
            || base.is_some_and(|n| self.published[n + 1..].iter().any(|c| c.reset));
        let mut memory_ids = BTreeSet::new();
        let mut source_ids = BTreeSet::new();
        if let Some(base) = base.filter(|_| !reset_required) {
            for event in &self.published[base + 1..] {
                memory_ids.extend(event.memories.iter().map(|r| &r.memory_id));
                source_ids.extend(event.sources.iter().map(|r| &r.source_id));
            }
        }
        let changed_memories = scope
            .memories
            .iter()
            .filter(|m| memory_ids.contains(&m.memory.memory.memory_id))
            .copied()
            .collect();
        let changed_sources = scope
            .sources
            .iter()
            .filter(|(id, _)| source_ids.contains(id))
            .map(|(_, s)| *s)
            .collect();
        let owner_events = if include_history {
            scope
                .owner_actions
                .iter()
                .filter_map(|a| {
                    self.publication_positions
                        .get(&a.record.committed_in)
                        .map(|n| AuthorizedOwnerEvent {
                            record: a.record,
                            stamp: &self.published[*n].stamp,
                        })
                })
                .collect()
        } else {
            vec![]
        };
        AuthorizedProcessRead {
            owner_events,
            current: scope,
            history,
            captures,
            machine_events,
            permission_hash,
            reset_required,
            changed_memories,
            changed_sources,
        }
    }
}
impl SegmentedAutomationVault {
    pub fn read_process<T>(
        &self,
        system: &ActorRef,
        max: Sensitivity,
        after: Option<&SnapshotBinding>,
        include_history: bool,
        f: impl FnOnce(AuthorizedProcessRead<'_>) -> T,
    ) -> Result<T> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let scope = state.authorized(&self.native.descriptor, max, self.native.now()?)?;
        Ok(f(state.process(scope, max, after, include_history)))
    }
}

//! ADR-MEM-111: queue-only admission of already captured Runtime sources. The
//! capture value is data, never authority: every binding is re-read from state.
use super::{runtime_capture::provenance, *};
use enouia_memory_contract::{common::SourceRevisionRef, source::SourceKind};
use serde::Serialize;

/// Durable waiting jobs only; this never reports formation success.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CapturedQueueReceipt {
    pub schema_version: NextVersion,
    pub kind: &'static str,
    pub queue_key: Sha256Hex,
    pub job_keys: Vec<Sha256Hex>,
    pub state: &'static str,
}
impl From<&CapturedQueueEntry> for CapturedQueueReceipt {
    fn from(entry: &CapturedQueueEntry) -> Self {
        Self {
            schema_version: NextVersion,
            kind: "captured_queue_receipt",
            queue_key: entry.queue_key.clone(),
            job_keys: entry.job_keys.clone(),
            state: "queued",
        }
    }
}
/// One dispatchable source, in native per-conversation order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuedCapture {
    pub queue_key: Sha256Hex,
    pub job_key: Sha256Hex,
    pub session_id: SessionId,
    pub source: SourceRevisionRef,
    pub request_id: RequestId,
    pub policy: PolicyRef,
}
impl State {
    fn turn_position(&self, session: &SessionId, source: &SourceRevisionRef) -> Option<usize> {
        self.conversations
            .get(session)?
            .last()?
            .turns
            .iter()
            .position(|t| t.source == *source)
    }
    fn policy_live(&self, id: &PolicyId, at: &Timestamp) -> Option<&AutomationPolicy> {
        self.policies
            .get(id)
            .filter(|p| p.active && *at >= p.valid_from && *at < p.valid_until)
    }
    /// Shared by admission and chain replay, so a forged commit cannot skip it.
    pub(super) fn queue_admissible(&self, entry: &CapturedQueueEntry, at: &Timestamp) -> bool {
        let Some(p) = self
            .policy_live(&entry.policy.policy_id, at)
            .filter(|p| p.reference() == entry.policy)
        else {
            return false;
        };
        let mut last = None;
        entry.sources.iter().all(|r| {
            let Some(position) = self.turn_position(&entry.session_id, r) else {
                return false;
            };
            let ordered = last.is_none_or(|l| l < position);
            last = Some(position);
            ordered
                && self.source_in_policy(r, p, p.max_sensitivity)
                && self.source(r).is_some_and(|s| {
                    s.record.source_kind == SourceKind::RuntimeEvent
                        && s.record_fields
                            .as_ref()
                            .and_then(|f| f.conversation_id.as_ref())
                            == Some(&entry.session_id)
                        && provenance(s).is_some_and(|m| {
                            m.request_id == entry.request_id
                                && m.request_hash == entry.request_hash
                                && m.producer == entry.producer
                        })
                })
        })
    }
}
impl SegmentedAutomationVault {
    /// Replaying the original key returns the original receipt. A changed
    /// packet, source set or policy under that key conflicts; nothing rebases.
    pub fn enqueue_captured(
        &self,
        system: &ActorRef,
        capture: &NativeRuntimeCapture,
        policy: &PolicyRef,
    ) -> Result<CapturedQueueReceipt> {
        if self.native.descriptor.layout != AutomationLayout::SegmentedOwnerV1 {
            return Err(AutomationFault::Denied);
        }
        self.native.system(system)?;
        if capture.capability != OWNER_RUNTIME_CAPTURE_CAPABILITY
            || !capture.captured
            || capture.preflight
            || capture.observed_in.vault_id != self.native.descriptor.vault_id
        {
            return Err(AutomationFault::Denied);
        }
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let queue_key = captured_queue_key(
            &self.native.descriptor.vault_id,
            &capture.producer,
            &capture.request_id,
        );
        if let Some(index) = state.capture_queue_keys.get(&queue_key) {
            let old = &state.capture_queue[*index];
            if old.request_hash != capture.request_hash
                || old.policy != *policy
                || old.sources != capture.sources
            {
                return Err(AutomationFault::Conflict);
            }
            return Ok(old.into());
        }
        let session_id = capture
            .sources
            .first()
            .and_then(|r| state.source(r))
            .and_then(|s| s.record_fields.as_ref())
            .and_then(|f| f.conversation_id.clone())
            .ok_or(AutomationFault::Denied)?;
        let entry = CapturedQueueEntry {
            job_keys: capture
                .sources
                .iter()
                .map(|s| captured_job_key(&queue_key, s))
                .collect(),
            queue_key,
            session_id,
            request_id: capture.request_id.clone(),
            request_hash: capture.request_hash.clone(),
            producer: capture.producer.clone(),
            policy: policy.clone(),
            sources: capture.sources.clone(),
        };
        let receipt = (&entry).into();
        self.commit(
            &mut state,
            self.native.descriptor.system_actor.clone(),
            SegmentedDelta::Enqueue {
                entry: Box::new(entry),
            },
        )?;
        Ok(receipt)
    }
    /// Sessions keep first-enqueue order; each follows native turn order, so a
    /// late enqueue after a crash still precedes later turns. Deleted sources
    /// drop out. A job outside the policy's current revision holds its session;
    /// it is never skipped, and resumes if that policy covers it again.
    pub fn queued_captures(&self, system: &ActorRef) -> Result<Vec<QueuedCapture>> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = self.native.now()?;
        let mut sessions: Vec<(&SessionId, BTreeMap<usize, QueuedCapture>)> = Vec::new();
        let mut held = BTreeMap::new();
        for entry in &state.capture_queue {
            let index = match sessions.iter().position(|(s, _)| *s == &entry.session_id) {
                Some(index) => index,
                None => {
                    sessions.push((&entry.session_id, BTreeMap::new()));
                    sessions.len() - 1
                }
            };
            let policy = state.policy_live(&entry.policy.policy_id, &now);
            for (source, job_key) in entry.sources.iter().zip(&entry.job_keys) {
                if state.deleted_sources.contains(&source.source_id) {
                    continue;
                }
                let position = state
                    .turn_position(&entry.session_id, source)
                    .ok_or(AutomationFault::Unavailable)?;
                if !policy.is_some_and(|p| state.source_in_policy(source, p, p.max_sensitivity)) {
                    let first: &mut usize = held.entry(index).or_insert(position);
                    *first = position.min(*first);
                    continue;
                }
                sessions[index].1.insert(
                    position,
                    QueuedCapture {
                        queue_key: entry.queue_key.clone(),
                        job_key: job_key.clone(),
                        session_id: entry.session_id.clone(),
                        source: source.clone(),
                        request_id: entry.request_id.clone(),
                        policy: entry.policy.clone(),
                    },
                );
            }
        }
        Ok(sessions
            .into_iter()
            .enumerate()
            .flat_map(|(index, (_, jobs))| {
                let hold = held.get(&index).copied();
                jobs.into_iter()
                    .filter(move |(position, _)| hold.is_none_or(|h| *position < h))
                    .map(|(_, job)| job)
            })
            .collect())
    }
}

//! Optional Agent hints use the existing witnessed Owner commit chain.
use super::*;

impl State {
    pub(super) fn note_admissible(&self, note: &AgentNoteRecord, at: &Timestamp) -> bool {
        let reference = enouia_memory_contract::common::SourceRevisionRef {
            source_id: note.evidence.reference.source_id.clone(),
            source_revision: note.evidence.reference.source_revision,
        };
        let Some(source) = self.source(&reference) else {
            return false;
        };
        let Some(policy) = self.policies.get(&note.policy.policy_id) else {
            return false;
        };
        !self.agent_notes.contains_key(&note.note_key)
            && self.agent_notes.len() < MAX_SEGMENTED_REVISIONS
            && self
                .agent_notes
                .values()
                .filter(|(n, _)| n.evidence.reference.source_id == reference.source_id)
                .count()
                < MAX_AGENT_NOTES_PER_SOURCE
            && policy.reference() == note.policy
            && policy.active
            && *at >= policy.valid_from
            && *at < policy.valid_until
            && self.source_in_policy(&reference, policy, policy.max_sensitivity)
            && self.sources[&reference.source_id]
                .last()
                .unwrap()
                .reference()
                == reference
            && source.record.source_kind == enouia_memory_contract::source::SourceKind::RuntimeEvent
            && runtime_capture::provenance(source).is_some()
            && note.evidence.resolves(std::slice::from_ref(source))
    }
}

impl SegmentedAutomationVault {
    /// Submit the original request, never a serialized preparation result.
    pub fn submit_agent_note(
        &self,
        permit: &AgentNotePermit,
        request: &AgentNoteRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<AgentNoteReceipt> {
        let draft = self.prepare_agent_note(permit, request, locate)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        permit.live(self)?;
        if state.snapshot()? != draft.snapshot {
            return Err(AutomationFault::Stale);
        }
        if let Some((old, receipt)) = state.agent_notes.get(&draft.note_key) {
            return if old.request_hash == draft.request_hash {
                Ok(receipt.clone())
            } else {
                Err(AutomationFault::Conflict)
            };
        }
        if state.agent_notes.len() >= MAX_SEGMENTED_REVISIONS
            || state
                .agent_notes
                .values()
                .filter(|(n, _)| n.evidence.reference.source_id == request.source.source_id)
                .count()
                >= MAX_AGENT_NOTES_PER_SOURCE
        {
            return Err(AutomationFault::Capacity);
        }
        let key = draft.note_key.clone();
        let note = AgentNoteRecord {
            note_key: draft.note_key,
            request_id: draft.request_id,
            request_hash: draft.request_hash,
            principal: draft.principal,
            scope_hash: draft.scope_hash,
            policy: draft.policy,
            evidence: draft.evidence,
            note: draft.note,
        };
        self.commit(
            &mut state,
            self.descriptor().system_actor.clone(),
            SegmentedDelta::AgentNote {
                note: Box::new(note),
            },
        )?;
        Ok(state.agent_notes[&key].1.clone())
    }
}

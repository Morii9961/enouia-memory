//! Recorded Agent forget requests use the existing witnessed Owner commit chain.
use super::agent::ForgetAdmission;
use super::*;

impl State {
    pub(super) fn forget_admissible(&self, report: &AgentForgetRecord, at: &Timestamp) -> bool {
        let reference = enouia_memory_contract::common::SourceRevisionRef {
            source_id: report.evidence.reference.source_id.clone(),
            source_revision: report.evidence.reference.source_revision,
        };
        let Some(source) = self.source(&reference) else {
            return false;
        };
        let Some(policy) = self.policies.get(&report.policy.policy_id) else {
            return false;
        };
        let target = &report.target.memory.memory_id;
        !self.agent_forget_requests.contains_key(&report.request_key)
            && self.agent_forget_requests.len() < MAX_SEGMENTED_REVISIONS
            && self
                .agent_forget_requests
                .values()
                .filter(|(r, _)| &r.target.memory.memory_id == target)
                .count()
                < MAX_AGENT_FORGET_REQUESTS_PER_TARGET
            && !self.deleted_memories.contains(target)
            && self
                .memories
                .get(target)
                .and_then(|v| v.last())
                .is_some_and(|m| m.target == report.target)
            && policy.reference() == report.policy
            && policy.active
            && *at >= policy.valid_from
            && *at < policy.valid_until
            && self.source_in_policy(&reference, policy, policy.max_sensitivity)
            && source.record.source_kind == enouia_memory_contract::source::SourceKind::RuntimeEvent
            && runtime_capture::provenance(source).is_some()
            && source.record.speaker_role == enouia_memory_contract::common::SpeakerRole::User
            && source.record.evidence_class
                == enouia_memory_contract::common::EvidenceClass::UserStatement
            && source.text.len() <= 16384
            && report.evidence.resolves(std::slice::from_ref(source))
    }
}

impl SegmentedAutomationVault {
    /// Submit the original request, never a draft. Authorization precedes any
    /// receipt lookup; success means the report is saved, not that a memory
    /// was corrected. A saved key replays after its target is revised.
    pub fn submit_agent_forget(
        &self,
        permit: &AgentForgetPermit,
        request: &AgentForgetRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<AgentForgetReceipt> {
        let admission = self.admit_agent_forget(permit, request, locate)?;
        let snapshot = match &admission {
            ForgetAdmission::Current(draft) => draft.snapshot.clone(),
            ForgetAdmission::Original(snapshot) => snapshot.clone(),
        };
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        permit.live(self)?;
        if state.snapshot()? != snapshot {
            return Err(AutomationFault::Stale);
        }
        let key = agent_forget_key(
            &self.descriptor().vault_id,
            permit.principal(),
            &request.request_id,
        );
        if let Some((old, receipt)) = state.agent_forget_requests.get(&key) {
            return if old.request_hash == digest(request) {
                Ok(receipt.clone())
            } else {
                Err(AutomationFault::Conflict)
            };
        }
        let ForgetAdmission::Current(draft) = admission else {
            return Err(AutomationFault::Stale);
        };
        let draft = *draft;
        if state.agent_forget_requests.len() >= MAX_SEGMENTED_REVISIONS
            || state
                .agent_forget_requests
                .values()
                .filter(|(r, _)| r.target.memory.memory_id == request.target.memory.memory_id)
                .count()
                >= MAX_AGENT_FORGET_REQUESTS_PER_TARGET
        {
            return Err(AutomationFault::Capacity);
        }
        let policy = permit
            .source_policy(&state, &request.source)
            .ok_or(AutomationFault::Denied)?;
        let report = AgentForgetRecord {
            request_key: draft.request_key,
            request_id: draft.request_id,
            request_hash: draft.request_hash,
            principal: draft.principal,
            scope_hash: draft.scope_hash,
            target: draft.target,
            policy,
            evidence: draft.evidence,
            forget: draft.forget,
        };
        self.commit(
            &mut state,
            self.descriptor().system_actor.clone(),
            SegmentedDelta::AgentForgetRequest {
                request: Box::new(report),
            },
        )?;
        Ok(state.agent_forget_requests[&key].1.clone())
    }
}

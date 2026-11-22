//! ADR-MEM-161: Morii's desktop queue of saved Agent correction reports and
//! forget requests. Read-only; acting on a request goes through the existing
//! Owner action plan/confirm/commit, which changes or forgets the target.
use super::*;

#[derive(Clone, Copy, Debug, Eq, PartialEq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentRequestKind {
    Correction,
    Forget,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct AgentRequestView {
    pub kind: AgentRequestKind,
    pub key: Sha256Hex,
    pub request_id: RequestId,
    pub principal: ActorRef,
    pub target: TargetRef,
    /// The quoted user statement the Agent cited (exact captured evidence).
    pub evidence: QuotedEvidence,
    /// The Agent's untrusted interpretation: proposed correction or reason.
    pub text: String,
    pub committed_in: CommitId,
    /// The target is still the exact revision the request named and is not
    /// deleted; false once Morii corrected, forgot or otherwise revised it.
    pub pending: bool,
}

impl SegmentedAutomationVault {
    /// Morii-only. Saved requests whose cited source is within `max` and not
    /// deleted: corrections first, then forget requests, each by request key.
    pub fn agent_requests(
        &self,
        owner: &ActorRef,
        max: Sensitivity,
    ) -> Result<Vec<AgentRequestView>> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let pending = |target: &TargetRef| {
            !state.deleted_memories.contains(&target.memory.memory_id)
                && state
                    .memories
                    .get(&target.memory.memory_id)
                    .and_then(|v| v.last())
                    .is_some_and(|m| &m.target == target)
        };
        let visible = |e: &QuotedEvidence| {
            state
                .sources
                .get(&e.reference.source_id)
                .and_then(|v| v.last())
                .is_some_and(|s| {
                    s.record.sensitivity <= max
                        && !state.deleted_sources.contains(&s.record.source_id)
                })
        };
        let corrections = state
            .agent_corrections
            .values()
            .filter(|(r, _)| visible(&r.evidence))
            .map(|(r, receipt)| AgentRequestView {
                kind: AgentRequestKind::Correction,
                key: r.report_key.clone(),
                request_id: r.request_id.clone(),
                principal: r.principal.clone(),
                target: r.target.clone(),
                evidence: r.evidence.clone(),
                text: r.correction.clone(),
                committed_in: receipt.committed_in.clone(),
                pending: pending(&r.target),
            });
        let forgets = state
            .agent_forget_requests
            .values()
            .filter(|(r, _)| visible(&r.evidence))
            .map(|(r, receipt)| AgentRequestView {
                kind: AgentRequestKind::Forget,
                key: r.request_key.clone(),
                request_id: r.request_id.clone(),
                principal: r.principal.clone(),
                target: r.target.clone(),
                evidence: r.evidence.clone(),
                text: r.forget.clone(),
                committed_in: receipt.committed_in.clone(),
                pending: pending(&r.target),
            });
        Ok(corrections.chain(forgets).collect())
    }
}

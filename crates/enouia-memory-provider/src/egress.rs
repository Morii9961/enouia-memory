//! Latest server-resolved policy/resource snapshot. No client sensitivity flags.
use crate::{Result, error};
use enouia_memory_contract::{
    MemoryErrorCode,
    approval::{ApprovalBinding, ApprovalRecord},
    common::{ActorRef, Sensitivity},
    context::{DispatchRecord, Purpose},
    policy::{
        AccessContext, EgressRule, PolicyRecord, ResourceContext, Scope, egress_rule, evaluate,
    },
    time::Timestamp,
};
use std::collections::BTreeSet;

pub struct EgressSnapshot {
    pub principal: ActorRef,
    pub purpose: Purpose,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub resources: Vec<ResourceContext>,
    pub policies: Vec<PolicyRecord>,
    pub approval: Option<ApprovalRecord>,
    /// Approvals consumed by other dispatches. Supplied by the durable host.
    pub consumed_approvals: BTreeSet<enouia_memory_contract::ids::ApprovalId>,
    pub checked_at: Timestamp,
}
impl EgressSnapshot {
    pub fn authorize(&self, dispatch: &DispatchRecord) -> Result<()> {
        let deny = || error(MemoryErrorCode::PermissionDenied);
        if dispatch.egress.policy_epoch != self.policy_epoch
            || dispatch.egress.deletion_epoch != self.deletion_epoch
        {
            return Err(error(MemoryErrorCode::RevisionConflict));
        }
        let expected = dispatch.resource_refs();
        let actual: BTreeSet<_> = self.resources.iter().map(|r| &r.record).collect();
        if actual != expected || actual.len() != self.resources.len() || actual.is_empty() {
            return Err(deny());
        }
        let id = dispatch.egress.egress_policy_id.as_ref().ok_or_else(deny)?;
        // One exact policy must cover all carried resources, including input.
        let policies: Vec<_> = self
            .policies
            .iter()
            .filter(|p| &p.policy_id == id)
            .collect();
        let mut private = false;
        for resource in &self.resources {
            let mut violations = vec![];
            resource.record.validate("/resource", &mut violations);
            if !violations.is_empty() {
                return Err(deny());
            }
            if egress_rule(resource.sensitivity, dispatch.destination.kind) == EgressRule::Denied {
                return Err(deny());
            }
            if !evaluate(
                &policies,
                &AccessContext {
                    principal: &self.principal,
                    scope: Scope::ProviderSend,
                    purpose: Some(self.purpose),
                    destination: Some(&dispatch.destination),
                    resource,
                },
                &self.checked_at,
            )
            .is_allow()
            {
                return Err(deny());
            }
            private |= resource.sensitivity == Sensitivity::Private;
        }
        if !private && dispatch.egress.egress_approval_id.is_none() {
            return Ok(());
        }
        let approval = self.approval.as_ref().ok_or_else(deny)?;
        if !approval.validate().is_empty()
            || !approval.valid_at(&self.checked_at)
            || approval.approved_by != self.principal
            || dispatch.egress.egress_approval_id.as_ref() != Some(&approval.approval_id)
            || self.consumed_approvals.contains(&approval.approval_id)
        {
            return Err(deny());
        }
        let ApprovalBinding::Egress {
            request_id,
            capsule_id,
            payload_hash,
            destination,
            resources,
            policy_id,
            policy_epoch,
        } = &approval.binding
        else {
            return Err(deny());
        };
        let approved: BTreeSet<_> = resources.iter().collect();
        if request_id != &dispatch.request_id
            || capsule_id != &dispatch.capsule_id
            || payload_hash != &dispatch.request_hash
            || destination != &dispatch.destination
            || policy_id != id
            || *policy_epoch != self.policy_epoch
            || approved != expected
        {
            return Err(deny());
        }
        Ok(())
    }
}

/// Called immediately before admission and during streaming. Implementations
/// resolve freshness, lock state and policies from the owning Vault, then
/// durably audit an allowed send; errors stop HTTP without disclosing content.
pub trait SendGuard {
    fn check(&self, dispatch: &DispatchRecord, audit: bool) -> Result<()>;
}

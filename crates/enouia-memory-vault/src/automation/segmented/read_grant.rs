//! Explicit native Owner management; no serialized value authenticates an Owner.
use super::*;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerReadGrantPlan {
    pub snapshot: SnapshotBinding,
    pub prepared_at: Timestamp,
    pub expires_at: Timestamp,
    pub grant: OwnerReadGrant,
}
#[derive(serde::Serialize)]
pub struct OwnerReadStatus {
    pub snapshot: SnapshotBinding,
    pub now: Timestamp,
    pub policies: Vec<AutomationPolicy>,
    pub policy_refs: Vec<PolicyRef>,
    pub grants: Vec<OwnerReadGrant>,
}

impl State {
    pub(super) fn check_read_grant(
        &self,
        descriptor: &AutomationDescriptor,
        grant: &OwnerReadGrant,
    ) -> Result<()> {
        if descriptor.layout != AutomationLayout::SegmentedOwnerV1
            || !grant.valid()
            || grant.vault_id != descriptor.vault_id
            || self.policies.contains_key(&grant.grant_id)
            || grant.revision.get()
                != self
                    .read_grants
                    .get(&grant.grant_id)
                    .map_or(1, |g| g.revision.get() + 1)
            || (!self.read_grants.contains_key(&grant.grant_id)
                && self.read_grants.len() >= MAX_SEGMENTED_POLICIES)
            || grant.policies.iter().any(|r| {
                self.policy_history.get(&r.hash).is_none_or(|p| {
                    p.reference() != *r
                        || p.subject_id != grant.subject_id
                        || p.project_id != grant.project_id
                        || grant.max_sensitivity > p.max_sensitivity
                })
            })
            || grant.source_ids.iter().any(|id| {
                !grant
                    .policies
                    .iter()
                    .any(|r| self.policy_history[&r.hash].source_ids.contains(id))
                    || (grant.active
                        && (self.deleted_sources.contains(id) || !self.sources.contains_key(id)))
            })
        {
            return Err(AutomationFault::Denied);
        }
        Ok(())
    }
}

impl SegmentedAutomationVault {
    pub fn owner_read_grant(&self, owner: &ActorRef, id: &PolicyId) -> Result<OwnerReadGrant> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        state
            .read_grants
            .get(id)
            .cloned()
            .ok_or(AutomationFault::Denied)
    }
    pub fn owner_read_status(&self, owner: &ActorRef) -> Result<OwnerReadStatus> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if self.descriptor().layout != AutomationLayout::SegmentedOwnerV1 {
            return Err(AutomationFault::Denied);
        }
        Ok(OwnerReadStatus {
            snapshot: state.snapshot()?,
            now: self.native.now()?,
            policies: state.policy_history.values().cloned().collect(),
            policy_refs: state
                .policy_history
                .values()
                .map(AutomationPolicy::reference)
                .collect(),
            grants: state.read_grants.values().cloned().collect(),
        })
    }
    pub fn plan_owner_read_grant(
        &self,
        owner: &ActorRef,
        grant: OwnerReadGrant,
    ) -> Result<OwnerReadGrantPlan> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        state.check_read_grant(self.descriptor(), &grant)?;
        let now = self.native.now()?;
        if grant.active && now >= grant.valid_until {
            return Err(AutomationFault::Denied);
        }
        Ok(OwnerReadGrantPlan {
            snapshot: state.snapshot()?,
            expires_at: Timestamp::from_unix_ms(now.unix_ms() + 300_000)
                .ok_or(AutomationFault::Denied)?,
            prepared_at: now,
            grant,
        })
    }
    /// The native caller authenticates the Owner and confirms the reviewed plan.
    /// On unknown publication recover first, then inspect this exact grant revision.
    pub fn commit_owner_read_grant(
        &self,
        owner: &ActorRef,
        plan: &OwnerReadGrantPlan,
    ) -> Result<AutomationCurrent> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let mut state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if state.read_grants.get(&plan.grant.grant_id) == Some(&plan.grant) {
            return Ok(state.head.as_ref().unwrap().current());
        }
        let now = self.native.now()?;
        if plan.snapshot != state.snapshot()?
            || now < plan.prepared_at
            || now >= plan.expires_at
            || plan.expires_at.unix_ms() - plan.prepared_at.unix_ms() != 300_000
            || (plan.grant.active && now >= plan.grant.valid_until)
        {
            return Err(AutomationFault::Stale);
        }
        self.commit(
            &mut state,
            owner.clone(),
            SegmentedDelta::OwnerReadGrant {
                grant: Box::new(plan.grant.clone()),
            },
        )
    }
}

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
/// ADR-MEM-168: what an Owner read grant does not cover yet in its own
/// subject and project, and the next revision that would cover it.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct OwnerReadGrantCoverage {
    pub snapshot: SnapshotBinding,
    pub grant: OwnerReadGrant,
    /// Current revisions of the subject's and project's policies (capture
    /// windows included) that the grant does not list.
    pub uncovered_policies: Vec<PolicyRef>,
    /// Captured, undeleted sources of the listed and uncovered policies that
    /// the grant does not list.
    pub uncovered_sources: Vec<SourceId>,
    /// `revision + 1` listing all of them, otherwise unchanged and already
    /// checked like a plan. Absent when nothing is uncovered, the grant is
    /// revoked or it would exceed the grant limits. Plan and confirm it to apply.
    pub refreshed: Option<OwnerReadGrant>,
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
    /// ADR-MEM-160: an active request-capable grant of `principal` covers this
    /// exact source and policy revision at `at`. Grants are on the same chain,
    /// so replay sees exactly the grants that existed at that commit.
    pub(super) fn request_grant_covers(
        &self,
        principal: &ActorRef,
        source: &enouia_memory_contract::common::SourceRevisionRef,
        policy: &AutomationPolicy,
        sensitivity: Sensitivity,
        at: &Timestamp,
    ) -> bool {
        let reference = policy.reference();
        self.read_grants.values().any(|g| {
            g.active
                && g.allow_requests
                && &g.principal == principal
                && *at >= g.valid_from
                && *at < g.valid_until
                && sensitivity <= g.max_sensitivity
                && g.source_ids.contains(&source.source_id)
                && g.policies.contains(&reference)
        })
    }
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
    /// ADR-MEM-168: read-only; writes nothing and authorizes nothing.
    pub fn owner_read_grant_coverage(
        &self,
        owner: &ActorRef,
        id: &PolicyId,
    ) -> Result<OwnerReadGrantCoverage> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let grant = state
            .read_grants
            .get(id)
            .cloned()
            .ok_or(AutomationFault::Denied)?;
        let listed: BTreeSet<_> = grant.policies.iter().map(|r| &r.hash).collect();
        let mut uncovered_policies: Vec<PolicyRef> = state
            .policies
            .values()
            .filter(|p| p.subject_id == grant.subject_id && p.project_id == grant.project_id)
            .map(AutomationPolicy::reference)
            .filter(|r| !listed.contains(&r.hash))
            .collect();
        uncovered_policies.sort_by(|a, b| (&a.policy_id, &a.hash).cmp(&(&b.policy_id, &b.hash)));
        let policies: Vec<PolicyRef> = grant
            .policies
            .iter()
            .chain(&uncovered_policies)
            .cloned()
            .collect();
        let sources: BTreeSet<SourceId> = policies
            .iter()
            .flat_map(|r| &state.policy_history[&r.hash].source_ids)
            .filter(|id| state.sources.contains_key(*id) && !state.deleted_sources.contains(*id))
            .cloned()
            .collect();
        let uncovered_sources: Vec<SourceId> = sources
            .iter()
            .filter(|id| !grant.source_ids.contains(id))
            .cloned()
            .collect();
        let refreshed = (grant.active
            && !(uncovered_policies.is_empty() && uncovered_sources.is_empty()))
        .then(|| {
            let mut next = grant.clone();
            next.revision = Revision::new(grant.revision.get() + 1)?;
            next.policies = policies;
            next.source_ids = sources.into_iter().collect();
            state
                .check_read_grant(self.descriptor(), &next)
                .is_ok()
                .then_some(next)
        })
        .flatten();
        Ok(OwnerReadGrantCoverage {
            snapshot: state.snapshot()?,
            grant,
            uncovered_policies,
            uncovered_sources,
            refreshed,
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

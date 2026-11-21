//! ADR-MEM-156: Morii-opened capture windows for Runtime capture on an Owner
//! root. A window is an ordinary formation policy with fresh source IDs.
use super::*;

pub const MAX_CAPTURE_WINDOW_DAYS: u32 = 90;

#[derive(Clone, Debug, serde::Serialize)]
pub struct CaptureWindow {
    pub policy: AutomationPolicy,
    /// Reserved source IDs not yet captured or deleted, in policy order.
    pub unused_source_ids: Vec<SourceId>,
}

impl SegmentedAutomationVault {
    /// Morii-only. Commits a new formation policy for Runtime capture: Morii's
    /// subject, no project, no revision targets, `MAX_SOURCES` fresh source IDs
    /// and fresh access/egress IDs, valid from now for `days`.
    pub fn open_capture_window(
        &self,
        owner: &ActorRef,
        days: u32,
        max_sensitivity: Sensitivity,
        allowed_forms: Vec<FormationKind>,
    ) -> Result<CaptureWindow> {
        let d = &self.native.descriptor;
        let Some(subject_id) = d.owner_subject_id.clone() else {
            return Err(AutomationFault::Denied);
        };
        if d.layout != AutomationLayout::SegmentedOwnerV1
            || !(1..=MAX_CAPTURE_WINDOW_DAYS).contains(&days)
            || allowed_forms.is_empty()
        {
            return Err(AutomationFault::Denied);
        }
        let now = self.native.now()?;
        let valid_until = Timestamp::from_unix_ms(now.unix_ms() + i64::from(days) * 86_400_000)
            .ok_or(AutomationFault::Capacity)?;
        let ids = &self.native.ids;
        let policy = AutomationPolicy {
            schema_version: NextVersion,
            kind: PolicyKind::AutomationPolicy,
            policy_id: PolicyId::from_random(ids.random_16()),
            revision: Revision::new(1).unwrap(),
            rule_version: RuleVersion::V3,
            system_actor: d.system_actor.clone(),
            vault_id: d.vault_id.clone(),
            subject_id,
            project_id: None,
            source_ids: (0..MAX_SOURCES)
                .map(|_| SourceId::from_random(ids.random_16()))
                .collect(),
            allow_create: true,
            allow_revise_created: false,
            target_ids: Vec::new(),
            allowed_forms,
            max_sensitivity,
            access_policy_id: PolicyId::from_random(ids.random_16()),
            egress_policy_id: PolicyId::from_random(ids.random_16()),
            active: true,
            valid_from: now,
            valid_until,
        };
        self.set_policy(owner, policy.clone())?;
        Ok(CaptureWindow {
            unused_source_ids: policy.source_ids.clone(),
            policy,
        })
    }
    /// Morii-only. Open capture windows that still have reserved source IDs.
    pub fn capture_windows(&self, owner: &ActorRef) -> Result<Vec<CaptureWindow>> {
        self.native.owner(owner)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = self.native.now()?;
        let subject = self.native.descriptor.owner_subject_id.as_ref();
        Ok(state
            .policies
            .values()
            .filter(|p| {
                p.active
                    && now >= p.valid_from
                    && now < p.valid_until
                    && Some(&p.subject_id) == subject
                    && p.project_id.is_none()
                    && p.allow_create
                    && !p.allow_revise_created
                    && p.target_ids.is_empty()
            })
            .filter_map(|p| {
                let unused: Vec<_> = p
                    .source_ids
                    .iter()
                    .filter(|id| {
                        !state.sources.contains_key(*id) && !state.deleted_sources.contains(*id)
                    })
                    .cloned()
                    .collect();
                (!unused.is_empty()).then(|| CaptureWindow {
                    policy: p.clone(),
                    unused_source_ids: unused,
                })
            })
            .collect())
    }
}

//! Planned owner actions only. G08 supplies authentication and publication.
use super::*;
use crate::{common::*, error::Violation, hash::Sha256Hex, ids::*, time::Timestamp};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttentionAnnotation {
    Protect,
    Pin,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum OwnerAction {
    Correct {
        target: TargetRef,
        source_id: SourceId,
        assertion_text: String,
    },
    Withdraw {
        target: TargetRef,
        #[serde(deserialize_with = "crate::json::nullable")]
        reason: Option<String>,
    },
    Forget {
        target: TargetRef,
        impact_preview_hash: Sha256Hex,
    },
    Attention {
        target: TargetRef,
        annotation: AttentionAnnotation,
        #[serde(deserialize_with = "crate::json::nullable")]
        until: Option<Timestamp>,
    },
    ThemeRename {
        theme: TargetRef,
        name: String,
        members: Vec<TargetRef>,
    },
    ThemePin {
        theme: TargetRef,
        members: Vec<TargetRef>,
    },
    ThemeDissolve {
        theme: TargetRef,
        members: Vec<TargetRef>,
    },
    EntityMerge {
        from_entity: String,
        into_entity: String,
        #[serde(deserialize_with = "crate::json::nullable")]
        reason: Option<String>,
    },
    EntitySplit {
        from_entity: String,
        into_entity: String,
        evidence: Vec<QuotedEvidence>,
        #[serde(deserialize_with = "crate::json::nullable")]
        reason: Option<String>,
    },
    Tag {
        name: String,
        targets: Vec<TargetRef>,
    },
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OwnerPlanKind {
    #[serde(rename = "owner_action_plan")]
    OwnerActionPlan,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerActionPlan {
    pub schema_version: NextVersion,
    pub kind: OwnerPlanKind,
    pub operation_id: OperationId,
    pub snapshot: SnapshotBinding,
    pub owner: ActorRef,
    pub planned_at: Timestamp,
    pub expires_at: Timestamp,
    pub action: OwnerAction,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerActionConfirmation {
    pub operation_id: OperationId,
    pub plan_hash: Sha256Hex,
    pub owner: ActorRef,
    pub surface: TrustedSurface,
    pub confirmed_at: Timestamp,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OwnerRecordKind {
    #[serde(rename = "owner_action_record")]
    OwnerActionRecord,
}
/// Prospective committed shape, not a witnessed receipt when decoded from JSON.
/// No machine publication transaction accepts it in this slice.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerActionRecord {
    pub schema_version: NextVersion,
    pub kind: OwnerRecordKind,
    pub plan: OwnerActionPlan,
    pub confirmation: OwnerActionConfirmation,
    pub committed_in: CommitId,
    pub sequence: crate::json::Revision,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerActionLedger {
    pub withdrawal_watermark: crate::json::Revision,
    pub records: Vec<OwnerActionRecord>,
}
impl Default for OwnerActionLedger {
    fn default() -> Self {
        Self {
            withdrawal_watermark: crate::json::Revision::new(1).unwrap(),
            records: vec![],
        }
    }
}
impl OwnerActionLedger {
    pub fn validate(&self) -> Vec<Violation> {
        let mut errors = Vec::new();
        if self.records.len() > 128
            || self.withdrawal_watermark.get()
                != 1 + self
                    .records
                    .iter()
                    .filter(|r| matches!(r.plan.action, OwnerAction::Withdraw { .. }))
                    .count() as u64
            || self
                .records
                .iter()
                .map(|r| &r.plan.operation_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                != self.records.len()
        {
            errors.push(Violation::new("ui.owner_ledger", "/owner_actions"));
        }
        for record in &self.records {
            errors.extend(record.validate());
        }
        errors
    }
}
impl NextDocument for OwnerActionRecord {
    fn validate(&self) -> Vec<Violation> {
        if validate_owner_action_binding(
            &self.plan,
            &self.confirmation,
            &self.plan.owner,
            &self.plan.snapshot,
            &self.confirmation.confirmed_at,
        )
        .is_err()
        {
            vec![Violation::new("ui.owner_record_binding", "")]
        } else {
            vec![]
        }
    }
}
impl NextDocument for OwnerActionPlan {
    fn validate(&self) -> Vec<Violation> {
        let entities = |a: &str, b: &str| {
            a != b && check_prefixed_uuid(a, "ent").is_ok() && check_prefixed_uuid(b, "ent").is_ok()
        };
        let reason = |r: &Option<String>| r.as_ref().is_none_or(|s| record_fields::text(s, 500));
        let targets = |t: &[TargetRef]| {
            !t.is_empty()
                && t.len() <= 128
                && t.iter()
                    .map(|r| &r.memory.memory_id)
                    .collect::<std::collections::BTreeSet<_>>()
                    .len()
                    == t.len()
        };
        let valid = self.owner.actor_type == ActorType::Owner
            && self.planned_at < self.expires_at
            && match &self.action {
                OwnerAction::Correct { assertion_text, .. } => {
                    !assertion_text.trim().is_empty() && assertion_text.len() <= 16 * 1024
                }
                OwnerAction::Withdraw { reason: r, .. } => reason(r),
                OwnerAction::Forget { .. } => true,
                OwnerAction::Attention { until, .. } => {
                    until.as_ref().is_none_or(|t| t > &self.planned_at)
                }
                OwnerAction::ThemeRename { name, members, .. } => {
                    record_fields::text(name, 60) && targets(members)
                }
                OwnerAction::ThemePin { members, .. }
                | OwnerAction::ThemeDissolve { members, .. } => targets(members),
                OwnerAction::EntityMerge {
                    from_entity,
                    into_entity,
                    reason: r,
                } => entities(from_entity, into_entity) && reason(r),
                OwnerAction::EntitySplit {
                    from_entity,
                    into_entity,
                    evidence,
                    reason: r,
                } => {
                    entities(from_entity, into_entity)
                        && reason(r)
                        && !evidence.is_empty()
                        && evidence.len() <= 8
                        && evidence.iter().all(QuotedEvidence::valid_shape)
                }
                OwnerAction::Tag { name, targets: t } => {
                    record_fields::text(name, 64) && targets(t)
                }
            };
        if valid {
            vec![]
        } else {
            vec![Violation::new("ui.owner_plan", "/action")]
        }
    }
}
/// Native identity/snapshot are trusted observations, never page/model JSON.
/// This checks a prospective binding and produces no record/receipt/authority.
pub fn validate_owner_action_binding(
    plan: &OwnerActionPlan,
    confirmation: &OwnerActionConfirmation,
    native_owner: &ActorRef,
    current: &SnapshotBinding,
    now: &Timestamp,
) -> Result<(), crate::ContractError> {
    super::checked(plan.validate())?;
    if native_owner.actor_type != ActorType::Owner
        || &plan.owner != native_owner
        || confirmation.owner != *native_owner
        || &plan.snapshot != current
        || confirmation.operation_id != plan.operation_id
        || confirmation.plan_hash != digest(plan)
        || confirmation.confirmed_at < plan.planned_at
        || confirmation.confirmed_at > *now
        || *now >= plan.expires_at
    {
        return Err(super::invalid("ui.owner_confirmation", ""));
    }
    Ok(())
}

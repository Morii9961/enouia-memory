//! Native owner execution shapes. Decoding these does not create consent.
use super::*;
use crate::{common::*, error::Violation, hash::Sha256Hex, ids::*};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerEntityMention {
    pub entity_id: String,
    pub role: EntityRole,
}
/// Proposed System metadata, never owner verification of the described facts.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerCorrectionMetadata {
    pub title: String,
    pub content_lang: String,
    pub sensitivity: Sensitivity,
    pub interpretation: Interpretation,
    pub dimensions: FormationDimensions,
    pub event_time: RecordTime,
    pub valid_time: RecordTime,
    pub entities: Vec<OwnerEntityMention>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerCorrection {
    pub policy: AutomationPolicy,
    pub source: CapturedSource,
    pub decision: DecisionDraft,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerActionEffects {
    pub scope_hash: Sha256Hex,
    pub max_sensitivity: Sensitivity,
    pub sensitivity: Sensitivity,
    pub family_members: Vec<TargetRef>,
    pub correction: Option<OwnerCorrection>,
}
impl OwnerActionEffects {
    pub fn valid(&self) -> bool {
        self.sensitivity <= self.max_sensitivity
            && self.family_members.len() <= MAX_SEGMENTED_REVISIONS
            && self
                .family_members
                .windows(2)
                .all(|w| w[0].memory.memory_id < w[1].memory.memory_id)
            && self.correction.as_ref().is_none_or(|c| {
                c.policy.validate().is_empty()
                    && c.source.record.validate().is_empty()
                    && c.decision.validate().is_empty()
            })
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeOwnerOutcome {
    pub schema_version: NextVersion,
    pub kind: String,
    pub operation_id: OperationId,
    pub plan_hash: Sha256Hex,
    pub committed_in: CommitId,
    pub actor: ActorRef,
    pub action: String,
    pub memory_result: Option<MemoryRevisionRef>,
    pub physical_purge: bool,
}
impl NextDocument for NativeOwnerOutcome {
    fn validate(&self) -> Vec<Violation> {
        if self.kind == "native_owner_outcome"
            && self.actor.actor_type == ActorType::Owner
            && !self.physical_purge
            && [
                "correct",
                "withdraw",
                "forget",
                "attention",
                "theme_rename",
                "theme_pin",
                "theme_dissolve",
                "entity_merge",
                "entity_split",
                "tag",
            ]
            .contains(&self.action.as_str())
        {
            vec![]
        } else {
            vec![Violation::new("owner.outcome", "")]
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerActionProof {
    pub operation_id: OperationId,
    pub committed_in: CommitId,
    pub owner: ActorRef,
    pub confirmed_at: crate::time::Timestamp,
    pub action: String,
    pub entities: Vec<String>,
}
impl OwnerActionProof {
    pub fn from_record(r: &OwnerActionRecord) -> Self {
        let (action, entities) = match &r.plan.action {
            OwnerAction::Correct { .. } => ("correct", vec![]),
            OwnerAction::Withdraw { .. } => ("withdraw", vec![]),
            OwnerAction::Forget { .. } => ("forget", vec![]),
            OwnerAction::Attention { .. } => ("attention", vec![]),
            OwnerAction::ThemeRename { .. } => ("theme_rename", vec![]),
            OwnerAction::ThemePin { .. } => ("theme_pin", vec![]),
            OwnerAction::ThemeDissolve { .. } => ("theme_dissolve", vec![]),
            OwnerAction::EntityMerge {
                from_entity,
                into_entity,
                ..
            } => (
                "entity_merge",
                vec![from_entity.clone(), into_entity.clone()],
            ),
            OwnerAction::EntitySplit {
                from_entity,
                into_entity,
                ..
            } => (
                "entity_split",
                vec![from_entity.clone(), into_entity.clone()],
            ),
            OwnerAction::Tag { .. } => ("tag", vec![]),
        };
        Self {
            operation_id: r.plan.operation_id.clone(),
            committed_in: r.committed_in.clone(),
            owner: r.plan.owner.clone(),
            confirmed_at: r.confirmation.confirmed_at.clone(),
            action: action.into(),
            entities,
        }
    }
    pub fn valid(&self) -> bool {
        self.owner.actor_type == ActorType::Owner
            && self.entities.len() <= 2
            && self
                .entities
                .iter()
                .all(|id| crate::ids::check_prefixed_uuid(id, "ent").is_ok())
            && [
                "correct",
                "withdraw",
                "forget",
                "attention",
                "theme_rename",
                "theme_pin",
                "theme_dissolve",
                "entity_merge",
                "entity_split",
                "tag",
            ]
            .contains(&self.action.as_str())
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerAttentionMark {
    pub annotation: AttentionAnnotation,
    pub until: Option<crate::time::Timestamp>,
    pub proof: OwnerActionProof,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerTagMark {
    pub name: String,
    pub proof: OwnerActionProof,
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OwnerMemoryMarks {
    pub corrected: Option<OwnerActionProof>,
    pub attention: Option<OwnerAttentionMark>,
    pub tags: Vec<OwnerTagMark>,
    pub tag_count_in_scope: u64,
    pub tags_complete: bool,
}
impl OwnerMemoryMarks {
    pub fn empty() -> Self {
        Self {
            tags_complete: true,
            ..Self::default()
        }
    }
    pub fn valid(&self) -> bool {
        self.corrected
            .as_ref()
            .is_none_or(|p| p.valid() && p.action == "correct")
            && self
                .attention
                .as_ref()
                .is_none_or(|m| m.proof.valid() && m.proof.action == "attention")
            && self.tags.len() <= 64
            && self.tags.len() as u64 <= self.tag_count_in_scope
            && self.tags_complete == (self.tags.len() as u64 == self.tag_count_in_scope)
            && self.tags.iter().all(|t| {
                record_fields::text(&t.name, 64) && t.proof.valid() && t.proof.action == "tag"
            })
            && self.tags.windows(2).all(|w| w[0].name < w[1].name)
    }
}

impl NextDocument for OwnerActionEffects {
    fn validate(&self) -> Vec<Violation> {
        if self.valid() {
            vec![]
        } else {
            vec![Violation::new("owner.effects", "")]
        }
    }
}
impl NextDocument for OwnerCorrectionMetadata {
    fn validate(&self) -> Vec<Violation> {
        if record_fields::text(&self.title, 60)
            && valid_content_lang(&self.content_lang)
            && self.event_time.valid()
            && self.valid_time.valid()
            && self.dimensions.validate().is_empty()
            && self.entities.len() <= 32
            && self
                .entities
                .iter()
                .all(|e| crate::ids::check_prefixed_uuid(&e.entity_id, "ent").is_ok())
        {
            vec![]
        } else {
            vec![Violation::new("owner.metadata", "")]
        }
    }
}
impl NextDocument for OwnerActionProof {
    fn validate(&self) -> Vec<Violation> {
        if self.valid() {
            vec![]
        } else {
            vec![Violation::new("owner.proof", "")]
        }
    }
}

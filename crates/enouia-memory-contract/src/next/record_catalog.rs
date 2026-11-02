//! Revisioned entity/conversation metadata stored in the same immutable commit.
use super::*;
use crate::{common::*, error::Violation, ids::*, json::Revision, time::Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    Person,
    Project,
    Place,
    Organization,
    Thing,
    Pet,
    Work,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityReality {
    Real,
    Fictional,
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityAlias {
    pub name: String,
    pub evidence: Vec<QuotedEvidence>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct EntityRecord {
    pub entity_id: String,
    pub revision: Revision,
    pub entity_type: EntityType,
    pub reality: EntityReality,
    pub display_name: String,
    pub aliases: Vec<EntityAlias>,
    pub evidence: Vec<QuotedEvidence>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub project_id: Option<ProjectId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub first_seen: Option<Timestamp>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub last_seen: Option<Timestamp>,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationParticipant {
    pub label: String,
    pub speaker: SpeakerRole,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub entity_id: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationTurn {
    pub source: SourceRevisionRef,
    pub branch_id: BranchId,
    pub turn_id: TurnId,
    pub sequence: u32,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent_source: Option<SourceRevisionRef>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationRecord {
    pub conversation_id: SessionId,
    pub revision: Revision,
    pub platform: String,
    pub account_scope: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub original_conversation_id: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub title: Option<String>,
    pub participants: Vec<ConversationParticipant>,
    pub started_at: RecordTime,
    pub ended_at: RecordTime,
    pub turns: Vec<ConversationTurn>,
    /// Number of distinct turns in this captured revision, not platform total.
    pub turn_count: u32,
    pub branches: Vec<BranchId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub import_batch_id: Option<ImportId>,
    pub complete_in_scope: bool,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourceRecordFields {
    pub content_lang: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub assistant_model: Option<String>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub conversation_id: Option<SessionId>,
}
impl SourceRecordFields {
    pub fn valid(&self, source: &CapturedSource) -> bool {
        valid_content_lang(&self.content_lang)
            && (source.record.original_conversation_id.is_none() || self.conversation_id.is_some())
            && self.assistant_model.as_ref().is_none_or(|m| {
                record_fields::text(m, 128) && source.record.speaker_role == SpeakerRole::Assistant
            })
    }
}
#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecordCatalog {
    pub entities: Vec<EntityRecord>,
    pub conversations: Vec<ConversationRecord>,
}
impl RecordCatalog {
    pub fn is_empty(&self) -> bool {
        self.entities.is_empty() && self.conversations.is_empty()
    }
    pub fn extends(&self, before: &Self) -> bool {
        self.entities.starts_with(&before.entities)
            && self.conversations.starts_with(&before.conversations)
    }
    pub fn validate(&self, sources: &[CapturedSource]) -> Vec<Violation> {
        let quote = |q: &QuotedEvidence, sensitivity: Sensitivity, policy: &PolicyId| {
            q.resolves(sources)
                && sources.iter().any(|s| {
                    s.record.source_id == q.reference.source_id
                        && s.record.revision == q.reference.source_revision
                        && s.record.sensitivity <= sensitivity
                        && &s.record.access_policy_id == policy
                })
        };
        let mut entities = std::collections::BTreeMap::new();
        let mut valid = self.entities.len() <= 128 && self.conversations.len() <= 128;
        for e in &self.entities {
            let revision = entities.insert(&e.entity_id, e.revision.get()).unwrap_or(0);
            valid &= e.revision.get() == revision + 1
                && check_prefixed_uuid(&e.entity_id, "ent").is_ok()
                && record_fields::text(&e.display_name, 128)
                && e.aliases.len() <= 32
                && (e.entity_type == EntityType::Project) == e.project_id.is_some()
                && e.first_seen
                    .as_ref()
                    .zip(e.last_seen.as_ref())
                    .is_none_or(|(a, b)| a <= b)
                && !e.evidence.is_empty()
                && e.evidence.len() <= 8
                && e.evidence
                    .iter()
                    .all(|q| quote(q, e.sensitivity, &e.access_policy_id))
                && e.aliases.iter().all(|a| {
                    record_fields::text(&a.name, 128)
                        && !a.evidence.is_empty()
                        && a.evidence.len() <= 8
                        && a.evidence
                            .iter()
                            .all(|q| quote(q, e.sensitivity, &e.access_policy_id))
                });
        }
        let mut conversations = std::collections::BTreeMap::new();
        for c in &self.conversations {
            let revision = conversations
                .insert(&c.conversation_id, c.revision.get())
                .unwrap_or(0);
            valid &= c.revision.get() == revision + 1
                && record_fields::text(&c.platform, 64)
                && crate::source::is_account_alias(&c.account_scope)
                && c.original_conversation_id
                    .as_ref()
                    .is_none_or(|s| record_fields::text(s, 256))
                && c.title.as_ref().is_none_or(|s| record_fields::text(s, 256))
                && !c.participants.is_empty()
                && c.participants.len() <= 32
                && c.participants.iter().all(|p| {
                    record_fields::text(&p.label, 128)
                        && p.entity_id.as_ref().is_none_or(|id| {
                            self.entities.iter().any(|e| {
                                &e.entity_id == id
                                    && e.sensitivity <= c.sensitivity
                                    && e.access_policy_id == c.access_policy_id
                            })
                        })
                })
                && c.started_at.valid()
                && c.ended_at.valid()
                && c.started_at
                    .from
                    .as_ref()
                    .zip(c.ended_at.from.as_ref())
                    .is_none_or(|(a, b)| a <= b)
                && c.turns.len() <= 128
                && !c.branches.is_empty()
                && c.branches.len() <= 32
                && c.branches.iter().collect::<BTreeSet<_>>().len() == c.branches.len()
                && c.turns
                    .iter()
                    .map(|t| &t.turn_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == c.turn_count as usize
                && c.turns
                    .iter()
                    .map(|t| &t.source)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == c.turns.len()
                && c.turns
                    .iter()
                    .map(|t| (&t.branch_id, t.sequence))
                    .collect::<BTreeSet<_>>()
                    .len()
                    == c.turns.len()
                && c.turns.iter().all(|t| {
                    t.sequence > 0
                        && c.branches.contains(&t.branch_id)
                        && t.parent_source.as_ref().is_none_or(|p| {
                            c.turns
                                .iter()
                                .any(|a| &a.source == p && a.sequence < t.sequence)
                        })
                        && sources.iter().any(|s| {
                            s.reference() == t.source
                                && s.record.sensitivity <= c.sensitivity
                                && s.record.access_policy_id == c.access_policy_id
                                && s.record_fields
                                    .as_ref()
                                    .and_then(|f| f.conversation_id.as_ref())
                                    == Some(&c.conversation_id)
                                && s.record.original_conversation_id.as_ref().is_none_or(|id| {
                                    Some(id) == c.original_conversation_id.as_ref()
                                })
                                && match &s.record.branch_id {
                                    crate::json::Knowable::Known(Some(b)) => b == &t.branch_id,
                                    _ => true,
                                }
                        })
                });
        }
        valid &= sources.iter().all(|s| {
            s.record_fields.as_ref().is_some_and(|f| {
                f.valid(s)
                    && f.conversation_id.as_ref().is_none_or(|id| {
                        self.conversations.iter().any(|c| {
                            &c.conversation_id == id
                                && c.turns.iter().any(|t| t.source == s.reference())
                        })
                    })
            })
        });
        if valid {
            vec![]
        } else {
            vec![Violation::new("ui.catalog_closure", "/record_catalog")]
        }
    }
}

//! Scoped native process/detail DTOs. Parsing never grants access or proves a publication.
use super::*;
use crate::{
    common::*, error::Violation, hash::Sha256Hex, ids::*, json::Revision, time::Timestamp,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeReadReceipt {
    pub job_key: Sha256Hex,
    pub binding_hash: Sha256Hex,
    pub operation_id: OperationId,
    pub request_hash: Sha256Hex,
    pub response_hash: Sha256Hex,
    pub decision_hash: Sha256Hex,
    pub committed_in: CommitId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<Revision>,
    pub result: MemoryRevisionRef,
}
impl From<&MemoryReceipt> for NativeReadReceipt {
    fn from(r: &MemoryReceipt) -> Self {
        Self {
            job_key: r.job_key.clone(),
            binding_hash: r.binding_hash.clone(),
            operation_id: r.operation_id.clone(),
            request_hash: r.request_hash.clone(),
            response_hash: r.response_hash.clone(),
            decision_hash: r.decision_hash.clone(),
            committed_in: r.committed_in.clone(),
            sequence: None,
            result: r.result.clone(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryFormationReason {
    pub interpretation: Interpretation,
    pub rule_version: RuleVersion,
    pub prompt_version: String,
    pub model_version: Option<String>,
    pub mode: FormationMode,
    pub import_batch_id: Option<ImportId>,
    pub note: Option<String>,
    pub note_is_machine_annotation: bool,
}
impl MemoryFormationReason {
    pub fn valid(&self) -> bool {
        record_fields::text(&self.prompt_version, 128)
            && self.note_is_machine_annotation
            && self
                .model_version
                .as_ref()
                .is_none_or(|s| record_fields::text(s, 128))
            && self
                .note
                .as_ref()
                .is_none_or(|s| record_fields::text(s, 500))
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessHeader {
    pub selection: CatalogSelection,
    pub algorithm: String,
    pub version: Revision,
    pub parameters_hash: Sha256Hex,
    pub permission_hash: Sha256Hex,
    pub authorized_at: Timestamp,
    pub scope: String,
}
impl ProcessHeader {
    pub fn valid(&self, algorithm: &str) -> bool {
        self.selection.valid()
            && match self.selection.algorithm {
                CatalogReadAlgorithm::V3 => self.version.get() == 1 && self.algorithm == algorithm,
                CatalogReadAlgorithm::V4 => {
                    self.version.get() == 2
                        && self.algorithm
                            == format!("{}-2", algorithm.strip_suffix("-1").unwrap_or(algorithm))
                }
                _ => false,
            }
            && self.selection.evaluated_at <= self.authorized_at
            && self.scope == "authorized_current_dependencies"
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NativeActivityKind {
    MaterialCaptured,
    Formation,
    Reevaluation,
    Synthesis,
    Correction,
    Reversion,
    StateChange,
    OwnerAction,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityQuery {
    pub memory: Option<MemoryId>,
    pub event_kind: Option<NativeActivityKind>,
    pub include_sources: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativePublicationProof {
    pub commit_id: CommitId,
    pub commit_hash: Sha256Hex,
    pub actor: ActorRef,
    pub written_at: Timestamp,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryActivityEvent {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_action: Option<OwnerActionProof>,
    pub event_id: Sha256Hex,
    pub event_kind: NativeActivityKind,
    pub memory: Option<MemoryRevisionRef>,
    pub source: Option<SourceRevisionRef>,
    pub title: Option<String>,
    pub change: Option<FormationChange>,
    pub knowledge: Option<KnowledgeState>,
    pub adoption: Option<AdoptionState>,
    pub effective_sensitivity: Sensitivity,
    pub formation_reason: Option<MemoryFormationReason>,
    pub proof: NativePublicationProof,
    pub receipt: Option<NativeReadReceipt>,
}
impl MemoryActivityEvent {
    fn valid(&self) -> bool {
        self.title
            .as_ref()
            .is_none_or(|s| record_fields::text(s, 60))
            && self
                .formation_reason
                .as_ref()
                .is_none_or(MemoryFormationReason::valid)
            && if self.event_kind == NativeActivityKind::MaterialCaptured {
                self.source.is_some()
                    && self.memory.is_none()
                    && self.title.is_none()
                    && self.change.is_none()
                    && self.knowledge.is_none()
                    && self.adoption.is_none()
                    && self.formation_reason.is_none()
                    && self.receipt.is_none()
                    && self.proof.actor.actor_type == ActorType::System
            } else if self.event_kind == NativeActivityKind::OwnerAction {
                self.proof.actor.actor_type == ActorType::Owner
                    && self.receipt.is_none()
                    && self.owner_action.as_ref().is_some_and(|p| {
                        p.valid()
                            && p.owner == self.proof.actor
                            && p.committed_in == self.proof.commit_id
                    })
            } else {
                self.memory.is_some()
                    && self.source.is_none()
                    && self.title.is_some()
                    && self.change.is_some()
                    && self.knowledge.is_some()
                    && self.adoption.is_some()
                    && self.formation_reason.is_some()
                    && if self.proof.actor.actor_type == ActorType::System {
                        self.owner_action.is_none()
                    } else {
                        self.owner_action.as_ref().is_some_and(|p| {
                            p.valid()
                                && p.action == "correct"
                                && p.owner == self.proof.actor
                                && p.committed_in == self.proof.commit_id
                                && self
                                    .receipt
                                    .as_ref()
                                    .is_some_and(|r| r.operation_id == p.operation_id)
                        })
                    }
                    && self.receipt.as_ref().is_some_and(|r| {
                        Some(&r.result) == self.memory.as_ref()
                            && r.committed_in == self.proof.commit_id
                    })
            }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ActivityCursor {
    pub header: ProcessHeader,
    pub after_time: Timestamp,
    pub after_id: Sha256Hex,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryActivityPage {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub matched_in_scope: u64,
    pub items: Vec<MemoryActivityEvent>,
    pub next: Option<ActivityCursor>,
    pub complete_in_scope: bool,
    pub time_derived_events_available: bool,
}
impl NextDocument for MemoryActivityPage {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "memory_activity_page"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("witnessed-native-activity-1")
            && self.matched_in_scope <= 3 * MAX_SEGMENTED_REVISIONS as u64
            && self.items.len() <= 100
            && self.items.len() as u64 <= self.matched_in_scope
            && self.items.iter().all(MemoryActivityEvent::valid)
            && (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                || self
                    .items
                    .iter()
                    .all(|e| e.receipt.as_ref().is_none_or(|r| r.sequence.is_none())))
            && !self.time_derived_events_available
            && self.items.windows(2).all(|w| {
                (&w[0].proof.written_at, &w[0].event_id) > (&w[1].proof.written_at, &w[1].event_id)
            })
            && self.complete_in_scope == self.next.is_none()
            && self.next.as_ref().is_none_or(|c| {
                c.header == self.header
                    && self.items.last().is_some_and(|e| {
                        e.proof.written_at == c.after_time && e.event_id == c.after_id
                    })
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.activity", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeAnchor {
    pub snapshot: SnapshotBinding,
    pub permission_hash: Sha256Hex,
    pub query_hash: Sha256Hex,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeStatus {
    Initialize,
    UnchangedInScope,
    Changed,
    ReloadRequired,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryChanges {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub status: ChangeStatus,
    pub next_anchor: ChangeAnchor,
    pub memories: Vec<MemoryRevisionRef>,
    pub sources: Vec<SourceRevisionRef>,
    pub all_projections_invalidated: bool,
    pub complete_in_scope: bool,
}
impl NextDocument for MemoryChanges {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "memory_changes"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("authorized-native-changes-1")
            && self.memories.len() + self.sources.len() <= 100
            && self
                .memories
                .windows(2)
                .all(|w| w[0].memory_id < w[1].memory_id)
            && self
                .sources
                .windows(2)
                .all(|w| w[0].source_id < w[1].source_id)
            && self.complete_in_scope
            && self.next_anchor.snapshot == self.header.selection.snapshot
            && self.next_anchor.permission_hash == self.header.permission_hash
            && self.next_anchor.query_hash == self.header.selection.query_hash
            && if self.status == ChangeStatus::Changed {
                !self.memories.is_empty() || !self.sources.is_empty()
            } else {
                self.memories.is_empty() && self.sources.is_empty()
            };
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.changes", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeRevisionSummary {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub owner_correction: Option<OwnerActionProof>,
    pub memory: TargetRef,
    pub title: String,
    pub knowledge: KnowledgeState,
    pub adoption: AdoptionState,
    pub interpretation: Interpretation,
    pub change: FormationChange,
    pub learned_at: Timestamp,
    pub effective_sensitivity: Sensitivity,
    pub receipt: NativeReadReceipt,
}
impl NativeRevisionSummary {
    fn valid(&self) -> bool {
        record_fields::text(&self.title, 60)
            && self.receipt.result == self.memory.memory
            && self.owner_correction.as_ref().is_none_or(|p| {
                p.valid()
                    && p.action == "correct"
                    && p.committed_in == self.receipt.committed_in
                    && p.operation_id == self.receipt.operation_id
            })
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HistoryCursor {
    pub header: ProcessHeader,
    pub memory: MemoryId,
    pub before_revision: Revision,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMemoryDetail {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub item: Option<CatalogReadItem>,
    pub fields: Option<MemoryRecordFields>,
    pub history: Vec<NativeRevisionSummary>,
    pub history_count_in_scope: u64,
    pub next_history: Option<HistoryCursor>,
}
impl NextDocument for NativeMemoryDetail {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "native_memory_detail"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("authorized-native-detail-1")
            && self.item.is_some() == self.fields.is_some()
            && self.history.len() <= 32
            && self.history.len() as u64 <= self.history_count_in_scope
            && self.history_count_in_scope <= MAX_SEGMENTED_REVISIONS as u64
            && self.history.iter().all(NativeRevisionSummary::valid)
            && (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                || self.history.iter().all(|r| r.receipt.sequence.is_none()))
            && self.item.as_ref().is_none_or(|i| {
                i.valid(&self.header.selection.evaluated_at)
                    && (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                        || (i.owner_marks.is_some()
                            && i.base
                                .receipt
                                .as_ref()
                                .is_some_and(|r| r.sequence.is_none())))
                    && i.effective_sensitivity.is_some()
                    && i.formation_reason.is_some()
            })
            && self
                .history
                .windows(2)
                .all(|w| w[0].memory.memory.revision > w[1].memory.memory.revision)
            && (self.item.is_some()
                || (self.history.is_empty()
                    && self.history_count_in_scope == 0
                    && self.next_history.is_none()))
            && self.next_history.as_ref().is_none_or(|c| {
                c.header == self.header
                    && self
                        .history
                        .last()
                        .is_some_and(|m| m.memory.memory.revision == c.before_revision)
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.detail", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConflictCursor {
    pub header: ProcessHeader,
    pub memory: MemoryId,
    pub after: MemoryId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryConflictGroup {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub selected: Option<MemoryRevisionRef>,
    pub basis: String,
    pub counter_evidence: Vec<SourceRevisionRef>,
    pub matched_in_scope: u64,
    pub items: Vec<CatalogReadItem>,
    pub next: Option<ConflictCursor>,
}
impl NextDocument for MemoryConflictGroup {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "memory_conflict_group"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("recorded-counter-evidence-1")
            && self.basis == "recorded_counter_source_overlap_not_independent_verification"
            && self.counter_evidence.len() <= 128
            && self.items.len() <= 100
            && self.matched_in_scope <= MAX_SEGMENTED_REVISIONS as u64
            && self.items.len() as u64 <= self.matched_in_scope
            && self.items.iter().all(|i| {
                i.valid(&self.header.selection.evaluated_at)
                    && (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                        || (i.owner_marks.is_some()
                            && i.base
                                .receipt
                                .as_ref()
                                .is_some_and(|r| r.sequence.is_none())))
                    && i.effective_sensitivity.is_some()
                    && i.formation_reason.is_some()
            })
            && self
                .items
                .windows(2)
                .all(|w| w[0].base.memory.memory_id < w[1].base.memory.memory_id)
            && (self.selected.is_some()
                || (self.items.is_empty()
                    && self.matched_in_scope == 0
                    && self.counter_evidence.is_empty()
                    && self.next.is_none()))
            && self.next.as_ref().is_none_or(|c| {
                c.header == self.header
                    && self
                        .items
                        .last()
                        .is_some_and(|m| m.base.memory.memory_id == c.after)
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.conflicts", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RevisionFieldDiff {
    pub field: String,
    pub before: serde_json::Value,
    pub after: serde_json::Value,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRevisionDiff {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub before: Option<TargetRef>,
    pub after: Option<TargetRef>,
    pub fields: Vec<RevisionFieldDiff>,
    pub structural_only: bool,
}
impl NextDocument for MemoryRevisionDiff {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "memory_revision_diff"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("structural-revision-diff-1")
            && self.structural_only
            && self.before.is_some() == self.after.is_some()
            && self.fields.len() <= 16
            && self
                .before
                .as_ref()
                .zip(self.after.as_ref())
                .is_none_or(|(a, b)| {
                    a.memory.memory_id == b.memory.memory_id
                        && a.memory.revision.get() + 1 == b.memory.revision.get()
                })
            && self
                .fields
                .iter()
                .all(|d| record_fields::text(&d.field, 64) && d.before != d.after)
            && (self.before.is_some() || self.fields.is_empty());
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.diff", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeNeighborRelation {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner_relations: Vec<OwnerActionProof>,
    pub relation: String,
    pub class: GraphEdgeClass,
    pub direction: String,
    pub witnesses: Vec<String>,
    pub witness_count_in_scope: u64,
    pub witnesses_complete: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMemoryNeighbor {
    pub other: NativeRevisionSummary,
    pub relations: Vec<NativeNeighborRelation>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NeighborCursor {
    pub header: ProcessHeader,
    pub memory: MemoryId,
    pub after: MemoryRevisionRef,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeMemoryNeighbors {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub selected: Option<TargetRef>,
    pub matched_in_scope: u64,
    pub items: Vec<NativeMemoryNeighbor>,
    pub next: Option<NeighborCursor>,
}
impl NextDocument for NativeMemoryNeighbors {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "native_memory_neighbors"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("typed-selected-neighbors-1")
            && self.items.len() <= 100
            && self.matched_in_scope <= MAX_SEGMENTED_REVISIONS as u64 + 2
            && self.items.iter().all(|n| {
                n.other.valid()
                    && (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                        || n.other.receipt.sequence.is_none())
                    && self.selected.as_ref().is_none_or(|t| &n.other.memory != t)
                    && !n.relations.is_empty()
                    && n.relations.len() <= 7
                    && n.relations.iter().all(|r| {
                        r.owner_relations.len() <= 64
                            && r.owner_relations.iter().all(|p| {
                                p.valid()
                                    && ["entity_merge", "entity_split"].contains(&p.action.as_str())
                            })
                            && (r.owner_relations.is_empty() || r.relation == "same_entity")
                            && !r.witnesses.is_empty()
                            && r.witnesses.len() <= 128
                            && r.witnesses.len() as u64 <= r.witness_count_in_scope
                            && r.witnesses_complete
                                == (r.witnesses.len() as u64 == r.witness_count_in_scope)
                            && ["outgoing", "incoming", "undirected"]
                                .contains(&r.direction.as_str())
                            && [
                                "derived_from",
                                "reversion",
                                "revises",
                                "corrects",
                                "same_entity",
                                "same_conversation",
                                "theme_cooccurrence",
                            ]
                            .contains(&r.relation.as_str())
                            && (r.class == GraphEdgeClass::AlgorithmSuggestion)
                                == (r.relation == "theme_cooccurrence")
                    })
            })
            && (self.selected.is_some()
                || (self.items.is_empty() && self.matched_in_scope == 0 && self.next.is_none()))
            && self.next.as_ref().is_none_or(|c| {
                c.header == self.header
                    && self
                        .items
                        .last()
                        .is_some_and(|m| m.other.memory.memory == c.after)
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.neighbors", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSourceMetadata {
    pub reference: SourceRevisionRef,
    pub source_kind: crate::source::SourceKind,
    pub locator: Locator,
    pub speaker: SpeakerRole,
    pub author_label: Option<String>,
    pub evidence_class: EvidenceClass,
    pub original_conversation_id: Option<String>,
    pub original_message_id: Option<String>,
    pub original_time: Option<String>,
    pub original_timezone: Option<String>,
    pub occurred_at: Option<Timestamp>,
    pub captured_at: Timestamp,
    pub time_precision: TimePrecision,
    pub content_hash: Sha256Hex,
    pub anchor_hash: Sha256Hex,
    pub effective_sensitivity: Sensitivity,
    pub extensions: crate::json::Extensions,
    pub extensions_are_untrusted_data: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeSourceExcerpt {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub source: Option<SourceRevisionRef>,
    pub source_record: Option<NativeSourceMetadata>,
    pub fields: Option<SourceRecordFields>,
    pub excerpt: Option<String>,
    pub start: u32,
    pub end: u32,
    pub total_characters: u32,
    pub complete_in_scope: bool,
}
impl NextDocument for NativeSourceExcerpt {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "native_source_excerpt"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("selected-source-excerpt-1")
            && self.source.is_some() == self.source_record.is_some()
            && self.source.is_some() == self.excerpt.is_some()
            && self.source_record.as_ref().is_none_or(|s| {
                s.extensions_are_untrusted_data && Some(&s.reference) == self.source.as_ref()
            })
            && self.start <= self.end
            && self.end <= self.total_characters
            && self
                .excerpt
                .as_ref()
                .is_none_or(|s| s.chars().count() as u32 == self.end - self.start)
            && (self.source.is_some()
                || (self.fields.is_none()
                    && self.start == 0
                    && self.end == 0
                    && self.total_characters == 0));
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.source", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConversationMetadata {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub owner_relations: Vec<OwnerActionProof>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turns: Option<Vec<ConversationTurn>>,
    /// Number of distinct turns in this captured revision, not platform total.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_count: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub branches: Option<Vec<BranchId>>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub import_batch_id: Option<ImportId>,
    pub complete_in_scope: bool,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub turn_sequence_scope: Option<String>,
}
impl NativeConversationMetadata {
    pub fn from_record(r: &ConversationRecord) -> Self {
        Self {
            owner_relations: vec![],
            conversation_id: r.conversation_id.clone(),
            revision: r.revision,
            platform: r.platform.clone(),
            account_scope: r.account_scope.clone(),
            original_conversation_id: r.original_conversation_id.clone(),
            title: r.title.clone(),
            participants: r.participants.clone(),
            started_at: r.started_at.clone(),
            ended_at: r.ended_at.clone(),
            turns: None,
            turn_count: None,
            branches: None,
            import_batch_id: r.import_batch_id.clone(),
            complete_in_scope: r.complete_in_scope,
            sensitivity: r.sensitivity,
            access_policy_id: r.access_policy_id.clone(),
            turn_sequence_scope: Some("authorized_captured_branch_order".into()),
        }
    }
    fn scoped(&self) -> bool {
        self.owner_relations
            .iter()
            .all(|p| p.valid() && ["entity_merge", "entity_split"].contains(&p.action.as_str()))
            && self.turns.is_none()
            && self.turn_count.is_none()
            && self.branches.is_none()
            && self.turn_sequence_scope.as_deref() == Some("authorized_captured_branch_order")
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationSummary {
    pub conversation: NativeConversationMetadata,
    pub visible_captured_turns: u64,
    pub visible_captured_sources: u64,
    pub effective_sensitivity: Sensitivity,
    pub platform_complete_not_claimed: bool,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationCursor {
    pub header: ProcessHeader,
    pub after: SessionId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConversationPage {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub matched_in_scope: u64,
    pub items: Vec<ConversationSummary>,
    pub next: Option<ConversationCursor>,
}
impl NextDocument for NativeConversationPage {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "native_conversation_page"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("authorized-conversation-list-1")
            && self.matched_in_scope <= MAX_SEGMENTED_REVISIONS as u64
            && self.items.len() <= 100
            && self.items.len() as u64 <= self.matched_in_scope
            && self.items.iter().all(|c| {
                (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                    || c.conversation.scoped())
                    && c.visible_captured_turns <= c.visible_captured_sources
                    && c.visible_captured_sources <= MAX_SEGMENTED_REVISIONS as u64
                    && c.platform_complete_not_claimed
            })
            && self
                .items
                .windows(2)
                .all(|w| w[0].conversation.conversation_id < w[1].conversation.conversation_id)
            && self.next.as_ref().is_none_or(|c| {
                c.header == self.header
                    && self
                        .items
                        .last()
                        .is_some_and(|s| s.conversation.conversation_id == c.after)
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.conversations", "")]
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConversationTurn {
    pub turn: ConversationTurn,
    pub source_record: NativeSourceMetadata,
    pub fields: SourceRecordFields,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ConversationTurnCursor {
    pub header: ProcessHeader,
    pub conversation: SessionId,
    pub after_source: SourceId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeConversationDetail {
    pub schema_version: NextVersion,
    pub kind: String,
    pub capability: CatalogCapability,
    pub effect: MemoryReadEffect,
    pub header: ProcessHeader,
    pub conversation: Option<NativeConversationMetadata>,
    pub effective_sensitivity: Option<Sensitivity>,
    pub visible_captured_turns: u64,
    pub visible_captured_sources: u64,
    pub turns: Vec<NativeConversationTurn>,
    pub next: Option<ConversationTurnCursor>,
    pub platform_complete_not_claimed: bool,
}
impl NextDocument for NativeConversationDetail {
    fn validate(&self) -> Vec<Violation> {
        let valid = self.kind == "native_conversation_detail"
            && self.effect == MemoryReadEffect::NativeRead
            && self.header.valid("authorized-conversation-turns-1")
            && self.visible_captured_turns <= MAX_SEGMENTED_REVISIONS as u64
            && self.visible_captured_sources <= MAX_SEGMENTED_REVISIONS as u64
            && self.visible_captured_turns <= self.visible_captured_sources
            && self.turns.len() as u64 <= self.visible_captured_sources
            && self.effective_sensitivity.is_some() == self.conversation.is_some()
            && (self.header.selection.algorithm != CatalogReadAlgorithm::V4
                || self
                    .conversation
                    .as_ref()
                    .is_none_or(NativeConversationMetadata::scoped))
            && self.turns.len() <= 100
            && self.platform_complete_not_claimed
            && (self.conversation.is_some()
                || (self.turns.is_empty()
                    && self.visible_captured_turns == 0
                    && self.visible_captured_sources == 0
                    && self.next.is_none()))
            && self.turns.iter().all(|t| {
                t.source_record.extensions_are_untrusted_data
                    && valid_content_lang(&t.fields.content_lang)
                    && t.fields.assistant_model.as_ref().is_none_or(|m| {
                        record_fields::text(m, 128)
                            && t.source_record.speaker == SpeakerRole::Assistant
                    })
                    && t.fields.conversation_id.as_ref()
                        == self.conversation.as_ref().map(|c| &c.conversation_id)
                    && t.turn.source.source_id == t.source_record.reference.source_id
                    && t.turn.source.source_revision == t.source_record.reference.source_revision
            })
            && self.next.as_ref().is_none_or(|c| {
                c.header == self.header
                    && self
                        .turns
                        .last()
                        .is_some_and(|t| t.turn.source.source_id == c.after_source)
            });
        if valid {
            vec![]
        } else {
            vec![Violation::new("process.conversation_detail", "")]
        }
    }
}

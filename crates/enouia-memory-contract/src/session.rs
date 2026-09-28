//! SessionRecord, SessionEvent, and the provisional SessionCheckpoint artifact
//! (DATA_MODEL §7). An automatic checkpoint is a Session artifact; it becomes a
//! canonical `session_checkpoint` memory only through owner review.

use crate::common::{ActorRef, ContentRef, Sensitivity, SourceRevisionRef};
use crate::error::Violation;
use crate::hash::Sha256Hex;
use crate::ids::{
    BranchId, CheckpointId, CommitId, EventId, ItemId, PolicyId, RequestId, ReviewId, SessionId,
    TurnId,
};
use crate::json::{Extensions, Revision, SchemaVersion, validate_extensions};
use crate::memory::StateKind;
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClientSurface {
    WindowsApp,
    LocalCli,
    McpClient,
    Import,
    Test,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Open,
    Closed,
    Interrupted,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProviderBinding {
    pub provider: String,
    pub model: String,
    pub adapter_version: String,
}

/// Provider binding history: which binding applied from which event sequence.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindingSpan {
    pub binding: ProviderBinding,
    pub from_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BranchRecord {
    pub branch_id: BranchId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent_branch_id: Option<BranchId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub forked_from_event_id: Option<EventId>,
    pub last_event_seq: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionRecord {
    pub schema_version: SchemaVersion,
    pub session_id: SessionId,
    pub revision: Revision,
    pub origin_surface: ClientSurface,
    pub provider_bindings: Vec<BindingSpan>,
    pub branches: Vec<BranchRecord>,
    pub default_branch_id: BranchId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent_session_id: Option<SessionId>,
    pub participants: Vec<ActorRef>,
    pub last_event_seq: u64,
    pub status: SessionStatus,
    pub sensitivity: Sensitivity,
    pub policy_id: PolicyId,
    pub created_at: Timestamp,
    pub updated_at: Timestamp,
    pub extensions: Extensions,
}

impl SessionRecord {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        let ids: BTreeSet<_> = self.branches.iter().map(|b| &b.branch_id).collect();
        if ids.len() != self.branches.len() || !ids.contains(&self.default_branch_id) {
            out.push(Violation::new("session.branches", "/branches"));
        }
        if self
            .branches
            .iter()
            .any(|b| b.last_event_seq > self.last_event_seq)
        {
            out.push(Violation::new("session.last_event_seq", "/last_event_seq"));
        }
        if self
            .provider_bindings
            .windows(2)
            .any(|w| w[0].from_sequence >= w[1].from_sequence)
        {
            out.push(Violation::new(
                "session.binding_order",
                "/provider_bindings",
            ));
        }
        if self.created_at > self.updated_at {
            out.push(Violation::new("session.time_order", "/updated_at"));
        }
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}

/// No kind exists for hidden model reasoning: it is never requested or stored.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    UserMessage,
    AssistantChunk,
    AssistantCompleted,
    ToolRequest,
    ToolResult,
    TurnCancelled,
    TurnFailed,
    CheckpointCreated,
    ProviderSwitched,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DeliveryState {
    NotApplicable,
    Partial,
    Completed,
    Interrupted,
    Failed,
}

impl EventKind {
    pub const fn delivery_state(self) -> DeliveryState {
        match self {
            Self::AssistantChunk => DeliveryState::Partial,
            Self::AssistantCompleted => DeliveryState::Completed,
            Self::TurnCancelled => DeliveryState::Interrupted,
            Self::TurnFailed => DeliveryState::Failed,
            _ => DeliveryState::NotApplicable,
        }
    }

    pub const fn needs_content(self) -> bool {
        matches!(
            self,
            Self::UserMessage
                | Self::AssistantChunk
                | Self::AssistantCompleted
                | Self::ToolRequest
                | Self::ToolResult
        )
    }

    pub const fn is_turn_event(self) -> bool {
        !matches!(self, Self::CheckpointCreated | Self::ProviderSwitched)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionEvent {
    pub schema_version: SchemaVersion,
    pub event_id: EventId,
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub sequence: u64,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub parent_event_id: Option<EventId>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub turn_id: Option<TurnId>,
    pub kind: EventKind,
    pub actor: ActorRef,
    pub occurred_at: Timestamp,
    pub captured_at: Timestamp,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub content_ref: Option<ContentRef>,
    pub source_refs: Vec<SourceRevisionRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub request_id: Option<RequestId>,
    pub delivery_state: DeliveryState,
    pub sensitivity: Sensitivity,
    pub extensions: Extensions,
}

impl SessionEvent {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.sequence == 0 {
            out.push(Violation::new("event.sequence", "/sequence"));
        }
        if self.kind.delivery_state() != self.delivery_state {
            out.push(Violation::new("event.delivery_state", "/delivery_state"));
        }
        if self.kind.needs_content() && self.content_ref.is_none() {
            out.push(Violation::new("event.content_required", "/content_ref"));
        }
        if self.kind.is_turn_event() && self.turn_id.is_none() {
            out.push(Violation::new("event.turn_required", "/turn_id"));
        }
        if self.occurred_at > self.captured_at {
            out.push(Violation::new("event.time_order", "/captured_at"));
        }
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Coverage {
    Range {
        from_sequence: u64,
        to_sequence: u64,
    },
    EventIds {
        event_ids: Vec<EventId>,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum CheckpointSourceRef {
    Event {
        event_id: EventId,
    },
    Source {
        source_id: crate::ids::SourceId,
        source_revision: Revision,
    },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SourcedItem {
    pub item_id: ItemId,
    pub claim: String,
    pub state_kind: StateKind,
    pub source_refs: Vec<CheckpointSourceRef>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckpointStatus {
    Provisional,
    Reviewed,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GeneratedBy {
    pub actor: ActorRef,
    pub generator_version: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SessionCheckpoint {
    pub schema_version: SchemaVersion,
    pub checkpoint_id: CheckpointId,
    pub revision: Revision,
    pub session_id: SessionId,
    pub branch_id: BranchId,
    pub coverage: Coverage,
    pub coverage_hash: Sha256Hex,
    pub base_vault_commit_id: CommitId,
    pub summary: String,
    pub decisions: Vec<SourcedItem>,
    pub open_loops: Vec<SourcedItem>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub last_completed_turn_id: Option<TurnId>,
    pub generated_by: GeneratedBy,
    pub status: CheckpointStatus,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub review_id: Option<ReviewId>,
    pub sensitivity: Sensitivity,
    pub created_at: Timestamp,
    pub extensions: Extensions,
}

impl SessionCheckpoint {
    pub fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        match &self.coverage {
            Coverage::Range {
                from_sequence,
                to_sequence,
            } => {
                if *from_sequence == 0 || from_sequence > to_sequence {
                    out.push(Violation::new("checkpoint.coverage", "/coverage"));
                }
            }
            Coverage::EventIds { event_ids } => {
                let unique: BTreeSet<_> = event_ids.iter().collect();
                if event_ids.is_empty() || unique.len() != event_ids.len() {
                    out.push(Violation::new("checkpoint.coverage", "/coverage"));
                }
            }
        }
        if (self.status == CheckpointStatus::Reviewed) != self.review_id.is_some() {
            out.push(Violation::new("checkpoint.review", "/review_id"));
        }
        for (index, item) in self.decisions.iter().chain(&self.open_loops).enumerate() {
            if item.source_refs.is_empty() {
                out.push(Violation::new(
                    "checkpoint.item_sources",
                    format!("/items/{index}"),
                ));
            }
        }
        validate_extensions(&self.extensions, "/extensions", &mut out);
        out
    }
}

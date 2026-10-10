//! Explicit session/branch operations. Every event and its exact body are
//! committed together; an unfinished turn remains unfinished after reopening.

use crate::util::*;
use enouia_memory_contract::commit::{ObjectKind, OperationKind};
use enouia_memory_contract::common::{ActorRef, ContentRef};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::*;
use enouia_memory_contract::json::{Revision, SchemaVersion};
use enouia_memory_contract::ports::{CommitOutcome, CommitPin, StagedObject};
use enouia_memory_contract::provider::InvocationState;
use enouia_memory_contract::record::{RecordKind, RecordRef};
use enouia_memory_contract::session::*;
use enouia_memory_vault::{
    Vault, VaultError,
    service::{SessionStart, Written},
};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeSet;

fn replay_record(outcome: &CommitOutcome, kind: RecordKind) -> Result<Option<&RecordRef>> {
    match outcome {
        CommitOutcome::Committed { .. } => Ok(None),
        CommitOutcome::Replayed { receipt, .. } => receipt
            .records
            .iter()
            .find(|r| r.record_kind == kind)
            .map(Some)
            .ok_or_else(missing),
    }
}

fn receipt_session(vault: &Vault, reference: &RecordRef) -> Result<SessionRecord> {
    read(
        vault,
        &vault.pin_current()?,
        RecordKind::Session,
        &reference.record_id,
        reference.revision,
    )
}

pub fn start(
    vault: &Vault,
    input: &SessionStart,
    key: &[u8],
) -> Result<Written<(SessionId, BranchId)>> {
    owner(vault, &input.owner)?;
    let payload = sha256(&bytes(
        &json!({"start": input.owner, "surface": input.surface, "sensitivity": input.sensitivity, "policy": input.policy_id}),
    )?);
    if let Some((commit_id, receipt)) = replay(
        vault,
        &input.owner,
        OperationKind::SessionAppend,
        key,
        &payload,
    )? {
        let id = receipt
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::Session)
            .ok_or_else(missing)?;
        let session = receipt_session(vault, id)?;
        return Ok(Written {
            outcome: CommitOutcome::Replayed { commit_id, receipt },
            id: (session.session_id, session.default_branch_id),
        });
    }
    let pin = vault.pin_current()?;
    let now = vault.now()?;
    let sid = SessionId::from_random(vault.random_id_bytes());
    let bid = BranchId::from_random(vault.random_id_bytes());
    let session = SessionRecord {
        provider_invocations: vec![],
        extraction_jobs: vec![],
        schema_version: SchemaVersion,
        session_id: sid.clone(),
        revision: one(),
        origin_surface: input.surface,
        provider_bindings: vec![],
        branches: vec![BranchRecord {
            branch_id: bid.clone(),
            parent_branch_id: None,
            forked_from_event_id: None,
            last_event_seq: 0,
        }],
        default_branch_id: bid.clone(),
        parent_session_id: None,
        participants: vec![input.owner.clone()],
        last_event_seq: 0,
        status: SessionStatus::Open,
        sensitivity: input.sensitivity,
        policy_id: input.policy_id.clone(),
        created_at: now.clone(),
        updated_at: now,
        extensions: Default::default(),
    };
    let outcome = commit(
        vault,
        &input.owner,
        &pin,
        OperationKind::SessionAppend,
        key,
        payload,
        vec![staged(RecordKind::Session, sid.as_str(), one(), &session)?],
        vec![],
    )?;
    let id = if let Some(reference) = replay_record(&outcome, RecordKind::Session)? {
        let stored = receipt_session(vault, reference)?;
        (stored.session_id, stored.default_branch_id)
    } else {
        (sid, bid)
    };
    Ok(Written { outcome, id })
}

pub fn get(vault: &Vault, pin: &CommitPin, sid: &SessionId) -> Result<SessionRecord> {
    let entry = vault
        .record_entry(pin, RecordKind::Session, sid.as_str())?
        .ok_or_else(missing)?;
    read(
        vault,
        pin,
        RecordKind::Session,
        sid.as_str(),
        entry.revision,
    )
}

pub fn events(
    vault: &Vault,
    pin: &CommitPin,
    sid: &SessionId,
    bid: &BranchId,
) -> Result<Vec<SessionEvent>> {
    let session = get(vault, pin, sid)?;
    if !session.branches.iter().any(|b| &b.branch_id == bid) {
        return Err(missing());
    }
    let all: Vec<SessionEvent> = latest(vault, pin, RecordKind::SessionEvent)?;
    let mut allowed = vec![(bid.clone(), u64::MAX)];
    let mut cursor = bid;
    let mut limit = u64::MAX;
    let mut visited = BTreeSet::new();
    while visited.insert(cursor.clone()) {
        let branch = session
            .branches
            .iter()
            .find(|b| &b.branch_id == cursor)
            .ok_or_else(missing)?;
        let (Some(parent), Some(fork)) = (&branch.parent_branch_id, &branch.forked_from_event_id)
        else {
            break;
        };
        let event = all
            .iter()
            .find(|e| &e.event_id == fork && &e.session_id == sid && &e.branch_id == parent)
            .ok_or_else(missing)?;
        limit = limit.min(event.sequence);
        allowed.push((parent.clone(), limit));
        cursor = parent;
    }
    let mut out: Vec<_> = all
        .into_iter()
        .filter(|e| {
            &e.session_id == sid
                && allowed
                    .iter()
                    .any(|(b, max)| &e.branch_id == b && e.sequence <= *max)
        })
        .collect();
    out.sort_by_key(|e| e.sequence);
    let references: Vec<_> = out
        .iter()
        .map(|e| RecordRef::new(RecordKind::SessionEvent, e.event_id.as_str(), one()))
        .collect();
    vault.check_fresh(pin, &references)?;
    Ok(out)
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct TurnStatus {
    pub turn_id: TurnId,
    pub input_event_id: EventId,
    pub last_persisted_event_id: EventId,
    pub state: String,
    pub partial_chunks: usize,
}

pub fn turns(
    vault: &Vault,
    actor: &ActorRef,
    sid: &SessionId,
    bid: &BranchId,
) -> Result<Vec<TurnStatus>> {
    owner(vault, actor)?;
    let pin = vault.pin_current()?;
    let mut out: Vec<TurnStatus> = vec![];
    for event in events(vault, &pin, sid, bid)? {
        let Some(turn) = event.turn_id else {
            continue;
        };
        if event.kind == EventKind::UserMessage {
            out.push(TurnStatus {
                turn_id: turn,
                input_event_id: event.event_id.clone(),
                last_persisted_event_id: event.event_id,
                state: "pending".into(),
                partial_chunks: 0,
            });
        } else if let Some(status) = out.iter_mut().find(|s| s.turn_id == turn) {
            status.last_persisted_event_id = event.event_id;
            match event.kind {
                EventKind::AssistantChunk => {
                    status.partial_chunks += 1;
                    status.state = "partial".into();
                }
                EventKind::AssistantCompleted => status.state = "completed".into(),
                EventKind::TurnCancelled => status.state = "interrupted".into(),
                EventKind::TurnFailed => status.state = "failed".into(),
                _ => {}
            }
        }
    }
    Ok(out)
}

/// Save input before compilation or a Mock call. Reusing a key with another
/// session, branch, request, or text is an idempotency conflict.
pub fn save_input(
    vault: &Vault,
    actor: &ActorRef,
    sid: &SessionId,
    bid: &BranchId,
    text: &str,
    request: &RequestId,
    key: &[u8],
) -> Result<Written<EventId>> {
    append(
        vault,
        actor,
        sid,
        bid,
        EventKind::UserMessage,
        None,
        Some(text),
        Some(request),
        key,
        None,
        &[],
        None,
    )
}

/// Save a native extraction prompt with its real source dependencies. Body,
/// source references and inherited sensitivity are published together.
#[allow(clippy::too_many_arguments)]
pub fn save_input_with_sources(
    vault: &Vault,
    actor: &ActorRef,
    sid: &SessionId,
    bid: &BranchId,
    text: &str,
    request: &RequestId,
    key: &[u8],
    sources: &[enouia_memory_contract::common::SourceRevisionRef],
) -> Result<Written<EventId>> {
    append(
        vault,
        actor,
        sid,
        bid,
        EventKind::UserMessage,
        None,
        Some(text),
        Some(request),
        key,
        None,
        sources,
        None,
    )
}

/// Stream chunks are explicitly partial. Completing stores the full response
/// as a separate durable event; cancellation/failure preserves existing chunks.
pub fn append_output(
    vault: &Vault,
    actor: &ActorRef,
    input: &EventId,
    kind: EventKind,
    text: Option<&str>,
    key: &[u8],
) -> Result<Written<EventId>> {
    append_output_with_guard(vault, actor, input, kind, text, key, None)
}

/// Provider publication barrier: commit against these epochs and inherit the
/// strictest request evidence sensitivity. The writer checks the pinned head.
#[derive(Clone, Copy)]
pub struct OutputGuard {
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub sensitivity: enouia_memory_contract::common::Sensitivity,
}
pub fn append_output_with_guard(
    vault: &Vault,
    actor: &ActorRef,
    input: &EventId,
    kind: EventKind,
    text: Option<&str>,
    key: &[u8],
    guard: Option<OutputGuard>,
) -> Result<Written<EventId>> {
    append_output_impl(vault, actor, input, kind, text, key, guard, None)
}

/// Native result metadata published atomically with the terminal event. This
/// is transport-neutral; it cannot create an admission or authorize a send.
#[derive(Clone, Debug, Serialize)]
pub struct InvocationCompletion {
    pub dispatch_id: DispatchId,
    pub state: InvocationState,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub error_code: Option<enouia_memory_contract::MemoryErrorCode>,
}

#[allow(clippy::too_many_arguments)]
pub fn append_invocation_output(
    vault: &Vault,
    actor: &ActorRef,
    input: &EventId,
    kind: EventKind,
    text: Option<&str>,
    key: &[u8],
    guard: Option<OutputGuard>,
    completion: &InvocationCompletion,
) -> Result<Written<EventId>> {
    append_output_impl(
        vault,
        actor,
        input,
        kind,
        text,
        key,
        guard,
        Some(completion),
    )
}

#[allow(clippy::too_many_arguments)]
fn append_output_impl(
    vault: &Vault,
    actor: &ActorRef,
    input: &EventId,
    kind: EventKind,
    text: Option<&str>,
    key: &[u8],
    guard: Option<OutputGuard>,
    completion: Option<&InvocationCompletion>,
) -> Result<Written<EventId>> {
    owner(vault, actor)?;
    if !matches!(
        kind,
        EventKind::AssistantChunk
            | EventKind::AssistantCompleted
            | EventKind::TurnCancelled
            | EventKind::TurnFailed
    ) {
        return Err(invalid("session.output_kind"));
    }
    let pin = vault.pin_current()?;
    let event: SessionEvent = read(vault, &pin, RecordKind::SessionEvent, input.as_str(), one())?;
    if event.kind != EventKind::UserMessage {
        return Err(invalid("session.input_required"));
    }
    append(
        vault,
        actor,
        &event.session_id,
        &event.branch_id,
        kind,
        Some(input),
        text,
        event.request_id.as_ref(),
        key,
        guard,
        &[],
        completion,
    )
}

#[allow(clippy::too_many_arguments)]
fn append(
    vault: &Vault,
    actor: &ActorRef,
    sid: &SessionId,
    bid: &BranchId,
    kind: EventKind,
    input: Option<&EventId>,
    text: Option<&str>,
    request: Option<&RequestId>,
    key: &[u8],
    guard: Option<OutputGuard>,
    sources: &[enouia_memory_contract::common::SourceRevisionRef],
    completion: Option<&InvocationCompletion>,
) -> Result<Written<EventId>> {
    owner(vault, actor)?;
    if (kind.needs_content() != text.is_some()
        && !(kind == EventKind::TurnFailed && text.is_some()))
        || text.is_some_and(|s| {
            // Output is exact Provider text, including whitespace or an empty
            // terminal response. Empty chunks carry no content/progress.
            kind == EventKind::UserMessage && s.trim().is_empty()
                || kind == EventKind::AssistantChunk && s.is_empty()
        })
    {
        return Err(invalid("session.content"));
    }
    let mut logical =
        json!({"session":sid,"branch":bid,"kind":kind,"input":input,"text":text,"request":request});
    if !sources.is_empty() {
        logical["sources"] = json!(sources);
    }
    if let Some(completion) = completion {
        logical["invocation"] = json!(completion);
    }
    let payload = sha256(&bytes(&logical)?);
    if let Some((commit_id, receipt)) =
        replay(vault, actor, OperationKind::SessionAppend, key, &payload)?
    {
        let id = receipt
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::SessionEvent)
            .and_then(|r| EventId::parse(&r.record_id).ok())
            .ok_or_else(missing)?;
        return Ok(Written {
            outcome: CommitOutcome::Replayed { commit_id, receipt },
            id,
        });
    }
    let pin = vault.pin_current()?;
    if guard.is_some_and(|g| {
        g.policy_epoch != pin.policy_epoch || g.deletion_epoch != pin.deletion_epoch
    }) {
        return Err(invalid("session.output_stale"));
    }
    let mut session = get(vault, &pin, sid)?;
    let mut sensitivity = session.sensitivity;
    for source in sources {
        let record = vault.read_parsed(
            &pin,
            &RecordRef::new(
                RecordKind::Source,
                source.source_id.as_str(),
                source.source_revision,
            ),
        )?;
        let enouia_memory_contract::record::AnyRecord::Source(record) = record else {
            return Err(missing());
        };
        sensitivity = sensitivity.max(record.sensitivity);
    }
    let branch_events = events(vault, &pin, sid, bid)?;
    let previous = branch_events.last();
    let turn = match input {
        Some(id) => {
            let user = branch_events
                .iter()
                .find(|e| {
                    &e.event_id == id && &e.branch_id == bid && e.kind == EventKind::UserMessage
                })
                .ok_or_else(missing)?;
            if branch_events.iter().any(|e| {
                e.turn_id == user.turn_id
                    && matches!(
                        e.kind,
                        EventKind::AssistantCompleted
                            | EventKind::TurnCancelled
                            | EventKind::TurnFailed
                    )
            }) {
                return Err(invalid("session.turn_terminal"));
            }
            user.turn_id.clone().ok_or_else(missing)?
        }
        None => {
            if let Some(user) = branch_events
                .iter()
                .rev()
                .find(|e| &e.branch_id == bid && e.kind == EventKind::UserMessage)
                && !branch_events.iter().any(|e| {
                    e.turn_id == user.turn_id
                        && matches!(
                            e.kind,
                            EventKind::AssistantCompleted
                                | EventKind::TurnCancelled
                                | EventKind::TurnFailed
                        )
                })
            {
                return Err(invalid("session.turn_in_progress"));
            }
            TurnId::from_random(vault.random_id_bytes())
        }
    };
    let now = vault.now()?;
    let sequence = session
        .last_event_seq
        .checked_add(1)
        .ok_or_else(|| invalid("number.out_of_range"))?;
    let id = EventId::from_random(vault.random_id_bytes());
    let content_ref = text.map(|text| ContentRef {
        object_hash: sha256(text.as_bytes()),
        size_bytes: text.len() as u64,
        media_type: "text/plain; charset=utf-8".into(),
    });
    let event = SessionEvent {
        schema_version: SchemaVersion,
        event_id: id.clone(),
        session_id: sid.clone(),
        branch_id: bid.clone(),
        sequence,
        parent_event_id: previous.map(|e| e.event_id.clone()),
        turn_id: Some(turn),
        kind,
        actor: actor.clone(),
        occurred_at: now.clone(),
        captured_at: now.clone(),
        content_ref: content_ref.clone(),
        source_refs: sources.to_vec(),
        request_id: request.cloned(),
        delivery_state: kind.delivery_state(),
        sensitivity: guard.map_or(sensitivity, |g| sensitivity.max(g.sensitivity)),
        extensions: Default::default(),
    };
    if let Some(completion) = completion {
        let matches_kind = matches!(
            (completion.state, kind),
            (InvocationState::Completed, EventKind::AssistantCompleted)
                | (
                    InvocationState::Length | InvocationState::Failed,
                    EventKind::TurnFailed
                )
                | (InvocationState::Cancelled, EventKind::TurnCancelled)
        );
        if !matches_kind {
            return Err(invalid("session.invocation_terminal_kind"));
        }
        let row = session
            .provider_invocations
            .iter_mut()
            .find(|r| r.dispatch_id == completion.dispatch_id && Some(&r.input_event_id) == input)
            .ok_or_else(|| invalid("session.invocation_required"))?;
        if row.state != InvocationState::OutcomeUnknown {
            return Err(invalid("session.invocation_terminal"));
        }
        row.state = completion.state;
        row.terminal_event_id = Some(id.clone());
        row.input_tokens = completion.input_tokens;
        row.output_tokens = completion.output_tokens;
        row.error_code = completion.error_code;
        if !row.validate().is_empty() {
            return Err(invalid("session.invocation_metadata"));
        }
    }
    session.revision =
        Revision::new(session.revision.get() + 1).ok_or_else(|| invalid("number.out_of_range"))?;
    session.last_event_seq = sequence;
    session.updated_at = now;
    session.status = if matches!(kind, EventKind::TurnCancelled | EventKind::TurnFailed) {
        SessionStatus::Interrupted
    } else {
        SessionStatus::Open
    };
    session
        .branches
        .iter_mut()
        .find(|b| &b.branch_id == bid)
        .ok_or_else(missing)?
        .last_event_seq = sequence;
    let objects = match (text, content_ref) {
        (Some(text), Some(reference)) => vec![StagedObject {
            hash: reference.object_hash,
            kind: ObjectKind::SessionContent,
            bytes: text.as_bytes().to_vec(),
        }],
        _ => vec![],
    };
    let outcome = commit(
        vault,
        actor,
        &pin,
        OperationKind::SessionAppend,
        key,
        payload,
        vec![
            staged(
                RecordKind::Session,
                sid.as_str(),
                session.revision,
                &session,
            )?,
            staged(RecordKind::SessionEvent, id.as_str(), one(), &event)?,
        ],
        objects,
    )?;
    let id = if let Some(reference) = replay_record(&outcome, RecordKind::SessionEvent)? {
        EventId::parse(&reference.record_id).map_err(|_| missing())?
    } else {
        id
    };
    Ok(Written { outcome, id })
}

/// Fork only at a persisted terminal turn. Subsequent events on the parent
/// cannot enter the new branch's ancestry or checkpoint.
pub fn fork(
    vault: &Vault,
    actor: &ActorRef,
    sid: &SessionId,
    at: &EventId,
    key: &[u8],
) -> Result<Written<BranchId>> {
    owner(vault, actor)?;
    let payload = sha256(&bytes(&json!({"fork":sid,"at":at}))?);
    if let Some((commit_id, receipt)) =
        replay(vault, actor, OperationKind::SessionAppend, key, &payload)?
    {
        let entry = receipt
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::Session)
            .ok_or_else(missing)?;
        let session = receipt_session(vault, entry)?;
        let id = session
            .branches
            .last()
            .ok_or_else(missing)?
            .branch_id
            .clone();
        return Ok(Written {
            outcome: CommitOutcome::Replayed { commit_id, receipt },
            id,
        });
    }
    let pin = vault.pin_current()?;
    let event: SessionEvent = read(vault, &pin, RecordKind::SessionEvent, at.as_str(), one())?;
    if &event.session_id != sid
        || !matches!(
            event.kind,
            EventKind::AssistantCompleted | EventKind::TurnCancelled | EventKind::TurnFailed
        )
    {
        return Err(invalid("session.fork_terminal_required"));
    }
    let mut session = get(vault, &pin, sid)?;
    let id = BranchId::from_random(vault.random_id_bytes());
    session.branches.push(BranchRecord {
        branch_id: id.clone(),
        parent_branch_id: Some(event.branch_id),
        forked_from_event_id: Some(at.clone()),
        last_event_seq: 0,
    });
    session.revision =
        Revision::new(session.revision.get() + 1).ok_or_else(|| invalid("number.out_of_range"))?;
    session.updated_at = vault.now()?;
    let outcome = commit(
        vault,
        actor,
        &pin,
        OperationKind::SessionAppend,
        key,
        payload,
        vec![staged(
            RecordKind::Session,
            sid.as_str(),
            session.revision,
            &session,
        )?],
        vec![],
    )?;
    let id = if let Some(reference) = replay_record(&outcome, RecordKind::Session)? {
        receipt_session(vault, reference)?
            .branches
            .last()
            .ok_or_else(missing)?
            .branch_id
            .clone()
    } else {
        id
    };
    Ok(Written { outcome, id })
}

/// Canonical byte digest of exact covered event records, ordered by sequence.
pub fn coverage_hash(events: &[SessionEvent]) -> Result<Sha256Hex> {
    Ok(sha256(&bytes(&events)?))
}

#[derive(Clone, Debug, Serialize)]
pub struct CheckpointInput {
    pub summary: String,
    pub decisions: Vec<SourcedItem>,
    pub open_loops: Vec<SourcedItem>,
}

/// Each checkpoint replaces the branch's complete provisional summary/open
/// loop view. Callers explicitly remove solved loops in the next checkpoint.
pub fn checkpoint(
    vault: &Vault,
    actor: &ActorRef,
    sid: &SessionId,
    bid: &BranchId,
    input: &CheckpointInput,
    key: &[u8],
) -> Result<Written<CheckpointId>> {
    owner(vault, actor)?;
    if input.summary.trim().is_empty() {
        return Err(invalid("checkpoint.summary"));
    }
    let payload = sha256(&bytes(
        &json!({"session":sid,"branch":bid,"checkpoint":input}),
    )?);
    if let Some((commit_id, receipt)) = replay(
        vault,
        actor,
        OperationKind::CheckpointPropose,
        key,
        &payload,
    )? {
        let id = receipt
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::Checkpoint)
            .and_then(|r| CheckpointId::parse(&r.record_id).ok())
            .ok_or_else(missing)?;
        return Ok(Written {
            outcome: CommitOutcome::Replayed { commit_id, receipt },
            id,
        });
    }
    let pin = vault.pin_current()?;
    let session = get(vault, &pin, sid)?;
    let covered: Vec<_> = events(vault, &pin, sid, bid)?
        .into_iter()
        .filter(|e| &e.branch_id == bid)
        .collect();
    if covered.is_empty() {
        return Err(invalid("checkpoint.empty_coverage"));
    }
    // No external evidence is silently added to this branch-local summary.
    for item in input.decisions.iter().chain(&input.open_loops) {
        if item.source_refs.is_empty() || item.source_refs.iter().any(|r| !matches!(r,CheckpointSourceRef::Event{event_id} if covered.iter().any(|e| &e.event_id==event_id))) {return Err(invalid("checkpoint.item_outside_coverage"));}
    }
    let id = CheckpointId::from_random(vault.random_id_bytes());
    let artifact = SessionCheckpoint {
        schema_version: SchemaVersion,
        checkpoint_id: id.clone(),
        revision: one(),
        session_id: sid.clone(),
        branch_id: bid.clone(),
        coverage: Coverage::EventIds {
            event_ids: covered.iter().map(|e| e.event_id.clone()).collect(),
        },
        coverage_hash: coverage_hash(&covered)?,
        base_vault_commit_id: pin.commit_id.clone(),
        summary: input.summary.clone(),
        decisions: input.decisions.clone(),
        open_loops: input.open_loops.clone(),
        last_completed_turn_id: covered
            .iter()
            .rev()
            .find(|e| e.kind == EventKind::AssistantCompleted)
            .and_then(|e| e.turn_id.clone()),
        generated_by: GeneratedBy {
            actor: actor.clone(),
            generator_version: "offline-checkpoint-1".into(),
        },
        status: CheckpointStatus::Provisional,
        review_id: None,
        sensitivity: session.sensitivity,
        created_at: vault.now()?,
        extensions: Default::default(),
    };
    let outcome = commit(
        vault,
        actor,
        &pin,
        OperationKind::CheckpointPropose,
        key,
        payload,
        vec![staged(
            RecordKind::Checkpoint,
            id.as_str(),
            one(),
            &artifact,
        )?],
        vec![],
    )?;
    let id = if let Some(reference) = replay_record(&outcome, RecordKind::Checkpoint)? {
        CheckpointId::parse(&reference.record_id).map_err(|_| missing())?
    } else {
        id
    };
    Ok(Written { outcome, id })
}

pub fn verify_checkpoint(
    vault: &Vault,
    pin: &CommitPin,
    artifact: &SessionCheckpoint,
) -> Result<()> {
    let covered = events(vault, pin, &artifact.session_id, &artifact.branch_id)?
        .into_iter()
        .filter(|e| match &artifact.coverage {
            Coverage::EventIds { event_ids } => event_ids.contains(&e.event_id),
            Coverage::Range {
                from_sequence,
                to_sequence,
            } => e.sequence >= *from_sequence && e.sequence <= *to_sequence,
        })
        .collect::<Vec<_>>();
    if coverage_hash(&covered)? != artifact.coverage_hash {
        return Err(invalid("checkpoint.coverage_hash"));
    }
    Ok(())
}

pub fn text(vault: &Vault, pin: &CommitPin, event: &SessionEvent) -> Result<String> {
    let reference = event.content_ref.as_ref().ok_or_else(missing)?;
    let bytes = vault.read_object(pin, &reference.object_hash)?;
    if bytes.len() as u64 != reference.size_bytes || sha256(&bytes) != reference.object_hash {
        return Err(VaultError::corrupt("session_content"));
    }
    String::from_utf8(bytes).map_err(|_| invalid("session.utf8"))
}

pub fn switch_binding(
    vault: &Vault,
    actor: &ActorRef,
    sid: &SessionId,
    binding: &ProviderBinding,
    key: &[u8],
) -> Result<CommitOutcome> {
    owner(vault, actor)?;
    let payload = sha256(&bytes(&json!({"session":sid,"binding":binding}))?);
    if let Some((commit_id, receipt)) =
        replay(vault, actor, OperationKind::SessionAppend, key, &payload)?
    {
        return Ok(CommitOutcome::Replayed { commit_id, receipt });
    }
    let pin = vault.pin_current()?;
    let mut session = get(vault, &pin, sid)?;
    let from_sequence = session.last_event_seq + 1;
    if let Some(last) = session
        .provider_bindings
        .last_mut()
        .filter(|b| b.from_sequence == from_sequence)
    {
        last.binding = binding.clone();
    } else {
        session.provider_bindings.push(BindingSpan {
            binding: binding.clone(),
            from_sequence,
        });
    }
    session.revision =
        Revision::new(session.revision.get() + 1).ok_or_else(|| invalid("number.out_of_range"))?;
    session.updated_at = vault.now()?;
    commit(
        vault,
        actor,
        &pin,
        OperationKind::SessionAppend,
        key,
        payload,
        vec![staged(
            RecordKind::Session,
            sid.as_str(),
            session.revision,
            &session,
        )?],
        vec![],
    )
}

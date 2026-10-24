//! Provisional Agent summaries of authorized existing branch events.
use super::*;
use enouia_memory_context::session::{self, AgentCheckpointInput, AgentCheckpointItem};
use enouia_memory_contract::ids::{CommitId, OperationId};
use enouia_memory_contract::ipc::SessionCheckpointArgs;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_contract::session::{CheckpointStatus, SessionCheckpoint, SessionRecord};

fn authorize(
    read: &mut Read<'_>,
    args: &SessionCheckpointArgs,
) -> Result<Vec<enouia_memory_contract::session::SessionEvent>> {
    let record: SessionRecord = read.record(RecordKind::Session, args.session_id.as_str(), None)?;
    let reference = RecordRef::new(
        RecordKind::Session,
        args.session_id.as_str(),
        record.revision,
    );
    if !read.allowed(&reference, None, record.sensitivity, Scope::SessionPropose)? {
        return Err(missing());
    }
    read.refs.push(reference);
    let events: Vec<_> = session::events(read.vault, &read.pin, &args.session_id, &args.branch_id)?
        .into_iter()
        .filter(|e| {
            e.branch_id == args.branch_id
                && e.sequence >= args.from_sequence
                && e.sequence <= args.to_sequence
        })
        .collect();
    if events
        .first()
        .is_none_or(|e| e.sequence != args.from_sequence)
        || events.last().is_none_or(|e| e.sequence != args.to_sequence)
    {
        return Err(fail(MemoryErrorCode::InvalidRequest));
    }
    for event in &events {
        let reference = RecordRef::new(
            RecordKind::SessionEvent,
            event.event_id.as_str(),
            Revision::new(1).expect("one"),
        );
        if !read.allowed(&reference, None, event.sensitivity, Scope::SessionPropose)? {
            return Err(missing());
        }
        read.refs.push(reference);
        for cited in &event.source_refs {
            let current: SourceRecord =
                read.record(RecordKind::Source, cited.source_id.as_str(), None)?;
            let source: SourceRecord = read.record(
                RecordKind::Source,
                cited.source_id.as_str(),
                Some(cited.source_revision),
            )?;
            for s in [&current, &source] {
                let reference =
                    RecordRef::new(RecordKind::Source, s.source_id.as_str(), s.revision);
                if !read.allowed(&reference, None, s.sensitivity, Scope::SessionPropose)? {
                    return Err(missing());
                }
                read.refs.push(reference);
            }
        }
    }
    for item in args.decisions.iter().chain(&args.open_loops) {
        if item.event_ids.is_empty()
            || item
                .event_ids
                .iter()
                .any(|id| !events.iter().any(|e| &e.event_id == id))
        {
            return Err(fail(MemoryErrorCode::InvalidRequest));
        }
    }
    Ok(events)
}
struct Saved {
    value: Value,
    commit: CommitId,
    operation: OperationId,
}
pub(super) fn execute(
    vault: &Vault,
    principal: &ActorRef,
    request: &IpcRequest,
    args: SessionCheckpointArgs,
) -> std::result::Result<Value, PortFault> {
    let pin = vault.pin_current().map_err(|_| PortFault::Unavailable)?;
    let mut read = Read {
        vault,
        pin,
        principal,
        policies: Vec::new(),
        tombstones: Vec::new(),
        refs: Vec::new(),
    };
    let mut entered = false;
    let mut published = false;
    let result = (|| {
        read.policies = latest(vault, &read.pin, RecordKind::Policy)?;
        read.tombstones = latest(vault, &read.pin, RecordKind::Tombstone)?;
        let events = authorize(&mut read, &args)?;
        propose::audit(&read, request, None)?;
        let items = |rows: Vec<ipc::ProposedItem>| {
            rows.into_iter()
                .map(|item| AgentCheckpointItem {
                    claim: item.claim,
                    state_kind: item.state_kind,
                    event_ids: item.event_ids,
                })
                .collect()
        };
        let input = AgentCheckpointInput {
            session_id: args.session_id,
            branch_id: args.branch_id,
            from_sequence: args.from_sequence,
            to_sequence: args.to_sequence,
            summary: args.summary,
            decisions: items(args.decisions),
            open_loops: items(args.open_loops),
        };
        let key = request
            .idempotency_key
            .as_ref()
            .ok_or_else(|| fail(MemoryErrorCode::InvalidRequest))?;
        entered = true;
        let done = session::checkpoint_agent_at_head(
            vault,
            principal,
            &input,
            key.as_bytes(),
            &read.pin.commit_id,
        )?;
        published = matches!(&done.outcome, CommitOutcome::Committed { .. });
        let (commit, receipt) = match done.outcome {
            CommitOutcome::Committed { commit_id, receipt }
            | CommitOutcome::Replayed { commit_id, receipt } => (commit_id, receipt),
        };
        let head = vault.pin_current()?;
        let entry = vault
            .record_entry(&head, RecordKind::Checkpoint, done.id.as_str())?
            .ok_or_else(missing)?;
        let current_ref = RecordRef::new(RecordKind::Checkpoint, done.id.as_str(), entry.revision);
        if !read.visible(&current_ref) {
            return Err(missing());
        }
        let current: SessionCheckpoint = parse_record(&vault.read_record(&head, &current_ref)?)
            .map_err(|_| fail(MemoryErrorCode::StorageFailed))?;
        if current.status != CheckpointStatus::Provisional
            || current.generated_by.actor != *principal
        {
            return Err(fail(MemoryErrorCode::RevisionConflict));
        }
        let reference = receipt
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::Checkpoint && r.record_id == done.id.as_str())
            .ok_or_else(|| fail(MemoryErrorCode::StorageFailed))?;
        let artifact: SessionCheckpoint = parse_record(&vault.read_revision(&head, reference)?)
            .map_err(|_| fail(MemoryErrorCode::StorageFailed))?;
        if artifact.generated_by.actor != *principal
            || artifact.status != CheckpointStatus::Provisional
            || artifact.session_id != input.session_id
            || artifact.branch_id != input.branch_id
            || artifact.coverage_hash != session::coverage_hash(&events)?
        {
            return Err(fail(MemoryErrorCode::StorageFailed));
        }
        session::verify_checkpoint(vault, &head, &artifact)?;
        read.refs.push(reference.clone());
        Ok(Saved {
            value: json!({"checkpointId":done.id,"revision":reference.revision,"status":"provisional"}),
            commit,
            operation: receipt.operation_id,
        })
    })();
    let code = result.as_ref().err().map(VaultError::code);
    let final_audit = propose::audit(&read, request, code);
    if (published
        || entered
            && code.is_some_and(|c| {
                matches!(
                    c,
                    MemoryErrorCode::StorageFailed
                        | MemoryErrorCode::StorageFull
                        | MemoryErrorCode::VaultRecovering
                )
            }))
        && (result.is_err() || final_audit.is_err())
    {
        return Err(PortFault::OutcomeUnknown);
    }
    if result.is_ok() && final_audit.is_err() {
        return Err(PortFault::OutcomeUnknown);
    }
    if result.is_ok() {
        let fresh = vault
            .check_fresh(&read.pin, &read.refs)
            .map_err(|_| PortFault::OutcomeUnknown)?;
        if fresh.policy_changed
            || fresh.head.deletion_epoch != read.pin.deletion_epoch
            || vault.host_writer_state().is_err()
        {
            return Err(PortFault::OutcomeUnknown);
        }
    }
    let result = final_audit.and(result);
    let error = result.as_ref().err().map(|e| {
        let mut error = e.error;
        error.retryable = false;
        error
    });
    let (value, commit, operation) = match result {
        Ok(saved) => (Some(saved.value), saved.commit, Some(saved.operation)),
        Err(_) => (None, read.pin.commit_id, None),
    };
    Ok(
        json!({"schemaVersion":1,"requestId":request.request_id,"kind":if error.is_some(){"memory_error"}else{"checkpoint_proposed"},
        "vaultCommitId":commit,"policyEpoch":read.pin.policy_epoch,"operationId":operation,"result":value,"error":error}),
    )
}

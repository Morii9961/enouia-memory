//! Pending Agent proposals over the existing bound governance writer.
use super::*;
use enouia_memory_contract::candidate::{
    CandidateRecord, CandidateStatus, ProposalKind, ProposedType,
};
use enouia_memory_contract::commit::{AuditDecision, AuditEvent};
use enouia_memory_contract::common::Locator;
use enouia_memory_contract::ids::{AuditId, CommitId, OperationId};
use enouia_memory_contract::ipc::MemoryProposeArgs;
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::ports::CommitOutcome;
use enouia_memory_govern::{
    EvidenceSpec, Origin, Proposal, Proposed, propose_agent_bound_at_head, replay_bound_proposal,
};
use enouia_memory_vault::service::Written;

struct Stored {
    result: Value,
    commit: CommitId,
    operation: OperationId,
}
fn authorize(read: &mut Read<'_>, args: &MemoryProposeArgs) -> Result<()> {
    if matches!(
        args.proposed_type,
        ProposedType::Identity | ProposedType::SessionCheckpoint
    ) || matches!(
        args.proposal_kind,
        ProposalKind::IdentityChange | ProposalKind::Delete
    ) {
        return Err(fail(MemoryErrorCode::PermissionDenied));
    }
    for evidence in &args.evidence {
        let current: SourceRecord =
            read.record(RecordKind::Source, evidence.source_id.as_str(), None)?;
        let cited: SourceRecord = read.record(
            RecordKind::Source,
            evidence.source_id.as_str(),
            Some(evidence.source_revision),
        )?;
        for source in [&current, &cited] {
            let reference = RecordRef::new(
                RecordKind::Source,
                source.source_id.as_str(),
                source.revision,
            );
            if !read.allowed(&reference, None, source.sensitivity, Scope::MemoryPropose)? {
                return Err(missing());
            }
            read.refs.push(reference);
        }
    }
    if let Some(target) = &args.target_memory_id {
        let memory: CanonicalMemory = read.record(RecordKind::Memory, target.as_str(), None)?;
        let reference = RecordRef::new(RecordKind::Memory, target.as_str(), memory.revision);
        if !read.allowed(
            &reference,
            memory.project_id,
            memory.sensitivity,
            Scope::MemoryPropose,
        )? {
            return Err(missing());
        }
        read.refs.push(reference);
    }
    Ok(())
}
pub(super) fn audit(
    read: &Read<'_>,
    request: &IpcRequest,
    code: Option<MemoryErrorCode>,
) -> Result<()> {
    AuditLog::new(read.vault.managed_root().clone(), DEFAULT_SEGMENT_BYTES).append(&AuditEvent {
        schema_version: SchemaVersion,
        audit_id: AuditId::from_random(read.vault.random_id_bytes()),
        actor: read.principal.clone(),
        operation: request.operation,
        object_refs: read.refs.clone(),
        purpose: (request.operation == ipc::Operation::SessionCheckpoint)
            .then_some(enouia_memory_contract::context::Purpose::Checkpoint),
        destination: None,
        policy_epoch: read.pin.policy_epoch,
        decision: if code.is_some() {
            AuditDecision::Deny
        } else {
            AuditDecision::Allow
        },
        error_code: code,
        request_id: request.request_id.clone(),
        created_at: read.vault.now()?,
    })
}
fn stored(
    read: &mut Read<'_>,
    done: Written<enouia_memory_contract::ids::CandidateId>,
) -> Result<Stored> {
    let (commit, receipt) = match done.outcome {
        CommitOutcome::Committed { commit_id, receipt }
        | CommitOutcome::Replayed { commit_id, receipt } => (commit_id, receipt),
    };
    // Re-pin only to inspect the just-published result; never rerun the request.
    let head = read.vault.pin_current()?;
    let reference = receipt
        .records
        .iter()
        .find(|r| r.record_kind == RecordKind::Candidate && r.record_id == done.id.as_str())
        .ok_or_else(|| fail(MemoryErrorCode::StorageFailed))?;
    let current = read
        .vault
        .record_entry(&head, RecordKind::Candidate, done.id.as_str())?
        .ok_or_else(missing)?;
    let current_ref = RecordRef::new(RecordKind::Candidate, done.id.as_str(), current.revision);
    let current: CandidateRecord = parse_record(&read.vault.read_record(&head, &current_ref)?)
        .map_err(|_| fail(MemoryErrorCode::StorageFailed))?;
    if current.origin_actor != *read.principal || current.status != CandidateStatus::Pending {
        return Err(fail(MemoryErrorCode::RevisionConflict));
    }
    let original: CandidateRecord = parse_record(&read.vault.read_revision(&head, reference)?)
        .map_err(|_| fail(MemoryErrorCode::StorageFailed))?;
    if original.origin_actor != *read.principal || original.status != CandidateStatus::Pending {
        return Err(fail(MemoryErrorCode::StorageFailed));
    }
    read.refs.push(reference.clone());
    // The source/target epochs from admission still guard receipt delivery.
    let fresh = read.vault.check_fresh(&read.pin, &read.refs)?;
    if fresh.policy_changed || fresh.head.deletion_epoch != read.pin.deletion_epoch {
        return Err(fail(MemoryErrorCode::RevisionConflict));
    }
    read.vault.host_writer_state()?;
    Ok(Stored {
        result: json!({"candidateId":done.id,"revision":reference.revision,"state":"pending"}),
        commit,
        operation: receipt.operation_id,
    })
}

pub(super) fn execute(
    vault: &Vault,
    principal: &ActorRef,
    request: &IpcRequest,
    args: MemoryProposeArgs,
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
    let mut entered_publication = false;
    let mut published = false;
    let result = (|| {
        read.policies = latest(vault, &read.pin, RecordKind::Policy)?;
        read.tombstones = latest(vault, &read.pin, RecordKind::Tombstone)?;
        authorize(&mut read, &args)?;
        let key = request
            .idempotency_key
            .as_ref()
            .ok_or_else(|| fail(MemoryErrorCode::InvalidRequest))?;
        let payload = sha256(
            &canonical_bytes(
                &json!({"operation":request.operation.canonical(),"arguments":request.arguments}),
            )
            .map_err(|_| fail(MemoryErrorCode::InvalidRequest))?,
        );
        let key = format!("mcp-agent-proposal-v1:{key}");
        // Check current permissions first, including on receipt-only replay.
        let replay = replay_bound_proposal(vault, principal, key.as_bytes(), &payload)?;
        audit(&read, request, None)?;
        if let Some(done) = replay {
            return stored(&mut read, done);
        }
        let proposal = Proposal {
            kind: args.proposal_kind,
            proposed_type: args.proposed_type,
            content: args.proposed_content,
            details: args.proposed_details,
            evidence: args
                .evidence
                .into_iter()
                .map(|e| EvidenceSpec {
                    source_id: e.source_id,
                    source_revision: e.source_revision,
                    supports: e.supports,
                    locator: e.range.map(|r| Locator::ByteRange {
                        start: r.start,
                        end: r.end,
                    }),
                })
                .collect(),
            context_source_refs: vec![],
            reason: args.reason,
            sensitivity: None,
            target_memory_id: args.target_memory_id,
            target_identity_id: None,
            expected_revision: args.expected_revision,
            effective_from: args.proposed_effective_from,
            reopens_candidate_id: None,
        };
        entered_publication = true;
        match propose_agent_bound_at_head(
            vault,
            &proposal,
            &Origin::agent(principal.clone()),
            key.as_bytes(),
            &payload,
            &read.pin.commit_id,
        )? {
            Proposed::Stored(done) => {
                published = true;
                stored(&mut read, done)
            }
            // A global duplicate is not a durable success for this caller/key.
            // Do not reveal its ID or fabricate a successful operation receipt.
            Proposed::DuplicateOf(_) => Err(fail(MemoryErrorCode::RevisionConflict)),
        }
    })();
    let code = result.as_ref().err().map(VaultError::code);
    let final_audit = audit(&read, request, code);
    let ambiguous = published
        || entered_publication
            && code.is_some_and(|c| {
                matches!(
                    c,
                    MemoryErrorCode::StorageFailed
                        | MemoryErrorCode::StorageFull
                        | MemoryErrorCode::VaultRecovering
                )
            });
    if ambiguous && (result.is_err() || final_audit.is_err()) {
        return Err(PortFault::OutcomeUnknown);
    }
    // A recovered receipt can also become unavailable during final auditing.
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
        Ok(done) => (Some(done.result), done.commit, Some(done.operation)),
        Err(_) => (None, read.pin.commit_id, None),
    };
    Ok(json!({"schemaVersion":1,"requestId":request.request_id,
        "kind":if error.is_some(){"memory_error"}else{"candidate_proposed"},
        "vaultCommitId":commit,"policyEpoch":read.pin.policy_epoch,"operationId":operation,
        "result":value,"error":error}))
}

//! Fixed offline/current-memory context. No sessions, history or Provider send.
use super::*;
use enouia_memory_context::{CompileInput, compiler};
use enouia_memory_contract::context::{DestinationKind, Purpose};
use enouia_memory_contract::identity::IdentityMetadata;
use enouia_memory_contract::ipc::ContextGetArgs;
use enouia_memory_contract::policy::{EgressRule, egress_rule};
use enouia_memory_contract::session::ClientSurface;

fn audit(
    read: &Read<'_>,
    request: &IpcRequest,
    purpose: Purpose,
    code: Option<MemoryErrorCode>,
) -> Result<()> {
    AuditLog::new(read.vault.managed_root().clone(), DEFAULT_SEGMENT_BYTES).append(&AuditEvent {
        schema_version: SchemaVersion,
        audit_id: AuditId::from_random(read.vault.random_id_bytes()),
        actor: read.principal.clone(),
        operation: request.operation,
        object_refs: read.refs.clone(),
        purpose: Some(purpose),
        destination: Some(DestinationKind::LocalMock),
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
fn metadata(
    read: &Read<'_>,
    reference: &RecordRef,
    revision: Option<Revision>,
) -> Result<(Option<ProjectId>, Sensitivity)> {
    match reference.record_kind {
        RecordKind::Memory => {
            let m: CanonicalMemory =
                read.record(RecordKind::Memory, &reference.record_id, revision)?;
            Ok((m.project_id, m.sensitivity))
        }
        RecordKind::Source => {
            let s: SourceRecord =
                read.record(RecordKind::Source, &reference.record_id, revision)?;
            Ok((None, s.sensitivity))
        }
        RecordKind::Identity => {
            let i: IdentityMetadata =
                read.record(RecordKind::Identity, &reference.record_id, revision)?;
            Ok((None, i.sensitivity))
        }
        _ => Err(fail(MemoryErrorCode::InvalidRequest)),
    }
}
fn check(read: &Read<'_>, reference: &RecordRef, purpose: Purpose) -> Result<()> {
    for revision in [None, Some(reference.revision)] {
        let (project, sensitivity) = metadata(read, reference, revision)?;
        let context = AccessContext {
            principal: read.principal,
            scope: Scope::ContextRead,
            purpose: Some(purpose),
            destination: None,
            resource: &ResourceContext {
                record: reference.clone(),
                project_id: project,
                sensitivity,
            },
        };
        if egress_rule(sensitivity, DestinationKind::LocalMock) == EgressRule::Denied
            || !evaluate(
                &read.policies.iter().collect::<Vec<_>>(),
                &context,
                &read.vault.now()?,
            )
            .is_allow()
        {
            return Err(missing());
        }
    }
    Ok(())
}
pub(super) fn execute(
    vault: &Vault,
    principal: &ActorRef,
    request: &IpcRequest,
    args: ContextGetArgs,
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
    let purpose = args.purpose;
    let mut entered = false;
    let mut compiled = false;
    let result = (|| {
        if args.session_id.is_some()
            || args.branch_id.is_some()
            || args.time_mode.known_at_commit_id.is_some()
            || !matches!(purpose, Purpose::Answer | Purpose::InspectionPreview)
            || args.destination.kind != DestinationKind::LocalMock
            || args.destination.provider_binding_name.is_some()
        {
            return Err(fail(MemoryErrorCode::InvalidRequest));
        }
        audit(&read, request, purpose, None)?;
        read.policies = latest(vault, &read.pin, RecordKind::Policy)?;
        read.tombstones = latest(vault, &read.pin, RecordKind::Tombstone)?;
        let mut index = Index::open(vault)?;
        index.update(vault, &|| false)?;
        let mut input =
            CompileInput::local(&args.query, principal.clone(), request.request_id.clone());
        input.project_ids = args.project_hints;
        input.as_of = args.time_mode.as_of;
        input.max_tokens = args.max_tokens;
        entered = true;
        let saved = compiler::compile_agent_local(vault, &index, &input, purpose)?;
        compiled = true;
        if saved.capsule.request_id != request.request_id
            || saved.capsule.requested_by != *principal
            || saved.capsule.client_surface != ClientSurface::McpClient
            || saved.capsule.purpose != purpose
            || saved.capsule.destination.kind != DestinationKind::LocalMock
            || saved.capsule.destination.provider_binding.is_some()
            || saved.capsule.session_id.is_some()
            || saved.capsule.branch_id.is_some()
        {
            return Err(fail(MemoryErrorCode::StorageFailed));
        }
        read.refs = compiler::resource_refs(&saved.capsule);
        for reference in &read.refs {
            check(&read, reference, purpose)?;
        }
        compiler::validate_saved(vault, principal, &saved.capsule)?;
        let value = json!({"capsule":saved.capsule,"inspectionId":saved.inspection.inspection_id});
        if value.to_string().len() > MAX_RESULT_BYTES {
            return Err(fail(MemoryErrorCode::BudgetExceeded));
        }
        Ok(value)
    })();
    let code = result.as_ref().err().map(VaultError::code);
    let final_audit = audit(&read, request, purpose, code);
    if (compiled
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
    let result = final_audit.and(result).and_then(|value| {
        let fresh = vault.check_fresh(&read.pin, &read.refs)?;
        if fresh.policy_changed || fresh.head.deletion_epoch != read.pin.deletion_epoch {
            return Err(fail(MemoryErrorCode::RevisionConflict));
        }
        vault.host_writer_state()?;
        Ok(value)
    });
    if compiled && result.is_err() {
        return Err(PortFault::OutcomeUnknown);
    }
    let error = result.as_ref().err().map(|e| {
        let mut error = e.error;
        error.retryable = false;
        error
    });
    let value = result.ok();
    let snapshot = value
        .as_ref()
        .map(|v| v["capsule"]["vault_commit_id"].clone())
        .unwrap_or(json!(read.pin.commit_id));
    Ok(
        json!({"schemaVersion":1,"requestId":request.request_id,"kind":if error.is_some(){"memory_error"}else{"context_capsule"},
        "vaultCommitId":snapshot,"policyEpoch":read.pin.policy_epoch,"operationId":null,"result":value,"error":error}),
    )
}

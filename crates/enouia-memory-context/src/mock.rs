//! Offline evidence-only Mock. It reads one saved capsule, never the search
//! index or raw history. Its request body is archived with the exact dispatch.

use crate::{
    compiler::{WRAPPER_BYTES, render, resource_refs},
    session,
    util::*,
};
use enouia_memory_contract::common::{ActorRef, SourceRevisionRef};
use enouia_memory_contract::{
    commit::{ObjectKind, OperationKind, OperationReceipt},
    context::*,
    hash::{Sha256Hex, sha256},
    ids::*,
    json::SchemaVersion,
    ports::{CommitOutcome, StagedObject},
    provider::ProviderRequest,
    record::RecordKind,
};
use enouia_memory_vault::Vault;
use serde::{Deserialize, Serialize};
use serde_json::json;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct MockAnswer {
    pub dispatch_id: DispatchId,
    pub status: String,
    pub statements: Vec<String>,
    pub memories: Vec<MemoryRevisionRef>,
    pub sources: Vec<SourceRevisionRef>,
    pub request_hash: Sha256Hex,
}

fn restore_dispatch(
    vault: &Vault,
    receipt: &OperationReceipt,
    capsule: &ContextCapsule,
    answer: &mut MockAnswer,
) -> Result<()> {
    let reference = receipt
        .records
        .iter()
        .find(|r| r.record_kind == RecordKind::Dispatch)
        .ok_or_else(missing)?;
    // A receipt can appear after this call's initial pin. Read through the
    // current catalog, retaining the original receipt revision and barriers.
    let previous: DispatchRecord = read(
        vault,
        &vault.pin_current()?,
        RecordKind::Dispatch,
        &reference.record_id,
        reference.revision,
    )?;
    let fresh = vault.check_fresh(
        &pin_at(vault, &capsule.vault_commit_id)?,
        &resource_refs(capsule),
    )?;
    if fresh.head.policy_epoch != capsule.policy_epoch
        || fresh.head.deletion_epoch != capsule.deletion_epoch
    {
        return Err(invalid("mock.recompile_required"));
    }
    if previous.capsule_id != capsule.capsule_id || previous.request_id != capsule.request_id {
        return Err(invalid("mock.request_mismatch"));
    }
    answer.dispatch_id = previous.dispatch_id;
    answer.request_hash = previous.request_hash;
    Ok(())
}

/// There is no alternate prompt, evidence, model memory, hidden tool, or
/// provider fallback. Persisted capsule -> rendered request -> exact record.
pub fn answer_saved(
    vault: &Vault,
    actor: &ActorRef,
    id: &CapsuleId,
    output_tokens: u64,
    input_event: Option<&EventId>,
) -> Result<MockAnswer> {
    let current = vault.pin_current()?;
    let capsule: ContextCapsule = read(vault, &current, RecordKind::Capsule, id.as_str(), one())?;
    if &capsule.requested_by != actor
        || capsule.destination.kind != DestinationKind::LocalMock
        || output_tokens == 0
    {
        return Err(denied());
    }
    let pin = pin_at(vault, &capsule.vault_commit_id)?;
    let refs = resource_refs(&capsule);
    let fresh = vault.check_fresh(&pin, &refs)?;
    if fresh.head.policy_epoch != capsule.policy_epoch
        || fresh.head.deletion_epoch != capsule.deletion_epoch
    {
        return Err(invalid("mock.recompile_required"));
    }
    let inspection = latest::<ContextInspection>(vault, &current, RecordKind::Inspection)?
        .into_iter()
        .find(|i| i.capsule_id == *id)
        .ok_or_else(missing)?;
    let messages = render(&capsule)?;
    let used = messages.iter().map(|m| m.text.len() as u64).sum::<u64>() + WRAPPER_BYTES;
    if used != capsule.budget.estimated_tokens
        || used
            .checked_add(output_tokens)
            .and_then(|n| n.checked_add(capsule.budget.safety_margin_tokens))
            .is_none_or(|n| n > capsule.budget.max_tokens)
    {
        return Err(invalid("mock.rendered_budget"));
    }
    let now = vault.now()?;
    let dispatch_id = DispatchId::from_random(vault.random_id_bytes());
    let request = ProviderRequest {
        dispatch_id: dispatch_id.clone(),
        capsule_id: id.clone(),
        destination: capsule.destination.clone(),
        messages,
        tools: vec![],
        output: OutputConfig {
            max_output_tokens: output_tokens,
            streaming: false,
        },
    };
    let mut dispatch = DispatchRecord {
        schema_version: SchemaVersion,
        dispatch_id: dispatch_id.clone(),
        capsule_id: id.clone(),
        inspection_id: inspection.inspection_id.clone(),
        request_id: capsule.request_id.clone(),
        destination: capsule.destination.clone(),
        request_hash: request.payload_hash(),
        messages: request
            .messages
            .iter()
            .enumerate()
            .map(|(i, m)| DispatchMessage {
                role: m.role,
                content_hash: sha256(m.text.as_bytes()),
                size_bytes: m.text.len() as u64,
                resource_refs: if i == 0 { refs.clone() } else { vec![] },
            })
            .collect(),
        tools: vec![],
        output: request.output.clone(),
        egress: EgressDecision {
            policy_epoch: fresh.head.policy_epoch,
            deletion_epoch: fresh.head.deletion_epoch,
            egress_policy_id: None,
            egress_approval_id: None,
            checked_at: now.clone(),
        },
        state: DispatchState::Completed,
        prepared_at: now.clone(),
        sent_at: Some(now.clone()),
        completed_at: Some(now),
    };
    if !request.verify_against(&dispatch).is_empty() {
        return Err(invalid("mock.request_mismatch"));
    }
    let conflicted = capsule
        .memory_items()
        .any(|m| m.currency == Currency::Conflicted);
    let implementation_question = ["实现", "上线", "implemented", "released", "deployed"]
        .iter()
        .any(|term| capsule.query.to_lowercase().contains(term));
    let uncertain = capsule.memory_items().any(|m| {
        matches!(
            m.currency,
            Currency::NeedsReverification | Currency::HistoricalOnly
        )
    });
    let mut answer = MockAnswer {
        dispatch_id: dispatch_id.clone(),
        status: if capsule.memory_items().next().is_none() {
            "no_supported_evidence"
        } else if conflicted {
            "conflicted"
        } else if implementation_question {
            "implementation_unverified"
        } else if uncertain {
            "needs_reverification"
        } else {
            "supported_evidence"
        }
        .into(),
        statements: capsule.memory_items().map(|m| m.content.clone()).collect(),
        memories: capsule
            .memory_items()
            .map(|m| MemoryRevisionRef {
                memory_id: m.memory_id.clone(),
                revision: m.revision,
            })
            .collect(),
        sources: capsule
            .provenance
            .iter()
            .map(|p| SourceRevisionRef {
                source_id: p.source_id.clone(),
                source_revision: p.source_revision,
            })
            .collect(),
        request_hash: dispatch.request_hash.clone(),
    };
    // Mock output is also measured in UTF-8 bytes. It never truncates a claim
    // or its qualification: a too-small output budget is a clear failure.
    if bytes(&answer)?.len() as u64 > output_tokens {
        return Err(enouia_memory_vault::VaultError::new(
            enouia_memory_contract::MemoryErrorCode::BudgetExceeded,
            enouia_memory_vault::Fault::Contract(vec!["mock.output_budget"]),
        ));
    }
    if let Some(input) = input_event {
        let event: enouia_memory_contract::session::SessionEvent = read(
            vault,
            &current,
            RecordKind::SessionEvent,
            input.as_str(),
            one(),
        )?;
        if capsule.session_id.as_ref() != Some(&event.session_id)
            || capsule.branch_id.as_ref() != Some(&event.branch_id)
            || event.request_id.as_ref() != Some(&capsule.request_id)
            || event.kind != enouia_memory_contract::session::EventKind::UserMessage
            || session::text(vault, &current, &event)? != capsule.query
        {
            return Err(invalid("mock.session_input_mismatch"));
        }
    }
    // The body is a private, content-addressed Session object. The dispatch
    // extension is unnecessary: its message hashes locate the exact objects.
    let objects: Vec<_> = request
        .messages
        .iter()
        .map(|m| StagedObject {
            hash: sha256(m.text.as_bytes()),
            kind: ObjectKind::SessionContent,
            bytes: m.text.as_bytes().to_vec(),
        })
        .collect();
    let payload = sha256(&bytes(
        &json!({"capsule":id,"output":output_tokens,"input":input_event}),
    )?);
    let key = format!("mock:{}", id);
    if let Some((_, receipt)) = replay(
        vault,
        actor,
        OperationKind::SessionAppend,
        key.as_bytes(),
        &payload,
    )? {
        restore_dispatch(vault, &receipt, &capsule, &mut answer)?;
        // A crash between dispatch publication and reply publication leaves
        // the turn pending. Retry repairs that last durable step exactly once.
        if let Some(input) = input_event {
            let text = String::from_utf8(bytes(&answer)?).map_err(|_| invalid("mock.utf8"))?;
            session::append_output(
                vault,
                actor,
                input,
                enouia_memory_contract::session::EventKind::AssistantCompleted,
                Some(&text),
                format!("mock-reply:{id}").as_bytes(),
            )?;
        }
        return Ok(answer);
    }
    // An immutable local Mock dispatch records the completed pure operation;
    // external Prepared/Sent/OutcomeUnknown transitions belong to MV-7.
    let checked = vault.check_fresh(&pin, &refs)?;
    if checked.head.policy_epoch != capsule.policy_epoch
        || checked.head.deletion_epoch != capsule.deletion_epoch
    {
        return Err(invalid("mock.recompile_required"));
    }
    dispatch.egress.checked_at = vault.now()?;
    let outcome = commit(
        vault,
        actor,
        &checked.head,
        OperationKind::SessionAppend,
        key.as_bytes(),
        payload,
        vec![staged(
            RecordKind::Dispatch,
            dispatch_id.as_str(),
            one(),
            &dispatch,
        )?],
        objects,
    )?;
    if let CommitOutcome::Replayed { receipt, .. } = outcome {
        restore_dispatch(vault, &receipt, &capsule, &mut answer)?;
    }
    if let Some(input) = input_event {
        let text = String::from_utf8(bytes(&answer)?).map_err(|_| invalid("mock.utf8"))?;
        session::append_output(
            vault,
            actor,
            input,
            enouia_memory_contract::session::EventKind::AssistantCompleted,
            Some(&text),
            format!("mock-reply:{id}").as_bytes(),
        )?;
    }
    Ok(answer)
}

/// Saved exact body inspector; no reconstruction from a newer memory revision.
pub fn inspect_request(
    vault: &Vault,
    actor: &ActorRef,
    id: &DispatchId,
) -> Result<ProviderRequest> {
    let pin = vault.pin_current()?;
    let dispatch: DispatchRecord = read(vault, &pin, RecordKind::Dispatch, id.as_str(), one())?;
    let capsule: ContextCapsule = read(
        vault,
        &pin,
        RecordKind::Capsule,
        dispatch.capsule_id.as_str(),
        one(),
    )?;
    if &capsule.requested_by != actor {
        return Err(denied());
    }
    let fresh = vault.check_fresh(
        &pin_at(vault, &capsule.vault_commit_id)?,
        &resource_refs(&capsule),
    )?;
    if fresh.head.policy_epoch != capsule.policy_epoch
        || fresh.head.deletion_epoch != capsule.deletion_epoch
    {
        return Err(invalid("mock.recompile_required"));
    }
    let messages = dispatch
        .messages
        .iter()
        .map(|m| {
            let body = vault.read_object(&pin, &m.content_hash)?;
            Ok(enouia_memory_contract::provider::ProviderMessage {
                role: m.role,
                text: String::from_utf8(body).map_err(|_| invalid("mock.utf8"))?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let request = ProviderRequest {
        dispatch_id: id.clone(),
        capsule_id: capsule.capsule_id,
        destination: dispatch.destination.clone(),
        messages,
        tools: vec![],
        output: dispatch.output.clone(),
    };
    if !request.verify_against(&dispatch).is_empty() {
        return Err(invalid("mock.request_mismatch"));
    }
    Ok(request)
}

//! Durable MV-7 integration without a new database or private side channel.
//! Admission uses the Vault's optimistic writer and Session revision. Exact
//! message bodies are ordinary SessionContent objects and survive backup.
use crate::{
    Result,
    client::{InvocationJournal, Limits, PreparedCall},
    codec::Api,
    egress::{EgressSnapshot, SendGuard},
    error,
};
use enouia_memory_context::{compiler, session};
use enouia_memory_contract::{
    MemoryError, MemoryErrorCode,
    approval::{ApprovalBinding, ApprovalRecord},
    commit::{AuditDecision, AuditEvent, ObjectKind, OperationKind},
    common::{ActorRef, ActorType, TrustedSurface},
    context::*,
    hash::sha256,
    ids::*,
    ipc::Operation,
    json::{Revision, SchemaVersion, canonical_bytes},
    policy::{PolicyRecord, ResourceContext},
    ports::{CommitOutcome, CommitRequest, IdempotencyScope, StagedObject, StagedRecord},
    provider::{
        FinishReason, Invocation, InvocationState, ProviderCapabilities, ProviderRequest,
        ProviderResponse,
    },
    record::{AnyRecord, Record, RecordKind, RecordRef, parse_record},
    session::{EventKind, SessionEvent, SessionRecord},
    time::Timestamp,
};
use enouia_memory_vault::{Vault, audit::AuditLog};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    sync::atomic::{AtomicBool, Ordering},
};

pub(crate) fn one() -> Revision {
    Revision::new(1).expect("one")
}
pub(crate) fn bytes(v: &impl Serialize) -> Result<Vec<u8>> {
    canonical_bytes(&serde_json::to_value(v).map_err(|_| error(MemoryErrorCode::InvalidRequest))?)
        .map_err(|_| error(MemoryErrorCode::InvalidRequest))
}
pub(crate) fn read<T: Record>(vault: &Vault, kind: RecordKind, id: &str) -> Result<T> {
    let pin = vault.pin_current().map_err(|e| e.error)?;
    let entry = vault
        .record_entry(&pin, kind, id)
        .map_err(|e| e.error)?
        .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
    let data = vault
        .read_record(&pin, &RecordRef::new(kind, id, entry.revision))
        .map_err(|e| e.error)?;
    parse_record(&data).map_err(|_| error(MemoryErrorCode::InvalidRequest))
}
pub(crate) fn staged(
    kind: RecordKind,
    id: &str,
    revision: Revision,
    v: &impl Serialize,
) -> Result<StagedRecord> {
    Ok(StagedRecord {
        record_kind: kind,
        record_id: id.into(),
        revision,
        bytes: bytes(v)?,
    })
}
pub(crate) fn commit(
    vault: &Vault,
    actor: &ActorRef,
    pin: &enouia_memory_contract::ports::CommitPin,
    key: &str,
    records: Vec<StagedRecord>,
    objects: Vec<StagedObject>,
) -> Result<bool> {
    let payload = sha256(&bytes(
        &serde_json::json!({"records":records.iter().map(|r| sha256(&r.bytes)).collect::<Vec<_>>(),"objects":objects.iter().map(|o| &o.hash).collect::<Vec<_>>() }),
    )?);
    let expected = records
        .iter()
        .map(|r| {
            (
                r.record_kind,
                r.record_id.clone(),
                Revision::new(r.revision.get() - 1),
            )
        })
        .collect();
    let outcome = vault
        .commit(CommitRequest {
            commit_id: CommitId::from_random(vault.random_id_bytes()),
            expected_commit_id: Some(pin.commit_id.clone()),
            principal: actor.clone(),
            operation_kind: OperationKind::SessionAppend,
            idempotency: IdempotencyScope {
                principal_id: actor.actor_id.clone(),
                operation_kind: OperationKind::SessionAppend,
                key_hash: sha256(key.as_bytes()),
            },
            request_payload_hash: payload,
            expected_revisions: expected,
            records,
            objects,
        })
        .map_err(|e| e.error)?;
    Ok(matches!(outcome, CommitOutcome::Committed { .. }))
}

#[derive(Clone, Copy, Debug)]
pub struct PriceLimit {
    pub input_microusd_per_million: u64,
    pub output_microusd_per_million: u64,
    pub max_reserved_microusd: u64,
}
#[derive(Clone, Copy, Debug)]
pub struct Quota {
    pub max_attempts: usize,
    /// Per-Session admission ceiling. Each earlier attempt debits at least its
    /// reservation, or more if reported token usage exceeds that estimate.
    pub max_reserved_tokens: u64,
    pub price_limit: Option<PriceLimit>,
}
impl Default for Quota {
    fn default() -> Self {
        Self {
            max_attempts: 10,
            max_reserved_tokens: 100_000,
            price_limit: None,
        }
    }
}

#[derive(Clone, Debug)]
pub struct CallOptions {
    pub egress_policy: PolicyId,
    pub approval: Option<ApprovalId>,
    pub streaming: bool,
    pub output_tokens: u64,
}
impl CallOptions {
    pub fn text(egress_policy: PolicyId, output_tokens: u64) -> Self {
        Self {
            egress_policy,
            approval: None,
            streaming: false,
            output_tokens,
        }
    }
}

/// An owner-authenticated native handle. `enabled` is independent of Mock and
/// must be turned off (and cancellation signalled) before native lock/close.
pub struct VaultAdapter<'a> {
    pub vault: &'a Vault,
    pub owner: ActorRef,
    pub input_event: EventId,
    pub enabled: &'a AtomicBool,
    pub quota: Quota,
}
impl VaultAdapter<'_> {
    fn output_guard(&self, dispatch: &DispatchRecord) -> Result<session::OutputGuard> {
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        let mut sensitivity = self.input()?.sensitivity;
        for r in dispatch.resource_refs() {
            let record = self.vault.read_parsed(&pin, r).map_err(|e| e.error)?;
            let level = match record {
                AnyRecord::Memory(r) => r.sensitivity,
                AnyRecord::Identity(r) => r.sensitivity,
                AnyRecord::Source(r) => r.sensitivity,
                AnyRecord::SessionEvent(r) => r.sensitivity,
                AnyRecord::Checkpoint(r) => r.sensitivity,
                _ => return Err(error(MemoryErrorCode::PermissionDenied)),
            };
            sensitivity = sensitivity.max(level);
        }
        Ok(session::OutputGuard {
            policy_epoch: dispatch.egress.policy_epoch,
            deletion_epoch: dispatch.egress.deletion_epoch,
            sensitivity,
        })
    }
    fn owner_check(&self) -> Result<()> {
        if self.owner != self.vault.descriptor().created_by
            || self.owner.actor_type != ActorType::Owner
        {
            return Err(error(MemoryErrorCode::PermissionDenied));
        }
        if !self.enabled.load(Ordering::SeqCst) {
            return Err(error(MemoryErrorCode::VaultLocked));
        }
        Ok(())
    }
    fn input(&self) -> Result<SessionEvent> {
        self.owner_check()?;
        let input: SessionEvent = read(
            self.vault,
            RecordKind::SessionEvent,
            self.input_event.as_str(),
        )?;
        if input.kind != EventKind::UserMessage || input.actor != self.owner {
            return Err(error(MemoryErrorCode::PermissionDenied));
        }
        Ok(input)
    }
    pub fn prepare_saved(
        &self,
        api: Api,
        capsule_id: &CapsuleId,
        capabilities: &ProviderCapabilities,
        limits: Limits,
        options: CallOptions,
    ) -> Result<PreparedCall> {
        let input = self.input()?;
        let capsule: ContextCapsule = read(self.vault, RecordKind::Capsule, capsule_id.as_str())?;
        compiler::validate_saved(self.vault, &self.owner, &capsule).map_err(|e| e.error)?;
        if capsule.session_id.as_ref() != Some(&input.session_id)
            || capsule.branch_id.as_ref() != Some(&input.branch_id)
            || capsule.request_id
                != input
                    .request_id
                    .clone()
                    .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?
            || !capsule
                .recent_turns
                .iter()
                .any(|t| t.event_id == input.event_id && t.text == capsule.query)
        {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        if session::text(self.vault, &pin, &input).map_err(|e| e.error)? != capsule.query {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        let inspections = self
            .vault
            .record_entries(&pin, RecordKind::Inspection)
            .map_err(|e| e.error)?;
        let mut inspection_id = None;
        for e in inspections {
            let inspection: ContextInspection =
                read(self.vault, RecordKind::Inspection, &e.record_id)?;
            if inspection.capsule_id == *capsule_id {
                inspection_id = Some(inspection.inspection_id);
                break;
            }
        }
        let request = ProviderRequest {
            dispatch_id: DispatchId::from_random(self.vault.random_id_bytes()),
            capsule_id: capsule_id.clone(),
            destination: capsule.destination.clone(),
            messages: compiler::render(&capsule).map_err(|e| e.error)?,
            tools: vec![],
            output: OutputConfig {
                max_output_tokens: options.output_tokens,
                streaming: options.streaming,
            },
        };
        let refs = compiler::resource_refs(&capsule);
        let now = self.vault.now().map_err(|e| e.error)?;
        let dispatch = DispatchRecord {
            schema_version: SchemaVersion,
            dispatch_id: request.dispatch_id.clone(),
            capsule_id: capsule_id.clone(),
            inspection_id: inspection_id.ok_or_else(|| error(MemoryErrorCode::NotFound))?,
            request_id: capsule.request_id,
            destination: request.destination.clone(),
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
                policy_epoch: capsule.policy_epoch,
                deletion_epoch: capsule.deletion_epoch,
                egress_policy_id: Some(options.egress_policy),
                egress_approval_id: options.approval,
                checked_at: now.clone(),
            },
            state: DispatchState::Prepared,
            prepared_at: now,
            sent_at: None,
            completed_at: None,
        };
        let call = crate::client::prepare(api, &request, &dispatch, capabilities, limits)?;
        self.owner_check()?;
        Ok(call)
    }
    /// Trusted native confirmation after exact wire inspection. The logical
    /// approval binds resources/destination and the diff hash binds wire bytes.
    /// Returning a new PreparedCall does not send or activate network access.
    pub fn approve(
        &self,
        mut call: PreparedCall,
        inspected_hash: &enouia_memory_contract::hash::Sha256Hex,
        surface: TrustedSurface,
        nonce: &str,
    ) -> Result<PreparedCall> {
        self.owner_check()?;
        if &call.wire.hash() != inspected_hash || call.dispatch.egress.egress_approval_id.is_some()
        {
            return Err(error(MemoryErrorCode::PermissionDenied));
        }
        let issued_at = self.vault.now().map_err(|e| e.error)?;
        let expires_at = Timestamp::from_unix_ms(issued_at.unix_ms() + 15 * 60 * 1000)
            .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?;
        let approval = ApprovalRecord {
            schema_version: SchemaVersion,
            approval_id: ApprovalId::from_random(self.vault.random_id_bytes()),
            approved_by: self.owner.clone(),
            trusted_surface: surface,
            approval_nonce: nonce.into(),
            approved_diff_hash: inspected_hash.clone(),
            issued_at,
            expires_at: Some(expires_at),
            binding: ApprovalBinding::Egress {
                request_id: call.dispatch.request_id.clone(),
                capsule_id: call.dispatch.capsule_id.clone(),
                payload_hash: call.dispatch.request_hash.clone(),
                destination: call.dispatch.destination.clone(),
                resources: call.dispatch.resource_refs().into_iter().cloned().collect(),
                policy_id: call
                    .dispatch
                    .egress
                    .egress_policy_id
                    .clone()
                    .ok_or_else(|| error(MemoryErrorCode::PermissionDenied))?,
                policy_epoch: call.dispatch.egress.policy_epoch,
            },
        };
        if !approval.validate().is_empty() {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        self.owner_check()?;
        commit(
            self.vault,
            &self.owner,
            &pin,
            &format!("provider-approval:{}", approval.approval_id),
            vec![staged(
                RecordKind::Approval,
                approval.approval_id.as_str(),
                one(),
                &approval,
            )?],
            vec![],
        )?;
        call.dispatch.egress.egress_approval_id = Some(approval.approval_id);
        Ok(call)
    }
    /// Reads metadata without a network attempt. Unknown remains unknown after
    /// crash; the owner must create a new input to authorize another attempt.
    pub fn invocations(&self) -> Result<Vec<Invocation>> {
        let input = self.input()?;
        let header: SessionRecord =
            read(self.vault, RecordKind::Session, input.session_id.as_str())?;
        self.owner_check()?;
        Ok(header.provider_invocations)
    }
    /// Reads the archived HTTP body, without credentials or an HTTP attempt.
    /// Owner/freshness/deletion barriers still apply to protected inspection.
    pub fn inspect_saved(&self, id: &DispatchId) -> Result<crate::codec::WireRequest> {
        let row = self
            .invocations()?
            .into_iter()
            .find(|r| &r.dispatch_id == id)
            .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
        let dispatch: DispatchRecord = read(self.vault, RecordKind::Dispatch, id.as_str())?;
        let capsule: ContextCapsule = read(
            self.vault,
            RecordKind::Capsule,
            dispatch.capsule_id.as_str(),
        )?;
        compiler::validate_saved(self.vault, &self.owner, &capsule).map_err(|e| e.error)?;
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        let mut messages = vec![];
        for m in &dispatch.messages {
            let body = self
                .vault
                .read_object(&pin, &m.content_hash)
                .map_err(|e| e.error)?;
            messages.push(enouia_memory_contract::provider::ProviderMessage {
                role: m.role,
                text: String::from_utf8(body).map_err(|_| error(MemoryErrorCode::StorageFailed))?,
            });
        }
        let request = ProviderRequest {
            dispatch_id: id.clone(),
            capsule_id: dispatch.capsule_id.clone(),
            destination: dispatch.destination.clone(),
            messages,
            tools: vec![],
            output: dispatch.output.clone(),
        };
        if !request.verify_against(&dispatch).is_empty() {
            return Err(error(MemoryErrorCode::StorageFailed));
        }
        let api = match dispatch
            .destination
            .provider_binding
            .as_ref()
            .map(|b| b.provider.as_str())
        {
            Some("openai") => Api::OpenAiResponses,
            Some("anthropic") => Api::AnthropicMessages,
            _ => return Err(error(MemoryErrorCode::ProviderUnavailable)),
        };
        let wire = crate::codec::encode(api, &request)?;
        let archived = self
            .vault
            .read_object(&pin, &row.wire_hash)
            .map_err(|e| e.error)?;
        if wire.body() != archived
            || archived.len() as u64 != row.wire_size_bytes
            || wire.hash() != row.wire_hash
        {
            return Err(error(MemoryErrorCode::StorageFailed));
        }
        self.owner_check()?;
        Ok(wire)
    }
    /// Repairs a crash between terminal Session publication and ledger update.
    /// No response receipt means no repair and no permission to resend.
    pub fn recover_local_outcome(&self, id: &DispatchId) -> Result<bool> {
        let input = self.input()?;
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        let mut header: SessionRecord =
            read(self.vault, RecordKind::Session, input.session_id.as_str())?;
        let row = header
            .provider_invocations
            .iter_mut()
            .find(|r| &r.dispatch_id == id)
            .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
        if row.state != InvocationState::OutcomeUnknown {
            return Ok(false);
        }
        let scope = IdempotencyScope {
            principal_id: self.owner.actor_id.clone(),
            operation_kind: OperationKind::SessionAppend,
            key_hash: sha256(format!("provider-terminal:{id}").as_bytes()),
        };
        let Some((_, _, receipt)) = self.vault.find_receipt(&scope).map_err(|e| e.error)? else {
            return Ok(false);
        };
        let r = receipt
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::SessionEvent)
            .ok_or_else(|| error(MemoryErrorCode::StorageFailed))?;
        let event: SessionEvent = read(self.vault, RecordKind::SessionEvent, &r.record_id)?;
        if event.turn_id != input.turn_id || event.session_id != input.session_id {
            return Err(error(MemoryErrorCode::PermissionDenied));
        }
        row.state = match event.kind {
            EventKind::AssistantCompleted => InvocationState::Completed,
            EventKind::TurnCancelled => InvocationState::Cancelled,
            EventKind::TurnFailed => InvocationState::Failed,
            _ => return Err(error(MemoryErrorCode::StorageFailed)),
        };
        row.terminal_event_id = Some(event.event_id);
        header.revision = Revision::new(header.revision.get() + 1)
            .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?;
        header.updated_at = self.vault.now().map_err(|e| e.error)?;
        self.owner_check()?;
        commit(
            self.vault,
            &self.owner,
            &pin,
            &format!("provider-recovery:{id}"),
            vec![staged(
                RecordKind::Session,
                header.session_id.as_str(),
                header.revision,
                &header,
            )?],
            vec![],
        )
    }
    pub fn saved_response(&self, id: &DispatchId) -> Result<Option<ProviderResponse>> {
        let row = self
            .invocations()?
            .into_iter()
            .find(|r| &r.dispatch_id == id)
            .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
        if !matches!(
            row.state,
            InvocationState::Completed | InvocationState::Length
        ) {
            return Ok(None);
        }
        let event: SessionEvent = read(
            self.vault,
            RecordKind::SessionEvent,
            row.terminal_event_id
                .as_ref()
                .ok_or_else(|| error(MemoryErrorCode::NotFound))?
                .as_str(),
        )?;
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        let response = ProviderResponse {
            text: session::text(self.vault, &pin, &event).map_err(|e| e.error)?,
            finish: if row.state == InvocationState::Completed {
                FinishReason::Completed
            } else {
                FinishReason::Length
            },
            tool_requests: vec![],
            input_tokens: row.input_tokens,
            output_tokens: row.output_tokens,
        };
        self.owner_check()?;
        Ok(Some(response))
    }
}

impl SendGuard for VaultAdapter<'_> {
    fn check(&self, dispatch: &DispatchRecord, audit: bool) -> Result<()> {
        self.owner_check()?;
        if !enouia_memory_vault::backup::network_allowed(self.vault).map_err(|e| e.error)? {
            return Err(error(MemoryErrorCode::VaultRecovering));
        }
        let input = self.input()?;
        let capsule: ContextCapsule = read(
            self.vault,
            RecordKind::Capsule,
            dispatch.capsule_id.as_str(),
        )?;
        compiler::validate_saved(self.vault, &self.owner, &capsule).map_err(|e| e.error)?;
        if !capsule
            .recent_turns
            .iter()
            .any(|t| t.event_id == input.event_id)
            || capsule.requested_by != self.owner
        {
            return Err(error(MemoryErrorCode::PermissionDenied));
        }
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        let mut policies: Vec<PolicyRecord> = vec![];
        for e in self
            .vault
            .record_entries(&pin, RecordKind::Policy)
            .map_err(|e| e.error)?
        {
            policies.push(read(self.vault, RecordKind::Policy, &e.record_id)?);
        }
        let mut resources = vec![];
        for r in dispatch.resource_refs() {
            let record = self.vault.read_parsed(&pin, r).map_err(|e| e.error)?;
            let (sensitivity, project_id) = match record {
                AnyRecord::Memory(r) => (r.sensitivity, r.project_id),
                AnyRecord::Identity(r) => (r.sensitivity, None),
                AnyRecord::Source(r) => (r.sensitivity, None),
                AnyRecord::SessionEvent(r) => (r.sensitivity, None),
                AnyRecord::Checkpoint(r) => (r.sensitivity, None),
                AnyRecord::Attachment(r) => (r.sensitivity, None),
                _ => return Err(error(MemoryErrorCode::PermissionDenied)),
            };
            resources.push(ResourceContext {
                record: r.clone(),
                project_id,
                sensitivity,
            });
        }
        let approval = dispatch
            .egress
            .egress_approval_id
            .as_ref()
            .map(|id| read(self.vault, RecordKind::Approval, id.as_str()))
            .transpose()?;
        let mut consumed_approvals = BTreeSet::new();
        for e in self
            .vault
            .record_entries(&pin, RecordKind::Dispatch)
            .map_err(|e| e.error)?
        {
            let old: DispatchRecord = read(self.vault, RecordKind::Dispatch, &e.record_id)?;
            if old.dispatch_id != dispatch.dispatch_id
                && let Some(id) = old.egress.egress_approval_id
            {
                consumed_approvals.insert(id);
            }
        }
        let now = self.vault.now().map_err(|e| e.error)?;
        EgressSnapshot {
            principal: self.owner.clone(),
            purpose: capsule.purpose,
            policy_epoch: pin.policy_epoch,
            deletion_epoch: pin.deletion_epoch,
            resources,
            policies,
            approval,
            consumed_approvals,
            checked_at: now.clone(),
        }
        .authorize(dispatch)?;
        if audit {
            AuditLog::new(self.vault.managed_root().clone(), 1024 * 1024)
                .append(&AuditEvent {
                    schema_version: SchemaVersion,
                    audit_id: AuditId::from_random(self.vault.random_id_bytes()),
                    actor: self.owner.clone(),
                    operation: Operation::ContextGet,
                    object_refs: dispatch.resource_refs().into_iter().cloned().collect(),
                    purpose: Some(capsule.purpose),
                    destination: Some(dispatch.destination.kind),
                    policy_epoch: pin.policy_epoch,
                    decision: AuditDecision::Allow,
                    error_code: None,
                    request_id: dispatch.request_id.clone(),
                    created_at: now,
                })
                .map_err(|e| e.error)?;
        }
        Ok(())
    }
}
impl InvocationJournal for VaultAdapter<'_> {
    fn claim(&self, call: &PreparedCall) -> Result<bool> {
        self.check(&call.dispatch, false)?;
        let input = self.input()?;
        let pin = self.vault.pin_current().map_err(|e| e.error)?;
        if pin.policy_epoch != call.dispatch.egress.policy_epoch
            || pin.deletion_epoch != call.dispatch.egress.deletion_epoch
        {
            return Err(error(MemoryErrorCode::RevisionConflict));
        }
        let mut header: SessionRecord =
            read(self.vault, RecordKind::Session, input.session_id.as_str())?;
        if header.provider_invocations.iter().any(|r| {
            r.input_event_id == input.event_id || r.dispatch_id == call.dispatch.dispatch_id
        }) {
            return Ok(false);
        }
        let reserved = call
            .estimated_input_tokens
            .checked_add(call.dispatch.output.max_output_tokens)
            .ok_or_else(|| error(MemoryErrorCode::BudgetExceeded))?;
        let spent = header
            .provider_invocations
            .iter()
            .try_fold(0u64, |sum, r| {
                // Missing usage remains unknown in the ledger. For admission
                // retain the whole reservation floor, including failed/unknown
                // attempts, and never ignore a higher reported token count.
                let known = r
                    .input_tokens
                    .unwrap_or(0)
                    .checked_add(r.output_tokens.unwrap_or(0))?;
                sum.checked_add(r.reserved_tokens.max(known))
            })
            .ok_or_else(|| error(MemoryErrorCode::BudgetExceeded))?;
        let job = header
            .extraction_jobs
            .iter()
            .find(|j| j.input_event_id == input.event_id);
        if let Some(job) = job {
            let capsule: ContextCapsule = read(
                self.vault,
                RecordKind::Capsule,
                call.dispatch.capsule_id.as_str(),
            )?;
            if job.binding != call.wire.binding || capsule.purpose != Purpose::Extraction {
                return Err(error(MemoryErrorCode::PermissionDenied));
            }
        }
        if job
            .is_some_and(|j| j.state != enouia_memory_contract::extraction::ExtractionState::Ready)
        {
            return Err(error(MemoryErrorCode::Cancelled));
        }
        let token_limit = job.map_or(self.quota.max_reserved_tokens, |j| {
            self.quota.max_reserved_tokens.min(j.max_reserved_tokens)
        });
        if header.provider_invocations.len() >= self.quota.max_attempts
            || spent.checked_add(reserved).is_none_or(|n| n > token_limit)
        {
            return Err(error(MemoryErrorCode::BudgetExceeded));
        }
        let reserved_cost_microusd = match self.quota.price_limit {
            Some(p) => {
                let cost = ((call.estimated_input_tokens as u128
                    * p.input_microusd_per_million as u128)
                    + (call.dispatch.output.max_output_tokens as u128
                        * p.output_microusd_per_million as u128))
                    .div_ceil(1_000_000);
                let cost =
                    u64::try_from(cost).map_err(|_| error(MemoryErrorCode::BudgetExceeded))?;
                let prior = header
                    .provider_invocations
                    .iter()
                    .try_fold(0u64, |sum, r| sum.checked_add(r.reserved_cost_microusd?))
                    .ok_or_else(|| error(MemoryErrorCode::UnsupportedBudget))?;
                let cost_limit = job
                    .and_then(|j| j.max_reserved_cost_microusd)
                    .map_or(p.max_reserved_microusd, |n| n.min(p.max_reserved_microusd));
                if prior.checked_add(cost).is_none_or(|n| n > cost_limit) {
                    return Err(error(MemoryErrorCode::BudgetExceeded));
                }
                Some(cost)
            }
            None if job.is_some_and(|j| j.max_reserved_cost_microusd.is_some()) => {
                return Err(error(MemoryErrorCode::UnsupportedBudget));
            }
            None => None,
        };
        header.provider_invocations.push(Invocation {
            dispatch_id: call.dispatch.dispatch_id.clone(),
            input_event_id: input.event_id,
            wire_hash: call.wire.hash(),
            wire_size_bytes: call.wire.body().len() as u64,
            reserved_tokens: reserved,
            reserved_cost_microusd,
            state: InvocationState::OutcomeUnknown,
            terminal_event_id: None,
            input_tokens: None,
            output_tokens: None,
            error_code: None,
        });
        header.revision = Revision::new(header.revision.get() + 1)
            .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?;
        header.updated_at = self.vault.now().map_err(|e| e.error)?;
        let capsule: ContextCapsule = read(
            self.vault,
            RecordKind::Capsule,
            call.dispatch.capsule_id.as_str(),
        )?;
        let messages = compiler::render(&capsule).map_err(|e| e.error)?;
        let actual = ProviderRequest {
            dispatch_id: call.dispatch.dispatch_id.clone(),
            capsule_id: call.dispatch.capsule_id.clone(),
            destination: call.dispatch.destination.clone(),
            messages,
            tools: vec![],
            output: call.dispatch.output.clone(),
        };
        if !actual.verify_against(&call.dispatch).is_empty()
            || crate::codec::encode(call.wire.api, &actual)?.hash() != call.wire.hash()
        {
            return Err(error(MemoryErrorCode::PermissionDenied));
        }
        let mut objects: Vec<_> = actual
            .messages
            .iter()
            .map(|m| StagedObject {
                kind: ObjectKind::SessionContent,
                hash: sha256(m.text.as_bytes()),
                bytes: m.text.as_bytes().to_vec(),
            })
            .collect();
        objects.push(StagedObject {
            kind: ObjectKind::SessionContent,
            hash: call.wire.hash(),
            bytes: call.wire.body().to_vec(),
        });
        self.check(&call.dispatch, false)?;
        let mut dispatch = call.dispatch.clone();
        dispatch.state = DispatchState::OutcomeUnknown;
        dispatch.sent_at = Some(header.updated_at.clone());
        dispatch.egress.checked_at = header.updated_at.clone();
        commit(
            self.vault,
            &self.owner,
            &pin,
            &format!("provider-admission:{}", dispatch.dispatch_id),
            vec![
                staged(
                    RecordKind::Session,
                    header.session_id.as_str(),
                    header.revision,
                    &header,
                )?,
                staged(
                    RecordKind::Dispatch,
                    dispatch.dispatch_id.as_str(),
                    one(),
                    &dispatch,
                )?,
            ],
            objects,
        )
    }
    fn chunk(&self, dispatch: &DispatchRecord, sequence: u64, text: &str) -> Result<()> {
        self.check(dispatch, false)?;
        session::append_output_with_guard(
            self.vault,
            &self.owner,
            &self.input_event,
            EventKind::AssistantChunk,
            Some(text),
            format!("provider-chunk:{}:{sequence}", dispatch.dispatch_id).as_bytes(),
            Some(self.output_guard(dispatch)?),
        )
        .map_err(|e| e.error)?;
        Ok(())
    }
    fn finish(
        &self,
        dispatch: &DispatchRecord,
        response: Option<&ProviderResponse>,
        failure: Option<MemoryError>,
    ) -> Result<()> {
        self.owner_check()?;
        let (state, kind, text) = match response {
            Some(r) if r.finish == FinishReason::Completed => (
                InvocationState::Completed,
                EventKind::AssistantCompleted,
                Some(r.text.as_str()),
            ),
            Some(r) if r.finish == FinishReason::Length => (
                InvocationState::Length,
                EventKind::TurnFailed,
                Some(r.text.as_str()),
            ),
            _ if failure.is_some_and(|e| e.code == MemoryErrorCode::Cancelled) => {
                (InvocationState::Cancelled, EventKind::TurnCancelled, None)
            }
            _ => (InvocationState::Failed, EventKind::TurnFailed, None),
        };
        // Length is incomplete but must keep the returned bytes. Save an
        // explicit partial chunk before the terminal failed turn.
        if state == InvocationState::Length
            && !dispatch.output.streaming
            && let Some(r) = response
            && !r.text.is_empty()
        {
            session::append_output_with_guard(
                self.vault,
                &self.owner,
                &self.input_event,
                EventKind::AssistantChunk,
                Some(&r.text),
                format!("provider-length:{}", dispatch.dispatch_id).as_bytes(),
                Some(self.output_guard(dispatch)?),
            )
            .map_err(|e| e.error)?;
        }
        session::append_invocation_output(
            self.vault,
            &self.owner,
            &self.input_event,
            kind,
            text,
            format!("provider-terminal:{}", dispatch.dispatch_id).as_bytes(),
            if response.is_some() {
                Some(self.output_guard(dispatch)?)
            } else {
                None
            },
            &session::InvocationCompletion {
                dispatch_id: dispatch.dispatch_id.clone(),
                state,
                input_tokens: response.and_then(|r| r.input_tokens),
                output_tokens: response.and_then(|r| r.output_tokens),
                error_code: failure.map(|e| e.code),
            },
        )
        .map_err(|e| e.error)?;
        Ok(())
    }
}

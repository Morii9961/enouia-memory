//! In-process Agent reads over the Host's single bound Vault (ADR-MEM-81).
#[path = "checkpoint.rs"]
mod checkpoint;
#[path = "context.rs"]
mod context;
#[path = "propose.rs"]
mod propose;
use super::{AgentPort, Grant, PortFault, Server};
use crate::service::{HostService, ServiceFault};
use crate::{Connection, SessionFault};
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::{AuditDecision, AuditEvent, Tombstone};
use enouia_memory_contract::common::{ActorRef, Sensitivity};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::{AuditId, ProjectId};
use enouia_memory_contract::ipc::{self, Arguments, IpcRequest};
use enouia_memory_contract::json::{Revision, SchemaVersion};
use enouia_memory_contract::memory::{CanonicalMemory, ProvenanceState};
use enouia_memory_contract::policy::{
    AccessContext, PolicyRecord, ResourceContext, Scope, evaluate,
};
use enouia_memory_contract::ports::CommitPin;
use enouia_memory_contract::record::{Record, RecordKind, RecordRef, parse_record};
use enouia_memory_contract::source::{AttachmentRecord, SourceRecord};
use enouia_memory_govern::view::{Canonical, canonical_memories};
use enouia_memory_index::{Index, SearchRequest, search};
use enouia_memory_vault::audit::{AuditLog, DEFAULT_SEGMENT_BYTES};
use enouia_memory_vault::{Fault, Vault, VaultError};
use serde_json::{Value, json};

const MAX_SOURCE_OBJECT_BYTES: usize = 64 * 1024 * 1024;
const MAX_RESULT_BYTES: usize = 128 * 1024;
type Result<T> = std::result::Result<T, VaultError>;

pub struct HostPort<'a> {
    host: &'a HostService,
    connection: Option<Connection>,
    grant: Grant,
}
/// The original read constructor remains restricted to read scopes.
pub type ReadPort<'a> = HostPort<'a>;

impl HostService {
    /// Trusted in-process binding after separate Agent authentication. Only
    /// read scopes are implemented. No root is opened and no pipe is enrolled.
    pub fn mcp_read_server(
        &self,
        grant: Grant,
    ) -> std::result::Result<Server<ReadPort<'_>>, ServiceFault> {
        if !grant.active.load(std::sync::atomic::Ordering::SeqCst)
            || grant
                .scopes
                .iter()
                .any(|s| !matches!(s, Scope::MemoryRead | Scope::SourceRead))
        {
            return Err(SessionFault::ScopeDenied.into());
        }
        self.mcp_agent_server(grant)
    }

    /// In-process read/propose/local-context binding after native authentication.
    pub fn mcp_agent_server(
        &self,
        grant: Grant,
    ) -> std::result::Result<Server<HostPort<'_>>, ServiceFault> {
        if !grant.active.load(std::sync::atomic::Ordering::SeqCst)
            || grant.scopes.iter().any(|s| {
                !matches!(
                    s,
                    Scope::MemoryRead
                        | Scope::SourceRead
                        | Scope::MemoryPropose
                        | Scope::ContextRead
                        | Scope::SessionPropose
                )
            })
        {
            return Err(SessionFault::ScopeDenied.into());
        }
        let connection = self.admit_agent()?;
        Ok(Server::new(
            grant.clone(),
            HostPort {
                host: self,
                connection: Some(connection),
                grant,
            },
        ))
    }
}

impl AgentPort for HostPort<'_> {
    fn call(
        &mut self,
        principal: &ActorRef,
        request: &IpcRequest,
        arguments: Arguments,
    ) -> std::result::Result<Value, PortFault> {
        let connection = self.connection.as_ref().ok_or(PortFault::Unavailable)?;
        if principal != &self.grant.principal || !self.grant.allows(request.operation) {
            return Err(PortFault::Unavailable);
        }
        // The public typed port must not trust a separately supplied Arguments
        // value that disagrees with the validated packet.
        let packet = serde_json::to_value(request).map_err(|_| PortFault::Unavailable)?;
        if !ipc::parse_request(&packet).is_ok_and(|(_, a)| a == arguments) {
            return Err(PortFault::Unavailable);
        }
        let result =
            self.host
                .dispatch_agent(connection, |vault| match arguments {
                    Arguments::MemoryPropose(a) => propose::execute(vault, principal, request, a),
                    Arguments::ContextGet(a) => context::execute(vault, principal, request, a),
                    Arguments::SessionCheckpoint(a) => {
                        checkpoint::execute(vault, principal, request, a)
                    }
                    other => execute(vault, principal, request, other)
                        .map_err(|_| PortFault::Unavailable),
                });
        if !self.grant.allows(request.operation) {
            self.disconnect();
            return Err(
                if request.operation.is_write() || request.operation == ipc::Operation::ContextGet {
                    PortFault::OutcomeUnknown
                } else {
                    PortFault::Unavailable
                },
            );
        }
        match result {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(fault)) => {
                self.disconnect();
                Err(fault)
            }
            Err(_) => {
                self.disconnect();
                Err(PortFault::Unavailable)
            }
        }
    }
    fn disconnect(&mut self) {
        self.connection.take();
        self.grant.revoke();
    }
}
impl Drop for HostPort<'_> {
    fn drop(&mut self) {
        self.disconnect();
    }
}

fn fail(code: MemoryErrorCode) -> VaultError {
    VaultError::new(code, Fault::Contract(vec!["agent.read_refused"]))
}
fn missing() -> VaultError {
    fail(MemoryErrorCode::NotFound)
}

fn latest<T: Record>(vault: &Vault, pin: &CommitPin, kind: RecordKind) -> Result<Vec<T>> {
    vault
        .record_entries(pin, kind)?
        .into_iter()
        .map(|e| {
            parse_record(&vault.read_record(pin, &RecordRef::new(kind, &e.record_id, e.revision))?)
                .map_err(|_| fail(MemoryErrorCode::StorageFailed))
        })
        .collect()
}

struct Read<'a> {
    vault: &'a Vault,
    pin: CommitPin,
    principal: &'a ActorRef,
    policies: Vec<PolicyRecord>,
    tombstones: Vec<Tombstone>,
    refs: Vec<RecordRef>,
}
impl Read<'_> {
    fn visible(&self, r: &RecordRef) -> bool {
        !self.tombstones.iter().any(|t| {
            t.targets.iter().any(|target| {
                target.record_kind == r.record_kind
                    && target.record_id == r.record_id
                    && target
                        .revision
                        .is_none_or(|revision| revision == r.revision)
            })
        })
    }
    fn allowed(
        &self,
        r: &RecordRef,
        project: Option<ProjectId>,
        sensitivity: Sensitivity,
        scope: Scope,
    ) -> Result<bool> {
        Ok(self.visible(r)
            && evaluate(
                &self.policies.iter().collect::<Vec<_>>(),
                &AccessContext {
                    principal: self.principal,
                    scope,
                    purpose: None,
                    destination: None,
                    resource: &ResourceContext {
                        record: r.clone(),
                        project_id: project,
                        sensitivity,
                    },
                },
                &self.vault.now()?,
            )
            .is_allow())
    }
    fn record<T: Record>(
        &self,
        kind: RecordKind,
        id: &str,
        revision: Option<Revision>,
    ) -> Result<T> {
        let entry = self
            .vault
            .record_entry(&self.pin, kind, id)?
            .ok_or_else(missing)?;
        let reference = RecordRef::new(kind, id, revision.unwrap_or(entry.revision));
        if !self.visible(&reference) {
            return Err(missing());
        }
        parse_record(&self.vault.read_revision(&self.pin, &reference)?)
            .map_err(|_| fail(MemoryErrorCode::StorageFailed))
    }
    fn memory_allowed(&self, memory: &CanonicalMemory) -> Result<bool> {
        self.allowed(
            &RecordRef::new(
                RecordKind::Memory,
                memory.memory_id.as_str(),
                memory.revision,
            ),
            memory.project_id.clone(),
            memory.sensitivity,
            Scope::MemoryRead,
        )
    }
    fn memory(&mut self, a: ipc::MemoryReadArgs) -> Result<Value> {
        let current: CanonicalMemory =
            self.record(RecordKind::Memory, a.memory_id.as_str(), None)?;
        if !self.memory_allowed(&current)? {
            return Err(missing());
        }
        let memory: CanonicalMemory =
            self.record(RecordKind::Memory, a.memory_id.as_str(), a.revision)?;
        if !self.memory_allowed(&memory)? {
            return Err(missing());
        }
        self.verify_memory(&memory)?;
        let mut superseded_by = Vec::new();
        for other in canonical_memories(self.vault, &self.pin, Canonical::AllStatuses)? {
            if other
                .supersedes
                .iter()
                .any(|s| s.memory_id == memory.memory_id)
                && self.memory_allowed(&other)?
            {
                superseded_by.push(other.memory_id);
            }
        }
        self.refs.push(RecordRef::new(
            RecordKind::Memory,
            current.memory_id.as_str(),
            current.revision,
        ));
        self.refs.push(RecordRef::new(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            memory.revision,
        ));
        Ok(json!({"record":memory,"supersededBy":superseded_by}))
    }
    fn verify_memory(&mut self, memory: &CanonicalMemory) -> Result<()> {
        if memory.provenance_state == ProvenanceState::Broken {
            return Err(fail(MemoryErrorCode::BrokenProvenance));
        }
        for e in &memory.evidence {
            let source: SourceRecord = self.record(
                RecordKind::Source,
                e.source_id.as_str(),
                Some(e.source_revision),
            )?;
            if source.anchor_hash() != &e.object_hash || !e.locator.within(&source.locator) {
                return Err(fail(MemoryErrorCode::BrokenProvenance));
            }
            self.refs.push(RecordRef::new(
                RecordKind::Source,
                source.source_id.as_str(),
                source.revision,
            ));
        }
        for s in &memory.context_source_refs {
            let source: SourceRecord = self.record(
                RecordKind::Source,
                s.source_id.as_str(),
                Some(s.source_revision),
            )?;
            self.refs.push(RecordRef::new(
                RecordKind::Source,
                source.source_id.as_str(),
                source.revision,
            ));
        }
        Ok(())
    }
    fn search(&mut self, a: ipc::MemorySearchArgs) -> Result<Value> {
        let mut index = Index::open(self.vault)?;
        index.update(self.vault, &|| false)?;
        let page = search(
            &index,
            self.vault,
            self.principal,
            &SearchRequest {
                query: a.query,
                types: a.types,
                project_ids: a.project_ids,
                include_historical: a.include_historical,
                as_of: a.time_mode.as_of,
                known_at: a.time_mode.known_at_commit_id,
                cursor: a.cursor,
                limit: a.limit,
            },
        )?;
        for item in &page.items {
            // Historical index hits must also pass current resource metadata;
            // a later sensitivity/project change cannot reopen older content.
            let current: CanonicalMemory =
                self.record(RecordKind::Memory, item.memory_id.as_str(), None)?;
            let historical: CanonicalMemory = self.record(
                RecordKind::Memory,
                item.memory_id.as_str(),
                Some(item.revision),
            )?;
            if !self.memory_allowed(&current)? || !self.memory_allowed(&historical)? {
                return Err(fail(MemoryErrorCode::PermissionDenied));
            }
            self.verify_memory(&historical)?;
            self.refs.push(RecordRef::new(
                RecordKind::Memory,
                current.memory_id.as_str(),
                current.revision,
            ));
            self.refs.push(RecordRef::new(
                RecordKind::Memory,
                historical.memory_id.as_str(),
                historical.revision,
            ));
        }
        serde_json::to_value(page.to_ipc()).map_err(|_| fail(MemoryErrorCode::StorageFailed))
    }
    fn source(&mut self, a: ipc::MemorySourceArgs) -> Result<Value> {
        let current: SourceRecord = self.record(RecordKind::Source, a.source_id.as_str(), None)?;
        let source: SourceRecord =
            self.record(RecordKind::Source, a.source_id.as_str(), a.source_revision)?;
        for record in [&current, &source] {
            if !self.allowed(
                &RecordRef::new(
                    RecordKind::Source,
                    record.source_id.as_str(),
                    record.revision,
                ),
                None,
                record.sensitivity,
                Scope::SourceRead,
            )? {
                return Err(missing());
            }
        }
        let bytes = if let Some(manual) = &source.manual_assertion {
            manual.input_text.as_bytes().to_vec()
        } else if let Some(agent) = &source.agent_submission {
            agent.submitted_text.as_bytes().to_vec()
        } else {
            let hash = source
                .raw_object_hash
                .as_ref()
                .ok_or_else(|| fail(MemoryErrorCode::BrokenProvenance))?;
            let raw = self
                .vault
                .read_object_bounded(&self.pin, hash, MAX_SOURCE_OBJECT_BYTES)?;
            enouia_memory_import::locate::resolve(&raw, &source.locator)
                .map_err(|_| fail(MemoryErrorCode::BrokenProvenance))?
        };
        if bytes.len() > MAX_SOURCE_OBJECT_BYTES {
            return Err(fail(MemoryErrorCode::BudgetExceeded));
        }
        if sha256(&bytes) != source.content_hash {
            return Err(fail(MemoryErrorCode::BrokenProvenance));
        }
        let text =
            std::str::from_utf8(&bytes).map_err(|_| fail(MemoryErrorCode::BrokenProvenance))?;
        let start = a.range.as_ref().map_or(0, |r| r.start as usize);
        let bound = a.range.as_ref().map_or(text.len(), |r| r.end as usize);
        if start >= bound
            || bound > text.len()
            || !text.is_char_boundary(start)
            || !text.is_char_boundary(bound)
        {
            return Err(fail(MemoryErrorCode::InvalidRequest));
        }
        let mut end = bound.min(start.saturating_add(a.max_bytes as usize));
        while end > start && !text.is_char_boundary(end) {
            end -= 1;
        }
        if end == start {
            return Err(fail(MemoryErrorCode::InvalidRequest));
        }
        let mut attachments = Vec::new();
        for id in &source.attachment_refs {
            let attachment: AttachmentRecord =
                self.record(RecordKind::Attachment, id.as_str(), None)?;
            let reference =
                RecordRef::new(RecordKind::Attachment, id.as_str(), attachment.revision);
            if self.allowed(&reference, None, attachment.sensitivity, Scope::SourceRead)? {
                attachments.push(json!({"attachmentId":id,"availability":attachment.availability}));
                self.refs.push(reference);
            }
        }
        self.refs.push(RecordRef::new(
            RecordKind::Source,
            current.source_id.as_str(),
            current.revision,
        ));
        self.refs.push(RecordRef::new(
            RecordKind::Source,
            source.source_id.as_str(),
            source.revision,
        ));
        Ok(
            json!({"sourceId":source.source_id,"sourceRevision":source.revision,
            "excerpt":&text[start..end],"byteStart":start,"byteEnd":end,
            "truncated":end < bound,"attachments":attachments}),
        )
    }
}

fn execute(
    vault: &Vault,
    principal: &ActorRef,
    request: &IpcRequest,
    arguments: Arguments,
) -> Result<Value> {
    let pin = vault.pin_current()?;
    let mut read = Read {
        vault,
        pin,
        principal,
        policies: Vec::new(),
        tombstones: Vec::new(),
        refs: Vec::new(),
    };
    let result = (|| {
        read.policies = latest(vault, &read.pin, RecordKind::Policy)?;
        read.tombstones = latest(vault, &read.pin, RecordKind::Tombstone)?;
        let value = match arguments {
            Arguments::MemorySearch(a) => read.search(a)?,
            Arguments::MemoryRead(a) => read.memory(a)?,
            Arguments::MemorySource(a) => read.source(a)?,
            _ => return Err(fail(MemoryErrorCode::PermissionDenied)),
        };
        if value.to_string().len() > MAX_RESULT_BYTES {
            return Err(fail(MemoryErrorCode::BudgetExceeded));
        }
        let fresh = vault.check_fresh(&read.pin, &read.refs)?;
        if fresh.policy_changed || fresh.head.deletion_epoch != read.pin.deletion_epoch {
            return Err(fail(MemoryErrorCode::RevisionConflict));
        }
        vault.host_writer_state()?;
        Ok(value)
    })();
    let code = result.as_ref().err().map(VaultError::code);
    let audit = AuditLog::new(vault.managed_root().clone(), DEFAULT_SEGMENT_BYTES);
    let audit_result = audit.append(&AuditEvent {
        schema_version: SchemaVersion,
        audit_id: AuditId::from_random(vault.random_id_bytes()),
        actor: principal.clone(),
        operation: request.operation,
        object_refs: read.refs.clone(),
        purpose: None,
        destination: None,
        policy_epoch: read.pin.policy_epoch,
        decision: if code.is_some() {
            AuditDecision::Deny
        } else {
            AuditDecision::Allow
        },
        error_code: code,
        request_id: request.request_id.clone(),
        created_at: vault.now()?,
    });
    let result = audit_result.and(result).and_then(|value| {
        let fresh = vault.check_fresh(&read.pin, &read.refs)?;
        if fresh.policy_changed || fresh.head.deletion_epoch != read.pin.deletion_epoch {
            return Err(fail(MemoryErrorCode::RevisionConflict));
        }
        vault.host_writer_state()?;
        Ok(value)
    });
    let error = result.as_ref().err().map(|e| {
        let mut error = e.error;
        error.retryable = false;
        error
    });
    Ok(json!({"schemaVersion":1,"requestId":request.request_id,
        "kind":if error.is_some() { json!("memory_error") } else { json!(request.operation.success_kind()) },
        "vaultCommitId":read.pin.commit_id,"policyEpoch":read.pin.policy_epoch,
        "operationId":null,"result":result.ok(),"error":error}))
}

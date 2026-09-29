//! Minimal source and session writers (MV-1.3). They turn deterministic
//! synthetic input or an owner's typed statement into complete, validated
//! records and commit them with every key field on disk: the exact text as an
//! object or record field, its hash, times, actor, surface, and policy.
//!
//! Nothing here extracts, summarizes, or proposes memories. A manual
//! assertion is a *source* (evidence that the owner said something), not a
//! canonical memory; turning it into one is MV-3 review work.

use crate::error::{Result, VaultError};
use crate::store::Vault;
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::{ObjectKind, OperationKind};
use enouia_memory_contract::common::{
    ActorRef, ActorType, Sensitivity, TimePrecision, TrustedSurface,
};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{
    BranchId, CommitId, EventId, PolicyId, RequestId, SessionId, SourceId, TurnId,
};
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::ports::{
    CommitOutcome, CommitRequest, IdempotencyScope, StagedObject, StagedRecord,
};
use enouia_memory_contract::record::{RecordKind, RecordRef, parse_value};
use enouia_memory_contract::session::{ClientSurface, SessionRecord};
use enouia_memory_contract::source::{ConfirmationMethod, SourceRecord};
use enouia_memory_contract::time::Timestamp;
use serde_json::{Value, json};

pub const TEXT_MEDIA_TYPE: &str = "text/plain; charset=utf-8";

/// What the owner typed and confirmed on a trusted local surface.
#[derive(Clone, Debug)]
pub struct ManualAssertionInput {
    pub text: String,
    pub operator: ActorRef,
    pub trusted_surface: TrustedSurface,
    pub confirmation: ConfirmationMethod,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    pub time_precision: TimePrecision,
}

#[derive(Clone, Debug)]
pub struct SessionStart {
    pub owner: ActorRef,
    pub surface: ClientSurface,
    pub sensitivity: Sensitivity,
    pub policy_id: PolicyId,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Written<T> {
    pub outcome: CommitOutcome,
    pub id: T,
}

fn to_record<T: enouia_memory_contract::record::Record>(value: &Value) -> Result<(T, Vec<u8>)> {
    let record = parse_value::<T>(value).map_err(|e| VaultError::invalid(e.rules()))?;
    let bytes = canonical_bytes(value).map_err(|e| VaultError::invalid(e.rules()))?;
    Ok((record, bytes))
}

fn scope(principal: &ActorRef, kind: OperationKind, key: &[u8]) -> IdempotencyScope {
    IdempotencyScope {
        principal_id: principal.actor_id.clone(),
        operation_kind: kind,
        key_hash: sha256(key),
    }
}

/// The genesis default policy (ADR-MEM-31): owner-only, local destinations,
/// never `provider:send`. Every other grant is a later owner decision.
pub fn genesis_default_policy(policy_id: &PolicyId, now: &Timestamp) -> Result<StagedRecord> {
    let value = json!({
        "schema_version": 1,
        "policy_id": policy_id,
        "revision": 1,
        "status": "active",
        "origin": "genesis_default",
        "approval_id": null,
        "principals": [{"actor_type": "owner", "actor_id": null}],
        "scopes": ["memory:read", "source:read", "memory:propose", "session:propose",
                   "context:read", "owner:review", "owner:identity", "operation:read"],
        "resources": {
            "all_projects": true,
            "project_ids": [],
            "record_kinds": ["source", "attachment", "project", "memory", "candidate", "review",
                             "identity", "session", "session_event", "checkpoint"],
            "max_sensitivity": "highly_sensitive",
        },
        "purposes": ["answer", "continue_session", "checkpoint", "extraction", "inspection_preview"],
        "destinations": [
            {"kind": "local_mock", "provider": null, "model": null},
            {"kind": "local_model", "provider": null, "model": null},
        ],
        "valid_from": now,
        "valid_until": null,
        "revoked_at": null,
        "created_at": now,
        "updated_at": now,
    });
    let (_, bytes): (enouia_memory_contract::policy::PolicyRecord, _) = to_record(&value)?;
    Ok(StagedRecord {
        record_kind: RecordKind::Policy,
        record_id: policy_id.to_string(),
        revision: Revision::new(1).expect("one"),
        bytes,
    })
}

/// A complete genesis request with fresh IDs and the default policy.
pub fn new_genesis(
    ids: &dyn enouia_memory_contract::ports::IdSource,
    owner: ActorRef,
    trusted_surface: TrustedSurface,
    now: &Timestamp,
) -> Result<crate::store::GenesisRequest> {
    let policy_id = PolicyId::from_random(ids.random_16());
    let policy = genesis_default_policy(&policy_id, now)?;
    Ok(crate::store::GenesisRequest {
        vault_id: enouia_memory_contract::ids::VaultId::from_random(ids.random_16()),
        commit_id: CommitId::from_random(ids.random_16()),
        device_id: enouia_memory_contract::ids::DeviceId::from_random(ids.random_16()),
        owner,
        trusted_surface,
        request_payload_hash: sha256(&policy.bytes),
        records: vec![policy],
    })
}

impl Vault {
    fn new_commit_id(&self) -> CommitId {
        CommitId::from_random(self.random_id_bytes())
    }

    /// Record an owner's statement as a `manual_assertion` source. The
    /// idempotency key is the caller's (e.g. a UI submission ID); resubmitting
    /// the same key and text replays instead of creating a second source.
    pub fn record_manual_assertion(
        &self,
        input: &ManualAssertionInput,
        idempotency_key: &[u8],
    ) -> Result<Written<SourceId>> {
        if input.operator.actor_type != ActorType::Owner {
            return Err(VaultError::invalid(vec!["source.manual_owner"]));
        }
        if input.text.trim().is_empty() {
            return Err(VaultError::invalid(vec!["source.manual_text"]));
        }
        let key_scope = scope(&input.operator, OperationKind::Import, idempotency_key);
        let payload = sha256(input.text.as_bytes());
        if let Some((commit_id, stored, receipt)) = self.find_receipt(&key_scope)? {
            return if stored == payload {
                let id = receipt
                    .records
                    .iter()
                    .find(|r| r.record_kind == RecordKind::Source)
                    .and_then(|r| SourceId::parse(&r.record_id).ok())
                    .ok_or_else(|| VaultError::corrupt("receipt"))?;
                Ok(Written {
                    outcome: CommitOutcome::Replayed { commit_id, receipt },
                    id,
                })
            } else {
                Err(VaultError::new(
                    MemoryErrorCode::IdempotencyConflict,
                    crate::Fault::IdempotencyConflict,
                ))
            };
        }
        let now = self.now()?;
        let source_id = SourceId::from_random(self.random_id_bytes());
        let value = json!({
            "schema_version": 1,
            "source_id": source_id,
            "revision": 1,
            "source_kind": "manual_assertion",
            "provider": null,
            "account_scope": null,
            "import_id": null,
            "raw_object_hash": null,
            "content_hash": payload,
            "original_conversation_id": null,
            "original_message_id": null,
            "parent_source_ids": [],
            "branch_id": null,
            "locator": {"kind": "manual_input"},
            "original_time": null,
            "original_timezone": null,
            "occurred_at": now,
            "captured_at": now,
            "time_precision": input.time_precision,
            "speaker_role": "user",
            "author_label": null,
            "evidence_class": "user_statement",
            "completeness": "complete",
            "sensitivity": input.sensitivity,
            "access_policy_id": input.access_policy_id,
            "attachment_refs": [],
            "parser_version": null,
            "parse_warnings": [],
            "manual_assertion": {
                "input_text": input.text,
                "operator": input.operator,
                "trusted_surface": input.trusted_surface,
                "confirmation_method": input.confirmation,
                "confirmed_at": now,
            },
            "agent_submission": null,
            "created_at": now,
            "extensions": {},
        });
        let (_, bytes): (SourceRecord, _) = to_record(&value)?;
        let outcome = self.commit(CommitRequest {
            commit_id: self.new_commit_id(),
            expected_commit_id: None,
            principal: input.operator.clone(),
            operation_kind: OperationKind::Import,
            idempotency: key_scope,
            request_payload_hash: payload,
            expected_revisions: vec![(RecordKind::Source, source_id.to_string(), None)],
            records: vec![StagedRecord {
                record_kind: RecordKind::Source,
                record_id: source_id.to_string(),
                revision: Revision::new(1).expect("one"),
                bytes,
            }],
            objects: Vec::new(),
        })?;
        Ok(Written {
            outcome,
            id: source_id,
        })
    }

    /// Create a session with one default branch and no events yet.
    pub fn start_session(
        &self,
        start: &SessionStart,
        idempotency_key: &[u8],
    ) -> Result<Written<(SessionId, BranchId)>> {
        let now = self.now()?;
        let session_id = SessionId::from_random(self.random_id_bytes());
        let branch_id = BranchId::from_random(self.random_id_bytes());
        let value = json!({
            "schema_version": 1,
            "session_id": session_id,
            "revision": 1,
            "origin_surface": start.surface,
            "provider_bindings": [],
            "branches": [{
                "branch_id": branch_id,
                "parent_branch_id": null,
                "forked_from_event_id": null,
                "last_event_seq": 0,
            }],
            "default_branch_id": branch_id,
            "parent_session_id": null,
            "participants": [start.owner],
            "last_event_seq": 0,
            "status": "open",
            "sensitivity": start.sensitivity,
            "policy_id": start.policy_id,
            "created_at": now,
            "updated_at": now,
            "extensions": {},
        });
        let (_, bytes): (SessionRecord, _) = to_record(&value)?;
        let key_scope = scope(&start.owner, OperationKind::SessionAppend, idempotency_key);
        let payload = sha256(&bytes);
        let outcome = self.commit(CommitRequest {
            commit_id: self.new_commit_id(),
            expected_commit_id: None,
            principal: start.owner.clone(),
            operation_kind: OperationKind::SessionAppend,
            idempotency: key_scope,
            request_payload_hash: payload,
            expected_revisions: vec![(RecordKind::Session, session_id.to_string(), None)],
            records: vec![StagedRecord {
                record_kind: RecordKind::Session,
                record_id: session_id.to_string(),
                revision: Revision::new(1).expect("one"),
                bytes,
            }],
            objects: Vec::new(),
        })?;
        Ok(Written {
            outcome,
            id: (session_id, branch_id),
        })
    }

    fn latest_session(&self, session_id: &SessionId) -> Result<SessionRecord> {
        let pin = self.pin_current()?;
        let entry = self
            .record_entry(&pin, RecordKind::Session, session_id.as_str())?
            .ok_or_else(|| VaultError::new(MemoryErrorCode::NotFound, crate::Fault::NotFound))?;
        let bytes = self.read_record(
            &pin,
            &RecordRef::new(RecordKind::Session, session_id.as_str(), entry.revision),
        )?;
        enouia_memory_contract::parse_record(&bytes).map_err(|_| VaultError::corrupt("session"))
    }

    /// Save a user message before anything else happens to it: the event,
    /// its exact text as a session-content object, and the next session
    /// revision commit together. The text is never lost to a later failure.
    pub fn append_user_message(
        &self,
        session_id: &SessionId,
        author: &ActorRef,
        text: &str,
        request_id: &RequestId,
        idempotency_key: &[u8],
    ) -> Result<Written<EventId>> {
        let key_scope = scope(author, OperationKind::SessionAppend, idempotency_key);
        let content_hash: Sha256Hex = sha256(text.as_bytes());
        if let Some((commit_id, stored, receipt)) = self.find_receipt(&key_scope)? {
            return if stored == content_hash {
                let id = receipt
                    .records
                    .iter()
                    .find(|r| r.record_kind == RecordKind::SessionEvent)
                    .and_then(|r| EventId::parse(&r.record_id).ok())
                    .ok_or_else(|| VaultError::corrupt("receipt"))?;
                Ok(Written {
                    outcome: CommitOutcome::Replayed { commit_id, receipt },
                    id,
                })
            } else {
                Err(VaultError::new(
                    MemoryErrorCode::IdempotencyConflict,
                    crate::Fault::IdempotencyConflict,
                ))
            };
        }
        let session = self.latest_session(session_id)?;
        let now = self.now()?;
        let sequence = session
            .last_event_seq
            .checked_add(1)
            .ok_or_else(|| VaultError::invalid(vec!["number.out_of_range"]))?;
        let event_id = EventId::from_random(self.random_id_bytes());
        let event = json!({
            "schema_version": 1,
            "event_id": event_id,
            "session_id": session_id,
            "branch_id": session.default_branch_id,
            "sequence": sequence,
            "parent_event_id": null,
            "turn_id": TurnId::from_random(self.random_id_bytes()),
            "kind": "user_message",
            "actor": author,
            "occurred_at": now,
            "captured_at": now,
            "content_ref": {
                "object_hash": content_hash,
                "size_bytes": text.len(),
                "media_type": TEXT_MEDIA_TYPE,
            },
            "source_refs": [],
            "request_id": request_id,
            "delivery_state": "not_applicable",
            "sensitivity": session.sensitivity,
            "extensions": {},
        });
        let (_, event_bytes): (enouia_memory_contract::session::SessionEvent, _) =
            to_record(&event)?;
        let mut next = serde_json::to_value(&session).expect("session serializes");
        next["revision"] = json!(session.revision.get() + 1);
        next["last_event_seq"] = json!(sequence);
        next["updated_at"] = json!(now);
        if let Some(branches) = next["branches"].as_array_mut() {
            for branch in branches {
                if branch["branch_id"] == json!(session.default_branch_id) {
                    branch["last_event_seq"] = json!(sequence);
                }
            }
        }
        let (_, session_bytes): (SessionRecord, _) = to_record(&next)?;
        let outcome = self.commit(CommitRequest {
            commit_id: self.new_commit_id(),
            expected_commit_id: None,
            principal: author.clone(),
            operation_kind: OperationKind::SessionAppend,
            idempotency: key_scope,
            request_payload_hash: content_hash.clone(),
            expected_revisions: vec![(
                RecordKind::Session,
                session_id.to_string(),
                Some(session.revision),
            )],
            records: vec![
                StagedRecord {
                    record_kind: RecordKind::Session,
                    record_id: session_id.to_string(),
                    revision: Revision::new(session.revision.get() + 1)
                        .ok_or_else(|| VaultError::invalid(vec!["number.out_of_range"]))?,
                    bytes: session_bytes,
                },
                StagedRecord {
                    record_kind: RecordKind::SessionEvent,
                    record_id: event_id.to_string(),
                    revision: Revision::new(1).expect("one"),
                    bytes: event_bytes,
                },
            ],
            objects: vec![StagedObject {
                kind: ObjectKind::SessionContent,
                hash: content_hash,
                bytes: text.as_bytes().to_vec(),
            }],
        })?;
        Ok(Written {
            outcome,
            id: event_id,
        })
    }
}

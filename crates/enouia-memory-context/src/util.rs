use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::common::ActorRef;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::CommitId;
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::ports::{
    CommitOutcome, CommitPin, CommitRequest, IdempotencyScope, StagedObject, StagedRecord,
};
use enouia_memory_contract::record::{Record, RecordKind, RecordRef, parse_record};
use enouia_memory_vault::{Fault, Vault, VaultError};
use serde::Serialize;

pub type Result<T> = std::result::Result<T, VaultError>;
pub fn one() -> Revision {
    Revision::new(1).expect("one")
}
pub fn invalid(rule: &'static str) -> VaultError {
    VaultError::invalid(vec![rule])
}
pub fn denied() -> VaultError {
    VaultError::new(
        MemoryErrorCode::PermissionDenied,
        Fault::Contract(vec!["context.permission_denied"]),
    )
}
pub fn missing() -> VaultError {
    VaultError::new(MemoryErrorCode::NotFound, Fault::NotFound)
}
pub fn owner(vault: &Vault, actor: &ActorRef) -> Result<()> {
    if actor == &vault.descriptor().created_by {
        Ok(())
    } else {
        Err(denied())
    }
}
pub fn bytes(value: &impl Serialize) -> Result<Vec<u8>> {
    canonical_bytes(&serde_json::to_value(value).map_err(|_| invalid("context.serialize"))?)
        .map_err(|e| VaultError::invalid(e.rules()))
}
pub fn staged(
    kind: RecordKind,
    id: &str,
    revision: Revision,
    value: &impl Serialize,
) -> Result<StagedRecord> {
    Ok(StagedRecord {
        record_kind: kind,
        record_id: id.to_owned(),
        revision,
        bytes: bytes(value)?,
    })
}
pub fn read<T: Record>(
    vault: &Vault,
    pin: &CommitPin,
    kind: RecordKind,
    id: &str,
    revision: Revision,
) -> Result<T> {
    parse_record(&vault.read_revision(pin, &RecordRef::new(kind, id, revision))?)
        .map_err(|e| VaultError::invalid(e.rules()))
}
pub fn latest<T: Record>(vault: &Vault, pin: &CommitPin, kind: RecordKind) -> Result<Vec<T>> {
    vault
        .record_entries(pin, kind)?
        .iter()
        .map(|e| read(vault, pin, kind, &e.record_id, e.revision))
        .collect()
}
pub fn scope(actor: &ActorRef, operation: OperationKind, key: &[u8]) -> IdempotencyScope {
    IdempotencyScope {
        principal_id: actor.actor_id.clone(),
        operation_kind: operation,
        key_hash: sha256(key),
    }
}
pub fn replay(
    vault: &Vault,
    actor: &ActorRef,
    operation: OperationKind,
    key: &[u8],
    payload: &Sha256Hex,
) -> Result<Option<(CommitId, enouia_memory_contract::commit::OperationReceipt)>> {
    match vault.find_receipt(&scope(actor, operation, key))? {
        Some((id, hash, receipt)) if &hash == payload => Ok(Some((id, receipt))),
        Some(_) => Err(VaultError::new(
            MemoryErrorCode::IdempotencyConflict,
            Fault::IdempotencyConflict,
        )),
        None => Ok(None),
    }
}

pub fn pin_at(vault: &Vault, id: &CommitId) -> Result<CommitPin> {
    let mut pin = vault.pin_current()?;
    loop {
        if &pin.commit_id == id {
            return Ok(pin);
        }
        let stored = vault.stored_commit(&pin)?;
        let parent = stored.parent_commit_id.clone().ok_or_else(missing)?;
        let previous = vault.stored_commit(&CommitPin {
            commit_id: parent,
            sequence: pin.sequence - 1,
            policy_epoch: 0,
            deletion_epoch: 0,
        })?;
        pin = CommitPin {
            commit_id: previous.commit_id.clone(),
            sequence: previous.sequence,
            policy_epoch: previous.policy_epoch,
            deletion_epoch: previous.deletion_epoch,
        };
    }
}
// Keep the vault commit's optimistic preconditions and staged payload together.
#[allow(clippy::too_many_arguments)]
pub fn commit(
    vault: &Vault,
    actor: &ActorRef,
    pin: &CommitPin,
    operation: OperationKind,
    key: &[u8],
    payload: Sha256Hex,
    records: Vec<StagedRecord>,
    objects: Vec<StagedObject>,
) -> Result<CommitOutcome> {
    let expected_revisions = records
        .iter()
        .map(|r| {
            let previous = if r.revision.get() == 1 {
                None
            } else {
                Revision::new(r.revision.get() - 1)
            };
            (r.record_kind, r.record_id.clone(), previous)
        })
        .collect();
    vault.commit(CommitRequest {
        commit_id: CommitId::from_random(vault.random_id_bytes()),
        expected_commit_id: Some(pin.commit_id.clone()),
        principal: actor.clone(),
        operation_kind: operation,
        idempotency: scope(actor, operation, key),
        request_payload_hash: payload,
        expected_revisions,
        records,
        objects,
    })
}

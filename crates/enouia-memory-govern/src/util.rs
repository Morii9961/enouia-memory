//! Small helpers shared by the governance modules.

use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::ports::{CommitPin, StagedRecord};
use enouia_memory_contract::record::{Record, RecordKind, RecordRef, parse_record, parse_value};
use enouia_memory_vault::{Fault, Vault, VaultError};
use serde_json::Value;

pub(crate) type Result<T> = std::result::Result<T, VaultError>;

pub(crate) fn invalid(rule: &'static str) -> VaultError {
    VaultError::invalid(vec![rule])
}

pub(crate) fn stale() -> VaultError {
    VaultError::new(MemoryErrorCode::RevisionConflict, Fault::RevisionMismatch)
}

/// One revision the pinned catalog names (latest or earlier), parsed.
pub(crate) fn revision<T: Record>(
    vault: &Vault,
    pin: &CommitPin,
    kind: RecordKind,
    id: &str,
    revision: Revision,
) -> Result<Option<T>> {
    match vault.read_revision(pin, &RecordRef::new(kind, id, revision)) {
        Ok(bytes) => Ok(Some(
            parse_record(&bytes).map_err(|_| VaultError::corrupt("record"))?,
        )),
        // Purged content reads as absent; its tombstone says why.
        Err(error)
            if matches!(
                error.code(),
                MemoryErrorCode::NotFound | MemoryErrorCode::IntentionallyPurged
            ) =>
        {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// The latest revision of one record at the pin.
pub(crate) fn latest<T: Record>(
    vault: &Vault,
    pin: &CommitPin,
    kind: RecordKind,
    id: &str,
) -> Result<Option<(T, Revision)>> {
    let Some(entry) = vault.record_entry(pin, kind, id)? else {
        return Ok(None);
    };
    Ok(revision(vault, pin, kind, id, entry.revision)?.map(|record| (record, entry.revision)))
}

/// The latest revision of every record of one kind at the pin.
pub(crate) fn all_latest<T: Record>(
    vault: &Vault,
    pin: &CommitPin,
    kind: RecordKind,
) -> Result<Vec<T>> {
    let mut out = Vec::new();
    for entry in vault.record_entries(pin, kind)? {
        if let Some(record) = revision(vault, pin, kind, &entry.record_id, entry.revision)? {
            out.push(record);
        }
    }
    Ok(out)
}

/// Strictly parse a record value (every per-record rule) and stage its
/// canonical bytes.
pub(crate) fn staged<T: Record>(
    kind: RecordKind,
    id: &str,
    revision: Revision,
    value: &Value,
) -> Result<(T, StagedRecord)> {
    let record = parse_value::<T>(value).map_err(|e| VaultError::invalid(e.rules()))?;
    let bytes = canonical_bytes(value).map_err(|e| VaultError::invalid(e.rules()))?;
    Ok((
        record,
        StagedRecord {
            record_kind: kind,
            record_id: id.to_owned(),
            revision,
            bytes,
        },
    ))
}

pub(crate) fn next(revision: Revision) -> Revision {
    Revision::new(revision.get() + 1).expect("revision below the safe-integer limit")
}

pub(crate) fn one() -> Revision {
    Revision::new(1).expect("one")
}

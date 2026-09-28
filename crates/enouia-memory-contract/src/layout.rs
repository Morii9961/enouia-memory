//! Relative Vault layout (MEMORY_ARCHITECTURE §5). Pure path builders only:
//! MV-1 resolves them under a verified data root. File names use generated IDs
//! and hashes only, never titles, user names, or import paths. No Memory path
//! lies under the independent `activity/` root.

use crate::hash::Sha256Hex;
use crate::json::Revision;
use crate::record::RecordKind;

pub const VAULT_DIR: &str = "vault";
pub const INDEX_FILE: &str = "indexes/memory.sqlite";
pub const ACTIVITY_DIR: &str = "activity";

/// Every top-level entry a Memory component may manage under the data root.
pub const MANAGED_ROOTS: &[&str] = &["vault", "indexes", "config", "backup-state", "sync-state"];

pub fn current_pointer() -> String {
    format!("{VAULT_DIR}/CURRENT")
}

pub fn commit_manifest(commit_id: &str) -> String {
    format!("{VAULT_DIR}/commits/{commit_id}.json")
}

/// Revisioned records: `vault/records/<kind>/<id>/<revision>.<ext>`;
/// single-revision records: `vault/records/<kind>/<id>.json`.
pub fn record_path(kind: RecordKind, id: &str, revision: Revision) -> Option<String> {
    let dir = match kind {
        RecordKind::Memory => "memory",
        RecordKind::Identity => "identity",
        RecordKind::Source => "source",
        RecordKind::Attachment => "attachment",
        RecordKind::Project => "project",
        RecordKind::Candidate => "candidate",
        RecordKind::Session => "session",
        RecordKind::Checkpoint => "checkpoint",
        RecordKind::Policy => "policy",
        RecordKind::Review => return Some(format!("{VAULT_DIR}/records/review/{id}.json")),
        RecordKind::Approval => return Some(format!("{VAULT_DIR}/records/approval/{id}.json")),
        RecordKind::Tombstone => return Some(format!("{VAULT_DIR}/records/tombstone/{id}.json")),
        RecordKind::PurgeReceipt => {
            return Some(format!("{VAULT_DIR}/records/receipt/{id}.json"));
        }
        _ => return None,
    };
    let ext = if kind == RecordKind::Identity {
        "md"
    } else {
        "json"
    };
    Some(format!(
        "{VAULT_DIR}/records/{dir}/{id}/{}.{ext}",
        revision.get()
    ))
}

/// Identity sidecar metadata lives next to its Markdown revision.
pub fn identity_sidecar(id: &str, revision: Revision) -> String {
    format!("{VAULT_DIR}/records/identity/{id}/{}.json", revision.get())
}

pub fn session_event(session_id: &str, event_id: &str) -> String {
    format!("{VAULT_DIR}/session-events/{session_id}/{event_id}.json")
}

pub fn raw_object(hash: &Sha256Hex) -> String {
    format!("{VAULT_DIR}/raw/objects/{hash}")
}

pub fn asset_object(hash: &Sha256Hex) -> String {
    format!("{VAULT_DIR}/assets/objects/{hash}")
}

pub fn staging_dir(transaction_id: &str) -> String {
    format!("{VAULT_DIR}/staging/{transaction_id}")
}

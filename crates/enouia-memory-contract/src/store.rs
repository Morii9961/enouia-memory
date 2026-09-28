//! File contracts of the Vault store itself (MV-1, ADR-MEM-36): the vault
//! descriptor, the `CURRENT` pointer, the publish journal, idempotency index
//! entries, recovery receipts, and the pinned-commit export used by backups.
//!
//! These are shapes and pure rules only. Whether a store writes them in the
//! right order, flushes them, and recovers from a crash between two of them is
//! store behavior, tested in `enouia-memory-vault`.

use crate::commit::{FormatVersion, OperationKind};
use crate::common::{ActorRef, ActorType, TrustedSurface};
use crate::error::{ContractError, Violation};
use crate::hash::{Sha256Hex, sha256};
use crate::ids::{CommitId, DeviceId, OperationId, PrincipalId, VaultId};
use crate::json::{SchemaVersion, canonical_bytes, check_safe_integers, ensure_writable};
use crate::layout;
use crate::time::Timestamp;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeSet;

/// A store document with its own semantic rules.
pub trait StoreDocument: Serialize + DeserializeOwned {
    fn validate(&self) -> Vec<Violation>;
}

/// Strict parse of a store document: supported major, safe integers, exact
/// shape, then rules. Mirrors `record::parse_value`.
pub fn parse_store_value<T: StoreDocument>(value: &Value) -> Result<T, ContractError> {
    if !value.is_object() {
        return Err(ContractError::Malformed);
    }
    ensure_writable(value)?;
    let mut ranges = Vec::new();
    check_safe_integers(value, "", &mut ranges);
    if !ranges.is_empty() {
        return Err(ContractError::Invalid(ranges));
    }
    let document: T = serde_json::from_value(value.clone()).map_err(|e| {
        let text = e.to_string();
        ContractError::Shape(text.split(" at line ").next().unwrap_or("").to_owned())
    })?;
    let violations = document.validate();
    if violations.is_empty() {
        Ok(document)
    } else {
        Err(ContractError::Invalid(violations))
    }
}

pub fn parse_store<T: StoreDocument>(bytes: &[u8]) -> Result<T, ContractError> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ContractError::Malformed)?;
    parse_store_value(&value)
}

/// `vault/vault.json`: written once by genesis, before the first `CURRENT`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct VaultDescriptor {
    pub schema_version: SchemaVersion,
    pub vault_id: VaultId,
    pub format_version: FormatVersion,
    pub genesis_commit_id: CommitId,
    pub genesis_device_id: DeviceId,
    pub created_by: ActorRef,
    pub created_at: Timestamp,
}

impl StoreDocument for VaultDescriptor {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.created_by.actor_type != ActorType::Owner {
            out.push(Violation::new("vault.owner_required", "/created_by"));
        }
        out
    }
}

/// `vault/CURRENT`: names exactly one complete commit manifest and pins its
/// bytes. A reader that cannot verify the named manifest against
/// `manifest_sha256` reports `vault_recovering`; it never guesses.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CurrentPointer {
    pub schema_version: SchemaVersion,
    pub vault_id: VaultId,
    pub commit_id: CommitId,
    pub sequence: u64,
    pub manifest_sha256: Sha256Hex,
}

impl StoreDocument for CurrentPointer {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.sequence == 0 {
            out.push(Violation::new("current.sequence", "/sequence"));
        }
        out
    }
}

/// One line of `vault/journal/published.jsonl`, appended after `CURRENT` was
/// replaced. It is evidence that a commit was published, used by owner-driven
/// recovery when `CURRENT` itself is damaged. A torn final line is ignored.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublishRecord {
    pub schema_version: SchemaVersion,
    pub vault_id: VaultId,
    pub commit_id: CommitId,
    pub sequence: u64,
    pub manifest_sha256: Sha256Hex,
    pub published_at: Timestamp,
}

impl StoreDocument for PublishRecord {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.sequence == 0 {
            out.push(Violation::new("current.sequence", "/sequence"));
        }
        out
    }
}

/// `vault/idempotency/<scope_hash>.json`. Written before `CURRENT` is
/// published; a lookup only trusts it when the named commit is on the
/// published chain, so an entry left by a crashed attempt is inert.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdempotencyEntry {
    pub schema_version: SchemaVersion,
    pub vault_id: VaultId,
    pub principal_id: PrincipalId,
    pub operation_kind: OperationKind,
    pub key_hash: Sha256Hex,
    pub request_payload_hash: Sha256Hex,
    pub commit_id: CommitId,
    pub sequence: u64,
}

impl StoreDocument for IdempotencyEntry {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.operation_kind == OperationKind::Genesis {
            out.push(Violation::new("idempotency.genesis", "/operation_kind"));
        }
        if self.sequence < 2 {
            out.push(Violation::new("idempotency.sequence", "/sequence"));
        }
        out
    }
}

/// File name of an idempotency entry: SHA-256 of the canonical scope. The key
/// itself is already a hash; no caller text reaches a file name.
pub fn idempotency_scope_hash(
    principal_id: &PrincipalId,
    operation_kind: OperationKind,
    key_hash: &Sha256Hex,
) -> Sha256Hex {
    let scope = json!({
        "scope_version": 1,
        "principal_id": principal_id,
        "operation_kind": operation_kind,
        "key_hash": key_hash,
    });
    sha256(&canonical_bytes(&scope).expect("scope serializes"))
}

/// What proves that an adopted commit is a safe recovery point.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoveryEvidence {
    /// Named by a valid publish-journal line: it was once `CURRENT`.
    PublishJournal,
    /// Complete and verified, but never shown to have been published.
    VerifiedUnpublished,
    /// Restored from a verified pinned-commit export.
    RestoredExport,
}

/// `vault/recovery/<recovery_id>.json`: the owner's explicit choice of a
/// recovery point. Recovery never selects "the newest manifest" by itself.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoveryReceipt {
    pub schema_version: SchemaVersion,
    pub recovery_id: OperationId,
    pub vault_id: VaultId,
    pub adopted_commit_id: CommitId,
    pub adopted_sequence: u64,
    pub manifest_sha256: Sha256Hex,
    /// SHA-256 of the damaged `CURRENT` bytes, `null` when it was missing.
    #[serde(deserialize_with = "crate::json::nullable")]
    pub previous_current_sha256: Option<Sha256Hex>,
    pub evidence: RecoveryEvidence,
    pub approved_by: ActorRef,
    pub trusted_surface: TrustedSurface,
    pub created_at: Timestamp,
}

impl StoreDocument for RecoveryReceipt {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.approved_by.actor_type != ActorType::Owner {
            out.push(Violation::new("recovery.owner_required", "/approved_by"));
        }
        if self.adopted_sequence == 0 {
            out.push(Violation::new("current.sequence", "/adopted_sequence"));
        }
        out
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportFile {
    pub path: String,
    pub sha256: Sha256Hex,
    pub size_bytes: u64,
}

/// `export-manifest.json` at the root of a pinned-commit export: the plaintext
/// tree an encrypted backup tool (restic) then stores. It lists every file
/// needed to rebuild the commit and nothing rebuildable (indexes) or secret.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ExportManifest {
    pub schema_version: SchemaVersion,
    pub export_format: FormatVersion,
    pub vault_id: VaultId,
    pub commit_id: CommitId,
    pub sequence: u64,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
    pub manifest_sha256: Sha256Hex,
    pub created_at: Timestamp,
    pub files: Vec<ExportFile>,
}

pub const EXPORT_MANIFEST_FILE: &str = "export-manifest.json";

/// Store bookkeeping that an export never carries: the live pointer, the
/// lock, unpublished staging, quarantined orphans, and indexes rebuilt on
/// restore (idempotency entries are rebuilt from the commit chain).
const EXPORT_EXCLUDED: &[&str] = &[
    "vault/CURRENT",
    "vault/LOCK",
    "vault/staging/",
    "vault/orphans/",
    "vault/idempotency/",
    "vault/journal/",
    "vault/recovery/",
];

/// A relative, `/`-separated Vault path of generated names only: no empty,
/// `.` or `..` segment, no drive, backslash, colon (ADS), or other characters.
pub fn is_safe_vault_path(path: &str) -> bool {
    path.starts_with("vault/")
        && path.split('/').all(|segment| {
            !segment.is_empty()
                && segment != "."
                && segment != ".."
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.'))
        })
}

impl StoreDocument for ExportManifest {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.sequence == 0 {
            out.push(Violation::new("current.sequence", "/sequence"));
        }
        let mut seen = BTreeSet::new();
        let mut previous: Option<&str> = None;
        for (index, file) in self.files.iter().enumerate() {
            let path = file.path.as_str();
            let excluded = EXPORT_EXCLUDED.iter().any(|e| {
                if e.ends_with('/') {
                    path.starts_with(e)
                } else {
                    path == *e
                }
            });
            if !is_safe_vault_path(path) || excluded {
                out.push(Violation::new(
                    "export.path",
                    format!("/files/{index}/path"),
                ));
            }
            if !seen.insert(path) || previous.is_some_and(|p| p >= path) {
                out.push(Violation::new(
                    "export.order",
                    format!("/files/{index}/path"),
                ));
            }
            previous = Some(path);
        }
        for required in [
            layout::vault_descriptor(),
            layout::commit_manifest(self.commit_id.as_str()),
        ] {
            if !seen.contains(required.as_str()) {
                out.push(Violation::new("export.required_file", "/files"));
            }
        }
        out
    }
}

/// `config/restore-state.json`: a restored Vault starts with network,
/// Provider, MCP, and sync disabled until newer tombstones and device
/// revocations have been reconciled by the owner.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RestoreState {
    pub schema_version: SchemaVersion,
    pub vault_id: VaultId,
    pub restored_commit_id: CommitId,
    pub export_manifest_sha256: Sha256Hex,
    pub restored_at: Timestamp,
    pub network_disabled_until_reconciled: bool,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub reconciled_at: Option<Timestamp>,
}

impl StoreDocument for RestoreState {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.reconciled_at.is_none() && !self.network_disabled_until_reconciled {
            out.push(Violation::new(
                "restore.network_gate",
                "/network_disabled_until_reconciled",
            ));
        }
        if let Some(reconciled) = &self.reconciled_at
            && reconciled < &self.restored_at
        {
            out.push(Violation::new("restore.reconciled_order", "/reconciled_at"));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_vault_paths_reject_escapes_and_foreign_names() {
        assert!(is_safe_vault_path("vault/records/memory/mem_1/2.json"));
        for bad in [
            "vault",
            "vault/",
            "vault//x",
            "vault/../x",
            "vault/./x",
            "records/x",
            "/vault/x",
            "vault/a\\b",
            "vault/a:b",
            "vault/C:",
            "vault/名前",
            "vault/a b",
        ] {
            assert!(!is_safe_vault_path(bad), "{bad}");
        }
    }

    #[test]
    fn scope_hash_depends_on_every_part() {
        let principal = PrincipalId::parse("prn_00000001-0000-4000-8000-000000000001").unwrap();
        let other = PrincipalId::parse("prn_00000002-0000-4000-8000-000000000002").unwrap();
        let key = sha256(b"key");
        let base = idempotency_scope_hash(&principal, OperationKind::Import, &key);
        assert_ne!(
            base,
            idempotency_scope_hash(&other, OperationKind::Import, &key)
        );
        assert_ne!(
            base,
            idempotency_scope_hash(&principal, OperationKind::ReviewCommit, &key)
        );
        assert_ne!(
            base,
            idempotency_scope_hash(&principal, OperationKind::Import, &sha256(b"other"))
        );
        assert_eq!(
            base,
            idempotency_scope_hash(&principal, OperationKind::Import, &key)
        );
    }
}

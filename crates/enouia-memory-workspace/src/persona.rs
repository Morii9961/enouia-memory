//! Public persona projection export (ADR-MEM-128). Owner-only: the page
//! names one exact Identity revision and the records checked for an
//! audience; the Core builds the projection from the public Identity block and
//! eligible `public` records only, and writes a new file into an owner-picked
//! folder after the exact preview digest still matches. No network, schedule,
//! sync or automatic retry.

use super::*;
use enouia_memory_contract::commit::Tombstone;
use enouia_memory_contract::hash::Sha256Hex;
use enouia_memory_contract::identity::{IdentityMetadata, IdentitySlug};
use enouia_memory_contract::ids::IdentityId;
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::memory::{MemoryBody, MemoryStatus, ProvenanceState};
use enouia_memory_contract::persona::{
    self, Audience, ContractName, Document, KnowledgeItem, KnowledgeSource, Projection,
};
use enouia_memory_contract::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use std::io::Write;

const LEDGER: &str = "sync-state/public-persona-1.json";
const LEDGER_MAX_BYTES: usize = 32 * 1024 * 1024;
const OPTIONS_MAX_MEMORIES: usize = 500;
const MEMORY_VERSION: &str = concat!("enouia-memory-workspace/", env!("CARGO_PKG_VERSION"));

#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
enum LedgerName {
    #[default]
    #[serde(rename = "public-persona-ledger-1")]
    V1,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Issued {
    version: u64,
    projection_sha256: Sha256Hex,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct LastExport {
    version: u64,
    projection_sha256: Sha256Hex,
    projection: Projection,
    memory_ids: Vec<MemoryId>,
    exported_at: Timestamp,
    file_name: String,
    file_sha256: Sha256Hex,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct AudienceState {
    issued: Option<Issued>,
    last_export: Option<LastExport>,
}

/// One export request by its key. `document` is kept only while the write
/// has not been confirmed, so an owner resend writes the same bytes.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct KeyEntry {
    fingerprint: Sha256Hex,
    written: bool,
    document: Option<Document>,
    receipt: Value,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Ledger {
    ledger: LedgerName,
    audiences: BTreeMap<Audience, AudienceState>,
    receipts: BTreeMap<Sha256Hex, KeyEntry>,
}

/// A preview as computed from the current head, the ledger and the selection.
struct Computed {
    projection: Projection,
    projection_sha256: Sha256Hex,
    version: u64,
    digest: Sha256Hex,
    result: Value,
}

/// A contract refusal led by where it happened: the Identity block or the
/// assembled projection (for example a public record's text).
fn contract_fail(stage: &str, error: &ContractError) -> Fail {
    let mut rules: Vec<String> = error.rules().iter().map(|r| (*r).to_owned()).collect();
    rules.insert(0, stage.to_owned());
    rules.dedup();
    rules.truncate(16);
    Fail(WorkspaceError {
        code: MemoryErrorCode::InvalidRequest,
        retryable: false,
        rules,
    })
}

fn read_ledger(open: &Open) -> R<Ledger> {
    match open
        .vault
        .managed_root()
        .read_bounded(LEDGER, LEDGER_MAX_BYTES)?
    {
        None => Ok(Ledger::default()),
        Some(bytes) => serde_json::from_slice(&bytes)
            .map_err(|_| fail(MemoryErrorCode::StorageFailed, "persona.ledger_unreadable")),
    }
}

fn write_ledger(open: &Open, ledger: &Ledger) -> R<()> {
    let bytes = canonical_bytes(ledger).expect("ledger serializes");
    open.vault.managed_root().write_atomic(LEDGER, &bytes)?;
    Ok(())
}

fn digest_of(value: &Value) -> Sha256Hex {
    sha256(&canonical_bytes(value).expect("value serializes"))
}

/// Whether a record may be projected now: active, intact and in its valid time.
fn current(memory: &CanonicalMemory, now: &Timestamp) -> bool {
    memory.status == MemoryStatus::Active
        && memory.provenance_state == ProvenanceState::Intact
        && memory.valid_from.as_ref().is_none_or(|from| from <= now)
        && memory.valid_until.as_ref().is_none_or(|until| until > now)
}

fn names_people(memory: &CanonicalMemory) -> bool {
    !memory.subject_ids.is_empty()
        || matches!(&memory.body, MemoryBody::Episode(e) if !e.participants.is_empty())
}

fn identity_tombstoned(tombstones: &[Tombstone], identity: &IdentityMetadata) -> bool {
    tombstones.iter().any(|t| {
        t.targets.iter().any(|target| {
            target.record_kind == RecordKind::Identity
                && target.record_id == identity.identity_id.as_str()
                && target.revision.is_none_or(|r| r == identity.revision)
        })
    })
}

/// Field-level changes of the Identity part and per-item knowledge changes.
/// Removed items name only their ID and source: their text is now excluded.
fn diff(old: Option<&LastExport>, new: &Projection) -> Value {
    let a = old.map(|l| serde_json::to_value(&l.projection).expect("projection"));
    let b = serde_json::to_value(new).expect("projection");
    let mut fields = Vec::new();
    for pointer in [
        "/source",
        "/display_name",
        "/persona/background",
        "/persona/personality",
        "/persona/voice",
        "/persona/motivation",
        "/persona/values",
        "/persona/rules",
        "/persona/limits",
        "/style/rules",
        "/style/anti_examples",
        "/examples",
        "/interests",
        "/boundaries/refuse",
        "/boundaries/ai_disclosure",
        "/boundaries/avoid_topics",
        "/boundaries/forbidden_words",
    ] {
        match &a {
            None => fields.push(json!({"field": pointer, "change": "added"})),
            Some(a) if a.pointer(pointer) != b.pointer(pointer) => {
                fields.push(json!({"field": pointer, "change": "changed"}))
            }
            Some(_) => {}
        }
    }
    let empty = Vec::new();
    let before: BTreeMap<&str, &KnowledgeItem> = old
        .map_or(&empty, |l| &l.projection.public_knowledge)
        .iter()
        .map(|i| (i.item_id.as_str(), i))
        .collect();
    let after: BTreeMap<&str, &KnowledgeItem> = new
        .public_knowledge
        .iter()
        .map(|i| (i.item_id.as_str(), i))
        .collect();
    let mut knowledge = Vec::new();
    let mut unchanged = 0u64;
    for id in before.keys().chain(after.keys()).collect::<BTreeSet<_>>() {
        match (before.get(id), after.get(id)) {
            (Some(x), Some(y)) if x == y => unchanged += 1,
            (Some(_), Some(y)) => {
                knowledge.push(json!({"itemId": id, "change": "changed", "source": y.source}))
            }
            (None, Some(y)) => {
                knowledge.push(json!({"itemId": id, "change": "added", "source": y.source}))
            }
            (Some(x), None) => {
                knowledge.push(json!({"itemId": id, "change": "removed", "source": x.source}))
            }
            (None, None) => {}
        }
    }
    json!({"baselineVersion": old.map(|l| l.version), "fields": fields, "knowledge": knowledge,
        "unchangedKnowledge": unchanged})
}

fn last_export_summary(state: Option<&AudienceState>) -> Value {
    match state.and_then(|s| s.last_export.as_ref()) {
        None => Value::Null,
        Some(last) => json!({
            "version": last.version, "projectionSha256": last.projection_sha256,
            "identityId": last.projection.source.identity_id,
            "identityRevision": last.projection.source.identity_revision,
            "memoryIds": last.memory_ids, "exportedAt": last.exported_at,
            "fileName": last.file_name, "fileSha256": last.file_sha256,
        }),
    }
}

impl Workspace {
    pub(super) fn persona_options(&self, a: &wire::PersonaOptionsArgs) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let now = Timestamp::from_unix_ms(self.now_ms())
            .ok_or_else(|| fail(MemoryErrorCode::ClockRegression, "persona.clock"))?;
        let tombstones: Vec<Tombstone> = latest_records(&open.vault, &pin, RecordKind::Tombstone)?;
        let mut identities = Vec::new();
        for identity in latest_records::<IdentityMetadata>(&open.vault, &pin, RecordKind::Identity)?
        {
            if identity_tombstoned(&tombstones, &identity) {
                continue;
            }
            let bytes = open.vault.read_object(&pin, &identity.content_hash)?;
            let block = match std::str::from_utf8(&bytes).map(persona::extract_block) {
                Ok(Ok(_)) => "valid",
                Ok(Err(ContractError::Invalid(v)))
                    if v.iter().any(|v| v.rule == "persona.block_missing") =>
                {
                    "missing"
                }
                _ => "invalid",
            };
            identities.push(json!({"identityId": identity.identity_id,
                "revision": identity.revision, "slug": identity.slug, "title": identity.title,
                "sensitivity": identity.sensitivity, "publicBlock": block}));
        }
        let mut eligible: Vec<Value> = canonical_memories(&open.vault, &pin, Canonical::Active)?
            .iter()
            .filter(|m| m.sensitivity == Sensitivity::Public && current(m, &now))
            .map(|m| {
                json!({"memoryId": m.memory_id, "revision": m.revision, "title": m.title,
                    "type": m.memory_type(), "tags": m.tags, "namesPeople": names_people(m)})
            })
            .collect();
        let total = eligible.len();
        eligible.truncate(OPTIONS_MAX_MEMORIES);
        let ledger = read_ledger(&open)?;
        Ok(json!({
            "audience": a.audience,
            "projectionId": persona::projection_id(open.vault.vault_id().as_str(), a.audience),
            "identities": identities, "publicMemories": eligible,
            "publicMemoriesTotal": total, "truncated": total > OPTIONS_MAX_MEMORIES,
            "lastExport": last_export_summary(ledger.audiences.get(&a.audience)),
        }))
    }

    pub(super) fn persona_preview(&self, a: &wire::PersonaPreviewArgs) -> R<Value> {
        let open = self.open()?;
        let ledger = read_ledger(&open)?;
        Ok(self
            .persona_compute(
                &open,
                &ledger,
                a.audience,
                &a.identity_id,
                a.identity_revision,
                &a.memory_ids,
            )?
            .result)
    }

    fn persona_compute(
        &self,
        open: &Open,
        ledger: &Ledger,
        audience: Audience,
        identity_id: &IdentityId,
        identity_revision: Revision,
        memory_ids: &[MemoryId],
    ) -> R<Computed> {
        let pin = open.vault.pin_current()?;
        let now = Timestamp::from_unix_ms(self.now_ms())
            .ok_or_else(|| fail(MemoryErrorCode::ClockRegression, "persona.clock"))?;
        let tombstones: Vec<Tombstone> = latest_records(&open.vault, &pin, RecordKind::Tombstone)?;
        let identities: Vec<IdentityMetadata> =
            latest_records(&open.vault, &pin, RecordKind::Identity)?;
        let identity = identities
            .iter()
            .find(|i| &i.identity_id == identity_id && !identity_tombstoned(&tombstones, i))
            .ok_or_else(|| fail(MemoryErrorCode::NotFound, "persona.identity_missing"))?;
        if identity.revision != identity_revision {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "persona.identity_stale",
            ));
        }
        let markdown = open.vault.read_object(&pin, &identity.content_hash)?;
        let markdown = String::from_utf8(markdown)
            .map_err(|_| fail(MemoryErrorCode::StorageFailed, "persona.identity_utf8"))?;
        let extracted = persona::extract_block(&markdown)
            .map_err(|e| contract_fail("persona.identity_block", &e))?;

        let projection_id = persona::projection_id(open.vault.vault_id().as_str(), audience);
        let selected: BTreeSet<&MemoryId> = memory_ids.iter().collect();
        let mut found = BTreeSet::new();
        let mut items = Vec::new();
        let mut people = Vec::new();
        let (mut not_public, mut not_selected) = (BTreeMap::<String, u64>::new(), 0u64);
        let (mut selected_not_public, mut selected_unavailable) = (0u64, 0u64);
        for memory in canonical_memories(&open.vault, &pin, Canonical::AllStatuses)? {
            let chosen = selected.contains(&memory.memory_id);
            if chosen {
                found.insert(memory.memory_id.clone());
            }
            let live = current(&memory, &now);
            let public = memory.sensitivity == Sensitivity::Public;
            match (chosen, live, public) {
                (true, true, true) => {
                    let item_id = persona::item_id(&projection_id, &memory.memory_id);
                    if names_people(&memory) {
                        people.push(item_id.clone());
                    }
                    items.push(KnowledgeItem {
                        item_id,
                        text: memory.content.clone(),
                        source: KnowledgeSource {
                            memory_id: memory.memory_id.clone(),
                            memory_revision: memory.revision,
                        },
                        tags: memory.tags.clone(),
                    });
                }
                (true, false, _) => selected_unavailable += 1,
                (true, true, false) => selected_not_public += 1,
                (false, true, true) => not_selected += 1,
                (false, true, false) => {
                    let word = serde_json::to_value(memory.sensitivity).expect("sensitivity");
                    *not_public
                        .entry(word.as_str().expect("word").to_owned())
                        .or_default() += 1;
                }
                (false, false, _) => {}
            }
        }
        // Selected IDs that no longer resolve: deleted, forgotten or unknown.
        selected_unavailable += (selected.len() - found.len()) as u64;
        people.sort();

        let projection = Projection::new(
            projection_id.clone(),
            audience,
            identity.identity_id.clone(),
            identity.revision,
            extracted.block,
            items,
        );
        let violations = projection.validate();
        if !violations.is_empty() {
            return Err(contract_fail(
                "persona.projection_invalid",
                &ContractError::Invalid(violations),
            ));
        }
        let projection_sha256 = persona::projection_sha256(&projection);
        if canonical_bytes(&projection).expect("projection").len() > persona::MAX_DOCUMENT_BYTES / 2
        {
            return Err(fail(
                MemoryErrorCode::BudgetExceeded,
                "persona.document_size",
            ));
        }
        let state = ledger.audiences.get(&audience);
        let issued = state.and_then(|s| s.issued.as_ref());
        let version = match issued {
            Some(i) if i.projection_sha256 == projection_sha256 => i.version,
            Some(i) => i.version + 1,
            None => 1,
        };
        let last = state.and_then(|s| s.last_export.as_ref());
        let digest = digest_of(&json!({"audience": audience, "version": version,
            "projectionSha256": projection_sha256,
            "baselineVersion": last.map(|l| l.version)}));
        let mut warnings = Vec::new();
        if identity.slug == IdentitySlug::Relationship {
            warnings.push(json!({"code": "relationship_identity", "itemIds": []}));
        }
        if !people.is_empty() {
            warnings.push(json!({"code": "names_people", "itemIds": people}));
        }
        let result = json!({
            "audience": audience, "projectionId": projection_id,
            "version": version, "projectionSha256": projection_sha256,
            "previewDigest": digest, "memoryVersion": MEMORY_VERSION,
            "contract": ContractName::V1,
            "projection": projection,
            "diff": diff(last, &projection),
            "exclusions": {
                "identityLinesOutsideBlock": extracted.lines_outside,
                "otherIdentityRecords": identities.len() - 1,
                "memoriesNotPublic": not_public,
                "publicMemoriesNotSelected": not_selected,
                "selectedNotPublic": selected_not_public,
                "selectedUnavailable": selected_unavailable,
            },
            "warnings": warnings,
        });
        Ok(Computed {
            projection,
            projection_sha256,
            version,
            digest,
            result,
        })
    }

    pub(super) fn persona_export(&self, a: &wire::PersonaExportArgs, key: &str) -> R<Value> {
        let open = self.open()?;
        // One export at a time in this Core; the host lock excludes others.
        let _serial = self.persona.lock().unwrap_or_else(|e| e.into_inner());
        let mut ledger = read_ledger(&open)?;
        let key_hash = sha256(format!("persona-export\n{key}").as_bytes());
        let mut memory_ids = a.memory_ids.clone();
        memory_ids.sort();
        let fingerprint = digest_of(&json!({"audience": a.audience, "identityId": a.identity_id,
            "identityRevision": a.identity_revision, "memoryIds": memory_ids,
            "previewDigest": a.preview_digest}));
        let reserved = match ledger.receipts.get(&key_hash) {
            Some(entry) if entry.fingerprint != fingerprint => {
                return Err(fail(
                    MemoryErrorCode::IdempotencyConflict,
                    "workspace.key_reuse",
                ));
            }
            Some(entry) if entry.written => {
                let mut receipt = entry.receipt.clone();
                receipt["replayed"] = json!(true);
                return Ok(receipt);
            }
            Some(entry) => entry.document.clone(),
            None => None,
        };
        let computed = self.persona_compute(
            &open,
            &ledger,
            a.audience,
            &a.identity_id,
            a.identity_revision,
            &a.memory_ids,
        )?;
        if computed.digest != a.preview_digest {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "persona.preview_changed",
            ));
        }
        self.with_pick(
            &a.destination_token,
            PickKind::PersonaDestination,
            true,
            |folder| {
                let folder = persona_destination(&open, &folder)?;
                let document = reserved.unwrap_or_else(|| Document {
                    contract: ContractName::V1,
                    projection: computed.projection.clone(),
                    projection_sha256: computed.projection_sha256.clone(),
                    version: computed.version,
                    exported_at: Timestamp::from_unix_ms(self.now_ms()).expect("checked clock"),
                    memory_version: MEMORY_VERSION.to_owned(),
                });
                let bytes = canonical_bytes(&document).expect("document serializes");
                let file_name = format!(
                    "public-persona-{}-v{}.json",
                    a.audience.as_str(),
                    document.version
                );
                let target = folder.join(&file_name);
                let receipt = json!({
                    "audience": a.audience, "projectionId": document.projection.projection_id,
                    "version": document.version, "projectionSha256": document.projection_sha256,
                    "fileName": file_name, "fileSha256": sha256(&bytes), "bytes": bytes.len(),
                    "exportedAt": document.exported_at, "memoryVersion": document.memory_version,
                    "replayed": false,
                });
                // A different file already there is never replaced.
                if existing_differs(&target, &bytes)? {
                    return Err(fail(
                        MemoryErrorCode::InvalidRequest,
                        "persona.destination_exists",
                    ));
                }
                // Reserve the version before the file exists, so the number
                // never names different content after an interruption.
                let state = ledger.audiences.entry(a.audience).or_default();
                state.issued = Some(Issued {
                    version: document.version,
                    projection_sha256: document.projection_sha256.clone(),
                });
                ledger.receipts.insert(
                    key_hash.clone(),
                    KeyEntry {
                        fingerprint: fingerprint.clone(),
                        written: false,
                        document: Some(document.clone()),
                        receipt: receipt.clone(),
                    },
                );
                write_ledger(&open, &ledger)?;
                write_new(&target, &bytes)?;
                let state = ledger.audiences.entry(a.audience).or_default();
                state.last_export = Some(LastExport {
                    version: document.version,
                    projection_sha256: document.projection_sha256.clone(),
                    projection: document.projection.clone(),
                    memory_ids: memory_ids.clone(),
                    exported_at: document.exported_at.clone(),
                    file_name: file_name.clone(),
                    file_sha256: sha256(&bytes),
                });
                let entry = ledger.receipts.get_mut(&key_hash).expect("reserved");
                entry.written = true;
                entry.document = None;
                write_ledger(&open, &ledger)?;
                Ok(receipt)
            },
        )
    }
}

/// The picked folder must be a verified local directory outside any Git
/// repository or worktree and outside every Vault root.
fn persona_destination(open: &Open, folder: &Path) -> R<PathBuf> {
    let verified = verify_data_root(folder, &RootPolicy::default()).map_err(|reason| {
        let mut f = root_rejected(reason);
        f.0.rules[0] = "persona.destination_rejected".into();
        f
    })?;
    let path = verified.path().to_path_buf();
    let vault_root = open.vault.managed_root().root();
    let inside_vault = path.starts_with(vault_root)
        || path.ancestors().any(|a| {
            a.join(enouia_memory_contract::layout::vault_descriptor())
                .exists()
        });
    if inside_vault {
        return Err(Fail(WorkspaceError::new(
            MemoryErrorCode::InvalidRequest,
            &[
                "persona.destination_rejected",
                "persona.destination_in_vault",
            ],
        )));
    }
    Ok(path)
}

/// Whether a file at `target` exists with bytes other than `bytes`.
fn existing_differs(target: &Path, bytes: &[u8]) -> R<bool> {
    match std::fs::symlink_metadata(target) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(fail(
            MemoryErrorCode::StorageFailed,
            "persona.destination_unreadable",
        )),
        Ok(meta) if !meta.is_file() || meta.len() != bytes.len() as u64 => Ok(true),
        Ok(_) => Ok(std::fs::read(target).map_err(|_| {
            fail(
                MemoryErrorCode::StorageFailed,
                "persona.destination_unreadable",
            )
        })? != bytes),
    }
}

/// Create the file, never replacing one; an identical file already written by
/// an interrupted attempt is accepted. A failed write removes only the file
/// this call created.
fn write_new(target: &Path, bytes: &[u8]) -> R<()> {
    let created = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target);
    let mut file = match created {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {
            return if existing_differs(target, bytes)? {
                Err(fail(
                    MemoryErrorCode::InvalidRequest,
                    "persona.destination_exists",
                ))
            } else {
                Ok(())
            };
        }
        Err(_) => {
            return Err(fail(MemoryErrorCode::StorageFailed, "persona.write_failed"));
        }
    };
    if file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .is_err()
    {
        drop(file);
        let _ = std::fs::remove_file(target);
        return Err(fail(MemoryErrorCode::StorageFailed, "persona.write_failed"));
    }
    Ok(())
}

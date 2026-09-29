//! The import pipeline (IMPORT_REVIEW §2):
//!
//! 1. read the selected file (a regular file, not a link; bounded size),
//!    hash it, and read it again to confirm the hash;
//! 2. if these exact bytes were imported before, record a duplicate import
//!    and stop (Raw is deduplicated by hash);
//! 3. archive: commit the bytes as a Raw object with import revision 1
//!    (`archived`, or `partial` when no adapter recognizes them);
//! 4. parse in deterministic units and commit them in batches, each batch
//!    with the next import revision (cursor, counts, coverage);
//! 5. reconcile with earlier imports by upstream identity: unchanged
//!    messages are not duplicated, changed ones become new revisions.
//!
//! Cancellation stops between batches; a crash loses at most the batch in
//! flight. `resume_import` continues from the committed cursor, re-reading
//! the archived bytes from the Vault (not the original file) and refusing a
//! different adapter version.

use crate::detect::{Detection, detect};
use crate::parsed::{ParentLink, ParsedInput, ParsedMessage, Unit, sniff_media_type, warning};
use crate::zip::ZipLimits;
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::{ObjectKind, OperationKind};
use enouia_memory_contract::common::{ActorRef, Sensitivity, Warning};
use enouia_memory_contract::foundation::Cancellation;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{AttachmentId, BranchId, CommitId, ImportId, PolicyId, SourceId};
use enouia_memory_contract::import::{
    ConversationCoverage, CursorUnit, ImportCounts, ImportCursor, ImportManifest, ImportStatus,
    InputKind,
};
use enouia_memory_contract::json::{Revision, SchemaVersion, canonical_bytes};
use enouia_memory_contract::ports::{CommitRequest, IdempotencyScope, StagedObject, StagedRecord};
use enouia_memory_contract::record::{RecordKind, RecordRef, parse_record};
use enouia_memory_contract::source::{Availability, SourceKind, SourceRecord};
use enouia_memory_contract::time::Timestamp;
use enouia_memory_vault::{Fault, Vault, VaultError};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

type Result<T> = std::result::Result<T, VaultError>;

#[derive(Clone, Debug)]
pub struct ImportOptions {
    pub owner: ActorRef,
    /// Local alias for the upstream account (never a login or e-mail).
    pub account_scope: String,
    pub access_policy_id: PolicyId,
    pub sensitivity: Sensitivity,
    /// Units (conversations, files, sessions) per commit.
    pub batch_units: usize,
    pub max_input_bytes: u64,
    pub zip: ZipLimits,
}

impl ImportOptions {
    pub fn new(owner: ActorRef, account_scope: &str, access_policy_id: PolicyId) -> Self {
        Self {
            owner,
            account_scope: account_scope.to_owned(),
            access_policy_id,
            sensitivity: Sensitivity::Private,
            batch_units: 50,
            max_input_bytes: 1024 * 1024 * 1024,
            zip: ZipLimits::default(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct ImportReport {
    pub import_id: ImportId,
    pub manifest: ImportManifest,
    /// Commits made by this call.
    pub commits: usize,
}

/// Never cancels.
pub struct NeverCancel;

impl Cancellation for NeverCancel {
    fn is_cancelled(&self) -> bool {
        false
    }
}

fn invalid(rule: &'static str) -> VaultError {
    VaultError::invalid(vec![rule])
}

fn read_input(path: &Path, max: u64) -> Result<Vec<u8>> {
    let meta = std::fs::symlink_metadata(path)?;
    if enouia_memory_vault::platform::is_reparse_point(&meta) || !meta.is_file() {
        return Err(invalid("import.input_not_regular_file"));
    }
    if meta.len() > max {
        return Err(invalid("import.input_too_large"));
    }
    let first = std::fs::read(path)?;
    let second = std::fs::read(path)?;
    if first.len() as u64 != meta.len() || sha256(&first) != sha256(&second) {
        return Err(invalid("import.input_changed"));
    }
    Ok(first)
}

/// Upstream identity used to reconcile repeated imports (DATA_MODEL §1):
/// provider + account alias + conversation + message; different accounts
/// never merge.
type Key = (String, String, String, String);

struct Existing {
    sources: BTreeMap<Key, (SourceId, Revision, Sha256Hex)>,
    imports: Vec<ImportManifest>,
}

fn load_existing(vault: &Vault) -> Result<Existing> {
    let pin = vault.pin_current()?;
    let mut existing = Existing {
        sources: BTreeMap::new(),
        imports: Vec::new(),
    };
    for entry in vault.record_entries(&pin, RecordKind::Source)? {
        let reference = RecordRef::new(RecordKind::Source, &entry.record_id, entry.revision);
        let bytes = vault.read_record(&pin, &reference)?;
        let source: SourceRecord =
            parse_record(&bytes).map_err(|_| VaultError::corrupt("source"))?;
        if let (Some(p), Some(a), Some(c), Some(m)) = (
            &source.provider,
            &source.account_scope,
            &source.original_conversation_id,
            &source.original_message_id,
        ) {
            existing.sources.insert(
                (p.clone(), a.clone(), c.clone(), m.clone()),
                (
                    source.source_id.clone(),
                    source.revision,
                    source.content_hash.clone(),
                ),
            );
        }
    }
    for entry in vault.record_entries(&pin, RecordKind::Import)? {
        let reference = RecordRef::new(RecordKind::Import, &entry.record_id, entry.revision);
        let bytes = vault.read_record(&pin, &reference)?;
        existing
            .imports
            .push(parse_record(&bytes).map_err(|_| VaultError::corrupt("import"))?);
    }
    Ok(existing)
}

fn staged(
    kind: RecordKind,
    id: &str,
    revision: Revision,
    record: &ImportManifest,
) -> Result<StagedRecord> {
    let value = serde_json::to_value(record).map_err(|_| invalid("import.serialize"))?;
    Ok(StagedRecord {
        record_kind: kind,
        record_id: id.to_owned(),
        revision,
        bytes: canonical_bytes(&value).map_err(|e| VaultError::invalid(e.rules()))?,
    })
}

fn commit(
    vault: &Vault,
    options: &ImportOptions,
    manifest: &ImportManifest,
    mut records: Vec<StagedRecord>,
    objects: Vec<StagedObject>,
    expected: Vec<(RecordKind, String, Option<Revision>)>,
) -> Result<()> {
    records.push(staged(
        RecordKind::Import,
        manifest.import_id.as_str(),
        manifest.revision,
        manifest,
    )?);
    let mut digest = Vec::new();
    for r in &records {
        digest.extend_from_slice(sha256(&r.bytes).as_str().as_bytes());
    }
    for o in &objects {
        digest.extend_from_slice(o.hash.as_str().as_bytes());
    }
    let key = format!("{}:{}", manifest.import_id, manifest.revision.get());
    vault.commit(CommitRequest {
        commit_id: CommitId::from_random(vault.random_id_bytes()),
        expected_commit_id: None,
        principal: options.owner.clone(),
        operation_kind: OperationKind::Import,
        idempotency: IdempotencyScope {
            principal_id: options.owner.actor_id.clone(),
            operation_kind: OperationKind::Import,
            key_hash: sha256(key.as_bytes()),
        },
        request_payload_hash: sha256(&digest),
        expected_revisions: expected,
        records,
        objects,
    })?;
    Ok(())
}

fn next_revision(manifest: &ImportManifest) -> Result<Revision> {
    Revision::new(manifest.revision.get() + 1).ok_or_else(|| invalid("number.out_of_range"))
}

/// Record, without parsing, a byte-identical re-import.
fn record_duplicate(
    vault: &Vault,
    options: &ImportOptions,
    original: &ImportManifest,
    now: &Timestamp,
) -> Result<ImportReport> {
    let import_id = ImportId::from_random(vault.random_id_bytes());
    let status = if original.adapter.is_some() {
        ImportStatus::Completed
    } else {
        ImportStatus::Partial
    };
    let manifest = ImportManifest {
        schema_version: SchemaVersion,
        import_id: import_id.clone(),
        revision: Revision::new(1).expect("one"),
        status,
        input_kind: original.input_kind,
        provider: original.provider.clone(),
        account_scope: original.account_scope.clone(),
        input_object_hash: original.input_object_hash.clone(),
        input_size_bytes: original.input_size_bytes,
        received_at: now.clone(),
        updated_at: now.clone(),
        adapter: original.adapter.clone(),
        source_schema_observed: original.source_schema_observed.clone(),
        members: Vec::new(),
        cursor: ImportCursor {
            unit: original.cursor.unit,
            completed: 0,
            total: Some(0),
        },
        counts: ImportCounts::default(),
        coverage: Vec::new(),
        warnings: vec![warning("duplicate_input", None)],
        duplicate_of: Some(original.import_id.clone()),
        extensions: BTreeMap::new(),
    };
    commit(
        vault,
        options,
        &manifest,
        Vec::new(),
        Vec::new(),
        Vec::new(),
    )?;
    Ok(ImportReport {
        import_id,
        manifest,
        commits: 1,
    })
}

/// Import one user-selected file. Returns when the import is completed,
/// partial (unsupported format, archived only), or cancelled between
/// batches (status stays `parsing`; call `resume_import`).
pub fn import_file(
    vault: &Vault,
    path: &Path,
    options: &ImportOptions,
    cancel: &dyn Cancellation,
) -> Result<ImportReport> {
    if !enouia_memory_contract::source::is_account_alias(&options.account_scope) {
        return Err(invalid("source.account_alias"));
    }
    let bytes = read_input(path, options.max_input_bytes)?;
    let hash = sha256(&bytes);
    let existing = load_existing(vault)?;
    let now = vault.now()?;
    let mut latest: BTreeMap<&ImportId, &ImportManifest> = BTreeMap::new();
    for import in &existing.imports {
        latest.insert(&import.import_id, import);
    }
    if let Some(original) = latest
        .values()
        .filter(|i| i.input_object_hash == hash && i.duplicate_of.is_none())
        .min_by(|a, b| a.received_at.cmp(&b.received_at))
    {
        if matches!(
            original.status,
            ImportStatus::Archived | ImportStatus::Parsing
        ) {
            return Err(invalid("import.resume_existing"));
        }
        return record_duplicate(vault, options, original, &now);
    }
    let detection = detect(&bytes, &options.zip);
    let import_id = ImportId::from_random(vault.random_id_bytes());
    let raw = StagedObject {
        kind: ObjectKind::Raw,
        hash: hash.clone(),
        bytes: bytes.clone(),
    };
    let base = |input_kind: InputKind, status| ImportManifest {
        schema_version: SchemaVersion,
        import_id: import_id.clone(),
        revision: Revision::new(1).expect("one"),
        status,
        input_kind,
        provider: None,
        account_scope: None,
        input_object_hash: hash.clone(),
        input_size_bytes: bytes.len() as u64,
        received_at: now.clone(),
        updated_at: now.clone(),
        adapter: None,
        source_schema_observed: None,
        members: Vec::new(),
        cursor: ImportCursor {
            unit: CursorUnit::File,
            completed: 0,
            total: None,
        },
        counts: ImportCounts::default(),
        coverage: Vec::new(),
        warnings: Vec::new(),
        duplicate_of: None,
        extensions: BTreeMap::new(),
    };
    match detection {
        Detection::Unsupported {
            input_kind,
            members,
            warnings,
        } => {
            let mut manifest = base(input_kind, ImportStatus::Partial);
            manifest.members = members;
            manifest.warnings = warnings;
            commit(vault, options, &manifest, Vec::new(), vec![raw], Vec::new())?;
            Ok(ImportReport {
                import_id,
                manifest,
                commits: 1,
            })
        }
        Detection::Parsed(parsed) => {
            let mut manifest = base(parsed.input_kind, ImportStatus::Archived);
            manifest.provider = parsed.provider.clone();
            manifest.account_scope = parsed
                .provider
                .as_ref()
                .map(|_| options.account_scope.clone());
            manifest.adapter = Some(parsed.adapter.clone());
            manifest.source_schema_observed = Some(parsed.source_schema_observed.clone());
            manifest.members = parsed.members.clone();
            manifest.warnings = parsed.warnings.clone();
            manifest.cursor = ImportCursor {
                unit: parsed.unit_kind,
                completed: 0,
                total: Some(parsed.units.len() as u64),
            };
            commit(vault, options, &manifest, Vec::new(), vec![raw], Vec::new())?;
            let mut report = run_batches(vault, options, manifest, &parsed, cancel)?;
            report.commits += 1;
            Ok(report)
        }
    }
}

/// Continue an import from its committed cursor, parsing the archived bytes.
pub fn resume_import(
    vault: &Vault,
    import_id: &ImportId,
    options: &ImportOptions,
    cancel: &dyn Cancellation,
) -> Result<ImportReport> {
    let existing = load_existing(vault)?;
    let manifest = existing
        .imports
        .into_iter()
        .find(|i| &i.import_id == import_id)
        .ok_or_else(|| VaultError::new(MemoryErrorCode::NotFound, Fault::NotFound))?;
    if !matches!(
        manifest.status,
        ImportStatus::Archived | ImportStatus::Parsing
    ) {
        return Ok(ImportReport {
            import_id: import_id.clone(),
            manifest,
            commits: 0,
        });
    }
    let pin = vault.pin_current()?;
    let bytes = vault.read_object(&pin, &manifest.input_object_hash)?;
    let Detection::Parsed(parsed) = detect(&bytes, &options.zip) else {
        return Err(invalid("import.adapter_changed"));
    };
    if manifest.adapter.as_ref() != Some(&parsed.adapter)
        || manifest.source_schema_observed.as_deref()
            != Some(parsed.source_schema_observed.as_str())
        || manifest.cursor.total != Some(parsed.units.len() as u64)
    {
        return Err(invalid("import.adapter_changed"));
    }
    run_batches(vault, options, manifest, &parsed, cancel)
}

/// Topologically ordered positions (parents first) and branch labels.
fn branches(unit: &Unit) -> (Vec<usize>, Vec<Option<usize>>, u64) {
    let index: BTreeMap<&str, usize> = unit
        .messages
        .iter()
        .enumerate()
        .filter_map(|(i, m)| m.upstream_id.as_deref().map(|id| (id, i)))
        .collect();
    let parent = |i: usize| match &unit.messages[i].parent {
        ParentLink::Upstream(id) => index.get(id.as_str()).copied(),
        _ => None,
    };
    let depth = |mut i: usize| {
        let mut d = 0;
        while let Some(p) = parent(i) {
            d += 1;
            i = p;
            if d > unit.messages.len() {
                break;
            }
        }
        d
    };
    let mut order: Vec<usize> = (0..unit.messages.len()).collect();
    order.sort_by_key(|&i| (depth(i), i));
    if !unit.threaded {
        return (order, vec![None; unit.messages.len()], 0);
    }
    let mut main = BTreeSet::new();
    let leaf = unit
        .current_leaf
        .as_deref()
        .and_then(|id| index.get(id).copied())
        .or_else(|| order.last().copied());
    let mut at = leaf;
    while let Some(i) = at {
        if !main.insert(i) {
            break;
        }
        at = parent(i);
    }
    let mut children: BTreeMap<Option<usize>, usize> = BTreeMap::new();
    for i in 0..unit.messages.len() {
        *children.entry(parent(i)).or_default() += 1;
    }
    let mut labels: Vec<Option<usize>> = vec![None; unit.messages.len()];
    let mut next = 1;
    for &i in &order {
        labels[i] = Some(if main.contains(&i) {
            0
        } else {
            match parent(i) {
                Some(p) if !main.contains(&p) && children.get(&Some(p)) == Some(&1) => {
                    labels[p].unwrap_or(0)
                }
                _ => {
                    next += 1;
                    next - 1
                }
            }
        });
    }
    let count = labels.iter().flatten().collect::<BTreeSet<_>>().len() as u64;
    (order, labels, count)
}

struct Batch {
    records: Vec<StagedRecord>,
    objects: Vec<StagedObject>,
    expected: Vec<(RecordKind, String, Option<Revision>)>,
}

#[allow(clippy::too_many_arguments)]
fn build_unit(
    vault: &Vault,
    options: &ImportOptions,
    parsed: &ParsedInput,
    manifest: &ImportManifest,
    unit: &Unit,
    existing: &mut Existing,
    counts: &mut ImportCounts,
    batch: &mut Batch,
    warnings: &mut BTreeSet<String>,
) -> Result<ConversationCoverage> {
    let now = vault.now()?;
    let (order, labels, branch_count) = branches(unit);
    let branch_ids: BTreeMap<usize, BranchId> = labels
        .iter()
        .flatten()
        .copied()
        .collect::<BTreeSet<_>>()
        .into_iter()
        .map(|label| (label, BranchId::from_random(vault.random_id_bytes())))
        .collect();
    let provider = parsed.provider.clone();
    let account = provider.as_ref().map(|_| options.account_scope.clone());
    let key_of = |m: &ParsedMessage| -> Option<Key> {
        Some((
            provider.clone()?,
            account.clone()?,
            unit.conversation_id.clone()?,
            m.upstream_id.clone()?,
        ))
    };
    // Target source IDs first, so parents resolve in any order.
    let mut ids: Vec<(SourceId, Option<(Revision, Sha256Hex)>)> = Vec::new();
    let mut by_upstream: BTreeMap<String, SourceId> = BTreeMap::new();
    let mut seen_keys = BTreeSet::new();
    let mut duplicate = vec![false; unit.messages.len()];
    for (i, message) in unit.messages.iter().enumerate() {
        let key = key_of(message);
        if let Some(key) = &key
            && !seen_keys.insert(key.clone())
        {
            duplicate[i] = true;
        }
        let (id, prior) = match key.as_ref().and_then(|k| existing.sources.get(k)) {
            Some((id, rev, hash)) => (id.clone(), Some((*rev, hash.clone()))),
            None => (SourceId::from_random(vault.random_id_bytes()), None),
        };
        if let Some(upstream) = &message.upstream_id {
            by_upstream
                .entry(upstream.clone())
                .or_insert_with(|| id.clone());
        }
        ids.push((id, prior));
    }
    let mut coverage = ConversationCoverage {
        original_conversation_id: unit.conversation_id.clone(),
        message_count: 0,
        branch_count,
        earliest_source_time: None,
        latest_source_time: None,
        unknown_time_count: 0,
        missing_parents: 0,
        unparseable: unit.unparseable,
    };
    for &i in &order {
        let message = &unit.messages[i];
        if duplicate[i] {
            coverage.unparseable += 1;
            warnings.insert("duplicate_message_id".to_owned());
            continue;
        }
        let (source_id, prior) = &ids[i];
        if prior
            .as_ref()
            .is_some_and(|(_, hash)| hash == &message.content_hash)
        {
            counts.sources_unchanged += 1;
            continue;
        }
        let revision = match prior {
            Some((rev, _)) => {
                batch
                    .expected
                    .push((RecordKind::Source, source_id.to_string(), Some(*rev)));
                counts.sources_revised += 1;
                Revision::new(rev.get() + 1).ok_or_else(|| invalid("number.out_of_range"))?
            }
            None => {
                batch
                    .expected
                    .push((RecordKind::Source, source_id.to_string(), None));
                counts.sources_created += 1;
                Revision::new(1).expect("one")
            }
        };
        let parents = match &message.parent {
            ParentLink::Root => json!([]),
            ParentLink::Upstream(up) => match by_upstream.get(up) {
                Some(id) => json!([id]),
                None => json!("unknown"),
            },
            ParentLink::Missing => {
                coverage.missing_parents += 1;
                counts.missing_parents += 1;
                warnings.insert("missing_parent".to_owned());
                json!("unknown")
            }
        };
        let branch = match labels[i] {
            Some(label) => json!(branch_ids[&label]),
            None => Value::Null,
        };
        let mut attachment_ids = Vec::new();
        for attachment in &message.attachments {
            let attachment_id = AttachmentId::from_random(vault.random_id_bytes());
            let object_hash = attachment.bytes.as_ref().map(|b| sha256(b));
            match attachment.availability {
                Availability::Present => counts.attachments_present += 1,
                Availability::ExternalReference => counts.attachments_external += 1,
                Availability::Quarantined => counts.attachments_quarantined += 1,
                _ => counts.attachments_missing += 1,
            }
            if let (Some(bytes), Some(hash)) = (&attachment.bytes, &object_hash) {
                batch.objects.push(StagedObject {
                    kind: ObjectKind::Asset,
                    hash: hash.clone(),
                    bytes: bytes.clone(),
                });
            }
            let record = json!({
                "schema_version": 1,
                "attachment_id": attachment_id,
                "revision": 1,
                "source_id": source_id,
                "original_name": attachment.original_name,
                "claimed_media_type": attachment.claimed_media_type,
                "detected_media_type": attachment.bytes.as_deref().and_then(sniff_media_type),
                "size_bytes": attachment.size_bytes,
                "object_hash": object_hash,
                "availability": attachment.availability,
                "external_reference": attachment.external_reference,
                "sensitivity": options.sensitivity,
                "created_at": now,
                "extensions": {},
            });
            batch.records.push(StagedRecord {
                record_kind: RecordKind::Attachment,
                record_id: attachment_id.to_string(),
                revision: Revision::new(1).expect("one"),
                bytes: canonical_bytes(&record).map_err(|e| VaultError::invalid(e.rules()))?,
            });
            batch
                .expected
                .push((RecordKind::Attachment, attachment_id.to_string(), None));
            attachment_ids.push(attachment_id);
        }
        let record = json!({
            "schema_version": 1,
            "source_id": source_id,
            "revision": revision.get(),
            "source_kind": message.kind,
            "provider": if message.kind == SourceKind::ExportMessage { provider.clone() } else { None },
            "account_scope": if message.kind == SourceKind::ExportMessage { account.clone() } else { None },
            "import_id": manifest.import_id,
            "raw_object_hash": manifest.input_object_hash,
            "content_hash": message.content_hash,
            "original_conversation_id": unit.conversation_id,
            "original_message_id": message.upstream_id,
            "parent_source_ids": parents,
            "branch_id": branch,
            "locator": message.locator,
            "original_time": message.original_time,
            "original_timezone": null,
            "occurred_at": message.occurred_at,
            "captured_at": now,
            "time_precision": message.time_precision,
            "speaker_role": message.role,
            "author_label": null,
            "evidence_class": message.evidence,
            "completeness": message.completeness,
            "sensitivity": options.sensitivity,
            "access_policy_id": options.access_policy_id,
            "attachment_refs": attachment_ids,
            "parser_version": format!("{}/{}", parsed.adapter.name, parsed.adapter.version),
            "parse_warnings": message.warnings,
            "manual_assertion": null,
            "agent_submission": null,
            "created_at": now,
            "extensions": {},
        });
        batch.records.push(StagedRecord {
            record_kind: RecordKind::Source,
            record_id: source_id.to_string(),
            revision,
            bytes: canonical_bytes(&record).map_err(|e| VaultError::invalid(e.rules()))?,
        });
        for w in &message.warnings {
            warnings.insert(w.code.clone());
        }
        if let Some(key) = key_of(message) {
            existing.sources.insert(
                key,
                (source_id.clone(), revision, message.content_hash.clone()),
            );
        }
        coverage.message_count += 1;
        match &message.occurred_at {
            None => coverage.unknown_time_count += 1,
            Some(t) => {
                if coverage.earliest_source_time.as_ref().is_none_or(|e| t < e) {
                    coverage.earliest_source_time = Some(t.clone());
                }
                if coverage.latest_source_time.as_ref().is_none_or(|l| t > l) {
                    coverage.latest_source_time = Some(t.clone());
                }
            }
        }
    }
    counts.conversations += 1;
    counts.branches += branch_count;
    counts.unparseable += coverage.unparseable;
    Ok(coverage)
}

fn run_batches(
    vault: &Vault,
    options: &ImportOptions,
    mut manifest: ImportManifest,
    parsed: &ParsedInput,
    cancel: &dyn Cancellation,
) -> Result<ImportReport> {
    let mut existing = load_existing(vault)?;
    let total = parsed.units.len();
    let mut commits = 0;
    let mut next = manifest.cursor.completed as usize;
    let mut warnings: BTreeSet<String> = manifest.warnings.iter().map(|w| w.code.clone()).collect();
    if total == 0 {
        manifest.revision = next_revision(&manifest)?;
        manifest.status = ImportStatus::Completed;
        manifest.updated_at = vault.now()?;
        commit(
            vault,
            options,
            &manifest,
            Vec::new(),
            Vec::new(),
            Vec::new(),
        )?;
        return Ok(ImportReport {
            import_id: manifest.import_id.clone(),
            manifest,
            commits: 1,
        });
    }
    while next < total {
        if cancel.is_cancelled() {
            break;
        }
        let end = (next + options.batch_units.max(1)).min(total);
        let mut batch = Batch {
            records: Vec::new(),
            objects: Vec::new(),
            expected: Vec::new(),
        };
        let mut counts = manifest.counts.clone();
        let mut coverage = manifest.coverage.clone();
        for unit in &parsed.units[next..end] {
            coverage.push(build_unit(
                vault,
                options,
                parsed,
                &manifest,
                unit,
                &mut existing,
                &mut counts,
                &mut batch,
                &mut warnings,
            )?);
        }
        let mut updated = manifest.clone();
        updated.revision = next_revision(&manifest)?;
        updated.status = if end == total {
            ImportStatus::Completed
        } else {
            ImportStatus::Parsing
        };
        updated.cursor.completed = end as u64;
        updated.counts = counts;
        updated.coverage = coverage;
        updated.warnings = warnings
            .iter()
            .map(|code| Warning {
                code: code.clone(),
                pointer: None,
            })
            .collect();
        updated.updated_at = vault.now()?;
        let mut unique = BTreeSet::new();
        batch.objects.retain(|o| unique.insert(o.hash.clone()));
        commit(
            vault,
            options,
            &updated,
            batch.records,
            batch.objects,
            batch.expected,
        )?;
        commits += 1;
        manifest = updated;
        next = end;
    }
    Ok(ImportReport {
        import_id: manifest.import_id.clone(),
        manifest,
        commits,
    })
}

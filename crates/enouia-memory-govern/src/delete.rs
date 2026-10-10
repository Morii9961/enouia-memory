//! Forgetting and purging (MV-3.4, PRIVACY_RECOVERY §6).
//!
//! Both start as an owner-only delete candidate and an owner review that
//! confirms exactly which records and objects are affected; the review
//! commits one tombstone and moves the deletion epoch (the immediate
//! barrier: the canonical view and pinned reads refuse the targets from that
//! commit on).
//!
//! - `logical_delete` (forget) keeps the text, restricted, so the decision
//!   can be revisited.
//! - `purge` names what is destroyed: the memory's revisions, the candidates
//!   that proposed or targeted it, and with `with_dependents` the cited
//!   sources and the raw objects that hold them. `purge_preview` lists this
//!   first, including other memories that would lose their evidence and how
//!   many other sources share a raw object (a message inside an imported
//!   export cannot be removed alone; the whole object goes or stays).
//!   `complete_purge` deletes the bytes and commits a receipt that says,
//!   store by store, what was purged, what is outside the Vault, and that
//!   backups are still pending; there is no "globally erased" state.
//!
//! Restoring an older backup can bring forgotten content back. The deletion
//! ledger (tombstones only: IDs, hashes, modes; no text) is kept apart from
//! the backups; `reconcile_deletions` re-applies it to a restored Vault,
//! and only then opens its network gate (B02).

use crate::evidence::EvidenceSpec;
use crate::propose::{Origin, Proposal, Proposed, propose};
use crate::review::{
    Built, Context, Decision, OwnerConfirmation, PlannedIds, confirm, plan, reference,
};
use crate::util::{Result, all_latest, invalid, latest, revision, staged, stale};
use enouia_memory_contract::candidate::{
    CandidateRecord, ProposalKind, ProposedType, ReviewRecord,
};
use enouia_memory_contract::commit::{
    DeleteMode, DeleteScope, OperationKind, PurgeReceipt, Tombstone,
};
use enouia_memory_contract::common::ActorRef;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{CommitId, DeleteId, MemoryId, PurgeReceiptId};
use enouia_memory_contract::import::ImportManifest;
use enouia_memory_contract::memory::{CanonicalMemory, MemoryType};
use enouia_memory_contract::ports::{CommitOutcome, CommitPin, CommitRequest, IdempotencyScope};
use enouia_memory_contract::record::{RecordKind, parse_value};
use enouia_memory_contract::source::SourceRecord;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_vault::Vault;
use enouia_memory_vault::purge::PurgeReport;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// How long backups may keep purged content before the owner must rebuild
/// them (shown on the receipt as the latest purge deadline).
pub const BACKUP_PURGE_WINDOW_MS: i64 = 30 * 24 * 3600 * 1000;

/// An owner request to forget or purge one memory.
pub fn delete_proposal(memory: &CanonicalMemory, mode: DeleteMode, scope: DeleteScope) -> Proposal {
    let mut details = Map::new();
    details.insert("mode".into(), json!(mode));
    details.insert("scope".into(), json!(scope));
    let cited = &memory.evidence[0];
    Proposal {
        kind: ProposalKind::Delete,
        proposed_type: match memory.memory_type() {
            MemoryType::Fact => ProposedType::Fact,
            MemoryType::Preference => ProposedType::Preference,
            MemoryType::Episode => ProposedType::Episode,
            MemoryType::ProjectState => ProposedType::ProjectState,
            MemoryType::SessionCheckpoint => ProposedType::SessionCheckpoint,
        },
        content: format!("delete {}", memory.memory_id),
        details: Some(details),
        evidence: vec![EvidenceSpec::content(
            cited.source_id.clone(),
            cited.source_revision,
        )],
        reason: "owner delete request".into(),
        sensitivity: None,
        target_memory_id: Some(memory.memory_id.clone()),
        target_identity_id: None,
        expected_revision: Some(memory.revision),
        effective_from: None,
        reopens_candidate_id: None,
    }
}

fn choice(candidate: &CandidateRecord) -> Result<(DeleteMode, DeleteScope)> {
    let field = |key: &str| {
        candidate
            .proposed_details
            .as_ref()
            .and_then(|d| d.get(key))
            .cloned()
    };
    let mode = match field("mode") {
        Some(v) => serde_json::from_value(v).map_err(|_| invalid("candidate.details_field"))?,
        None => DeleteMode::LogicalDelete,
    };
    let scope = match field("scope") {
        Some(v) => serde_json::from_value(v).map_err(|_| invalid("candidate.details_field"))?,
        None => DeleteScope::AllRevisions,
    };
    Ok((mode, scope))
}

/// Exactly what a deletion covers, and what it would break.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct PurgeImpact {
    /// Tombstone targets: `(kind, id, revision or all)`.
    pub targets: Vec<(RecordKind, String, Option<u64>)>,
    /// Raw objects destroyed with their cited sources.
    pub object_hashes: Vec<Sha256Hex>,
    /// Other memories whose evidence would be purged (they stay, with
    /// broken provenance, until the owner revises or deletes them).
    pub losing_provenance: Vec<MemoryId>,
    /// For each destroyed raw object: how many other sources it holds.
    pub shared_raw: Vec<(Sha256Hex, usize)>,
}

fn target_value(kind: RecordKind, id: &str, rev: Option<u64>) -> Value {
    json!({"record_kind": kind, "record_id": id, "revision": rev})
}

/// Compute the impact of deleting `memory` (at its latest revision).
fn impact(
    vault: &Vault,
    pin: &CommitPin,
    memory: &CanonicalMemory,
    mode: DeleteMode,
    scope: DeleteScope,
    exclude: Option<&str>,
) -> Result<PurgeImpact> {
    let mut out = PurgeImpact::default();
    let id = memory.memory_id.as_str();
    if scope == DeleteScope::SelectedRevisions {
        out.targets.push((
            RecordKind::Memory,
            id.to_owned(),
            Some(memory.revision.get()),
        ));
        if mode == DeleteMode::Purge {
            add_context_dependents(vault, pin, &mut out)?;
        }
        return Ok(out);
    }
    out.targets.push((RecordKind::Memory, id.to_owned(), None));
    if mode == DeleteMode::LogicalDelete {
        return Ok(out);
    }
    // Every revision's evidence and the candidates behind them.
    let mut candidates = BTreeSet::new();
    let mut sources: BTreeSet<(String, u64)> = BTreeSet::new();
    for r in 1..=memory.revision.get() {
        let Some(old): Option<CanonicalMemory> = revision(
            vault,
            pin,
            RecordKind::Memory,
            id,
            enouia_memory_contract::json::Revision::new(r).expect("revision"),
        )?
        else {
            continue;
        };
        for e in &old.evidence {
            sources.insert((e.source_id.to_string(), e.source_revision.get()));
        }
        if let Some((review, _)) =
            latest::<ReviewRecord>(vault, pin, RecordKind::Review, old.review_id.as_str())?
        {
            candidates.insert(review.candidate_id.to_string());
        }
    }
    for candidate in all_latest::<CandidateRecord>(vault, pin, RecordKind::Candidate)? {
        if candidate
            .target_memory_id
            .as_ref()
            .is_some_and(|t| t.as_str() == id)
            && candidate.proposal_kind != ProposalKind::Delete
        {
            candidates.insert(candidate.candidate_id.to_string());
        }
    }
    for candidate in candidates {
        if Some(candidate.as_str()) != exclude {
            out.targets.push((RecordKind::Candidate, candidate, None));
        }
    }
    if scope != DeleteScope::WithDependents {
        add_context_dependents(vault, pin, &mut out)?;
        return Ok(out);
    }
    let mut purged_sources: BTreeSet<String> = BTreeSet::new();
    let mut raw = BTreeSet::new();
    for (source_id, r) in &sources {
        purged_sources.insert(source_id.clone());
        let source: Option<SourceRecord> = revision(
            vault,
            pin,
            RecordKind::Source,
            source_id,
            enouia_memory_contract::json::Revision::new(*r).expect("revision"),
        )?;
        if let Some(hash) = source.and_then(|s| s.raw_object_hash) {
            raw.insert(hash);
        }
    }
    for source_id in &purged_sources {
        out.targets
            .push((RecordKind::Source, source_id.clone(), None));
    }
    // A raw object holds a whole export: every source cut from it loses its
    // text too. Find them through the imports that received the object.
    let imports: Vec<ImportManifest> = all_latest(vault, pin, RecordKind::Import)?;
    let entries = vault.record_entries(pin, RecordKind::Source)?;
    let mut affected: BTreeSet<String> = purged_sources.clone();
    for hash in &raw {
        let groups: BTreeSet<String> = imports
            .iter()
            .filter(|i| &i.input_object_hash == hash)
            .map(|i| i.import_id.to_string())
            .collect();
        let inside: Vec<&String> = entries
            .iter()
            .filter(|e| e.group_ids.iter().any(|g| groups.contains(g)))
            .map(|e| &e.record_id)
            .collect();
        let others = inside
            .iter()
            .filter(|s| !purged_sources.contains(**s))
            .count();
        affected.extend(inside.into_iter().cloned());
        out.shared_raw.push((hash.clone(), others));
        out.object_hashes.push(hash.clone());
    }
    for other in crate::view::canonical_memories(vault, pin, crate::view::Canonical::AllStatuses)? {
        if other.memory_id.as_str() != id
            && other
                .evidence
                .iter()
                .any(|e| affected.contains(e.source_id.as_str()))
        {
            out.losing_provenance.push(other.memory_id.clone());
        }
    }
    add_context_dependents(vault, pin, &mut out)?;
    Ok(out)
}

/// Saved context and exact Mock bodies are content stores now (MV-5). Purge
/// follows their explicit references, including derived assistant events and
/// checkpoints. It never leaves a capsule containing the purged statement.
fn add_context_dependents(vault: &Vault, pin: &CommitPin, out: &mut PurgeImpact) -> Result<()> {
    use enouia_memory_contract::context::{ContextCapsule, ContextInspection, DispatchRecord};
    use enouia_memory_contract::session::{
        CheckpointSourceRef, Coverage, EventKind, SessionCheckpoint, SessionEvent, SessionRecord,
    };
    let capsules: Vec<ContextCapsule> = all_latest(vault, pin, RecordKind::Capsule)?;
    let inspections: Vec<ContextInspection> = all_latest(vault, pin, RecordKind::Inspection)?;
    let dispatches: Vec<DispatchRecord> = all_latest(vault, pin, RecordKind::Dispatch)?;
    let events: Vec<SessionEvent> = all_latest(vault, pin, RecordKind::SessionEvent)?;
    let sessions: Vec<SessionRecord> = all_latest(vault, pin, RecordKind::Session)?;
    let checkpoints: Vec<SessionCheckpoint> = all_latest(vault, pin, RecordKind::Checkpoint)?;
    let mut targets: BTreeSet<(RecordKind, String)> = out
        .targets
        .iter()
        .map(|(k, id, _)| (*k, id.clone()))
        .collect();
    let mut hashes: BTreeSet<_> = out.object_hashes.iter().cloned().collect();
    loop {
        let previous = targets.len();
        let has = |kind, id: &str| targets.contains(&(kind, id.to_owned()));
        let affected: Vec<_> = capsules
            .iter()
            .filter(|c| {
                has(RecordKind::Capsule, c.capsule_id.as_str())
                    || c.memory_items()
                        .any(|m| has(RecordKind::Memory, m.memory_id.as_str()))
                    || c.provenance
                        .iter()
                        .any(|s| has(RecordKind::Source, s.source_id.as_str()))
                    || c.recent_turns
                        .iter()
                        .any(|e| has(RecordKind::SessionEvent, e.event_id.as_str()))
                    || c.recent_session_checkpoints
                        .iter()
                        .any(|p| has(RecordKind::Checkpoint, p.checkpoint_id.as_str()))
            })
            .collect();
        for capsule in affected {
            targets.insert((RecordKind::Capsule, capsule.capsule_id.to_string()));
            for inspection in inspections
                .iter()
                .filter(|i| i.capsule_id == capsule.capsule_id)
            {
                targets.insert((RecordKind::Inspection, inspection.inspection_id.to_string()));
            }
            for dispatch in dispatches
                .iter()
                .filter(|d| d.capsule_id == capsule.capsule_id)
            {
                targets.insert((RecordKind::Dispatch, dispatch.dispatch_id.to_string()));
                hashes.extend(dispatch.messages.iter().map(|m| m.content_hash.clone()));
            }
            for event in events.iter().filter(|e| {
                e.request_id.as_ref() == Some(&capsule.request_id)
                    && matches!(
                        e.kind,
                        EventKind::AssistantCompleted
                            | EventKind::AssistantChunk
                            | EventKind::TurnFailed
                    )
            }) {
                targets.insert((RecordKind::SessionEvent, event.event_id.to_string()));
                if let Some(content) = &event.content_ref {
                    hashes.insert(content.object_hash.clone());
                }
            }
        }
        for checkpoint in &checkpoints {
            let affected=events.iter().any(|e|targets.contains(&(RecordKind::SessionEvent,e.event_id.to_string())) && e.session_id==checkpoint.session_id && match &checkpoint.coverage {
                Coverage::EventIds{event_ids}=>event_ids.contains(&e.event_id),Coverage::Range{from_sequence,to_sequence}=>e.sequence>=*from_sequence&&e.sequence<=*to_sequence,
            }) ||checkpoint.decisions.iter().chain(&checkpoint.open_loops).any(|i|i.source_refs.iter().any(|s|matches!(s,CheckpointSourceRef::Source{source_id,..} if targets.contains(&(RecordKind::Source,source_id.to_string())))));
            if affected {
                targets.insert((RecordKind::Checkpoint, checkpoint.checkpoint_id.to_string()));
            }
        }
        // Content addressing means an exact message body can be shared with
        // another event/dispatch; preview and purge include those references.
        for event in &events {
            if event
                .source_refs
                .iter()
                .any(|r| targets.contains(&(RecordKind::Source, r.source_id.to_string())))
            {
                targets.insert((RecordKind::SessionEvent, event.event_id.to_string()));
                if let Some(content) = &event.content_ref {
                    hashes.insert(content.object_hash.clone());
                }
            }
            if event
                .content_ref
                .as_ref()
                .is_some_and(|c| hashes.contains(&c.object_hash))
            {
                targets.insert((RecordKind::SessionEvent, event.event_id.to_string()));
            }
        }
        for dispatch in &dispatches {
            if dispatch
                .messages
                .iter()
                .any(|m| hashes.contains(&m.content_hash))
            {
                targets.insert((RecordKind::Dispatch, dispatch.dispatch_id.to_string()));
            }
        }
        for session in &sessions {
            for invocation in &session.provider_invocations {
                if targets.contains(&(RecordKind::Dispatch, invocation.dispatch_id.to_string()))
                    || targets.contains(&(
                        RecordKind::SessionEvent,
                        invocation.input_event_id.to_string(),
                    ))
                    || targets.contains(&(RecordKind::Session, session.session_id.to_string()))
                {
                    hashes.insert(invocation.wire_hash.clone());
                }
            }
        }
        if targets.len() == previous {
            break;
        }
    }
    for (kind, id) in targets {
        if !out.targets.iter().any(|(k, i, _)| *k == kind && i == &id) {
            out.targets.push((kind, id, None));
        }
    }
    out.object_hashes = hashes.into_iter().collect();
    Ok(())
}

/// What deleting `memory_id` in this way would cover and break.
pub fn purge_preview(
    vault: &Vault,
    memory_id: &MemoryId,
    mode: DeleteMode,
    scope: DeleteScope,
) -> Result<PurgeImpact> {
    let pin = vault.pin_current()?;
    let (memory, _): (CanonicalMemory, _) =
        latest(vault, &pin, RecordKind::Memory, memory_id.as_str())?
            .ok_or_else(|| invalid("candidate.target_missing"))?;
    impact(vault, &pin, &memory, mode, scope, None)
}

/// Stage the tombstone of a confirmed delete candidate. Returns the delete
/// binding the review records.
pub(crate) fn confirm_delete(
    ctx: &Context,
    built: &mut Built,
    candidate: &CandidateRecord,
    ids: &PlannedIds,
    results: &mut Vec<Value>,
    targets: &mut Vec<Value>,
) -> Result<Value> {
    let (mode, scope) = choice(candidate)?;
    let target = candidate
        .target_memory_id
        .as_ref()
        .ok_or_else(|| invalid("candidate.target_fields"))?;
    // A forgotten memory can still be purged: read it past its tombstone.
    let (memory, current): (CanonicalMemory, _) =
        latest(ctx.vault, &ctx.pin, RecordKind::Memory, target.as_str())?
            .ok_or_else(|| invalid("review.target_missing"))?;
    if Some(current) != candidate.expected_revision {
        return Err(stale());
    }
    built.expect(RecordKind::Memory, target.as_str(), Some(current))?;
    targets.push(reference(RecordKind::Memory, target.as_str(), current));
    let impact = impact(
        ctx.vault,
        &ctx.pin,
        &memory,
        mode,
        scope,
        Some(candidate.candidate_id.as_str()),
    )?;
    let tombstone_targets: Vec<Value> = impact
        .targets
        .iter()
        .map(|(kind, id, rev)| target_value(*kind, id, *rev))
        .collect();
    let epoch = ctx.vault.stored_commit(&ctx.pin)?.deletion_epoch + 1;
    let tombstone = json!({
        "schema_version": 1,
        "delete_id": ids.delete_id,
        "mode": mode,
        "scope": scope,
        "targets": tombstone_targets,
        "object_hashes": impact.object_hashes,
        "requested_by": ctx.header.owner,
        "review_id": ids.review_id,
        "deletion_epoch": epoch,
        "created_at": ctx.now,
    });
    let one = crate::util::one();
    let _: (Tombstone, _) = staged(
        RecordKind::Tombstone,
        ids.delete_id.as_str(),
        one,
        &tombstone,
    )?;
    built.expect(RecordKind::Tombstone, ids.delete_id.as_str(), None)?;
    built.push(
        RecordKind::Tombstone,
        ids.delete_id.as_str(),
        one,
        tombstone,
    );
    results.push(reference(
        RecordKind::Tombstone,
        ids.delete_id.as_str(),
        one,
    ));
    built.operation = match mode {
        DeleteMode::LogicalDelete => OperationKind::LogicalDelete,
        DeleteMode::Purge => OperationKind::Purge,
    };
    Ok(json!({"mode": mode, "scope": scope, "targets": tombstone_targets}))
}

/// What a completed purge produced.
#[derive(Clone, Debug, PartialEq)]
pub struct PurgeOutcome {
    pub files: PurgeReport,
    pub receipt_id: PurgeReceiptId,
    pub commit: CommitOutcome,
}

fn tombstone(vault: &Vault, pin: &CommitPin, id: &DeleteId) -> Result<Tombstone> {
    latest::<Tombstone>(vault, pin, RecordKind::Tombstone, id.as_str())?
        .map(|(t, _)| t)
        .ok_or_else(|| invalid("purge.tombstone"))
}

/// Delete the bytes a published purge tombstone names and commit the
/// receipt. Safe to repeat: the file deletion is idempotent and the receipt
/// commit replays under the same key.
pub fn complete_purge(
    vault: &Vault,
    delete_id: &DeleteId,
    owner: &ActorRef,
    key: &[u8],
) -> Result<PurgeOutcome> {
    if owner != &vault.descriptor().created_by {
        return Err(invalid("review.not_vault_owner"));
    }
    let pin = vault.pin_current()?;
    let tombstone = tombstone(vault, &pin, delete_id)?;
    if tombstone.mode != DeleteMode::Purge {
        return Err(invalid("purge.tombstone"));
    }
    let files = vault.purge_files()?;
    let now: Timestamp = vault.now()?;
    let has = |kind: RecordKind| tombstone.targets.iter().any(|t| t.record_kind == kind);
    let state = |present: bool| if present { "purged" } else { "not_present" };
    let at = |present: bool| if present { json!(now) } else { Value::Null };
    let raw = has(RecordKind::Source) || !tombstone.object_hashes.is_empty();
    let store = |name: &str, present: bool| json!({"store": name, "state": state(present), "confirmed_at": at(present), "detail_code": null});
    let deadline = Timestamp::from_unix_ms(now.unix_ms() + BACKUP_PURGE_WINDOW_MS)
        .ok_or_else(|| invalid("purge.deadline"))?;
    let receipt_id = PurgeReceiptId::from_random(vault.random_id_bytes());
    let receipt = json!({
        "schema_version": 1,
        "receipt_id": receipt_id,
        "delete_id": delete_id,
        "stores": [
            store("canonical", has(RecordKind::Memory)),
            store("raw", raw),
            store("assets", false),
            store("candidates", has(RecordKind::Candidate)),
            store("sessions", has(RecordKind::SessionEvent) || has(RecordKind::Dispatch)),
            store("checkpoints", has(RecordKind::Checkpoint)),
            store("capsules", has(RecordKind::Capsule)),
            store("index", false),
            store("embeddings", false),
            store("staging", false),
            {"store": "exports", "state": "not_manageable", "confirmed_at": null,
             "detail_code": "outside_vault"},
            {"store": "backups", "state": "pending", "confirmed_at": null,
             "detail_code": "rebuild_backups"},
            store("device_replicas", false),
            store("gateway", false),
        ],
        "overall_state": "backup_purge_pending",
        "latest_purge_deadline": deadline,
        "created_at": now,
    });
    let (_, record) = staged::<PurgeReceipt>(
        RecordKind::PurgeReceipt,
        receipt_id.as_str(),
        crate::util::one(),
        &receipt,
    )?;
    let scope = IdempotencyScope {
        principal_id: owner.actor_id.clone(),
        operation_kind: OperationKind::Purge,
        key_hash: sha256(key),
    };
    let commit = vault.commit(CommitRequest {
        commit_id: CommitId::from_random(vault.random_id_bytes()),
        expected_commit_id: None,
        principal: owner.clone(),
        operation_kind: OperationKind::Purge,
        idempotency: scope,
        request_payload_hash: sha256(delete_id.as_str().as_bytes()),
        expected_revisions: Vec::new(),
        records: vec![record],
        objects: Vec::new(),
    })?;
    let receipt_id = match &commit {
        CommitOutcome::Replayed { receipt, .. } => receipt
            .records
            .iter()
            .find(|r| r.record_kind == RecordKind::PurgeReceipt)
            .and_then(|r| PurgeReceiptId::parse(&r.record_id).ok())
            .unwrap_or(receipt_id),
        CommitOutcome::Committed { .. } => receipt_id,
    };
    Ok(PurgeOutcome {
        files,
        receipt_id,
        commit,
    })
}

/// The deletion ledger: every tombstone at the head, oldest first. It holds
/// IDs, hashes, and modes only, so it can be kept apart from the backups.
pub fn deletion_ledger(vault: &Vault) -> Result<Vec<Tombstone>> {
    let pin = vault.pin_current()?;
    let mut out: Vec<Tombstone> = all_latest(vault, &pin, RecordKind::Tombstone)?;
    out.sort_by_key(|t| t.deletion_epoch);
    Ok(out)
}

pub fn ledger_bytes(ledger: &[Tombstone]) -> Vec<u8> {
    enouia_memory_contract::json::canonical_bytes(&json!(ledger)).expect("ledger serializes")
}

pub fn parse_ledger(bytes: &[u8]) -> Result<Vec<Tombstone>> {
    let value: Value = serde_json::from_slice(bytes).map_err(|_| invalid("ledger.malformed"))?;
    value
        .as_array()
        .ok_or_else(|| invalid("ledger.malformed"))?
        .iter()
        .map(|v| parse_value::<Tombstone>(v).map_err(|_| invalid("ledger.malformed")))
        .collect()
}

/// What reconciliation did with each ledger entry.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Reconciled {
    /// Re-applied to the restored Vault (a new tombstone each).
    pub applied: Vec<DeleteId>,
    /// Already covered by a tombstone in the restored Vault.
    pub already: Vec<DeleteId>,
    /// Their memories are not in the restored Vault.
    pub not_present: Vec<DeleteId>,
}

fn covered(existing: &[Tombstone], memory: &str, mode: DeleteMode) -> bool {
    existing.iter().any(|t| {
        (t.mode == mode || t.mode == DeleteMode::Purge)
            && t.targets
                .iter()
                .any(|x| x.record_kind == RecordKind::Memory && x.record_id == memory)
    })
}

/// Re-apply a deletion ledger to a restored Vault, each deletion through an
/// owner-confirmed plan (and a purge through `complete_purge`), then open
/// the restore's network gate. Nothing forgotten goes out before this.
pub fn reconcile_deletions(
    vault: &Vault,
    ledger: &[Tombstone],
    confirmation: &OwnerConfirmation,
) -> Result<Reconciled> {
    let owner = &confirmation.owner;
    if owner != &vault.descriptor().created_by {
        return Err(invalid("review.not_vault_owner"));
    }
    let mut out = Reconciled::default();
    let mut entries: Vec<&Tombstone> = ledger.iter().collect();
    entries.sort_by_key(|t| t.deletion_epoch);
    for entry in entries {
        let pin = vault.pin_current()?;
        let existing: Vec<Tombstone> = all_latest(vault, &pin, RecordKind::Tombstone)?;
        let memories: Vec<&str> = entry
            .targets
            .iter()
            .filter(|t| t.record_kind == RecordKind::Memory)
            .map(|t| t.record_id.as_str())
            .collect();
        let mut any = false;
        let mut applied = false;
        for memory_id in memories {
            let Some((memory, _)) =
                latest::<CanonicalMemory>(vault, &pin, RecordKind::Memory, memory_id)?
            else {
                continue;
            };
            any = true;
            if covered(&existing, memory_id, entry.mode) {
                continue;
            }
            let key = format!("reconcile-{}-{memory_id}", entry.delete_id);
            let proposal = delete_proposal(&memory, entry.mode, entry.scope);
            let id = match propose(
                vault,
                &proposal,
                &Origin::owner(owner.clone()),
                key.as_bytes(),
            )? {
                Proposed::Stored(written) => written.id,
                Proposed::DuplicateOf(id) => id,
            };
            let (candidate, rev): (CandidateRecord, _) = latest(
                vault,
                &vault.pin_current()?,
                RecordKind::Candidate,
                id.as_str(),
            )?
            .ok_or_else(|| invalid("candidate.missing"))?;
            let shown = plan(
                vault,
                &[Decision::Accept {
                    candidate_id: candidate.candidate_id.clone(),
                    revision: rev,
                }],
                owner,
                confirmation.surface,
            )?;
            confirm(vault, &shown, confirmation)?;
            if entry.mode == DeleteMode::Purge {
                let delete_id = shown.ids[0].delete_id.clone();
                complete_purge(vault, &delete_id, owner, format!("{key}-purge").as_bytes())?;
            }
            applied = true;
        }
        if applied {
            out.applied.push(entry.delete_id.clone());
        } else if any {
            out.already.push(entry.delete_id.clone());
        } else {
            out.not_present.push(entry.delete_id.clone());
        }
    }
    enouia_memory_vault::backup::mark_reconciled(vault)?;
    Ok(out)
}

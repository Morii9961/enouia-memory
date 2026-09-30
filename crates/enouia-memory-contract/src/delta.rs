//! Scoped validation of one commit (ADR-MEM-39).
//!
//! `set::validate_records` over a whole Vault grows with its history. A store
//! instead validates each commit on a *scoped* set: every record of the small
//! kinds (all revisions), plus only those revisions of the bulk kinds
//! (sources, attachments, session events) that the rules touching the delta
//! read, as listed by [`delta_needs`]. It runs the same rules on the scoped set
//! without and with the delta and keeps the violations that appear, counted
//! per `(rule, path)` ([`validate_delta`]).
//!
//! Why this is exact: a rule instance that reads no delta record gives the
//! same result with and without the delta, so it cancels, even when the
//! scoped set lacks some of its inputs. A rule instance that reads a delta
//! record has every input in the scoped set (that is what the needs
//! guarantee), so it gives the same result as on the whole Vault. The signed
//! difference therefore equals the one over the whole Vault; the property test
//! `tests/delta.rs` checks this on every synthetic set and mutation case.
//!
//! Two premises: bulk records are read by rules only by exact revision, by
//! record ID (all revisions), or by group (the sources that cite one import,
//! the events of one session); and a later import revision cannot change the
//! received bytes that earlier sources cite (`import.input_changed`).

use crate::candidate::ReviewRecord;
use crate::commit::Tombstone;
use crate::common::SourceRevisionRef;
use crate::error::Violation;
use crate::import::ImportStatus;
use crate::record::{AnyRecord, RecordKind};
use crate::session::{CheckpointSourceRef, SessionCheckpoint};
use crate::set::{RecordSet, validate_records};
use std::collections::{BTreeMap, BTreeSet};

/// Bulk kinds grow with imported and appended history. Every other stored
/// kind is loaded completely for each commit.
pub const fn is_bulk(kind: RecordKind) -> bool {
    matches!(
        kind,
        RecordKind::Source | RecordKind::Attachment | RecordKind::SessionEvent
    )
}

/// Documents a scoped set must contain.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum Need {
    /// One revision of one record.
    Revision(RecordKind, String, u64),
    /// Every revision of one record.
    AllRevisions(RecordKind, String),
    /// Every revision of every source that cites this import in any revision.
    ImportSources(String),
    /// Every event of this session.
    SessionEvents(String),
    /// The manifest of this commit (its review and tombstone IDs and epochs).
    Commit(String),
}

/// `(kind, id, revision)` of a stored record; None for kinds a Vault never
/// stores as records (commits, audit events, capsules, inspections,
/// dispatches, capability snapshots).
pub fn record_key(record: &AnyRecord) -> Option<(RecordKind, String, u64)> {
    Some(match record {
        AnyRecord::Source(r) => (
            RecordKind::Source,
            r.source_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Attachment(r) => (
            RecordKind::Attachment,
            r.attachment_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Project(r) => (
            RecordKind::Project,
            r.project_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Memory(r) => (
            RecordKind::Memory,
            r.memory_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Candidate(r) => (
            RecordKind::Candidate,
            r.candidate_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Review(r) => (RecordKind::Review, r.review_id.to_string(), 1),
        AnyRecord::Identity(r) => (
            RecordKind::Identity,
            r.identity_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Session(r) => (
            RecordKind::Session,
            r.session_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::SessionEvent(r) => (RecordKind::SessionEvent, r.event_id.to_string(), 1),
        AnyRecord::Checkpoint(r) => (
            RecordKind::Checkpoint,
            r.checkpoint_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Tombstone(r) => (RecordKind::Tombstone, r.delete_id.to_string(), 1),
        AnyRecord::PurgeReceipt(r) => (RecordKind::PurgeReceipt, r.receipt_id.to_string(), 1),
        AnyRecord::Approval(r) => (RecordKind::Approval, r.approval_id.to_string(), 1),
        AnyRecord::Policy(r) => (
            RecordKind::Policy,
            r.policy_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Import(r) => (
            RecordKind::Import,
            r.import_id.to_string(),
            r.revision.get(),
        ),
        AnyRecord::Capsule(r) => (RecordKind::Capsule, r.capsule_id.to_string(), 1),
        AnyRecord::Inspection(r) => (RecordKind::Inspection, r.inspection_id.to_string(), 1),
        AnyRecord::Dispatch(r) => (RecordKind::Dispatch, r.dispatch_id.to_string(), 1),
        AnyRecord::Commit(_) | AnyRecord::AuditEvent(_) | AnyRecord::ProviderCapabilities(_) => {
            return None;
        }
    })
}

fn source_need(reference: &SourceRevisionRef) -> Need {
    Need::Revision(
        RecordKind::Source,
        reference.source_id.to_string(),
        reference.source_revision.get(),
    )
}

fn evidence_needs<'a>(items: impl Iterator<Item = &'a crate::common::EvidenceRef>) -> Vec<Need> {
    items
        .map(|item| {
            Need::Revision(
                RecordKind::Source,
                item.source_id.to_string(),
                item.source_revision.get(),
            )
        })
        .collect()
}

fn review_needs(review: &ReviewRecord) -> Vec<Need> {
    let mut out: Vec<Need> = review.evidence_refs.iter().map(source_need).collect();
    for reference in review
        .resulting_records
        .iter()
        .chain(&review.target_expected_revisions)
    {
        out.push(Need::Revision(
            reference.record_kind,
            reference.record_id.clone(),
            reference.revision.get(),
        ));
    }
    out.push(Need::Commit(review.commit_id.to_string()));
    out
}

fn checkpoint_needs(checkpoint: &SessionCheckpoint) -> Vec<Need> {
    let mut out = vec![Need::SessionEvents(checkpoint.session_id.to_string())];
    for item in checkpoint.decisions.iter().chain(&checkpoint.open_loops) {
        for reference in &item.source_refs {
            if let CheckpointSourceRef::Source {
                source_id,
                source_revision,
            } = reference
            {
                out.push(Need::Revision(
                    RecordKind::Source,
                    source_id.to_string(),
                    source_revision.get(),
                ));
            }
        }
    }
    out.push(Need::Commit(checkpoint.base_vault_commit_id.to_string()));
    out
}

fn tombstone_needs(tombstone: &Tombstone) -> Vec<Need> {
    tombstone
        .targets
        .iter()
        .map(|target| match target.revision {
            Some(revision) => {
                Need::Revision(target.record_kind, target.record_id.clone(), revision.get())
            }
            None => Need::AllRevisions(target.record_kind, target.record_id.clone()),
        })
        .collect()
}

/// Bulk documents and commits that the rules about `record` read. Small-kind
/// references are not listed: small kinds are always loaded completely.
pub fn record_needs(record: &AnyRecord) -> Vec<Need> {
    match record {
        AnyRecord::Memory(memory) => evidence_needs(memory.evidence.iter()),
        AnyRecord::Candidate(candidate) => evidence_needs(candidate.evidence.iter()),
        AnyRecord::Review(review) => review_needs(review),
        AnyRecord::Checkpoint(checkpoint) => checkpoint_needs(checkpoint),
        AnyRecord::Tombstone(tombstone) => tombstone_needs(tombstone),
        AnyRecord::Session(session) => vec![Need::SessionEvents(session.session_id.to_string())],
        AnyRecord::SessionEvent(event) => vec![Need::SessionEvents(event.session_id.to_string())],
        AnyRecord::Capsule(capsule) => capsule_needs(capsule),
        _ => Vec::new(),
    }
}

fn capsule_needs(capsule: &crate::context::ContextCapsule) -> Vec<Need> {
    let mut needs = vec![Need::Commit(capsule.vault_commit_id.to_string())];
    needs.extend(capsule.provenance.iter().map(|s| {
        Need::Revision(
            RecordKind::Source,
            s.source_id.to_string(),
            s.source_revision.get(),
        )
    }));
    if let Some(session) = &capsule.session_id {
        needs.push(Need::SessionEvents(session.to_string()));
    }
    needs
}

/// Everything a scoped set must hold, beyond every small-kind record of the
/// head, to validate `delta` on top of `head` (the head's small-kind records;
/// bulk lists may be empty).
pub fn delta_needs(head: &RecordSet, delta: &[AnyRecord]) -> BTreeSet<Need> {
    let mut needs = BTreeSet::new();
    let mut keys = BTreeSet::new();
    let mut ids = BTreeSet::new();
    let mut sessions = BTreeSet::new();
    let mut cited = BTreeSet::new();
    let mut completed: BTreeSet<String> = BTreeSet::new();
    // Imports completed at the head (latest revision) or in the delta.
    let mut latest: BTreeMap<&str, (u64, ImportStatus)> = BTreeMap::new();
    for import in &head.imports {
        let entry = latest
            .entry(import.import_id.as_str())
            .or_insert((0, import.status));
        if import.revision.get() > entry.0 {
            *entry = (import.revision.get(), import.status);
        }
    }
    completed.extend(
        latest
            .into_iter()
            .filter(|(_, (_, status))| *status == ImportStatus::Completed)
            .map(|(id, _)| id.to_owned()),
    );
    for record in delta {
        if let AnyRecord::Import(import) = record
            && import.status == ImportStatus::Completed
        {
            completed.insert(import.import_id.to_string());
        }
    }
    for record in delta {
        let Some((kind, id, revision)) = record_key(record) else {
            continue;
        };
        needs.insert(Need::AllRevisions(kind, id.clone()));
        needs.extend(record_needs(record));
        match record {
            AnyRecord::Source(source) => {
                if let Some(import) = &source.import_id {
                    cited.insert(import.to_string());
                    if completed.contains(import.as_str()) {
                        needs.insert(Need::ImportSources(import.to_string()));
                    }
                }
            }
            AnyRecord::Import(import) if completed.contains(import.import_id.as_str()) => {
                needs.insert(Need::ImportSources(import.import_id.to_string()));
            }
            AnyRecord::SessionEvent(event) => {
                sessions.insert(event.session_id.to_string());
            }
            _ => {}
        }
        keys.insert((kind, id.clone(), revision));
        ids.insert((kind, id));
    }
    // Small records whose rules read a delta document: their other inputs
    // must be present too.
    let touches = |need: &Need| match need {
        Need::Revision(kind, id, revision) => keys.contains(&(*kind, id.clone(), *revision)),
        Need::AllRevisions(kind, id) => ids.contains(&(*kind, id.clone())),
        Need::SessionEvents(session) => sessions.contains(session),
        Need::ImportSources(import) => cited.contains(import),
        Need::Commit(_) => false,
    };
    let mut extend = |own: Vec<Need>| {
        if own.iter().any(touches) {
            needs.extend(own);
        }
    };
    head.memories
        .iter()
        .for_each(|m| extend(evidence_needs(m.evidence.iter())));
    head.candidates
        .iter()
        .for_each(|c| extend(evidence_needs(c.evidence.iter())));
    head.reviews.iter().for_each(|r| extend(review_needs(r)));
    head.checkpoints
        .iter()
        .for_each(|c| extend(checkpoint_needs(c)));
    head.tombstones
        .iter()
        .for_each(|t| extend(tombstone_needs(t)));
    head.sessions
        .iter()
        .for_each(|s| extend(vec![Need::SessionEvents(s.session_id.to_string())]));
    // Saved context records remain small-kind documents. Their pinned commit
    // and bulk citations must be present when checking later dispatches too.
    for capsule in &head.capsules {
        needs.extend(capsule_needs(capsule));
    }
    needs
}

/// Violations per `(rule, path)`: count in `after` minus count in `before`,
/// nonzero entries only.
pub fn signed_difference(
    before: &[Violation],
    after: &[Violation],
) -> BTreeMap<(&'static str, String), i64> {
    let mut counts: BTreeMap<(&'static str, String), i64> = BTreeMap::new();
    for v in before {
        *counts.entry((v.rule, v.path.clone())).or_default() -= 1;
    }
    for v in after {
        *counts.entry((v.rule, v.path.clone())).or_default() += 1;
    }
    counts.retain(|_, n| *n != 0);
    counts
}

/// Validate `delta` (and the new commits' stubs) against a scoped set built
/// from [`delta_needs`]: the violations it adds. Empty means the whole Vault
/// stays as consistent as it was.
pub fn validate_delta(
    before: &RecordSet,
    delta: &[AnyRecord],
    commits: &[crate::commit::CommitManifest],
) -> Vec<Violation> {
    let mut after = before.clone();
    for record in delta {
        after.push(record.clone());
    }
    after.commits.extend(commits.iter().cloned());
    let difference = signed_difference(&validate_records(before), &validate_records(&after));
    let mut out = Vec::new();
    for ((rule, path), count) in difference {
        for _ in 0..count.max(0) {
            out.push(Violation::new(rule, path.clone()));
        }
    }
    out
}

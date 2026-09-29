//! Cross-record constraints over an in-memory record set: the rules JSON
//! Schema cannot express. A set is what one Vault would contain at its head
//! commit (plus request artifacts such as capsules). This is pure validation;
//! it does not prove that a store publishes or recovers these records safely.

use crate::approval::{ApprovalBinding, ApprovalRecord};
use crate::candidate::{CandidateRecord, CandidateStatus, MergeTarget, ReviewAction, ReviewRecord};
use crate::commit::{AuditEvent, CommitManifest, PurgeReceipt, Tombstone};
use crate::common::{Sensitivity, SourceRevisionRef};
use crate::context::{
    CheckpointItemStatus, ContextCapsule, ContextInspection, Currency, Decision, DecisionReason,
    DestinationKind, DispatchRecord, LoopOrigin, MemoryRevisionRef,
};
use crate::error::{ContractError, Violation};
use crate::hash::sha256;
use crate::identity::IdentityMetadata;
use crate::ids::ProjectId;
use crate::import::{ImportManifest, ImportStatus};
use crate::json::Revision;
use crate::memory::{CanonicalMemory, MemoryBody, MemoryStatus, ProjectEntity, ProvenanceState};
use crate::policy::{
    AccessContext, EgressRule, PolicyOrigin, PolicyRecord, ResourceContext, Scope,
    derived_sensitivity_ok, egress_rule, evaluate,
};
use crate::record::{AnyRecord, Record, RecordKind, RecordRef, parse_value};
use crate::session::{
    CheckpointSourceRef, Coverage, EventKind, SessionCheckpoint, SessionEvent, SessionRecord,
};
use crate::source::{AttachmentRecord, SourceRecord};
use crate::temporal::{Effect, currency, effect};
use crate::time::Timestamp;
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default)]
pub struct RecordSet {
    pub sources: Vec<SourceRecord>,
    pub attachments: Vec<AttachmentRecord>,
    pub projects: Vec<ProjectEntity>,
    pub memories: Vec<CanonicalMemory>,
    pub candidates: Vec<CandidateRecord>,
    pub reviews: Vec<ReviewRecord>,
    pub identities: Vec<IdentityMetadata>,
    pub sessions: Vec<SessionRecord>,
    pub session_events: Vec<SessionEvent>,
    pub checkpoints: Vec<SessionCheckpoint>,
    pub commits: Vec<CommitManifest>,
    pub tombstones: Vec<Tombstone>,
    pub purge_receipts: Vec<PurgeReceipt>,
    pub audit_events: Vec<AuditEvent>,
    pub capsules: Vec<ContextCapsule>,
    pub inspections: Vec<ContextInspection>,
    pub dispatches: Vec<DispatchRecord>,
    pub approvals: Vec<ApprovalRecord>,
    pub policies: Vec<PolicyRecord>,
    pub imports: Vec<ImportManifest>,
}

const SET_KEYS: &[&str] = &[
    "description",
    "sources",
    "attachments",
    "projects",
    "memories",
    "candidates",
    "reviews",
    "identities",
    "sessions",
    "session_events",
    "checkpoints",
    "commits",
    "tombstones",
    "purge_receipts",
    "audit_events",
    "capsules",
    "inspections",
    "dispatches",
    "approvals",
    "policies",
    "imports",
];

fn load<T: Record>(value: &Value, key: &str) -> Result<Vec<T>, (String, ContractError)> {
    let Some(items) = value.get(key) else {
        return Ok(Vec::new());
    };
    let Some(items) = items.as_array() else {
        return Err((format!("/{key}"), ContractError::Malformed));
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| parse_value::<T>(item).map_err(|e| (format!("/{key}/{index}"), e)))
        .collect()
}

impl RecordSet {
    /// Add one record to its kind's list. Kinds a set does not hold
    /// (provider capability snapshots) are ignored.
    pub fn push(&mut self, record: AnyRecord) {
        match record {
            AnyRecord::Source(r) => self.sources.push(r),
            AnyRecord::Attachment(r) => self.attachments.push(r),
            AnyRecord::Project(r) => self.projects.push(r),
            AnyRecord::Memory(r) => self.memories.push(*r),
            AnyRecord::Candidate(r) => self.candidates.push(*r),
            AnyRecord::Review(r) => self.reviews.push(r),
            AnyRecord::Identity(r) => self.identities.push(r),
            AnyRecord::Session(r) => self.sessions.push(r),
            AnyRecord::SessionEvent(r) => self.session_events.push(r),
            AnyRecord::Checkpoint(r) => self.checkpoints.push(r),
            AnyRecord::Commit(r) => self.commits.push(r),
            AnyRecord::Tombstone(r) => self.tombstones.push(r),
            AnyRecord::PurgeReceipt(r) => self.purge_receipts.push(r),
            AnyRecord::AuditEvent(r) => self.audit_events.push(r),
            AnyRecord::Capsule(r) => self.capsules.push(*r),
            AnyRecord::Inspection(r) => self.inspections.push(r),
            AnyRecord::Dispatch(r) => self.dispatches.push(r),
            AnyRecord::ProviderCapabilities(_) => {}
            AnyRecord::Approval(r) => self.approvals.push(*r),
            AnyRecord::Policy(r) => self.policies.push(*r),
            AnyRecord::Import(r) => self.imports.push(*r),
        }
    }

    /// Every record is strictly parsed and individually validated first.
    pub fn from_value(value: &Value) -> Result<Self, (String, ContractError)> {
        let Some(map) = value.as_object() else {
            return Err(("/".to_owned(), ContractError::Malformed));
        };
        if let Some(key) = map.keys().find(|k| !SET_KEYS.contains(&k.as_str())) {
            return Err((
                format!("/{key}"),
                ContractError::Shape("unknown set key".into()),
            ));
        }
        Ok(Self {
            sources: load(value, "sources")?,
            attachments: load(value, "attachments")?,
            projects: load(value, "projects")?,
            memories: load(value, "memories")?,
            candidates: load(value, "candidates")?,
            reviews: load(value, "reviews")?,
            identities: load(value, "identities")?,
            sessions: load(value, "sessions")?,
            session_events: load(value, "session_events")?,
            checkpoints: load(value, "checkpoints")?,
            commits: load(value, "commits")?,
            tombstones: load(value, "tombstones")?,
            purge_receipts: load(value, "purge_receipts")?,
            audit_events: load(value, "audit_events")?,
            capsules: load(value, "capsules")?,
            inspections: load(value, "inspections")?,
            dispatches: load(value, "dispatches")?,
            approvals: load(value, "approvals")?,
            policies: load(value, "policies")?,
            imports: load(value, "imports")?,
        })
    }

    fn memory_revision(&self, id: &str, revision: Revision) -> Option<&CanonicalMemory> {
        self.memories
            .iter()
            .find(|m| m.memory_id.as_str() == id && m.revision == revision)
    }

    /// Latest revision per memory ID across the whole set.
    pub fn latest_memories(&self) -> Vec<&CanonicalMemory> {
        let mut latest: BTreeMap<&str, &CanonicalMemory> = BTreeMap::new();
        for memory in &self.memories {
            let entry = latest.entry(memory.memory_id.as_str()).or_insert(memory);
            if memory.revision > entry.revision {
                *entry = memory;
            }
        }
        latest.into_values().collect()
    }

    /// Memory revisions visible at a commit: its catalog decides `known_at`.
    pub fn memories_at(&self, commit: &CommitManifest) -> Vec<&CanonicalMemory> {
        commit
            .catalog
            .iter()
            .filter(|e| e.record_kind == RecordKind::Memory)
            .filter_map(|e| self.memory_revision(&e.record_id, e.revision))
            .collect()
    }

    fn candidate_revisions(&self, id: &str) -> Vec<&CandidateRecord> {
        let mut revisions: Vec<_> = self
            .candidates
            .iter()
            .filter(|c| c.candidate_id.as_str() == id)
            .collect();
        revisions.sort_by_key(|c| c.revision);
        revisions
    }

    fn approval(&self, id: &str) -> Option<&ApprovalRecord> {
        self.approvals.iter().find(|a| a.approval_id.as_str() == id)
    }

    /// Every policy revision in the set; `policy::evaluate` uses the latest.
    pub fn all_policies(&self) -> Vec<&PolicyRecord> {
        self.policies.iter().collect()
    }

    /// Policy revisions visible at a commit (authorization as known then).
    pub fn policies_at(&self, commit: &CommitManifest) -> Vec<&PolicyRecord> {
        commit
            .catalog
            .iter()
            .filter(|e| e.record_kind == RecordKind::Policy)
            .filter_map(|e| {
                self.policies
                    .iter()
                    .find(|p| p.policy_id.as_str() == e.record_id && p.revision == e.revision)
            })
            .collect()
    }

    /// Sensitivity and project of any resource a Dispatch may carry.
    fn resource_info(&self, reference: &RecordRef) -> Option<(Sensitivity, Option<ProjectId>)> {
        let id = reference.record_id.as_str();
        let rev = reference.revision;
        match reference.record_kind {
            RecordKind::Memory => self
                .memory_revision(id, rev)
                .map(|m| (m.sensitivity, m.project_id.clone())),
            RecordKind::Identity => self
                .identities
                .iter()
                .find(|i| i.identity_id.as_str() == id && i.revision == rev)
                .map(|i| (i.sensitivity, None)),
            RecordKind::Checkpoint => self
                .checkpoints
                .iter()
                .find(|c| c.checkpoint_id.as_str() == id && c.revision == rev)
                .map(|c| (c.sensitivity, None)),
            RecordKind::SessionEvent => self
                .session_events
                .iter()
                .find(|e| e.event_id.as_str() == id && rev.get() == 1)
                .map(|e| (e.sensitivity, None)),
            RecordKind::Source => self
                .sources
                .iter()
                .find(|s| s.source_id.as_str() == id && s.revision == rev)
                .map(|s| (s.sensitivity, None)),
            RecordKind::Attachment => self
                .attachments
                .iter()
                .find(|a| a.attachment_id.as_str() == id && a.revision == rev)
                .map(|a| (a.sensitivity, None)),
            _ => None,
        }
    }

    fn commit(&self, id: &str) -> Option<&CommitManifest> {
        self.commits.iter().find(|c| c.commit_id.as_str() == id)
    }

    fn tombstoned(&self, kind: RecordKind, id: &str, revision: Revision, epoch: u64) -> bool {
        self.tombstones.iter().any(|t| {
            t.deletion_epoch <= epoch
                && t.targets.iter().any(|target| {
                    target.record_kind == kind
                        && target.record_id == id
                        && target.revision.is_none_or(|r| r == revision)
                })
        })
    }

    /// Every `(kind, id, revision)` document in the set that a commit catalogs.
    fn cataloged_documents(&self) -> BTreeSet<(RecordKind, String, u64)> {
        let mut all = BTreeSet::new();
        let mut add = |kind, id: &str, rev: Revision| {
            all.insert((kind, id.to_owned(), rev.get()));
        };
        for r in &self.sources {
            add(RecordKind::Source, r.source_id.as_str(), r.revision);
        }
        for r in &self.attachments {
            add(RecordKind::Attachment, r.attachment_id.as_str(), r.revision);
        }
        for r in &self.projects {
            add(RecordKind::Project, r.project_id.as_str(), r.revision);
        }
        for r in &self.memories {
            add(RecordKind::Memory, r.memory_id.as_str(), r.revision);
        }
        for r in &self.candidates {
            add(RecordKind::Candidate, r.candidate_id.as_str(), r.revision);
        }
        for r in &self.identities {
            add(RecordKind::Identity, r.identity_id.as_str(), r.revision);
        }
        for r in &self.sessions {
            add(RecordKind::Session, r.session_id.as_str(), r.revision);
        }
        for r in &self.checkpoints {
            add(RecordKind::Checkpoint, r.checkpoint_id.as_str(), r.revision);
        }
        for r in &self.policies {
            add(RecordKind::Policy, r.policy_id.as_str(), r.revision);
        }
        for r in &self.imports {
            add(RecordKind::Import, r.import_id.as_str(), r.revision);
        }
        let one = Revision::new(1).expect("1 is a revision");
        for r in &self.reviews {
            add(RecordKind::Review, r.review_id.as_str(), one);
        }
        for r in &self.tombstones {
            add(RecordKind::Tombstone, r.delete_id.as_str(), one);
        }
        for r in &self.approvals {
            add(RecordKind::Approval, r.approval_id.as_str(), one);
        }
        for r in &self.purge_receipts {
            add(RecordKind::PurgeReceipt, r.receipt_id.as_str(), one);
        }
        for r in &self.session_events {
            add(RecordKind::SessionEvent, r.event_id.as_str(), one);
        }
        all
    }
}

/// Run every cross-record rule. Returns all violations (empty = consistent).
pub fn validate_set(set: &RecordSet) -> Vec<Violation> {
    let view = View::new(set);
    let mut out = records(&view);
    check_commits(&view, &mut out);
    out
}

/// Every rule except the commit chain and catalog closure (`check_commits`).
/// A store that builds each manifest from the previous one checks the chain
/// step itself (`check_commit_step`) and validates records through
/// `delta::validate_delta` on a scoped set (ADR-MEM-39).
pub fn validate_records(set: &RecordSet) -> Vec<Violation> {
    records(&View::new(set))
}

fn records(view: &View) -> Vec<Violation> {
    let mut out = Vec::new();
    check_revisions(view, &mut out);
    check_imports(view, &mut out);
    check_evidence(view, &mut out);
    check_reviews(view, &mut out);
    check_candidates(view, &mut out);
    check_supersession(view, &mut out);
    check_projects(view, &mut out);
    check_sessions(view, &mut out);
    check_deletions(view, &mut out);
    check_policies(view, &mut out);
    check_context(view, &mut out);
    out
}

/// Lookup indexes over one set, built once per validation run. Rules take a
/// `View`; fields are reached through `Deref`, and these inherent lookups
/// shadow the linear ones on `RecordSet` (first match wins, as before).
struct View<'a> {
    set: &'a RecordSet,
    source_ix: BTreeMap<(&'a str, u64), &'a SourceRecord>,
    memory_ix: BTreeMap<(&'a str, u64), &'a CanonicalMemory>,
    candidate_ix: BTreeMap<&'a str, Vec<&'a CandidateRecord>>,
    review_ix: BTreeMap<&'a str, &'a ReviewRecord>,
    approval_ix: BTreeMap<&'a str, &'a ApprovalRecord>,
    document_ix: BTreeSet<(RecordKind, String, u64)>,
}

impl std::ops::Deref for View<'_> {
    type Target = RecordSet;
    fn deref(&self) -> &RecordSet {
        self.set
    }
}

impl<'a> View<'a> {
    fn new(set: &'a RecordSet) -> Self {
        let mut source_ix = BTreeMap::new();
        for s in &set.sources {
            source_ix
                .entry((s.source_id.as_str(), s.revision.get()))
                .or_insert(s);
        }
        let mut memory_ix = BTreeMap::new();
        for m in &set.memories {
            memory_ix
                .entry((m.memory_id.as_str(), m.revision.get()))
                .or_insert(m);
        }
        let mut candidate_ix: BTreeMap<&str, Vec<&CandidateRecord>> = BTreeMap::new();
        for c in &set.candidates {
            candidate_ix
                .entry(c.candidate_id.as_str())
                .or_default()
                .push(c);
        }
        for revisions in candidate_ix.values_mut() {
            revisions.sort_by_key(|c| c.revision);
        }
        let mut review_ix = BTreeMap::new();
        for r in &set.reviews {
            review_ix.entry(r.review_id.as_str()).or_insert(r);
        }
        let mut approval_ix = BTreeMap::new();
        for a in &set.approvals {
            approval_ix.entry(a.approval_id.as_str()).or_insert(a);
        }
        Self {
            set,
            source_ix,
            memory_ix,
            candidate_ix,
            review_ix,
            approval_ix,
            document_ix: set.cataloged_documents(),
        }
    }

    fn source(&self, reference: &SourceRevisionRef) -> Option<&'a SourceRecord> {
        self.source_ix
            .get(&(
                reference.source_id.as_str(),
                reference.source_revision.get(),
            ))
            .copied()
    }

    fn memory_revision(&self, id: &str, revision: Revision) -> Option<&'a CanonicalMemory> {
        self.memory_ix.get(&(id, revision.get())).copied()
    }

    fn candidate_revisions(&self, id: &str) -> Vec<&'a CandidateRecord> {
        self.candidate_ix.get(id).cloned().unwrap_or_default()
    }

    fn review(&self, id: &str) -> Option<&'a ReviewRecord> {
        self.review_ix.get(id).copied()
    }

    fn approval(&self, id: &str) -> Option<&'a ApprovalRecord> {
        self.approval_ix.get(id).copied()
    }

    fn cataloged_documents(&self) -> &BTreeSet<(RecordKind, String, u64)> {
        &self.document_ix
    }
}

fn contiguous(revisions: &mut [u64]) -> bool {
    revisions.sort_unstable();
    revisions
        .iter()
        .enumerate()
        .all(|(i, r)| *r == i as u64 + 1)
}

fn check_revisions(set: &View, out: &mut Vec<Violation>) {
    let mut groups: BTreeMap<(RecordKind, String), Vec<u64>> = BTreeMap::new();
    for (kind, id, rev) in set.cataloged_documents() {
        if kind.is_revisioned() {
            groups.entry((*kind, id.clone())).or_default().push(*rev);
        }
    }
    // Duplicate (kind,id,rev) collapse in the set above; count documents directly.
    let total = set.sources.len()
        + set.attachments.len()
        + set.projects.len()
        + set.memories.len()
        + set.candidates.len()
        + set.identities.len()
        + set.sessions.len()
        + set.checkpoints.len()
        + set.policies.len()
        + set.imports.len();
    let distinct: usize = groups.values().map(Vec::len).sum();
    if distinct != total {
        out.push(Violation::new("set.duplicate_revision", "/"));
    }
    for ((_, id), revisions) in &mut groups {
        if !contiguous(revisions) {
            out.push(Violation::new("set.revision_sequence", id.clone()));
        }
    }
}

fn check_evidence(set: &View, out: &mut Vec<Violation>) {
    struct Derived<'a> {
        target: RecordRef,
        evidence: &'a [crate::common::EvidenceRef],
        sensitivity: Sensitivity,
        content: &'a str,
        approval: Option<&'a crate::ids::ApprovalId>,
        broken: bool,
    }
    let derived = set
        .memories
        .iter()
        .map(|m| Derived {
            target: RecordRef::new(RecordKind::Memory, m.memory_id.as_str(), m.revision),
            evidence: &m.evidence,
            sensitivity: m.sensitivity,
            content: &m.content,
            approval: m.declassification_approval_id.as_ref(),
            broken: m.provenance_state == ProvenanceState::Broken,
        })
        .chain(set.candidates.iter().map(|c| Derived {
            target: RecordRef::new(RecordKind::Candidate, c.candidate_id.as_str(), c.revision),
            evidence: &c.evidence,
            sensitivity: c.sensitivity,
            content: &c.proposed_content,
            approval: c.declassification_approval_id.as_ref(),
            broken: false,
        }));
    for record in derived {
        let path = format!(
            "{}@{}",
            record.target.record_id,
            record.target.revision.get()
        );
        let mut source_levels = Vec::new();
        let mut cited = BTreeSet::new();
        for item in record.evidence {
            let reference = SourceRevisionRef {
                source_id: item.source_id.clone(),
                source_revision: item.source_revision,
            };
            cited.insert(reference.clone());
            let Some(source) = set.source(&reference) else {
                if !record.broken {
                    out.push(Violation::new("evidence.unresolved", path.clone()));
                }
                continue;
            };
            source_levels.push(source.sensitivity);
            if &item.object_hash != source.anchor_hash() {
                out.push(Violation::new("evidence.object_mismatch", path.clone()));
            }
            if item.evidence_class != source.evidence_class {
                out.push(Violation::new("evidence.class_mismatch", path.clone()));
            }
            if !item.locator.within(&source.locator) {
                out.push(Violation::new("evidence.locator_mismatch", path.clone()));
            }
        }
        if derived_sensitivity_ok(record.sensitivity, source_levels.iter().copied(), false) {
            continue;
        }
        // A downgrade needs an owner declassification approval bound to this
        // exact revision, both levels, its sources, and its content.
        let strictest = source_levels.iter().copied().max();
        let valid = record
            .approval
            .and_then(|id| set.approval(id.as_str()))
            .is_some_and(|approval| match &approval.binding {
                ApprovalBinding::Declassification {
                    target,
                    from_sensitivity,
                    to_sensitivity,
                    source_refs,
                    final_content_hash,
                } => {
                    target == &record.target
                        && Some(*from_sensitivity) == strictest
                        && *to_sensitivity == record.sensitivity
                        && source_refs.iter().cloned().collect::<BTreeSet<_>>() == cited
                        && final_content_hash == &sha256(record.content.as_bytes())
                }
                _ => false,
            });
        if !valid {
            let rule = if record.approval.is_some() {
                "sensitivity.declassification_invalid"
            } else {
                "sensitivity.downgrade"
            };
            out.push(Violation::new(rule, path));
        }
    }
}

/// Import closure (ADR-MEM-38): an imported source names an import that
/// exists and cites that import's received bytes; a completed import's
/// coverage counts exactly the sources that cite it.
fn check_imports(set: &View, out: &mut Vec<Violation>) {
    let latest_import = |id: &str| {
        set.imports
            .iter()
            .filter(|i| i.import_id.as_str() == id)
            .max_by_key(|i| i.revision)
    };
    let mut cited: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();
    for source in &set.sources {
        let Some(import_id) = &source.import_id else {
            continue;
        };
        let path = source.source_id.to_string();
        match latest_import(import_id.as_str()) {
            None => out.push(Violation::new("source.import_unresolved", path)),
            Some(import) => {
                if source.raw_object_hash.as_ref() != Some(&import.input_object_hash) {
                    out.push(Violation::new("source.import_raw_mismatch", path));
                }
                cited
                    .entry(import_id.as_str())
                    .or_default()
                    .insert(source.source_id.as_str());
            }
        }
    }
    let ids: BTreeSet<&str> = set.imports.iter().map(|i| i.import_id.as_str()).collect();
    for id in ids {
        let import = latest_import(id).expect("present");
        // Every revision describes the same received bytes. Sources compare
        // against the latest revision only, so this keeps a later revision
        // from silently changing what earlier sources cite (ADR-MEM-39).
        if set.imports.iter().any(|i| {
            i.import_id.as_str() == id
                && (i.input_object_hash != import.input_object_hash
                    || i.input_size_bytes != import.input_size_bytes)
        }) {
            out.push(Violation::new("import.input_changed", id.to_owned()));
        }
        if import.status != ImportStatus::Completed {
            continue;
        }
        let covered: u64 = import.coverage.iter().map(|c| c.message_count).sum();
        let sources = cited.get(id).map_or(0, |s| s.len() as u64);
        if covered != sources {
            out.push(Violation::new("import.coverage_mismatch", id.to_owned()));
        }
        if let Some(original) = &import.duplicate_of
            && latest_import(original.as_str())
                .is_none_or(|o| o.input_object_hash != import.input_object_hash)
        {
            out.push(Violation::new("import.duplicate", id.to_owned()));
        }
    }
}

fn check_reviews(set: &View, out: &mut Vec<Violation>) {
    let mut nonces = BTreeSet::new();
    for review in &set.reviews {
        let path = review.review_id.to_string();
        if !nonces.insert(&review.approval_nonce) {
            out.push(Violation::new("review.nonce_reuse", path.clone()));
        }
        match set.commit(review.commit_id.as_str()) {
            Some(commit) if commit.review_ids.contains(&review.review_id) => {}
            _ => out.push(Violation::new("review.commit_ref", path.clone())),
        }
        for reference in &review.evidence_refs {
            if set.source(reference).is_none() {
                out.push(Violation::new("evidence.unresolved", path.clone()));
            }
        }
        let revisions = set.candidate_revisions(review.candidate_id.as_str());
        let reviewed = revisions
            .iter()
            .find(|c| c.revision == review.candidate_revision);
        let next = revisions
            .iter()
            .find(|c| c.revision.get() == review.candidate_revision.get() + 1);
        match (reviewed, next) {
            (Some(reviewed), Some(next)) if reviewed.status == CandidateStatus::Pending => {
                let expected = match review.action {
                    ReviewAction::Reject => CandidateStatus::Rejected,
                    ReviewAction::Merge => CandidateStatus::Merged,
                    _ => CandidateStatus::Accepted,
                };
                if next.status != expected
                    || next.resolution_review_id.as_ref() != Some(&review.review_id)
                {
                    out.push(Violation::new("review.candidate_resolution", path.clone()));
                }
                if next.revision != revisions.last().expect("non-empty").revision {
                    out.push(Violation::new("candidate.status_transition", path.clone()));
                }
            }
            _ => out.push(Violation::new(
                "review.stale_candidate_revision",
                path.clone(),
            )),
        }
        for reference in &review.resulting_records {
            if !set.cataloged_documents().contains(&(
                reference.record_kind,
                reference.record_id.clone(),
                reference.revision.get(),
            )) {
                out.push(Violation::new("review.result_missing", path.clone()));
            }
        }
        for reference in &review.target_expected_revisions {
            if !set.cataloged_documents().contains(&(
                reference.record_kind,
                reference.record_id.clone(),
                reference.revision.get(),
            )) {
                out.push(Violation::new("review.target_missing", path.clone()));
            }
        }
    }
    for memory in &set.memories {
        let path = format!("{}@{}", memory.memory_id, memory.revision.get());
        let Some(review) = set.review(memory.review_id.as_str()) else {
            out.push(Violation::new("memory.review_missing", path));
            continue;
        };
        let me = RecordRef::new(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            memory.revision,
        );
        if !review.resulting_records.contains(&me) || review.action == ReviewAction::Reject {
            out.push(Violation::new("memory.review_result", path.clone()));
        }
        if review.actor != memory.approved_by || review.created_at != memory.approved_at {
            out.push(Violation::new("memory.approval_mismatch", path.clone()));
        }
        let accepted = set
            .candidate_revisions(review.candidate_id.as_str())
            .last()
            .is_some_and(|c| {
                matches!(
                    c.status,
                    CandidateStatus::Accepted | CandidateStatus::Merged
                )
            });
        if !accepted {
            out.push(Violation::new("memory.unreviewed_candidate", path));
        }
    }
    for project in &set.projects {
        if set.review(project.review_id.as_str()).is_none() {
            out.push(Violation::new(
                "project.review_missing",
                project.project_id.to_string(),
            ));
        }
    }
    for identity in &set.identities {
        let ok = set.review(identity.review_id.as_str()).is_some_and(|r| {
            r.action == ReviewAction::IdentityAccept && r.actor == identity.approved_by
        });
        if !ok {
            out.push(Violation::new(
                "identity.review_missing",
                identity.identity_id.to_string(),
            ));
        }
    }
}

fn check_candidates(set: &View, out: &mut Vec<Violation>) {
    let ids: BTreeSet<&str> = set
        .candidates
        .iter()
        .map(|c| c.candidate_id.as_str())
        .collect();
    for id in ids {
        let revisions = set.candidate_revisions(id);
        for (index, candidate) in revisions.iter().enumerate() {
            let last = index + 1 == revisions.len();
            if candidate.status.is_terminal() && !last {
                out.push(Violation::new("candidate.status_transition", id.to_owned()));
            }
            if let Some(review) = &candidate.resolution_review_id {
                let ok = set
                    .review(review.as_str())
                    .is_some_and(|r| r.candidate_id == candidate.candidate_id);
                if !ok {
                    out.push(Violation::new("candidate.resolution_review", id.to_owned()));
                }
            }
            match &candidate.merged_into {
                Some(MergeTarget::Candidate { candidate_id })
                    if set.candidate_revisions(candidate_id.as_str()).is_empty() =>
                {
                    out.push(Violation::new("candidate.merge_target", id.to_owned()));
                }
                Some(MergeTarget::Memory { memory_id })
                    if !set.memories.iter().any(|m| &m.memory_id == memory_id) =>
                {
                    out.push(Violation::new("candidate.merge_target", id.to_owned()));
                }
                _ => {}
            }
            if let Some(reopened) = &candidate.reopens_candidate_id {
                let terminal = set
                    .candidate_revisions(reopened.as_str())
                    .last()
                    .is_some_and(|c| c.status.is_terminal());
                if !terminal {
                    out.push(Violation::new("candidate.reopen_target", id.to_owned()));
                }
            }
        }
    }
}

fn same_scope(a: &CanonicalMemory, b: &CanonicalMemory) -> bool {
    let subjects = || {
        a.subject_ids.is_empty() && b.subject_ids.is_empty()
            || a.subject_ids.iter().any(|s| b.subject_ids.contains(s))
    };
    match (&a.body, &b.body) {
        (MemoryBody::Fact(x), MemoryBody::Fact(y)) => x.claim_key == y.claim_key && subjects(),
        (MemoryBody::Preference(_), MemoryBody::Preference(_))
        | (MemoryBody::Episode(_), MemoryBody::Episode(_)) => subjects(),
        (MemoryBody::ProjectState(_), MemoryBody::ProjectState(_)) => a.project_id == b.project_id,
        (MemoryBody::SessionCheckpoint(x), MemoryBody::SessionCheckpoint(y)) => {
            x.session_id == y.session_id
        }
        _ => false,
    }
}

fn check_supersession(set: &View, out: &mut Vec<Violation>) {
    let latest = set.latest_memories();
    let by_id: BTreeMap<&str, &CanonicalMemory> =
        latest.iter().map(|m| (m.memory_id.as_str(), *m)).collect();
    let mut targeted = BTreeSet::new();
    for memory in &latest {
        for edge in &memory.supersedes {
            let path = format!("{}->{}", memory.memory_id, edge.memory_id);
            if set
                .memory_revision(edge.memory_id.as_str(), edge.revision)
                .is_none()
            {
                out.push(Violation::new("supersession.target_missing", path));
                continue;
            }
            targeted.insert(edge.memory_id.as_str());
            let target = by_id[edge.memory_id.as_str()];
            if !same_scope(memory, target) {
                out.push(Violation::new("supersession.cross_scope", path.clone()));
            }
            if memory.status != MemoryStatus::Archived
                && !matches!(
                    target.status,
                    MemoryStatus::Superseded | MemoryStatus::Archived
                )
            {
                out.push(Violation::new("supersession.status", path));
            }
        }
    }
    // The effective time on each edge is exactly what the owner confirmed in
    // the supersede review; it cannot be filled in or changed afterwards.
    for memory in latest.iter().filter(|m| !m.supersedes.is_empty()) {
        let confirmed = set
            .review(memory.review_id.as_str())
            .filter(|r| r.action == ReviewAction::Supersede)
            .and_then(|r| r.effective_from.as_ref());
        if memory
            .supersedes
            .iter()
            .any(|edge| confirmed != Some(&edge.effective_from))
        {
            out.push(Violation::new(
                "supersession.effective_mismatch",
                memory.memory_id.to_string(),
            ));
        }
    }
    for memory in &latest {
        if memory.status == MemoryStatus::Superseded
            && !targeted.contains(memory.memory_id.as_str())
        {
            out.push(Violation::new(
                "supersession.orphan_status",
                memory.memory_id.to_string(),
            ));
        }
    }
    // Cycle detection over latest revisions (edge: replacement -> target).
    fn visit<'a>(
        id: &'a str,
        by_id: &BTreeMap<&'a str, &'a CanonicalMemory>,
        state: &mut BTreeMap<&'a str, u8>,
    ) -> bool {
        match state.get(id) {
            Some(1) => return true,
            Some(2) => return false,
            _ => {}
        }
        state.insert(id, 1);
        if let Some(memory) = by_id.get(id) {
            for edge in &memory.supersedes {
                if visit(edge.memory_id.as_str(), by_id, state) {
                    return true;
                }
            }
        }
        state.insert(id, 2);
        false
    }
    let mut state = BTreeMap::new();
    for id in by_id.keys() {
        if visit(id, &by_id, &mut state) {
            out.push(Violation::new("supersession.cycle", (*id).to_owned()));
            break;
        }
    }
    let mut groups: BTreeMap<&str, Vec<&CanonicalMemory>> = BTreeMap::new();
    for memory in &latest {
        if let Some(group) = &memory.conflict_group_id {
            groups.entry(group.as_str()).or_default().push(memory);
        }
    }
    for (group, members) in groups {
        if members.len() < 2 {
            out.push(Violation::new("conflict.singleton", group.to_owned()));
        }
        if members
            .windows(2)
            .any(|w| w[0].memory_type() != w[1].memory_type())
        {
            out.push(Violation::new("conflict.type_mismatch", group.to_owned()));
        }
    }
}

fn check_projects(set: &View, out: &mut Vec<Violation>) {
    let mut latest: BTreeMap<&str, &ProjectEntity> = BTreeMap::new();
    for project in &set.projects {
        let entry = latest.entry(project.project_id.as_str()).or_insert(project);
        if project.revision > entry.revision {
            *entry = project;
        }
    }
    let mut owners: BTreeMap<String, &str> = BTreeMap::new();
    for (id, project) in &latest {
        for name in project.names() {
            if let Some(other) = owners.insert(name, id)
                && other != *id
            {
                out.push(Violation::new("project.alias_collision", (*id).to_owned()));
            }
        }
    }
    for memory in &set.memories {
        if let Some(project) = &memory.project_id
            && !latest.contains_key(project.as_str())
        {
            out.push(Violation::new(
                "memory.project_missing",
                memory.memory_id.to_string(),
            ));
        }
    }
}

fn check_sessions(set: &View, out: &mut Vec<Violation>) {
    let latest_session = |id: &str| {
        set.sessions
            .iter()
            .filter(|s| s.session_id.as_str() == id)
            .max_by_key(|s| s.revision)
    };
    let mut sequences: BTreeMap<&str, BTreeMap<u64, &SessionEvent>> = BTreeMap::new();
    for event in &set.session_events {
        let path = event.event_id.to_string();
        match latest_session(event.session_id.as_str()) {
            Some(session) => {
                if !session
                    .branches
                    .iter()
                    .any(|b| b.branch_id == event.branch_id)
                {
                    out.push(Violation::new("event.branch_missing", path.clone()));
                }
            }
            None => out.push(Violation::new("event.session_missing", path.clone())),
        }
        if sequences
            .entry(event.session_id.as_str())
            .or_default()
            .insert(event.sequence, event)
            .is_some()
        {
            out.push(Violation::new("event.sequence_duplicate", path));
        }
    }
    let mut by_id: BTreeMap<&str, &SessionEvent> = BTreeMap::new();
    for event in &set.session_events {
        by_id.entry(event.event_id.as_str()).or_insert(event);
    }
    let find_event = |id: &str| by_id.get(id).copied();
    // Earliest user message per (session, turn).
    let mut inputs: BTreeMap<(&str, &crate::ids::TurnId), u64> = BTreeMap::new();
    for e in set
        .session_events
        .iter()
        .filter(|e| e.kind == EventKind::UserMessage)
    {
        if let Some(turn) = &e.turn_id {
            let entry = inputs
                .entry((e.session_id.as_str(), turn))
                .or_insert(e.sequence);
            *entry = (*entry).min(e.sequence);
        }
    }
    for event in &set.session_events {
        let path = event.event_id.to_string();
        if let Some(parent) = &event.parent_event_id {
            match find_event(parent.as_str()) {
                Some(p) if p.sequence < event.sequence && p.session_id == event.session_id => {}
                _ => out.push(Violation::new("event.parent_order", path.clone())),
            }
        }
        if event.kind == EventKind::AssistantCompleted {
            let has_input = match &event.turn_id {
                Some(turn) => inputs
                    .get(&(event.session_id.as_str(), turn))
                    .is_some_and(|first| *first < event.sequence),
                // Two absent turn IDs compare equal.
                None => set.session_events.iter().any(|e| {
                    e.kind == EventKind::UserMessage
                        && e.turn_id.is_none()
                        && e.session_id == event.session_id
                        && e.sequence < event.sequence
                }),
            };
            if !has_input {
                out.push(Violation::new("event.completed_without_input", path));
            }
        }
    }
    for session in &set.sessions {
        let is_latest = latest_session(session.session_id.as_str())
            .is_some_and(|l| l.revision == session.revision);
        let max = sequences
            .get(session.session_id.as_str())
            .and_then(|s| s.keys().next_back().copied())
            .unwrap_or(0);
        if is_latest && session.last_event_seq != max {
            out.push(Violation::new(
                "session.last_event_seq",
                session.session_id.to_string(),
            ));
        }
    }
    for checkpoint in &set.checkpoints {
        let path = checkpoint.checkpoint_id.to_string();
        let events = sequences.get(checkpoint.session_id.as_str());
        let covered: Vec<&SessionEvent> = match &checkpoint.coverage {
            Coverage::Range {
                from_sequence,
                to_sequence,
            } => {
                // Iterate existing events, never the declared range: a hostile
                // range (e.g. 1..=2^53) must not become an unbounded loop.
                let found: Vec<_> = events
                    .map(|e| {
                        e.range(*from_sequence..=*to_sequence)
                            .map(|(_, v)| *v)
                            .collect()
                    })
                    .unwrap_or_default();
                let expected = to_sequence
                    .checked_sub(*from_sequence)
                    .and_then(|d| d.checked_add(1));
                if expected != Some(found.len() as u64) {
                    out.push(Violation::new("checkpoint.coverage_missing", path.clone()));
                }
                found
            }
            Coverage::EventIds { event_ids } => {
                let found: Vec<_> = event_ids
                    .iter()
                    .filter_map(|id| find_event(id.as_str()))
                    .filter(|e| e.session_id == checkpoint.session_id)
                    .collect();
                if found.len() != event_ids.len() {
                    out.push(Violation::new("checkpoint.coverage_missing", path.clone()));
                }
                found
            }
        };
        if covered.iter().any(|e| e.branch_id != checkpoint.branch_id) {
            out.push(Violation::new("checkpoint.branch_mismatch", path.clone()));
        }
        if let Some(turn) = &checkpoint.last_completed_turn_id {
            let completed = covered.iter().any(|e| {
                e.kind == EventKind::AssistantCompleted && e.turn_id.as_ref() == Some(turn)
            });
            if !completed {
                out.push(Violation::new(
                    "checkpoint.turn_not_completed",
                    path.clone(),
                ));
            }
        }
        for item in checkpoint.decisions.iter().chain(&checkpoint.open_loops) {
            for reference in &item.source_refs {
                let ok = match reference {
                    CheckpointSourceRef::Event { event_id } => {
                        covered.iter().any(|e| &e.event_id == event_id)
                    }
                    CheckpointSourceRef::Source {
                        source_id,
                        source_revision,
                    } => set
                        .source(&SourceRevisionRef {
                            source_id: source_id.clone(),
                            source_revision: *source_revision,
                        })
                        .is_some(),
                };
                if !ok {
                    out.push(Violation::new(
                        "checkpoint.item_outside_coverage",
                        path.clone(),
                    ));
                }
            }
        }
        if set
            .commit(checkpoint.base_vault_commit_id.as_str())
            .is_none()
        {
            out.push(Violation::new("checkpoint.base_commit", path.clone()));
        }
        if let Some(review) = &checkpoint.review_id
            && set.review(review.as_str()).is_none()
        {
            out.push(Violation::new("checkpoint.review_missing", path));
        }
    }
    for memory in &set.memories {
        if let MemoryBody::SessionCheckpoint(fields) = &memory.body {
            let reviewed = set.checkpoints.iter().any(|c| {
                c.checkpoint_id == fields.checkpoint_id
                    && c.revision == fields.checkpoint_revision
                    && c.session_id == fields.session_id
                    && c.status == crate::session::CheckpointStatus::Reviewed
            });
            if !reviewed {
                out.push(Violation::new(
                    "memory.checkpoint_unreviewed",
                    memory.memory_id.to_string(),
                ));
            }
        }
    }
}

fn check_commits(set: &View, out: &mut Vec<Violation>) {
    let mut commits: Vec<&CommitManifest> = set.commits.iter().collect();
    commits.sort_by_key(|c| c.sequence);
    for (index, commit) in commits.iter().enumerate() {
        let path = commit.commit_id.to_string();
        if commit.sequence != index as u64 + 1 {
            out.push(Violation::new("commit.chain", path.clone()));
        }
        if let Some(previous) = index.checked_sub(1).map(|i| commits[i]) {
            if commit.parent_commit_id.as_ref() != Some(&previous.commit_id)
                || commit.vault_id != previous.vault_id
            {
                out.push(Violation::new("commit.chain", path.clone()));
            }
            if commit.policy_epoch < previous.policy_epoch
                || commit.deletion_epoch < previous.deletion_epoch
            {
                out.push(Violation::new("commit.epoch_regression", path.clone()));
            }
            if commit.created_at < previous.created_at {
                out.push(Violation::new("commit.clock_regression", path.clone()));
            }
        }
        commit_refs(set, commit, out);
    }
    let mut scopes: BTreeMap<(String, String, String), (&str, String)> = BTreeMap::new();
    for commit in &commits {
        let Some(key) = &commit.idempotency_key_hash else {
            continue;
        };
        let scope = (
            commit.principal.actor_id.to_string(),
            format!("{:?}", commit.operation_kind),
            key.to_string(),
        );
        if let Some((_, payload)) = scopes.get(&scope) {
            let rule = if *payload == commit.request_payload_hash.to_string() {
                "commit.duplicate_application"
            } else {
                "commit.idempotency_conflict"
            };
            out.push(Violation::new(rule, commit.commit_id.to_string()));
        } else {
            scopes.insert(
                scope,
                (
                    commit.commit_id.as_str(),
                    commit.request_payload_hash.to_string(),
                ),
            );
        }
    }
    let documents = set.cataloged_documents();
    for commit in &commits {
        for entry in &commit.catalog {
            if !documents.contains(&(
                entry.record_kind,
                entry.record_id.clone(),
                entry.revision.get(),
            )) {
                out.push(Violation::new(
                    "commit.catalog_unresolved",
                    commit.commit_id.to_string(),
                ));
            }
        }
    }
    if let Some(head) = commits.last() {
        let mut latest: BTreeMap<(RecordKind, String), u64> = BTreeMap::new();
        for (kind, id, rev) in documents {
            let entry = latest.entry((*kind, id.clone())).or_insert(*rev);
            *entry = (*entry).max(*rev);
        }
        let expected: BTreeSet<(RecordKind, String, u64)> =
            latest.into_iter().map(|((k, i), r)| (k, i, r)).collect();
        let actual: BTreeSet<(RecordKind, String, u64)> = head
            .catalog
            .iter()
            .map(|e| (e.record_kind, e.record_id.clone(), e.revision.get()))
            .collect();
        if expected != actual {
            out.push(Violation::new(
                "commit.catalog_mismatch",
                head.commit_id.to_string(),
            ));
        }
    }
}

fn commit_refs(set: &View, commit: &CommitManifest, out: &mut Vec<Violation>) {
    let path = commit.commit_id.to_string();
    for review in &commit.review_ids {
        if set
            .review(review.as_str())
            .is_none_or(|r| r.commit_id != commit.commit_id)
        {
            out.push(Violation::new("commit.review_ref", path.clone()));
        }
    }
    for delete in &commit.tombstone_ids {
        let ok = set
            .tombstones
            .iter()
            .any(|t| &t.delete_id == delete && t.deletion_epoch <= commit.deletion_epoch);
        if !ok {
            out.push(Violation::new("commit.tombstone_ref", path.clone()));
        }
    }
}

/// The chain rules of `check_commits` for one new commit on top of
/// `previous` (None for genesis), with its review and tombstone references
/// resolved in `set`. Catalog closure is the store's construction; the
/// idempotency scope is unique through the store's index.
pub fn check_commit_step(
    set: &RecordSet,
    previous: Option<&CommitManifest>,
    commit: &CommitManifest,
) -> Vec<Violation> {
    let mut out = Vec::new();
    let path = commit.commit_id.to_string();
    match previous {
        None if commit.sequence != 1 || commit.parent_commit_id.is_some() => {
            out.push(Violation::new("commit.chain", path.clone()));
        }
        None => {}
        Some(previous) => {
            if commit.sequence != previous.sequence + 1
                || commit.parent_commit_id.as_ref() != Some(&previous.commit_id)
                || commit.vault_id != previous.vault_id
            {
                out.push(Violation::new("commit.chain", path.clone()));
            }
            if commit.policy_epoch < previous.policy_epoch
                || commit.deletion_epoch < previous.deletion_epoch
            {
                out.push(Violation::new("commit.epoch_regression", path.clone()));
            }
            if commit.created_at < previous.created_at {
                out.push(Violation::new("commit.clock_regression", path));
            }
        }
    }
    commit_refs(&View::new(set), commit, &mut out);
    out
}

fn check_deletions(set: &View, out: &mut Vec<Violation>) {
    let documents = set.cataloged_documents();
    let mut epochs = BTreeSet::new();
    for tombstone in &set.tombstones {
        let path = tombstone.delete_id.to_string();
        if !epochs.insert(tombstone.deletion_epoch) {
            out.push(Violation::new("tombstone.epoch_duplicate", path.clone()));
        }
        for target in &tombstone.targets {
            let exists = documents.iter().any(|(k, i, r)| {
                *k == target.record_kind
                    && *i == target.record_id
                    && target.revision.is_none_or(|rev| rev.get() == *r)
            });
            if !exists {
                out.push(Violation::new("tombstone.target_missing", path.clone()));
            }
        }
        // The tombstone must be the result of an owner confirm_delete review
        // whose binding names exactly this mode, scope, and target set, and
        // whose candidate is a delete proposal for one of these targets.
        let Some(review) = set.review(tombstone.review_id.as_str()) else {
            out.push(Violation::new("tombstone.review_missing", path));
            continue;
        };
        if review.action != ReviewAction::ConfirmDelete {
            out.push(Violation::new("tombstone.review_action", path.clone()));
            continue;
        }
        let bound = review.delete_binding.as_ref().is_some_and(|binding| {
            binding.mode == tombstone.mode
                && binding.scope == tombstone.scope
                && binding.targets == tombstone.targets
        });
        if !bound {
            out.push(Violation::new("tombstone.review_binding", path.clone()));
        }
        let me = RecordRef::new(
            RecordKind::Tombstone,
            tombstone.delete_id.as_str(),
            Revision::new(1).expect("1 is a revision"),
        );
        if !review.resulting_records.contains(&me) {
            out.push(Violation::new("tombstone.review_result", path.clone()));
        }
        let proposal_ok = set
            .candidate_revisions(review.candidate_id.as_str())
            .into_iter()
            .find(|c| c.revision == review.candidate_revision)
            .is_some_and(|c| {
                c.proposal_kind == crate::candidate::ProposalKind::Delete
                    && c.target_memory_id.as_ref().is_some_and(|m| {
                        tombstone.targets.iter().any(|t| {
                            t.record_kind == RecordKind::Memory && t.record_id == m.as_str()
                        })
                    })
            });
        if !proposal_ok {
            out.push(Violation::new("tombstone.candidate_mismatch", path));
        }
    }
    for receipt in &set.purge_receipts {
        let ok = set.tombstones.iter().any(|t| {
            t.delete_id == receipt.delete_id && t.mode == crate::commit::DeleteMode::Purge
        });
        if !ok {
            out.push(Violation::new(
                "purge.tombstone",
                receipt.receipt_id.to_string(),
            ));
        }
    }
}

/// Policy references resolve; every owner grant is bound to an approval of
/// exactly that policy revision (digest of its canonical bytes).
fn check_policies(set: &View, out: &mut Vec<Violation>) {
    let known: BTreeSet<&str> = set.policies.iter().map(|p| p.policy_id.as_str()).collect();
    let mut references: Vec<(String, &crate::ids::PolicyId)> = Vec::new();
    for m in &set.memories {
        references.push((m.memory_id.to_string(), &m.access_policy_id));
        if let Some(egress) = &m.egress_policy_id {
            references.push((m.memory_id.to_string(), egress));
        }
    }
    for s in &set.sources {
        references.push((s.source_id.to_string(), &s.access_policy_id));
    }
    for i in &set.identities {
        references.push((i.identity_id.to_string(), &i.access_policy_id));
        if let Some(egress) = &i.egress_policy_id {
            references.push((i.identity_id.to_string(), egress));
        }
    }
    for s in &set.sessions {
        references.push((s.session_id.to_string(), &s.policy_id));
    }
    for d in &set.dispatches {
        if let Some(egress) = &d.egress.egress_policy_id {
            references.push((d.dispatch_id.to_string(), egress));
        }
    }
    for (path, policy) in references {
        if !known.contains(policy.as_str()) {
            out.push(Violation::new("policy.unresolved", path));
        }
    }
    for policy in &set.policies {
        if policy.origin != PolicyOrigin::OwnerGrant {
            continue;
        }
        let digest = crate::json::canonical_bytes(policy)
            .map(|b| sha256(&b))
            .ok();
        let valid = policy
            .approval_id
            .as_ref()
            .and_then(|id| set.approval(id.as_str()))
            .is_some_and(|approval| match &approval.binding {
                ApprovalBinding::PolicyGrant {
                    policy_id,
                    policy_revision,
                    grant_hash,
                } => {
                    policy_id == &policy.policy_id
                        && *policy_revision == policy.revision
                        && Some(grant_hash) == digest.as_ref()
                }
                _ => false,
            });
        if !valid {
            out.push(Violation::new(
                "policy.grant_invalid",
                format!("{}@{}", policy.policy_id, policy.revision.get()),
            ));
        }
    }
}

fn allowed(
    policies: &[&PolicyRecord],
    principal: &crate::common::ActorRef,
    scope: Scope,
    purpose: crate::context::Purpose,
    destination: Option<&crate::context::Destination>,
    resource: &ResourceContext,
    at: &Timestamp,
) -> bool {
    let context = AccessContext {
        principal,
        scope,
        purpose: Some(purpose),
        destination,
        resource,
    };
    evaluate(policies, &context, at).is_allow()
}

/// Policy check for one resource placed into a capsule: the frozen egress
/// table, the requesting principal's `context:read` access, and, for an
/// external destination, `provider:send` through the record's egress policy.
fn resource_policy_ok(
    capsule: &ContextCapsule,
    policies: &[&PolicyRecord],
    resource: &ResourceContext,
    egress_policy: Option<&crate::ids::PolicyId>,
) -> bool {
    let destination = &capsule.destination;
    if egress_rule(resource.sensitivity, destination.kind) == EgressRule::Denied {
        return false;
    }
    let at = &capsule.generated_at;
    if !allowed(
        policies,
        &capsule.requested_by,
        Scope::ContextRead,
        capsule.purpose,
        None,
        resource,
        at,
    ) {
        return false;
    }
    if destination.kind.is_local() {
        return true;
    }
    let Some(egress_policy) = egress_policy else {
        return false;
    };
    let bound: Vec<&PolicyRecord> = policies
        .iter()
        .copied()
        .filter(|p| &p.policy_id == egress_policy)
        .collect();
    allowed(
        &bound,
        &capsule.requested_by,
        Scope::ProviderSend,
        capsule.purpose,
        Some(destination),
        resource,
        at,
    )
}

/// Why a memory revision may not appear in a capsule, independent of relevance.
pub fn hard_exclusion(
    set: &RecordSet,
    memory: &CanonicalMemory,
    visible: &[&CanonicalMemory],
    as_of: &Timestamp,
    capsule: &ContextCapsule,
    policies: &[&PolicyRecord],
) -> Option<DecisionReason> {
    if set.tombstoned(
        RecordKind::Memory,
        memory.memory_id.as_str(),
        memory.revision,
        capsule.deletion_epoch,
    ) {
        return Some(DecisionReason::Tombstoned);
    }
    if memory.provenance_state == ProvenanceState::Broken {
        return Some(DecisionReason::BrokenProvenance);
    }
    let resource = ResourceContext {
        record: RecordRef::new(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            memory.revision,
        ),
        project_id: memory.project_id.clone(),
        sensitivity: memory.sensitivity,
    };
    if !resource_policy_ok(
        capsule,
        policies,
        &resource,
        memory.egress_policy_id.as_ref(),
    ) {
        return Some(DecisionReason::PolicyDenied);
    }
    match effect(memory, visible, as_of) {
        Effect::Archived => Some(DecisionReason::Archived),
        Effect::NotYetEffective => Some(DecisionReason::NotYetEffective),
        _ => None,
    }
}

fn check_context(set: &View, out: &mut Vec<Violation>) {
    for capsule in &set.capsules {
        let path = capsule.capsule_id.to_string();
        let Some(commit) = set.commit(capsule.vault_commit_id.as_str()) else {
            out.push(Violation::new("capsule.commit_missing", path));
            continue;
        };
        if capsule.deletion_epoch < commit.deletion_epoch
            || capsule.policy_epoch < commit.policy_epoch
        {
            out.push(Violation::new("capsule.epoch", path.clone()));
        }
        let visible = set.memories_at(commit);
        let policies = set.policies_at(commit);
        let now = &capsule.generated_at;
        let as_of = capsule.as_of.as_ref().unwrap_or(now);
        let mut included = BTreeSet::new();
        for item in capsule.memory_items() {
            included.insert(MemoryRevisionRef {
                memory_id: item.memory_id.clone(),
                revision: item.revision,
            });
            let Some(memory) = visible
                .iter()
                .find(|m| m.memory_id == item.memory_id && m.revision == item.revision)
            else {
                out.push(Violation::new(
                    "capsule.stale_revision",
                    item.memory_id.to_string(),
                ));
                continue;
            };
            if let Some(reason) = hard_exclusion(set, memory, &visible, as_of, capsule, &policies) {
                let rule = match reason {
                    DecisionReason::Tombstoned => "capsule.tombstoned_included",
                    DecisionReason::BrokenProvenance => "capsule.broken_provenance_included",
                    DecisionReason::PolicyDenied => "capsule.policy_violation",
                    _ => "capsule.not_effective_included",
                };
                out.push(Violation::new(rule, item.memory_id.to_string()));
            }
            if currency(memory, &visible, as_of, now) != Some(item.currency) {
                out.push(Violation::new(
                    "capsule.currency_mismatch",
                    item.memory_id.to_string(),
                ));
            }
            let evidence: BTreeSet<SourceRevisionRef> = memory
                .evidence
                .iter()
                .map(|e| SourceRevisionRef {
                    source_id: e.source_id.clone(),
                    source_revision: e.source_revision,
                })
                .collect();
            let cited: BTreeSet<SourceRevisionRef> = item.evidence.iter().cloned().collect();
            if item.content != memory.content
                || item.sensitivity != memory.sensitivity
                || item.memory_type != memory.memory_type()
                || cited != evidence
                || item.conflict_group_id != memory.conflict_group_id
            {
                out.push(Violation::new(
                    "capsule.item_mismatch",
                    item.memory_id.to_string(),
                ));
            }
            for reference in &item.evidence {
                let listed = capsule.provenance.iter().any(|p| {
                    p.source_id == reference.source_id
                        && p.source_revision == reference.source_revision
                });
                if !listed {
                    out.push(Violation::new(
                        "capsule.provenance_missing",
                        item.memory_id.to_string(),
                    ));
                }
            }
        }
        for item in capsule
            .memory_items()
            .filter(|i| i.currency == Currency::Conflicted)
        {
            let group = item.conflict_group_id.as_ref();
            let all_members = visible
                .iter()
                .filter(|m| m.conflict_group_id.as_ref() == group)
                .filter(|m| matches!(effect(m, &visible, as_of), Effect::InEffect { .. }))
                .all(|m| capsule.memory_items().any(|i| i.memory_id == m.memory_id));
            let flagged = capsule
                .verification_needed
                .iter()
                .any(|v| v.conflict_group_id.as_ref() == group);
            if !all_members && !flagged {
                out.push(Violation::new(
                    "capsule.conflict_one_sided",
                    item.memory_id.to_string(),
                ));
            }
        }
        for item in &capsule.recent_session_checkpoints {
            let artifact = set
                .checkpoints
                .iter()
                .find(|c| c.checkpoint_id == item.checkpoint_id && c.revision == item.revision);
            let expected = artifact.map(|c| {
                if c.status == crate::session::CheckpointStatus::Reviewed {
                    CheckpointItemStatus::Reviewed
                } else {
                    CheckpointItemStatus::Provisional
                }
            });
            if expected != Some(item.status) {
                out.push(Violation::new(
                    "capsule.checkpoint_status",
                    item.checkpoint_id.to_string(),
                ));
            }
        }
        for open_loop in &capsule.open_loops {
            if open_loop.origin == LoopOrigin::Checkpoint {
                let provisional = set
                    .checkpoints
                    .iter()
                    .filter(|c| c.checkpoint_id.as_str() == open_loop.origin_id)
                    .all(|c| c.status != crate::session::CheckpointStatus::Reviewed);
                if provisional && !open_loop.provisional {
                    out.push(Violation::new(
                        "capsule.open_loop_provisional",
                        open_loop.item_id.to_string(),
                    ));
                }
            }
        }
        for item in &capsule.identity {
            let identity = set
                .identities
                .iter()
                .find(|i| i.identity_id == item.identity_id && i.revision == item.revision);
            match identity {
                Some(identity) if identity.slug == item.slug => {
                    let resource = ResourceContext {
                        record: RecordRef::new(
                            RecordKind::Identity,
                            identity.identity_id.as_str(),
                            identity.revision,
                        ),
                        project_id: None,
                        sensitivity: identity.sensitivity,
                    };
                    if !resource_policy_ok(
                        capsule,
                        &policies,
                        &resource,
                        identity.egress_policy_id.as_ref(),
                    ) {
                        out.push(Violation::new(
                            "capsule.policy_violation",
                            item.identity_id.to_string(),
                        ));
                    }
                }
                _ => out.push(Violation::new(
                    "capsule.identity_missing",
                    item.identity_id.to_string(),
                )),
            }
        }
        check_inspection(set, capsule, &visible, &policies, &included, out);
        check_dispatch(set, capsule, out);
    }
    for inspection in &set.inspections {
        if !set
            .capsules
            .iter()
            .any(|c| c.capsule_id == inspection.capsule_id)
        {
            out.push(Violation::new(
                "inspection.capsule_missing",
                inspection.inspection_id.to_string(),
            ));
        }
    }
    let mut approval_uses: BTreeMap<&str, usize> = BTreeMap::new();
    for dispatch in &set.dispatches {
        if !set
            .capsules
            .iter()
            .any(|c| c.capsule_id == dispatch.capsule_id)
        {
            out.push(Violation::new(
                "dispatch.capsule_missing",
                dispatch.dispatch_id.to_string(),
            ));
        }
        if let Some(approval) = &dispatch.egress.egress_approval_id {
            *approval_uses.entry(approval.as_str()).or_default() += 1;
        }
    }
    // Egress approvals are single-use: one approval, one dispatched request.
    for (approval, uses) in approval_uses {
        if uses > 1 {
            out.push(Violation::new(
                "egress.approval_replayed",
                approval.to_owned(),
            ));
        }
    }
}

fn check_inspection(
    set: &RecordSet,
    capsule: &ContextCapsule,
    visible: &[&CanonicalMemory],
    policies: &[&PolicyRecord],
    included: &BTreeSet<MemoryRevisionRef>,
    out: &mut Vec<Violation>,
) {
    let as_of = capsule.as_of.as_ref().unwrap_or(&capsule.generated_at);
    for inspection in set
        .inspections
        .iter()
        .filter(|i| i.capsule_id == capsule.capsule_id)
    {
        let path = inspection.inspection_id.to_string();
        if inspection.request_id != capsule.request_id
            || inspection.vault_commit_id != capsule.vault_commit_id
            || inspection.policy_epoch != capsule.policy_epoch
            || inspection.deletion_epoch != capsule.deletion_epoch
            || inspection.ranking_version != capsule.ranking_version
        {
            out.push(Violation::new("inspection.header_mismatch", path.clone()));
        }
        let listed: BTreeSet<MemoryRevisionRef> = inspection
            .decisions
            .iter()
            .filter(|d| d.record_kind == RecordKind::Memory && d.decision == Decision::Included)
            .filter_map(|d| {
                Some(MemoryRevisionRef {
                    memory_id: crate::ids::MemoryId::parse(&d.record_id).ok()?,
                    revision: d.revision,
                })
            })
            .collect();
        if &listed != included {
            out.push(Violation::new("inspection.capsule_mismatch", path.clone()));
        }
        for decision in inspection
            .decisions
            .iter()
            .filter(|d| d.decision == Decision::Excluded)
        {
            let supported = match decision.record_kind {
                RecordKind::Memory => {
                    let Some(memory) = visible.iter().find(|m| {
                        m.memory_id.as_str() == decision.record_id
                            && m.revision == decision.revision
                    }) else {
                        out.push(Violation::new("inspection.unknown_record", path.clone()));
                        continue;
                    };
                    let hard = hard_exclusion(set, memory, visible, as_of, capsule, policies);
                    match decision.reason {
                        DecisionReason::Tombstoned
                        | DecisionReason::BrokenProvenance
                        | DecisionReason::PolicyDenied
                        | DecisionReason::NotYetEffective
                        | DecisionReason::Archived => hard == Some(decision.reason),
                        DecisionReason::Superseded => {
                            matches!(effect(memory, visible, as_of), Effect::Superseded { .. })
                        }
                        DecisionReason::Expired => effect(memory, visible, as_of) == Effect::Ended,
                        DecisionReason::Conflicted => memory.conflict_group_id.is_some(),
                        DecisionReason::Unrelated | DecisionReason::OverBudget => hard.is_none(),
                        _ => false,
                    }
                }
                RecordKind::Candidate => {
                    let status = set
                        .candidate_revisions(&decision.record_id)
                        .into_iter()
                        .find(|c| c.revision == decision.revision)
                        .map(|c| c.status);
                    matches!(
                        (decision.reason, status),
                        (DecisionReason::Pending, Some(CandidateStatus::Pending))
                            | (DecisionReason::Rejected, Some(CandidateStatus::Rejected))
                    )
                }
                _ => true,
            };
            if !supported {
                out.push(Violation::new(
                    "inspection.reason_unsupported",
                    decision.record_id.clone(),
                ));
            }
        }
    }
}

/// Every resource a capsule legitimately carries, as exact record revisions.
fn capsule_resources(capsule: &ContextCapsule) -> BTreeSet<RecordRef> {
    let one = Revision::new(1).expect("1 is a revision");
    let mut resources = BTreeSet::new();
    for item in capsule.memory_items() {
        resources.insert(RecordRef::new(
            RecordKind::Memory,
            item.memory_id.as_str(),
            item.revision,
        ));
    }
    for item in &capsule.identity {
        resources.insert(RecordRef::new(
            RecordKind::Identity,
            item.identity_id.as_str(),
            item.revision,
        ));
    }
    for item in &capsule.recent_session_checkpoints {
        resources.insert(RecordRef::new(
            RecordKind::Checkpoint,
            item.checkpoint_id.as_str(),
            item.revision,
        ));
    }
    for turn in &capsule.recent_turns {
        resources.insert(RecordRef::new(
            RecordKind::SessionEvent,
            turn.event_id.as_str(),
            one,
        ));
    }
    for source in &capsule.provenance {
        resources.insert(RecordRef::new(
            RecordKind::Source,
            source.source_id.as_str(),
            source.source_revision,
        ));
    }
    resources
}

fn check_dispatch(set: &RecordSet, capsule: &ContextCapsule, out: &mut Vec<Violation>) {
    let carried = capsule_resources(capsule);
    for dispatch in set
        .dispatches
        .iter()
        .filter(|d| d.capsule_id == capsule.capsule_id)
    {
        let path = dispatch.dispatch_id.to_string();
        let inspection_ok = set.inspections.iter().any(|i| {
            i.inspection_id == dispatch.inspection_id && i.capsule_id == capsule.capsule_id
        });
        if dispatch.request_id != capsule.request_id || !inspection_ok {
            out.push(Violation::new("dispatch.header_mismatch", path.clone()));
        }
        if dispatch.destination != capsule.destination {
            out.push(Violation::new(
                "dispatch.destination_mismatch",
                path.clone(),
            ));
        }
        if dispatch.egress.policy_epoch < capsule.policy_epoch
            || dispatch.egress.deletion_epoch < capsule.deletion_epoch
        {
            out.push(Violation::new("dispatch.epoch_regression", path.clone()));
        }
        let external = dispatch.destination.kind == DestinationKind::ExternalProvider;
        // Egress is decided against the latest policies at send time, so a
        // revocation after compilation still blocks the send.
        let policies = set.all_policies();
        let send_time = dispatch.sent_at.as_ref().unwrap_or(&dispatch.prepared_at);
        if external {
            let usable = dispatch
                .egress
                .egress_policy_id
                .as_ref()
                .and_then(|id| {
                    set.policies
                        .iter()
                        .filter(|p| &p.policy_id == id)
                        .max_by_key(|p| p.revision)
                })
                .is_some_and(|p| {
                    p.active_at(send_time)
                        && p.scopes.contains(&Scope::ProviderSend)
                        && p.destinations
                            .iter()
                            .any(|d| d.matches(&dispatch.destination))
                });
            if !usable {
                out.push(Violation::new(
                    "dispatch.egress_policy_invalid",
                    path.clone(),
                ));
            }
        }
        let mut private = BTreeSet::new();
        for reference in dispatch.resource_refs() {
            if !carried.contains(reference) {
                out.push(Violation::new("dispatch.hidden_resource", path.clone()));
            }
            let Some((sensitivity, project_id)) = set.resource_info(reference) else {
                out.push(Violation::new("dispatch.hidden_resource", path.clone()));
                continue;
            };
            for tombstone in &set.tombstones {
                let targets = tombstone.targets.iter().any(|t| {
                    t.record_kind == reference.record_kind
                        && t.record_id == reference.record_id
                        && t.revision.is_none_or(|r| r == reference.revision)
                });
                if !targets {
                    continue;
                }
                if tombstone.deletion_epoch <= dispatch.egress.deletion_epoch {
                    out.push(Violation::new("dispatch.tombstoned_content", path.clone()));
                } else if tombstone.created_at <= dispatch.egress.checked_at {
                    out.push(Violation::new("dispatch.stale_barrier", path.clone()));
                }
            }
            if egress_rule(sensitivity, dispatch.destination.kind) == EgressRule::Denied {
                out.push(Violation::new("dispatch.policy_violation", path.clone()));
                continue;
            }
            if !external {
                continue;
            }
            let resource = ResourceContext {
                record: reference.clone(),
                project_id,
                sensitivity,
            };
            if !allowed(
                &policies,
                &capsule.requested_by,
                Scope::ProviderSend,
                capsule.purpose,
                Some(&dispatch.destination),
                &resource,
                send_time,
            ) {
                out.push(Violation::new("dispatch.policy_denied", path.clone()));
            }
            if sensitivity == Sensitivity::Private {
                private.insert(reference.clone());
            }
        }
        if private.is_empty() {
            continue;
        }
        // Private content to an external model: an owner egress approval bound
        // to this exact request, payload digest, destination, resources, and
        // policy epoch, valid at send time. A memory review never qualifies.
        let Some(approval) = dispatch
            .egress
            .egress_approval_id
            .as_ref()
            .and_then(|id| set.approval(id.as_str()))
        else {
            out.push(Violation::new("egress.approval_missing", path.clone()));
            continue;
        };
        let ApprovalBinding::Egress {
            request_id,
            capsule_id,
            payload_hash,
            destination,
            resources,
            policy_id: _,
            policy_epoch,
        } = &approval.binding
        else {
            out.push(Violation::new("egress.approval_kind", path.clone()));
            continue;
        };
        if request_id != &dispatch.request_id || capsule_id != &dispatch.capsule_id {
            out.push(Violation::new("egress.approval_request", path.clone()));
        }
        if payload_hash != &dispatch.request_hash {
            out.push(Violation::new("egress.approval_payload", path.clone()));
        }
        if destination != &dispatch.destination {
            out.push(Violation::new("egress.approval_destination", path.clone()));
        }
        let approved: BTreeSet<&RecordRef> = resources.iter().collect();
        if private.iter().any(|r| !approved.contains(r)) {
            out.push(Violation::new("egress.approval_resources", path.clone()));
        }
        if *policy_epoch != dispatch.egress.policy_epoch {
            out.push(Violation::new("egress.approval_epoch", path.clone()));
        }
        if !approval.valid_at(send_time) {
            out.push(Violation::new("egress.approval_expired", path.clone()));
        }
    }
}

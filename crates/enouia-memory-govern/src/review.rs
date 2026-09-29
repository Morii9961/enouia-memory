//! Owner review (MV-3.2, MV-3.3): plan, show, confirm.
//!
//! `plan` turns the owner's decisions into the exact records a commit would
//! write and hashes them (volatile timestamps and the hash itself excluded);
//! the trusted surface shows that plan. `confirm` rebuilds the plan on the
//! current head with the same generated IDs and commits only if the rebuilt
//! plan hashes the same, before the plan expires, for the Vault's owner on
//! the same trusted surface. A candidate or target that moved in between
//! makes the whole batch stale: nothing is written, nothing is reported as
//! done (M03). Each review carries a single-use nonce derived from the plan;
//! a repeated confirmation replays the first receipt.

use crate::propose::type_fields;
use crate::util::{Result, invalid, latest, next, one, revision, staged, stale};
use crate::view::{Canonical, canonical_memories};
use enouia_memory_contract::candidate::{
    CandidateRecord, CandidateStatus, MergeTarget, ProposalKind, ProposedType, ReviewAction,
    ReviewRecord,
};
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::common::{ActorRef, ActorType, EvidenceRef, TrustedSurface};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{CandidateId, CommitId, ConflictGroupId, MemoryId, ReviewId};
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::memory::{CanonicalMemory, MemoryBody, MemoryStatus};
use enouia_memory_contract::ports::{
    CommitOutcome, CommitPin, CommitRequest, IdempotencyScope, StagedRecord,
};
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::source::SourceRecord;
use enouia_memory_contract::time::{BusinessTime, Timestamp};
use enouia_memory_vault::Vault;
use enouia_memory_vault::fs::hex;
use serde_json::{Map, Value, json};
use std::collections::BTreeSet;

/// How long a shown plan may be confirmed.
pub const PLAN_TTL_MS: i64 = 10 * 60 * 1000;

/// One owner decision on one exact candidate revision.
#[derive(Clone, Debug, PartialEq)]
pub enum Decision {
    /// Write what the candidate proposes (a create, revise, archive, or
    /// supersede, by its proposal kind).
    Accept {
        candidate_id: CandidateId,
        revision: Revision,
    },
    /// Write it with the owner's text and fields instead. The candidate keeps
    /// the original proposal; the review hashes the approved text.
    EditAccept {
        candidate_id: CandidateId,
        revision: Revision,
        content: String,
        details: Option<Map<String, Value>>,
    },
    Reject {
        candidate_id: CandidateId,
        revision: Revision,
        reason_code: Option<String>,
    },
    /// Fold the candidate's evidence into a memory (new revision) or into a
    /// pending candidate (new pending revision).
    Merge {
        candidate_id: CandidateId,
        revision: Revision,
        into: MergeTarget,
    },
}

impl Decision {
    fn candidate(&self) -> (&CandidateId, Revision) {
        match self {
            Self::Accept {
                candidate_id,
                revision,
            }
            | Self::EditAccept {
                candidate_id,
                revision,
                ..
            }
            | Self::Reject {
                candidate_id,
                revision,
                ..
            }
            | Self::Merge {
                candidate_id,
                revision,
                ..
            } => (candidate_id, *revision),
        }
    }
}

/// IDs generated when a plan is made and reused when it is confirmed.
#[derive(Clone, Debug, PartialEq)]
pub struct PlannedIds {
    pub review_id: ReviewId,
    pub memory_id: MemoryId,
    pub conflict_group_id: ConflictGroupId,
}

/// What the trusted surface shows before the owner confirms.
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewPlan {
    pub owner: ActorRef,
    pub surface: TrustedSurface,
    pub decisions: Vec<Decision>,
    pub ids: Vec<PlannedIds>,
    pub commit_id: CommitId,
    pub nonce: String,
    pub issued_at: Timestamp,
    pub expires_at: Timestamp,
    /// Every record the commit writes, without volatile timestamps.
    pub diff: Value,
    pub diff_hash: Sha256Hex,
}

/// The owner's confirmation on a trusted surface.
#[derive(Clone, Debug, PartialEq)]
pub struct OwnerConfirmation {
    pub owner: ActorRef,
    pub surface: TrustedSurface,
}

/// Keys left out of the diff: when it is confirmed, not what is approved.
const VOLATILE: &[&str] = &[
    "created_at",
    "updated_at",
    "approved_at",
    "approved_diff_hash",
];

fn check_owner(vault: &Vault, owner: &ActorRef) -> Result<()> {
    if owner.actor_type != ActorType::Owner {
        return Err(invalid("review.owner_required"));
    }
    if owner != &vault.descriptor().created_by {
        return Err(invalid("review.not_vault_owner"));
    }
    Ok(())
}

fn scope(owner: &ActorRef, nonce: &str) -> IdempotencyScope {
    IdempotencyScope {
        principal_id: owner.actor_id.clone(),
        operation_kind: OperationKind::ReviewCommit,
        key_hash: sha256(nonce.as_bytes()),
    }
}

/// Records and preconditions of one plan, built on one head.
struct Built {
    records: Vec<(RecordKind, String, Revision, Value)>,
    expected: Vec<(RecordKind, String, Option<Revision>)>,
}

impl Built {
    fn push(&mut self, kind: RecordKind, id: &str, rev: Revision, value: Value) {
        self.records.push((kind, id.to_owned(), rev, value));
    }

    fn expect(&mut self, kind: RecordKind, id: &str, rev: Option<Revision>) -> Result<()> {
        if self.expected.iter().any(|(k, i, _)| *k == kind && i == id) {
            return Err(invalid("review.duplicate_target"));
        }
        self.expected.push((kind, id.to_owned(), rev));
        Ok(())
    }

    fn diff(&self, plan: &PlanHeader) -> Value {
        let records: Vec<Value> = self
            .records
            .iter()
            .map(|(kind, id, rev, value)| {
                let mut value = value.clone();
                if let Some(map) = value.as_object_mut() {
                    for key in VOLATILE {
                        map.remove(*key);
                    }
                }
                json!({"record_kind": kind, "record_id": id, "revision": rev.get(), "value": value})
            })
            .collect();
        json!({
            "owner": plan.owner,
            "surface": plan.surface,
            "commit_id": plan.commit_id,
            "nonce": plan.nonce,
            "records": records,
        })
    }
}

struct PlanHeader<'a> {
    owner: &'a ActorRef,
    surface: TrustedSurface,
    commit_id: &'a CommitId,
    nonce: &'a str,
}

struct Context<'a> {
    vault: &'a Vault,
    pin: CommitPin,
    header: PlanHeader<'a>,
    now: Timestamp,
    diff_hash: Sha256Hex,
    visible: Vec<CanonicalMemory>,
}

impl Context<'_> {
    fn memory(&self, id: &MemoryId) -> Result<&CanonicalMemory> {
        self.visible
            .iter()
            .find(|m| &m.memory_id == id)
            .ok_or_else(|| invalid("review.target_missing"))
    }

    fn primary_source(&self, candidate: &CandidateRecord) -> Result<SourceRecord> {
        let cited = candidate
            .evidence
            .iter()
            .find(|e| e.source_id == candidate.source_id)
            .ok_or_else(|| invalid("candidate.primary_source"))?;
        revision(
            self.vault,
            &self.pin,
            RecordKind::Source,
            candidate.source_id.as_str(),
            cited.source_revision,
        )?
        .ok_or_else(|| invalid("evidence.unresolved"))
    }
}

fn union(mut base: Vec<EvidenceRef>, more: &[EvidenceRef]) -> Vec<EvidenceRef> {
    for item in more {
        if !base.contains(item) {
            base.push(item.clone());
        }
    }
    base
}

fn source_refs(evidence: &[EvidenceRef]) -> Vec<Value> {
    let mut seen = BTreeSet::new();
    evidence
        .iter()
        .filter(|e| seen.insert((e.source_id.to_string(), e.source_revision.get())))
        .map(|e| json!({"source_id": e.source_id, "source_revision": e.source_revision}))
        .collect()
}

fn reference(kind: RecordKind, id: &str, rev: Revision) -> Value {
    json!({"record_kind": kind, "record_id": id, "revision": rev.get()})
}

/// The scope string recorded on a supersession edge.
fn edge_scope(memory: &CanonicalMemory) -> String {
    match &memory.body {
        MemoryBody::Fact(f) => format!("claim:{}", f.claim_key),
        MemoryBody::Preference(p) => format!("preference:{}", p.scope),
        MemoryBody::ProjectState(_) => "project_state".to_owned(),
        MemoryBody::Episode(_) => "episode".to_owned(),
        MemoryBody::SessionCheckpoint(_) => "session_checkpoint".to_owned(),
    }
}

fn type_name(proposed: ProposedType) -> Result<&'static str> {
    Ok(match proposed {
        ProposedType::Fact => "fact",
        ProposedType::Preference => "preference",
        ProposedType::Episode => "episode",
        ProposedType::ProjectState => "project_state",
        _ => return Err(invalid("govern.type_unsupported")),
    })
}

/// A new memory, revision 1, from a candidate and the approved text/fields.
#[allow(clippy::too_many_arguments)]
fn new_memory(
    ctx: &Context,
    candidate: &CandidateRecord,
    content: &str,
    details: &Option<Map<String, Value>>,
    id: &MemoryId,
    review_id: &ReviewId,
    group: Option<&ConflictGroupId>,
    supersedes: Value,
) -> Result<Value> {
    let source = ctx.primary_source(candidate)?;
    let title: String = content.chars().take(40).collect();
    let mut value = json!({
        "title": title,
        "subject_ids": [],
        "project_id": null,
        "category": null,
        "tags": [],
        "epistemic_status": "asserted",
        "valid_from": null,
        "valid_until": null,
        "observed_at": source.occurred_at,
        "review_after": null,
        "volatility": "stable",
        "priority": "P2",
    });
    let map = value.as_object_mut().expect("object");
    for (key, field) in details.iter().flatten() {
        map.insert(key.clone(), field.clone());
    }
    let fixed = json!({
        "schema_version": 1,
        "memory_id": id,
        "revision": 1,
        "type": type_name(candidate.proposed_type)?,
        "content": content,
        "source_id": candidate.source_id,
        "evidence": candidate.evidence,
        "confidence": candidate.confidence,
        "last_verified_at": null,
        "sensitivity": candidate.sensitivity,
        "access_policy_id": source.access_policy_id,
        "egress_policy_id": null,
        "status": "active",
        "supersedes": supersedes,
        "conflict_group_id": group,
        "provenance_state": "intact",
        "review_id": review_id,
        "approved_by": ctx.header.owner,
        "approved_at": ctx.now,
        "declassification_approval_id": candidate.declassification_approval_id,
        "created_at": ctx.now,
        "updated_at": ctx.now,
        "extensions": {},
    });
    for (key, field) in fixed.as_object().expect("object") {
        map.insert(key.clone(), field.clone());
    }
    Ok(value)
}

/// The next revision of an existing memory, approved by `review_id`.
fn revised(ctx: &Context, memory: &CanonicalMemory, review_id: &ReviewId) -> Value {
    let mut value = serde_json::to_value(memory).expect("memory serializes");
    value["revision"] = json!(next(memory.revision).get());
    value["review_id"] = json!(review_id);
    value["approved_by"] = json!(ctx.header.owner);
    value["approved_at"] = json!(ctx.now);
    value["updated_at"] = json!(ctx.now);
    value
}

fn stage_memory(built: &mut Built, value: Value) -> Result<()> {
    let id = value["memory_id"].as_str().unwrap_or_default().to_owned();
    let rev = value["revision"]
        .as_u64()
        .and_then(Revision::new)
        .ok_or_else(|| invalid("shape"))?;
    let _: (CanonicalMemory, StagedRecord) = staged(RecordKind::Memory, &id, rev, &value)?;
    built.push(RecordKind::Memory, &id, rev, value);
    Ok(())
}

/// Stage one decision: the resolved candidate, the review, and every record
/// the decision writes.
fn decide(
    ctx: &Context,
    built: &mut Built,
    decision: &Decision,
    ids: &PlannedIds,
    index: usize,
) -> Result<()> {
    let (candidate_id, rev) = decision.candidate();
    let (candidate, latest_rev): (CandidateRecord, _) = latest(
        ctx.vault,
        &ctx.pin,
        RecordKind::Candidate,
        candidate_id.as_str(),
    )?
    .ok_or_else(|| invalid("candidate.missing"))?;
    if latest_rev != rev || candidate.status != CandidateStatus::Pending {
        return Err(stale());
    }
    built.expect(RecordKind::Candidate, candidate_id.as_str(), Some(rev))?;
    let review_id = &ids.review_id;
    let mut results: Vec<Value> = Vec::new();
    let mut targets: Vec<Value> = Vec::new();
    let mut final_content = candidate.proposed_content.clone();
    let mut effective_from: Option<BusinessTime> = None;
    let mut merge_target: Option<MergeTarget> = None;
    let mut reason_code: Option<String> = None;
    let (action, status) = match decision {
        Decision::Reject { reason_code: r, .. } => {
            reason_code = r.clone();
            (ReviewAction::Reject, "rejected")
        }
        Decision::Merge { into, .. } => {
            merge_target = Some(into.clone());
            match into {
                MergeTarget::Memory { memory_id } => {
                    let memory = ctx.memory(memory_id)?.clone();
                    built.expect(
                        RecordKind::Memory,
                        memory_id.as_str(),
                        Some(memory.revision),
                    )?;
                    targets.push(reference(
                        RecordKind::Memory,
                        memory_id.as_str(),
                        memory.revision,
                    ));
                    let mut value = revised(ctx, &memory, review_id);
                    value["evidence"] = json!(union(memory.evidence.clone(), &candidate.evidence));
                    value["sensitivity"] = json!(memory.sensitivity.max(candidate.sensitivity));
                    results.push(reference(
                        RecordKind::Memory,
                        memory_id.as_str(),
                        next(memory.revision),
                    ));
                    final_content = memory.content.clone();
                    stage_memory(built, value)?;
                }
                MergeTarget::Candidate {
                    candidate_id: target,
                } => {
                    if target == candidate_id {
                        return Err(invalid("candidate.merge_target"));
                    }
                    let (other, other_rev): (CandidateRecord, _) =
                        latest(ctx.vault, &ctx.pin, RecordKind::Candidate, target.as_str())?
                            .ok_or_else(|| invalid("candidate.merge_target"))?;
                    if other.status != CandidateStatus::Pending {
                        return Err(invalid("candidate.merge_target"));
                    }
                    built.expect(RecordKind::Candidate, target.as_str(), Some(other_rev))?;
                    let mut value = serde_json::to_value(&other).expect("candidate");
                    value["revision"] = json!(next(other_rev).get());
                    value["evidence"] = json!(union(other.evidence.clone(), &candidate.evidence));
                    value["sensitivity"] = json!(other.sensitivity.max(candidate.sensitivity));
                    value["updated_at"] = json!(ctx.now);
                    let _: (CandidateRecord, StagedRecord) = staged(
                        RecordKind::Candidate,
                        target.as_str(),
                        next(other_rev),
                        &value,
                    )?;
                    built.push(
                        RecordKind::Candidate,
                        target.as_str(),
                        next(other_rev),
                        value,
                    );
                    results.push(reference(
                        RecordKind::Candidate,
                        target.as_str(),
                        next(other_rev),
                    ));
                }
            }
            (ReviewAction::Merge, "merged")
        }
        Decision::Accept { .. } | Decision::EditAccept { .. } => {
            let (content, details, edited) = match decision {
                Decision::EditAccept {
                    content, details, ..
                } => {
                    crate::propose::check_details(candidate.proposed_type, details)?;
                    (
                        content.clone(),
                        details.clone().or(candidate.proposed_details.clone()),
                        true,
                    )
                }
                _ => (
                    candidate.proposed_content.clone(),
                    candidate.proposed_details.clone(),
                    false,
                ),
            };
            final_content = content.clone();
            let plain = if edited {
                ReviewAction::EditAccept
            } else {
                ReviewAction::Accept
            };
            let action = match candidate.proposal_kind {
                ProposalKind::Create => {
                    create(
                        ctx,
                        built,
                        &candidate,
                        &content,
                        &details,
                        ids,
                        &mut results,
                        &mut targets,
                    )?;
                    plain
                }
                ProposalKind::Revise | ProposalKind::Archive => {
                    let target_id = candidate
                        .target_memory_id
                        .as_ref()
                        .ok_or_else(|| invalid("candidate.target_fields"))?;
                    let memory = ctx.memory(target_id)?.clone();
                    if Some(memory.revision) != candidate.expected_revision {
                        return Err(stale());
                    }
                    built.expect(
                        RecordKind::Memory,
                        target_id.as_str(),
                        Some(memory.revision),
                    )?;
                    targets.push(reference(
                        RecordKind::Memory,
                        target_id.as_str(),
                        memory.revision,
                    ));
                    let mut value = revised(ctx, &memory, review_id);
                    if candidate.proposal_kind == ProposalKind::Archive {
                        value["status"] = json!("archived");
                        final_content = memory.content.clone();
                    } else {
                        value["content"] = json!(content);
                        for (key, field) in details.iter().flatten() {
                            value[key.as_str()] = field.clone();
                        }
                        value["evidence"] =
                            json!(union(memory.evidence.clone(), &candidate.evidence));
                        value["sensitivity"] = json!(memory.sensitivity.max(candidate.sensitivity));
                    }
                    results.push(reference(
                        RecordKind::Memory,
                        target_id.as_str(),
                        next(memory.revision),
                    ));
                    stage_memory(built, value)?;
                    plain
                }
                ProposalKind::Supersede => {
                    effective_from = candidate.proposed_effective_from.clone();
                    supersede(
                        ctx,
                        built,
                        &candidate,
                        &content,
                        &details,
                        ids,
                        &mut results,
                        &mut targets,
                    )?;
                    ReviewAction::Supersede
                }
                ProposalKind::IdentityChange | ProposalKind::Delete => {
                    return Err(invalid("govern.not_supported"));
                }
            };
            (action, "accepted")
        }
    };
    // The candidate's resolving revision.
    let mut resolved = serde_json::to_value(&candidate).expect("candidate");
    resolved["revision"] = json!(next(rev).get());
    resolved["status"] = json!(status);
    resolved["resolution_review_id"] = json!(review_id);
    resolved["merged_into"] = json!(merge_target);
    resolved["updated_at"] = json!(ctx.now);
    let _: (CandidateRecord, StagedRecord) = staged(
        RecordKind::Candidate,
        candidate_id.as_str(),
        next(rev),
        &resolved,
    )?;
    built.push(
        RecordKind::Candidate,
        candidate_id.as_str(),
        next(rev),
        resolved,
    );
    let review = json!({
        "schema_version": 1,
        "review_id": review_id,
        "actor": ctx.header.owner,
        "trusted_surface": ctx.header.surface,
        "action": action,
        "candidate_id": candidate_id,
        "candidate_revision": rev.get(),
        "final_content_hash": sha256(final_content.as_bytes()),
        "approved_diff_hash": ctx.diff_hash,
        "evidence_refs": source_refs(&candidate.evidence),
        "target_expected_revisions": targets,
        "approval_nonce": format!("{}-{index}", ctx.header.nonce),
        "effective_from": effective_from,
        "merge_target": merge_target,
        "resulting_records": results,
        "delete_binding": null,
        "reason_code": reason_code,
        "created_at": ctx.now,
        "commit_id": ctx.header.commit_id,
    });
    let _: (ReviewRecord, StagedRecord) =
        staged(RecordKind::Review, review_id.as_str(), one(), &review)?;
    built.expect(RecordKind::Review, review_id.as_str(), None)?;
    built.push(RecordKind::Review, review_id.as_str(), one(), review);
    Ok(())
}

/// Accept a create: a new memory. If the candidate listed contradicting
/// active memories, the new memory and every one of them join one conflict
/// group; neither wins (M07). A supersede proposal is how one replaces the
/// other.
#[allow(clippy::too_many_arguments)]
fn create(
    ctx: &Context,
    built: &mut Built,
    candidate: &CandidateRecord,
    content: &str,
    details: &Option<Map<String, Value>>,
    ids: &PlannedIds,
    results: &mut Vec<Value>,
    targets: &mut Vec<Value>,
) -> Result<()> {
    let mut group = None;
    let mut others = Vec::new();
    for conflict in &candidate.conflicts {
        let Ok(memory) = ctx.memory(&conflict.memory_id) else {
            continue; // archived, superseded, or deleted since: no longer a live claim
        };
        if memory.status != MemoryStatus::Active {
            continue;
        }
        if group.is_none() {
            group = memory.conflict_group_id.clone();
        }
        others.push(memory.clone());
    }
    let group = match (group, others.is_empty()) {
        (_, true) => None,
        (Some(existing), false) => Some(existing),
        (None, false) => Some(ids.conflict_group_id.clone()),
    };
    let value = new_memory(
        ctx,
        candidate,
        content,
        details,
        &ids.memory_id,
        &ids.review_id,
        group.as_ref(),
        json!([]),
    )?;
    stage_memory(built, value)?;
    results.push(reference(RecordKind::Memory, ids.memory_id.as_str(), one()));
    for memory in others {
        if memory.conflict_group_id == group {
            continue;
        }
        built.expect(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            Some(memory.revision),
        )?;
        targets.push(reference(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            memory.revision,
        ));
        let mut value = revised(ctx, &memory, &ids.review_id);
        value["conflict_group_id"] = json!(group);
        results.push(reference(
            RecordKind::Memory,
            memory.memory_id.as_str(),
            next(memory.revision),
        ));
        stage_memory(built, value)?;
    }
    Ok(())
}

/// Accept a supersede: a new active memory with an explicit edge (target
/// revision, effective time as confirmed, scope) and the target's next
/// revision marked superseded, in one commit (V04, M05). The old revisions
/// stay; `effective_from` in the future does not hide the current fact (M06).
#[allow(clippy::too_many_arguments)]
fn supersede(
    ctx: &Context,
    built: &mut Built,
    candidate: &CandidateRecord,
    content: &str,
    details: &Option<Map<String, Value>>,
    ids: &PlannedIds,
    results: &mut Vec<Value>,
    targets: &mut Vec<Value>,
) -> Result<()> {
    let target_id = candidate
        .target_memory_id
        .as_ref()
        .ok_or_else(|| invalid("candidate.target_fields"))?;
    let old = ctx.memory(target_id)?.clone();
    if Some(old.revision) != candidate.expected_revision {
        return Err(stale());
    }
    let effective = candidate
        .proposed_effective_from
        .clone()
        .ok_or_else(|| invalid("candidate.effective_from"))?;
    let edge = json!([{
        "memory_id": target_id,
        "revision": old.revision.get(),
        "effective_from": effective,
        "scope": edge_scope(&old),
    }]);
    // The replacement keeps the claim's scope unless the owner edits it.
    let mut inherited = Map::new();
    let old_value = serde_json::to_value(&old).expect("memory");
    for key in type_fields(candidate.proposed_type)
        .iter()
        .chain(["subject_ids", "project_id"].iter())
    {
        if let Some(field) = old_value.get(*key) {
            inherited.insert((*key).to_owned(), field.clone());
        }
    }
    for (key, field) in details.iter().flatten() {
        inherited.insert(key.clone(), field.clone());
    }
    let value = new_memory(
        ctx,
        candidate,
        content,
        &Some(inherited),
        &ids.memory_id,
        &ids.review_id,
        None,
        edge,
    )?;
    stage_memory(built, value)?;
    results.push(reference(RecordKind::Memory, ids.memory_id.as_str(), one()));
    built.expect(RecordKind::Memory, target_id.as_str(), Some(old.revision))?;
    targets.push(reference(
        RecordKind::Memory,
        target_id.as_str(),
        old.revision,
    ));
    let mut value = revised(ctx, &old, &ids.review_id);
    value["status"] = json!("superseded");
    results.push(reference(
        RecordKind::Memory,
        target_id.as_str(),
        next(old.revision),
    ));
    stage_memory(built, value)
}

fn build(
    vault: &Vault,
    header: PlanHeader,
    decisions: &[Decision],
    ids: &[PlannedIds],
    now: Timestamp,
    diff_hash: Sha256Hex,
) -> Result<(Built, Value)> {
    let pin = vault.pin_current()?;
    let visible = canonical_memories(vault, &pin, Canonical::AllStatuses)?;
    let ctx = Context {
        vault,
        pin,
        header,
        now,
        diff_hash,
        visible,
    };
    let mut built = Built {
        records: Vec::new(),
        expected: Vec::new(),
    };
    for (index, (decision, ids)) in decisions.iter().zip(ids).enumerate() {
        decide(&ctx, &mut built, decision, ids, index)?;
    }
    let diff = built.diff(&ctx.header);
    Ok((built, diff))
}

fn hash(diff: &Value) -> Sha256Hex {
    sha256(&canonical_bytes(diff).expect("diff serializes"))
}

/// Build the plan the trusted surface shows. Refused for anyone but the
/// Vault's owner.
pub fn plan(
    vault: &Vault,
    decisions: &[Decision],
    owner: &ActorRef,
    surface: TrustedSurface,
) -> Result<ReviewPlan> {
    check_owner(vault, owner)?;
    if decisions.is_empty() {
        return Err(invalid("review.empty"));
    }
    let mut seen = BTreeSet::new();
    for decision in decisions {
        if !seen.insert(decision.candidate().0.to_string()) {
            return Err(invalid("review.duplicate_target"));
        }
    }
    let ids: Vec<PlannedIds> = decisions
        .iter()
        .map(|_| PlannedIds {
            review_id: ReviewId::from_random(vault.random_id_bytes()),
            memory_id: MemoryId::from_random(vault.random_id_bytes()),
            conflict_group_id: ConflictGroupId::from_random(vault.random_id_bytes()),
        })
        .collect();
    let commit_id = CommitId::from_random(vault.random_id_bytes());
    let nonce = format!("rv-{}", hex(&vault.random_id_bytes()));
    let issued_at = vault.now()?;
    let expires_at = Timestamp::from_unix_ms(issued_at.unix_ms() + PLAN_TTL_MS)
        .ok_or_else(|| invalid("review.plan_time"))?;
    let header = PlanHeader {
        owner,
        surface,
        commit_id: &commit_id,
        nonce: &nonce,
    };
    let placeholder = sha256(b"");
    let (_, diff) = build(
        vault,
        header,
        decisions,
        &ids,
        issued_at.clone(),
        placeholder,
    )?;
    Ok(ReviewPlan {
        owner: owner.clone(),
        surface,
        decisions: decisions.to_vec(),
        ids,
        commit_id: commit_id.clone(),
        diff_hash: hash(&diff),
        diff,
        nonce,
        issued_at,
        expires_at,
    })
}

/// Commit a shown plan after the owner confirmed it on the same trusted
/// surface. Stale, expired, or foreign confirmations write nothing.
pub fn confirm(
    vault: &Vault,
    plan: &ReviewPlan,
    confirmation: &OwnerConfirmation,
) -> Result<CommitOutcome> {
    check_owner(vault, &confirmation.owner)?;
    if confirmation.owner != plan.owner || confirmation.surface != plan.surface {
        return Err(invalid("review.confirmation_mismatch"));
    }
    let key = scope(&plan.owner, &plan.nonce);
    if let Some((commit_id, payload, receipt)) = vault.find_receipt(&key)? {
        return if payload == plan.diff_hash {
            Ok(CommitOutcome::Replayed { commit_id, receipt })
        } else {
            Err(invalid("review.nonce_reuse"))
        };
    }
    let now = vault.now()?;
    if now > plan.expires_at {
        return Err(invalid("review.plan_expired"));
    }
    let header = PlanHeader {
        owner: &plan.owner,
        surface: plan.surface,
        commit_id: &plan.commit_id,
        nonce: &plan.nonce,
    };
    let (built, diff) = build(
        vault,
        header,
        &plan.decisions,
        &plan.ids,
        now,
        plan.diff_hash.clone(),
    )?;
    if hash(&diff) != plan.diff_hash {
        return Err(stale());
    }
    let mut records = Vec::new();
    for (kind, id, rev, value) in &built.records {
        records.push(StagedRecord {
            record_kind: *kind,
            record_id: id.clone(),
            revision: *rev,
            bytes: canonical_bytes(value)
                .map_err(|e| enouia_memory_vault::VaultError::invalid(e.rules()))?,
        });
    }
    vault.commit(CommitRequest {
        commit_id: plan.commit_id.clone(),
        expected_commit_id: None,
        principal: plan.owner.clone(),
        operation_kind: OperationKind::ReviewCommit,
        idempotency: key,
        request_payload_hash: plan.diff_hash.clone(),
        expected_revisions: built.expected,
        records,
        objects: Vec::new(),
    })
}

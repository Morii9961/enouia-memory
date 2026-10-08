//! Candidates (MV-3.1): anyone with the propose right may suggest a memory,
//! a change to one, or an identity change; the suggestion is stored as a
//! pending candidate and nothing canonical changes. Evidence is resolved from
//! the Vault, sensitivity never drops below the strictest cited source, a
//! proposal against a stale target revision is refused, a pending duplicate
//! is found by fingerprint instead of stored twice, and contradictions with
//! active memories are listed (never resolved automatically).

use crate::evidence::{EvidenceSpec, Resolved, resolve};
use crate::util::{Result, all_latest, invalid, latest, next, one, staged, stale};
use crate::view::{Canonical, canonical_memories};
use enouia_memory_contract::candidate::{
    CandidateConflict, CandidateRecord, CandidateStatus, OriginKind, ProposalKind, ProposedType,
};
use enouia_memory_contract::commit::OperationKind;
use enouia_memory_contract::common::{ActorRef, ActorType, Sensitivity};
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::ids::{CandidateId, CommitId, ExtractionRunId, IdentityId, MemoryId};
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::memory::{CanonicalMemory, MemoryBody};
use enouia_memory_contract::ports::{CommitPin, CommitRequest, IdempotencyScope};
use enouia_memory_contract::record::RecordKind;
use enouia_memory_contract::time::BusinessTime;
use enouia_memory_vault::Vault;
use enouia_memory_vault::service::Written;
use serde_json::{Map, Value, json};

/// Memory fields a proposal (or the owner's edit) may set. Everything else
/// (IDs, revisions, status, supersession, evidence, approval, policy) is
/// decided by the review that writes the memory.
pub const MEMORY_FIELDS: &[&str] = &[
    "title",
    "subject_ids",
    "project_id",
    "category",
    "tags",
    "epistemic_status",
    "valid_from",
    "valid_until",
    "observed_at",
    "review_after",
    "volatility",
    "priority",
    // `{display_name, aliases}`: a new project entity, created by the
    // accepting review and set as the memory's project.
    "new_project",
];

/// Type-specific fields, by proposed type.
pub fn type_fields(proposed: ProposedType) -> &'static [&'static str] {
    match proposed {
        ProposedType::Fact => &["claim_key"],
        ProposedType::Preference => &["scope", "strength"],
        ProposedType::Episode => &[
            "occurred_start",
            "occurred_end",
            "occurred_precision",
            "participants",
            "summary",
        ],
        ProposedType::ProjectState => &["state", "decisions", "open_loops"],
        ProposedType::Identity => &["slug", "title"],
        ProposedType::SessionCheckpoint => &[],
    }
}

/// What is being proposed.
#[derive(Clone, Debug, PartialEq)]
pub struct Proposal {
    pub kind: ProposalKind,
    pub proposed_type: ProposedType,
    pub content: String,
    /// Memory (or identity) fields, validated when a review writes them.
    pub details: Option<Map<String, Value>>,
    pub evidence: Vec<EvidenceSpec>,
    pub reason: String,
    /// Defaults to the strictest cited source; a lower level is refused.
    pub sensitivity: Option<Sensitivity>,
    pub target_memory_id: Option<MemoryId>,
    pub target_identity_id: Option<IdentityId>,
    pub expected_revision: Option<Revision>,
    pub effective_from: Option<BusinessTime>,
    pub reopens_candidate_id: Option<CandidateId>,
}

impl Proposal {
    /// A new memory of `proposed_type`.
    pub fn create(
        proposed_type: ProposedType,
        content: &str,
        details: Map<String, Value>,
        evidence: Vec<EvidenceSpec>,
    ) -> Self {
        Self {
            kind: ProposalKind::Create,
            proposed_type,
            content: content.to_owned(),
            details: Some(details),
            evidence,
            reason: "proposed".to_owned(),
            sensitivity: None,
            target_memory_id: None,
            target_identity_id: None,
            expected_revision: None,
            effective_from: None,
            reopens_candidate_id: None,
        }
    }
}

/// Who proposes, and how.
#[derive(Clone, Debug, PartialEq)]
pub struct Origin {
    pub kind: OriginKind,
    pub actor: ActorRef,
    pub extraction_run_id: Option<ExtractionRunId>,
    /// Advisory only; never approval.
    pub confidence: Option<f64>,
}

impl Origin {
    pub fn owner(actor: ActorRef) -> Self {
        Self {
            kind: OriginKind::OwnerManual,
            actor,
            extraction_run_id: None,
            confidence: None,
        }
    }

    pub fn agent(actor: ActorRef) -> Self {
        Self {
            kind: OriginKind::AgentProposal,
            actor,
            extraction_run_id: None,
            confidence: None,
        }
    }
}

/// Content without any whitespace: what the fingerprint and the conflict
/// check compare (Chinese text has no word spaces to preserve).
fn normalized(text: &str) -> String {
    text.chars().filter(|c| !c.is_whitespace()).collect()
}

fn fingerprint(
    kind: ProposalKind,
    proposed_type: ProposedType,
    content: &str,
    details: &Option<Map<String, Value>>,
    target_memory: &Option<MemoryId>,
    target_identity: &Option<IdentityId>,
) -> Sha256Hex {
    let value = json!({
        "proposal_kind": kind,
        "proposed_type": proposed_type,
        "content": normalized(content),
        "details": details,
        "target_memory_id": target_memory,
        "target_identity_id": target_identity,
    });
    sha256(&canonical_bytes(&value).expect("fingerprint input"))
}

pub(crate) fn check_details(
    kind: ProposalKind,
    proposed: ProposedType,
    details: &Option<Map<String, Value>>,
) -> Result<()> {
    if kind == ProposalKind::Delete {
        // A delete proposal carries only how to delete (MV-3.4).
        return match details
            .iter()
            .flat_map(|d| d.keys())
            .all(|k| k == "mode" || k == "scope")
        {
            true => Ok(()),
            false => Err(invalid("candidate.details_field")),
        };
    }
    if proposed == ProposedType::SessionCheckpoint {
        // Session checkpoint memories come from reviewed checkpoints (MV-5).
        return Err(invalid("govern.type_unsupported"));
    }
    let allowed: &[&str] = if proposed == ProposedType::Identity {
        &[]
    } else {
        MEMORY_FIELDS
    };
    for key in details.iter().flat_map(|d| d.keys()) {
        if !allowed.contains(&key.as_str()) && !type_fields(proposed).contains(&key.as_str()) {
            return Err(invalid("candidate.details_field"));
        }
    }
    Ok(())
}

/// The scope in which two memories of one type make competing claims.
fn claim_scope(memory: &CanonicalMemory) -> Option<String> {
    match &memory.body {
        MemoryBody::Fact(f) => Some(format!("fact:{}", f.claim_key)),
        MemoryBody::Preference(p) => Some(format!("preference:{}", p.scope)),
        _ => None,
    }
}

fn proposed_scope(proposed: ProposedType, details: &Option<Map<String, Value>>) -> Option<String> {
    let field = |key: &str| details.as_ref()?.get(key)?.as_str().map(str::to_owned);
    match proposed {
        ProposedType::Fact => field("claim_key").map(|k| format!("fact:{k}")),
        ProposedType::Preference => field("scope").map(|s| format!("preference:{s}")),
        _ => None,
    }
}

fn subjects(details: &Option<Map<String, Value>>) -> Vec<String> {
    details
        .as_ref()
        .and_then(|d| d.get("subject_ids"))
        .and_then(Value::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_owned))
                .collect()
        })
        .unwrap_or_default()
}

/// Active memories that make a different claim in the same scope (M07).
/// They are listed on the candidate; the review decides, never time or
/// confidence.
fn conflicts(
    vault: &Vault,
    pin: &CommitPin,
    proposal: &Proposal,
) -> Result<Vec<CandidateConflict>> {
    if proposal.kind != ProposalKind::Create {
        return Ok(Vec::new());
    }
    let Some(scope) = proposed_scope(proposal.proposed_type, &proposal.details) else {
        return Ok(Vec::new());
    };
    let wanted = subjects(&proposal.details);
    let mut out = Vec::new();
    for memory in canonical_memories(vault, pin, Canonical::Active)? {
        let shared = (wanted.is_empty() && memory.subject_ids.is_empty())
            || memory
                .subject_ids
                .iter()
                .any(|s| wanted.contains(&s.to_string()));
        if claim_scope(&memory).as_ref() == Some(&scope)
            && shared
            && normalized(&memory.content) != normalized(&proposal.content)
        {
            out.push(CandidateConflict {
                memory_id: memory.memory_id.clone(),
                revision: memory.revision,
                conflict_group_id: memory.conflict_group_id.clone(),
            });
        }
    }
    Ok(out)
}

fn is_owner(vault: &Vault, actor: &ActorRef) -> bool {
    actor.actor_type == ActorType::Owner && actor == &vault.descriptor().created_by
}

/// The target memory must exist, be visible, and be at the expected revision.
fn check_target(vault: &Vault, pin: &CommitPin, proposal: &Proposal) -> Result<()> {
    if let Some(target) = &proposal.target_memory_id {
        let visible = canonical_memories(vault, pin, Canonical::AllStatuses)?;
        let memory = visible
            .iter()
            .find(|m| &m.memory_id == target)
            .ok_or_else(|| invalid("candidate.target_missing"))?;
        if Some(memory.revision) != proposal.expected_revision {
            return Err(stale());
        }
    }
    if let Some(target) = &proposal.target_identity_id {
        let current = vault.record_entry(pin, RecordKind::Identity, target.as_str())?;
        if current.map(|e| e.revision) != proposal.expected_revision {
            return Err(stale());
        }
    }
    Ok(())
}

/// Result of a proposal.
#[derive(Clone, Debug, PartialEq)]
pub enum Proposed {
    /// A new pending candidate (or the replay of this exact request).
    Stored(Written<CandidateId>),
    /// A pending candidate with the same fingerprint already exists.
    DuplicateOf(CandidateId),
}

fn scope(actor: &ActorRef, key: &[u8]) -> IdempotencyScope {
    IdempotencyScope {
        principal_id: actor.actor_id.clone(),
        operation_kind: OperationKind::CandidatePropose,
        key_hash: sha256(key),
    }
}

fn commit_candidate(
    vault: &Vault,
    actor: &ActorRef,
    key: &[u8],
    candidate: &Value,
    id: &CandidateId,
    revision: Revision,
    expected_head: Option<&CommitId>,
) -> Result<Written<CandidateId>> {
    let (_, record): (CandidateRecord, _) =
        staged(RecordKind::Candidate, id.as_str(), revision, candidate)?;
    let expected = revision.get().checked_sub(1).and_then(Revision::new);
    let outcome = vault.commit(CommitRequest {
        commit_id: CommitId::from_random(vault.random_id_bytes()),
        expected_commit_id: expected_head.cloned(),
        principal: actor.clone(),
        operation_kind: OperationKind::CandidatePropose,
        idempotency: scope(actor, key),
        request_payload_hash: sha256(&record.bytes),
        expected_revisions: vec![(RecordKind::Candidate, id.to_string(), expected)],
        records: vec![record],
        objects: Vec::new(),
    })?;
    Ok(Written {
        outcome,
        id: id.clone(),
    })
}

fn replayed(vault: &Vault, actor: &ActorRef, key: &[u8]) -> Result<Option<Written<CandidateId>>> {
    let Some((commit_id, _, receipt)) = vault.find_receipt(&scope(actor, key))? else {
        return Ok(None);
    };
    let id = receipt
        .records
        .iter()
        .find(|r| r.record_kind == RecordKind::Candidate)
        .and_then(|r| CandidateId::parse(&r.record_id).ok())
        .ok_or_else(|| enouia_memory_vault::VaultError::corrupt("receipt"))?;
    Ok(Some(Written {
        outcome: enouia_memory_contract::ports::CommitOutcome::Replayed { commit_id, receipt },
        id,
    }))
}

/// Store a pending candidate. `key` is the caller's idempotency key.
pub fn propose(
    vault: &Vault,
    proposal: &Proposal,
    origin: &Origin,
    key: &[u8],
) -> Result<Proposed> {
    if let Some(done) = replayed(vault, &origin.actor, key)? {
        return Ok(Proposed::Stored(done));
    }
    check_details(proposal.kind, proposal.proposed_type, &proposal.details)?;
    let pin = vault.pin_current()?;
    let resolved: Resolved = resolve(vault, &pin, &proposal.evidence)?;
    let sensitivity = proposal.sensitivity.unwrap_or_else(|| resolved.strictest());
    if sensitivity < resolved.strictest() {
        return Err(invalid("sensitivity.downgrade"));
    }
    check_target(vault, &pin, proposal)?;
    let dedupe = fingerprint(
        proposal.kind,
        proposal.proposed_type,
        &proposal.content,
        &proposal.details,
        &proposal.target_memory_id,
        &proposal.target_identity_id,
    );
    if let Some(existing) = pending_candidates(vault, &pin)?
        .into_iter()
        .find(|c| c.dedupe_fingerprint == dedupe)
    {
        return Ok(Proposed::DuplicateOf(existing.candidate_id));
    }
    let now = vault.now()?;
    let id = CandidateId::from_random(vault.random_id_bytes());
    let candidate = json!({
        "schema_version": 1,
        "candidate_id": id,
        "revision": 1,
        "proposal_kind": proposal.kind,
        "proposed_type": proposal.proposed_type,
        "proposed_content": proposal.content,
        "proposed_details": proposal.details,
        "evidence": resolved.evidence,
        "source_id": proposal.evidence[0].source_id,
        "reason": proposal.reason,
        "origin_kind": origin.kind,
        "origin_actor": origin.actor,
        "extraction_run_id": origin.extraction_run_id,
        "confidence": origin.confidence,
        "sensitivity": sensitivity,
        "declassification_approval_id": null,
        "status": "pending",
        "target_memory_id": proposal.target_memory_id,
        "target_identity_id": proposal.target_identity_id,
        "expected_revision": proposal.expected_revision,
        "proposed_effective_from": proposal.effective_from,
        "conflicts": conflicts(vault, &pin, proposal)?,
        "dedupe_fingerprint": dedupe,
        "reopens_candidate_id": proposal.reopens_candidate_id,
        "merged_into": null,
        "resolution_review_id": null,
        "created_at": now,
        "updated_at": now,
        "extensions": {},
    });
    // The pending fingerprint lookup must remain valid through publication.
    // A different-key writer can otherwise publish the same fingerprint
    // between this snapshot and our commit. The existing head precondition
    // rejects that stale proposal before it writes another candidate.
    commit_candidate(
        vault,
        &origin.actor,
        key,
        &candidate,
        &id,
        one(),
        Some(&pin.commit_id),
    )
    .map(Proposed::Stored)
}

/// Changes to a pending candidate; unset fields keep their value.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CandidateEdit {
    pub content: Option<String>,
    pub details: Option<Map<String, Value>>,
    pub evidence: Option<Vec<EvidenceSpec>>,
    pub reason: Option<String>,
}

/// The latest candidate revision, if it is pending at `expected` and `actor`
/// may change it (its proposer or the owner).
fn pending_at(
    vault: &Vault,
    pin: &CommitPin,
    id: &CandidateId,
    expected: Revision,
    actor: &ActorRef,
) -> Result<CandidateRecord> {
    let (candidate, revision): (CandidateRecord, _) =
        latest(vault, pin, RecordKind::Candidate, id.as_str())?
            .ok_or_else(|| invalid("candidate.missing"))?;
    if revision != expected || candidate.status != CandidateStatus::Pending {
        return Err(stale());
    }
    if &candidate.origin_actor != actor && !is_owner(vault, actor) {
        return Err(invalid("candidate.not_proposer"));
    }
    Ok(candidate)
}

/// A new pending revision with edited content, details, or evidence.
pub fn edit_candidate(
    vault: &Vault,
    id: &CandidateId,
    expected: Revision,
    edit: &CandidateEdit,
    actor: &ActorRef,
    key: &[u8],
) -> Result<Written<CandidateId>> {
    if let Some(done) = replayed(vault, actor, key)? {
        return Ok(done);
    }
    let pin = vault.pin_current()?;
    let candidate = pending_at(vault, &pin, id, expected, actor)?;
    let mut value = serde_json::to_value(&candidate).expect("candidate serializes");
    let content = edit
        .content
        .clone()
        .unwrap_or(candidate.proposed_content.clone());
    let details = edit.details.clone().or(candidate.proposed_details.clone());
    check_details(candidate.proposal_kind, candidate.proposed_type, &details)?;
    if let Some(specs) = &edit.evidence {
        let resolved = resolve(vault, &pin, specs)?;
        if candidate.sensitivity < resolved.strictest() {
            return Err(invalid("sensitivity.downgrade"));
        }
        value["evidence"] = json!(resolved.evidence);
        value["source_id"] = json!(specs[0].source_id);
    }
    if let Some(reason) = &edit.reason {
        value["reason"] = json!(reason);
    }
    let proposal = Proposal {
        kind: candidate.proposal_kind,
        proposed_type: candidate.proposed_type,
        content: content.clone(),
        details: details.clone(),
        evidence: Vec::new(),
        reason: String::new(),
        sensitivity: None,
        target_memory_id: candidate.target_memory_id.clone(),
        target_identity_id: candidate.target_identity_id.clone(),
        expected_revision: candidate.expected_revision,
        effective_from: candidate.proposed_effective_from.clone(),
        reopens_candidate_id: None,
    };
    value["proposed_content"] = json!(content);
    value["proposed_details"] = json!(details);
    value["conflicts"] = json!(conflicts(vault, &pin, &proposal)?);
    value["dedupe_fingerprint"] = json!(fingerprint(
        candidate.proposal_kind,
        candidate.proposed_type,
        &content,
        &details,
        &candidate.target_memory_id,
        &candidate.target_identity_id,
    ));
    value["revision"] = json!(next(expected).get());
    value["updated_at"] = json!(vault.now()?);
    commit_candidate(vault, actor, key, &value, id, next(expected), None)
}

/// The proposer (or the owner) withdraws a pending candidate. Withdrawn is
/// final; a later proposal is a new candidate.
pub fn withdraw(
    vault: &Vault,
    id: &CandidateId,
    expected: Revision,
    actor: &ActorRef,
    key: &[u8],
) -> Result<Written<CandidateId>> {
    if let Some(done) = replayed(vault, actor, key)? {
        return Ok(done);
    }
    let pin = vault.pin_current()?;
    let candidate = pending_at(vault, &pin, id, expected, actor)?;
    let mut value = serde_json::to_value(&candidate).expect("candidate serializes");
    value["status"] = json!("withdrawn");
    value["revision"] = json!(next(expected).get());
    value["updated_at"] = json!(vault.now()?);
    commit_candidate(vault, actor, key, &value, id, next(expected), None)
}

/// The review queue: every candidate whose latest revision is pending,
/// oldest first.
pub fn pending_candidates(vault: &Vault, pin: &CommitPin) -> Result<Vec<CandidateRecord>> {
    let mut out: Vec<CandidateRecord> = all_latest(vault, pin, RecordKind::Candidate)?
        .into_iter()
        .filter(|c: &CandidateRecord| c.status == CandidateStatus::Pending)
        .collect();
    out.sort_by(|a, b| {
        (&a.created_at, a.candidate_id.as_str()).cmp(&(&b.created_at, b.candidate_id.as_str()))
    });
    Ok(out)
}

//! Owner-selected drafts from validated saved semantic responses. This module
//! Drafting never writes; explicit publication uses ordinary governance and
//! bound receipts. Neither path sends or confirms a canonical memory.
use crate::{
    Result, error, history_review,
    history_semantic::{self, AnalysisTask, ChangeKind, ReviewedChange, SemanticPreview},
    vault::{VaultAdapter, bytes, read},
};
use enouia_memory_contract::{
    MemoryErrorCode,
    candidate::{CandidateRecord, ProposalKind, ProposedType},
    hash::{Sha256Hex, sha256},
    ids::{CommitId, DispatchId, MemoryId},
    json::Revision,
    memory::{MemoryStatus, ProvenanceState},
    ports::CommitPin,
    record::{RecordKind, RecordRef},
    scan::{contains_local_path, contains_secret_material},
    source::SourceRecord,
    time::BusinessTime,
};
use enouia_memory_govern::{Canonical, EvidenceSpec, Origin, Proposal, Proposed};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeSet;

pub const VERSION: &str = "history-semantic-proposal-1";

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DraftAction {
    Create,
    /// A correction/cancellation still needs an explicit current target and
    /// business effective time. Message time never supplies this value.
    Supersede {
        target: MemoryId,
        revision: Revision,
        effective_from: BusinessTime,
    },
}

/// Bind the owner's choice to the inspected response. Content must be supplied
/// explicitly, whether retaining or editing the model statement.
#[derive(Serialize)]
pub struct OwnerSelection {
    pub dispatch_id: DispatchId,
    pub response_hash: Sha256Hex,
    pub change_index: usize,
    pub content: String,
    pub action: DraftAction,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ProposalDraft {
    pub base: CommitPin,
    pub proposal: Proposal,
    /// OwnerManual describes the act of proposing, never the evidence class
    /// or authorship of the model explanation retained in proposal.reason.
    pub origin: Origin,
}

fn selected<'a>(
    review: &'a SemanticPreview,
    selection: &OwnerSelection,
) -> Result<&'a ReviewedChange> {
    if selection.dispatch_id != review.dispatch_id
        || selection.response_hash != review.response_hash
        || selection.content.trim().is_empty()
        || selection.content.len() > 4096
        || contains_secret_material(&selection.content)
        || contains_local_path(&selection.content)
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    review
        .changes
        .get(selection.change_index)
        .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))
}

pub fn draft(
    adapter: &VaultAdapter<'_>,
    task: &AnalysisTask,
    selection: &OwnerSelection,
) -> Result<ProposalDraft> {
    let review = history_semantic::preview(adapter, task)?;
    let change = selected(&review, selection)?;
    let (kind, mut details) = match review.topic.kind {
        history_review::TopicKind::Fact => {
            (ProposedType::Fact, json!({"claim_key":review.topic.label}))
        }
        history_review::TopicKind::Preference => {
            if !change
                .evidence
                .iter()
                .any(|c| c.evidence_class.is_user_evidence())
            {
                return Err(error(MemoryErrorCode::InvalidRequest));
            }
            (
                ProposedType::Preference,
                json!({"scope":review.topic.label,"strength":"tentative"}),
            )
        }
    };
    details["subject_ids"] = json!(review.topic.subjects);
    details["project_id"] = json!(review.topic.project);
    details["epistemic_status"] = json!(if change.kind == ChangeKind::Conflict {
        "disputed"
    } else {
        "uncertain"
    });
    let mut seen = BTreeSet::new();
    let evidence = change
        .evidence
        .iter()
        .chain(&change.earlier)
        .filter(|c| seen.insert(c.source.clone()))
        .map(|c| EvidenceSpec::content(c.source.source_id.clone(), c.source.source_revision))
        .collect();
    let mut proposal = Proposal::create(
        kind,
        &selection.content,
        details.as_object().expect("details").clone(),
        evidence,
    );
    proposal.context_source_refs = review
        .source_coverage
        .iter()
        .map(|c| c.slice.source.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    // The model saw all selected spans, including uncited context. Do not
    // downgrade sensitivity merely because its answer cites a public snippet.
    let mut strictest = enouia_memory_contract::common::Sensitivity::Public;
    for c in &review.source_coverage {
        let source: SourceRecord = read(
            adapter.vault,
            RecordKind::Source,
            c.slice.source.source_id.as_str(),
        )?;
        strictest = strictest.max(source.sensitivity);
    }
    proposal.sensitivity = Some(strictest);
    let citation = |c: &crate::extraction::ExtractionCitation| {
        json!({
            "source":c.source,"content_hash":c.content_hash,"quote":c.quote,
            "start":c.start,"end":c.end,"selected_start":c.selected_start,"selected_end":c.selected_end,
            "speaker_role":c.speaker_role,"evidence_class":c.evidence_class,
        })
    };
    // Stored as inert provenance text in the existing reason field. Original
    // Sources remain the evidence anchors; no fabricated extraction run or
    // manual-assertion Source is created for this derived interpretation.
    proposal.reason = json!({
        "version":VERSION,"selection":"owner_selected_model_interpretation",
        "task":{"session_id":task.session_id,"input_event_id":task.input_event_id,"input_hash":task.input_hash},
        "dispatch_id":review.dispatch_id,"response_hash":review.response_hash,
        "change_index":selection.change_index,"change_kind":change.kind,
        "model_statement":change.statement,"model_explanation":change.explanation,
        "owner_content_hash":sha256(selection.content.as_bytes()),
        "evidence":change.evidence.iter().map(citation).collect::<Vec<_>>(),
        "earlier":change.earlier.iter().map(citation).collect::<Vec<_>>(),
    }).to_string();
    if proposal.reason.len() > history_semantic::MAX_TASK_BYTES {
        return Err(error(MemoryErrorCode::BudgetExceeded));
    }
    let mut refs = review
        .source_coverage
        .iter()
        .map(|c| {
            RecordRef::new(
                RecordKind::Source,
                c.slice.source.source_id.as_str(),
                c.slice.source.source_revision,
            )
        })
        .collect::<Vec<_>>();
    if let DraftAction::Supersede {
        target,
        revision,
        effective_from,
    } = &selection.action
    {
        if !matches!(
            change.kind,
            ChangeKind::Correction | ChangeKind::Cancellation
        ) {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        let reference = RecordRef::new(RecordKind::Memory, target.as_str(), *revision);
        adapter
            .vault
            .read_record(&review.base, &reference)
            .map_err(|e| e.error)?;
        let memory = enouia_memory_govern::canonical_memories(
            adapter.vault,
            &review.base,
            Canonical::Active,
        )
        .map_err(|e| e.error)?
        .into_iter()
        .find(|m| &m.memory_id == target)
        .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
        if memory.revision != *revision
            || memory.status != MemoryStatus::Active
            || memory.provenance_state != ProvenanceState::Intact
            || history_review::memory_topic(&memory)? != review.topic
            || memory.content.trim() == selection.content.trim()
        {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        proposal.kind = ProposalKind::Supersede;
        proposal.target_memory_id = Some(target.clone());
        proposal.expected_revision = Some(*revision);
        proposal.effective_from = Some(effective_from.clone());
        refs.push(reference);
    }
    history_review::fresh(adapter.vault, &review.base, &refs, adapter.enabled)?;
    Ok(ProposalDraft {
        base: review.base,
        proposal,
        origin: Origin::owner(adapter.owner.clone()),
    })
}

/// Explicitly publish one owner choice, never a batch or a canonical memory.
/// The host must serialize this with lock/close lifecycle operations. A replay
/// revalidates the saved analysis and current candidate before returning its
/// original receipt, including after owner edits/review or supersession.
pub fn publish(
    adapter: &VaultAdapter<'_>,
    task: &AnalysisTask,
    selection: &OwnerSelection,
    required_head: &CommitId,
    key: &[u8],
) -> Result<Proposed> {
    if key.is_empty() || key.len() > 256 {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let review = history_semantic::preview(adapter, task)?;
    selected(&review, selection)?;
    let payload = sha256(&bytes(&json!({"version":"history-semantic-publication-1",
        "task":{"session_id":task.session_id,"input_event_id":task.input_event_id,"input_hash":task.input_hash},
        "choice":selection}))?);
    // Keep the caller operation key global within this feature: reusing it
    // for another task/choice conflicts rather than silently creating a job.
    let key = format!("history-semantic-publish-1:{}", sha256(key));
    if let Some(done) = enouia_memory_govern::replay_bound_proposal(
        adapter.vault,
        &adapter.owner,
        key.as_bytes(),
        &payload,
    )
    .map_err(|e| e.error)?
    {
        let candidate: CandidateRecord =
            read(adapter.vault, RecordKind::Candidate, done.id.as_str())?;
        let mut refs = review
            .source_coverage
            .iter()
            .map(|c| {
                RecordRef::new(
                    RecordKind::Source,
                    c.slice.source.source_id.as_str(),
                    c.slice.source.source_revision,
                )
            })
            .collect::<Vec<_>>();
        refs.push(RecordRef::new(
            RecordKind::Candidate,
            done.id.as_str(),
            candidate.revision,
        ));
        history_review::fresh(adapter.vault, &review.base, &refs, adapter.enabled)?;
        return Ok(Proposed::Stored(done));
    }
    let draft = draft(adapter, task, selection)?;
    history_review::owner(adapter.vault, &adapter.owner, adapter.enabled)?;
    enouia_memory_govern::propose_bound_at_head(
        adapter.vault,
        &draft.proposal,
        &draft.origin,
        key.as_bytes(),
        &payload,
        required_head,
    )
    .map_err(|e| e.error)
}

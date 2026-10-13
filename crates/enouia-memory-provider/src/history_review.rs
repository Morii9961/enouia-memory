//! Read-only comparisons of source-bound suggestions. Hints are not facts,
//! approval, or permission to supersede. Explicit drafts use ordinary governance.
use crate::{
    Result, error, extraction,
    vault::{Quota, VaultAdapter},
};
use enouia_memory_contract::{
    MemoryErrorCode,
    candidate::{CandidateStatus, ProposalKind, ProposedType},
    common::{ActorRef, ActorType},
    extraction::{ExtractionSource, ExtractionState},
    hash::{Sha256Hex, sha256},
    ids::{CandidateId, ExtractionRunId, MemoryId, ProjectId, SessionId, SourceId, SubjectId},
    json::{Knowable, Revision},
    memory::{CanonicalMemory, MemoryBody, MemoryStatus, ProvenanceState},
    ports::CommitPin,
    record::{AnyRecord, RecordKind, RecordRef},
    source::SourceRecord,
    time::BusinessTime,
};
use enouia_memory_govern::{EvidenceSpec, Origin, Proposal};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::atomic::{AtomicBool, Ordering},
};

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub enum TopicKind {
    Fact,
    Preference,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TopicKey {
    pub kind: TopicKind,
    pub subjects: Vec<SubjectId>,
    pub project: Option<ProjectId>,
    pub label: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RunSelection {
    pub session_id: SessionId,
    pub run_id: ExtractionRunId,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub enum ReviewFlag {
    UnknownTime,
    NonUserEvidence,
    OwnerEdited,
    ReviewedOrWithdrawn,
    Suppressed,
}
#[derive(Clone, Debug, PartialEq)]
pub struct ReviewEntry {
    pub selection: RunSelection,
    pub item: extraction::ExtractionPreviewItem,
    pub flags: Vec<ReviewFlag>,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum Difference {
    ExactRepeat,
    DatedDifference,
    UnorderedDifference,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub enum BranchRelation {
    SamePath,
    Divergent,
    SeparateContexts,
    Unknown,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct Comparison {
    pub left: usize,
    pub right: usize,
    pub difference: Difference,
    pub branches: BranchRelation,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TopicReview {
    pub key: TopicKey,
    pub entries: Vec<ReviewEntry>,
    pub comparisons: Vec<Comparison>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct HistoryReview {
    pub base: CommitPin,
    pub runs: Vec<RunCoverage>,
    pub topics: Vec<TopicReview>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SourceCoverage {
    pub slice: ExtractionSource,
    /// Exact selected text and source metadata, including uncited slices.
    pub context: serde_json::Value,
    pub suggestion_indices: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct RunCoverage {
    pub selection: RunSelection,
    pub response_hash: Sha256Hex,
    pub state: ExtractionState,
    pub applied: u64,
    pub suggestions: usize,
    pub sources: Vec<SourceCoverage>,
}

pub(crate) fn owner(
    vault: &enouia_memory_vault::Vault,
    actor: &ActorRef,
    enabled: &AtomicBool,
) -> Result<()> {
    if !enabled.load(Ordering::SeqCst) {
        return Err(error(MemoryErrorCode::VaultLocked));
    }
    if actor.actor_type != ActorType::Owner || actor != &vault.descriptor().created_by {
        return Err(error(MemoryErrorCode::PermissionDenied));
    }
    Ok(())
}
pub(crate) fn fresh(
    vault: &enouia_memory_vault::Vault,
    pin: &CommitPin,
    refs: &[RecordRef],
    enabled: &AtomicBool,
) -> Result<()> {
    let current = vault.check_fresh(pin, refs).map_err(|e| e.error)?;
    if current.head.commit_id != pin.commit_id {
        return Err(error(MemoryErrorCode::RevisionConflict));
    }
    if !enabled.load(Ordering::SeqCst) {
        return Err(error(MemoryErrorCode::VaultLocked));
    }
    Ok(())
}
fn topic(
    kind: ProposedType,
    details: &serde_json::Map<String, serde_json::Value>,
) -> Result<TopicKey> {
    // A proposed new entity has no stable project identity to compare yet.
    if details.get("new_project").is_some_and(|v| !v.is_null()) {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let (kind, field) = match kind {
        ProposedType::Fact => (TopicKind::Fact, "claim_key"),
        ProposedType::Preference => (TopicKind::Preference, "scope"),
        _ => return Err(error(MemoryErrorCode::InvalidRequest)),
    };
    let label = details
        .get(field)
        .and_then(|v| v.as_str())
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?
        .to_owned();
    let mut subjects: Vec<SubjectId> = serde_json::from_value(
        details
            .get("subject_ids")
            .cloned()
            .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?,
    )
    .map_err(|_| error(MemoryErrorCode::InvalidRequest))?;
    if subjects.is_empty() {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    subjects.sort();
    subjects.dedup();
    let project = details
        .get("project_id")
        .filter(|v| !v.is_null())
        .map(|v| serde_json::from_value(v.clone()))
        .transpose()
        .map_err(|_| error(MemoryErrorCode::InvalidRequest))?;
    Ok(TopicKey {
        kind,
        subjects,
        project,
        label,
    })
}
pub(crate) fn memory_topic(memory: &CanonicalMemory) -> Result<TopicKey> {
    let (kind, label) = match &memory.body {
        MemoryBody::Fact(f) => (TopicKind::Fact, f.claim_key.clone()),
        MemoryBody::Preference(p) => (TopicKind::Preference, p.scope.clone()),
        _ => return Err(error(MemoryErrorCode::InvalidRequest)),
    };
    let mut subjects = memory.subject_ids.clone();
    subjects.sort();
    subjects.dedup();
    Ok(TopicKey {
        kind,
        subjects,
        project: memory.project_id.clone(),
        label,
    })
}
pub(crate) fn ancestor(
    from: &SourceId,
    to: &SourceId,
    sources: &BTreeMap<SourceId, SourceRecord>,
) -> bool {
    let mut pending = vec![to.clone()];
    let mut seen = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if &id == from {
            return true;
        }
        if !seen.insert(id.clone()) {
            continue;
        }
        if let Some(source) = sources.get(&id)
            && let Knowable::Known(parents) = &source.parent_source_ids
        {
            pending.extend(parents.iter().cloned());
        }
    }
    false
}
fn branches(
    a: &SourceRecord,
    b: &SourceRecord,
    sources: &BTreeMap<SourceId, SourceRecord>,
) -> BranchRelation {
    if ancestor(&a.source_id, &b.source_id, sources)
        || ancestor(&b.source_id, &a.source_id, sources)
    {
        return BranchRelation::SamePath;
    }
    if a.original_conversation_id.is_some()
        && b.original_conversation_id.is_some()
        && (a.provider != b.provider
            || a.account_scope != b.account_scope
            || a.original_conversation_id != b.original_conversation_id)
    {
        return BranchRelation::SeparateContexts;
    }
    if a.raw_object_hash.is_some()
        && a.raw_object_hash == b.raw_object_hash
        && a.original_conversation_id.is_some()
        && a.original_conversation_id == b.original_conversation_id
        && a.provider == b.provider
        && a.account_scope == b.account_scope
        && let (Knowable::Known(Some(x)), Knowable::Known(Some(y))) = (&a.branch_id, &b.branch_id)
    {
        return if x == y {
            BranchRelation::SamePath
        } else {
            BranchRelation::Divergent
        };
    }
    BranchRelation::Unknown
}

/// Group original suggestions, retaining current owner edits/status separately.
/// No calls/writes, and no preferred/current statement is selected.
pub fn preview(
    vault: &enouia_memory_vault::Vault,
    actor: &ActorRef,
    selected: &[RunSelection],
    enabled: &AtomicBool,
) -> Result<HistoryReview> {
    owner(vault, actor, enabled)?;
    if selected.is_empty()
        || selected.len() > 32
        || selected.iter().collect::<BTreeSet<_>>().len() != selected.len()
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let pin = vault.pin_current().map_err(|e| e.error)?;
    let mut groups = BTreeMap::<TopicKey, Vec<ReviewEntry>>::new();
    let mut runs = vec![];
    let mut sources = BTreeMap::new();
    let mut refs = vec![];
    let mut rows = 0;
    for selection in selected {
        let job = extraction::job(vault, actor, &selection.session_id, &selection.run_id)?;
        let adapter = VaultAdapter {
            vault,
            owner: actor.clone(),
            input_event: job.input_event_id.clone(),
            enabled,
            quota: Quota::default(),
        };
        let review = extraction::preview(&adapter, &selection.run_id)?;
        rows += review.items.len();
        if rows > 256 {
            return Err(error(MemoryErrorCode::BudgetExceeded));
        }
        let slices = job
            .sources
            .iter()
            .map(|s| extraction::SourceSlice {
                source: s.source.clone(),
                start: s.start,
                end: s.end,
            })
            .collect::<Vec<_>>();
        let (resolved, contexts, _) = extraction::sources(vault, actor, &slices)?;
        if resolved != job.sources {
            return Err(error(MemoryErrorCode::BrokenProvenance));
        }
        runs.push(RunCoverage {
            selection: selection.clone(),
            response_hash: review.response_hash.clone(),
            state: review.job.state,
            applied: review.job.cursor,
            suggestions: review.items.len(),
            sources: resolved
                .into_iter()
                .zip(contexts)
                .map(|(slice, context)| {
                    let suggestion_indices = review
                        .items
                        .iter()
                        .filter(|item| {
                            item.citation.source == slice.source
                                && item.citation.selected_start == slice.start
                                && item.citation.selected_end == slice.end
                        })
                        .map(|item| item.response_index)
                        .collect();
                    SourceCoverage {
                        slice,
                        context,
                        suggestion_indices,
                    }
                })
                .collect(),
        });
        for source in &job.sources {
            let reference = RecordRef::new(
                RecordKind::Source,
                source.source.source_id.as_str(),
                source.source.source_revision,
            );
            let AnyRecord::Source(record) =
                vault.read_parsed(&pin, &reference).map_err(|e| e.error)?
            else {
                return Err(error(MemoryErrorCode::BrokenProvenance));
            };
            if sources.insert(record.source_id.clone(), record).is_none() && sources.len() > 512 {
                return Err(error(MemoryErrorCode::BudgetExceeded));
            }
            refs.push(reference);
        }
        for item in review.items {
            let key = topic(
                item.proposal.proposed_type,
                item.proposal
                    .details
                    .as_ref()
                    .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?,
            )?;
            let mut flags = vec![];
            if item.citation.occurred_at.is_none() {
                flags.push(ReviewFlag::UnknownTime);
            }
            if !item.citation.evidence_class.is_user_evidence() {
                flags.push(ReviewFlag::NonUserEvidence);
            }
            if item.application == extraction::ExtractionApplication::Suppressed {
                flags.push(ReviewFlag::Suppressed);
            }
            if let Some(candidate) = &item.current_candidate {
                refs.push(RecordRef::new(
                    RecordKind::Candidate,
                    candidate.candidate_id.as_str(),
                    candidate.revision,
                ));
                if candidate.proposed_content != item.proposal.content
                    || candidate.proposed_details != item.proposal.details
                {
                    flags.push(ReviewFlag::OwnerEdited);
                }
                if candidate.status != CandidateStatus::Pending {
                    flags.push(ReviewFlag::ReviewedOrWithdrawn);
                }
            }
            groups.entry(key).or_default().push(ReviewEntry {
                selection: selection.clone(),
                item,
                flags,
            });
        }
    }
    let mut topics = vec![];
    let mut comparisons = 0;
    for (key, mut entries) in groups {
        entries.sort_by(|a, b| {
            a.item
                .citation
                .occurred_at
                .is_none()
                .cmp(&b.item.citation.occurred_at.is_none())
                .then_with(|| {
                    a.item
                        .citation
                        .occurred_at
                        .cmp(&b.item.citation.occurred_at)
                })
                .then_with(|| a.selection.cmp(&b.selection))
                .then_with(|| a.item.response_index.cmp(&b.item.response_index))
        });
        let mut pairs = vec![];
        for left in 0..entries.len() {
            for right in left + 1..entries.len() {
                comparisons += 1;
                if comparisons > 4096 {
                    return Err(error(MemoryErrorCode::BudgetExceeded));
                }
                let (a, b) = (&entries[left].item, &entries[right].item);
                let difference = if a.proposal.content.trim() == b.proposal.content.trim()
                    && a.proposal.details == b.proposal.details
                {
                    Difference::ExactRepeat
                } else if a.citation.occurred_at.is_some()
                    && b.citation.occurred_at.is_some()
                    && a.citation.occurred_at != b.citation.occurred_at
                {
                    Difference::DatedDifference
                } else {
                    Difference::UnorderedDifference
                };
                pairs.push(Comparison {
                    left,
                    right,
                    difference,
                    branches: branches(
                        &sources[&a.citation.source.source_id],
                        &sources[&b.citation.source.source_id],
                        &sources,
                    ),
                });
            }
        }
        topics.push(TopicReview {
            key,
            entries,
            comparisons: pairs,
        });
    }
    fresh(vault, &pin, &refs, enabled)?;
    Ok(HistoryReview {
        base: pin,
        runs,
        topics,
    })
}

pub const DEFAULT_BRIEF_BYTES: usize = 128 * 1024;
pub const MAX_BRIEF_BYTES: usize = 2 * 1024 * 1024;
#[derive(Clone, Debug)]
pub struct BriefOptions {
    pub topic: Option<TopicKey>,
    pub max_bytes: usize,
}
impl Default for BriefOptions {
    fn default() -> Self {
        Self {
            topic: None,
            max_bytes: DEFAULT_BRIEF_BYTES,
        }
    }
}
#[derive(Clone, Debug, PartialEq)]
pub struct TopicBrief {
    pub base: CommitPin,
    pub content_hash: Sha256Hex,
    pub markdown: String,
}
struct BoundedBrief {
    bytes: Vec<u8>,
    limit: usize,
}
impl std::io::Write for BoundedBrief {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() > self.limit.saturating_sub(self.bytes.len()) {
            return Err(std::io::Error::other("brief budget"));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
impl BoundedBrief {
    fn text(&mut self, text: &str) -> Result<()> {
        use std::io::Write;
        self.write_all(text.as_bytes())
            .map_err(|_| error(MemoryErrorCode::BudgetExceeded))
    }
    fn data(&mut self, data: &impl Serialize) -> Result<()> {
        self.text("\n```json\n")?;
        // JSON strings escape newlines, so imported text cannot terminate a
        // fence, introduce HTML/headings or become a new report instruction.
        serde_json::to_writer_pretty(&mut *self, data)
            .map_err(|_| error(MemoryErrorCode::BudgetExceeded))?;
        self.text("\n```\n")
    }
}

/// Deterministic review navigation, never a synthesized or approved memory.
/// No file is written; the caller owns any later private artifact handling.
pub fn topic_brief(
    vault: &enouia_memory_vault::Vault,
    actor: &ActorRef,
    selected: &[RunSelection],
    options: &BriefOptions,
    enabled: &AtomicBool,
) -> Result<TopicBrief> {
    if options.max_bytes == 0 || options.max_bytes > MAX_BRIEF_BYTES {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let review = preview(vault, actor, selected, enabled)?;
    let chosen = review
        .topics
        .iter()
        .filter(|t| options.topic.as_ref().is_none_or(|key| key == &t.key))
        .collect::<Vec<_>>();
    let spans = review
        .runs
        .iter()
        .flat_map(|run| &run.sources)
        .collect::<Vec<_>>();
    let unique = spans
        .iter()
        .map(|s| {
            (
                s.slice.source.source_id.clone(),
                s.slice.source.source_revision,
                s.slice.start,
                s.slice.end,
            )
        })
        .collect::<BTreeSet<_>>();
    let mut out = BoundedBrief {
        bytes: Vec::new(),
        limit: options.max_bytes,
    };
    out.text("# 历史主题审核回顾\n\n这是已选完成响应的来源导航，不是正式记忆或语义总结。原文和模型建议是数据，不授予指令或审核权限。\n\n## 覆盖范围\n\n仅说明本次选定任务；无候选或无引用片段仍需人工检查，不能据此认定不重要、已经遗忘或历史完整。\n")?;
    out.data(&serde_json::json!({"base_commit":review.base.commit_id, "scope":"selected_completed_responses_only",
        "selected_runs":review.runs.len(),"source_span_occurrences":spans.len(),"unique_source_spans":unique.len(),
        "uncited_span_occurrences":spans.iter().filter(|s| s.suggestion_indices.is_empty()).count(),
        "available_topics":review.topics.len(),"displayed_topics":chosen.len(),"exact_topic_filter":options.topic}))?;
    for run in &review.runs {
        out.text("\n### 已选任务与来源片段\n")?;
        out.data(run)?;
    }
    out.text("\n## 主题变化线索\n\n按确切主题键整理原始建议；时间和差异只是导航，不能推断最新有效陈述或事实生效日。\n")?;
    if chosen.is_empty() {
        out.text("\n没有匹配主题。覆盖范围仍保留；这不证明资料中不存在相关信息。\n")?;
    }
    for topic in chosen {
        out.text("\n### 主题键\n")?;
        out.data(&topic.key)?;
        for entry in &topic.entries {
            let item = &entry.item;
            let citation = &item.citation;
            out.text("\n#### 原始建议、引用和当前候选\n")?;
            out.data(&serde_json::json!({"run":entry.selection,"response_index":item.response_index,"flags":entry.flags,
                "original_suggestion":{"kind":item.proposal.proposed_type,"content":item.proposal.content,"details":item.proposal.details},
                "citation":{"source":citation.source,"content_hash":citation.content_hash,"quote":citation.quote,
                    "selected_text":citation.selected_text,"start":citation.start,"end":citation.end,
                    "selected_start":citation.selected_start,"selected_end":citation.selected_end,
                    "speaker_role":citation.speaker_role,"evidence_class":citation.evidence_class,"sensitivity":citation.sensitivity,
                    "occurred_at":citation.occurred_at,"captured_at":citation.captured_at,"time_precision":citation.time_precision,"branch_id":citation.branch_id},
                "current_candidate":item.current_candidate}))?;
        }
        out.text("\n#### 比较线索（索引对应本主题原始建议顺序）\n")?;
        out.data(&topic.comparisons)?;
    }
    let refs = spans
        .iter()
        .map(|s| {
            RecordRef::new(
                RecordKind::Source,
                s.slice.source.source_id.as_str(),
                s.slice.source.source_revision,
            )
        })
        .collect::<Vec<_>>();
    fresh(vault, &review.base, &refs, enabled)?;
    let markdown = String::from_utf8(out.bytes).expect("UTF-8 serialization");
    Ok(TopicBrief {
        base: review.base,
        content_hash: sha256(markdown.as_bytes()),
        markdown,
    })
}

#[derive(Clone, Debug, PartialEq)]
pub struct SupersessionDraft {
    pub base: CommitPin,
    pub proposal: Proposal,
    pub origin: Origin,
}

/// Owner supplies the target AND effective time; nothing is inferred from
/// source occurrence/capture. The draft still requires propose + plan/confirm.
#[allow(clippy::too_many_arguments)]
pub fn supersession_draft(
    vault: &enouia_memory_vault::Vault,
    actor: &ActorRef,
    candidate: &CandidateId,
    expected_candidate: Revision,
    target: &MemoryId,
    expected_target: Revision,
    effective_from: BusinessTime,
    enabled: &AtomicBool,
) -> Result<SupersessionDraft> {
    owner(vault, actor, enabled)?;
    let pin = vault.pin_current().map_err(|e| e.error)?;
    let candidate_ref = RecordRef::new(
        RecordKind::Candidate,
        candidate.as_str(),
        expected_candidate,
    );
    let target_ref = RecordRef::new(RecordKind::Memory, target.as_str(), expected_target);
    // read_parsed also serves historical revisions; drafts require latest.
    vault
        .read_record(&pin, &candidate_ref)
        .map_err(|e| e.error)?;
    vault.read_record(&pin, &target_ref).map_err(|e| e.error)?;
    let AnyRecord::Candidate(c) = vault
        .read_parsed(&pin, &candidate_ref)
        .map_err(|e| e.error)?
    else {
        return Err(error(MemoryErrorCode::InvalidRequest));
    };
    let AnyRecord::Memory(m) = vault.read_parsed(&pin, &target_ref).map_err(|e| e.error)? else {
        return Err(error(MemoryErrorCode::InvalidRequest));
    };
    if !enouia_memory_govern::canonical_memories(
        vault,
        &pin,
        enouia_memory_govern::Canonical::Active,
    )
    .map_err(|e| e.error)?
    .iter()
    .any(|visible| visible.memory_id == m.memory_id)
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    if c.status != CandidateStatus::Pending
        || c.proposal_kind != ProposalKind::Create
        || m.status != MemoryStatus::Active
        || m.provenance_state != ProvenanceState::Intact
        || topic(
            c.proposed_type,
            c.proposed_details
                .as_ref()
                .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?,
        )? != memory_topic(&m)?
        || c.proposed_content.trim() == m.content.trim()
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let mut refs = vec![candidate_ref, target_ref];
    let mut evidence = vec![];
    for e in &c.evidence {
        let reference = RecordRef::new(RecordKind::Source, e.source_id.as_str(), e.source_revision);
        vault.read_record(&pin, &reference).map_err(|v| v.error)?;
        refs.push(reference);
        evidence.push(EvidenceSpec {
            source_id: e.source_id.clone(),
            source_revision: e.source_revision,
            supports: e.supports.clone(),
            locator: Some(e.locator.clone()),
        });
    }
    let mut proposal = Proposal::create(
        c.proposed_type,
        &c.proposed_content,
        c.proposed_details
            .clone()
            .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?,
        evidence,
    );
    proposal.kind = ProposalKind::Supersede;
    proposal.target_memory_id = Some(m.memory_id);
    proposal.expected_revision = Some(m.revision);
    proposal.effective_from = Some(effective_from);
    proposal.reason = "owner_selected_history_supersession".into();
    proposal.sensitivity = Some(c.sensitivity);
    let origin = Origin {
        kind: c.origin_kind,
        actor: c.origin_actor,
        extraction_run_id: c.extraction_run_id,
        confidence: c.confidence,
    };
    fresh(vault, &pin, &refs, enabled)?;
    Ok(SupersessionDraft {
        base: pin,
        proposal,
        origin,
    })
}

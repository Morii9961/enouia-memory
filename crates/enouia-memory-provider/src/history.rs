//! Owner-selected archived history -> existing durable extraction jobs.
//! No transport/secret ports, automatic sending, approval or private side DB.
//! The coordinator is source-linked Session input; progress is reconstructed
//! from ordinary child jobs, not from caller-supplied cursor claims.

use crate::{
    Result,
    codec::Api,
    error,
    extraction::{self, ExtractionOptions, SourceSlice},
    vault::bytes,
};
use enouia_memory_context::session;
use enouia_memory_contract::{
    MemoryErrorCode,
    common::{ActorRef, ActorType, Sensitivity, SourceRevisionRef},
    extraction::{ExtractionSource, ExtractionState},
    foundation::Cancellation,
    hash::{Sha256Hex, sha256},
    ids::{EventId, ExtractionRunId, ImportId, PolicyId, RequestId, SessionId, SubjectId},
    import::{ImportManifest, ImportStatus},
    ports::CommitPin,
    provider::InvocationState,
    record::{AnyRecord, RecordKind, RecordRef},
    session::{ClientSurface, SessionRecord},
    source::SourceRecord,
};
use enouia_memory_import::{
    detect::{Detection, detect},
    history::{HistoryLimits, HistoryPlanError, MessageRef, plan_history},
    locate::resolve,
    zip::ZipLimits,
};
use enouia_memory_vault::{Vault, service::SessionStart};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};

pub const PLAN_VERSION: &str = "history-extraction-plan-1";
pub const MAX_BATCHES: usize = 32;
pub const MAX_PLAN_BYTES: usize = 128 * 1024;

pub struct HistoryOptions {
    pub import_id: ImportId,
    pub local_policy: PolicyId,
    pub history_limits: HistoryLimits,
    /// Explicit indices in the H1 plan; their order is retained in the receipt.
    pub batch_indices: Vec<usize>,
    pub api: Api,
    pub model: String,
    pub subject_id: SubjectId,
    /// Bounds apply to each child. Host quotas still govern actual sends.
    pub max_candidates: u64,
    pub max_pending: u64,
    pub max_reserved_tokens: u64,
    pub max_reserved_cost_microusd: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryBatchProgress {
    pub batch_index: usize,
    pub session_id: SessionId,
    pub run_id: ExtractionRunId,
    pub input_event_id: EventId,
    pub state: ExtractionState,
    pub applied: u64,
    pub invocation_state: Option<InvocationState>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HistoryProgress {
    pub coordinator_session_id: SessionId,
    pub coordinator_input_id: EventId,
    pub input_object_hash: Sha256Hex,
    pub plan_hash: Sha256Hex,
    pub batches: Vec<HistoryBatchProgress>,
    /// First unfinished selected batch; never permission to retry an attempt.
    pub next_batch: Option<usize>,
}

struct BoundBatch {
    index: usize,
    sources: Vec<ExtractionSource>,
}

fn read_at(vault: &Vault, pin: &CommitPin, kind: RecordKind, id: &str) -> Result<AnyRecord> {
    let entry = vault
        .record_entry(pin, kind, id)
        .map_err(|e| e.error)?
        .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
    vault
        .read_parsed(pin, &RecordRef::new(kind, id, entry.revision))
        .map_err(|e| e.error)
}

fn slices(batch: &BoundBatch) -> Vec<SourceSlice> {
    batch
        .sources
        .iter()
        .map(|s| SourceSlice {
            source: s.source.clone(),
            start: s.start,
            end: s.end,
        })
        .collect()
}

/// Materialize offline. Retain the original key/options to resume after a
/// cancellation/crash; a changed plan under that key is refused. Existing jobs
/// retain their own state, cursor and failed/unknown invocation ledger.
pub fn materialize(
    vault: &Vault,
    actor: &ActorRef,
    options: &HistoryOptions,
    key: &[u8],
    cancel: &dyn Cancellation,
) -> Result<HistoryProgress> {
    if actor.actor_type != ActorType::Owner || actor != &vault.descriptor().created_by {
        return Err(error(MemoryErrorCode::PermissionDenied));
    }
    if key.is_empty()
        || key.len() > 256
        || options.batch_indices.is_empty()
        || options.batch_indices.len() > MAX_BATCHES
        || options.batch_indices.iter().collect::<BTreeSet<_>>().len()
            != options.batch_indices.len()
        || !(1..=15).contains(&options.history_limits.messages_per_batch)
        || !(1..=32).contains(&options.max_candidates)
        || !(1..=500).contains(&options.max_pending)
        || options.max_reserved_tokens == 0
        || options.max_reserved_tokens > 9_007_199_254_740_991
        || options
            .max_reserved_cost_microusd
            .is_some_and(|n| n > 9_007_199_254_740_991)
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let binding = options.api.binding(&options.model)?;
    if cancel.is_cancelled() {
        return Err(error(MemoryErrorCode::Cancelled));
    }
    let pin = vault.pin_current().map_err(|e| e.error)?;
    let AnyRecord::Import(manifest) =
        read_at(vault, &pin, RecordKind::Import, options.import_id.as_str())?
    else {
        return Err(error(MemoryErrorCode::InvalidRequest));
    };
    let raw = archived(vault, &pin, &manifest)?;
    let Detection::Parsed(input) = detect(&raw, &ZipLimits::default()) else {
        return Err(error(MemoryErrorCode::UnsupportedSchema));
    };
    if manifest.adapter.as_ref() != Some(&input.adapter)
        || manifest.source_schema_observed.as_ref() != Some(&input.source_schema_observed)
        || manifest.input_kind != input.input_kind
    {
        return Err(error(MemoryErrorCode::UnsupportedSchema));
    }
    let plan = plan_history(&input, options.history_limits).map_err(|e| {
        error(match e {
            HistoryPlanError::InvalidLimits => MemoryErrorCode::InvalidRequest,
            HistoryPlanError::ReferenceBudgetExceeded => MemoryErrorCode::BudgetExceeded,
        })
    })?;
    if options
        .batch_indices
        .iter()
        .any(|&i| i >= plan.batches.len())
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let mut records = BTreeMap::<Vec<u8>, SourceRecord>::new();
    let purged = vault.purged().map_err(|e| e.error)?;
    for entry in vault
        .record_entries(&pin, RecordKind::Source)
        .map_err(|e| e.error)?
    {
        // Skip known deletions only. Missing selected sources still fail
        // binding below; storage corruption is never silently ignored.
        if purged.record(RecordKind::Source, &entry.record_id, entry.revision.get()) {
            continue;
        }
        let AnyRecord::Source(source) = read_at(vault, &pin, RecordKind::Source, &entry.record_id)?
        else {
            return Err(error(MemoryErrorCode::BrokenProvenance));
        };
        if source.import_id.as_ref() == Some(&options.import_id)
            && source.raw_object_hash.as_ref() == Some(&manifest.input_object_hash)
            && records.insert(bytes(&source.locator)?, source).is_some()
        {
            return Err(error(MemoryErrorCode::BrokenProvenance));
        }
    }
    let mut batches = Vec::new();
    let mut dependencies = BTreeSet::new();
    let mut sensitivity = Sensitivity::Public;
    for &index in &options.batch_indices {
        let batch = &plan.batches[index];
        let selected: Vec<MessageRef> = batch
            .preceding
            .iter()
            .copied()
            .chain(batch.messages.iter().copied())
            .collect();
        let mut selections = Vec::new();
        for r in selected {
            let message = &input.units[r.unit].messages[r.message];
            let record = records
                .get(&bytes(&message.locator)?)
                .ok_or_else(|| error(MemoryErrorCode::BrokenProvenance))?;
            if record.content_hash != message.content_hash
                || record.speaker_role != message.role
                || record.evidence_class != message.evidence
                || record.occurred_at != message.occurred_at
                || record.time_precision != message.time_precision
                || record.completeness != message.completeness
                || record.original_time != message.original_time
                || record.original_message_id != message.upstream_id
                || record.original_conversation_id != input.units[r.unit].conversation_id
            {
                return Err(error(MemoryErrorCode::BrokenProvenance));
            }
            let text = resolve(&raw, &message.locator)
                .map_err(|_| error(MemoryErrorCode::BrokenProvenance))?;
            if text.is_empty() || text.len() > 32_768 {
                return Err(error(MemoryErrorCode::BudgetExceeded));
            }
            selections.push(SourceSlice {
                source: SourceRevisionRef {
                    source_id: record.source_id.clone(),
                    source_revision: record.revision,
                },
                start: 0,
                end: text.len() as u64,
            });
        }
        // Reuse current ACL/deletion/freshness/secret and byte-budget checks.
        let (sources, _, level) = extraction::sources(vault, actor, &selections)?;
        dependencies.extend(sources.iter().map(|s| s.source.clone()));
        sensitivity = sensitivity.max(level);
        batches.push(BoundBatch { index, sources });
    }
    let metadata = bytes(&json!({
        "task": PLAN_VERSION, "planner_version": "history-plan-1",
        "import_id": options.import_id, "input_object_hash": manifest.input_object_hash,
        "adapter": input.adapter, "source_schema_observed": input.source_schema_observed,
        "history_limits": {"messages_per_batch":options.history_limits.messages_per_batch,
            "max_path_references":options.history_limits.max_path_references},
        "binding":binding, "subject_id":options.subject_id,
        "max_candidates":options.max_candidates, "max_pending":options.max_pending,
        "max_reserved_tokens":options.max_reserved_tokens,
        "max_reserved_cost_microusd":options.max_reserved_cost_microusd,
        "coverage": {"dated":plan.chronological.len(), "undated":plan.undated.len(),
            "unparseable":plan.unparseable, "incomplete_messages":plan.incomplete_messages,
            "unavailable_attachments":plan.unavailable_attachments},
        "batches":batches.iter().map(|b| json!({"index":b.index,"sources":b.sources,
            "selected_in_export":plan.batches[b.index].selected_in_export})).collect::<Vec<_>>()
    }))?;
    if metadata.len() > MAX_PLAN_BYTES {
        return Err(error(MemoryErrorCode::BudgetExceeded));
    }
    let plan_hash = sha256(&metadata);
    let text = String::from_utf8(metadata).map_err(|_| error(MemoryErrorCode::InvalidRequest))?;
    if cancel.is_cancelled() {
        return Err(error(MemoryErrorCode::Cancelled));
    }
    let (sid, bid) = session::start(
        vault,
        &SessionStart {
            owner: actor.clone(),
            surface: ClientSurface::LocalCli,
            sensitivity,
            policy_id: options.local_policy.clone(),
        },
        format!("history-session:{}", sha256(key)).as_bytes(),
    )
    .map_err(|e| e.error)?
    .id;
    let request = RequestId::from_random(extraction::seed(key));
    let event = session::save_input_with_sources(
        vault,
        actor,
        &sid,
        &bid,
        &text,
        &request,
        format!("history-input:{}", sha256(key)).as_bytes(),
        &dependencies.into_iter().collect::<Vec<_>>(),
    )
    .map_err(|e| e.error)?
    .id;
    let mut progress = Vec::new();
    for batch in batches {
        if cancel.is_cancelled() {
            return Err(error(MemoryErrorCode::Cancelled));
        }
        let selection = slices(&batch);
        if extraction::sources(vault, actor, &selection)?.0 != batch.sources {
            return Err(error(MemoryErrorCode::BrokenProvenance));
        }
        let child_options = ExtractionOptions {
            api: options.api,
            model: options.model.clone(),
            subject_id: options.subject_id.clone(),
            sources: selection,
            max_candidates: options.max_candidates,
            max_pending: options.max_pending,
            max_reserved_tokens: options.max_reserved_tokens,
            max_reserved_cost_microusd: options.max_reserved_cost_microusd,
        };
        let child_key = format!("history-child:{}:{plan_hash}:{}", sha256(key), batch.index);
        let (child, run) = extraction::create(
            vault,
            actor,
            &options.local_policy,
            &child_options,
            child_key.as_bytes(),
        )?;
        let job = extraction::job(vault, actor, &child, &run)?;
        let header: SessionRecord = crate::vault::read(vault, RecordKind::Session, child.as_str())?;
        progress.push(HistoryBatchProgress {
            batch_index: batch.index,
            session_id: child,
            run_id: run,
            input_event_id: job.input_event_id.clone(),
            state: job.state,
            applied: job.cursor,
            invocation_state: header
                .provider_invocations
                .iter()
                .find(|i| i.input_event_id == job.input_event_id)
                .map(|i| i.state),
        });
    }
    let next_batch = progress
        .iter()
        .find(|b| b.state != ExtractionState::Completed)
        .map(|b| b.batch_index);
    Ok(HistoryProgress {
        coordinator_session_id: sid,
        coordinator_input_id: event,
        input_object_hash: manifest.input_object_hash,
        plan_hash,
        batches: progress,
        next_batch,
    })
}

fn archived(vault: &Vault, pin: &CommitPin, manifest: &ImportManifest) -> Result<Vec<u8>> {
    if manifest.status != ImportStatus::Completed || manifest.duplicate_of.is_some() {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let raw = vault
        .read_object_bounded(
            pin,
            &manifest.input_object_hash,
            extraction::MAX_SOURCE_OBJECT_BYTES,
        )
        .map_err(|e| e.error)?;
    if raw.len() as u64 != manifest.input_size_bytes || sha256(&raw) != manifest.input_object_hash {
        return Err(error(MemoryErrorCode::BrokenProvenance));
    }
    Ok(raw)
}

//! Private in-memory assessment materials, never an export or quality verdict.
use crate::{
    Result, error, history_review,
    history_semantic::{self, AnalysisTask},
    vault::{VaultAdapter, bytes},
};
use enouia_memory_contract::{
    MemoryErrorCode,
    hash::{Sha256Hex, sha256},
    json::Revision,
    ports::CommitPin,
    record::{AnyRecord, RecordKind, RecordRef},
};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub const MAX_BUNDLE_BYTES: usize = 262_144;
#[derive(Clone, Copy)]
pub enum EvidenceMode {
    Synthetic,
    OwnerSelected,
}
pub struct QualityOptions {
    pub case_id: String,
    /// Important units are chosen by the reviewer, never inferred from output.
    pub required_units: Vec<String>,
    /// A declaration of sample provenance, not permission to import real data.
    pub evidence_mode: EvidenceMode,
}
pub struct QualityBundle {
    pub base: CommitPin,
    /// A one-case history-quality-1 suite. Host handles any explicit save.
    pub suite: Value,
    /// Digest of compact JSON, not native task/wire bytes or an approval.
    pub suite_value_sha256: Sha256Hex,
}
fn value_hash(value: &Value) -> Result<Sha256Hex> {
    // Values here have only integer numeric fields. The sorted-map compact
    // UTF-8 representation matches the evaluator's ensure_ascii=False digest.
    Ok(sha256(
        &serde_json::to_vec(value).map_err(|_| error(MemoryErrorCode::InvalidRequest))?,
    ))
}
pub fn build(
    adapter: &VaultAdapter<'_>,
    task: &AnalysisTask,
    options: &QualityOptions,
) -> Result<QualityBundle> {
    build_inner(adapter, task, options, false)
}
/// Include the exact saved analysis input for private offline inspection.
/// This is opt-in; it never saves a file, sends a request or fills judgments.
pub fn build_with_analysis_input(
    adapter: &VaultAdapter<'_>,
    task: &AnalysisTask,
    options: &QualityOptions,
) -> Result<QualityBundle> {
    build_inner(adapter, task, options, true)
}
fn build_inner(
    adapter: &VaultAdapter<'_>,
    task: &AnalysisTask,
    options: &QualityOptions,
    include_analysis_input: bool,
) -> Result<QualityBundle> {
    if options.case_id.trim().is_empty()
        || options.case_id.len() > 128
        || options.required_units.is_empty()
        || options.required_units.len() > 32
        || options
            .required_units
            .iter()
            .any(|u| u.trim().is_empty() || u.len() > 256)
        || options.required_units.iter().collect::<BTreeSet<_>>().len()
            != options.required_units.len()
    {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let review = history_semantic::preview(adapter, task)?;
    let saved = adapter
        .saved_response(&review.dispatch_id)?
        .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
    if sha256(saved.text.as_bytes()) != review.response_hash {
        return Err(error(MemoryErrorCode::RevisionConflict));
    }
    let response: Value =
        serde_json::from_str(&saved.text).map_err(|_| error(MemoryErrorCode::InvalidRequest))?;
    let scope=Value::Array(review.source_coverage.iter().map(|c|json!({
        "source":c.slice.source,"selection":{"start":c.slice.start,"end":c.slice.end},
        "content_hash":c.slice.content_hash,"snippet_hash":c.slice.snippet_hash,
        "text":c.context["text"],"speaker_role":c.context["speaker_role"],
        "evidence_class":c.context["evidence_class"],"source_context":c.context["source_context"],
        "cited_by_response":c.cited_by_response,
    })).collect());
    let mut suite = json!({"schema_version":"history-quality-1","cases":[{
        "case_id":options.case_id,"evidence_mode":match options.evidence_mode {EvidenceMode::Synthetic=>"synthetic",EvidenceMode::OwnerSelected=>"owner_selected"},
        "reviewer":"pending","scope_kind":"native_selected_spans_v1","source_scope":scope,
        "scope_value_sha256":value_hash(&scope)?,"response":response,"response_value_sha256":value_hash(&response)?,
        "native_task_input_sha256":task.input_hash,"native_response_text_sha256":review.response_hash,
        "native_binding":{"session_id":task.session_id,"input_event_id":task.input_event_id,"dispatch_id":review.dispatch_id},
        "native_builder_validation":"passed_at_build",
        "required_units":options.required_units,"item_judgments":[],"unit_judgments":[],
    }]});
    if include_analysis_input {
        let AnyRecord::SessionEvent(input) = adapter
            .vault
            .read_parsed(
                &review.base,
                &RecordRef::new(
                    RecordKind::SessionEvent,
                    task.input_event_id.as_str(),
                    Revision::new(1).expect("first revision"),
                ),
            )
            .map_err(|e| e.error)?
        else {
            return Err(error(MemoryErrorCode::BrokenProvenance));
        };
        let text = enouia_memory_context::session::text(adapter.vault, &review.base, &input)
            .map_err(|e| e.error)?;
        if text.len() > history_semantic::MAX_TASK_BYTES
            || sha256(text.as_bytes()) != task.input_hash
        {
            return Err(error(MemoryErrorCode::InvalidRequest));
        }
        suite["cases"][0]["native_task_input_text"] = json!(text);
    }
    if bytes(&suite)?.len() > MAX_BUNDLE_BYTES {
        return Err(error(MemoryErrorCode::BudgetExceeded));
    }
    let refs = review
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
    history_review::fresh(adapter.vault, &review.base, &refs, adapter.enabled)?;
    let hash = value_hash(&suite)?;
    Ok(QualityBundle {
        base: review.base,
        suite,
        suite_value_sha256: hash,
    })
}

//! Owner-selected saved batches, compared locally through ADR-MEM-51.
//! No Provider configuration, mutation, supersession or current-truth selection.
use super::*;
use enouia_memory_provider::extraction::ExtractionApplication;
use enouia_memory_provider::history_review::{self, RunSelection};

const MAX_RESULT_BYTES: usize = 512 * 1024;

impl Workspace {
    pub(super) fn history_compare(&self, args: &wire::HistoryCompareArgs) -> R<Value> {
        let open = self.open()?;
        let base = open.vault.pin_current()?;
        let mut selections = Vec::new();
        let mut batches = Vec::new();
        for selected in &args.batches {
            let batch = crate::materialization::selected_batch(
                &open,
                &selected.task_id,
                selected.batch_index,
            )?;
            selections.push(RunSelection {
                session_id: batch.session_id.clone(),
                run_id: batch.run_id.clone(),
            });
            batches.push(
                json!({"taskId": selected.task_id, "batchIndex": selected.batch_index,
                "sessionId": batch.session_id, "runId": batch.run_id}),
            );
        }
        let review = history_review::preview(&open.vault, &open.owner, &selections, &open.enabled)
            .map_err(|e| {
                Fail(WorkspaceError::from_memory(
                    &e,
                    vec!["history.compare".into()],
                ))
            })?;
        let topics: Vec<_> = review
            .topics
            .iter()
            .map(|topic| {
                let entries: Vec<_> = topic.entries.iter().map(|entry| {
                let item = &entry.item;
                let c = &item.citation;
                json!({
                    "sessionId": entry.selection.session_id, "runId": entry.selection.run_id,
                    "flags": entry.flags, "responseIndex": item.response_index,
                    "proposedType": item.proposal.proposed_type,
                    "content": item.proposal.content, "details": item.proposal.details,
                    "quote": c.quote, "selectedText": c.selected_text,
                    "sourceId": c.source.source_id, "sourceRevision": c.source.source_revision,
                    "speakerRole": c.speaker_role, "evidenceClass": c.evidence_class,
                    "occurredAt": c.occurred_at, "capturedAt": c.captured_at,
                    "application": match item.application {
                        ExtractionApplication::AwaitingCursor => "awaiting_cursor",
                        ExtractionApplication::Suppressed => "suppressed",
                        ExtractionApplication::CandidateRecorded => "candidate_recorded",
                    },
                    "candidate": item.current_candidate.as_ref().map(|r| json!({
                        "candidateId": r.candidate_id, "revision": r.revision,
                        "status": r.status, "content": r.proposed_content,
                        "details": r.proposed_details,
                    })),
                })
            }).collect();
                json!({"key": topic.key, "entries": entries, "comparisons": topic.comparisons})
            })
            .collect();
        let result = json!({
             "base": {"commitId": review.base.commit_id, "sequence": review.base.sequence,
                "policyEpoch": review.base.policy_epoch, "deletionEpoch": review.base.deletion_epoch},
            "batches": batches, "runs": review.runs, "topics": topics,
        });
        if serde_json::to_vec(&result)
            .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "history.compare_result"))?
            .len()
            > MAX_RESULT_BYTES
        {
            return Err(fail(
                MemoryErrorCode::BudgetExceeded,
                "history.compare_budget",
            ));
        }
        // Serialization is part of this read; never return a torn snapshot.
        let current = open.vault.pin_current()?;
        if !open.enabled.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(fail(
                MemoryErrorCode::VaultLocked,
                "history.compare_disabled",
            ));
        }
        if current != review.base || base != review.base {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "history.compare_stale",
            ));
        }
        Ok(result)
    }
}

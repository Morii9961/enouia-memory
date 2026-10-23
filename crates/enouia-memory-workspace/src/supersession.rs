//! Explicit owner-selected draft only; no proposal, approval or canonical write.
use super::*;
use enouia_memory_contract::record::AnyRecord;
use enouia_memory_provider::history_review;

impl Workspace {
    pub(super) fn supersession_preview(&self, args: &wire::SupersessionPreviewArgs) -> R<Value> {
        let open = self.open()?;
        let base = open.vault.pin_current()?;
        let draft = history_review::supersession_draft(
            &open.vault,
            &open.owner,
            &args.candidate_id,
            args.candidate_revision,
            &args.target_memory_id,
            args.target_revision,
            args.effective_from.clone(),
            &open.enabled,
        )
        .map_err(|e| {
            Fail(WorkspaceError::from_memory(
                &e,
                vec!["history.supersession_preview".into()],
            ))
        })?;
        let reference = RecordRef::new(
            RecordKind::Memory,
            args.target_memory_id.as_str(),
            args.target_revision,
        );
        let AnyRecord::Memory(target) = open.vault.read_parsed(&draft.base, &reference)? else {
            return Err(fail(
                MemoryErrorCode::BrokenProvenance,
                "history.supersession_target",
            ));
        };
        let proposal = &draft.proposal;
        let evidence: Vec<_> = proposal
            .evidence
            .iter()
            .map(|e| {
                json!({
                    "sourceId": e.source_id, "sourceRevision": e.source_revision,
                    "supports": e.supports, "locator": e.locator,
                })
            })
            .collect();
        let result = json!({
            "readOnly": true,
            "base": {"commitId": draft.base.commit_id, "sequence": draft.base.sequence,
                "policyEpoch": draft.base.policy_epoch, "deletionEpoch": draft.base.deletion_epoch},
            "selection": args,
            "target": {"memoryId": target.memory_id, "revision": target.revision,
                "type": target.memory_type(), "content": target.content, "status": target.status},
            "proposal": {"kind": proposal.kind, "proposedType": proposal.proposed_type,
                "content": proposal.content, "details": proposal.details, "evidence": evidence,
                "reason": proposal.reason, "sensitivity": proposal.sensitivity,
                "targetMemoryId": proposal.target_memory_id, "expectedRevision": proposal.expected_revision,
                "effectiveFrom": proposal.effective_from},
            "origin": {"kind": draft.origin.kind, "actor": draft.origin.actor,
                "extractionRunId": draft.origin.extraction_run_id, "confidence": draft.origin.confidence},
        });
        if serde_json::to_vec(&result)
            .map_err(|_| {
                fail(
                    MemoryErrorCode::InvalidRequest,
                    "history.supersession_result",
                )
            })?
            .len()
            > 512 * 1024
        {
            return Err(fail(
                MemoryErrorCode::BudgetExceeded,
                "history.supersession_budget",
            ));
        }
        if !open.enabled.load(std::sync::atomic::Ordering::SeqCst) {
            return Err(fail(
                MemoryErrorCode::VaultLocked,
                "history.supersession_disabled",
            ));
        }
        if draft.base != base || open.vault.pin_current()? != base {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "history.supersession_stale",
            ));
        }
        Ok(result)
    }
}

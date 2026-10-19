//! Read-only archived history planning for the trusted workspace. No source
//! text, task creation, extraction, Provider or approval port is exposed.
use super::*;
use enouia_memory_contract::common::Locator;
use enouia_memory_contract::import::ImportStatus;
use enouia_memory_contract::policy::{
    AccessContext, PolicyRecord, ResourceContext, Scope, evaluate,
};
use enouia_memory_import::detect::{Detection, detect};
use enouia_memory_import::history::{
    HistoryIssueKind, HistoryLimits, HistoryPlanError, MessageRef, plan_history,
};
use enouia_memory_import::zip::ZipLimits;

const VERSION: &str = "workspace-history-plan-1";
const MAX_BYTES: usize = 64 * 1024 * 1024;
const MAX_MESSAGES: usize = 100_000;
const MAX_RESULT_BYTES: usize = 128 * 1024;
const MAX_SHOWN_ISSUES: usize = 100;

fn locator_key(locator: &Locator) -> R<Vec<u8>> {
    serde_json::to_vec(locator)
        .map_err(|_| fail(MemoryErrorCode::BrokenProvenance, "history.locator"))
}

fn source_ref(source: &SourceRecord) -> Value {
    json!({"sourceId": source.source_id, "sourceRevision": source.revision,
        "speakerRole": source.speaker_role, "evidenceClass": source.evidence_class,
        "occurredAt": source.occurred_at, "completeness": source.completeness})
}

fn offset(cursor: Option<&str>, hash: &str, total: usize) -> R<usize> {
    let Some(cursor) = cursor else { return Ok(0) };
    let (bound, offset) = cursor
        .split_once(':')
        .ok_or_else(|| fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"))?;
    if bound != hash {
        return Err(fail(
            MemoryErrorCode::RevisionConflict,
            "history.plan_stale",
        ));
    }
    let offset = offset
        .parse::<usize>()
        .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"))?;
    if offset > total {
        return Err(fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"));
    }
    Ok(offset)
}

impl Workspace {
    pub(super) fn history_plan(&self, args: &wire::HistoryPlanArgs) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let entry = open
            .vault
            .record_entry(&pin, RecordKind::Import, args.import_id.as_str())?
            .ok_or_else(|| fail(MemoryErrorCode::NotFound, "history.import_unknown"))?;
        if entry.revision != args.import_revision {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "history.import_revision",
            ));
        }
        let manifest: ImportManifest = parse(&open.vault.read_record(
            &pin,
            &RecordRef::new(RecordKind::Import, args.import_id.as_str(), entry.revision),
        )?)?;
        if manifest.status != ImportStatus::Completed || manifest.duplicate_of.is_some() {
            return Err(fail(
                MemoryErrorCode::InvalidRequest,
                "history.import_incomplete",
            ));
        }
        if manifest.input_size_bytes > MAX_BYTES as u64 {
            return Err(fail(
                MemoryErrorCode::BudgetExceeded,
                "history.input_budget",
            ));
        }
        let raw = open
            .vault
            .read_object_bounded(&pin, &manifest.input_object_hash, MAX_BYTES)?;
        if raw.len() as u64 != manifest.input_size_bytes
            || sha256(&raw) != manifest.input_object_hash
        {
            return Err(fail(
                MemoryErrorCode::BrokenProvenance,
                "history.input_hash",
            ));
        }
        // Bound decompression as well as the received object. Exceeding the
        // adapter's limits is never replaced with another parser or guessed.
        let limits = ZipLimits {
            max_member_bytes: MAX_BYTES as u64,
            max_total_bytes: MAX_BYTES as u64,
            ..ZipLimits::default()
        };
        let Detection::Parsed(input) = detect(&raw, &limits) else {
            return Err(fail(
                MemoryErrorCode::UnsupportedSchema,
                "history.unsupported",
            ));
        };
        if manifest.adapter.as_ref() != Some(&input.adapter)
            || manifest.source_schema_observed.as_ref() != Some(&input.source_schema_observed)
            || manifest.input_kind != input.input_kind
            || manifest.members != input.members
        {
            return Err(fail(
                MemoryErrorCode::UnsupportedSchema,
                "import.adapter_changed",
            ));
        }
        let message_count: usize = input.units.iter().map(|unit| unit.messages.len()).sum();
        if message_count > MAX_MESSAGES || input.units.len() > MAX_MESSAGES {
            return Err(fail(
                MemoryErrorCode::BudgetExceeded,
                "history.message_budget",
            ));
        }
        let plan = plan_history(
            &input,
            HistoryLimits {
                messages_per_batch: args.messages_per_batch as usize,
                max_path_references: args.max_path_references as usize,
            },
        )
        .map_err(|error| match error {
            HistoryPlanError::InvalidLimits => {
                fail(MemoryErrorCode::InvalidRequest, "history.limits")
            }
            HistoryPlanError::ReferenceBudgetExceeded => {
                fail(MemoryErrorCode::BudgetExceeded, "history.reference_budget")
            }
        })?;

        let policies: Vec<PolicyRecord> = latest_records(&open.vault, &pin, RecordKind::Policy)?;
        let policies: Vec<_> = policies.iter().collect();
        let now = open.vault.now()?;
        let purged = open.vault.purged()?;
        let mut records = BTreeMap::new();
        for entry in open.vault.record_entries(&pin, RecordKind::Source)? {
            if purged.record(RecordKind::Source, &entry.record_id, entry.revision.get()) {
                continue;
            }
            let reference = RecordRef::new(RecordKind::Source, &entry.record_id, entry.revision);
            let source: SourceRecord = parse(&open.vault.read_record(&pin, &reference)?)?;
            if source.import_id.as_ref() != Some(&args.import_id)
                || source.raw_object_hash.as_ref() != Some(&manifest.input_object_hash)
            {
                continue;
            }
            let resource = ResourceContext {
                record: reference,
                project_id: None,
                sensitivity: source.sensitivity,
            };
            if !evaluate(
                &policies,
                &AccessContext {
                    principal: &open.owner,
                    scope: Scope::SourceRead,
                    purpose: Some(enouia_memory_contract::context::Purpose::InspectionPreview),
                    destination: None,
                    resource: &resource,
                },
                &now,
            )
            .is_allow()
            {
                return Err(fail(
                    MemoryErrorCode::PermissionDenied,
                    "history.source_read",
                ));
            }
            if records
                .insert(locator_key(&source.locator)?, source)
                .is_some()
            {
                return Err(fail(
                    MemoryErrorCode::BrokenProvenance,
                    "history.source_ambiguous",
                ));
            }
        }
        let mut bound = BTreeMap::new();
        // Bind every parsed message, including unsafe graph/undated messages.
        // A removed/revised source cannot be revived by rereading Raw bytes.
        for (unit_index, unit) in input.units.iter().enumerate() {
            for (message_index, message) in unit.messages.iter().enumerate() {
                let source = records
                    .get(&locator_key(&message.locator)?)
                    .ok_or_else(|| {
                        fail(MemoryErrorCode::BrokenProvenance, "history.source_missing")
                    })?;
                if source.content_hash != message.content_hash
                    || source.source_kind != message.kind
                    || source.speaker_role != message.role
                    || source.evidence_class != message.evidence
                    || source.occurred_at != message.occurred_at
                    || source.time_precision != message.time_precision
                    || source.completeness != message.completeness
                    || source.original_time != message.original_time
                    || source.original_message_id != message.upstream_id
                    || source.original_conversation_id != unit.conversation_id
                {
                    return Err(fail(
                        MemoryErrorCode::BrokenProvenance,
                        "history.source_binding",
                    ));
                }
                bound.insert(
                    MessageRef {
                        unit: unit_index,
                        message: message_index,
                    },
                    source,
                );
            }
        }
        let identity = json!({"plannerVersion": VERSION, "vaultId": open.vault.vault_id(),
            "commitId": pin.commit_id, "importId": manifest.import_id, "importRevision": manifest.revision,
            "inputObjectHash": manifest.input_object_hash, "adapter": input.adapter,
            "messagesPerBatch": args.messages_per_batch, "maxPathReferences": args.max_path_references});
        let hash = sha256(
            &serde_json::to_vec(&json!([identity, hex(&open.history_nonce)]))
                .expect("JSON value serializes"),
        );
        let start = offset(args.cursor.as_deref(), hash.as_str(), plan.batches.len())?;
        let end = (start + args.limit.unwrap_or(wire::HISTORY_PAGE_MAX_LIMIT) as usize)
            .min(plan.batches.len());
        let batches: Vec<_> = plan.batches[start..end].iter().enumerate().map(|(n, batch)| {
            json!({"batchIndex": start + n, "leaf": source_ref(bound[&batch.leaf]),
                "selectedInExport": batch.selected_in_export, "observedPathStart": batch.observed_path_start,
                "preceding": batch.preceding.map(|id| source_ref(bound[&id])),
                "messages": batch.messages.iter().map(|id| source_ref(bound[id])).collect::<Vec<_>>()})
        }).collect();
        let issues: Vec<_> = plan
            .issues
            .iter()
            .take(MAX_SHOWN_ISSUES)
            .map(|issue| {
                json!({
                "unitIndex": issue.unit, "kind": match issue.kind {
                    HistoryIssueKind::AmbiguousMessageId => "ambiguous_message_id",
                    HistoryIssueKind::BrokenParentChain => "broken_parent_chain",
                    HistoryIssueKind::UnknownSelection => "unknown_selection",
                    HistoryIssueKind::NonMonotonicTime => "non_monotonic_time",
                }})
            })
            .collect();
        let result = json!({"identity": identity, "planHash": hash,
            "summary": {"units": input.units.len(), "messages": message_count,
                "dated": plan.chronological.len(), "undated": plan.undated.len(),
                "earliest": plan.earliest, "latest": plan.latest, "unparseable": plan.unparseable,
                "incompleteMessages": plan.incomplete_messages, "unavailableAttachments": plan.unavailable_attachments,
                "batches": plan.batches.len(), "issues": plan.issues.len()},
            "issues": issues, "issuesOmitted": plan.issues.len().saturating_sub(MAX_SHOWN_ISSUES),
            "batches": batches, "nextCursor": (end < plan.batches.len()).then(|| format!("{hash}:{end}")),
            "coverageIsCompleteHistory": false, "readOnly": true});
        if serde_json::to_vec(&result)
            .expect("JSON value serializes")
            .len()
            > MAX_RESULT_BYTES
        {
            return Err(fail(
                MemoryErrorCode::BudgetExceeded,
                "history.result_budget",
            ));
        }
        if open.vault.pin_current()?.commit_id != pin.commit_id {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "history.plan_stale",
            ));
        }
        Ok(result)
    }
}

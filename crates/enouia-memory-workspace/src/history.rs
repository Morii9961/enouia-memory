//! Native read-only history observation for Runtime (ADR-MEM-66).
//! This is a host method, not an additional page command or an extraction grant.

use super::*;
use enouia_memory_contract::commit::Tombstone;
use enouia_memory_contract::context::Purpose;
use enouia_memory_contract::ids::{CommitId, ImportId, VaultId};
use enouia_memory_contract::import::ImportStatus;
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::policy::{
    AccessContext, PolicyRecord, ResourceContext, Scope, evaluate,
};
use enouia_memory_import::detect::{Detection, detect};
use enouia_memory_import::history::{
    HistoryIssueKind, HistoryLimits, HistoryPlanError, MessageRef, plan_history,
};
use enouia_memory_import::zip::ZipLimits;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const VERSION: &str = "history-host-plan-1";
pub const MAX_INPUT_BYTES: usize = 8 * 1024 * 1024;
pub const MAX_RESULT_BYTES: usize = 256 * 1024;
pub const MAX_MESSAGES: usize = 4096;
pub const MAX_PATH_REFERENCES: u32 = 16_384;
const MAX_SOURCE_RECORDS: usize = 16_384;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BranchSelection {
    All,
    ExportSelected,
}

/// IDs/revisions come from the current host observation (`import_list` and
/// workspace status). Unit indices are in deterministic adapter order.
/// Hosts derive caller identity separately; it cannot be deserialized here.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct HistoryPlanRequest {
    pub vault_id: VaultId,
    pub commit_id: CommitId,
    pub import_id: ImportId,
    pub import_revision: Revision,
    pub unit_indices: Vec<u32>,
    pub branches: BranchSelection,
    pub messages_per_batch: u32,
    pub max_path_references: u32,
}

fn invalid(rule: &str) -> Fail {
    fail(MemoryErrorCode::InvalidRequest, rule)
}

fn budget() -> Fail {
    fail(MemoryErrorCode::BudgetExceeded, "history.plan_budget")
}

fn stale() -> Fail {
    fail(
        MemoryErrorCode::RevisionConflict,
        "history.observation_stale",
    )
}

fn deleted(tombstones: &[Tombstone], reference: &RecordRef) -> bool {
    tombstones.iter().any(|t| {
        t.targets.iter().any(|target| {
            target.record_kind == reference.record_kind
                && target.record_id == reference.record_id
                && target.revision.is_none_or(|r| r == reference.revision)
        })
    })
}

impl Workspace {
    /// Observe archived history without writing records or invoking a Provider.
    /// `surface` MUST come from the native window identity. The host discards
    /// the returned observation on lock/switch/close or any changed commit.
    /// Result format and bounds are specified in docs/integration/HISTORY.md.
    pub fn plan_history(
        &self,
        surface: HostSurface,
        request: &HistoryPlanRequest,
    ) -> Result<Value, WorkspaceError> {
        if surface != HostSurface::Workspace {
            return Err(WorkspaceError::new(
                MemoryErrorCode::PermissionDenied,
                &["history.host_surface"],
            ));
        }
        self.with_lifecycle(false, || self.history_plan_at(request))
            .map_err(|f| f.0)
    }

    fn history_plan_at(&self, request: &HistoryPlanRequest) -> R<Value> {
        if request.unit_indices.is_empty()
            || request.unit_indices.len() > 128
            || request.unit_indices.windows(2).any(|w| w[0] >= w[1])
            || !(1..=256).contains(&request.messages_per_batch)
            || !(1..=MAX_PATH_REFERENCES).contains(&request.max_path_references)
        {
            return Err(invalid("history.selection"));
        }
        let open = self.open()?;
        let vault = &open.vault;
        let pin = vault.pin_current()?;
        if request.vault_id != vault.descriptor().vault_id || request.commit_id != pin.commit_id {
            return Err(stale());
        }
        let import_ref = RecordRef::new(
            RecordKind::Import,
            request.import_id.as_str(),
            request.import_revision,
        );
        let manifest: ImportManifest = parse(&vault.read_record(&pin, &import_ref)?)?;
        if manifest.status != ImportStatus::Completed || manifest.duplicate_of.is_some() {
            return Err(invalid("history.import_not_completed"));
        }
        let tombstones: Vec<Tombstone> = latest_records(vault, &pin, RecordKind::Tombstone)?;
        if deleted(&tombstones, &import_ref)
            || tombstones
                .iter()
                .any(|t| t.object_hashes.contains(&manifest.input_object_hash))
        {
            return Err(fail(MemoryErrorCode::NotFound, "history.deleted"));
        }
        let raw = vault.read_object_bounded(&pin, &manifest.input_object_hash, MAX_INPUT_BYTES)?;
        if raw.len() as u64 != manifest.input_size_bytes {
            return Err(fail(
                MemoryErrorCode::BrokenProvenance,
                "history.archive_binding",
            ));
        }
        let zip_limits = ZipLimits {
            max_members: 1024,
            max_member_bytes: MAX_INPUT_BYTES as u64,
            max_total_bytes: 2 * MAX_INPUT_BYTES as u64,
            ..ZipLimits::default()
        };
        let Detection::Parsed(mut input) = detect(&raw, &zip_limits) else {
            return Err(fail(MemoryErrorCode::UnsupportedSchema, "history.adapter"));
        };
        if manifest.adapter.as_ref() != Some(&input.adapter)
            || manifest.source_schema_observed.as_ref() != Some(&input.source_schema_observed)
            || manifest.input_kind != input.input_kind
            || manifest.members != input.members
            || manifest.coverage.len() != input.units.len()
        {
            return Err(fail(MemoryErrorCode::UnsupportedSchema, "history.adapter"));
        }
        let total_units = input.units.len();
        if request
            .unit_indices
            .iter()
            .any(|&i| i as usize >= total_units)
        {
            return Err(invalid("history.unit_index"));
        }
        let selected: BTreeSet<_> = request.unit_indices.iter().copied().collect();
        input.units = input
            .units
            .into_iter()
            .enumerate()
            .filter_map(|(i, unit)| selected.contains(&(i as u32)).then_some(unit))
            .collect();
        if input.units.iter().map(|u| u.messages.len()).sum::<usize>() > MAX_MESSAGES {
            return Err(budget());
        }
        let plan = plan_history(
            &input,
            HistoryLimits {
                messages_per_batch: request.messages_per_batch as usize,
                max_path_references: request.max_path_references as usize,
            },
        )
        .map_err(|e| match e {
            HistoryPlanError::InvalidLimits => invalid("history.selection"),
            HistoryPlanError::ReferenceBudgetExceeded => budget(),
        })?;

        // Bind every selected message, including those in unsafe graphs. A
        // deleted/missing source cannot reappear through the retained archive.
        let wanted: BTreeSet<_> = input
            .units
            .iter()
            .flat_map(|u| &u.messages)
            .map(|m| canonical_bytes(&m.locator))
            .collect::<Result<_, _>>()?;
        let entries = vault.record_entries(&pin, RecordKind::Source)?;
        if entries.len() > MAX_SOURCE_RECORDS {
            return Err(budget());
        }
        let purged = vault.purged()?;
        let mut sources = BTreeMap::new();
        for entry in entries {
            // Physical tombstones stay in the catalog. Selected omissions
            // fail binding below; unrelated purges do not prevent planning.
            if purged.record(RecordKind::Source, &entry.record_id, entry.revision.get()) {
                continue;
            }
            let reference = RecordRef::new(RecordKind::Source, &entry.record_id, entry.revision);
            let source: SourceRecord = parse(&vault.read_record(&pin, &reference)?)?;
            if source.import_id.as_ref() != Some(&request.import_id)
                || source.raw_object_hash.as_ref() != Some(&manifest.input_object_hash)
            {
                continue;
            }
            let locator = canonical_bytes(&source.locator)?;
            if wanted.contains(&locator) && sources.insert(locator, source).is_some() {
                return Err(fail(
                    MemoryErrorCode::BrokenProvenance,
                    "history.source_binding",
                ));
            }
        }
        let policies: Vec<PolicyRecord> = latest_records(vault, &pin, RecordKind::Policy)?;
        let policy_refs: Vec<_> = policies.iter().collect();
        let now = vault.now()?;
        let mut references = vec![import_ref];
        let mut messages = Vec::new();
        let mut indices = BTreeMap::new();
        for (u, unit) in input.units.iter().enumerate() {
            for (m, message) in unit.messages.iter().enumerate() {
                let source = sources
                    .get(&canonical_bytes(&message.locator)?)
                    .ok_or_else(|| {
                        fail(MemoryErrorCode::BrokenProvenance, "history.source_binding")
                    })?;
                let reference = RecordRef::new(
                    RecordKind::Source,
                    source.source_id.as_str(),
                    source.revision,
                );
                if deleted(&tombstones, &reference) {
                    return Err(fail(MemoryErrorCode::NotFound, "history.deleted"));
                }
                let resource = ResourceContext {
                    record: reference.clone(),
                    project_id: None,
                    sensitivity: source.sensitivity,
                };
                if !evaluate(
                    &policy_refs,
                    &AccessContext {
                        principal: &open.owner,
                        scope: Scope::SourceRead,
                        purpose: Some(Purpose::InspectionPreview),
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
                if source.content_hash != message.content_hash
                    || source.source_kind != message.kind
                    || source.speaker_role != message.role
                    || source.evidence_class != message.evidence
                    || source.occurred_at != message.occurred_at
                    || source.original_time != message.original_time
                    || source.time_precision != message.time_precision
                    || source.completeness != message.completeness
                    || source.original_message_id != message.upstream_id
                    || source.original_conversation_id != unit.conversation_id
                {
                    return Err(fail(
                        MemoryErrorCode::BrokenProvenance,
                        "history.source_binding",
                    ));
                }
                indices.insert(
                    MessageRef {
                        unit: u,
                        message: m,
                    },
                    messages.len(),
                );
                messages.push(json!({
                    "unitIndex": request.unit_indices[u], "messageIndex": m,
                    "sourceId": source.source_id, "sourceRevision": source.revision,
                    "contentHash": source.content_hash, "occurredAt": source.occurred_at,
                    "timePrecision": source.time_precision, "speakerRole": source.speaker_role,
                    "completeness": source.completeness,
                }));
                references.push(reference);
            }
        }
        let identity = json!({"version": VERSION, "request": request,
            "inputObjectHash": manifest.input_object_hash, "messages": messages});
        let plan_id = sha256(&canonical_bytes(&identity)?);
        let batches: Vec<_> = plan.batches.iter().enumerate()
            .filter(|(_, b)| request.branches == BranchSelection::All || b.selected_in_export)
            .map(|(i, b)| json!({
                "batchId": sha256(format!("{VERSION}:{plan_id}:{i}").as_bytes()),
                "plannerBatchIndex": i, "leaf": indices[&b.leaf],
                "selectedInExport": b.selected_in_export, "observedPathStart": b.observed_path_start,
                "messages": b.messages.iter().map(|r| indices[r]).collect::<Vec<_>>(),
                "preceding": b.preceding.map(|r| indices[&r]),
            })).collect();
        let result = json!({
            "version": VERSION, "planId": plan_id, "vaultId": request.vault_id,
            "commitId": pin.commit_id, "importId": request.import_id,
            "importRevision": manifest.revision, "inputObjectHash": manifest.input_object_hash,
            "observedAt": now, "selection": request, "totalUnits": total_units,
            "archive": {
                "quarantinedMembers": manifest.members.iter().filter(|m| m.disposition == enouia_memory_contract::import::MemberDisposition::Quarantined).count(),
                "preservedOnlyMembers": manifest.members.iter().filter(|m| m.disposition == enouia_memory_contract::import::MemberDisposition::PreservedOnly).count(),
                "warningCodes": manifest.warnings.iter().map(|w| &w.code).collect::<BTreeSet<_>>(),
            },
            "units": input.units.iter().enumerate().map(|(i, u)| json!({
                "unitIndex": request.unit_indices[i], "messages": u.messages.len(),
                "threaded": u.threaded, "unparseable": u.unparseable,
            })).collect::<Vec<_>>(),
            "coverage": {"dated": plan.chronological.len(), "undated": plan.undated.len(),
                "earliest": plan.earliest, "latest": plan.latest, "unparseable": plan.unparseable,
                "incompleteMessages": plan.incomplete_messages,
                "unavailableAttachments": plan.unavailable_attachments},
            "issues": plan.issues.iter().map(|i| json!({"unitIndex": request.unit_indices[i.unit],
                "kind": match i.kind {
                    HistoryIssueKind::AmbiguousMessageId => "ambiguous_message_id",
                    HistoryIssueKind::BrokenParentChain => "broken_parent_chain",
                    HistoryIssueKind::UnknownSelection => "unknown_selection",
                    HistoryIssueKind::NonMonotonicTime => "non_monotonic_time",
                }})).collect::<Vec<_>>(),
            "chronological": plan.chronological.iter().map(|r| indices[r]).collect::<Vec<_>>(),
            "undated": plan.undated.iter().map(|r| indices[r]).collect::<Vec<_>>(),
            "messages": messages, "batches": batches,
        });
        if canonical_bytes(&result)?.len() > MAX_RESULT_BYTES {
            return Err(budget());
        }
        let freshness = vault.check_fresh(&pin, &references)?;
        if freshness.policy_changed || freshness.head.commit_id != pin.commit_id {
            return Err(stale());
        }
        Ok(result)
    }
}

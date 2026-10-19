//! Explicit offline H2 tasks. Stored admission owns recovery, not a page cursor.
use super::*;
use enouia_memory_contract::foundation::Cancellation;
use enouia_memory_contract::{history::HistoryMaterialization, ids::SessionId};
use enouia_memory_import::history::HistoryLimits;
use enouia_memory_provider::{codec::Api, history as provider_history};
use enouia_memory_vault::service::SessionStart;

fn api_of(api: wire::ExternalApi) -> Api {
    match api {
        wire::ExternalApi::Openai => Api::OpenAiResponses,
        wire::ExternalApi::Anthropic => Api::AnthropicMessages,
    }
}
fn local_error(e: enouia_memory_contract::MemoryError) -> Fail {
    Fail(WorkspaceError::from_memory(
        &e,
        vec!["history.local_task".into()],
    ))
}
fn options(
    header: &SessionRecord,
    a: &HistoryMaterialization,
) -> R<provider_history::HistoryOptions> {
    let api = match a.binding.provider.as_str() {
        "openai" => Api::OpenAiResponses,
        "anthropic" => Api::AnthropicMessages,
        _ => return Err(fail(MemoryErrorCode::UnsupportedSchema, "history.binding")),
    };
    Ok(provider_history::HistoryOptions {
        import_id: a.import_id.clone(),
        local_policy: header.policy_id.clone(),
        history_limits: HistoryLimits {
            messages_per_batch: a.messages_per_batch as usize,
            max_path_references: a.max_path_references as usize,
        },
        batch_indices: a.batch_indices.iter().map(|&n| n as usize).collect(),
        api,
        model: a.binding.model.clone(),
        subject_id: a.subject_id.clone(),
        max_candidates: a.max_candidates,
        max_pending: a.max_pending,
        max_reserved_tokens: a.max_reserved_tokens,
        max_reserved_cost_microusd: a.max_reserved_cost_microusd,
    })
}
fn helper_key(id: &SessionId) -> Vec<u8> {
    format!("workspace-history-task:{id}").into_bytes()
}
fn row(s: &SessionRecord, a: &HistoryMaterialization) -> Value {
    json!({"taskId": s.session_id, "createdAt": s.created_at,
        "request": {"importId": a.import_id, "importRevision": a.import_revision,
            "inputObjectHash": a.input_object_hash, "planHash": a.preview_plan_hash,
            "previewCommitId": a.preview_commit_id, "messagesPerBatch": a.messages_per_batch,
            "maxPathReferences": a.max_path_references, "batchIndices": a.batch_indices,
            "api": a.binding.provider, "model": a.binding.model, "maxCandidates": a.max_candidates,
            "maxPending": a.max_pending, "maxReservedTokens": a.max_reserved_tokens,
            "maxReservedCostMicrousd": a.max_reserved_cost_microusd}})
}
fn task(open: &Open, id: &SessionId) -> R<(SessionRecord, HistoryMaterialization)> {
    let pin = open.vault.pin_current()?;
    let header = session::get(&open.vault, &pin, id)?;
    if !header.participants.contains(&open.owner) {
        return Err(fail(MemoryErrorCode::PermissionDenied, "history.owner"));
    }
    let admission = header
        .history_materialization
        .clone()
        .ok_or_else(|| fail(MemoryErrorCode::NotFound, "history.task_unknown"))?;
    let entry = open
        .vault
        .record_entry(&pin, RecordKind::Import, admission.import_id.as_str())?
        .ok_or_else(|| fail(MemoryErrorCode::NotFound, "history.import_unknown"))?;
    let manifest: ImportManifest = parse(&open.vault.read_record(
        &pin,
        &RecordRef::new(
            RecordKind::Import,
            admission.import_id.as_str(),
            entry.revision,
        ),
    )?)?;
    if entry.revision != admission.import_revision
        || manifest.input_object_hash != admission.input_object_hash
    {
        return Err(fail(
            MemoryErrorCode::RevisionConflict,
            "history.import_revision",
        ));
    }
    Ok((header, admission))
}
fn progress(open: &Open, id: &SessionId) -> R<Value> {
    let pin = open.vault.pin_current()?;
    let (header, admission) = task(open, id)?;
    let state = provider_history::inspect(
        &open.vault,
        &open.owner,
        &options(&header, &admission)?,
        &helper_key(id),
    )
    .map_err(local_error)?;
    if open.vault.pin_current()?.commit_id != pin.commit_id {
        return Err(fail(
            MemoryErrorCode::RevisionConflict,
            "history.plan_stale",
        ));
    }
    let mut result = row(&header, &admission);
    result["identity"] = json!({"vaultId": open.vault.vault_id(),
        "commitId": pin.commit_id, "taskId": id});
    result["coordinatorSessionId"] = json!(state.coordinator_session_id);
    result["coordinatorInputId"] = json!(state.coordinator_input_id);
    result["materializationPlanHash"] = json!(state.plan_hash);
    result["batches"] = json!(
        state
            .batches
            .iter()
            .map(|b| json!({
        "batchIndex": b.batch_index, "sessionId": b.session_id, "runId": b.run_id,
        "inputEventId": b.input_event_id, "state": b.state, "applied": b.applied,
        "invocationState": b.invocation_state}))
            .collect::<Vec<_>>()
    );
    result["materializationComplete"] = json!(state.missing_batches.is_empty());
    result["missingBatchIndices"] = json!(state.missing_batches);
    result["readOnly"] = json!(true);
    Ok(result)
}
impl Workspace {
    pub(super) fn history_task_get(&self, a: &wire::HistoryTaskArgs) -> R<Value> {
        let open = self.open()?;
        progress(&open, &a.task_id)
    }
    pub(super) fn history_task_list(&self, a: &wire::PageArgs) -> R<Value> {
        let open = self.open()?;
        let pin = open.vault.pin_current()?;
        let mut headers: Vec<SessionRecord> =
            latest_records(&open.vault, &pin, RecordKind::Session)?;
        headers.retain(|h| {
            h.history_materialization.is_some() && h.participants.contains(&open.owner)
        });
        headers.sort_by(|a, b| {
            b.created_at
                .cmp(&a.created_at)
                .then(a.session_id.cmp(&b.session_id))
        });
        let start = match a.cursor.as_deref() {
            None => 0,
            Some(c) => {
                let (head, n) = c
                    .split_once(':')
                    .ok_or_else(|| fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"))?;
                if head != pin.commit_id.as_str() {
                    return Err(fail(
                        MemoryErrorCode::RevisionConflict,
                        "history.plan_stale",
                    ));
                }
                n.parse::<usize>()
                    .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"))?
            }
        };
        if start > headers.len() {
            return Err(fail(MemoryErrorCode::InvalidRequest, "workspace.cursor"));
        }
        let end = start
            .saturating_add(a.limit.unwrap_or(wire::LIST_DEFAULT_LIMIT) as usize)
            .min(headers.len());
        let items: Vec<_> = headers[start..end]
            .iter()
            .map(|s| row(s, s.history_materialization.as_ref().expect("retained")))
            .collect();
        if open.vault.pin_current()?.commit_id != pin.commit_id {
            return Err(fail(
                MemoryErrorCode::RevisionConflict,
                "history.plan_stale",
            ));
        }
        Ok(
            json!({"identity": {"vaultId": open.vault.vault_id(), "commitId": pin.commit_id},
            "items": items, "nextCursor": (end < headers.len()).then(|| format!("{}:{end}", pin.commit_id)), "readOnly": true}),
        )
    }
    pub(super) fn history_materialize(
        &self,
        a: &wire::HistoryMaterializeArgs,
        key: &str,
    ) -> R<Done> {
        let fingerprint = sha256(&serde_json::to_vec(a).expect("typed arguments")).to_string();
        self.start_once("history_materialize", key, fingerprint, || {
            let open = self.open()?;
            let admission_key = format!("workspace-history-admission:{key}").into_bytes();
            let admission = HistoryMaterialization {
                import_id: a.import_id.clone(),
                import_revision: a.import_revision,
                input_object_hash: a.input_object_hash.clone(),
                preview_plan_hash: a.plan_hash.clone(),
                preview_commit_id: a.preview_commit_id.clone(),
                messages_per_batch: a.messages_per_batch,
                max_path_references: a.max_path_references,
                batch_indices: a.batch_indices.clone(),
                binding: api_of(a.api).binding(&a.model).map_err(local_error)?,
                subject_id: SubjectId::parse(
                    &open.owner.actor_id.as_str().replacen("prn_", "sub_", 1),
                )
                .map_err(|_| fail(MemoryErrorCode::InvalidRequest, "workspace.subject"))?,
                max_candidates: a.max_candidates,
                max_pending: a.max_pending,
                max_reserved_tokens: a.max_reserved_tokens,
                max_reserved_cost_microusd: a.max_reserved_cost_microusd,
                submission_key_hash: sha256(&admission_key),
            };
            let scope = IdempotencyScope {
                principal_id: open.owner.actor_id.clone(),
                operation_kind: OperationKind::SessionAppend,
                key_hash: sha256(&admission_key),
            };
            let recovered = open
                .vault
                .find_receipt(&scope)?
                .map(|(_, _, r)| {
                    r.records
                        .into_iter()
                        .find(|r| r.record_kind == RecordKind::Session)
                        .ok_or_else(|| {
                            fail(
                                MemoryErrorCode::BrokenProvenance,
                                "history.admission_receipt",
                            )
                        })
                })
                .transpose()?;
            let (input, existing) = if let Some(reference) = recovered {
                let id = SessionId::parse(&reference.record_id).map_err(|_| {
                    fail(
                        MemoryErrorCode::BrokenProvenance,
                        "history.admission_receipt",
                    )
                })?;
                let (header, saved) = task(&open, &id)?;
                if saved != admission {
                    return Err(fail(
                        MemoryErrorCode::IdempotencyConflict,
                        "workspace.key_reuse",
                    ));
                }
                (
                    SessionStart {
                        owner: open.owner.clone(),
                        surface: header.origin_surface,
                        sensitivity: header.sensitivity,
                        policy_id: header.policy_id,
                    },
                    Some(id),
                )
            } else {
                let provider = self.native_provider()?;
                let destination = Self::destination(&provider, api_of(a.api))?;
                if destination.capabilities.binding != admission.binding {
                    return Err(fail(MemoryErrorCode::RevisionConflict, "history.binding"));
                }
                let plan = self.history_plan(&wire::HistoryPlanArgs {
                    import_id: a.import_id.clone(),
                    import_revision: a.import_revision,
                    messages_per_batch: a.messages_per_batch,
                    max_path_references: a.max_path_references,
                    cursor: None,
                    limit: Some(1),
                })?;
                if plan["planHash"] != json!(a.plan_hash)
                    || plan["identity"]["commitId"] != json!(a.preview_commit_id)
                    || plan["identity"]["inputObjectHash"] != json!(a.input_object_hash)
                {
                    return Err(fail(
                        MemoryErrorCode::RevisionConflict,
                        "history.plan_stale",
                    ));
                }
                let policy_id = self.policy(&open)?;
                let selected = provider_history::HistoryOptions {
                    import_id: a.import_id.clone(),
                    local_policy: policy_id.clone(),
                    history_limits: HistoryLimits {
                        messages_per_batch: a.messages_per_batch as usize,
                        max_path_references: a.max_path_references as usize,
                    },
                    batch_indices: a.batch_indices.iter().map(|&n| n as usize).collect(),
                    api: api_of(a.api),
                    model: a.model.clone(),
                    subject_id: admission.subject_id.clone(),
                    max_candidates: a.max_candidates,
                    max_pending: a.max_pending,
                    max_reserved_tokens: a.max_reserved_tokens,
                    max_reserved_cost_microusd: a.max_reserved_cost_microusd,
                };
                let sensitivity = provider_history::validate_selection(
                    &open.vault,
                    &open.owner,
                    &selected,
                    &admission_key,
                )
                .map_err(local_error)?;
                (
                    SessionStart {
                        owner: open.owner.clone(),
                        surface: ClientSurface::WindowsApp,
                        sensitivity,
                        policy_id,
                    },
                    None,
                )
            };
            Ok(self.spawn("history_materialize", move |ticket| {
                if ticket.is_cancelled() {
                    return Err(fail(MemoryErrorCode::Cancelled, "history.cancelled"));
                }
                let id = if let Some(id) = existing {
                    id
                } else {
                    session::start_history_materialization(
                        &open.vault,
                        &input,
                        &admission_key,
                        &admission,
                    )?
                    .id
                    .0
                };
                let (header, saved) = task(&open, &id)?;
                provider_history::materialize(
                    &open.vault,
                    &open.owner,
                    &options(&header, &saved)?,
                    &helper_key(&id),
                    ticket,
                )
                .map_err(local_error)?;
                progress(&open, &id)
            }))
        })
    }
    pub(super) fn history_resume(&self, a: &wire::HistoryTaskArgs, key: &str) -> R<Done> {
        self.start_once("history_resume", key, a.task_id.to_string(), || {
            let open = self.open()?;
            let id = a.task_id.clone();
            let (header, admission) = task(&open, &id)?;
            let options = options(&header, &admission)?;
            Ok(self.spawn("history_resume", move |ticket| {
                provider_history::materialize(
                    &open.vault,
                    &open.owner,
                    &options,
                    &helper_key(&id),
                    ticket,
                )
                .map_err(local_error)?;
                progress(&open, &id)
            }))
        })
    }
}

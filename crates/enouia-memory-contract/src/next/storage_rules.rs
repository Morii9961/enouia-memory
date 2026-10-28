use super::*;
use crate::{
    ContractError,
    common::{ActorRef, ActorType},
    hash::Sha256Hex,
    host::PrimaryBinding,
    json::Revision,
    time::Timestamp,
};
use std::collections::{BTreeMap, BTreeSet};

pub fn admission_batch_key(inputs: &[AdmissionInput]) -> Sha256Hex {
    let mut values = serde_json::to_value(inputs).expect("closed inputs");
    for input in values.as_array_mut().expect("input array") {
        let object = input.as_object_mut().expect("input object");
        object.remove("operation_id");
        object.remove("result");
        // The annotation's revision binding repeats the native-generated result;
        // it must not turn an identical prepared retry into different work.
        if let Some(annotations) = object
            .get_mut("record_fields")
            .and_then(|f| f.get_mut("annotations"))
            .and_then(|a| a.as_object_mut())
        {
            annotations.remove("memory");
        }
    }
    digest(&values)
}

/// The model can change interpretation/content/citations, never native scope,
/// original operation/result identity or the complete selected input boundary.
pub fn check_generation_binding(
    request: &AdmissionInput,
    result: &AdmissionInput,
) -> Result<(), ContractError> {
    if request.snapshot != result.snapshot
        || request.actor != result.actor
        || request.policy != result.policy
        || request.operation_id != result.operation_id
        || request.result != result.result
        || request.target != result.target
        || request.subject_id != result.subject_id
        || request.project_id != result.project_id
        || request.input_refs != result.input_refs
        || request.access_policy_id != result.access_policy_id
        || request.egress_policy_id != result.egress_policy_id
        || request.producer_version != result.producer_version
        || request.model_version != result.model_version
        || result.sensitivity < request.sensitivity
    {
        return Err(super::invalid("automation.generation_binding", ""));
    }
    if request.record_fields.as_ref().map(|r| &r.formation)
        != result.record_fields.as_ref().map(|r| &r.formation)
    {
        return Err(super::invalid(
            "ui.generation_origin",
            "/record_fields/formation",
        ));
    }
    if request.dimensions.as_ref().is_some_and(|d| {
        result.dimensions.as_ref().is_none_or(|r| {
            d.kind != r.kind || d.derived_from != r.derived_from || d.reverts_to != r.reverts_to
        })
    }) {
        return Err(super::invalid(
            "formation.generation_binding",
            "/dimensions",
        ));
    }
    Ok(())
}

pub fn with_stored_context<T>(
    data: &AutomationData,
    snapshot: &SnapshotBinding,
    actor: &ActorRef,
    now: &Timestamp,
    action: impl FnOnce(&EvaluationContext<'_>) -> T,
) -> T {
    let mut current_sources = BTreeMap::new();
    for source in &data.sources {
        current_sources.insert(&source.record.source_id, source);
    }
    let sources: Vec<_> = current_sources
        .values()
        .map(|source| SourceView {
            record: &source.record,
            attributed_subject: source.attributed_subject.clone(),
            deleted: data.deleted_sources.contains(&source.record.source_id),
            readable: data.policy.active
                && source.record.sensitivity <= data.policy.max_sensitivity
                && source.record.access_policy_id == data.policy.access_policy_id
                && data.policy.source_ids.contains(&source.record.source_id),
        })
        .collect();
    let mut current_memories = BTreeMap::new();
    for memory in &data.memories {
        current_memories.insert(&memory.memory.memory_id, memory);
    }
    let targets: Vec<_> = current_memories
        .values()
        .map(|memory| TargetView {
            created_under_policy: Some(memory.request.policy.policy_id.clone()),
            target: memory.target(),
            dimensions: memory.decision.input.dimensions.clone(),
            input_refs: memory.decision.input.input_refs.clone(),
            policy: memory.decision.input.policy.clone(),
            history: data
                .memories
                .iter()
                .filter(|m| m.memory.memory_id == memory.memory.memory_id)
                .map(|m| HistoricalMemoryView {
                    target: m.target(),
                    content: m.decision.input.content.clone(),
                    interpretation: m.decision.input.interpretation,
                    dimensions: m.decision.input.dimensions.clone(),
                    input_refs: m.decision.input.input_refs.clone(),
                })
                .collect(),
            legacy_owner_record: false,
            subject_id: memory.decision.input.subject_id.clone(),
            project_id: memory.decision.input.project_id.clone(),
            sensitivity: memory.decision.input.sensitivity,
            access_policy_id: memory.decision.input.access_policy_id.clone(),
            egress_policy_id: memory.decision.input.egress_policy_id.clone(),
            writable: memory.decision.input.subject_id == data.policy.subject_id
                && memory.decision.input.project_id == data.policy.project_id,
            deleted: data.deleted_memories.contains(&memory.memory.memory_id),
        })
        .collect();
    action(&EvaluationContext {
        system_actor: actor,
        policy: &data.policy,
        snapshot,
        now,
        sources: &sources,
        targets: &targets,
    })
}

fn next(revision: Revision) -> Result<Revision, ContractError> {
    Revision::new(revision.get() + 1).ok_or_else(|| super::invalid("automation.counter", ""))
}
fn require(condition: bool, rule: &'static str) -> Result<(), ContractError> {
    if condition {
        Ok(())
    } else {
        Err(super::invalid(rule, ""))
    }
}

pub fn validate_delta(
    descriptor: &AutomationDescriptor,
    before: Option<&AutomationCommit>,
    after: &AutomationCommit,
) -> Result<(), ContractError> {
    super::checked(descriptor.validate())?;
    super::checked(after.validate())?;
    require(
        (descriptor.layout == AutomationLayout::HostV3
            || (after.data.record_catalog.is_none()
                && after.data.owner_actions.is_none()
                && after
                    .data
                    .sources
                    .iter()
                    .all(|s| s.record_fields.is_none() && s.attachments.is_empty())
                && after.data.memories.iter().all(|m| {
                    m.request.record_fields.is_none() && m.decision.input.record_fields.is_none()
                })))
            && match descriptor.layout {
                AutomationLayout::SegmentedV1 => false,
                AutomationLayout::HostV1 => {
                    after.data.policy.rule_version != RuleVersion::V3
                        && after
                            .data
                            .memories
                            .iter()
                            .all(|m| m.decision.input.dimensions.is_none())
                }
                AutomationLayout::HostV3 => {
                    after.data.policy.rule_version == RuleVersion::V3
                        && after.data.record_catalog.is_some()
                        && after.data.owner_actions.as_ref().is_some_and(|l| {
                            l.records.iter().all(|r| {
                                r.plan.owner == descriptor.created_by
                                    && r.plan.snapshot.vault_id == descriptor.vault_id
                            })
                        })
                        && after.data.sources.iter().all(|s| s.record_fields.is_some())
                        && after.data.memories.iter().all(|m| {
                            m.request.record_fields.is_some()
                                && m.decision.input.record_fields.is_some()
                                && m.decision.input.dimensions.is_some()
                        })
                }
                AutomationLayout::HostV2 => {
                    after.data.policy.rule_version == RuleVersion::V3
                        && after
                            .data
                            .memories
                            .iter()
                            .all(|m| m.decision.input.dimensions.is_some())
                }
            },
        "formation.layout",
    )?;
    require(
        after.data.policy.vault_id == descriptor.vault_id
            && after.data.policy.system_actor == descriptor.system_actor
            && after.authority.primary.host_id == descriptor.host_id,
        "automation.root_binding",
    )?;
    match before {
        None => {
            require(
                after.change == AutomationChange::Genesis
                    && after.parent.is_none()
                    && after.sequence.get() == 1
                    && after.actor == descriptor.created_by
                    && after.data.policy.revision.get() == 1
                    && after.data.policy_watermark.get() == 1
                    && after.data.deletion_watermark.get() == 1
                    && after
                        .data
                        .record_catalog
                        .as_ref()
                        .is_none_or(RecordCatalog::is_empty)
                    && after.data.sources.is_empty()
                    && after
                        .data
                        .owner_actions
                        .as_ref()
                        .is_none_or(|l| *l == OwnerActionLedger::default())
                    && after.data.memories.is_empty()
                    && after.data.receipts.is_empty()
                    && after.data.deleted_sources.is_empty()
                    && after.data.deleted_memories.is_empty(),
                "automation.genesis",
            )?;
            let expected = crate::host::AuthorityState::initial(
                descriptor.vault_id.clone(),
                descriptor.host_id.clone(),
                after.checkpoint(),
            )
            .map_err(|_| super::invalid("automation.authority", ""))?;
            require(after.authority == expected, "automation.authority")
        }
        Some(before) => {
            super::checked(before.validate())?;
            require(
                after.parent == Some(before.current())
                    && after.sequence == next(before.sequence)?
                    && after.commit_id != before.commit_id
                    && after.created_at >= before.created_at,
                "automation.parent_or_time",
            )?;
            let writer = PrimaryBinding {
                host_id: descriptor.host_id.clone(),
                primary_epoch: before.authority.primary.primary_epoch,
            };
            let authority = before
                .authority
                .advance(&writer, &before.checkpoint(), after.checkpoint())
                .map_err(|_| super::invalid("automation.authority", ""))?;
            require(after.authority == authority, "automation.authority")?;
            let mut expected = before.data.clone();
            match after.change {
                AutomationChange::OwnerAction => {
                    return Err(super::invalid("ui.owner_entry_unavailable", ""));
                }
                AutomationChange::Genesis => return Err(super::invalid("automation.genesis", "")),
                AutomationChange::Capture => {
                    require(
                        after.actor == descriptor.system_actor
                            && after.data.sources.len() == before.data.sources.len() + 1,
                        "automation.capture_delta",
                    )?;
                    let source = after.data.sources.last().expect("one appended source");
                    require(
                        before.data.policy.active
                            && after.created_at >= before.data.policy.valid_from
                            && after.created_at < before.data.policy.valid_until
                            && before
                                .data
                                .policy
                                .source_ids
                                .contains(&source.record.source_id)
                            && !before
                                .data
                                .deleted_sources
                                .contains(&source.record.source_id)
                            && source.record.access_policy_id
                                == before.data.policy.access_policy_id
                            && source.record.sensitivity <= before.data.policy.max_sensitivity
                            && source
                                .attributed_subject
                                .as_ref()
                                .is_none_or(|s| s == &before.data.policy.subject_id),
                        "automation.capture_scope",
                    )?;
                    let old = before
                        .data
                        .sources
                        .iter()
                        .rev()
                        .find(|s| s.record.source_id == source.record.source_id);
                    require(
                        old.is_none_or(|old| old.record.locator == source.record.locator)
                            && !before.data.sources.iter().any(|other| {
                                other.record.source_id != source.record.source_id
                                    && other.record.locator == source.record.locator
                            }),
                        "automation.event_identity",
                    )?;
                    require(
                        source.record.revision
                            == old.map_or_else(
                                || Ok(Revision::new(1).unwrap()),
                                |s| next(s.record.revision),
                            )?,
                        "automation.source_revision",
                    )?;
                    expected.sources.push(source.clone());
                    if let (Some(old), Some(new)) =
                        (&before.data.record_catalog, &after.data.record_catalog)
                    {
                        require(new.extends(old), "ui.catalog_append")?;
                        expected.record_catalog = Some(new.clone());
                    }
                }
                AutomationChange::Policy => {
                    require(
                        after.actor == descriptor.created_by
                            && after.actor.actor_type == ActorType::Owner
                            && after.data.policy.policy_id == before.data.policy.policy_id
                            && after.data.policy.revision == next(before.data.policy.revision)?,
                        "automation.policy_actor_or_revision",
                    )?;
                    expected.policy = after.data.policy.clone();
                    expected.policy_watermark = next(before.data.policy_watermark)?;
                }
                AutomationChange::DeleteSource => {
                    require(
                        after.actor == descriptor.created_by
                            && after.data.deleted_sources.len()
                                == before.data.deleted_sources.len() + 1,
                        "automation.delete_actor",
                    )?;
                    let id = after.data.deleted_sources.last().expect("one deletion");
                    require(
                        before
                            .data
                            .sources
                            .iter()
                            .any(|s| &s.record.source_id == id)
                            && !before.data.deleted_sources.contains(id),
                        "automation.delete_target",
                    )?;
                    expected.deleted_sources.push(id.clone());
                    expected.deletion_watermark = next(before.data.deletion_watermark)?;
                }
                AutomationChange::DeleteMemory => {
                    require(
                        after.actor == descriptor.created_by
                            && after.data.deleted_memories.len()
                                == before.data.deleted_memories.len() + 1,
                        "automation.delete_actor",
                    )?;
                    let id = after.data.deleted_memories.last().expect("one deletion");
                    require(
                        before
                            .data
                            .memories
                            .iter()
                            .any(|m| &m.memory.memory_id == id)
                            && !before.data.deleted_memories.contains(id),
                        "automation.delete_target",
                    )?;
                    expected.deleted_memories.push(id.clone());
                    expected.deletion_watermark = next(before.data.deletion_watermark)?;
                }
                AutomationChange::AdmitBatch => {
                    require(
                        after.actor == descriptor.system_actor
                            && after.data.memories.len() > before.data.memories.len()
                            && after.data.memories.len()
                                <= before.data.memories.len() + MAX_AUTOMATION_BATCH
                            && after.data.receipts.len() - before.data.receipts.len()
                                == after.data.memories.len() - before.data.memories.len(),
                        "automation.admit_delta",
                    )?;
                    let memories = &after.data.memories[before.data.memories.len()..];
                    let receipts = &after.data.receipts[before.data.receipts.len()..];
                    let inputs: Vec<_> = memories.iter().map(|m| m.request.clone()).collect();
                    let key = admission_batch_key(&inputs);
                    let bindings: Vec<_> = receipts
                        .iter()
                        .map(|r| GenerationBinding {
                            operation_id: r.operation_id.clone(),
                            input_hash: r.request_hash.clone(),
                            response_hash: r.response_hash.clone(),
                        })
                        .collect();
                    let binding_hash = digest(&bindings);
                    require(
                        !before.data.receipts.iter().any(|r| r.job_key == key),
                        "automation.job_duplicate",
                    )?;
                    let mut result_ids = BTreeSet::new();
                    for (memory, receipt) in memories.iter().zip(receipts) {
                        require(
                            result_ids.insert(&memory.memory.memory_id)
                                && memory.committed_in == after.commit_id
                                && receipt.committed_in == after.commit_id
                                && receipt.sequence == after.sequence
                                && receipt.job_key == key
                                && receipt.binding_hash == binding_hash,
                            "automation.batch_binding",
                        )?;
                        check_generation_binding(&memory.request, &memory.decision.input)?;
                        with_stored_context(
                            &before.data,
                            &before.snapshot(),
                            &descriptor.system_actor,
                            &after.created_at,
                            |context| {
                                evaluate(&memory.request, context)?;
                                require(
                                    evaluate(&memory.decision.input, context)? == memory.decision,
                                    "automation.decision",
                                )?;
                                validate_commit_binding(
                                    &memory.decision,
                                    &PlannedCommit {
                                        schema_version: NextVersion,
                                        kind: CommitKind::PlannedCommit,
                                        snapshot: before.snapshot(),
                                        planned_commit_id: after.commit_id.clone(),
                                        operation_id: receipt.operation_id.clone(),
                                        decision_hash: receipt.decision_hash.clone(),
                                        result: receipt.result.clone(),
                                    },
                                    context,
                                )
                            },
                        )?;
                    }
                    expected.memories.extend_from_slice(memories);
                    expected.receipts.extend_from_slice(receipts);
                }
            }
            require(expected == after.data, "automation.non_append_delta")
        }
    }
}

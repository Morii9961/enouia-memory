use super::admission::outcome;
use super::*;
use crate::common::{ActorRef, ActorType, Sensitivity, SourceRevisionRef, SpeakerRole};
use crate::error::ContractError;
use crate::ids::{PolicyId, ProjectId, SubjectId};
use crate::source::{Completeness, SourceRecord};
use crate::time::Timestamp;
use std::collections::BTreeSet;

/// Supplied by the trusted adapter after current policy/deletion resolution.
/// Source metadata is exact, current and validated; body semantics are not.
pub struct SourceView<'a> {
    pub record: &'a SourceRecord,
    /// Identity resolved by the adapter, not inferred from an author label or
    /// the requested subject. Required for adopted self-report attribution.
    pub attributed_subject: Option<SubjectId>,
    pub readable: bool,
    pub deleted: bool,
}

#[derive(Clone)]
pub struct TargetView {
    /// Native machine-creation provenance, never inferred from model JSON.
    pub created_under_policy: Option<PolicyId>,
    pub target: TargetRef,
    /// Only next-version machine memories may be revised by this slice.
    pub legacy_owner_record: bool,
    pub subject_id: SubjectId,
    pub project_id: Option<ProjectId>,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    pub egress_policy_id: PolicyId,
    pub writable: bool,
    pub deleted: bool,
}

/// Not deserializable from a caller request. These are trusted observations,
/// NOT an authentication API or a cached production write permit. Core-3 must
/// obtain them freshly inside the legal transaction's security barriers.
pub struct EvaluationContext<'a> {
    pub system_actor: &'a ActorRef,
    pub policy: &'a AutomationPolicy,
    pub snapshot: &'a SnapshotBinding,
    pub now: &'a Timestamp,
    pub sources: &'a [SourceView<'a>],
    pub targets: &'a [TargetView],
}

pub fn evaluate(
    input: &AdmissionInput,
    context: &EvaluationContext<'_>,
) -> Result<DecisionDraft, ContractError> {
    checked(input.validate())?;
    let policy = context.policy;
    checked(policy.validate())?;
    if context.system_actor.actor_type != ActorType::System
        || &input.actor != context.system_actor
        || input.actor != policy.system_actor
    {
        return Err(invalid("next.system_actor", "/actor"));
    }
    if input.policy != policy.reference() {
        return Err(invalid("next.policy_binding", "/policy"));
    }
    if !policy.active || context.now < &policy.valid_from || context.now >= &policy.valid_until {
        return Err(invalid("next.policy_inactive", "/policy"));
    }
    if &input.snapshot != context.snapshot {
        return Err(invalid("next.snapshot_stale", "/snapshot"));
    }
    if input.snapshot.vault_id != policy.vault_id
        || input.subject_id != policy.subject_id
        || input.project_id != policy.project_id
        || input.sensitivity > policy.max_sensitivity
        || input.access_policy_id != policy.access_policy_id
        || input.egress_policy_id != policy.egress_policy_id
    {
        return Err(invalid("next.policy_scope", "/policy"));
    }

    // An adapter must supply one current version per identity. Never choose a
    // convenient duplicate observation or silently ignore ambiguous metadata.
    if context
        .sources
        .iter()
        .map(|s| &s.record.source_id)
        .collect::<BTreeSet<_>>()
        .len()
        != context.sources.len()
        || context
            .targets
            .iter()
            .map(|t| &t.target.memory.memory_id)
            .collect::<BTreeSet<_>>()
            .len()
            != context.targets.len()
    {
        return Err(invalid("next.ambiguous_view", ""));
    }

    let mut dependencies = BTreeSet::new();
    for support in &input.evidence {
        dependencies.insert(SourceRevisionRef {
            source_id: support.source_id.clone(),
            source_revision: support.source_revision,
        });
    }
    dependencies.extend(input.context_source_refs.iter().cloned());
    if dependencies != input.input_refs.iter().cloned().collect() {
        return Err(invalid("next.dependency_closure", "/input_refs"));
    }
    let mut sensitivity_floor = Sensitivity::Public;
    for dependency in &input.input_refs {
        if !policy.source_ids.contains(&dependency.source_id) {
            return Err(invalid("next.policy_scope", "/input_refs"));
        }
        let source = context
            .sources
            .iter()
            .find(|s| s.record.source_id == dependency.source_id)
            .ok_or_else(|| invalid("next.source_missing", "/input_refs"))?;
        checked(source.record.validate())?;
        if source.record.revision != dependency.source_revision {
            return Err(invalid("next.source_stale", "/input_refs"));
        }
        if source.deleted
            || !source.readable
            || source.record.completeness != Completeness::Complete
        {
            return Err(invalid("next.source_unavailable", "/input_refs"));
        }
        if source.record.access_policy_id != input.access_policy_id {
            return Err(invalid("next.source_policy", "/access_policy_id"));
        }
        sensitivity_floor = sensitivity_floor.max(source.record.sensitivity);
    }

    let mut content_hashes = BTreeSet::new();
    let mut upstream_ids = BTreeSet::new();
    for support in &input.evidence {
        // Closure above already proved existence and exact revisions.
        let source = context
            .sources
            .iter()
            .find(|s| s.record.source_id == support.source_id)
            .expect("validated dependency closure")
            .record;
        if &support.object_hash != source.anchor_hash()
            || !support.locator.within(&source.locator)
            || support.evidence_class != source.evidence_class
        {
            return Err(invalid("next.evidence_anchor", "/evidence"));
        }
        let upstream = source.original_message_id.as_ref().map(|message| {
            (
                &source.provider,
                &source.account_scope,
                &source.original_conversation_id,
                message,
            )
        });
        if !content_hashes.insert(source.content_hash.clone())
            || upstream.is_some_and(|id| !upstream_ids.insert(id))
        {
            return Err(invalid("next.duplicate_origin", "/evidence"));
        }
        if input.interpretation == Interpretation::ClearSelfReport
            && (source.speaker_role != SpeakerRole::User
                || !source.evidence_class.is_user_evidence()
                || context
                    .sources
                    .iter()
                    .find(|s| s.record.source_id == support.source_id)
                    .and_then(|s| s.attributed_subject.as_ref())
                    != Some(&input.subject_id))
        {
            return Err(invalid("next.self_report_support", "/evidence"));
        }
    }

    match &input.target {
        None => {
            if !policy.allow_create {
                return Err(invalid("next.policy_scope", "/target"));
            }
            if input.result.revision.get() != 1
                || context
                    .targets
                    .iter()
                    .any(|t| t.target.memory.memory_id == input.result.memory_id)
            {
                return Err(invalid("next.result_revision", "/result"));
            }
        }
        Some(target) => {
            let current = context
                .targets
                .iter()
                .find(|t| t.target.memory.memory_id == target.memory.memory_id)
                .ok_or_else(|| invalid("next.target_missing", "/target"))?;
            if !policy.target_ids.contains(&target.memory.memory_id)
                && !(policy.allow_revise_created
                    && current.created_under_policy.as_ref() == Some(&policy.policy_id))
            {
                return Err(invalid("next.policy_scope", "/target"));
            }
            if &current.target != target {
                return Err(invalid("next.target_stale", "/target"));
            }
            if current.legacy_owner_record || current.deleted || !current.writable {
                return Err(invalid("next.target_unavailable", "/target"));
            }
            if current.subject_id != input.subject_id
                || current.project_id != input.project_id
                || current.access_policy_id != input.access_policy_id
                || current.egress_policy_id != input.egress_policy_id
            {
                return Err(invalid("next.target_scope", "/target"));
            }
            if input.result.memory_id != target.memory.memory_id
                || target.memory.revision.get().checked_add(1) != Some(input.result.revision.get())
            {
                return Err(invalid("next.result_revision", "/result"));
            }
            sensitivity_floor = sensitivity_floor.max(current.sensitivity);
        }
    }
    if input.sensitivity < sensitivity_floor {
        return Err(invalid("next.sensitivity_floor", "/sensitivity"));
    }
    let draft = DecisionDraft {
        schema_version: NextVersion,
        kind: DecisionKind::AdmissionDraft,
        rule_version: policy.rule_version,
        input: input.clone(),
        outcome: outcome(input.interpretation),
    };
    checked(draft.validate())?;
    Ok(draft)
}

/// Reevaluate rather than trusting a deserialized outcome. This validates only
/// the prospective association; it does not allocate, publish or receipt it.
pub fn validate_commit_binding(
    decision: &DecisionDraft,
    commit: &PlannedCommit,
    context: &EvaluationContext<'_>,
) -> Result<(), ContractError> {
    checked(decision.validate())?;
    checked(commit.validate())?;
    if evaluate(&decision.input, context)? != *decision {
        return Err(invalid("next.decision_changed", ""));
    }
    if commit.snapshot != decision.input.snapshot
        || commit.result != decision.input.result
        || commit.operation_id != decision.input.operation_id
        || commit.decision_hash != digest(decision)
    {
        return Err(invalid("next.commit_binding", ""));
    }
    Ok(())
}

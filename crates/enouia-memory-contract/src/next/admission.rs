use super::{MAX_DOCUMENT_BYTES, MAX_SOURCES, NextDocument, NextVersion};
use crate::common::{ActorRef, ActorType, EvidenceRef, Sensitivity, SourceRevisionRef};
use crate::error::Violation;
use crate::hash::{Sha256Hex, sha256};
use crate::ids::{
    CommitId, MemoryId, OperationId, PolicyId, ProjectId, SourceId, SubjectId, VaultId,
};
use crate::json::{Revision, canonical_bytes};
use crate::time::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

macro_rules! kind {
    ($name:ident, $variant:ident, $wire:literal) => {
        #[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
        pub enum $name {
            #[serde(rename = $wire)]
            $variant,
        }
    };
}
kind!(PolicyKind, AutomationPolicy, "automation_policy");
kind!(InputKind, AdmissionInput, "admission_input");
kind!(DecisionKind, AdmissionDraft, "admission_draft");
kind!(CommitKind, PlannedCommit, "planned_commit");
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RuleVersion {
    #[serde(rename = "admission-rules-1")]
    V1,
    #[serde(rename = "admission-rules-2")]
    V2,
    #[serde(rename = "admission-rules-3")]
    V3,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SnapshotBinding {
    pub vault_id: VaultId,
    pub base_commit_id: CommitId,
    pub primary_epoch: Revision,
    pub policy_watermark: Revision,
    pub deletion_watermark: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PolicyRef {
    pub policy_id: PolicyId,
    pub revision: Revision,
    pub hash: Sha256Hex,
}

/// Policy is supplied by a trusted adapter, never granted by parsing it.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutomationPolicy {
    pub schema_version: NextVersion,
    pub kind: PolicyKind,
    pub policy_id: PolicyId,
    pub revision: Revision,
    pub rule_version: RuleVersion,
    pub system_actor: ActorRef,
    pub vault_id: VaultId,
    pub subject_id: SubjectId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub project_id: Option<ProjectId>,
    pub source_ids: Vec<SourceId>,
    pub allow_create: bool,
    /// Core-3 opt-in scope: revise only machine memories whose native provenance
    /// names this same policy. Absent/false preserves existing policy bytes.
    #[serde(default, skip_serializing_if = "is_false")]
    pub allow_revise_created: bool,
    pub target_ids: Vec<MemoryId>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allowed_forms: Vec<super::FormationKind>,
    pub max_sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    pub egress_policy_id: PolicyId,
    pub active: bool,
    pub valid_from: Timestamp,
    pub valid_until: Timestamp,
}
fn is_false(value: &bool) -> bool {
    !*value
}

impl AutomationPolicy {
    pub fn reference(&self) -> PolicyRef {
        PolicyRef {
            policy_id: self.policy_id.clone(),
            revision: self.revision,
            hash: digest(self),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryRevisionRef {
    pub memory_id: MemoryId,
    pub revision: Revision,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TargetRef {
    pub memory: MemoryRevisionRef,
    pub hash: Sha256Hex,
}

/// Semantic analysis supplied by a producer; these labels are not inferred by
/// structural validation and do not prove the model's interpretation correct.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Interpretation {
    ClearSelfReport,
    Inference,
    Ambiguous,
    Conflicting,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AdmissionInput {
    pub schema_version: NextVersion,
    pub kind: InputKind,
    pub snapshot: SnapshotBinding,
    pub operation_id: OperationId,
    pub actor: ActorRef,
    pub policy: PolicyRef,
    pub producer_version: String,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub model_version: Option<String>,
    pub subject_id: SubjectId,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub project_id: Option<ProjectId>,
    pub content: String,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::record_fields::present"
    )]
    pub record_fields: Option<super::MemoryRecordFields>,
    pub interpretation: Interpretation,
    #[serde(
        default,
        skip_serializing_if = "Option::is_none",
        deserialize_with = "super::formation::deserialize_dimensions"
    )]
    pub dimensions: Option<super::FormationDimensions>,
    pub input_refs: Vec<SourceRevisionRef>,
    pub evidence: Vec<EvidenceRef>,
    pub context_source_refs: Vec<SourceRevisionRef>,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub target: Option<TargetRef>,
    pub result: MemoryRevisionRef,
    pub sensitivity: Sensitivity,
    pub access_policy_id: PolicyId,
    pub egress_policy_id: PolicyId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KnowledgeState {
    AttributedStatement,
    Hypothesis,
    Uncertain,
    Disputed,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdoptionState {
    Adopted,
    Staged,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reassessment {
    OnEvidenceOrPolicyChange,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Outcome {
    pub knowledge: KnowledgeState,
    pub adoption: AdoptionState,
    #[serde(deserialize_with = "crate::json::nullable")]
    pub reassessment: Option<Reassessment>,
}

/// A reproducible decision draft; not an approved memory or a commit receipt.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DecisionDraft {
    pub schema_version: NextVersion,
    pub kind: DecisionKind,
    pub rule_version: RuleVersion,
    pub input: AdmissionInput,
    pub outcome: Outcome,
}

/// A prospective transaction association only. It is not CommitManifest, has
/// no success field and is not accepted by the existing writer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PlannedCommit {
    pub schema_version: NextVersion,
    pub kind: CommitKind,
    pub snapshot: SnapshotBinding,
    pub planned_commit_id: CommitId,
    pub operation_id: OperationId,
    pub decision_hash: Sha256Hex,
    pub result: MemoryRevisionRef,
}

/// Stable binding, with the same canonical JSON convention as v1 documents.
pub fn digest(value: &impl Serialize) -> Sha256Hex {
    sha256(&canonical_bytes(value).expect("closed contract serialization"))
}

fn bounded_unique<T: Ord>(values: &[T], nonempty: bool, path: &str, out: &mut Vec<Violation>) {
    if values.len() > MAX_SOURCES
        || (nonempty && values.is_empty())
        || values.iter().collect::<BTreeSet<_>>().len() != values.len()
    {
        out.push(Violation::new("next.references", path));
    }
}

fn document_budget(value: &impl Serialize, out: &mut Vec<Violation>) {
    if canonical_bytes(value)
        .expect("closed contract serialization")
        .len()
        > MAX_DOCUMENT_BYTES
    {
        out.push(Violation::new("next.document_budget", ""));
    }
}

impl NextDocument for AutomationPolicy {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if (self.allow_revise_created && self.rule_version == RuleVersion::V1)
            || (!self.allowed_forms.is_empty() && self.rule_version != RuleVersion::V3)
            || (self.rule_version == RuleVersion::V3 && self.allowed_forms.is_empty())
            || self.allowed_forms.iter().collect::<BTreeSet<_>>().len() != self.allowed_forms.len()
        {
            out.push(Violation::new("next.policy_rule_version", "/rule_version"));
        }
        document_budget(self, &mut out);
        if self.system_actor.actor_type != ActorType::System {
            out.push(Violation::new("next.system_actor", "/system_actor"));
        }
        bounded_unique(&self.source_ids, true, "/source_ids", &mut out);
        bounded_unique(&self.target_ids, false, "/target_ids", &mut out);
        if self.valid_from >= self.valid_until {
            out.push(Violation::new("next.policy_time", "/valid_until"));
        }
        out
    }
}

impl NextDocument for AdmissionInput {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        document_budget(self, &mut out);
        if let Some(fields) = &self.record_fields {
            out.extend(fields.validate_input(self));
        }
        if let Some(dimensions) = &self.dimensions {
            out.extend(dimensions.validate());
        }
        if self.actor.actor_type != ActorType::System {
            out.push(Violation::new("next.system_actor", "/actor"));
        }
        if self.content.trim().is_empty() || self.content.len() > 16 * 1024 {
            out.push(Violation::new("next.content", "/content"));
        }
        for (path, value) in [
            ("/producer_version", Some(&self.producer_version)),
            ("/model_version", self.model_version.as_ref()),
        ] {
            if value.is_some_and(|v| v.trim().is_empty() || v.len() > 128) {
                out.push(Violation::new("next.producer_version", path));
            }
        }
        bounded_unique(&self.input_refs, true, "/input_refs", &mut out);
        bounded_unique(
            &self.context_source_refs,
            false,
            "/context_source_refs",
            &mut out,
        );
        if self.evidence.is_empty() || self.evidence.len() > MAX_SOURCES {
            out.push(Violation::new("next.evidence_required", "/evidence"));
        }
        let mut support_ids = BTreeSet::new();
        for evidence in &self.evidence {
            evidence.locator.validate("/evidence/locator", &mut out);
            if evidence.supports != "content" {
                out.push(Violation::new("next.evidence_claim", "/evidence/supports"));
            }
            if !support_ids.insert(&evidence.source_id) {
                out.push(Violation::new("next.duplicate_support", "/evidence"));
            }
        }
        out
    }
}

impl NextDocument for DecisionDraft {
    fn validate(&self) -> Vec<Violation> {
        let mut out = self.input.validate();
        document_budget(self, &mut out);
        if self.input.dimensions.is_some() && self.rule_version != RuleVersion::V3 {
            out.push(Violation::new("formation.rule_version", "/rule_version"));
        }
        if self.outcome != outcome(self.input.interpretation) {
            out.push(Violation::new("next.outcome", "/outcome"));
        }
        out
    }
}

impl NextDocument for PlannedCommit {
    fn validate(&self) -> Vec<Violation> {
        let mut out = Vec::new();
        if self.planned_commit_id == self.snapshot.base_commit_id {
            out.push(Violation::new("next.commit_identity", "/planned_commit_id"));
        }
        out
    }
}

pub(super) fn outcome(interpretation: Interpretation) -> Outcome {
    let knowledge = match interpretation {
        Interpretation::ClearSelfReport => KnowledgeState::AttributedStatement,
        Interpretation::Inference => KnowledgeState::Hypothesis,
        Interpretation::Ambiguous => KnowledgeState::Uncertain,
        Interpretation::Conflicting => KnowledgeState::Disputed,
    };
    let adoption = if interpretation == Interpretation::ClearSelfReport {
        AdoptionState::Adopted
    } else {
        AdoptionState::Staged
    };
    Outcome {
        knowledge,
        adoption,
        reassessment: (adoption == AdoptionState::Staged)
            .then_some(Reassessment::OnEvidenceOrPolicyChange),
    }
}

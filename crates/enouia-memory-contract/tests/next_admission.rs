//! Synthetic Core-1 only. No Vault, Provider, Runtime or native authentication.
mod support;

use enouia_memory_contract::common::{
    ActorRef, ActorType, EvidenceClass, Sensitivity, SourceRevisionRef, SpeakerRole,
};
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::{MemoryId, OperationId, PrincipalId, SourceId, SubjectId};
use enouia_memory_contract::json::{Revision, canonical_bytes};
use enouia_memory_contract::next::*;
use enouia_memory_contract::source::{Completeness, SourceRecord};
use enouia_memory_contract::time::Timestamp;
use enouia_memory_contract::{ContractError, RecordKind, parse_any};
use serde_json::{Value, json};
use support::schema::SchemaStore;

fn fixture(name: &str) -> Value {
    support::fixture(&format!("../next/{name}.json"))
}
fn parse(kind: &str, value: &Value) -> Result<Value, ContractError> {
    let bytes = serde_json::to_vec(value).unwrap();
    match kind {
        "policy" => {
            parse_document::<AutomationPolicy>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        "input" => {
            parse_document::<AdmissionInput>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        "decision" => {
            parse_document::<DecisionDraft>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        "commit" => {
            parse_document::<PlannedCommit>(&bytes).map(|v| serde_json::to_value(v).unwrap())
        }
        _ => panic!("unknown fixture kind"),
    }
}
fn rule<T: std::fmt::Debug>(result: Result<T, ContractError>, expected: &str) {
    let error = result.unwrap_err();
    assert!(
        error.rules().contains(&expected),
        "expected {expected}, got {error}"
    );
}
fn rev(n: u64) -> Revision {
    Revision::new(n).unwrap()
}

#[derive(Clone)]
struct World {
    input: AdmissionInput,
    policy: AutomationPolicy,
    actor: ActorRef,
    snapshot: SnapshotBinding,
    now: Timestamp,
    sources: Vec<SourceRecord>,
    readable: bool,
    deleted: bool,
    attributed: bool,
    targets: Vec<TargetView>,
}

impl World {
    fn new() -> Self {
        let input: AdmissionInput = serde_json::from_value(fixture("input")).unwrap();
        Self {
            policy: serde_json::from_value(fixture("policy")).unwrap(),
            actor: input.actor.clone(),
            snapshot: input.snapshot.clone(),
            input,
            now: Timestamp::parse("2026-10-24T12:00:00.000Z").unwrap(),
            sources: serde_json::from_value(fixture("sources")).unwrap(),
            readable: true,
            deleted: false,
            attributed: true,
            targets: Vec::new(),
        }
    }
    fn with_context<T>(&self, f: impl FnOnce(&EvaluationContext<'_>) -> T) -> T {
        let sources: Vec<_> = self
            .sources
            .iter()
            .map(|record| SourceView {
                record,
                attributed_subject: self.attributed.then(|| self.input.subject_id.clone()),
                readable: self.readable,
                deleted: self.deleted,
            })
            .collect();
        f(&EvaluationContext {
            system_actor: &self.actor,
            policy: &self.policy,
            snapshot: &self.snapshot,
            now: &self.now,
            sources: &sources,
            targets: &self.targets,
        })
    }
    fn evaluate(&self) -> Result<DecisionDraft, ContractError> {
        self.with_context(|context| evaluate(&self.input, context))
    }
    fn rebind_policy(&mut self) {
        self.input.policy = self.policy.reference();
    }
    fn target(&mut self) {
        let memory = MemoryRevisionRef {
            memory_id: MemoryId::from_random([2; 16]),
            revision: rev(2),
        };
        let target = TargetRef {
            memory: memory.clone(),
            hash: sha256(b"synthetic target revision"),
        };
        self.input.target = Some(target.clone());
        self.input.result = MemoryRevisionRef {
            revision: rev(3),
            ..memory.clone()
        };
        self.policy.target_ids = vec![memory.memory_id];
        self.rebind_policy();
        self.targets = vec![TargetView {
            created_under_policy: None,
            target,
            legacy_owner_record: false,
            subject_id: self.input.subject_id.clone(),
            project_id: self.input.project_id.clone(),
            sensitivity: self.input.sensitivity,
            access_policy_id: self.input.access_policy_id.clone(),
            egress_policy_id: self.input.egress_policy_id.clone(),
            writable: true,
            deleted: false,
        }];
    }
}

#[test]
fn schemas_and_native_validators_agree_on_versioned_documents() {
    let schemas = SchemaStore::load();
    let manifest = fixture("manifest");
    for entry in manifest["valid"].as_array().unwrap() {
        let name = entry["type"].as_str().unwrap();
        let value = fixture(name);
        assert!(
            schemas
                .validate("next/automation-v2.schema.json", &value)
                .is_empty(),
            "{name}"
        );
        assert_eq!(parse(name, &value).unwrap(), value);
    }
    for case in manifest["invalid"].as_array().unwrap() {
        let name = case["base"].as_str().unwrap().trim_end_matches(".json");
        let mut value = fixture(name);
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/automation-v2.schema.json", &value)
                .is_empty(),
            case["schema"] == "accept",
            "{}",
            case["id"]
        );
        rule(parse(name, &value), case["rust_rule"].as_str().unwrap());
    }
}

#[test]
fn self_report_is_attributed_and_adopted_without_owner_or_truth_claim() {
    let world = World::new();
    assert_eq!(world.policy.reference(), world.input.policy);
    let draft = world.evaluate().unwrap();
    assert_eq!(serde_json::to_value(&draft).unwrap(), fixture("decision"));
    assert_eq!(draft.outcome.knowledge, KnowledgeState::AttributedStatement);
    assert_eq!(draft.outcome.adoption, AdoptionState::Adopted);
    assert_eq!(draft.input.actor.actor_type, ActorType::System);
    let commit: PlannedCommit = serde_json::from_value(fixture("commit")).unwrap();
    world
        .with_context(|ctx| validate_commit_binding(&draft, &commit, ctx))
        .unwrap();
}

#[test]
fn uncertainty_is_system_staged_and_can_be_reevaluated_without_review() {
    let mut world = World::new();
    for (interpretation, knowledge) in [
        (Interpretation::Inference, KnowledgeState::Hypothesis),
        (Interpretation::Ambiguous, KnowledgeState::Uncertain),
        (Interpretation::Conflicting, KnowledgeState::Disputed),
    ] {
        world.input.interpretation = interpretation;
        let draft = world.evaluate().unwrap();
        assert_eq!(
            draft.outcome,
            Outcome {
                knowledge,
                adoption: AdoptionState::Staged,
                reassessment: Some(Reassessment::OnEvidenceOrPolicyChange)
            }
        );
    }
    world.input.interpretation = Interpretation::ClearSelfReport;
    assert_eq!(
        world.evaluate().unwrap().outcome.adoption,
        AdoptionState::Adopted
    );
}

#[test]
fn actor_identity_is_bound_to_actual_system_context_and_policy() {
    let world = World::new();
    for actor_type in [
        ActorType::Owner,
        ActorType::Agent,
        ActorType::Provider,
        ActorType::Device,
        ActorType::Client,
    ] {
        let mut w = world.clone();
        w.input.actor.actor_type = actor_type;
        rule(w.evaluate(), "next.system_actor");
        let mut w = world.clone();
        w.actor.actor_type = actor_type;
        rule(w.evaluate(), "next.system_actor");
        let mut w = world.clone();
        w.policy.system_actor.actor_type = actor_type;
        w.rebind_policy();
        rule(w.evaluate(), "next.system_actor");
    }
    let mut w = world.clone();
    w.actor.actor_id = PrincipalId::from_random([9; 16]);
    rule(w.evaluate(), "next.system_actor");
    let mut w = world;
    w.policy.system_actor.actor_id = PrincipalId::from_random([9; 16]);
    w.rebind_policy();
    rule(w.evaluate(), "next.system_actor");
}

#[test]
fn policy_binding_scope_revocation_and_time_are_enforced() {
    let world = World::new();
    let mut w = world.clone();
    w.policy.revision = rev(2);
    rule(w.evaluate(), "next.policy_binding");
    let mut w = world.clone();
    w.policy.active = false;
    w.rebind_policy();
    rule(w.evaluate(), "next.policy_inactive");
    let mut w = world.clone();
    w.now = w.policy.valid_until.clone();
    rule(w.evaluate(), "next.policy_inactive");
    let mut w = world.clone();
    w.now = Timestamp::parse("2026-10-23T00:00:00.000Z").unwrap();
    rule(w.evaluate(), "next.policy_inactive");
    let mut w = world.clone();
    w.input.subject_id = SubjectId::from_random([8; 16]);
    rule(w.evaluate(), "next.policy_scope");
    let mut w = world.clone();
    w.policy.max_sensitivity = Sensitivity::Normal;
    w.rebind_policy();
    rule(w.evaluate(), "next.policy_scope");
    let mut w = world.clone();
    w.policy.source_ids.pop();
    w.rebind_policy();
    rule(w.evaluate(), "next.policy_scope");
    let mut w = world.clone();
    w.policy.allow_create = false;
    w.rebind_policy();
    rule(w.evaluate(), "next.policy_scope");
    let mut w = world;
    w.input.egress_policy_id = w.input.access_policy_id.clone();
    rule(w.evaluate(), "next.policy_scope");
}

#[test]
fn exact_snapshot_and_policy_deletion_primary_barriers_are_not_optional() {
    for field in [
        "base_commit_id",
        "primary_epoch",
        "policy_watermark",
        "deletion_watermark",
        "vault_id",
    ] {
        let mut w = World::new();
        let mut snapshot = serde_json::to_value(&w.snapshot).unwrap();
        snapshot[field] = if field.ends_with("_id") {
            let prefix = if field == "vault_id" { "vlt" } else { "cmt" };
            json!(format!("{prefix}_00000009-0000-4000-8000-000000000001"))
        } else {
            json!(2)
        };
        w.snapshot = serde_json::from_value(snapshot).unwrap();
        rule(w.evaluate(), "next.snapshot_stale");
    }
}

#[test]
fn source_and_context_closure_exact_revisions_and_access_are_required() {
    let world = World::new();
    let mut w = world.clone();
    w.input.context_source_refs.clear();
    rule(w.evaluate(), "next.dependency_closure");
    let mut w = world.clone();
    w.input.input_refs.pop();
    rule(w.evaluate(), "next.dependency_closure");
    let mut w = world.clone();
    w.sources.pop();
    rule(w.evaluate(), "next.source_missing");
    for index in [0, 1] {
        let mut w = world.clone();
        w.sources[index].revision = rev(2);
        rule(w.evaluate(), "next.source_stale");
        let mut w = world.clone();
        w.sources[index].completeness = Completeness::MetadataOnly;
        rule(w.evaluate(), "next.source_unavailable");
        let mut w = world.clone();
        w.sources[index].access_policy_id = w.policy.policy_id.clone();
        rule(w.evaluate(), "next.source_policy");
    }
    let mut w = world.clone();
    w.deleted = true;
    rule(w.evaluate(), "next.source_unavailable");
    let mut w = world.clone();
    w.readable = false;
    rule(w.evaluate(), "next.source_unavailable");
    let mut w = world;
    w.sources.push(w.sources[0].clone());
    rule(w.evaluate(), "next.ambiguous_view");
}

#[test]
fn context_inherits_sensitivity_without_becoming_support() {
    let mut w = World::new();
    w.sources[0].sensitivity = Sensitivity::Normal;
    w.input.sensitivity = Sensitivity::Normal;
    rule(w.evaluate(), "next.sensitivity_floor");
    w.input.sensitivity = Sensitivity::Private;
    assert_eq!(w.evaluate().unwrap().input.evidence.len(), 1);
}

#[test]
fn evidence_anchor_class_locator_and_subject_cannot_be_forged() {
    let world = World::new();
    let mut w = world.clone();
    w.input.evidence[0].object_hash = sha256(b"foreign bytes");
    rule(w.evaluate(), "next.evidence_anchor");
    let mut w = world.clone();
    w.input.evidence[0].locator = enouia_memory_contract::common::Locator::ManualInput;
    rule(w.evaluate(), "next.evidence_anchor");
    let mut w = world.clone();
    w.input.evidence[0].evidence_class = EvidenceClass::UserConfirmation;
    rule(w.evaluate(), "next.evidence_anchor");
    let mut w = world.clone();
    w.attributed = false;
    rule(w.evaluate(), "next.self_report_support");
    let mut w = world.clone();
    w.sources[0].speaker_role = SpeakerRole::Assistant;
    w.sources[0].evidence_class = EvidenceClass::ModelClaim;
    w.input.evidence[0].evidence_class = EvidenceClass::ModelClaim;
    rule(w.evaluate(), "next.self_report_support");
    w.input.interpretation = Interpretation::Inference;
    assert_eq!(
        w.evaluate().unwrap().outcome.adoption,
        AdoptionState::Staged
    );
}

#[test]
fn repeated_sources_copies_and_upstream_messages_do_not_multiply_support() {
    let mut w = World::new();
    w.input.evidence.push(w.input.evidence[0].clone());
    rule(w.evaluate(), "next.duplicate_support");
    for same_content in [true, false] {
        let mut w = World::new();
        let mut copy = w.sources[0].clone();
        copy.source_id = SourceId::from_random([7; 16]);
        if !same_content {
            copy.content_hash = sha256(b"rewrapped same upstream message");
        }
        let dependency = SourceRevisionRef {
            source_id: copy.source_id.clone(),
            source_revision: copy.revision,
        };
        let mut evidence = w.input.evidence[0].clone();
        evidence.source_id = copy.source_id.clone();
        w.input.evidence.push(evidence);
        w.input.input_refs.push(dependency);
        w.policy.source_ids.push(copy.source_id.clone());
        w.rebind_policy();
        w.sources.push(copy);
        rule(w.evaluate(), "next.duplicate_origin");
    }
}

#[test]
fn targets_require_exact_machine_revision_scope_and_next_result() {
    let mut world = World::new();
    world.target();
    world.evaluate().unwrap();
    let mut w = world.clone();
    w.input.target.as_mut().unwrap().memory.revision = rev(1);
    rule(w.evaluate(), "next.target_stale");
    let mut w = world.clone();
    w.input.target.as_mut().unwrap().hash = sha256(b"stale");
    rule(w.evaluate(), "next.target_stale");
    let mut w = world.clone();
    w.targets[0].legacy_owner_record = true;
    rule(w.evaluate(), "next.target_unavailable");
    let mut w = world.clone();
    w.targets[0].deleted = true;
    rule(w.evaluate(), "next.target_unavailable");
    let mut w = world.clone();
    w.targets[0].writable = false;
    rule(w.evaluate(), "next.target_unavailable");
    let mut w = world.clone();
    w.targets[0].subject_id = SubjectId::from_random([8; 16]);
    rule(w.evaluate(), "next.target_scope");
    let mut w = world.clone();
    w.input.result.revision = rev(4);
    rule(w.evaluate(), "next.result_revision");
    let mut w = world.clone();
    w.policy.target_ids.clear();
    w.rebind_policy();
    rule(w.evaluate(), "next.policy_scope");
    let mut w = world;
    w.input.target = None;
    w.input.result.revision = rev(1);
    rule(w.evaluate(), "next.result_revision");
}

#[test]
fn commit_association_revalidates_input_and_never_returns_a_receipt() {
    let world = World::new();
    let draft = world.evaluate().unwrap();
    let commit: PlannedCommit = serde_json::from_value(fixture("commit")).unwrap();
    let mut bad = commit.clone();
    bad.decision_hash = sha256(b"another draft");
    rule(
        world.with_context(|ctx| validate_commit_binding(&draft, &bad, ctx)),
        "next.commit_binding",
    );
    let mut bad = commit.clone();
    bad.operation_id = OperationId::from_random([8; 16]);
    rule(
        world.with_context(|ctx| validate_commit_binding(&draft, &bad, ctx)),
        "next.commit_binding",
    );
    let mut bad = commit.clone();
    bad.result.revision = rev(2);
    rule(
        world.with_context(|ctx| validate_commit_binding(&draft, &bad, ctx)),
        "next.commit_binding",
    );
    let mut bad = commit.clone();
    bad.snapshot.deletion_watermark = rev(2);
    rule(
        world.with_context(|ctx| validate_commit_binding(&draft, &bad, ctx)),
        "next.commit_binding",
    );
    let mut changed = draft.clone();
    changed.input.content.push_str(" changed");
    rule(
        world.with_context(|ctx| validate_commit_binding(&changed, &commit, ctx)),
        "next.commit_binding",
    );
    let mut w = world;
    w.policy.active = false;
    w.rebind_policy();
    rule(
        w.with_context(|ctx| validate_commit_binding(&draft, &commit, ctx)),
        "next.policy_binding",
    );
}

#[test]
fn bounded_parser_rejects_duplicate_keys_unknown_fields_and_versions() {
    let bytes = serde_json::to_string(&fixture("input")).unwrap();
    let duplicate = bytes.replacen(
        "\"schema_version\":2",
        "\"schema_version\":2,\"schema_version\":2",
        1,
    );
    rule(
        parse_document::<AdmissionInput>(duplicate.as_bytes()),
        "shape",
    );
    rule(
        parse_document::<AdmissionInput>(&vec![b' '; MAX_DOCUMENT_BYTES + 1]),
        "next.document_budget",
    );
    for major in [0, 1, 3, 9007199254740991i64] {
        let mut value = fixture("input");
        value["schema_version"] = json!(major);
        rule(parse("input", &value), "unsupported_schema");
    }
    let mut value = fixture("input");
    value["input_refs"][0]["source_revision"] = json!(9007199254740992u64);
    assert!(parse("input", &value).is_err());
}

#[test]
fn legacy_owner_records_and_raw_unknown_major_are_preserved_not_adopted() {
    let world = World::new();
    for (file, kind, actor_field) in [
        ("memory-fact", RecordKind::Memory, "approved_by"),
        ("review-accept", RecordKind::Review, "actor"),
    ] {
        let original = support::fixture(&format!("records/{file}.json"));
        let before = canonical_bytes(&original).unwrap();
        let record = parse_any(kind, &original).unwrap();
        world.evaluate().unwrap();
        assert_eq!(canonical_bytes(&record.to_value()).unwrap(), before);
        let mut forged = original.clone();
        forged[actor_field]["actor_type"] = json!("system");
        assert!(parse_any(kind, &forged).is_err());
    }
    assert_eq!(enouia_memory_contract::json::SCHEMA_VERSION, 1);
    let raw = b"{ \"schema_version\": 2, \"kind\": \"admission_draft\" }\n";
    let value: Value = serde_json::from_slice(raw).unwrap();
    assert_eq!(
        enouia_memory_contract::json::schema_disposition(&value),
        enouia_memory_contract::json::SchemaDisposition::UnknownMajorReadOnly(2)
    );
    rule(
        enouia_memory_contract::json::ensure_writable(&value),
        "unsupported_schema",
    );
    rule(parse_any(RecordKind::Memory, &value), "unsupported_schema");
    let mut next_as_v1 = fixture("decision");
    next_as_v1["schema_version"] = json!(1);
    assert!(parse_any(RecordKind::Memory, &next_as_v1).is_err());
}

#[test]
fn legacy_descriptor_and_current_refuse_new_major_before_record_decoding() {
    use enouia_memory_contract::store::{CurrentPointer, VaultDescriptor, parse_store};
    let mut descriptor = support::fixture("../store/descriptor.json");
    parse_store::<VaultDescriptor>(&canonical_bytes(&descriptor).unwrap()).unwrap();
    descriptor["schema_version"] = json!(2);
    rule(
        parse_store::<VaultDescriptor>(&canonical_bytes(&descriptor).unwrap()),
        "unsupported_schema",
    );
    let mut current = support::fixture("../store/current.json");
    parse_store::<CurrentPointer>(&canonical_bytes(&current).unwrap()).unwrap();
    current["schema_version"] = json!(2);
    rule(
        parse_store::<CurrentPointer>(&canonical_bytes(&current).unwrap()),
        "unsupported_schema",
    );
    assert!(serde_json::from_value::<RecordKind>(json!("admission_draft")).is_err());
}

#[test]
fn revise_created_scope_requires_native_policy_provenance_and_never_owner_records() {
    let mut w = World::new();
    w.target();
    w.policy.target_ids.clear();
    w.policy.allow_revise_created = true;
    w.policy.rule_version = RuleVersion::V2;
    w.targets[0].created_under_policy = Some(w.policy.policy_id.clone());
    w.rebind_policy();
    w.evaluate().unwrap();
    w.targets[0].legacy_owner_record = true;
    rule(w.evaluate(), "next.target_unavailable");
    w.targets[0].legacy_owner_record = false;
    w.targets[0].created_under_policy = None;
    rule(w.evaluate(), "next.policy_scope");
    w.targets[0].created_under_policy = Some(w.policy.policy_id.clone());
    w.policy.allow_revise_created = false;
    w.rebind_policy();
    rule(w.evaluate(), "next.policy_scope");
}

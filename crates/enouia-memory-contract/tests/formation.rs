mod support;
#[test]
fn independent_key_units_compare_representation_and_detect_an_omitted_output() {
    let units = fixture("key-units")["units"].as_array().unwrap().clone();
    let outputs = fixture("scripted-output")["outputs"]
        .as_array()
        .unwrap()
        .clone();
    let missing = |outputs: &[Value]| {
        units
            .iter()
            .filter(|unit| {
                !outputs.iter().any(|output| {
                    output["unit_id"] == unit["id"]
                        && output["dimensions"]["kind"] == unit["required_kind"]
                        && output["dimensions"]["state"] == unit["required_state"]
                        && output["dimensions"]["conditions"] == unit["required_conditions"]
                        && output["dimensions"]["event_time"] == unit["required_event_time"]
                })
            })
            .count()
    };
    assert_eq!(units.len(), 9);
    assert_eq!(missing(&[]), 9); // Core-4 has no typed dimensional output.
    assert_eq!(missing(&outputs), 0);
    assert_eq!(missing(&outputs[1..]), 1); // Denominator does not shrink to output count.
    let mut wrong = outputs.clone();
    wrong[1]["dimensions"]["state"] = serde_json::json!("completed");
    assert_eq!(missing(&wrong), 1);
}
use enouia_memory_contract::{
    common::*,
    hash::sha256,
    ids::*,
    json::{Revision, canonical_bytes},
    next::*,
    time::Timestamp,
};
use serde_json::Value;
use support::schema::SchemaStore;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../formation/{name}.json"))
}
fn dimensions(kind: FormationKind, state: Option<FormationState>) -> FormationDimensions {
    let mut d: FormationDimensions = serde_json::from_value(fixture("dimensions")).unwrap();
    d.kind = kind;
    d.state = state;
    d.conditions.clear();
    d
}
struct World {
    data: AutomationData,
    input: AdmissionInput,
    snapshot: SnapshotBinding,
    actor: ActorRef,
    now: Timestamp,
}
impl World {
    fn new() -> Self {
        let captured: AutomationCommit =
            serde_json::from_value(support::fixture("../automation-vault/capture.json")).unwrap();
        let admitted: AutomationCommit =
            serde_json::from_value(support::fixture("../automation-vault/admitted.json")).unwrap();
        let mut data = captured.data;
        data.policy = serde_json::from_value(fixture("policy")).unwrap();
        let mut source = data.sources[0].clone();
        source.record.source_id = SourceId::from_random([2; 16]);
        source.record.content_hash = sha256(b"A new independent correction.");
        source.text = "A new independent correction.".into();
        source.record.locator = Locator::RuntimeEvent {
            event_id: EventId::from_random([2; 16]),
        };
        data.policy.source_ids.push(source.record.source_id.clone());
        data.sources.push(source);
        let mut input = admitted.data.memories[0].decision.input.clone();
        input.policy = data.policy.reference();
        input.dimensions = Some(dimensions(FormationKind::Preference, None));
        Self {
            snapshot: input.snapshot.clone(),
            actor: data.policy.system_actor.clone(),
            data,
            input,
            now: Timestamp::from_unix_ms(1_792_843_200_000).unwrap(),
        }
    }
    fn evaluate(&self) -> Result<DecisionDraft, enouia_memory_contract::ContractError> {
        with_stored_context(&self.data, &self.snapshot, &self.actor, &self.now, |ctx| {
            evaluate(&self.input, ctx)
        })
    }
    fn add(&mut self) -> TargetRef {
        let decision = self.evaluate().unwrap();
        let response_json = serde_json::to_string(&decision).unwrap();
        let memory = MachineMemory {
            schema_version: NextVersion,
            kind: MachineKind::MachineMemory,
            memory: decision.input.result.clone(),
            request: decision.input.clone(),
            response_hash: sha256(response_json.as_bytes()),
            response_json,
            decision,
            committed_in: self.snapshot.base_commit_id.clone(),
        };
        let target = memory.target();
        self.data.memories.push(memory);
        target
    }
    fn revise(&mut self, target: TargetRef, source: usize) {
        self.input.target = Some(target.clone());
        self.input.result = target.memory;
        self.input.result.revision = Revision::new(self.input.result.revision.get() + 1).unwrap();
        self.input.operation_id =
            OperationId::from_random([self.input.result.revision.get() as u8; 16]);
        self.select(source);
    }
    fn select(&mut self, index: usize) {
        let s = &self.data.sources[index];
        self.input.input_refs = vec![s.reference()];
        self.input.context_source_refs.clear();
        self.input.evidence = vec![EvidenceRef {
            source_id: s.record.source_id.clone(),
            source_revision: s.record.revision,
            locator: s.record.locator.clone(),
            object_hash: s.record.anchor_hash().clone(),
            evidence_class: s.record.evidence_class,
            supports: "content".into(),
        }];
    }
}
#[test]
fn dimensions_schema_and_native_negative_cases_agree_without_semantic_quality_claims() {
    let schemas = SchemaStore::load();
    let m = fixture("manifest");
    let base = fixture("dimensions");
    assert!(
        schemas
            .validate("next/formation-v2.schema.json", &base)
            .is_empty()
    );
    assert!(
        serde_json::from_value::<FormationDimensions>(base.clone())
            .unwrap()
            .validate()
            .is_empty()
    );
    for case in m["invalid"].as_array().unwrap() {
        let mut v = base.clone();
        support::apply_ops(&mut v, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/formation-v2.schema.json", &v)
                .is_empty(),
            case["schema"] == "accept",
            "{}",
            case["id"]
        );
        assert!(
            serde_json::from_value::<FormationDimensions>(v)
                .map_or(true, |d| !d.validate().is_empty()),
            "{}",
            case["id"]
        );
    }
    assert!(
        schemas
            .validate("next/automation-v2.schema.json", &fixture("policy"))
            .is_empty()
    );
}
#[test]
fn plans_pause_resume_cancel_and_cannot_turn_cancellation_into_completion_or_episode() {
    use FormationChange as C;
    use FormationState as S;
    let mut w = World::new();
    let mut d = dimensions(FormationKind::Plan, Some(S::Planned));
    d.occurrence = OccurrenceAssertion::Intended;
    w.input.dimensions = Some(d);
    let plan = w.add();
    w.revise(plan, 1);
    let d = w.input.dimensions.as_mut().unwrap();
    d.change = C::Pause;
    d.state = Some(S::Paused);
    let paused = w.add();
    w.revise(paused, 0);
    let d = w.input.dimensions.as_mut().unwrap();
    d.change = C::Resume;
    d.state = Some(S::Planned);
    let resumed = w.add();
    w.revise(resumed, 1);
    let d = w.input.dimensions.as_mut().unwrap();
    d.change = C::Cancellation;
    d.state = Some(S::Cancelled);
    let cancelled = w.add();
    w.revise(cancelled, 0);
    let d = w.input.dimensions.as_mut().unwrap();
    d.change = C::Completion;
    d.state = Some(S::Completed);
    assert!(w.evaluate().is_err());
    let d = w.input.dimensions.as_mut().unwrap();
    d.kind = FormationKind::Episode;
    d.state = None;
    assert!(w.evaluate().is_err());
}
#[test]
fn native_policy_rule_and_new_evidence_requirements_are_explicit() {
    let mut w = World::new();
    let t = w.add();
    w.revise(t, 0);
    w.input.dimensions.as_mut().unwrap().change = FormationChange::Correction;
    assert!(w.evaluate().is_err());
    w.select(1);
    assert!(w.evaluate().is_ok());
    w.data.policy.rule_version = RuleVersion::V2;
    w.input.policy = w.data.policy.reference();
    assert!(w.evaluate().is_err());
    let old: AutomationPolicy =
        serde_json::from_value(support::fixture("../next/policy.json")).unwrap();
    assert_eq!(
        serde_json::to_value(old).unwrap(),
        support::fixture("../next/policy.json")
    );
}
#[test]
fn unknown_occurrence_time_is_not_filled_from_capture_or_commit_and_future_event_refuses() {
    let mut w = World::new();
    let mut d = dimensions(FormationKind::Episode, None);
    d.occurrence = OccurrenceAssertion::ReportedOccurred;
    w.input.dimensions = Some(d);
    assert!(w.evaluate().is_ok());
    assert!(
        w.evaluate()
            .unwrap()
            .input
            .dimensions
            .unwrap()
            .event_time
            .is_none()
    );
    w.input.dimensions.as_mut().unwrap().event_time = Some(FormationTime {
        from: Timestamp::parse("2030-01-01T00:00:00.000Z").unwrap(),
        until: None,
    });
    assert!(w.evaluate().is_err());
}
#[test]
fn system_understanding_and_counter_evidence_cannot_become_subject_self_report() {
    let mut w = World::new();
    w.input.dimensions = Some(dimensions(FormationKind::SubjectiveUnderstanding, None));
    assert!(w.evaluate().is_err());
    w.input.interpretation = Interpretation::Inference;
    assert_eq!(
        w.evaluate().unwrap().outcome.knowledge,
        KnowledgeState::Hypothesis
    );
    assert_eq!(
        w.evaluate().unwrap().outcome.adoption,
        AdoptionState::Staged
    );
    w.input.dimensions.as_mut().unwrap().counter_sources = vec![w.data.sources[1].reference()];
    w.input.interpretation = Interpretation::Conflicting;
    assert!(w.evaluate().is_err());
    w.input.input_refs.push(w.data.sources[1].reference());
    w.input.context_source_refs = vec![w.data.sources[1].reference()];
    assert_eq!(
        w.evaluate().unwrap().outcome.knowledge,
        KnowledgeState::Disputed
    );
}
#[test]
fn synthesis_requires_exact_current_parents_original_sources_and_preserves_hypothesis() {
    let mut w = World::new();
    let parent = w.add();
    w.input.target = None;
    w.input.result.memory_id = MemoryId::from_random([7; 16]);
    w.input.result.revision = Revision::new(1).unwrap();
    let mut d = dimensions(FormationKind::Theme, None);
    d.change = FormationChange::Synthesis;
    d.derived_from = vec![parent.clone()];
    w.input.dimensions = Some(d);
    w.input.interpretation = Interpretation::Inference;
    assert_eq!(
        w.evaluate().unwrap().outcome.adoption,
        AdoptionState::Staged
    );
    w.select(1);
    assert!(w.evaluate().is_err());
    w.select(0);
    w.input.dimensions.as_mut().unwrap().derived_from[0].hash = sha256(b"stale parent");
    assert!(w.evaluate().is_err());
    w.input.dimensions.as_mut().unwrap().derived_from = vec![parent.clone(), parent];
    assert!(w.evaluate().is_err());
}
#[test]
fn reversion_is_a_new_exact_historical_claim_and_cannot_restore_deleted_support() {
    let mut w = World::new();
    let first = w.add();
    let original = w.input.clone();
    w.revise(first.clone(), 1);
    w.input.content = "A corrected preference.".into();
    w.input.dimensions.as_mut().unwrap().change = FormationChange::Correction;
    let current = w.add();
    w.revise(current, 0);
    w.input.content = original.content.clone();
    let mut d = original.dimensions.unwrap();
    d.change = FormationChange::Reversion;
    d.reverts_to = Some(first);
    w.input.dimensions = Some(d);
    assert!(w.evaluate().is_ok());
    w.input.content.push_str(" fabricated");
    assert!(w.evaluate().is_err());
    w.input.content = original.content;
    w.data
        .deleted_sources
        .push(w.input.input_refs[0].source_id.clone());
    assert!(w.evaluate().is_err());
    assert_eq!(w.data.memories.len(), 2);
}
#[test]
fn old_layout_cannot_write_rule_three_or_dimensional_memory() {
    let mut descriptor: AutomationDescriptor =
        serde_json::from_value(support::fixture("../automation-vault/descriptor.json")).unwrap();
    let mut data = World::new().data;
    data.sources.clear();
    let make = |descriptor: &AutomationDescriptor| {
        AutomationCommit::assemble(
            descriptor,
            None,
            descriptor.created_by.clone(),
            AutomationChange::Genesis,
            data.clone(),
            CommitId::from_random([4; 16]),
            Timestamp::from_unix_ms(1_792_843_200_000).unwrap(),
        )
    };
    assert!(make(&descriptor).is_err());
    descriptor.layout = AutomationLayout::HostV2;
    assert!(make(&descriptor).is_ok());
    let bytes = canonical_bytes(&descriptor).unwrap();
    assert!(
        String::from_utf8(bytes)
            .unwrap()
            .contains("automation-host-v2")
    );
}

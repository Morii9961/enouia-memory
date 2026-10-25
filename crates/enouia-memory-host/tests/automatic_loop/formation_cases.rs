use super::*;
#[test]
fn subjective_understanding_uses_continuous_fake_policy_and_never_becomes_user_self_report() {
    use enouia_memory_host::automation::continuation::ContinuousPolicyHost;
    let e = env();
    let jobs = e.jobs();
    let controller = ContinuousPolicyHost::open(
        jobs.clone(),
        e.descriptor.vault_id.clone(),
        e.descriptor.created_by.clone(),
        e.descriptor.system_actor.clone(),
        e.clock.clone(),
        Faults::none(),
    )
    .unwrap();
    let mut policy: ContinuationPolicy =
        serde_json::from_value(support::fixture("../continuation/policy.json")).unwrap();
    policy.admission_policy = e.policy.reference();
    policy.source_ids = e.policy.source_ids.clone();
    controller
        .set_policy(&e.descriptor.created_by, policy.clone())
        .unwrap();
    let host = e
        .host(e.vault(Faults::none()), jobs.clone())
        .with_continuation(controller.clone())
        .unwrap();
    host.capture_native(e.source(1, "I enjoyed trying a new lesson.", SpeakerRole::User))
        .unwrap();
    let state = host.inspect_native().unwrap();
    with_stored_context(
        &state.data,
        &state.snapshot(),
        &e.descriptor.system_actor,
        &Timestamp::from_unix_ms(NOW).unwrap(),
        |ctx| controller.activate(ctx),
    )
    .unwrap();
    let host = Arc::new(host);
    let generator = controller
        .fake_generator(policy.destination.clone(), DimensionalModel)
        .unwrap();
    let thread = host.start_worker(generator).unwrap();
    let mut input = e.input(1, None);
    input.model_version = Some(policy.destination.model);
    input.dimensions = Some(dims(
        FormationKind::SubjectiveUnderstanding,
        None,
        FormationChange::Initial,
    ));
    let key = host.prepare_submit(vec![input]).unwrap();
    let until = Instant::now() + Duration::from_secs(10);
    while host.receipts(&key).ok().flatten().is_none() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    let state = host.inspect_native().unwrap();
    let memory = &state.data.memories[0];
    assert_eq!(
        memory.decision.outcome.knowledge,
        KnowledgeState::Hypothesis
    );
    assert_eq!(memory.decision.outcome.adoption, AdoptionState::Staged);
    assert!(
        host.context("lesson", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    assert_eq!(controller.inspect_native().unwrap().attempts.len(), 1);
    drop(host);
    thread.join().unwrap();
}
#[test]
fn open_loop_does_not_complete_by_age_and_validity_excludes_current_use_without_deleting_history() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let mut dimensions = dims(
        FormationKind::OpenLoop,
        Some(FormationState::Open),
        FormationChange::Initial,
    );
    dimensions.valid_time = Some(FormationTime {
        from: Timestamp::from_unix_ms(NOW).unwrap(),
        until: Timestamp::from_unix_ms(NOW + 1000),
    });
    form(
        &e,
        &host,
        &jobs,
        1,
        "I still need to choose a class.",
        dimensions,
        None,
    );
    assert!(
        !host
            .context("class", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    e.clock.set(NOW + 1000);
    assert!(
        host.context("class", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    let state = host.inspect_native().unwrap();
    assert_eq!(
        state.data.memories[0]
            .decision
            .input
            .dimensions
            .as_ref()
            .unwrap()
            .state,
        Some(FormationState::Open)
    );
    assert_eq!(state.data.memories.len(), 1);
    drop(host);
    thread.join().unwrap();
}
pub(super) fn env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::HostV2;
    let policy: AutomationPolicy =
        serde_json::from_value(support::fixture("../formation/policy.json")).unwrap();
    e.policy.rule_version = RuleVersion::V3;
    e.policy.allowed_forms = policy.allowed_forms;
    e
}
pub(super) fn dims(
    kind: FormationKind,
    state: Option<FormationState>,
    change: FormationChange,
) -> FormationDimensions {
    FormationDimensions {
        kind,
        state,
        change,
        occurrence: if kind == FormationKind::Episode {
            OccurrenceAssertion::ReportedOccurred
        } else if kind == FormationKind::Plan {
            OccurrenceAssertion::Intended
        } else {
            OccurrenceAssertion::Unknown
        },
        conditions: vec![],
        event_time: None,
        valid_time: None,
        counter_sources: vec![],
        derived_from: vec![],
        reverts_to: None,
    }
}
pub(super) struct DimensionalModel;
struct DimensionalPending(AdmissionInput);
impl Generator for DimensionalModel {
    fn start(
        &mut self,
        input: &AdmissionInput,
        _: &Limits,
        _: &StopToken,
    ) -> std::result::Result<Box<dyn PendingGeneration>, Generation> {
        Ok(Box::new(DimensionalPending(input.clone())))
    }
}
impl PendingGeneration for DimensionalPending {
    fn poll(&mut self, _: &StopToken) -> Option<Generation> {
        let mut input = self.0.clone();
        let dimensions = input.dimensions.as_ref().unwrap();
        if dimensions.reverts_to.is_none() {
            let sources: Vec<CapturedSource> = serde_json::from_str(&input.content).unwrap();
            input.content = if matches!(
                dimensions.kind,
                FormationKind::Theme | FormationKind::SubjectiveUnderstanding
            ) {
                format!(
                    "System interpretation across sources: {}",
                    sources
                        .iter()
                        .map(|s| s.text.as_str())
                        .collect::<Vec<_>>()
                        .join(" / ")
                )
            } else {
                sources[0].text.clone()
            };
            input.interpretation = if !dimensions.counter_sources.is_empty() {
                Interpretation::Conflicting
            } else if matches!(
                dimensions.kind,
                FormationKind::Theme | FormationKind::SubjectiveUnderstanding
            ) {
                Interpretation::Inference
            } else {
                Interpretation::ClearSelfReport
            };
        }
        let (knowledge, adoption) = match input.interpretation {
            Interpretation::ClearSelfReport => {
                (KnowledgeState::AttributedStatement, AdoptionState::Adopted)
            }
            Interpretation::Inference => (KnowledgeState::Hypothesis, AdoptionState::Staged),
            Interpretation::Conflicting => (KnowledgeState::Disputed, AdoptionState::Staged),
            Interpretation::Ambiguous => (KnowledgeState::Uncertain, AdoptionState::Staged),
        };
        let decision = DecisionDraft {
            schema_version: NextVersion,
            kind: DecisionKind::AdmissionDraft,
            rule_version: RuleVersion::V3,
            input,
            outcome: Outcome {
                knowledge,
                adoption,
                reassessment: (adoption == AdoptionState::Staged)
                    .then_some(Reassessment::OnEvidenceOrPolicyChange),
            },
        };
        Some(Generation::Complete(Box::new(Generated {
            input_hash: digest(&self.0),
            response_json: serde_json::to_string(&decision).unwrap(),
            tokens_used: 5,
            cost_microusd: 1,
        })))
    }
}
pub(super) fn form(
    e: &Env,
    host: &AutomationHost,
    jobs: &SyntheticJobs,
    n: u8,
    text: &str,
    d: FormationDimensions,
    target: Option<TargetRef>,
) -> MachineMemory {
    host.capture_native(e.source(n, text, SpeakerRole::User))
        .unwrap();
    let mut input = e.input(n, target);
    input.dimensions = Some(d);
    let key = host.prepare_submit(vec![input]).unwrap();
    wait(jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    host.inspect_native()
        .unwrap()
        .data
        .memories
        .last()
        .unwrap()
        .clone()
}
#[test]
fn plan_pause_resume_cancellation_and_reported_episode_remain_distinct_after_reopen() {
    use FormationChange as C;
    use FormationKind as K;
    use FormationState as S;
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let p = form(
        &e,
        &host,
        &jobs,
        1,
        "I plan to take a pottery class.",
        dims(K::Plan, Some(S::Planned), C::Initial),
        None,
    );
    let p = form(
        &e,
        &host,
        &jobs,
        2,
        "I paused the pottery plan for the summer.",
        dims(K::Plan, Some(S::Paused), C::Pause),
        Some(p.target()),
    );
    let p = form(
        &e,
        &host,
        &jobs,
        3,
        "I resumed the pottery plan; no class has happened yet.",
        dims(K::Plan, Some(S::Planned), C::Resume),
        Some(p.target()),
    );
    let p = form(
        &e,
        &host,
        &jobs,
        4,
        "I cancelled the pottery plan without attending.",
        dims(K::Plan, Some(S::Cancelled), C::Cancellation),
        Some(p.target()),
    );
    let episode = form(
        &e,
        &host,
        &jobs,
        5,
        "Some time ago I made a cup in a different class and felt nervous.",
        dims(K::Episode, None, C::Initial),
        None,
    );
    assert!(
        episode
            .decision
            .input
            .dimensions
            .as_ref()
            .unwrap()
            .event_time
            .is_none()
    );
    assert_eq!(p.memory.revision.get(), 4);
    assert!(
        host.context("pottery", Sensitivity::Private, 8)
            .unwrap()
            .contains("cancelled")
    );
    let state = host.inspect_native().unwrap();
    assert_eq!(
        state
            .data
            .memories
            .iter()
            .filter(|m| m.decision.input.dimensions.as_ref().unwrap().kind == K::Episode)
            .count(),
        1
    );
    assert_eq!(state.created_at, Timestamp::from_unix_ms(NOW).unwrap());
    drop(host);
    thread.join().unwrap();
    drop(jobs);
    let jobs = e.jobs();
    let host = e.host(e.open(), jobs);
    assert!(
        host.context("cup", Sensitivity::Private, 8)
            .unwrap()
            .contains("felt nervous")
    );
    assert_eq!(host.inspect_native().unwrap().data.memories.len(), 5);
}
#[test]
fn project_state_progression_preserves_every_explicit_milestone() {
    use FormationState as S;
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let mut target = None;
    for (index, state) in [
        S::Planned,
        S::Decided,
        S::Implemented,
        S::Tested,
        S::Released,
    ]
    .into_iter()
    .enumerate()
    {
        let memory = form(
            &e,
            &host,
            &jobs,
            index as u8 + 1,
            &format!("Project step explicitly reported: {state:?}."),
            dims(
                FormationKind::ProjectState,
                Some(state),
                if index == 0 {
                    FormationChange::Initial
                } else {
                    FormationChange::Update
                },
            ),
            target,
        );
        target = Some(memory.target());
    }
    let state = host.inspect_native().unwrap();
    assert_eq!(state.data.memories.len(), 5);
    assert_eq!(
        state
            .data
            .memories
            .last()
            .unwrap()
            .decision
            .input
            .dimensions
            .as_ref()
            .unwrap()
            .state,
        Some(S::Released)
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn correction_and_reversion_create_new_revisions_with_original_conditions_and_sources() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let mut first_dims = dims(FormationKind::Preference, None, FormationChange::Initial);
    first_dims.conditions = vec!["During debugging only".into()];
    let first = form(
        &e,
        &host,
        &jobs,
        1,
        "During debugging, explain the details.",
        first_dims,
        None,
    );
    let current = form(
        &e,
        &host,
        &jobs,
        2,
        "Correction: keep debugging answers short too.",
        dims(FormationKind::Preference, None, FormationChange::Correction),
        Some(first.target()),
    );
    let mut reverted = first.decision.input.dimensions.clone().unwrap();
    reverted.change = FormationChange::Reversion;
    reverted.reverts_to = Some(first.target());
    let mut input = e.input(1, Some(current.target()));
    input.dimensions = Some(reverted);
    let key = host.prepare_submit(vec![input]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    let state = host.inspect_native().unwrap();
    let last = state.data.memories.last().unwrap();
    assert_eq!(last.memory.revision.get(), 3);
    assert_eq!(last.decision.input.content, first.decision.input.content);
    assert_eq!(
        last.decision.input.dimensions.as_ref().unwrap().conditions,
        vec!["During debugging only"]
    );
    assert_eq!(state.data.memories[0], first);
    assert_eq!(state.data.sources.len(), 2);
    drop(host);
    thread.join().unwrap();
}
#[test]
fn synthesis_and_repeated_reflection_keep_original_sources_without_independent_self_support() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    let first = form(
        &e,
        &host,
        &jobs,
        1,
        "I tried pottery and felt nervous.",
        dims(FormationKind::Episode, None, FormationChange::Initial),
        None,
    );
    let second = form(
        &e,
        &host,
        &jobs,
        2,
        "I later made a cup and felt proud.",
        dims(
            FormationKind::RelationshipExperience,
            None,
            FormationChange::Initial,
        ),
        None,
    );
    let key = host
        .submit_synthesis(e.input(1, None), vec![first.target(), second.target()])
        .unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    let state = host.inspect_native().unwrap();
    let theme = state.data.memories.last().unwrap().clone();
    assert_eq!(theme.decision.outcome.knowledge, KnowledgeState::Hypothesis);
    assert_eq!(theme.decision.outcome.adoption, AdoptionState::Staged);
    assert_eq!(theme.decision.input.input_refs.len(), 2);
    assert_eq!(theme.decision.input.evidence.len(), 1);
    let again = host
        .submit_synthesis(e.input(1, None), vec![theme.target()])
        .unwrap();
    wait(&jobs, &again, JobState::Generated);
    host.apply_job(&again).unwrap();
    let state = host.inspect_native().unwrap();
    assert_eq!(state.data.sources.len(), 2);
    let reflection = state.data.memories.last().unwrap();
    assert_eq!(
        reflection.decision.input.input_refs,
        theme.decision.input.input_refs
    );
    assert_eq!(
        reflection.decision.input.evidence,
        theme.decision.input.evidence
    );
    assert_eq!(reflection.decision.outcome.adoption, AdoptionState::Staged);
    form(
        &e,
        &host,
        &jobs,
        3,
        "Correction: that first class was relaxing.",
        dims(FormationKind::Episode, None, FormationChange::Correction),
        Some(first.target()),
    );
    assert!(host.receipts(&again).is_err());
    assert!(
        host.submit_synthesis(e.input(1, None), vec![theme.target()])
            .is_err()
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn conflict_uses_counter_sources_as_dependencies_then_new_evidence_can_correct_it() {
    let e = env();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = jobs.spawn(DimensionalModel).unwrap();
    for (n, text) in [
        (1, "I might attend the event."),
        (2, "I might not attend the event."),
    ] {
        host.capture_native(e.source(n, text, SpeakerRole::User))
            .unwrap();
    }
    let mut input = e.input(1, None);
    input.input_refs.push(
        e.source(2, "I might not attend the event.", SpeakerRole::User)
            .reference(),
    );
    let mut d = dims(FormationKind::Fact, None, FormationChange::Initial);
    d.counter_sources = vec![input.input_refs[1].clone()];
    input.dimensions = Some(d);
    let key = host.prepare_submit(vec![input]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    let conflict = host
        .inspect_native()
        .unwrap()
        .data
        .memories
        .last()
        .unwrap()
        .clone();
    assert_eq!(
        conflict.decision.outcome.knowledge,
        KnowledgeState::Disputed
    );
    assert!(
        host.context("event", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    let corrected = form(
        &e,
        &host,
        &jobs,
        3,
        "I decided not to attend the event.",
        dims(FormationKind::Fact, None, FormationChange::Correction),
        Some(conflict.target()),
    );
    assert_eq!(corrected.decision.outcome.adoption, AdoptionState::Adopted);
    drop(host);
    thread.join().unwrap();
}

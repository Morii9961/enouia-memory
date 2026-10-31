#![cfg(windows)]
#[path = "../../enouia-memory-vault/tests/support/segmented_fixture.rs"]
mod fixture;
use enouia_memory_contract::{common::*, ids::*, next::*, time::Timestamp};
use enouia_memory_init::{parse::*, pipeline::*};
use enouia_memory_vault::{
    OsIdSource,
    automation::{AutomationFault, segmented::*},
    fault::{FaultAction, FaultPoint, Faults},
    fs::ManagedRoot,
};
use fixture::*;
use serde_json::{Value, json};
use std::sync::Arc;

const JOURNAL: &str = "sync-state/captured-formation-1.json";
fn owner_env() -> Env {
    let mut e = Env::new();
    e.descriptor.layout = AutomationLayout::SegmentedOwnerV1;
    e.descriptor.owner_subject_id = Some(e.policy(1, 1, 1).subject_id);
    e
}
fn client() -> ActorRef {
    ActorRef {
        actor_type: ActorType::Client,
        actor_id: PrincipalId::parse(&id("prn", 77)).unwrap(),
    }
}
fn scope(p: &AutomationPolicy) -> RuntimeCaptureScope {
    RuntimeCaptureScope {
        policy_id: p.policy_id.clone(),
        source_ids: p.source_ids.clone(),
        max_sensitivity: Sensitivity::Private,
        platform: "enouia-runtime".into(),
        account_scope: "account-synthetic".into(),
        default_timezone: "Asia/Shanghai".into(),
        valid_until: Timestamp::from_unix_ms(NOW + 120_000).unwrap(),
    }
}
fn turn(n: u32, conversation: &str, parent: Option<&str>, text: &str) -> RuntimeCaptureRequest {
    let v: Value = serde_json::from_slice(
        &std::fs::read(
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../tests/fixtures/runtime-owner-capture/cases.json"),
        )
        .unwrap(),
    )
    .unwrap();
    let mut r: RuntimeCaptureRequest = serde_json::from_value(v["base"].clone()).unwrap();
    r.request_id = RequestId::parse(&id("req", n)).unwrap();
    r.conversation_id = conversation.into();
    r.turns[0].message_id = format!("m{n}");
    r.turns[0].parent_message_id = parent.map(Into::into);
    r.turns[0].sequence = n;
    r.turns[0].text = text.into();
    r.turns[0].attachments.clear();
    r
}
fn capture_and_enqueue(
    v: &SegmentedAutomationVault,
    e: &Env,
    p: &AutomationPolicy,
    r: &RuntimeCaptureRequest,
) -> CapturedQueueReceipt {
    let permit = v
        .authorize_owner_runtime_capture(&e.descriptor.created_by, client(), scope(p))
        .unwrap();
    let capture = v.capture_runtime(&permit, r, false).unwrap();
    v.enqueue_captured(&e.descriptor.system_actor, &capture, &p.reference())
        .unwrap()
}
fn statement(quote: &str, interpretation: Interpretation) -> CaptureStatement {
    CaptureStatement {
        quote: quote.into(),
        title: quote.trim_end_matches('.').into(),
        interpretation,
        dimensions: dimensions(FormationKind::Preference),
        event_time: unknown_time(),
        valid_time: unknown_time(),
        note: Some("Scripted synthetic analysis.".into()),
        prompt_version: "scripted-capture-1".into(),
        model_version: None,
    }
}
/// Quotes the first sentence of each turn. No model, network or embedding.
#[derive(Default)]
struct Scripted {
    calls: usize,
    next: Option<CaptureAnalysis>,
}
impl CaptureAnalyzer for Scripted {
    fn analyze(&mut self, _job: &QueuedCapture, source: &CapturedSource) -> CaptureAnalysis {
        self.calls += 1;
        self.next.take().unwrap_or_else(|| {
            CaptureAnalysis::Statements(vec![statement(
                &source.text,
                Interpretation::ClearSelfReport,
            )])
        })
    }
}
struct Panics;
impl CaptureAnalyzer for Panics {
    fn analyze(&mut self, _: &QueuedCapture, _: &CapturedSource) -> CaptureAnalysis {
        panic!("synthetic analyzer failure");
    }
}
fn rig(e: &Env) -> (Arc<SegmentedAutomationVault>, AutomationPolicy) {
    let v = Arc::new(e.create(Faults::none()));
    let p = e.policy(1, 1, 8);
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    (v, p)
}
fn drain(worker: &mut CapturedFormation, analyzer: &mut Scripted) {
    while worker.step(analyzer).unwrap() {}
}
fn memories(v: &SegmentedAutomationVault, e: &Env) -> Vec<String> {
    v.read(
        &e.descriptor.system_actor,
        Sensitivity::HighlySensitive,
        |r| {
            r.memories
                .iter()
                .map(|m| m.memory.decision.input.content.clone())
                .collect()
        },
    )
    .unwrap()
}

#[test]
fn captured_jobs_form_through_the_system_route_in_turn_order_and_survive_reopen() {
    let e = owner_env();
    let (v, p) = rig(&e);
    let system = e.descriptor.system_actor.clone();
    let first = capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "I prefer quiet rooms."));
    let second = capture_and_enqueue(&v, &e, &p, &turn(2, "c", Some("m1"), "I enjoy pottery."));
    let mut worker = CapturedFormation::open(v.clone(), &system).unwrap();
    let mut analyzer = Scripted::default();
    drain(&mut worker, &mut analyzer);
    assert_eq!(analyzer.calls, 2);
    assert_eq!(memories(&v, &e).len(), 2);
    let status = worker.status();
    assert_eq!(status[0].job_key, first.job_keys[0]);
    assert_eq!(status[1].job_key, second.job_keys[0]);
    assert!(status.iter().all(|s| s.state == FormationJobState::Formed));
    assert_eq!(status[0].receipts.len(), 1);
    assert!(status[0].receipts[0].sequence.get() < status[1].receipts[0].sequence.get());
    let memory = v
        .read(&system, Sensitivity::HighlySensitive, |r| {
            let m = r
                .memories
                .iter()
                .find(|m| m.memory.memory == status[0].receipts[0].result)
                .unwrap();
            (
                m.memory.decision.input.record_fields.clone().unwrap(),
                m.memory.decision.outcome.clone(),
                m.memory.decision.input.actor.clone(),
            )
        })
        .unwrap();
    assert_eq!(memory.0.formation.mode, FormationMode::Live);
    assert!(memory.0.formation.import_batch_id.is_none());
    assert_eq!(memory.0.evidence[0].quote, "I prefer quiet rooms.");
    assert_eq!(memory.1.adoption, AdoptionState::Adopted);
    assert_eq!(memory.2, system);
    let after = v.snapshot().unwrap();
    assert!(!worker.step(&mut analyzer).unwrap());
    drop(worker);
    drop(v);
    let v = Arc::new(e.open());
    let mut worker = CapturedFormation::open(v.clone(), &system).unwrap();
    assert!(!worker.step(&mut analyzer).unwrap());
    assert_eq!(analyzer.calls, 2);
    assert_eq!(v.snapshot().unwrap(), after);
    assert_eq!(worker.status(), status);
}
#[test]
fn worker_is_exclusive_and_refuses_old_layouts_wrong_actors_and_foreign_journals() {
    let e = owner_env();
    let (v, _) = rig(&e);
    let system = &e.descriptor.system_actor;
    assert!(CapturedFormation::open(v.clone(), &e.descriptor.created_by).is_err());
    let worker = CapturedFormation::open(v.clone(), system).unwrap();
    assert!(matches!(
        CapturedFormation::open(v.clone(), system),
        Err(AutomationFault::Busy)
    ));
    drop(worker);
    let managed = ManagedRoot::new(e.root.path(), Faults::none());
    let mut journal: Value =
        serde_json::from_slice(&managed.read_bounded(JOURNAL, 1 << 20).unwrap().unwrap()).unwrap();
    journal["vault_id"] = json!(id("vlt", 9));
    managed
        .write_atomic(JOURNAL, &serde_json::to_vec(&journal).unwrap())
        .unwrap();
    assert!(matches!(
        CapturedFormation::open(v.clone(), system),
        Err(AutomationFault::Denied)
    ));
    let old = Env::new();
    let old_vault = Arc::new(old.create(Faults::none()));
    assert!(matches!(
        CapturedFormation::open(old_vault, &old.descriptor.system_actor),
        Err(AutomationFault::Denied)
    ));
}
#[test]
fn uncertain_analysis_is_never_resent_and_paused_work_stays_queued() {
    let e = owner_env();
    let (v, p) = rig(&e);
    let system = &e.descriptor.system_actor;
    capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "First turn."));
    capture_and_enqueue(&v, &e, &p, &turn(2, "c", Some("m1"), "Second turn."));
    capture_and_enqueue(&v, &e, &p, &turn(3, "c", Some("m2"), "Third turn."));
    capture_and_enqueue(&v, &e, &p, &turn(4, "c", Some("m3"), "Fourth turn."));
    let mut worker = CapturedFormation::open(v.clone(), system).unwrap();
    let mut analyzer = Scripted {
        next: Some(CaptureAnalysis::Paused),
        ..Default::default()
    };
    assert!(!worker.step(&mut analyzer).unwrap());
    assert!(worker.status().is_empty());
    analyzer.next = Some(CaptureAnalysis::Unknown);
    assert!(worker.step(&mut analyzer).unwrap());
    assert_eq!(analyzer.calls, 2);
    assert_eq!(worker.status()[0].state, FormationJobState::AnalysisUnknown);
    assert!(worker.step(&mut Panics).unwrap());
    assert_eq!(worker.status()[1].state, FormationJobState::AnalysisUnknown);
    analyzer.next = Some(CaptureAnalysis::NoMemory);
    assert!(worker.step(&mut analyzer).unwrap());
    assert_eq!(worker.status()[2].state, FormationJobState::NoMemory);
    drain(&mut worker, &mut analyzer);
    assert_eq!(analyzer.calls, 4);
    assert_eq!(memories(&v, &e), vec!["Fourth turn."]);
    drop(worker);
    // A durable dispatch with no result: the handoff may have happened.
    let managed = ManagedRoot::new(e.root.path(), Faults::none());
    let mut journal: Value =
        serde_json::from_slice(&managed.read_bounded(JOURNAL, 1 << 22).unwrap().unwrap()).unwrap();
    journal["jobs"][0]["state"] = json!("dispatched");
    managed
        .write_atomic(JOURNAL, &serde_json::to_vec(&journal).unwrap())
        .unwrap();
    let mut worker = CapturedFormation::open(v.clone(), system).unwrap();
    assert_eq!(worker.status()[0].state, FormationJobState::AnalysisUnknown);
    assert!(!worker.step(&mut analyzer).unwrap());
    assert_eq!(analyzer.calls, 4);
}
#[test]
fn untrusted_analysis_goes_through_evaluation_and_ambiguity_stages_automatically() {
    let e = owner_env();
    let (v, p) = rig(&e);
    let system = &e.descriptor.system_actor;
    capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "I might like jazz."));
    capture_and_enqueue(&v, &e, &p, &turn(2, "c", Some("m1"), "Nothing usable."));
    let mut worker = CapturedFormation::open(v.clone(), system).unwrap();
    let mut analyzer = Scripted {
        next: Some(CaptureAnalysis::Statements(vec![
            statement("I might like jazz.", Interpretation::Ambiguous),
            statement(
                "A quote that was never said.",
                Interpretation::ClearSelfReport,
            ),
        ])),
        ..Default::default()
    };
    assert!(worker.step(&mut analyzer).unwrap());
    assert_eq!(worker.status()[0].refused, 1);
    for _ in 0..3 {
        assert!(worker.step(&mut analyzer).unwrap());
    }
    assert_eq!(worker.status()[0].state, FormationJobState::Formed);
    assert_eq!(
        v.read(system, Sensitivity::HighlySensitive, |r| {
            r.memories[0].memory.decision.outcome.adoption
        })
        .unwrap(),
        AdoptionState::Staged
    );
    analyzer.next = Some(CaptureAnalysis::Statements(vec![statement(
        "Forged.",
        Interpretation::ClearSelfReport,
    )]));
    drain(&mut worker, &mut analyzer);
    assert_eq!(worker.status()[1].state, FormationJobState::Rejected);
    assert_eq!(memories(&v, &e).len(), 1);
}
#[test]
fn an_evaluator_refusal_is_recorded_and_the_queue_moves_on() {
    let e = owner_env();
    let (v, p) = rig(&e);
    let system = &e.descriptor.system_actor;
    let mut assistant = turn(1, "c", None, "You prefer noise.");
    assistant.turns[0].speaker = SpeakerRole::Assistant;
    capture_and_enqueue(&v, &e, &p, &assistant);
    capture_and_enqueue(
        &v,
        &e,
        &p,
        &turn(2, "c", Some("m1"), "I prefer quiet rooms."),
    );
    let mut worker = CapturedFormation::open(v.clone(), system).unwrap();
    // A clear self-report quoting the assistant fails the evaluator's support rule.
    let mut analyzer = Scripted::default();
    drain(&mut worker, &mut analyzer);
    assert_eq!(analyzer.calls, 2);
    let status = worker.status();
    assert_eq!(status[0].state, FormationJobState::Rejected);
    assert_eq!(status[0].refused, 1);
    assert_eq!(status[1].state, FormationJobState::Formed);
    assert_eq!(memories(&v, &e), vec!["I prefer quiet rooms."]);
}
#[test]
fn a_moved_head_or_policy_gap_before_apply_reevaluates_without_new_analysis() {
    let e = owner_env();
    let (v, p) = rig(&e);
    let owner = &e.descriptor.created_by;
    let system = &e.descriptor.system_actor;
    capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "First turn."));
    let mut worker = CapturedFormation::open(v.clone(), system).unwrap();
    let mut analyzer = Scripted::default();
    assert!(worker.step(&mut analyzer).unwrap()); // Analysis.
    assert!(worker.step(&mut analyzer).unwrap()); // Prepare.
    capture_and_enqueue(&v, &e, &p, &turn(2, "c", Some("m1"), "Second turn."));
    assert!(worker.step(&mut analyzer).unwrap()); // Apply finds a moved head.
    assert!(memories(&v, &e).is_empty());
    assert_eq!(analyzer.calls, 1);
    assert!(worker.step(&mut analyzer).unwrap()); // Prepare again.
    let mut revoked = p.clone();
    revoked.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    revoked.active = false;
    v.set_policy(owner, revoked.clone()).unwrap();
    // A policy gap holds the job. Its prepared request names the old revision,
    // and must be re-evaluated rather than refused once the policy returns.
    assert!(!worker.step(&mut analyzer).unwrap());
    assert_eq!(worker.status()[0].refused, 0);
    assert!(memories(&v, &e).is_empty());
    let mut restored = revoked;
    restored.revision = enouia_memory_contract::json::Revision::new(3).unwrap();
    restored.active = true;
    v.set_policy(owner, restored).unwrap();
    drain(&mut worker, &mut analyzer);
    assert_eq!(analyzer.calls, 2);
    assert_eq!(memories(&v, &e).len(), 2);
    assert_eq!(worker.status()[0].receipts.len(), 1);
    assert_eq!(worker.status()[0].refused, 0);
}
#[test]
fn deletion_after_analysis_forms_nothing_for_that_source() {
    let e = owner_env();
    let (v, p) = rig(&e);
    let system = &e.descriptor.system_actor;
    capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "First turn."));
    capture_and_enqueue(&v, &e, &p, &turn(2, "c", Some("m1"), "Second turn."));
    let mut worker = CapturedFormation::open(v.clone(), system).unwrap();
    let mut analyzer = Scripted::default();
    assert!(worker.step(&mut analyzer).unwrap());
    let deleted = v.queued_captures(system).unwrap()[0]
        .source
        .source_id
        .clone();
    v.delete(&e.descriptor.created_by, vec![deleted], vec![])
        .unwrap();
    drain(&mut worker, &mut analyzer);
    assert_eq!(analyzer.calls, 2);
    assert_eq!(memories(&v, &e), vec!["Second turn."]);
    let status = worker.status();
    assert_eq!(status[0].state, FormationJobState::Forming);
    assert!(status[0].receipts.is_empty());
    assert_eq!(status[1].state, FormationJobState::Formed);
}
#[test]
fn revoked_work_waits_without_analysis_and_resumes_when_the_policy_returns() {
    let e = owner_env();
    let (v, p) = rig(&e);
    let owner = &e.descriptor.created_by;
    let system = &e.descriptor.system_actor;
    capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "First turn."));
    capture_and_enqueue(&v, &e, &p, &turn(2, "c", Some("m1"), "Second turn."));
    let mut worker = CapturedFormation::open(v.clone(), system).unwrap();
    let mut analyzer = Scripted::default();
    let mut revoked = p.clone();
    revoked.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
    revoked.active = false;
    v.set_policy(owner, revoked.clone()).unwrap();
    assert!(!worker.step(&mut analyzer).unwrap());
    assert_eq!(analyzer.calls, 0);
    assert!(memories(&v, &e).is_empty());
    let jobs = worker.status();
    assert!(jobs.is_empty());
    let mut restored = revoked;
    restored.revision = enouia_memory_contract::json::Revision::new(3).unwrap();
    restored.active = true;
    v.set_policy(owner, restored).unwrap();
    drain(&mut worker, &mut analyzer);
    assert_eq!(analyzer.calls, 2);
    assert_eq!(memories(&v, &e).len(), 2);
    assert_eq!(worker.status()[0].state, FormationJobState::Formed);
}
#[test]
fn publication_faults_resolve_from_the_chain_without_a_second_analysis() {
    for (point, persisted) in [
        (FaultPoint::StagingWritten, false),
        (FaultPoint::AfterCurrent, true),
    ] {
        let e = owner_env();
        let faults = Faults::armed();
        let v = Arc::new(e.create(faults.clone()));
        let system = e.descriptor.system_actor.clone();
        let p = e.policy(1, 1, 8);
        v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "I prefer quiet rooms."));
        let mut worker = CapturedFormation::open(v.clone(), &system).unwrap();
        let mut analyzer = Scripted::default();
        assert!(worker.step(&mut analyzer).unwrap());
        assert!(worker.step(&mut analyzer).unwrap());
        faults.arm(point, 1, FaultAction::Fail(112));
        assert!(matches!(
            worker.step(&mut analyzer),
            Err(AutomationFault::OutcomeUnknown)
        ));
        assert!(matches!(
            worker.step(&mut analyzer),
            Err(AutomationFault::OutcomeUnknown)
        ));
        drop(worker);
        drop(v);
        let v = Arc::new(
            SegmentedAutomationVault::recover(
                &e.root,
                e.descriptor.host_id.clone(),
                &system,
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none(),
            )
            .unwrap(),
        );
        assert_eq!(memories(&v, &e).len(), usize::from(persisted));
        let mut worker = CapturedFormation::open(v.clone(), &system).unwrap();
        drain(&mut worker, &mut analyzer);
        assert_eq!(analyzer.calls, 1);
        assert_eq!(memories(&v, &e), vec!["I prefer quiet rooms."]);
        assert_eq!(worker.status()[0].state, FormationJobState::Formed);
        assert_eq!(worker.status()[0].receipts.len(), 1);
    }
}
#[test]
fn live_captures_run_before_backfill_on_one_scheduler() {
    let e = owner_env();
    let inputs = vec![Material {
        alias: "synthetic-note".into(),
        format: Format::Document,
        bytes: b"A synthetic backfill note.".to_vec(),
        account_alias: "synthetic".into(),
    }];
    let prep = prepare(&inputs).unwrap();
    let backfill = Statement {
        source_index: 0,
        quote: "A synthetic backfill note.".into(),
        title: "A synthetic backfill note".into(),
        interpretation: Interpretation::Inference,
        dimensions: dimensions(FormationKind::Preference),
        event_time: unknown_time(),
        valid_time: unknown_time(),
        entities: vec![],
        note: None,
        annotations: AnalysisAnnotations::default(),
        revises_statement: None,
        derives_from_statements: vec![],
    };
    let mut init = Initializer::create_synthetic(
        &e.root,
        e.descriptor.clone(),
        e.clock.clone(),
        Arc::new(OsIdSource),
    )
    .unwrap();
    let plan = Plan::build(
        &prep,
        &[backfill],
        e.policy(1, 1, 1),
        FormationMode::Backfill,
        Timestamp::from_unix_ms(NOW).unwrap(),
        &OsIdSource,
    )
    .unwrap();
    init.enqueue(plan, &prep).unwrap();
    assert!(init.step().unwrap()); // Backfill sources and policy are published.
    let p = e.policy(2, 100, 8);
    let v = init.vault.clone();
    v.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    capture_and_enqueue(&v, &e, &p, &turn(1, "c", None, "I prefer quiet rooms."));
    let system = e.descriptor.system_actor.clone();
    let mut worker = CapturedFormation::open(v.clone(), &system).unwrap();
    let mut analyzer = Scripted::default();
    for _ in 0..4 {
        assert!(init.step_with_captures(&mut worker, &mut analyzer).unwrap());
    }
    assert_eq!(init.plans()[0].completed(), 0);
    assert_eq!(worker.status()[0].state, FormationJobState::Formed);
    assert!(init.step_with_captures(&mut worker, &mut analyzer).unwrap());
    assert_eq!(init.plans()[0].completed(), 1);
    assert!(!init.step_with_captures(&mut worker, &mut analyzer).unwrap());
}

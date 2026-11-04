#![cfg(windows)]
#[path = "support/agent_correction_fixture.rs"]
mod fixture;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod schema_support;
use enouia_memory_contract::{
    common::*,
    hash::{Sha256Hex, sha256},
    ids::*,
    json::Revision,
    next::*,
};
use enouia_memory_host::automation::{
    application::ApplicationFault, qualified_agent::QualifiedAgentHost,
};
use enouia_memory_vault::{
    OsIdSource,
    automation::{AutomationFault, segmented::*},
    fault::{FaultAction, FaultPoint, Faults},
};
use fixture::*;
use serde_json::json;
use std::sync::Arc;

fn fault(result: Result<AgentCorrectionReceipt, ApplicationFault>) -> AutomationFault {
    match result {
        Err(ApplicationFault::Vault(f)) => f,
        other => panic!("unexpected {:?}", other.map(|r| r.state)),
    }
}

#[test]
fn correction_draft_binds_current_target_actual_user_quote_and_separate_agent_identity_without_writes()
 {
    let (e, v, p, mut request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reporter = host
        .correction_reporter(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let before = v.snapshot().unwrap();
    let draft = reporter.prepare(&request).unwrap();
    assert_eq!(draft.capability, AGENT_CORRECTION_CAPABILITY);
    assert_eq!(draft.principal, principal);
    assert_eq!(draft.target, request.target);
    assert_eq!(draft.snapshot, before);
    assert_eq!(draft.evidence.quote, "「ＡＢＣ　纠正原话」");
    assert_eq!(draft.evidence.start, 2);
    assert_eq!(draft.evidence.speaker, SpeakerRole::User);
    assert_eq!(
        draft.evidence.reference.evidence_class,
        EvidenceClass::UserStatement
    );
    assert_eq!(
        reporter.prepare(&request).unwrap().report_key,
        draft.report_key
    );
    request.correction.push('!');
    let changed = reporter.prepare(&request).unwrap();
    assert_eq!(changed.report_key, draft.report_key);
    assert_ne!(changed.request_hash, draft.request_hash);
    let other = host
        .correction_reporter(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 89),
            scope(&p),
        )
        .unwrap();
    assert_ne!(
        other.prepare(&request).unwrap().report_key,
        draft.report_key
    );
    assert_eq!(v.snapshot().unwrap(), before);
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.memories.len(), 1);
        assert_eq!(d.memories[0].target, request.target);
        assert!(d.owner_actions.is_empty());
    })
    .unwrap();
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
}

#[test]
fn correction_requires_both_target_closure_and_real_current_user_source() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let reporter = host
        .correction_reporter(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    for variant in 0..10 {
        let mut r = request.clone();
        match variant {
            0 => r.target.hash = sha256(b"wrong target"),
            1 => r.target.memory.revision = Revision::new(2).unwrap(),
            2 => r.target.memory.memory_id = MemoryId::parse(&id("mem", 99)).unwrap(),
            3 => r.source.source_revision = Revision::new(2).unwrap(),
            4 => r.quote_hint = "missing".into(),
            5 => r.quote_hint = "x".repeat(501),
            6 => r.correction = "x".repeat(2001),
            7 => r.correction = " ".into(),
            8 => r.quote_hint.clear(),
            _ => {
                // A permitted System fixture source is not Runtime user-correction evidence.
                r.source = e.source(8, Sensitivity::Public).reference();
                r.quote_hint = e.source(8, Sensitivity::Public).text;
            }
        }
        assert!(reporter.prepare(&r).is_err(), "variant {variant}");
    }
    for excluded in [
        request.source.source_id.clone(),
        SourceId::parse(&id("src", 8)).unwrap(),
    ] {
        let mut narrow = scope(&p);
        narrow.source_ids.retain(|id| id != &excluded);
        let scoped = host
            .correction_reporter(
                &e.descriptor.created_by,
                actor(ActorType::Agent, 88),
                narrow,
            )
            .unwrap();
        assert!(scoped.prepare(&request).is_err());
    }
    let permit = v
        .authorize_agent_correction(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 88),
            scope(&p),
        )
        .unwrap();
    assert!(
        v.prepare_agent_correction(&permit, &request, |source, _| {
            let mut q = quoted_evidence(source, &source.text).unwrap();
            q.reference.evidence_class = EvidenceClass::UserConfirmation;
            Some(q)
        })
        .is_err()
    );
    let (e2, v2, p2, r2) = correction_rig(SpeakerRole::Assistant);
    let host2 = QualifiedAgentHost::from_authenticated_initialized_owner(
        v2.clone(),
        e2.descriptor.created_by.clone(),
    )
    .unwrap();
    assert!(
        host2
            .correction_reporter(
                &e2.descriptor.created_by,
                actor(ActorType::Agent, 88),
                scope(&p2)
            )
            .unwrap()
            .prepare(&r2)
            .is_err()
    );
    assert!(
        v2.prepare_agent_correction(&permit, &r2, |_, _| panic!("foreign grant reached source"))
            .is_err()
    );
}

#[test]
fn correction_delegation_requires_owner_setup_and_live_generation_policy_source_and_target() {
    for invalidation in 0..6 {
        let (e, v, p, request) = correction_rig(SpeakerRole::User);
        let owner = &e.descriptor.created_by;
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let agent = actor(ActorType::Agent, 88);
        let mut unreadable = scope(&p);
        unreadable.source_read = false;
        assert!(
            host.correction_reporter(owner, agent.clone(), unreadable)
                .is_err()
        );
        assert!(
            host.correction_reporter(&e.descriptor.system_actor, agent.clone(), scope(&p))
                .is_err()
        );
        assert!(
            host.correction_reporter(owner, owner.clone(), scope(&p))
                .is_err()
        );
        let old_host =
            QualifiedAgentHost::from_authenticated_owner(v.clone(), owner.clone()).unwrap();
        assert!(
            old_host
                .correction_reporter(owner, agent.clone(), scope(&p))
                .is_err()
        );
        let reporter = host.correction_reporter(owner, agent, scope(&p)).unwrap();
        assert!(reporter.prepare(&request).is_ok());
        match invalidation {
            0 => reporter.revoke(),
            1 => {
                host.lock().unwrap();
                host.unlock(owner).unwrap();
            }
            2 => {
                let mut replacement = p.clone();
                replacement.revision = Revision::new(2).unwrap();
                v.set_policy(owner, replacement).unwrap();
            }
            3 => e.clock.set(NOW + 120_001),
            4 => {
                v.delete(owner, vec![request.source.source_id.clone()], vec![])
                    .unwrap();
            }
            _ => {
                v.delete(owner, vec![], vec![request.target.memory.memory_id.clone()])
                    .unwrap();
            }
        }
        assert!(
            reporter.prepare(&request).is_err(),
            "invalidation {invalidation}"
        );
    }
}

#[test]
fn saved_report_replays_its_original_receipt_after_a_system_revision_without_correcting() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let principal = actor(ActorType::Agent, 88);
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let reporter = host
        .correction_reporter(owner, principal.clone(), scope(&p))
        .unwrap();
    let draft = reporter.prepare(&request).unwrap();
    let before = v.snapshot().unwrap();
    let receipt = reporter.submit(&request).unwrap();
    assert_eq!(receipt.state, AgentCorrectionState::ReportSaved);
    assert_eq!(receipt.report_key, draft.report_key);
    assert_eq!(receipt.request_hash, draft.request_hash);
    assert_eq!(receipt.target, request.target);
    let saved = v.snapshot().unwrap();
    assert_ne!(saved, before);
    assert_eq!(reporter.submit(&request).unwrap(), receipt);
    assert_eq!(v.snapshot().unwrap(), saved);
    // Saving is not correcting: no memory revision, Owner action or queue entry.
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        assert_eq!(d.memories.len(), 1);
        assert_eq!(d.memories[0].target, request.target);
        assert!(d.owner_actions.is_empty());
    })
    .unwrap();
    assert!(
        v.queued_captures(&e.descriptor.system_actor)
            .unwrap()
            .is_empty()
    );
    let mut changed = request.clone();
    changed.correction.push('!');
    assert!(matches!(
        fault(reporter.submit(&changed)),
        AutomationFault::Conflict
    ));
    assert_eq!(v.snapshot().unwrap(), saved);

    let current = revise(&e, &v, &p, &request);
    let revised = v.snapshot().unwrap();
    // New work needs the current target; the saved key still finds its receipt.
    assert!(matches!(
        reporter.prepare(&request),
        Err(ApplicationFault::Vault(AutomationFault::Stale))
    ));
    assert_eq!(reporter.submit(&request).unwrap(), receipt);
    assert!(matches!(
        fault(reporter.submit(&changed)),
        AutomationFault::Conflict
    ));
    let mut fresh = request.clone();
    fresh.request_id = RequestId::parse(&id("req", 300)).unwrap();
    assert!(matches!(
        fault(reporter.submit(&fresh)),
        AutomationFault::Stale
    ));
    let mut unknown = request.clone();
    unknown.target.memory.revision = Revision::new(9).unwrap();
    assert!(matches!(
        fault(reporter.submit(&unknown)),
        AutomationFault::Stale
    ));
    assert_eq!(v.snapshot().unwrap(), revised);
    fresh.target = current.clone();
    let second = reporter.submit(&fresh).unwrap();
    assert_eq!(second.target, current);
    assert_ne!(second.report_key, receipt.report_key);
    let after = v.snapshot().unwrap();

    // Chain replay restores both receipts on a fresh instance and delegation.
    drop(reporter);
    drop(host);
    drop(v);
    let v = Arc::new(e.open());
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let reporter = host
        .correction_reporter(owner, principal, scope(&p))
        .unwrap();
    assert_eq!(reporter.submit(&request).unwrap(), receipt);
    assert_eq!(reporter.submit(&fresh).unwrap(), second);
    assert_eq!(v.snapshot().unwrap(), after);
    // A different Agent sending the identical packet has a different key.
    let other = host
        .correction_reporter(owner, actor(ActorType::Agent, 89), scope(&p))
        .unwrap();
    assert!(matches!(
        fault(other.submit(&request)),
        AutomationFault::Stale
    ));
}

#[test]
fn deletion_revocation_and_scope_precede_original_receipt_replay() {
    for invalidation in 0..7 {
        let (e, v, p, request) = correction_rig(SpeakerRole::User);
        let owner = &e.descriptor.created_by;
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let agent = actor(ActorType::Agent, 88);
        let reporter = host
            .correction_reporter(owner, agent.clone(), scope(&p))
            .unwrap();
        reporter.submit(&request).unwrap();
        revise(&e, &v, &p, &request);
        assert!(reporter.submit(&request).is_ok());
        let mut narrow = scope(&p);
        narrow
            .source_ids
            .retain(|id| id != &request.source.source_id);
        let narrowed = host.correction_reporter(owner, agent, narrow).unwrap();
        let mut caller = &reporter;
        match invalidation {
            0 => reporter.revoke(),
            1 => {
                host.lock().unwrap();
                host.unlock(owner).unwrap();
            }
            2 => e.clock.set(NOW + 120_001),
            3 => {
                v.delete(owner, vec![request.source.source_id.clone()], vec![])
                    .unwrap();
            }
            4 => {
                v.delete(owner, vec![], vec![request.target.memory.memory_id.clone()])
                    .unwrap();
            }
            // A fresh grant that excludes the evidence source cannot replay.
            5 => caller = &narrowed,
            _ => {
                let mut replacement = p.clone();
                replacement.revision = Revision::new(2).unwrap();
                v.set_policy(owner, replacement).unwrap();
            }
        }
        let after = v.snapshot().unwrap();
        assert!(
            caller.submit(&request).is_err(),
            "invalidation {invalidation}"
        );
        assert_eq!(v.snapshot().unwrap(), after);
    }
}

#[test]
fn report_capacity_is_per_target_and_saved_receipts_still_replay() {
    let (e, v, p, mut request) = correction_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let reporter = host
        .correction_reporter(owner, actor(ActorType::Agent, 88), scope(&p))
        .unwrap();
    let original = request.clone();
    let first = reporter.submit(&original).unwrap();
    for n in 1..MAX_AGENT_CORRECTIONS_PER_TARGET {
        request.request_id = RequestId::parse(&id("req", 100 + n as u32)).unwrap();
        reporter.submit(&request).unwrap();
    }
    let full = v.snapshot().unwrap();
    request.request_id = RequestId::parse(&id("req", 999)).unwrap();
    assert!(matches!(
        fault(reporter.submit(&request)),
        AutomationFault::Capacity
    ));
    assert_eq!(reporter.submit(&original).unwrap(), first);
    assert_eq!(v.snapshot().unwrap(), full);
}

#[test]
fn uncertain_report_publication_latches_the_original_request_and_recovers_one_receipt() {
    for (point, persisted) in [
        (FaultPoint::StagingWritten, false),
        (FaultPoint::AfterCurrent, true),
    ] {
        let faults = Faults::armed();
        let (e, v, p, request) = correction_rig_with(SpeakerRole::User, faults.clone());
        let owner = &e.descriptor.created_by;
        let principal = actor(ActorType::Agent, 88);
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let reporter = host
            .correction_reporter(owner, principal.clone(), scope(&p))
            .unwrap();
        faults.arm(point, 1, FaultAction::Fail(121));
        assert!(matches!(
            fault(reporter.submit(&request)),
            AutomationFault::OutcomeUnknown
        ));
        assert_eq!(
            digest(reporter.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
        let mut changed = request.clone();
        changed.correction.push('!');
        assert!(matches!(
            fault(reporter.submit(&changed)),
            AutomationFault::OutcomeUnknown
        ));
        assert_eq!(
            digest(reporter.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
        drop(reporter);
        drop(host);
        drop(v);
        let v = Arc::new(
            SegmentedAutomationVault::recover(
                &e.root,
                e.descriptor.host_id.clone(),
                &e.descriptor.system_actor,
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none(),
            )
            .unwrap(),
        );
        if persisted {
            // A System revision between recovery and explicit replay must not
            // hide the report that was published before the uncertainty.
            revise(&e, &v, &p, &request);
        }
        let recovered = v.snapshot().unwrap();
        let host =
            QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone())
                .unwrap();
        let reporter = host
            .correction_reporter(owner, principal, scope(&p))
            .unwrap();
        let receipt = reporter.submit(&request).unwrap();
        let after = v.snapshot().unwrap();
        assert_eq!(after == recovered, persisted);
        assert_eq!(receipt.target, request.target);
        assert_eq!(reporter.submit(&request).unwrap(), receipt);
        assert_eq!(v.snapshot().unwrap(), after);
        assert!(reporter.original_unknown().unwrap().is_none());
    }
}

#[test]
fn concurrent_report_replay_returns_one_receipt_and_one_commit() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let owner = &e.descriptor.created_by;
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    let reporter = host
        .correction_reporter(owner, actor(ActorType::Agent, 88), scope(&p))
        .unwrap();
    let before = v.snapshot().unwrap();
    let barrier = std::sync::Barrier::new(2);
    let (a, b) = std::thread::scope(|scope| {
        let thread = scope.spawn(|| {
            barrier.wait();
            reporter.submit(&request).unwrap()
        });
        barrier.wait();
        let b = reporter.submit(&request).unwrap();
        (thread.join().unwrap(), b)
    });
    assert_eq!(a, b);
    let after = v.snapshot().unwrap();
    assert_ne!(after, before);
    assert_eq!(reporter.submit(&request).unwrap(), a);
    assert_eq!(v.snapshot().unwrap(), after);
}

/// Scripted System correction analysis. No model, network or embedding.
#[derive(Default)]
struct Corrections {
    calls: usize,
    seen: Vec<(Sha256Hex, TargetRef, String)>,
    next: Vec<Result<Option<CaptureAnalysis>, CaptureAnalysis>>,
}
struct Later(Option<CaptureAnalysis>);
impl PendingCaptureAnalysis for Later {
    fn poll(&mut self, _: &CaptureStopToken) -> Option<CaptureAnalysis> {
        self.0.take()
    }
}
fn correcting(target: &CorrectionTarget, quote: &str) -> CaptureStatement {
    let mut dimensions = target.dimensions.clone();
    dimensions.change = FormationChange::Correction;
    CaptureStatement {
        quote: quote.into(),
        title: "ＡＢＣ".into(),
        interpretation: Interpretation::ClearSelfReport,
        dimensions,
        event_time: target_time(),
        valid_time: target_time(),
        note: Some("Scripted synthetic correction analysis.".into()),
        prompt_version: "scripted-correction-1".into(),
        model_version: None,
    }
}
fn target_time() -> RecordTime {
    RecordTime {
        from: None,
        until: None,
        precision: RecordPrecision::Unknown,
        bound: TimeBound::Within,
        tz: None,
        basis: TimeBasis::Unknown,
    }
}
const QUOTE: &str = "「ＡＢＣ　纠正原话」";
impl CorrectionAnalyzer for Corrections {
    fn start(
        &mut self,
        report: &AgentCorrectionRecord,
        target: &CorrectionTarget,
        _: &CapturedSource,
        _: &CaptureStopToken,
    ) -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
        self.calls += 1;
        self.seen.push((
            report.report_key.clone(),
            target.target.clone(),
            report.correction.clone(),
        ));
        match self.next.pop() {
            Some(Ok(later)) => Ok(Box::new(Later(later))),
            Some(Err(analysis)) => Err(analysis),
            None => Err(CaptureAnalysis::Statements(vec![correcting(target, QUOTE)])),
        }
    }
}
fn drain(f: &mut CapturedFormation, analyzer: &mut Corrections) {
    for _ in 0..16 {
        if !f
            .step_corrections(analyzer, &CaptureStopToken::default())
            .unwrap()
        {
            return;
        }
    }
    panic!("bounded synthetic correction steps");
}
fn revisions(e: &Env, v: &SegmentedAutomationVault, memory: &MemoryId) -> Vec<(TargetRef, String)> {
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        d.memories
            .iter()
            .filter(|m| &m.target.memory.memory_id == memory)
            .map(|m| (m.target.clone(), m.memory.decision.input.content.clone()))
            .collect()
    })
    .unwrap()
}
fn saved(
    e: &Env,
    v: &Arc<SegmentedAutomationVault>,
    p: &AutomationPolicy,
    request: &AgentCorrectionRequest,
) -> AgentCorrectionReceipt {
    let owner = &e.descriptor.created_by;
    let host =
        QualifiedAgentHost::from_authenticated_initialized_owner(v.clone(), owner.clone()).unwrap();
    host.correction_reporter(owner, actor(ActorType::Agent, 88), scope(p))
        .unwrap()
        .submit(request)
        .unwrap()
}

#[test]
fn saved_report_is_consumed_once_into_a_target_bound_system_revision() {
    let (e, v, p, request) = correction_rig(SpeakerRole::User);
    let receipt = saved(&e, &v, &p, &request);
    let system = &e.descriptor.system_actor;
    let mut f = CapturedFormation::open(v.clone(), system).unwrap();
    let mut analyzer = Corrections::default();
    drain(&mut f, &mut analyzer);
    assert_eq!(analyzer.calls, 1);
    assert_eq!(analyzer.seen[0].0, receipt.report_key);
    assert_eq!(analyzer.seen[0].1, request.target);
    assert_eq!(analyzer.seen[0].2, request.correction);
    let status = f.status();
    assert_eq!(status.len(), 1);
    assert_eq!(status[0].state, FormationJobState::Formed);
    assert_eq!(status[0].receipts.len(), 1);
    let result = &status[0].receipts[0].result;
    assert_eq!(result.memory_id, request.target.memory.memory_id);
    assert_eq!(
        result.revision.get(),
        request.target.memory.revision.get() + 1
    );
    // The user's words become content; the Agent interpretation does not.
    v.read(system, Sensitivity::Private, |d| {
        let m = d
            .memories
            .iter()
            .find(|m| m.target.memory.memory_id == request.target.memory.memory_id)
            .unwrap();
        let input = &m.memory.decision.input;
        assert_eq!(input.content, QUOTE);
        assert_eq!(input.target.as_ref(), Some(&request.target));
        assert_eq!(
            input.dimensions.as_ref().unwrap().change,
            FormationChange::Correction
        );
        assert_eq!(
            input.evidence[0].evidence_class,
            EvidenceClass::UserStatement
        );
        assert_eq!(input.evidence[0].source_id, request.source.source_id);
        assert!(!input.content.contains(&request.correction));
        assert!(d.owner_actions.is_empty());
    })
    .unwrap();
    // Further steps and a reopened worker never analyze the report again.
    drain(&mut f, &mut analyzer);
    drop(f);
    let mut f = CapturedFormation::open(v.clone(), system).unwrap();
    drain(&mut f, &mut analyzer);
    assert_eq!(analyzer.calls, 1);
    // The saved report still replays its original receipt after consumption.
    assert_eq!(saved(&e, &v, &p, &request), receipt);
}

#[test]
fn correction_analysis_outcomes_never_resend_and_refusals_never_revise() {
    for variant in 0..7 {
        let (e, v, p, request) = correction_rig(SpeakerRole::User);
        saved(&e, &v, &p, &request);
        let system = &e.descriptor.system_actor;
        let mut f = CapturedFormation::open(v.clone(), system).unwrap();
        let mut analyzer = Corrections::default();
        let expected = match variant {
            0 => {
                analyzer.next = vec![Err(CaptureAnalysis::NoMemory)];
                FormationJobState::NoMemory
            }
            1 => {
                analyzer.next = vec![Err(CaptureAnalysis::Unknown)];
                FormationJobState::AnalysisUnknown
            }
            2 => {
                // Paused proves no handoff; the next step dispatches again.
                analyzer.next = vec![Err(CaptureAnalysis::NoMemory), Err(CaptureAnalysis::Paused)];
                FormationJobState::NoMemory
            }
            3 => {
                // An update is not a correction report's change.
                let mut s = correcting(&placeholder(&e, &v, &request), QUOTE);
                s.dimensions.change = FormationChange::Update;
                analyzer.next = vec![Err(CaptureAnalysis::Statements(vec![s]))];
                FormationJobState::Rejected
            }
            4 => {
                let s = correcting(&placeholder(&e, &v, &request), QUOTE);
                analyzer.next = vec![Err(CaptureAnalysis::Statements(vec![s.clone(), s]))];
                FormationJobState::Rejected
            }
            5 => {
                let s = correcting(&placeholder(&e, &v, &request), "not in the source");
                analyzer.next = vec![Err(CaptureAnalysis::Statements(vec![s]))];
                FormationJobState::Rejected
            }
            _ => {
                // Restart with analysis in flight: terminal unknown, no resend.
                analyzer.next = vec![Ok(None)];
                assert!(
                    f.step_corrections(&mut analyzer, &CaptureStopToken::default())
                        .unwrap()
                );
                assert!(f.has_pending());
                drop(f);
                f = CapturedFormation::open(v.clone(), system).unwrap();
                FormationJobState::AnalysisUnknown
            }
        };
        drain(&mut f, &mut analyzer);
        drain(&mut f, &mut analyzer);
        let status = f.status();
        assert_eq!(status.len(), 1, "variant {variant}");
        assert_eq!(status[0].state, expected, "variant {variant}");
        assert!(status[0].receipts.is_empty());
        assert_eq!(
            analyzer.calls,
            if variant == 2 { 2 } else { 1 },
            "variant {variant}"
        );
        let memory = &request.target.memory.memory_id;
        let current = revisions(&e, &v, memory);
        assert_eq!(current.len(), 1);
        assert_eq!(current[0].0, request.target, "variant {variant}");
    }
}
fn placeholder(
    e: &Env,
    v: &SegmentedAutomationVault,
    request: &AgentCorrectionRequest,
) -> CorrectionTarget {
    v.read(&e.descriptor.system_actor, Sensitivity::Private, |d| {
        let m = d
            .memories
            .iter()
            .find(|m| m.target == request.target)
            .unwrap();
        let input = &m.memory.decision.input;
        CorrectionTarget {
            target: m.target.clone(),
            content: input.content.clone(),
            title: String::new(),
            interpretation: input.interpretation,
            dimensions: input.dimensions.clone().unwrap(),
            sensitivity: input.sensitivity,
        }
    })
    .unwrap()
}

#[test]
fn deleted_inputs_close_without_analysis_and_a_moved_target_is_refused_not_retargeted() {
    for case in 0..3 {
        let (e, v, p, request) = correction_rig(SpeakerRole::User);
        saved(&e, &v, &p, &request);
        let owner = &e.descriptor.created_by;
        let system = &e.descriptor.system_actor;
        let mut f = CapturedFormation::open(v.clone(), system).unwrap();
        let mut analyzer = Corrections::default();
        match case {
            0 => {
                v.delete(owner, vec![], vec![request.target.memory.memory_id.clone()])
                    .unwrap();
            }
            1 => {
                v.delete(owner, vec![request.source.source_id.clone()], vec![])
                    .unwrap();
            }
            _ => {
                // Analysis in flight while another System revision lands.
                let target = placeholder(&e, &v, &request);
                analyzer.next = vec![Ok(Some(CaptureAnalysis::Statements(vec![correcting(
                    &target, QUOTE,
                )])))];
                assert!(
                    f.step_corrections(&mut analyzer, &CaptureStopToken::default())
                        .unwrap()
                );
                revise(&e, &v, &p, &request);
            }
        }
        let before = v.snapshot().unwrap();
        drain(&mut f, &mut analyzer);
        let status = f.status();
        assert_eq!(status[0].state, FormationJobState::Rejected, "case {case}");
        assert!(status[0].receipts.is_empty());
        assert_eq!(analyzer.calls, usize::from(case == 2), "case {case}");
        assert_eq!(v.snapshot().unwrap(), before, "case {case}");
    }
}

fn initialize_corrections(server: &mut enouia_memory_host::mcp::corrections::Server<'_, '_>) {
    assert!(server.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"synthetic-client","version":"1"}}})).unwrap().get("result").is_some());
    assert!(
        server
            .handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}))
            .is_none()
    );
}

#[test]
fn explicit_correction_server_pairs_authority_and_returns_only_report_saved_receipts() {
    use enouia_memory_host::mcp::{corrections, initialized};
    let (e, v, p, request) = correction_rig_with(SpeakerRole::User, Faults::none());
    let host = QualifiedAgentHost::from_authenticated_initialized_owner(
        v.clone(),
        e.descriptor.created_by.clone(),
    )
    .unwrap();
    let principal = actor(ActorType::Agent, 88);
    let reader = host
        .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let writer = host
        .correction_reporter(&e.descriptor.created_by, principal.clone(), scope(&p))
        .unwrap();
    let other = host
        .correction_reporter(
            &e.descriptor.created_by,
            actor(ActorType::Agent, 89),
            scope(&p),
        )
        .unwrap();
    let profile: AttentionProfile =
        serde_json::from_value(value("attention/profile.json")).unwrap();
    assert!(corrections::Server::new(&reader, &other, &profile).is_none());
    let (foreign_e, foreign_v, foreign_p, _) =
        correction_rig_with(SpeakerRole::User, Faults::none());
    let foreign_host = QualifiedAgentHost::from_authenticated_initialized_owner(
        foreign_v,
        foreign_e.descriptor.created_by.clone(),
    )
    .unwrap();
    let foreign = foreign_host
        .correction_reporter(
            &foreign_e.descriptor.created_by,
            principal,
            scope(&foreign_p),
        )
        .unwrap();
    assert_eq!(&foreign_e.descriptor.vault_id, reader.vault_id()); // Same synthetic logical ID, different lease.
    assert!(corrections::Server::new(&reader, &foreign, &profile).is_none());
    let mut server = corrections::Server::new(&reader, &writer, &profile).unwrap();
    initialize_corrections(&mut server);
    let listed = server
        .handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
        .unwrap();
    let tools = listed["result"]["tools"].as_array().unwrap();
    assert_eq!(tools.len(), 6);
    assert_eq!(tools[5]["name"], "memory_correction_report");
    assert_eq!(tools[5]["annotations"]["readOnlyHint"], false);
    assert!(!tools[5]["inputSchema"].to_string().contains("../"));
    assert!(!tools[5]["outputSchema"].to_string().contains("../"));
    let read_packet =
        schema_support::fixture("../agent-initialized-read/manifest.json")["valid"][0].clone();
    let read = server.handle(&json!({"jsonrpc":"2.0","id":"read-before-note","method":"tools/call","params":{"name":"memory_search","arguments":read_packet}})).unwrap();
    assert_eq!(
        read["result"]["structuredContent"]["capability"],
        "memory-agent-initialized-read-1"
    );
    assert!(read["result"]["structuredContent"]["error"].is_null());
    let mut call = json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"memory_correction_report","arguments":{"schema_version":2,"kind":"agent_correction_request","capability":corrections::CAPABILITY,"request":request}}});
    let before = v.snapshot().unwrap();
    for n in 0..5 {
        let mut bad = call.clone();
        bad["id"] = json!(20 + n);
        match n {
            0 => bad["params"]["arguments"]["owner"] = json!(true),
            1 => bad["params"]["arguments"]["schema_version"] = json!(1),
            2 => {
                bad["params"]["arguments"]["capability"] = json!("memory-agent-initialized-read-1")
            }
            3 => {
                bad["params"]["arguments"]["request"]["principal"] = json!(e.descriptor.created_by)
            }
            _ => bad["params"]["arguments"]["request"]["source"]["source_revision"] = json!(0),
        }
        assert!(server.handle(&bad).unwrap().get("error").is_some());
    }
    assert_eq!(v.snapshot().unwrap(), before);
    let mut refused = call.clone();
    refused["id"] = json!("known-refusal");
    refused["params"]["arguments"]["request"]["quote_hint"] = json!("No such quote.");
    let refusal = server.handle(&refused).unwrap();
    let refusal_data = &refusal["result"]["structuredContent"];
    assert_eq!(refusal_data["error"]["code"], "permission_denied");
    assert!(refusal_data["receipt"].is_null());
    assert!(!server.closed());
    assert!(
        schema_support::schema::SchemaStore::load()
            .validate("next/agent-correction-mcp-v1.schema.json", refusal_data)
            .is_empty()
    );
    let response = server.handle(&call).unwrap();
    let data = &response["result"]["structuredContent"];
    assert_eq!(data["receipt"]["state"], "report_saved");
    assert!(data["error"].is_null());
    assert!(
        schema_support::schema::SchemaStore::load()
            .validate("next/agent-correction-mcp-v1.schema.json", data)
            .is_empty()
    );
    assert!(data.get("scope_hash").is_none() && data.get("principal").is_none());
    assert!(data.get("source").is_none() && data.get("memory_id").is_none());
    let saved = v.snapshot().unwrap();
    assert_ne!(saved, before);
    revise(&e, &v, &p, &request);
    let saved = v.snapshot().unwrap();
    call["id"] = json!(4);
    assert_eq!(
        server.handle(&call).unwrap()["result"]["structuredContent"],
        *data
    );
    assert_eq!(v.snapshot().unwrap(), saved);
    assert!(server.handle(&call).unwrap().get("error").is_some()); // Duplicate MCP ID never writes.
    let mut old = initialized::Server::new(&reader, &profile).unwrap();
    old.handle(&json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"old-client","version":"1"}}}));
    old.handle(&json!({"jsonrpc":"2.0","method":"notifications/initialized"}));
    assert_eq!(
        old.handle(&json!({"jsonrpc":"2.0","id":2,"method":"tools/list"}))
            .unwrap()["result"]["tools"]
            .as_array()
            .unwrap()
            .len(),
        5
    );
    assert!(old.handle(&call).unwrap().get("error").is_some());
    assert_eq!(v.snapshot().unwrap(), saved);
}

#[test]
fn correction_server_unknown_returns_no_tool_result_and_keeps_the_entire_original_mcp_value() {
    use enouia_memory_host::mcp::corrections;
    for point in [FaultPoint::StagingWritten, FaultPoint::AfterCurrent] {
        let faults = Faults::armed();
        let (e, v, p, request) = correction_rig_with(SpeakerRole::User, faults.clone());
        let host = QualifiedAgentHost::from_authenticated_initialized_owner(
            v,
            e.descriptor.created_by.clone(),
        )
        .unwrap();
        let principal = actor(ActorType::Agent, 88);
        let reader = host
            .reader(&e.descriptor.created_by, principal.clone(), scope(&p))
            .unwrap();
        let writer = host
            .correction_reporter(&e.descriptor.created_by, principal, scope(&p))
            .unwrap();
        let profile: AttentionProfile =
            serde_json::from_value(value("attention/profile.json")).unwrap();
        let mut server = corrections::Server::new(&reader, &writer, &profile).unwrap();
        initialize_corrections(&mut server);
        let message = json!({"jsonrpc":"2.0","id":"original-note","method":"tools/call","params":{"name":"memory_correction_report","arguments":{"schema_version":2,"kind":"agent_correction_request","capability":corrections::CAPABILITY,"request":request}}});
        faults.arm(point, 1, FaultAction::Fail(118));
        assert!(server.handle(&message).is_none());
        assert!(server.outcome_unknown() && server.closed());
        assert_eq!(server.original_unknown(), Some(&message));
        let mut changed = message.clone();
        changed["id"] = json!("replacement");
        assert!(server.handle(&changed).is_none());
        assert_eq!(server.original_unknown(), Some(&message));
        assert_eq!(
            digest(writer.original_unknown().unwrap().as_ref().unwrap()),
            digest(&request)
        );
    }
}

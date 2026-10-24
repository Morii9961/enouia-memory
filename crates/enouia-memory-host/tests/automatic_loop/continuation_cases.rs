use super::*;
use enouia_memory_contract::foundation::Clock;
use enouia_memory_host::automation::continuation::{
    CONTINUATION_PATH, ContinuousPolicyHost, PauseReason,
};
fn policy(e: &Env) -> ContinuationPolicy {
    let mut p: ContinuationPolicy =
        serde_json::from_value(support::fixture("../continuation/policy.json")).unwrap();
    p.admission_policy = e.policy.reference();
    p.source_ids = e.policy.source_ids.clone();
    p
}
fn controller(e: &Env, jobs: &SyntheticJobs) -> ContinuousPolicyHost {
    ContinuousPolicyHost::open(
        jobs.clone(),
        e.descriptor.vault_id.clone(),
        e.descriptor.created_by.clone(),
        e.descriptor.system_actor.clone(),
        e.clock.clone(),
        Faults::none(),
    )
    .unwrap()
}
fn activate(e: &Env, host: &AutomationHost, c: &ContinuousPolicyHost) {
    let state = host.inspect_native().unwrap();
    with_stored_context(
        &state.data,
        &state.snapshot(),
        &e.descriptor.system_actor,
        &Timestamp::from_unix_ms(e.clock.now_unix_ms()).unwrap(),
        |context| c.activate(context),
    )
    .unwrap();
}
fn input(e: &Env, n: u8) -> AdmissionInput {
    let mut input = e.input(n, None);
    input.model_version = Some("fake-model-1".into());
    input
}
#[derive(Clone, Copy)]
enum Mode {
    Complete,
    Pending,
    Overrun,
    Unknown,
    PausedAfterHandoff,
}
struct Probe {
    calls: Arc<AtomicU64>,
    mode: Mode,
}
struct ProbePending {
    inner: Box<dyn PendingGeneration>,
    mode: Mode,
}
impl Generator for Probe {
    fn start(
        &mut self,
        input: &AdmissionInput,
        limits: &Limits,
        stop: &StopToken,
    ) -> std::result::Result<Box<dyn PendingGeneration>, Generation> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let inner = Model {
            mode: ModelMode::Faithful,
        }
        .start(input, limits, stop)?;
        Ok(Box::new(ProbePending {
            inner,
            mode: self.mode,
        }))
    }
}
impl PendingGeneration for ProbePending {
    fn poll(&mut self, stop: &StopToken) -> Option<Generation> {
        match self.mode {
            Mode::Pending => None,
            Mode::Unknown => Some(Generation::Unknown),
            Mode::PausedAfterHandoff => Some(Generation::Paused),
            _ => self.inner.poll(stop).map(|mut result| {
                if matches!(self.mode, Mode::Overrun)
                    && let Generation::Complete(r) = &mut result
                {
                    r.tokens_used = 150;
                    r.cost_microusd = 25;
                }
                result
            }),
        }
    }
}
fn generator(
    c: &ContinuousPolicyHost,
    p: &ContinuationPolicy,
    calls: &Arc<AtomicU64>,
    mode: Mode,
) -> impl Generator {
    c.fake_generator(
        p.destination.clone(),
        Probe {
            calls: calls.clone(),
            mode,
        },
    )
    .unwrap()
}

#[test]
fn no_policy_pauses_without_spending_then_a_native_grant_enables_automatic_formation() {
    let e = Env::new();
    let jobs = e.jobs();
    let c = controller(&e, &jobs);
    let p = policy(&e);
    let host = e
        .host(e.vault(Faults::none()), jobs.clone())
        .with_continuation(c.clone())
        .unwrap();
    host.capture_native(e.source(1, "I prefer autumn walks.", SpeakerRole::User))
        .unwrap();
    activate(&e, &host, &c);
    let host = Arc::new(host);
    let calls = Arc::new(AtomicU64::new(0));
    let thread = host
        .start_worker(generator(&c, &p, &calls, Mode::Complete))
        .unwrap();
    let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(jobs.status(&key).unwrap().reserved_tokens, 0);
    assert!(c.inspect_native().unwrap().attempts.is_empty());
    c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    activate(&e, &host, &c);
    assert_eq!(host.prepare_submit(vec![input(&e, 1)]).unwrap(), key);
    let until = Instant::now() + Duration::from_secs(10);
    while host.receipts(&key).ok().flatten().is_none() {
        assert!(Instant::now() < until);
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let ledger = c.inspect_native().unwrap();
    assert_eq!(ledger.charges(), Some((100, 10)));
    assert_eq!(ledger.attempts[0].state, ContinuationState::Complete);
    assert!(
        host.context("autumn", Sensitivity::Private, 8)
            .unwrap()
            .contains("autumn")
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn revocation_cancels_inflight_and_prevents_late_publication_or_refunds() {
    let e = Env::new();
    let jobs = e.jobs();
    let c = controller(&e, &jobs);
    let mut p = policy(&e);
    c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let host = e
        .host(e.vault(Faults::none()), jobs.clone())
        .with_continuation(c.clone())
        .unwrap();
    host.capture_native(e.source(1, "I prefer a quiet room.", SpeakerRole::User))
        .unwrap();
    activate(&e, &host, &c);
    let host = Arc::new(host);
    let calls = Arc::new(AtomicU64::new(0));
    let thread = host
        .start_worker(generator(&c, &p, &calls, Mode::Pending))
        .unwrap();
    let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
    wait(&jobs, &key, JobState::Running);
    let until = Instant::now() + Duration::from_secs(5);
    while calls.load(Ordering::SeqCst) == 0 {
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    p.revision = Revision::new(2).unwrap();
    p.active = false;
    c.set_policy(&e.descriptor.created_by, p).unwrap();
    assert!(host.apply_job(&key).is_err());
    assert!(host.inspect_native().unwrap().data.memories.is_empty());
    let ledger = c.inspect_native().unwrap();
    assert_eq!(ledger.attempts[0].state, ContinuationState::Revoked);
    assert_eq!(ledger.charges(), Some((100, 10)));
    assert_eq!(ledger.policy_history.len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(host);
    thread.join().unwrap();
}
#[test]
fn completed_generation_cannot_cross_a_later_policy_withdrawal_into_application() {
    let e = Env::new();
    let jobs = e.jobs();
    let c = controller(&e, &jobs);
    let mut p = policy(&e);
    c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let host = e
        .host(e.vault(Faults::none()), jobs.clone())
        .with_continuation(c.clone())
        .unwrap();
    host.capture_native(e.source(1, "A withheld generation.", SpeakerRole::User))
        .unwrap();
    activate(&e, &host, &c);
    let calls = Arc::new(AtomicU64::new(0));
    let thread = jobs
        .spawn(generator(&c, &p, &calls, Mode::Complete))
        .unwrap();
    let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    p.revision = Revision::new(2).unwrap();
    p.active = false;
    c.set_policy(&e.descriptor.created_by, p).unwrap();
    assert!(host.apply_job(&key).is_err());
    assert!(host.inspect_native().unwrap().data.memories.is_empty());
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    drop(host);
    thread.join().unwrap();
}
#[test]
fn unknown_and_overrun_charges_survive_restart_and_policy_revisions_without_resend() {
    for mode in [Mode::Unknown, Mode::Overrun, Mode::PausedAfterHandoff] {
        let e = Env::new();
        let jobs = e.jobs();
        let c = controller(&e, &jobs);
        let p = policy(&e);
        c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let host = e
            .host(e.vault(Faults::none()), jobs.clone())
            .with_continuation(c.clone())
            .unwrap();
        host.capture_native(e.source(1, "A bounded attempt.", SpeakerRole::User))
            .unwrap();
        activate(&e, &host, &c);
        let calls = Arc::new(AtomicU64::new(0));
        let thread = jobs.spawn(generator(&c, &p, &calls, mode)).unwrap();
        let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
        wait(
            &jobs,
            &key,
            if matches!(mode, Mode::Overrun) {
                JobState::Failed
            } else {
                JobState::OutcomeUnknown
            },
        );
        assert!(host.apply_job(&key).is_err());
        let saved = c.inspect_native().unwrap();
        let charges = saved.charges().unwrap();
        assert_eq!(
            charges,
            if matches!(mode, Mode::Overrun) {
                (150, 25)
            } else {
                (100, 10)
            }
        );
        drop(host);
        thread.join().unwrap();
        drop(c);
        drop(jobs);
        let jobs = e.jobs();
        let c = controller(&e, &jobs);
        assert_eq!(c.inspect_native().unwrap().charges(), Some(charges));
        let host = e
            .host(e.open(), jobs.clone())
            .with_continuation(c.clone())
            .unwrap();
        activate(&e, &host, &c);
        let _ = host.prepare_submit(vec![input(&e, 1)]);
        let expected = if matches!(mode, Mode::Overrun) {
            JobState::Failed
        } else {
            JobState::OutcomeUnknown
        };
        assert_eq!(jobs.status(&key).unwrap().state, expected);
        let thread = jobs
            .spawn(generator(&c, &p, &calls, Mode::Complete))
            .unwrap();
        std::thread::sleep(Duration::from_millis(50));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        let mut renewed = p.clone();
        renewed.revision = Revision::new(2).unwrap();
        c.set_policy(&e.descriptor.created_by, renewed).unwrap();
        assert_eq!(c.inspect_native().unwrap().charges(), Some(charges));
        drop(host);
        thread.join().unwrap();
    }
}
#[test]
fn destination_scope_and_budget_gate_retains_queued_inputs_without_calling_transport() {
    let e = Env::new();
    let jobs = e.jobs();
    let c = controller(&e, &jobs);
    let mut p = policy(&e);
    p.destination.model = "other".into();
    c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let host = e
        .host(e.vault(Faults::none()), jobs.clone())
        .with_continuation(c.clone())
        .unwrap();
    host.capture_native(e.source(1, "No model substitution.", SpeakerRole::User))
        .unwrap();
    activate(&e, &host, &c);
    let calls = Arc::new(AtomicU64::new(0));
    let thread = jobs
        .spawn(generator(&c, &p, &calls, Mode::Complete))
        .unwrap();
    let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert_eq!(jobs.status(&key).unwrap().cursor, 0);
    assert!(c.inspect_native().unwrap().attempts.is_empty());
    let mut live = p.destination.clone();
    live.api = GenerationApi::OpenaiResponses;
    assert!(
        c.fake_generator(
            live,
            Model {
                mode: ModelMode::Faithful
            }
        )
        .is_err()
    );
    drop(host);
    thread.join().unwrap();
}
#[test]
fn continuation_lease_corrupt_major_and_native_identity_are_fenced() {
    let e = Env::new();
    let jobs = e.jobs();
    let c = controller(&e, &jobs);
    assert!(matches!(
        ContinuousPolicyHost::open(
            jobs.clone(),
            e.descriptor.vault_id.clone(),
            e.descriptor.created_by.clone(),
            e.descriptor.system_actor.clone(),
            e.clock.clone(),
            Faults::none()
        ),
        Err(Fault::Busy)
    ));
    assert!(
        c.set_policy(&e.descriptor.system_actor, policy(&e))
            .is_err()
    );
    drop(c);
    let file = e.jobs_root.path().join(CONTINUATION_PATH);
    let original = std::fs::read(&file).unwrap();
    let mut value: Value = serde_json::from_slice(&original).unwrap();
    value["schema_version"] = json!(99);
    let corrupt = canonical_bytes(&value).unwrap();
    std::fs::write(&file, &corrupt).unwrap();
    assert!(
        ContinuousPolicyHost::open(
            jobs,
            e.descriptor.vault_id.clone(),
            e.descriptor.created_by.clone(),
            e.descriptor.system_actor.clone(),
            e.clock.clone(),
            Faults::none()
        )
        .is_err()
    );
    assert_eq!(std::fs::read(file).unwrap(), corrupt);
}

fn saved_input(e: &Env, key: &Sha256Hex) -> AdmissionInput {
    let bytes = std::fs::read(e.jobs_root.path().join(JOURNAL_PATH)).unwrap();
    Journal::parse(&bytes)
        .unwrap()
        .jobs
        .into_iter()
        .find(|j| &j.key == key)
        .unwrap()
        .inputs[0]
        .clone()
}
#[test]
fn total_budget_is_shared_across_jobs_and_a_revision_does_not_reset_charges() {
    let e = Env::new();
    let jobs = e.jobs();
    let c = controller(&e, &jobs);
    let mut p = policy(&e);
    p.total_tokens = 100;
    p.total_cost_microusd = 10;
    c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let host = e
        .host(e.vault(Faults::none()), jobs.clone())
        .with_continuation(c.clone())
        .unwrap();
    for n in 1..=2 {
        host.capture_native(e.source(
            n,
            &format!("A distinct bounded statement {n}."),
            SpeakerRole::User,
        ))
        .unwrap();
    }
    activate(&e, &host, &c);
    let calls = Arc::new(AtomicU64::new(0));
    let thread = jobs
        .spawn(generator(&c, &p, &calls, Mode::Complete))
        .unwrap();
    let first = host.prepare_submit(vec![input(&e, 1)]).unwrap();
    wait(&jobs, &first, JobState::Generated);
    let second = host.prepare_submit(vec![input(&e, 2)]).unwrap();
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(jobs.status(&second).unwrap().state, JobState::Queued);
    assert_eq!(
        c.paused(&saved_input(&e, &second), &p.destination, &limits()),
        Some(PauseReason::Budget)
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(c.inspect_native().unwrap().charges(), Some((100, 10)));
    p.revision = Revision::new(2).unwrap();
    c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    activate(&e, &host, &c);
    assert_eq!(host.prepare_submit(vec![input(&e, 2)]).unwrap(), second);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(c.inspect_native().unwrap().charges(), Some((100, 10)));
    drop(host);
    thread.join().unwrap();
}
#[test]
fn expiry_or_lifecycle_lock_stops_pending_output_and_retains_reservation() {
    for expiry in [false, true] {
        let e = Env::new();
        let jobs = e.jobs();
        let c = controller(&e, &jobs);
        let mut p = policy(&e);
        p.valid_until = Timestamp::from_unix_ms(NOW + 1000).unwrap();
        c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let host = e
            .host(e.vault(Faults::none()), jobs.clone())
            .with_continuation(c.clone())
            .unwrap();
        host.capture_native(e.source(1, "An expiring request.", SpeakerRole::User))
            .unwrap();
        activate(&e, &host, &c);
        let calls = Arc::new(AtomicU64::new(0));
        let thread = jobs
            .spawn(generator(&c, &p, &calls, Mode::Pending))
            .unwrap();
        let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while calls.load(Ordering::SeqCst) == 0 {
            assert!(Instant::now() < until);
            std::thread::yield_now();
        }
        if expiry {
            e.clock.set(NOW + 1000);
            wait(&jobs, &key, JobState::OutcomeUnknown);
        } else {
            c.lock().unwrap();
        }
        assert!(host.apply_job(&key).is_err());
        assert!(host.inspect_native().unwrap().data.memories.is_empty());
        assert_eq!(c.inspect_native().unwrap().charges(), Some((100, 10)));
        drop(host);
        thread.join().unwrap();
    }
}
#[test]
fn reservation_publication_failure_never_reaches_transport() {
    for point in [FaultPoint::BeforeRename, FaultPoint::HostAuthorityPublished] {
        let e = Env::new();
        let jobs = e.jobs();
        let faults = Faults::armed();
        let c = ContinuousPolicyHost::open(
            jobs.clone(),
            e.descriptor.vault_id.clone(),
            e.descriptor.created_by.clone(),
            e.descriptor.system_actor.clone(),
            e.clock.clone(),
            faults.clone(),
        )
        .unwrap();
        let p = policy(&e);
        c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
        let host = e
            .host(e.vault(Faults::none()), jobs.clone())
            .with_continuation(c.clone())
            .unwrap();
        host.capture_native(e.source(1, "A failed reservation.", SpeakerRole::User))
            .unwrap();
        activate(&e, &host, &c);
        faults.arm(point, 1, FaultAction::Fail(5));
        let calls = Arc::new(AtomicU64::new(0));
        let thread = jobs
            .spawn(generator(&c, &p, &calls, Mode::Complete))
            .unwrap();
        let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
        let until = Instant::now() + Duration::from_secs(5);
        while c.inspect_native().is_ok() {
            assert!(Instant::now() < until);
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        assert!(host.apply_job(&key).is_err());
        drop(host);
        thread.join().unwrap();
        drop(c);
        drop(jobs);
        let jobs = e.jobs();
        let c = controller(&e, &jobs);
        let ledger = c.inspect_native().unwrap();
        if point == FaultPoint::HostAuthorityPublished {
            assert_eq!(ledger.attempts[0].state, ContinuationState::OutcomeUnknown);
            assert_eq!(ledger.charges(), Some((100, 10)));
        } else {
            assert!(ledger.attempts.is_empty());
        }
    }
}
#[test]
fn core4_process_child() {
    let Some(path) = std::env::var_os("ENOU_CORE4_DATA_ROOT") else {
        return;
    };
    let mut e = Env::new();
    e.root = verify_data_root(&PathBuf::from(path), &RootPolicy::default()).unwrap();
    e.jobs_root = verify_data_root(
        &PathBuf::from(std::env::var_os("ENOU_CORE4_JOB_ROOT").unwrap()),
        &RootPolicy::default(),
    )
    .unwrap();
    let jobs = e.jobs();
    let faults = Faults::armed();
    let c = ContinuousPolicyHost::open(
        jobs.clone(),
        e.descriptor.vault_id.clone(),
        e.descriptor.created_by.clone(),
        e.descriptor.system_actor.clone(),
        e.clock.clone(),
        faults.clone(),
    )
    .unwrap();
    let p = policy(&e);
    c.set_policy(&e.descriptor.created_by, p.clone()).unwrap();
    let host = e
        .host(e.vault(Faults::none()), jobs.clone())
        .with_continuation(c.clone())
        .unwrap();
    host.capture_native(e.source(1, "A crash-safe budget.", SpeakerRole::User))
        .unwrap();
    activate(&e, &host, &c);
    faults.arm(FaultPoint::HostAuthorityPublished, 1, FaultAction::Abort);
    let calls = Arc::new(AtomicU64::new(0));
    let _thread = jobs
        .spawn(generator(&c, &p, &calls, Mode::Complete))
        .unwrap();
    let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    panic!("injected crash did not fire");
}
#[test]
fn actual_process_crash_keeps_unknown_budget_and_cannot_resend() {
    let e = Env::new();
    let status = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "continuation_cases::core4_process_child",
            "--nocapture",
        ])
        .env("ENOU_CORE4_DATA_ROOT", e.root.path())
        .env("ENOU_CORE4_JOB_ROOT", e.jobs_root.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert_eq!(status.code(), Some(3));
    let jobs = e.jobs();
    let c = controller(&e, &jobs);
    let ledger = c.inspect_native().unwrap();
    assert_eq!(ledger.attempts[0].state, ContinuationState::OutcomeUnknown);
    assert_eq!(ledger.charges(), Some((100, 10)));
    let host = e
        .host(e.open(), jobs.clone())
        .with_continuation(c.clone())
        .unwrap();
    activate(&e, &host, &c);
    let calls = Arc::new(AtomicU64::new(0));
    let thread = jobs
        .spawn(generator(&c, &policy(&e), &calls, Mode::Complete))
        .unwrap();
    let _ = host.prepare_submit(vec![input(&e, 1)]);
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    assert!(host.inspect_native().unwrap().data.memories.is_empty());
    drop(host);
    thread.join().unwrap();
}

struct ProvenNoHandoff {
    paused: Arc<std::sync::atomic::AtomicBool>,
    resume: Arc<std::sync::atomic::AtomicBool>,
    transport: Probe,
}
impl Generator for ProvenNoHandoff {
    fn ready(&mut self, _: &AdmissionInput, _: &Limits) -> bool {
        !self.paused.load(Ordering::SeqCst) || self.resume.load(Ordering::SeqCst)
    }
    fn start(
        &mut self,
        input: &AdmissionInput,
        limits: &Limits,
        stop: &StopToken,
    ) -> std::result::Result<Box<dyn PendingGeneration>, Generation> {
        if !self.resume.load(Ordering::SeqCst) {
            self.paused.store(true, Ordering::SeqCst);
            return Err(Generation::Paused);
        }
        self.transport.start(input, limits, stop)
    }
}
#[test]
fn proven_pre_handoff_pause_keeps_work_and_undoes_only_the_unsent_local_reservation() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    host.capture_native(e.source(1, "A safely paused handoff.", SpeakerRole::User))
        .unwrap();
    let paused = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let resume = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let calls = Arc::new(AtomicU64::new(0));
    let thread = jobs
        .spawn(ProvenNoHandoff {
            paused: paused.clone(),
            resume: resume.clone(),
            transport: Probe {
                calls: calls.clone(),
                mode: Mode::Complete,
            },
        })
        .unwrap();
    let key = host.prepare_submit(vec![input(&e, 1)]).unwrap();
    let until = Instant::now() + Duration::from_secs(5);
    while !paused.load(Ordering::SeqCst) {
        assert!(Instant::now() < until);
        std::thread::yield_now();
    }
    wait(&jobs, &key, JobState::Queued);
    assert_eq!(jobs.status(&key).unwrap().reserved_tokens, 0);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    resume.store(true, Ordering::SeqCst);
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(jobs.status(&key).unwrap().reserved_tokens, 100);
    drop(host);
    thread.join().unwrap();
}

//! Core-2: real atomic journal/OS lock, synthetic generation only.
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod support;

use enouia_memory_contract::{
    foundation::{Clock, FakeClock},
    hash::Sha256Hex,
    ids::{MemoryId, OperationId},
    json::canonical_bytes,
    next::*,
    source::SourceRecord,
    time::Timestamp,
};
use enouia_memory_host::automation::*;
use enouia_memory_vault::{
    RootPolicy, VerifiedRoot,
    fault::{FaultAction, FaultPoint, Faults},
    verify_data_root,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};

static ROOTS: AtomicU64 = AtomicU64::new(0);
const NOW: i64 = 1_792_843_200_000;
fn fixture(name: &str) -> Value {
    support::fixture(&format!("../automation/{name}.json"))
}
fn limits() -> Limits {
    serde_json::from_value(fixture("queued")["limits"].clone()).unwrap()
}
fn draft(input: &AdmissionInput) -> DecisionDraft {
    let mut draft: DecisionDraft =
        serde_json::from_value(support::fixture("../next/decision.json")).unwrap();
    draft.input = input.clone();
    draft
}
struct Env {
    root: VerifiedRoot,
    clock: Arc<FakeClock>,
    input: AdmissionInput,
    policy: AutomationPolicy,
    sources: Vec<SourceRecord>,
}
impl Env {
    fn new() -> Self {
        let base = std::env::temp_dir().join("enouia-memory-core2-tests");
        std::fs::create_dir_all(&base).unwrap();
        let base = std::fs::canonicalize(base).unwrap();
        let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
        let path = base.join(format!(
            "jobs-{}-{}",
            std::process::id(),
            ROOTS.fetch_add(1, Ordering::SeqCst)
        ));
        std::fs::create_dir(&path).unwrap();
        Self::at(path)
    }
    fn at(path: PathBuf) -> Self {
        Self {
            root: verify_data_root(&path, &RootPolicy::default()).unwrap(),
            clock: Arc::new(FakeClock::new(NOW)),
            input: serde_json::from_value(support::fixture("../next/input.json")).unwrap(),
            policy: serde_json::from_value(support::fixture("../next/policy.json")).unwrap(),
            sources: serde_json::from_value(support::fixture("../next/sources.json")).unwrap(),
        }
    }
    fn open(&self, limits: Limits, faults: Faults) -> SyntheticJobs {
        SyntheticJobs::open(&self.root, limits, self.clock.clone(), faults).unwrap()
    }
    fn context<T>(&self, count: usize, action: impl FnOnce(&[EvaluationContext<'_>]) -> T) -> T {
        let sources: Vec<_> = self
            .sources
            .iter()
            .map(|record| SourceView {
                record,
                attributed_subject: Some(self.input.subject_id.clone()),
                readable: true,
                deleted: false,
            })
            .collect();
        let now = Timestamp::from_unix_ms(self.clock.now_unix_ms()).unwrap();
        let contexts: Vec<_> = (0..count)
            .map(|_| EvaluationContext {
                system_actor: &self.input.actor,
                policy: &self.policy,
                snapshot: &self.input.snapshot,
                now: &now,
                sources: &sources,
                targets: &[],
            })
            .collect();
        action(&contexts)
    }
    fn activate(&self, jobs: &SyntheticJobs) {
        self.context(1, |ctx| {
            jobs.activate(Permit::from_context(&ctx[0]).unwrap())
        })
        .unwrap();
    }
    fn submit(&self, jobs: &SyntheticJobs, inputs: Vec<AdmissionInput>) -> Sha256Hex {
        self.context(inputs.len(), |ctx| jobs.submit(inputs, ctx))
            .unwrap()
    }
    fn batch(&self) -> Vec<AdmissionInput> {
        let mut second = self.input.clone();
        second.content.push_str(" Second synthetic work unit.");
        second.operation_id = OperationId::from_random([2; 16]);
        second.result.memory_id = MemoryId::from_random([2; 16]);
        vec![self.input.clone(), second]
    }
    fn journal(&self) -> Journal {
        Journal::parse(&std::fs::read(self.root.path().join(JOURNAL_PATH)).unwrap()).unwrap()
    }
}

#[derive(Clone)]
enum Mode {
    Complete,
    Pending,
    Unknown,
    Refused,
    PanicStart,
    PanicPoll,
    WrongInput,
    OverBudget,
    ChangedResponse,
    MalformedJson,
}
struct Fake {
    calls: Arc<AtomicUsize>,
    mode: Mode,
    pause: Option<SyntheticJobs>,
    entered: Option<mpsc::Sender<()>>,
    release: Option<mpsc::Receiver<()>>,
    call_file: Option<PathBuf>,
}
impl Fake {
    fn new(mode: Mode) -> Self {
        Self {
            calls: Arc::new(AtomicUsize::new(0)),
            mode,
            pause: None,
            entered: None,
            release: None,
            call_file: None,
        }
    }
}
struct Pending {
    input: AdmissionInput,
    mode: Mode,
    pause: Option<SyntheticJobs>,
    entered: Option<mpsc::Sender<()>>,
    release: Option<mpsc::Receiver<()>>,
}
impl Generator for Fake {
    fn start(
        &mut self,
        input: &AdmissionInput,
        _limits: &Limits,
        stop: &StopToken,
    ) -> std::result::Result<Box<dyn PendingGeneration>, Generation> {
        assert!(!stop.is_cancelled());
        self.calls.fetch_add(1, Ordering::SeqCst);
        if let Some(path) = &self.call_file {
            use std::io::Write;
            let mut f = std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(path)
                .unwrap();
            writeln!(f, "synthetic dispatch").unwrap();
            f.sync_all().unwrap();
        }
        if matches!(self.mode, Mode::PanicStart) {
            panic!("synthetic start panic");
        }
        Ok(Box::new(Pending {
            input: input.clone(),
            mode: self.mode.clone(),
            pause: self.pause.take(),
            entered: self.entered.take(),
            release: self.release.take(),
        }))
    }
}
impl PendingGeneration for Pending {
    fn poll(&mut self, _stop: &StopToken) -> Option<Generation> {
        if let Some(entered) = self.entered.take() {
            entered.send(()).unwrap();
        }
        if let Some(release) = self.release.take() {
            release.recv_timeout(Duration::from_secs(15)).unwrap();
        }
        if let Some(pause) = self.pause.take() {
            pause.foreground(true).unwrap();
        }
        match self.mode {
            Mode::Pending => None,
            Mode::PanicPoll => panic!("synthetic poll panic"),
            Mode::Unknown => Some(Generation::Unknown),
            Mode::Refused => Some(Generation::Refused),
            _ => {
                let mut result = draft(&self.input);
                if matches!(self.mode, Mode::ChangedResponse) {
                    result
                        .input
                        .content
                        .push_str(" Generated analysis differs from the request.");
                }
                Some(Generation::Complete(Box::new(Generated {
                    input_hash: if matches!(self.mode, Mode::WrongInput) {
                        enouia_memory_contract::hash::sha256(b"another input")
                    } else {
                        digest(&self.input)
                    },
                    response_json: if matches!(self.mode, Mode::MalformedJson) {
                        "{invalid".into()
                    } else {
                        serde_json::to_string(&result).unwrap()
                    },
                    tokens_used: if matches!(self.mode, Mode::OverBudget) {
                        101
                    } else {
                        5
                    },
                    cost_microusd: 1,
                })))
            }
        }
    }
}
fn wait(jobs: &SyntheticJobs, key: &Sha256Hex, predicate: impl Fn(&Status) -> bool) -> Status {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(status) = jobs.status(key)
            && predicate(&status)
        {
            return status;
        }
        assert!(
            Instant::now() < until,
            "job did not reach expected status: {:?}",
            jobs.status(key)
        );
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn end(jobs: &SyntheticJobs, worker: std::thread::JoinHandle<()>) {
    jobs.shutdown().unwrap();
    worker.join().unwrap();
}

#[test]
fn journal_schema_and_native_validation_cover_positive_and_negative_fixtures() {
    let schemas = support::schema::SchemaStore::load();
    let base = fixture("queued");
    let manifest = fixture("manifest");
    assert!(
        schemas
            .validate("next/job-journal-v2.schema.json", &base)
            .is_empty()
    );
    Journal::parse(&canonical_bytes(&base).unwrap()).unwrap();
    let generated = fixture("generated");
    assert!(
        schemas
            .validate("next/job-journal-v2.schema.json", &generated)
            .is_empty()
    );
    Journal::parse(&canonical_bytes(&generated).unwrap()).unwrap();
    for case in manifest["invalid"].as_array().unwrap() {
        let mut value = base.clone();
        support::apply_ops(&mut value, &case["ops"]);
        assert_eq!(
            schemas
                .validate("next/job-journal-v2.schema.json", &value)
                .is_empty(),
            case["schema"] == "accept",
            "{}",
            case["id"]
        );
        assert!(
            Journal::parse(&canonical_bytes(&value).unwrap()).is_err(),
            "{}",
            case["id"]
        );
    }
    let raw = serde_json::to_string(&base).unwrap().replace(
        "\"schema_version\":2",
        "\"schema_version\":2,\"schema_version\":2",
    );
    assert_eq!(Journal::parse(raw.as_bytes()).err(), Some(Fault::Invalid));
}

#[test]
fn exact_work_dedupes_generated_ids_and_reopen_preserves_original_results() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    let key = e.submit(&jobs, vec![e.input.clone()]);
    let mut replay = e.input.clone();
    replay.operation_id = OperationId::from_random([8; 16]);
    replay.result.memory_id = MemoryId::from_random([8; 16]);
    assert_eq!(e.submit(&jobs, vec![replay]), key);
    let fake = Fake::new(Mode::Complete);
    let calls = fake.calls.clone();
    let worker = jobs.spawn(fake).unwrap();
    wait(&jobs, &key, |s| s.state == JobState::Generated);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    let original = jobs.results(&key).unwrap();
    assert_eq!(original[0].input_hash, digest(&e.input));
    let decoded: DecisionDraft = serde_json::from_str(&original[0].response_json).unwrap();
    assert_eq!(decoded.input, e.input);
    end(&jobs, worker);
    drop(jobs);
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    e.context(1, |ctx| jobs.resume(&key, ctx)).unwrap();
    assert_eq!(jobs.results(&key).unwrap(), original);
    let fake = Fake::new(Mode::Complete);
    let calls = fake.calls.clone();
    let worker = jobs.spawn(fake).unwrap();
    assert_eq!(e.submit(&jobs, vec![e.input.clone()]), key);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    end(&jobs, worker);
}

#[test]
fn changed_payload_operation_and_overlapping_batch_are_refused() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    e.submit(&jobs, vec![e.input.clone()]);
    let mut changed = e.input.clone();
    changed.content.push_str(" changed");
    assert_eq!(
        e.context(1, |ctx| jobs.submit(vec![changed], ctx)).err(),
        Some(Fault::Conflict)
    );
    assert_eq!(
        e.context(2, |ctx| jobs.submit(e.batch(), ctx)).err(),
        Some(Fault::Conflict)
    );
    let mut duplicate = e.input.clone();
    duplicate.operation_id = OperationId::from_random([7; 16]);
    duplicate.result.memory_id = MemoryId::from_random([7; 16]);
    assert_eq!(
        e.context(2, |ctx| jobs.submit(vec![e.input.clone(), duplicate], ctx))
            .err(),
        Some(Fault::Invalid)
    );
}

#[test]
fn generated_response_is_not_predetermined_or_treated_as_admission() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    let key = e.submit(&jobs, vec![e.input.clone()]);
    let worker = jobs.spawn(Fake::new(Mode::ChangedResponse)).unwrap();
    wait(&jobs, &key, |s| s.state == JobState::Generated);
    let saved = jobs.results(&key).unwrap().remove(0);
    assert_eq!(saved.input_hash, digest(&e.input));
    assert_eq!(
        saved.response_hash,
        enouia_memory_contract::hash::sha256(saved.response_json.as_bytes())
    );
    let response: DecisionDraft = serde_json::from_str(&saved.response_json).unwrap();
    assert_ne!(response.input.content, e.input.content);
    assert_eq!(e.journal().jobs[0].inputs[0], e.input);
    assert!(!e.root.path().join("vault").exists());
    let mut tampered = e.journal();
    tampered.jobs[0].results[0].response_json.push(' ');
    assert_eq!(tampered.validate().err(), Some(Fault::Invalid));
    let mut tampered = e.journal();
    tampered.jobs[0].results[0].input_hash =
        enouia_memory_contract::hash::sha256(b"foreign request");
    assert_eq!(tampered.validate().err(), Some(Fault::Invalid));
    end(&jobs, worker);
}

#[test]
fn foreground_priority_blocks_new_dispatch_and_batch_cursor_advances_once() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    jobs.foreground(true).unwrap();
    let key = e.submit(&jobs, e.batch());
    let mut fake = Fake::new(Mode::Complete);
    fake.pause = Some(jobs.clone());
    let calls = fake.calls.clone();
    let worker = jobs.spawn(fake).unwrap();
    assert_eq!(jobs.status(&key).unwrap().state, JobState::Queued);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    jobs.foreground(false).unwrap();
    wait(&jobs, &key, |s| {
        s.cursor == 1 && s.state == JobState::Queued
    });
    assert_eq!(e.journal().jobs[0].results.len(), 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    jobs.foreground(false).unwrap();
    wait(&jobs, &key, |s| s.state == JobState::Generated);
    assert_eq!(jobs.results(&key).unwrap().len(), 2);
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    end(&jobs, worker);
}

#[test]
fn queued_cancel_and_lock_stop_dispatch_and_fresh_resume_is_explicit() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    let key = e.submit(&jobs, vec![e.input.clone()]);
    jobs.lock().unwrap();
    assert_eq!(jobs.status(&key).unwrap().state, JobState::Invalidated);
    assert_eq!(
        e.context(1, |ctx| jobs.resume(&key, ctx)).err(),
        Some(Fault::Unavailable)
    );
    e.activate(&jobs);
    e.context(1, |ctx| jobs.resume(&key, ctx)).unwrap();
    jobs.cancel(&key).unwrap();
    let fake = Fake::new(Mode::Complete);
    let calls = fake.calls.clone();
    let worker = jobs.spawn(fake).unwrap();
    assert_eq!(jobs.status(&key).unwrap().state, JobState::Cancelled);
    assert_eq!(
        e.context(1, |ctx| jobs.resume(&key, ctx)).err(),
        Some(Fault::Conflict)
    );
    end(&jobs, worker);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
}

#[test]
fn pending_work_does_not_hold_control_or_vault_writer_and_late_result_is_discarded() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    let key = e.submit(&jobs, vec![e.input.clone()]);
    let (entered_tx, entered_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let mut fake = Fake::new(Mode::Complete);
    fake.entered = Some(entered_tx);
    fake.release = Some(release_rx);
    let worker = jobs.spawn(fake).unwrap();
    entered_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    // The generator is deliberately stuck outside its bounded poll contract.
    assert_eq!(jobs.status(&key).unwrap().state, JobState::Running);
    let managed = enouia_memory_vault::fs::ManagedRoot::new(e.root.path(), Faults::none());
    let guard = enouia_memory_vault::lock::acquire(&managed, Duration::ZERO).unwrap();
    drop(guard);
    let (done_tx, done_rx) = mpsc::channel();
    let control = jobs.clone();
    let key_copy = key.clone();
    let controller = std::thread::spawn(move || {
        control.cancel(&key_copy).unwrap();
        control.shutdown().unwrap();
        done_tx.send(()).unwrap();
    });
    let control_ready = done_rx.recv_timeout(Duration::from_secs(5));
    release_tx.send(()).unwrap();
    controller.join().unwrap();
    worker.join().unwrap();
    control_ready.unwrap();
    let status = jobs.status(&key).unwrap();
    assert_eq!(status.state, JobState::Cancelled);
    assert!(status.uncertain);
    assert_eq!(status.cursor, 0);
}

#[test]
fn expiry_and_each_native_binding_change_invalidates_queued_work() {
    for field in [
        "vault_id",
        "base_commit_id",
        "primary_epoch",
        "policy_watermark",
        "deletion_watermark",
        "policy",
    ] {
        let mut e = Env::new();
        let jobs = e.open(limits(), Faults::none());
        e.activate(&jobs);
        let key = e.submit(&jobs, vec![e.input.clone()]);
        if field == "policy" {
            e.policy.revision = enouia_memory_contract::json::Revision::new(2).unwrap();
        } else {
            let mut snapshot = serde_json::to_value(&e.input.snapshot).unwrap();
            snapshot[field] = if field.ends_with("_id") {
                json!(format!(
                    "{}_00000009-0000-4000-8000-000000000001",
                    if field == "vault_id" { "vlt" } else { "cmt" }
                ))
            } else {
                json!(2)
            };
            e.input.snapshot = serde_json::from_value(snapshot).unwrap();
            if field == "vault_id" {
                e.policy.vault_id = e.input.snapshot.vault_id.clone();
            }
        }
        e.activate(&jobs);
        assert_eq!(jobs.status(&key).unwrap().state, JobState::Invalidated);
    }
    for regression in [false, true] {
        let e = Env::new();
        let jobs = e.open(limits(), Faults::none());
        e.activate(&jobs);
        let key = e.submit(&jobs, vec![e.input.clone()]);
        e.clock.set(if regression {
            NOW - 1
        } else {
            e.policy.valid_until.unix_ms()
        });
        let fake = Fake::new(Mode::Complete);
        let calls = fake.calls.clone();
        let worker = jobs.spawn(fake).unwrap();
        wait(&jobs, &key, |s| s.state == JobState::Invalidated);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
        end(&jobs, worker);
    }
}

#[test]
fn timeout_unknown_and_panics_never_retry_and_keep_budget_reserved() {
    for mode in [
        Mode::Pending,
        Mode::Unknown,
        Mode::PanicStart,
        Mode::PanicPoll,
    ] {
        let e = Env::new();
        let mut cap = limits();
        cap.call_timeout_ms = 30;
        let jobs = e.open(cap.clone(), Faults::none());
        e.activate(&jobs);
        let key = e.submit(&jobs, vec![e.input.clone()]);
        let fake = Fake::new(mode);
        let calls = fake.calls.clone();
        let worker = jobs.spawn(fake).unwrap();
        let status = wait(&jobs, &key, |s| s.state == JobState::OutcomeUnknown);
        assert!(status.uncertain);
        assert_eq!(status.reserved_tokens, 100);
        assert_eq!(status.reserved_cost_microusd, 10);
        assert_eq!(e.submit(&jobs, vec![e.input.clone()]), key);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        end(&jobs, worker);
        drop(jobs);
        let jobs = e.open(cap, Faults::none());
        e.activate(&jobs);
        assert_eq!(jobs.status(&key).unwrap().state, JobState::OutcomeUnknown);
        assert_eq!(
            e.context(1, |ctx| jobs.resume(&key, ctx)).err(),
            Some(Fault::Conflict)
        );
    }
}

#[test]
fn reservations_bound_cost_tokens_capacity_and_concurrency() {
    let e = Env::new();
    let mut cap = limits();
    cap.total_tokens = 100;
    cap.total_cost_microusd = 10;
    let jobs = e.open(cap, Faults::none());
    e.activate(&jobs);
    let key = e.submit(&jobs, e.batch());
    let fake = Fake::new(Mode::Complete);
    let calls = fake.calls.clone();
    let worker = jobs.spawn(fake).unwrap();
    assert_eq!(
        jobs.spawn(Fake::new(Mode::Complete)).err(),
        Some(Fault::Unavailable)
    );
    let status = wait(&jobs, &key, |s| s.state == JobState::BudgetBlocked);
    assert_eq!(status.cursor, 1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    jobs.cancel(&key).unwrap();
    assert_eq!(jobs.status(&key).unwrap().state, JobState::Cancelled);
    end(&jobs, worker);
    let e = Env::new();
    let mut cap = limits();
    cap.max_jobs = 1;
    let jobs = e.open(cap, Faults::none());
    e.activate(&jobs);
    e.submit(&jobs, vec![e.input.clone()]);
    let second = e.batch().remove(1);
    assert_eq!(
        e.context(1, |ctx| jobs.submit(vec![second], ctx)).err(),
        Some(Fault::Capacity)
    );
}

#[test]
fn output_binding_and_limits_fail_closed_without_an_application_receipt() {
    for mode in [
        Mode::WrongInput,
        Mode::OverBudget,
        Mode::Refused,
        Mode::MalformedJson,
    ] {
        let e = Env::new();
        let jobs = e.open(limits(), Faults::none());
        e.activate(&jobs);
        let key = e.submit(&jobs, vec![e.input.clone()]);
        let worker = jobs.spawn(Fake::new(mode)).unwrap();
        wait(&jobs, &key, |s| s.state == JobState::Failed);
        assert!(jobs.results(&key).is_err());
        assert_eq!(jobs.status(&key).unwrap().cursor, 0);
        end(&jobs, worker);
    }
    let e = Env::new();
    let mut cap = limits();
    cap.max_result_bytes = 1;
    let jobs = e.open(cap, Faults::none());
    e.activate(&jobs);
    let key = e.submit(&jobs, vec![e.input.clone()]);
    let worker = jobs.spawn(Fake::new(Mode::Complete)).unwrap();
    wait(&jobs, &key, |s| s.state == JobState::Failed);
    end(&jobs, worker);
}

#[test]
fn journal_reserves_result_capacity_before_dispatch_instead_of_stranding_a_result() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    let batch = |offset: u8| {
        (0..8u8)
            .map(|index| {
                let mut input = e.input.clone();
                let n = index + offset;
                input.operation_id = OperationId::from_random([n; 16]);
                input.result.memory_id = MemoryId::from_random([n; 16]);
                input.content.push_str(&format!(" Synthetic unit {n}."));
                input
            })
            .collect::<Vec<_>>()
    };
    let key = e.submit(&jobs, batch(1));
    assert_eq!(
        e.context(8, |ctx| jobs.submit(batch(10), ctx)).err(),
        Some(Fault::Capacity)
    );
    assert_eq!(jobs.status(&key).unwrap().state, JobState::Queued);
    jobs.cancel(&key).unwrap();
    e.submit(&jobs, batch(10));
}

#[test]
fn persistence_failure_stops_sends_and_lost_result_remains_unknown_on_reopen() {
    for nth in [1, 2] {
        let e = Env::new();
        let faults = Faults::armed();
        let jobs = e.open(limits(), faults.clone());
        e.activate(&jobs);
        let key = e.submit(&jobs, vec![e.input.clone()]);
        faults.arm(FaultPoint::BeforeRename, nth, FaultAction::Fail(5));
        let fake = Fake::new(Mode::Complete);
        let calls = fake.calls.clone();
        let worker = jobs.spawn(fake).unwrap();
        wait(&jobs, &key, |s| s.storage_unknown);
        worker.join().unwrap();
        assert_eq!(calls.load(Ordering::SeqCst), if nth == 1 { 0 } else { 1 });
        drop(jobs);
        let jobs = e.open(limits(), Faults::none());
        e.activate(&jobs);
        assert_eq!(
            jobs.status(&key).unwrap().state,
            if nth == 1 {
                JobState::Queued
            } else {
                JobState::OutcomeUnknown
            }
        );
    }
}

#[test]
fn lifetime_os_lock_existing_vault_and_unknown_journal_refuse_open() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    assert_eq!(
        SyntheticJobs::open(&e.root, limits(), e.clock.clone(), Faults::none()).err(),
        Some(Fault::Busy)
    );
    drop(jobs);
    let mut raw = fixture("queued");
    raw["schema_version"] = json!(3);
    std::fs::write(
        e.root.path().join(JOURNAL_PATH),
        canonical_bytes(&raw).unwrap(),
    )
    .unwrap();
    assert_eq!(
        SyntheticJobs::open(&e.root, limits(), e.clock.clone(), Faults::none()).err(),
        Some(Fault::UnsupportedVersion)
    );
    let e = Env::new();
    std::fs::create_dir(e.root.path().join("vault")).unwrap();
    assert_eq!(
        SyntheticJobs::open(&e.root, limits(), e.clock.clone(), Faults::none()).err(),
        Some(Fault::Unavailable)
    );
}

#[test]
fn process_child() {
    let Some(path) = std::env::var_os("ENOU_CORE2_TEST_ROOT") else {
        return;
    };
    let e = Env::at(PathBuf::from(path));
    let jobs = e.open(limits(), Faults::none());
    e.activate(&jobs);
    let mode = std::env::var("ENOU_CORE2_TEST_MODE").unwrap();
    let key = e.submit(
        &jobs,
        if mode == "queued" {
            e.batch()
        } else {
            vec![e.input.clone()]
        },
    );
    let mut fake = Fake::new(if mode == "queued" {
        Mode::Complete
    } else {
        Mode::Pending
    });
    fake.call_file = Some(e.root.path().join("sync-state/calls.txt"));
    if mode == "queued" {
        fake.pause = Some(jobs.clone());
    }
    let _worker = jobs.spawn(fake).unwrap();
    wait(&jobs, &key, |s| {
        if mode == "queued" {
            s.cursor == 1 && s.state == JobState::Queued
        } else {
            s.state == JobState::Running
        }
    });
    // Parent kills this exact owned child after observing the durable boundary.
    loop {
        std::thread::sleep(Duration::from_millis(20));
    }
}

#[test]
fn in_flight_epoch_change_or_expiry_prevents_late_adoption_and_resume() {
    for expire in [false, true] {
        let mut e = Env::new();
        let jobs = e.open(limits(), Faults::none());
        e.activate(&jobs);
        let key = e.submit(&jobs, vec![e.input.clone()]);
        let fake = Fake::new(Mode::Pending);
        let calls = fake.calls.clone();
        let worker = jobs.spawn(fake).unwrap();
        wait(&jobs, &key, |s| s.state == JobState::Running);
        if expire {
            e.clock.set(e.policy.valid_until.unix_ms());
        } else {
            e.input.snapshot.primary_epoch =
                enouia_memory_contract::json::Revision::new(2).unwrap();
            e.activate(&jobs);
        }
        let status = wait(&jobs, &key, |s| s.state == JobState::Invalidated);
        assert!(status.uncertain);
        assert_eq!(status.cursor, 0);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(jobs.results(&key).is_err());
        assert!(e.context(1, |ctx| jobs.resume(&key, ctx)).is_err());
        end(&jobs, worker);
    }
}

#[test]
fn admission_requires_live_core1_context_and_reopen_starts_locked() {
    let e = Env::new();
    let jobs = e.open(limits(), Faults::none());
    assert_eq!(
        e.context(1, |ctx| jobs.submit(vec![e.input.clone()], ctx))
            .err(),
        Some(Fault::Unavailable)
    );
    e.activate(&jobs);
    let mut forged = e.input.clone();
    forged.actor.actor_type = enouia_memory_contract::common::ActorType::Owner;
    assert_eq!(
        e.context(1, |ctx| jobs.submit(vec![forged], ctx)).err(),
        Some(Fault::Invalid)
    );
    let mut stale = e.input.clone();
    stale.context_source_refs.clear();
    assert_eq!(
        e.context(1, |ctx| jobs.submit(vec![stale], ctx)).err(),
        Some(Fault::Invalid)
    );
    let key = e.submit(&jobs, vec![e.input.clone()]);
    drop(jobs);
    let jobs = e.open(limits(), Faults::none());
    assert!(jobs.results(&key).is_err());
    let fake = Fake::new(Mode::Complete);
    let calls = fake.calls.clone();
    let worker = jobs.spawn(fake).unwrap();
    assert_eq!(jobs.status(&key).unwrap().state, JobState::Queued);
    assert_eq!(calls.load(Ordering::SeqCst), 0);
    e.activate(&jobs);
    wait(&jobs, &key, |s| s.state == JobState::Generated);
    end(&jobs, worker);
}

#[test]
fn actual_process_crash_recovers_cursor_and_never_resends_unknown_dispatch() {
    for mode in ["queued", "running"] {
        let e = Env::new();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "process_child", "--nocapture"])
            .env("ENOU_CORE2_TEST_ROOT", e.root.path())
            .env("ENOU_CORE2_TEST_MODE", mode)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        let until = Instant::now() + Duration::from_secs(20);
        loop {
            let ready = std::fs::read(e.root.path().join(JOURNAL_PATH))
                .ok()
                .and_then(|b| Journal::parse(&b).ok())
                .is_some_and(|j| {
                    j.jobs.first().is_some_and(|job| {
                        if mode == "queued" {
                            job.cursor == 1 && job.state == JobState::Queued
                        } else {
                            job.state == JobState::Running
                        }
                    })
                });
            let sent = std::fs::read_to_string(e.root.path().join("sync-state/calls.txt"))
                .is_ok_and(|text| text.lines().count() == 1);
            if ready && sent {
                break;
            }
            if Instant::now() >= until {
                let _ = child.kill();
                let _ = child.wait();
                panic!("owned child failed to reach durable boundary");
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        child.kill().unwrap();
        child.wait().unwrap();
        let calls = std::fs::read_to_string(e.root.path().join("sync-state/calls.txt"))
            .unwrap()
            .lines()
            .count();
        assert_eq!(calls, 1);
        let jobs = e.open(limits(), Faults::none());
        let inputs = if mode == "queued" {
            e.batch()
        } else {
            vec![e.input.clone()]
        };
        let key = work_key(&inputs);
        assert!(jobs.results(&key).is_err());
        e.activate(&jobs);
        let fake = Fake::new(Mode::Complete);
        let calls = fake.calls.clone();
        let worker = jobs.spawn(fake).unwrap();
        if mode == "queued" {
            wait(&jobs, &key, |s| s.state == JobState::Generated);
            assert_eq!(calls.load(Ordering::SeqCst), 1);
            assert_eq!(jobs.results(&key).unwrap().len(), 2);
        } else {
            assert_eq!(jobs.status(&key).unwrap().state, JobState::OutcomeUnknown);
            assert_eq!(calls.load(Ordering::SeqCst), 0);
        }
        end(&jobs, worker);
    }
}

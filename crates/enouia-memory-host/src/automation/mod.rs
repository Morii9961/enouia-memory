//! Core-2 opt-in synthetic job coordinator. No default Host wiring, Provider
//! adapter or default writer. The opt-in `application` module connects Core-3's
//! separate V2 synthetic transactions; the coordinator alone never applies them.
pub mod application;
pub mod continuation;
mod model;
mod worker;
pub use model::*;
pub use worker::{Generated, Generation, Generator, PendingGeneration, StopToken};

use enouia_memory_contract::{
    common::{ActorRef, ActorType},
    foundation::Clock,
    hash::Sha256Hex,
    json::canonical_bytes,
    next::*,
    time::Timestamp,
};
use enouia_memory_vault::{VerifiedRoot, fault::Faults, fs::ManagedRoot};
use std::{
    fs::File,
    sync::{
        Arc, Condvar, Mutex, MutexGuard,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
};

pub const JOURNAL_PATH: &str = "sync-state/core2-jobs.json";
const LOCK_PATH: &str = "sync-state/core2-jobs.lock";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Fault {
    Invalid,
    UnsupportedVersion,
    StorageUnknown,
    Busy,
    Capacity,
    Conflict,
    Unavailable,
    Expired,
    NotFound,
}
pub type Result<T> = std::result::Result<T, Fault>;

/// Construct only from a native trusted observation. No authentication or
/// permission grant is performed here. Refresh/revoke with actual lifecycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Permit {
    snapshot: SnapshotBinding,
    policy: PolicyRef,
    actor: ActorRef,
    from: Timestamp,
    until: Timestamp,
}
impl Permit {
    pub fn from_context(context: &EvaluationContext<'_>) -> Result<Self> {
        let p = context.policy;
        if !p.validate().is_empty()
            || !p.active
            || context.system_actor.actor_type != ActorType::System
            || *context.system_actor != p.system_actor
            || context.snapshot.vault_id != p.vault_id
            || context.now < &p.valid_from
            || context.now >= &p.valid_until
        {
            return Err(Fault::Unavailable);
        }
        Ok(Self {
            snapshot: context.snapshot.clone(),
            policy: p.reference(),
            actor: p.system_actor.clone(),
            from: p.valid_from.clone(),
            until: p.valid_until.clone(),
        })
    }
    fn matches(&self, input: &AdmissionInput) -> bool {
        self.snapshot == input.snapshot && self.policy == input.policy && self.actor == input.actor
    }
}

#[derive(Clone, Debug)]
pub struct Status {
    pub key: Sha256Hex,
    pub state: JobState,
    pub cursor: u64,
    pub total: u64,
    pub uncertain: bool,
    pub reserved_tokens: u64,
    pub reserved_cost_microusd: u64,
    pub storage_unknown: bool,
}
struct State {
    journal: Journal,
    permit: Option<Permit>,
    foreground: bool,
    closed: bool,
    faulted: bool,
    started: bool,
    stop: Option<Arc<AtomicBool>>,
}
struct Inner {
    root: ManagedRoot,
    _lease: File,
    state: Mutex<State>,
    wake: Condvar,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl Inner {
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.state.lock().map_err(|_| Fault::Unavailable)
    }
    fn persist(&self, state: &mut State, journal: Journal) -> Result<()> {
        if state.faulted {
            return Err(Fault::StorageUnknown);
        }
        journal.validate()?;
        let bytes = canonical_bytes(&journal).map_err(|_| Fault::Invalid)?;
        if bytes.len() > MAX_JOURNAL_BYTES {
            return Err(Fault::Capacity);
        }
        if self.root.write_atomic(JOURNAL_PATH, &bytes).is_err() {
            state.faulted = true;
            if let Some(stop) = &state.stop {
                stop.store(true, Ordering::SeqCst);
            }
            self.wake.notify_all();
            return Err(Fault::StorageUnknown);
        }
        state.journal = journal;
        Ok(())
    }
    fn time(&self, state: &mut State) -> Result<i64> {
        let now = self.clock.now_unix_ms();
        if state.closed || state.faulted {
            return Err(Fault::Unavailable);
        }
        if now < state.journal.last_ms
            || state
                .permit
                .as_ref()
                .is_some_and(|p| now >= p.until.unix_ms())
        {
            self.invalidate(state)?;
            return Err(Fault::Expired);
        }
        Ok(now)
    }
    fn invalidate(&self, state: &mut State) -> Result<()> {
        state.permit = None;
        if let Some(stop) = &state.stop {
            stop.store(true, Ordering::SeqCst);
        }
        let mut journal = state.journal.clone();
        for job in &mut journal.jobs {
            if matches!(
                job.state,
                JobState::Queued
                    | JobState::Running
                    | JobState::Generated
                    | JobState::BudgetBlocked
            ) {
                job.state = JobState::Invalidated;
            }
        }
        self.persist(state, journal)?;
        self.wake.notify_all();
        Ok(())
    }
}

#[derive(Clone)]
pub struct SyntheticJobs {
    inner: Arc<Inner>,
}
impl SyntheticJobs {
    /// Explicit isolated synthetic root only. Refuse all existing Vault data;
    /// do not place a job journal next to an embedded/Host production writer.
    pub fn open(
        root: &VerifiedRoot,
        limits: Limits,
        clock: Arc<dyn Clock + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        limits.validate()?;
        let managed = ManagedRoot::new(root.path(), faults);
        if managed.exists("vault").map_err(|_| Fault::Unavailable)? {
            return Err(Fault::Unavailable);
        }
        let lease = managed
            .open_lock_file(LOCK_PATH)
            .map_err(|_| Fault::Unavailable)?;
        lease.try_lock().map_err(|_| Fault::Busy)?;
        let now = clock.now_unix_ms();
        let journal = match managed
            .read_bounded(JOURNAL_PATH, MAX_JOURNAL_BYTES)
            .map_err(|_| Fault::Unavailable)?
        {
            Some(bytes) => {
                let mut j = Journal::parse(&bytes)?;
                if j.limits != limits {
                    return Err(Fault::Conflict);
                }
                for job in &mut j.jobs {
                    if job.state == JobState::Running {
                        job.state = JobState::OutcomeUnknown;
                    }
                }
                j
            }
            None => Journal {
                schema_version: NextVersion,
                kind: JournalKind::MemoryJobJournal,
                limits,
                reserved_tokens: 0,
                reserved_cost_microusd: 0,
                last_ms: now,
                jobs: Vec::new(),
            },
        };
        journal.validate()?;
        managed
            .write_atomic(
                JOURNAL_PATH,
                &canonical_bytes(&journal).map_err(|_| Fault::Invalid)?,
            )
            .map_err(|_| Fault::StorageUnknown)?;
        Ok(Self {
            inner: Arc::new(Inner {
                root: managed,
                _lease: lease,
                state: Mutex::new(State {
                    journal,
                    permit: None,
                    foreground: false,
                    closed: false,
                    faulted: false,
                    started: false,
                    stop: None,
                }),
                wake: Condvar::new(),
                clock,
            }),
        })
    }
    pub fn activate(&self, permit: Permit) -> Result<()> {
        let mut s = self.inner.lock()?;
        let now = self.inner.time(&mut s)?;
        if now < permit.from.unix_ms() || now >= permit.until.unix_ms() {
            return Err(Fault::Expired);
        }
        if s.permit.as_ref().is_some_and(|old| old != &permit) {
            self.inner.invalidate(&mut s)?;
        }
        let mut journal = s.journal.clone();
        for job in &mut journal.jobs {
            if !permit.matches(&job.inputs[0])
                && matches!(
                    job.state,
                    JobState::Queued
                        | JobState::Running
                        | JobState::Generated
                        | JobState::BudgetBlocked
                )
            {
                job.state = JobState::Invalidated;
            }
        }
        journal.last_ms = now;
        self.inner.persist(&mut s, journal)?;
        s.permit = Some(permit);
        self.inner.wake.notify_all();
        Ok(())
    }
    pub fn submit(
        &self,
        inputs: Vec<AdmissionInput>,
        contexts: &[EvaluationContext<'_>],
    ) -> Result<Sha256Hex> {
        model::validate_inputs(&inputs)?;
        if inputs.len() != contexts.len() {
            return Err(Fault::Invalid);
        }
        for (input, context) in inputs.iter().zip(contexts) {
            evaluate(input, context).map_err(|_| Fault::Invalid)?;
        }
        let key = work_key(&inputs);
        let mut s = self.inner.lock()?;
        let now = self.inner.time(&mut s)?;
        if !s.permit.as_ref().is_some_and(|p| p.matches(&inputs[0])) {
            return Err(Fault::Unavailable);
        }
        if s.journal.jobs.iter().any(|j| j.key == key) {
            return Ok(key);
        }
        if s.journal.jobs.len() as u64 >= s.journal.limits.max_jobs {
            return Err(Fault::Capacity);
        }
        if inputs.iter().any(|input| {
            s.journal.jobs.iter().flat_map(|j| &j.inputs).any(|old| {
                old.operation_id == input.operation_id
                    || work_key(std::slice::from_ref(old)) == work_key(std::slice::from_ref(input))
            })
        }) {
            return Err(Fault::Conflict);
        }
        let mut journal = s.journal.clone();
        journal.last_ms = now;
        journal.jobs.push(Job {
            key: key.clone(),
            inputs,
            state: JobState::Queued,
            cursor: 0,
            attempts: 0,
            uncertain: false,
            results: Vec::new(),
        });
        self.inner.persist(&mut s, journal)?;
        self.inner.wake.notify_all();
        Ok(key)
    }
    pub fn status(&self, key: &Sha256Hex) -> Result<Status> {
        let s = self.inner.state.try_lock().map_err(|_| Fault::Busy)?;
        let job = s
            .journal
            .jobs
            .iter()
            .find(|j| &j.key == key)
            .ok_or(Fault::NotFound)?;
        Ok(Status {
            key: key.clone(),
            state: job.state,
            cursor: job.cursor,
            total: job.inputs.len() as u64,
            uncertain: job.uncertain,
            reserved_tokens: s.journal.reserved_tokens,
            reserved_cost_microusd: s.journal.reserved_cost_microusd,
            storage_unknown: s.faulted,
        })
    }
    /// Explicit fresh native reevaluation may resume a lifecycle-paused job
    /// only when there is no unaccounted invocation. Never retry unknown work.
    pub fn resume(&self, key: &Sha256Hex, contexts: &[EvaluationContext<'_>]) -> Result<()> {
        let inputs = {
            let s = self.inner.lock()?;
            s.journal
                .jobs
                .iter()
                .find(|j| &j.key == key)
                .ok_or(Fault::NotFound)?
                .inputs
                .clone()
        };
        if inputs.len() != contexts.len() {
            return Err(Fault::Invalid);
        }
        for (input, context) in inputs.iter().zip(contexts) {
            evaluate(input, context).map_err(|_| Fault::Invalid)?;
        }
        let mut s = self.inner.lock()?;
        let now = self.inner.time(&mut s)?;
        if !s.permit.as_ref().is_some_and(|p| p.matches(&inputs[0])) {
            return Err(Fault::Unavailable);
        }
        let mut journal = s.journal.clone();
        let job = journal
            .jobs
            .iter_mut()
            .find(|j| &j.key == key)
            .ok_or(Fault::NotFound)?;
        if job.state != JobState::Invalidated || job.uncertain || job.attempts != job.cursor {
            return Err(Fault::Conflict);
        }
        job.state = if job.cursor == job.inputs.len() as u64 {
            JobState::Generated
        } else {
            JobState::Queued
        };
        journal.last_ms = now;
        self.inner.persist(&mut s, journal)?;
        self.inner.wake.notify_all();
        Ok(())
    }
    pub fn results(&self, key: &Sha256Hex) -> Result<Vec<GenerationResult>> {
        self.generated_batch(key).map(|(_, results)| results)
    }
    pub fn generated_batch(
        &self,
        key: &Sha256Hex,
    ) -> Result<(Vec<AdmissionInput>, Vec<GenerationResult>)> {
        let mut s = self.inner.lock()?;
        self.inner.time(&mut s)?;
        let job = s
            .journal
            .jobs
            .iter()
            .find(|j| &j.key == key)
            .ok_or(Fault::NotFound)?;
        if job.state != JobState::Generated
            || !s.permit.as_ref().is_some_and(|p| p.matches(&job.inputs[0]))
        {
            return Err(Fault::Unavailable);
        }
        Ok((job.inputs.clone(), job.results.clone()))
    }
    /// Content-free exact identities for recovery of an already committed
    /// batch. This grants no dispatch/application authority after invalidation.
    pub fn generated_bindings(&self, key: &Sha256Hex) -> Result<Vec<GenerationBinding>> {
        let s = self.inner.lock()?;
        let job = s
            .journal
            .jobs
            .iter()
            .find(|j| &j.key == key)
            .ok_or(Fault::NotFound)?;
        if !matches!(job.state, JobState::Generated | JobState::Invalidated)
            || job.results.len() != job.inputs.len()
        {
            return Err(Fault::Unavailable);
        }
        Ok(job
            .inputs
            .iter()
            .zip(&job.results)
            .map(|(input, result)| GenerationBinding {
                operation_id: input.operation_id.clone(),
                input_hash: result.input_hash.clone(),
                response_hash: result.response_hash.clone(),
            })
            .collect())
    }
    pub fn cancel(&self, key: &Sha256Hex) -> Result<()> {
        let mut s = self.inner.lock()?;
        let mut journal = s.journal.clone();
        let job = journal
            .jobs
            .iter_mut()
            .find(|j| &j.key == key)
            .ok_or(Fault::NotFound)?;
        if matches!(
            job.state,
            JobState::Queued
                | JobState::Running
                | JobState::Generated
                | JobState::Invalidated
                | JobState::BudgetBlocked
        ) {
            if job.state == JobState::Running
                && let Some(stop) = &s.stop
            {
                stop.store(true, Ordering::SeqCst);
            }
            job.state = JobState::Cancelled;
            self.inner.persist(&mut s, journal)?;
        }
        self.inner.wake.notify_all();
        Ok(())
    }
    pub fn foreground(&self, active: bool) -> Result<()> {
        self.inner.lock()?.foreground = active;
        self.inner.wake.notify_all();
        Ok(())
    }
    pub fn lock(&self) -> Result<()> {
        let mut s = self.inner.lock()?;
        self.inner.invalidate(&mut s)
    }
    /// Returns without joining any generator. Status/cancel remain available.
    pub fn shutdown(&self) -> Result<()> {
        let mut s = self.inner.lock()?;
        self.inner.invalidate(&mut s)?;
        s.closed = true;
        self.inner.wake.notify_all();
        Ok(())
    }
    pub fn spawn(&self, generator: impl Generator) -> Result<JoinHandle<()>> {
        self.spawn_with_completion(generator, |_| {})
    }
    /// Native completion hook outside job control exclusion. Default spawn
    /// retains its generation-only behavior.
    pub fn spawn_with_completion(
        &self,
        generator: impl Generator,
        completion: impl FnMut(Sha256Hex) + Send + 'static,
    ) -> Result<JoinHandle<()>> {
        let mut s = self.inner.lock()?;
        if s.started || s.closed || s.faulted {
            return Err(Fault::Unavailable);
        }
        s.started = true;
        let inner = self.inner.clone();
        std::thread::Builder::new()
            .name("memory-core2".into())
            .spawn(move || worker::run(inner, generator, completion))
            .map_err(|_| {
                s.started = false;
                Fault::Unavailable
            })
    }
}

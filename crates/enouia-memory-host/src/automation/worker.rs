use super::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct StopToken(Arc<AtomicBool>);
impl StopToken {
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::SeqCst)
    }
}
pub struct Generated {
    pub input_hash: Sha256Hex,
    pub response_json: String,
    pub tokens_used: u64,
    pub cost_microusd: u64,
}
pub enum Generation {
    Complete(Box<Generated>),
    Refused,
    Unknown,
}

/// A bounded nonblocking poll. Long model/embedding computation belongs outside
/// every Vault writer/control lock. This slice only exercises fake adapters.
pub trait PendingGeneration: Send {
    fn poll(&mut self, stop: &StopToken) -> Option<Generation>;
}
pub trait Generator: Send + 'static {
    /// Bounded, nonblocking handoff ONLY: cancellation and this dispatch share
    /// one gate, so a later cancel cannot be followed by an additional dispatch.
    /// No network wait, embedding or unbounded work inside this method.
    fn start(
        &mut self,
        input: &AdmissionInput,
        limits: &Limits,
        stop: &StopToken,
    ) -> std::result::Result<Box<dyn PendingGeneration>, Generation>;
}

struct Dispatch {
    index: usize,
    input: AdmissionInput,
    stop: StopToken,
    started: Instant,
    timeout_ms: u64,
    pending: std::result::Result<Box<dyn PendingGeneration>, Generation>,
}

fn dispatch(inner: &Inner, generator: &mut impl Generator) -> Result<Option<Dispatch>> {
    let mut s = inner.lock()?;
    if s.closed || s.faulted {
        return Err(Fault::Unavailable);
    }
    let now = match inner.time(&mut s) {
        Ok(now) => now,
        Err(Fault::Expired) => return Ok(None),
        Err(e) => return Err(e),
    };
    if s.foreground || s.permit.is_none() {
        return Ok(None);
    }
    let Some(index) = s
        .journal
        .jobs
        .iter()
        .position(|job| job.state == JobState::Queued)
    else {
        return Ok(None);
    };
    let mut journal = s.journal.clone();
    let limits = journal.limits.clone();
    let job = &mut journal.jobs[index];
    let input = job.inputs[job.cursor as usize].clone();
    if !s.permit.as_ref().is_some_and(|p| p.matches(&input)) {
        job.state = JobState::Invalidated;
        inner.persist(&mut s, journal)?;
        return Ok(None);
    }
    if journal
        .reserved_tokens
        .checked_add(limits.call_tokens)
        .is_none_or(|v| v > limits.total_tokens)
        || journal
            .reserved_cost_microusd
            .checked_add(limits.call_cost_microusd)
            .is_none_or(|v| v > limits.total_cost_microusd)
    {
        job.state = JobState::BudgetBlocked;
        inner.persist(&mut s, journal)?;
        return Ok(None);
    }
    job.state = JobState::Running;
    job.uncertain = true;
    job.attempts += 1;
    journal.reserved_tokens += limits.call_tokens;
    journal.reserved_cost_microusd += limits.call_cost_microusd;
    journal.last_ms = now;
    // Durable dispatch intent and budget BEFORE any generator handoff.
    inner.persist(&mut s, journal)?;
    let stop = StopToken(Arc::new(AtomicBool::new(false)));
    s.stop = Some(stop.0.clone());
    let started = Instant::now();
    let pending = catch_unwind(AssertUnwindSafe(|| generator.start(&input, &limits, &stop)))
        .unwrap_or(Err(Generation::Unknown));
    Ok(Some(Dispatch {
        index,
        input,
        stop,
        started,
        timeout_ms: limits.call_timeout_ms,
        pending,
    }))
}

fn finish(inner: &Inner, work: &Dispatch, generation: Generation) -> Result<()> {
    let mut s = inner.lock()?;
    if s.faulted || s.closed {
        return Ok(());
    }
    let now = match inner.time(&mut s) {
        Ok(now) => now,
        Err(_) => return Ok(()),
    };
    if s.journal.jobs[work.index].state != JobState::Running || work.stop.is_cancelled() {
        s.stop = None;
        return Ok(());
    }
    let mut journal = s.journal.clone();
    let job = &mut journal.jobs[work.index];
    match generation {
        Generation::Complete(result)
            if work.started.elapsed().as_millis() < work.timeout_ms as u128 =>
        {
            let saved = GenerationResult {
                input_hash: result.input_hash,
                response_hash: enouia_memory_contract::hash::sha256(
                    result.response_json.as_bytes(),
                ),
                response_json: result.response_json,
            };
            if saved.validate(&work.input, &journal.limits).is_err()
                || result.tokens_used > journal.limits.call_tokens
                || result.cost_microusd > journal.limits.call_cost_microusd
            {
                job.state = JobState::Failed;
                job.uncertain = false;
            } else {
                job.results.push(saved);
                job.cursor += 1;
                job.uncertain = false;
                job.state = if job.cursor == job.inputs.len() as u64 {
                    JobState::Generated
                } else {
                    JobState::Queued
                };
            }
        }
        Generation::Refused => {
            job.state = JobState::Failed;
            job.uncertain = false;
        }
        _ => {
            job.state = JobState::OutcomeUnknown;
        }
    }
    journal.last_ms = now;
    inner.persist(&mut s, journal)?;
    s.stop = None;
    inner.wake.notify_all();
    Ok(())
}

pub(super) fn run(inner: Arc<Inner>, mut generator: impl Generator) {
    loop {
        let work = match dispatch(&inner, &mut generator) {
            Ok(Some(work)) => work,
            Ok(None) => {
                let Ok(s) = inner.lock() else {
                    return;
                };
                if s.closed || s.faulted {
                    return;
                }
                let _ = inner.wake.wait_timeout(s, Duration::from_millis(10));
                continue;
            }
            Err(_) => return,
        };
        let mut work = work;
        let first = std::mem::replace(&mut work.pending, Err(Generation::Unknown));
        let generation = match first {
            Err(result) => result,
            Ok(mut pending) => loop {
                if work.stop.is_cancelled()
                    || work.started.elapsed().as_millis() >= work.timeout_ms as u128
                {
                    break Generation::Unknown;
                }
                match catch_unwind(AssertUnwindSafe(|| pending.poll(&work.stop))) {
                    Ok(Some(result)) => break result,
                    Err(_) => break Generation::Unknown,
                    Ok(None) => {}
                }
                let Ok(mut s) = inner.lock() else {
                    return;
                };
                if inner.time(&mut s).is_err() && s.faulted {
                    return;
                }
                if s.closed || s.faulted {
                    return;
                }
                let _ = inner.wake.wait_timeout(s, Duration::from_millis(5));
            },
        };
        if finish(&inner, &work, generation).is_err() {
            return;
        }
    }
}

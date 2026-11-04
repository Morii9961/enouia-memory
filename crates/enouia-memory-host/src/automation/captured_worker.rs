//! Native captured formation lifecycle on one already leased Owner Vault.
use super::{
    application::{ApplicationFault, ApplicationResult},
    runtime_capture::RuntimeCaptureHost,
};
use enouia_memory_contract::next::{
    AgentCorrectionRecord, AgentSupersedeRecord, CapturedSource, OWNER_RUNTIME_CAPTURE_CAPABILITY,
};
use enouia_memory_vault::automation::{
    AutomationFault,
    segmented::{
        CaptureAnalysis, CaptureAnalyzer, CaptureStopToken, CapturedFormation, CorrectionAnalyzer,
        CorrectionTarget, PendingCaptureAnalysis, SupersedeAnalyzer,
    },
};
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::{
        Arc, Condvar, Mutex, Weak,
        atomic::{AtomicUsize, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WorkerExit {
    Stopped,
    OutcomeUnknown,
    Fault,
    Panicked,
}
struct State {
    started: bool,
    foreground: bool,
    stopped: bool,
    exit: Option<WorkerExit>,
}
pub(super) struct Control {
    state: Mutex<State>,
    foreground_calls: AtomicUsize,
    wake: Condvar,
    stop: CaptureStopToken,
}
impl Default for Control {
    fn default() -> Self {
        Self {
            state: Mutex::new(State {
                started: false,
                foreground: false,
                stopped: false,
                exit: None,
            }),
            foreground_calls: AtomicUsize::new(0),
            wake: Condvar::new(),
            stop: CaptureStopToken::default(),
        }
    }
}
pub(super) struct Foreground<'a>(&'a Control);
impl Drop for Foreground<'_> {
    fn drop(&mut self) {
        self.0.foreground_calls.fetch_sub(1, Ordering::SeqCst);
        self.0.wake.notify_all();
    }
}
impl Control {
    pub(super) fn foreground(&self) -> ApplicationResult<Foreground<'_>> {
        // Announce before waiting for the current short step, so the next step
        // cannot overtake this native foreground call.
        self.foreground_calls.fetch_add(1, Ordering::SeqCst);
        let guard = Foreground(self);
        let state = self.state.lock().map_err(|_| ApplicationFault::Locked)?;
        if state.stopped || self.stop.is_cancelled() {
            return Err(ApplicationFault::Locked);
        }
        Ok(guard)
    }
    pub(super) fn cancel(&self) {
        self.stop.cancel();
        self.wake.notify_all();
    }
    pub(super) fn stop(&self) {
        self.cancel();
        if let Ok(mut state) = self.state.lock() {
            state.stopped = true;
        }
        self.wake.notify_all();
    }
}
/// Owned native handle. Drop cancels and joins this dedicated thread, never a
/// Runtime pool task. Adapters must honor the bounded start/poll contract.
pub struct CapturedWorker {
    control: Arc<Control>,
    thread: Option<JoinHandle<WorkerExit>>,
}
impl CapturedWorker {
    pub fn stop(&self) {
        self.control.stop();
    }
    pub fn exit(&self) -> Option<WorkerExit> {
        self.control.state.lock().ok().and_then(|s| s.exit)
    }
    pub fn shutdown(mut self) -> WorkerExit {
        self.stop();
        self.thread.take().map_or(WorkerExit::Stopped, |t| {
            t.join().unwrap_or(WorkerExit::Panicked)
        })
    }
}
impl Drop for CapturedWorker {
    fn drop(&mut self) {
        self.stop();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
impl RuntimeCaptureHost {
    pub fn set_foreground(&self, active: bool) -> ApplicationResult<()> {
        let mut state = self
            .worker
            .state
            .lock()
            .map_err(|_| ApplicationFault::Locked)?;
        if state.stopped {
            return Err(ApplicationFault::Locked);
        }
        state.foreground = active;
        self.worker.wake.notify_all();
        Ok(())
    }
    pub fn start_worker(
        self: &Arc<Self>,
        analyzer: impl CaptureAnalyzer + Send + 'static,
        timeout: Duration,
    ) -> ApplicationResult<CapturedWorker> {
        self.start(
            analyzer,
            None::<NoCorrections>,
            None::<NoSupersedes>,
            timeout,
        )
    }
    /// Also consumes saved Agent correction reports (ADR-MEM-122) whenever no
    /// captured step is ready. Captured behavior is otherwise unchanged.
    pub fn start_worker_with_corrections(
        self: &Arc<Self>,
        analyzer: impl CaptureAnalyzer + Send + 'static,
        corrections: impl CorrectionAnalyzer + Send + 'static,
        timeout: Duration,
    ) -> ApplicationResult<CapturedWorker> {
        self.start(analyzer, Some(corrections), None::<NoSupersedes>, timeout)
    }
    /// Also consumes independently saved supersede reports through System updates.
    pub fn start_worker_with_reports(
        self: &Arc<Self>,
        analyzer: impl CaptureAnalyzer + Send + 'static,
        corrections: impl CorrectionAnalyzer + Send + 'static,
        supersedes: impl SupersedeAnalyzer + Send + 'static,
        timeout: Duration,
    ) -> ApplicationResult<CapturedWorker> {
        self.start(analyzer, Some(corrections), Some(supersedes), timeout)
    }
    fn start(
        self: &Arc<Self>,
        analyzer: impl CaptureAnalyzer + Send + 'static,
        corrections: Option<impl CorrectionAnalyzer + Send + 'static>,
        supersedes: Option<impl SupersedeAnalyzer + Send + 'static>,
        timeout: Duration,
    ) -> ApplicationResult<CapturedWorker> {
        if self.capability() != OWNER_RUNTIME_CAPTURE_CAPABILITY {
            return Err(ApplicationFault::Invalid);
        }
        self.formation_live()?;
        let mut state = self
            .worker
            .state
            .lock()
            .map_err(|_| ApplicationFault::Locked)?;
        if state.started || state.stopped || self.worker.stop.is_cancelled() {
            return Err(AutomationFault::Busy.into());
        }
        let formation = match CapturedFormation::open_with_timeout(
            self.vault.clone(),
            &self.vault.descriptor().system_actor,
            timeout,
        ) {
            Ok(value) => value,
            Err(AutomationFault::OutcomeUnknown) => {
                self.worker.cancel();
                state.stopped = true;
                self.poison_formation();
                return Err(AutomationFault::OutcomeUnknown.into());
            }
            Err(error) => return Err(error.into()),
        };
        state.started = true;
        let control = self.worker.clone();
        let weak = Arc::downgrade(self);
        let thread = std::thread::Builder::new()
            .name("memory-captured-formation".into())
            .spawn(move || {
                let outcome = catch_unwind(AssertUnwindSafe(|| {
                    run(weak, &control, formation, analyzer, corrections, supersedes)
                }));
                let exit = outcome.unwrap_or(WorkerExit::Panicked);
                control.cancel();
                if let Ok(mut state) = control.state.lock() {
                    state.stopped = true;
                    state.exit = Some(exit);
                }
                exit
            })
            .map_err(|_| {
                state.started = false;
                ApplicationFault::Vault(AutomationFault::Unavailable)
            })?;
        Ok(CapturedWorker {
            control: self.worker.clone(),
            thread: Some(thread),
        })
    }
}
/// Uninhabited: the plain worker never consumes correction reports.
enum NoCorrections {}
impl CorrectionAnalyzer for NoCorrections {
    fn start(
        &mut self,
        _: &AgentCorrectionRecord,
        _: &CorrectionTarget,
        _: &CapturedSource,
        _: &CaptureStopToken,
    ) -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
        match *self {}
    }
}
enum NoSupersedes {}
impl SupersedeAnalyzer for NoSupersedes {
    fn start(
        &mut self,
        _: &AgentSupersedeRecord,
        _: &CorrectionTarget,
        _: &CapturedSource,
        _: &CaptureStopToken,
    ) -> Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis> {
        match *self {}
    }
}
fn run(
    host: Weak<RuntimeCaptureHost>,
    control: &Control,
    mut formation: CapturedFormation,
    mut analyzer: impl CaptureAnalyzer,
    mut corrections: Option<impl CorrectionAnalyzer>,
    mut supersedes: Option<impl SupersedeAnalyzer>,
) -> WorkerExit {
    let mut backoff = Duration::from_millis(10);
    loop {
        // Declared before the guard, so every early exit drops the guard first.
        let Some(host) = host.upgrade() else {
            return WorkerExit::Stopped;
        };
        let Ok(mut state) = control.state.lock() else {
            return WorkerExit::Fault;
        };
        if state.stopped || control.stop.is_cancelled() {
            return WorkerExit::Stopped;
        }
        if host.formation_live().is_err() {
            control.cancel();
            return WorkerExit::Stopped;
        }
        let result = if state.foreground || control.foreground_calls.load(Ordering::SeqCst) != 0 {
            Ok(false)
        } else {
            // The native lifecycle gate covers only one bounded local action.
            // Neither analyzer start nor poll may wait on external work.
            match catch_unwind(AssertUnwindSafe(|| {
                let result = match (
                    formation.step_with_stop(&mut analyzer, &control.stop),
                    corrections.as_mut(),
                ) {
                    (Ok(false), Some(corrections)) => {
                        formation.step_corrections(corrections, &control.stop)
                    }
                    (result, _) => result,
                };
                match (result, supersedes.as_mut()) {
                    (Ok(false), Some(supersedes)) => {
                        formation.step_supersedes(supersedes, &control.stop)
                    }
                    (result, _) => result,
                }
            })) {
                Ok(result) => result,
                Err(_) => {
                    control.cancel();
                    return WorkerExit::Panicked;
                }
            }
        };
        // Drop the temporary Host reference outside the lifecycle gate. The
        // final reference can run Host::drop, which takes that same gate.
        match result {
            Err(AutomationFault::OutcomeUnknown) => {
                control.cancel();
                host.poison_formation();
                return WorkerExit::OutcomeUnknown;
            }
            Err(AutomationFault::Busy) | Ok(false) => {
                backoff = (backoff * 2).min(Duration::from_millis(250));
            }
            Ok(true) => {
                backoff = Duration::from_millis(10);
            }
            Err(_) => {
                control.cancel();
                return WorkerExit::Fault;
            }
        }
        let delay = if matches!(result, Ok(true)) {
            Duration::from_millis(1)
        } else {
            backoff
        };
        // Host::drop must not run while this gate is held.
        drop(state);
        drop(host);
        state = match control.state.lock() {
            Ok(s) => s,
            Err(_) => return WorkerExit::Fault,
        };
        if state.stopped || control.stop.is_cancelled() {
            return WorkerExit::Stopped;
        }
        if control.wake.wait_timeout(state, delay).is_err() {
            return WorkerExit::Fault;
        }
    }
}

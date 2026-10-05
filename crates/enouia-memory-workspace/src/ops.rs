//! Long operations: each runs on its own worker thread and is observed by
//! ID. Progress is a count (commits applied, batches committed); it is never
//! evidence of completion. Only `succeeded` is.

use enouia_memory_contract::error::MemoryErrorCode;
use enouia_memory_contract::ids::OperationId;
use enouia_memory_contract::ipc::OperationState;
use enouia_memory_contract::workspace::WorkspaceError;
use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;

pub struct Operation {
    pub id: OperationId,
    pub kind: &'static str,
    cancel: AtomicBool,
    done: AtomicU64,
    total: Mutex<Option<u64>>,
    outcome: Mutex<(OperationState, Option<WorkspaceError>, Option<Value>)>,
}

/// What a worker sees: cancellation and progress.
#[derive(Clone)]
pub struct Ticket(Arc<Operation>);

impl Ticket {
    pub fn cancelled(&self) -> bool {
        self.0.cancel.load(Ordering::SeqCst)
    }

    pub fn advance(&self) {
        self.0.done.fetch_add(1, Ordering::SeqCst);
    }

    pub fn set_total(&self, total: Option<u64>) {
        *self.0.total.lock().expect("total") = total;
    }
}

/// Importers check between batches; each check counts as progress.
impl enouia_memory_contract::foundation::Cancellation for Ticket {
    fn is_cancelled(&self) -> bool {
        self.advance();
        self.cancelled()
    }
}

#[derive(Default)]
pub struct Operations {
    map: Mutex<BTreeMap<String, Arc<Operation>>>,
    threads: Mutex<Vec<JoinHandle<()>>>,
}

impl Operations {
    /// Start `work` on a worker thread. `Ok` becomes `succeeded` with its
    /// result; an error becomes `failed`, or `cancelled` when the worker
    /// reports the cancellation error. A request alone is not an outcome.
    pub fn spawn(
        &self,
        id: OperationId,
        kind: &'static str,
        work: impl FnOnce(&Ticket) -> Result<Value, WorkspaceError> + Send + 'static,
    ) -> OperationId {
        let op = Arc::new(Operation {
            id: id.clone(),
            kind,
            cancel: AtomicBool::new(false),
            done: AtomicU64::new(0),
            total: Mutex::new(None),
            outcome: Mutex::new((OperationState::Queued, None, None)),
        });
        self.map
            .lock()
            .expect("operations")
            .insert(id.to_string(), op.clone());
        let ticket = Ticket(op.clone());
        let handle = std::thread::spawn(move || {
            op.outcome.lock().expect("outcome").0 = OperationState::Running;
            // A panic is a defect, but it must end as `failed`, never stay
            // `running` forever (ADR-MEM-46).
            let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| work(&ticket)));
            let mut outcome = op.outcome.lock().unwrap_or_else(|e| e.into_inner());
            *outcome = match result {
                // Even after a cancel request: a panic is a failure.
                Err(_) => {
                    let mut error = WorkspaceError::new(
                        MemoryErrorCode::StorageFailed,
                        &["operation.panicked"],
                    );
                    error.retryable = false;
                    (OperationState::Failed, Some(error), None)
                }
                Ok(Ok(value)) if value.get("cancelled") == Some(&json!(true)) => {
                    (OperationState::Cancelled, None, Some(value))
                }
                Ok(Ok(value)) => (OperationState::Succeeded, None, Some(value)),
                Ok(Err(error)) if error.code == MemoryErrorCode::Cancelled => {
                    (OperationState::Cancelled, Some(error), None)
                }
                Ok(Err(error)) => (OperationState::Failed, Some(error), None),
            };
        });
        let mut threads = self.threads.lock().expect("threads");
        threads.retain(|t| !t.is_finished());
        threads.push(handle);
        id
    }

    pub fn status(&self, id: &OperationId) -> Option<Value> {
        let op = self
            .map
            .lock()
            .expect("operations")
            .get(id.as_str())?
            .clone();
        Some(describe(&op))
    }

    pub fn cancel(&self, id: &OperationId) -> Option<Value> {
        let op = self
            .map
            .lock()
            .expect("operations")
            .get(id.as_str())?
            .clone();
        request_cancel(&op);
        Some(describe(&op))
    }

    pub fn list(&self) -> Vec<Value> {
        self.map
            .lock()
            .expect("operations")
            .values()
            .map(|op| describe(op))
            .collect()
    }

    pub fn running(&self) -> usize {
        self.map
            .lock()
            .expect("operations")
            .values()
            .filter(|op| {
                matches!(
                    op.outcome.lock().expect("outcome").0,
                    OperationState::Queued | OperationState::Running
                )
            })
            .count()
    }

    /// Request cancellation of every operation and wait for the workers.
    pub fn cancel_all_and_join(&self) {
        for op in self.map.lock().expect("operations").values() {
            request_cancel(op);
        }
        let threads: Vec<JoinHandle<()>> =
            std::mem::take(&mut *self.threads.lock().expect("threads"));
        for thread in threads {
            let _ = thread.join();
        }
    }

    /// End an open Vault's operation history. Keep progress available while
    /// workers stop; discard results only once every worker has joined.
    pub(crate) fn close(&self) {
        self.cancel_all_and_join();
        self.map.lock().expect("operations").clear();
    }

    /// Wait for one operation (tests and the shell's bounded exit).
    pub fn wait(&self, id: &OperationId, timeout: std::time::Duration) -> Option<Value> {
        let start = std::time::Instant::now();
        loop {
            let status = self.status(id)?;
            let state = status["state"].as_str().unwrap_or_default().to_owned();
            if !matches!(state.as_str(), "queued" | "running") || start.elapsed() > timeout {
                return Some(status);
            }
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }
}

/// Serialize cancellation admission with terminal outcome publication. A
/// late request must not rewrite the history of a finished task.
fn request_cancel(op: &Operation) {
    let outcome = op.outcome.lock().expect("outcome");
    if matches!(outcome.0, OperationState::Queued | OperationState::Running) {
        op.cancel.store(true, Ordering::SeqCst);
    }
}

fn describe(op: &Operation) -> Value {
    let outcome = op.outcome.lock().expect("outcome");
    json!({
        "operationId": op.id,
        "kind": op.kind,
        "state": outcome.0,
        "progress": {"done": op.done.load(Ordering::SeqCst), "total": *op.total.lock().expect("total")},
        "cancelRequested": op.cancel.load(Ordering::SeqCst),
        "error": outcome.1,
        "result": outcome.2,
    })
}

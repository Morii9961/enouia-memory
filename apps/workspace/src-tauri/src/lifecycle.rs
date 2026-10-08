//! Reference-host lifecycle work belongs on blocking workers, never in the
//! native menu callback. Exit follows successful cleanup on that worker.

use enouia_memory_workspace::Workspace;
use serde_json::{Value, json};
use std::sync::Arc;
use tauri::async_runtime::JoinHandle;

pub fn queue_lock(core: Arc<Workspace>) -> JoinHandle<Value> {
    tauri::async_runtime::spawn_blocking(move || {
        core.call(&json!({
            "schemaVersion": 1,
            "requestId": "req_00000000-0000-4000-8000-000000000001",
            "command": "vault_lock", "idempotencyKey": null, "arguments": {},
        }))
    })
}

pub fn queue_shutdown(core: Arc<Workspace>) -> JoinHandle<()> {
    queue_exit(core, || {})
}

pub fn queue_exit(core: Arc<Workspace>, after: impl FnOnce() + Send + 'static) -> JoinHandle<()> {
    tauri::async_runtime::spawn_blocking(move || {
        core.shutdown();
        after();
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use enouia_memory_workspace::{Config, PickKind};
    use std::future::Future;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::task::{Context, Poll, Waker};
    use std::time::{Duration, Instant};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct Fixture {
        core: Arc<Workspace>,
        root: PathBuf,
    }

    impl Fixture {
        fn new() -> Self {
            let base = std::env::temp_dir().join("enouia-memory-shell-lifecycle-tests");
            std::fs::create_dir_all(&base).unwrap();
            let base = std::fs::canonicalize(base).unwrap();
            let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
            let root = base.join(format!(
                "synthetic-{}-{}",
                std::process::id(),
                COUNTER.fetch_add(1, Ordering::SeqCst)
            ));
            std::fs::create_dir(&root).unwrap();
            let core = Arc::new(Workspace::new(Config::default()));
            let picked = core.register_pick(PickKind::VaultRoot, &root).unwrap();
            let created = core.call(&json!({
                "schemaVersion": 1,
                "requestId": "req_00000000-0000-4000-8000-000000000002",
                "command": "vault_create", "idempotencyKey": null,
                "arguments": {"rootToken": picked.token, "confirmPhrase": "create new vault"},
            }));
            assert_eq!(created["error"], Value::Null, "{created}");
            Self { core, root }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.core.shutdown();
            let _ = std::fs::remove_dir_all(&self.root);
        }
    }

    fn while_worker_is_pending<T>(queue: impl FnOnce(Arc<Workspace>) -> JoinHandle<T>) -> T {
        let fixture = Fixture::new();
        let id = serde_json::from_value(json!("op_00000000-0000-4000-8000-000000000073")).unwrap();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        fixture
            .core
            .operations()
            .spawn(id, "synthetic_shutdown_wait", move |_| {
                ready_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                Ok(json!({"saved": true}))
            });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let start = Instant::now();
        let mut handle = Box::pin(queue(fixture.core.clone()));
        let admission = start.elapsed();
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut cancel_seen = false;
        while admission < Duration::from_millis(500) && Instant::now() < deadline {
            if fixture
                .core
                .operations()
                .list()
                .iter()
                .any(|op| op["cancelRequested"] == true)
            {
                cancel_seen = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }
        let mut context = Context::from_waker(Waker::noop());
        let first = handle.as_mut().poll(&mut context);
        let pending = first.is_pending();
        // Release even if an assertion would fail: never strand a test worker.
        let _ = release_tx.send(());
        let completed = match first {
            Poll::Ready(result) => result.unwrap(),
            Poll::Pending => loop {
                match handle.as_mut().poll(&mut context) {
                    Poll::Ready(result) => break result.unwrap(),
                    Poll::Pending => {
                        assert!(Instant::now() < deadline, "lifecycle worker did not finish");
                        std::thread::sleep(Duration::from_millis(5));
                    }
                }
            },
        };
        assert!(
            admission < Duration::from_millis(500),
            "native admission blocked: {admission:?}"
        );
        assert!(cancel_seen, "queued lifecycle requested cancellation");
        assert!(pending, "lifecycle cannot complete before its worker joins");
        assert_eq!(fixture.core.operations().list(), Vec::<Value>::new());
        completed
    }

    #[test]
    fn queued_lock_returns_before_join_and_finishes_locked() {
        let response = while_worker_is_pending(queue_lock);
        assert_eq!(response["error"], Value::Null);
        assert_eq!(response["result"]["vault"]["state"], "locked");
    }

    #[test]
    fn queued_shutdown_returns_before_join() {
        while_worker_is_pending(queue_shutdown);
    }

    #[test]
    fn exit_callback_runs_after_cleanup_on_a_worker() {
        let called = Arc::new(AtomicBool::new(false));
        let callback_thread = Arc::new(std::sync::Mutex::new(None));
        let native_thread = std::thread::current().id();
        let called_in_worker = called.clone();
        let thread_in_worker = callback_thread.clone();
        while_worker_is_pending(move |core| {
            let observed_core = core.clone();
            queue_exit(core, move || {
                assert!(observed_core.operations().list().is_empty());
                *thread_in_worker.lock().unwrap() = Some(std::thread::current().id());
                called_in_worker.store(true, Ordering::SeqCst);
            })
        });
        assert!(called.load(Ordering::SeqCst));
        assert_ne!(*callback_thread.lock().unwrap(), Some(native_thread));
    }

    #[test]
    fn dropping_a_tray_exit_handle_does_not_cancel_cleanup() {
        let (done_tx, done_rx) = std::sync::mpsc::channel();
        while_worker_is_pending(move |core| {
            drop(queue_exit(core, move || done_tx.send(()).unwrap()));
            tauri::async_runtime::spawn_blocking(move || {
                done_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            })
        });
    }

    #[test]
    fn dropping_a_tray_lock_handle_does_not_cancel_cleanup() {
        while_worker_is_pending(|core| {
            drop(queue_lock(core.clone()));
            tauri::async_runtime::spawn_blocking(move || {
                let deadline = Instant::now() + Duration::from_secs(5);
                while !core.operations().list().is_empty() {
                    assert!(Instant::now() < deadline, "detached lock did not finish");
                    std::thread::sleep(Duration::from_millis(5));
                }
                let status = core.call(&json!({
                    "schemaVersion": 1,
                    "requestId": "req_00000000-0000-4000-8000-000000000003",
                    "command": "workspace_status", "idempotencyKey": null, "arguments": {},
                }));
                assert_eq!(status["result"]["vault"]["state"], "locked");
            })
        });
    }
}

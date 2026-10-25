//! Opt-in Memory-owned maintenance. No frontend timer or notification channel.
use super::application::{ApplicationResult, AutomationHost};
use super::*;
use std::{
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Weak,
    time::Duration,
};

pub struct RecallRead {
    pub trace: RecallTrace,
    pub text: String,
}
struct MaintenanceControl {
    stop: Mutex<bool>,
    wake: Condvar,
}
pub struct MaintenanceWorker {
    control: Arc<MaintenanceControl>,
    thread: Option<JoinHandle<()>>,
}
impl MaintenanceWorker {
    pub fn cancel(&self) {
        if let Ok(mut stop) = self.control.stop.lock() {
            *stop = true;
            self.control.wake.notify_all();
        }
    }
    pub fn join(mut self) -> std::thread::Result<()> {
        self.cancel();
        self.thread.take().unwrap().join()
    }
}
impl Drop for MaintenanceWorker {
    fn drop(&mut self) {
        self.cancel();
    }
}
pub(super) fn spawn(
    host: Weak<AutomationHost>,
    profile: AttentionProfile,
    running: Arc<AtomicBool>,
) -> ApplicationResult<MaintenanceWorker> {
    if !profile.validate().is_empty() {
        return Err(super::application::ApplicationFault::Invalid);
    }
    let control = Arc::new(MaintenanceControl {
        stop: Mutex::new(false),
        wake: Condvar::new(),
    });
    let worker_control = control.clone();
    let thread = std::thread::Builder::new()
        .name("memory-maintenance".into())
        .spawn(move || {
            struct Running(Arc<AtomicBool>);
            impl Drop for Running {
                fn drop(&mut self) {
                    self.0.store(false, Ordering::SeqCst);
                }
            }
            let _running = Running(running);
            loop {
                let Ok(stop) = worker_control.stop.lock() else {
                    return;
                };
                if *stop {
                    return;
                }
                drop(stop);
                let Some(host) = host.upgrade() else {
                    return;
                };
                // No panic in derived maintenance may unwind through the product.
                let _ = catch_unwind(AssertUnwindSafe(|| {
                    host.maintain_attention_native(&profile)
                }));
                drop(host);
                let Ok(stop) = worker_control.stop.lock() else {
                    return;
                };
                if *stop {
                    return;
                }
                let _ = worker_control
                    .wake
                    .wait_timeout(stop, Duration::from_millis(profile.maintenance_interval_ms));
            }
        })
        .map_err(|_| super::application::ApplicationFault::Invalid)?;
    Ok(MaintenanceWorker {
        control,
        thread: Some(thread),
    })
}

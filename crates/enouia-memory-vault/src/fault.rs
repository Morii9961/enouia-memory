//! Failure injection at every persistence boundary (V02, V05). Production
//! code passes `Faults::none()`; tests arm a point to return an I/O error or
//! to abort the process there (a process crash, not an OS crash or power loss).

use std::io;
use std::sync::{Arc, Mutex};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum FaultPoint {
    /// Inside any managed file write, after open and before the bytes.
    WriteBytes,
    /// Inside an atomic replace, after the flushed temporary file exists.
    BeforeRename,
    /// Opt-in Host control file published; the response can still be lost.
    HostAuthorityPublished,
    HostIntentPublished,
    HostWitnessPublished,
    AfterLock,
    StagingWritten,
    RecordsPlaced,
    ObjectsPlaced,
    SegmentsPlaced,
    ManifestWritten,
    IdempotencyWritten,
    BeforeCurrent,
    AfterCurrent,
    AfterJournal,
}

impl FaultPoint {
    /// The commit boundaries in the order a transaction reaches them.
    pub const COMMIT_BOUNDARIES: [Self; 10] = [
        Self::AfterLock,
        Self::StagingWritten,
        Self::RecordsPlaced,
        Self::ObjectsPlaced,
        Self::SegmentsPlaced,
        Self::ManifestWritten,
        Self::IdempotencyWritten,
        Self::BeforeCurrent,
        Self::AfterCurrent,
        Self::AfterJournal,
    ];
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FaultAction {
    /// Return this raw OS error (e.g. 112 disk full, 5 access denied).
    Fail(i32),
    /// Terminate the process immediately, without unwinding.
    Abort,
}

#[derive(Debug)]
struct Armed {
    point: FaultPoint,
    /// Trigger on the n-th hit of the point (1 = first).
    nth: u32,
    seen: u32,
    action: FaultAction,
    fired: bool,
}

#[derive(Clone, Debug, Default)]
pub struct Faults(Option<Arc<Mutex<Vec<Armed>>>>);

impl Faults {
    pub fn none() -> Self {
        Self(None)
    }

    pub fn armed() -> Self {
        Self(Some(Arc::new(Mutex::new(Vec::new()))))
    }

    pub fn arm(&self, point: FaultPoint, nth: u32, action: FaultAction) {
        if let Some(list) = &self.0 {
            list.lock().expect("faults").push(Armed {
                point,
                nth: nth.max(1),
                seen: 0,
                action,
                fired: false,
            });
        }
    }

    pub fn disarm(&self) {
        if let Some(list) = &self.0 {
            list.lock().expect("faults").clear();
        }
    }

    pub fn io(&self, point: FaultPoint) -> io::Result<()> {
        let Some(list) = &self.0 else {
            return Ok(());
        };
        let mut list = list.lock().expect("faults");
        for armed in list.iter_mut().filter(|a| a.point == point && !a.fired) {
            armed.seen += 1;
            if armed.seen == armed.nth {
                armed.fired = true;
                match armed.action {
                    FaultAction::Fail(code) => return Err(io::Error::from_raw_os_error(code)),
                    FaultAction::Abort => abort_process(),
                }
            }
        }
        Ok(())
    }
}

/// Crash injection must stop at the armed boundary, without waiting for CRT
/// reporting or DLL cleanup. Normal Vault operation uses `Faults::none()`.
fn abort_process() -> ! {
    #[cfg(windows)]
    unsafe {
        use windows_sys::Win32::System::Threading::{GetCurrentProcess, TerminateProcess};
        // Only the current fault-injected process, never a caller-supplied PID.
        TerminateProcess(GetCurrentProcess(), 3);
    }
    // Portable path, and the fallback if the Windows call unexpectedly fails.
    std::process::abort()
}

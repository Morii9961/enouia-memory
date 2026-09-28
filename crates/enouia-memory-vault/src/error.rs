//! Store errors: a structured `MemoryError` for callers plus a local `Fault`
//! naming what happened. Neither carries paths, record text, or OS messages.

use enouia_memory_contract::foundation::ComponentId;
use enouia_memory_contract::{MemoryError, MemoryErrorCode};
use std::fmt;
use std::io;

/// Local diagnostic category. Safe to show in health output and logs.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Fault {
    DiskFull,
    AccessDenied,
    SharingViolation,
    NotFound,
    AlreadyExists,
    Io(io::ErrorKind),
    /// A managed path was unsafe: traversal, reparse point, or foreign name.
    UnsafePath(&'static str),
    /// Stored bytes failed verification (hash, parse, or chain).
    Corrupt(&'static str),
    /// The request violated a contract rule (stable rule identifiers).
    Contract(Vec<&'static str>),
    WriterBusy,
    HeadMoved,
    RevisionMismatch,
    IdempotencyConflict,
    ClockRegression,
    Recovering(&'static str),
    NotInitialized,
    AlreadyInitialized,
    /// An injected failure (tests only).
    Injected(&'static str),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VaultError {
    pub error: MemoryError,
    pub fault: Fault,
}

impl VaultError {
    pub fn new(code: MemoryErrorCode, fault: Fault) -> Self {
        Self {
            error: MemoryError::new(code, ComponentId::Vault),
            fault,
        }
    }

    pub fn code(&self) -> MemoryErrorCode {
        self.error.code
    }

    pub fn corrupt(what: &'static str) -> Self {
        Self::new(MemoryErrorCode::VaultRecovering, Fault::Corrupt(what))
    }

    pub fn recovering(what: &'static str) -> Self {
        Self::new(MemoryErrorCode::VaultRecovering, Fault::Recovering(what))
    }

    pub fn invalid(rules: Vec<&'static str>) -> Self {
        Self::new(MemoryErrorCode::InvalidRequest, Fault::Contract(rules))
    }

    pub fn unsafe_path(what: &'static str) -> Self {
        Self::new(MemoryErrorCode::PermissionDenied, Fault::UnsafePath(what))
    }
}

impl fmt::Display for VaultError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?} ({:?})", self.error.code, self.fault)
    }
}

impl std::error::Error for VaultError {}

impl From<MemoryError> for VaultError {
    fn from(error: MemoryError) -> Self {
        let fault = match error.code {
            MemoryErrorCode::ClockRegression => Fault::ClockRegression,
            MemoryErrorCode::Busy => Fault::WriterBusy,
            _ => Fault::Io(io::ErrorKind::Other),
        };
        Self { error, fault }
    }
}

impl From<VaultError> for MemoryError {
    fn from(error: VaultError) -> Self {
        error.error
    }
}

const ERROR_ACCESS_DENIED: i32 = 5;
const ERROR_SHARING_VIOLATION: i32 = 32;
const ERROR_LOCK_VIOLATION: i32 = 33;
const ERROR_HANDLE_DISK_FULL: i32 = 39;
const ERROR_DISK_FULL: i32 = 112;
const ERROR_DISK_QUOTA_EXCEEDED: i32 = 1295;

/// Classify an I/O error without keeping its message (which may hold a path).
pub fn classify_io(error: &io::Error) -> VaultError {
    use MemoryErrorCode as C;
    let raw = error.raw_os_error();
    match (error.kind(), raw) {
        (_, Some(ERROR_DISK_FULL | ERROR_HANDLE_DISK_FULL | ERROR_DISK_QUOTA_EXCEEDED))
        | (io::ErrorKind::StorageFull | io::ErrorKind::QuotaExceeded, _) => {
            VaultError::new(C::StorageFull, Fault::DiskFull)
        }
        (_, Some(ERROR_SHARING_VIOLATION | ERROR_LOCK_VIOLATION)) => {
            VaultError::new(C::Busy, Fault::SharingViolation)
        }
        (_, Some(ERROR_ACCESS_DENIED)) | (io::ErrorKind::PermissionDenied, _) => {
            VaultError::new(C::StorageFailed, Fault::AccessDenied)
        }
        (io::ErrorKind::NotFound, _) => VaultError::new(C::StorageFailed, Fault::NotFound),
        (io::ErrorKind::AlreadyExists, _) => {
            VaultError::new(C::StorageFailed, Fault::AlreadyExists)
        }
        (kind, _) => VaultError::new(C::StorageFailed, Fault::Io(kind)),
    }
}

impl From<io::Error> for VaultError {
    fn from(error: io::Error) -> Self {
        classify_io(&error)
    }
}

pub type Result<T> = std::result::Result<T, VaultError>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_errors_map_to_explicit_codes_without_text() {
        let full = io::Error::from_raw_os_error(ERROR_DISK_FULL);
        assert_eq!(classify_io(&full).code(), MemoryErrorCode::StorageFull);
        let shared = io::Error::from_raw_os_error(ERROR_SHARING_VIOLATION);
        assert_eq!(classify_io(&shared).code(), MemoryErrorCode::Busy);
        let denied = io::Error::new(io::ErrorKind::PermissionDenied, "C:\\secret\\path");
        let mapped = classify_io(&denied);
        assert_eq!(mapped.fault, Fault::AccessDenied);
        assert!(!mapped.to_string().contains("secret"));
    }
}

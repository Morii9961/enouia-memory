//! Contract validation errors and the Memory-domain structured error codes.

use crate::foundation::ComponentId;
use serde::{Deserialize, Serialize};
use std::fmt;

/// One failed rule. `rule` is a stable identifier used by fixtures and reports;
/// `path` is a JSON Pointer or record reference. Neither contains record text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Violation {
    pub rule: &'static str,
    pub path: String,
}

impl Violation {
    pub fn new(rule: &'static str, path: impl Into<String>) -> Self {
        Self {
            rule,
            path: path.into(),
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at {}", self.rule, self.path)
    }
}

/// Why a record could not be accepted for writing.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ContractError {
    /// Not JSON, or not a JSON object.
    Malformed,
    /// A different major `schema_version`. The bytes may be preserved and shown
    /// read-only, but this program must never rewrite them.
    UnsupportedSchema { found: Option<i64> },
    /// Serde rejected the shape: missing/unknown field, wrong type or enum value,
    /// invalid ID/time/hash syntax. The message never includes field values.
    Shape(String),
    /// The shape parsed but semantic rules failed.
    Invalid(Vec<Violation>),
}

impl ContractError {
    /// Stable rule identifiers carried by this error, for fixtures and reports.
    pub fn rules(&self) -> Vec<&'static str> {
        match self {
            Self::Malformed => vec!["malformed"],
            Self::UnsupportedSchema { .. } => vec!["unsupported_schema"],
            Self::Shape(_) => vec!["shape"],
            Self::Invalid(violations) => violations.iter().map(|v| v.rule).collect(),
        }
    }

    pub fn memory_code(&self) -> MemoryErrorCode {
        match self {
            Self::UnsupportedSchema { .. } => MemoryErrorCode::UnsupportedSchema,
            _ => MemoryErrorCode::InvalidRequest,
        }
    }
}

impl fmt::Display for ContractError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Malformed => f.write_str("malformed JSON object"),
            Self::UnsupportedSchema { found } => write!(f, "unsupported schema_version {found:?}"),
            Self::Shape(message) => write!(f, "shape: {message}"),
            Self::Invalid(violations) => {
                f.write_str("invalid:")?;
                for violation in violations {
                    write!(f, " [{violation}]")?;
                }
                Ok(())
            }
        }
    }
}

/// Memory-domain error codes exposed through IPC and returned by ports.
/// Codes from INTERFACES §1 plus the MV-0 additions recorded in ADR-020.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MemoryErrorCode {
    Unauthenticated,
    PermissionDenied,
    NotFound,
    VaultLocked,
    VaultRecovering,
    Busy,
    RevisionConflict,
    IdempotencyConflict,
    InvalidRequest,
    InvalidSource,
    BrokenProvenance,
    UnsupportedSchema,
    IndexNotReady,
    BudgetExceeded,
    UnsupportedBudget,
    StorageFull,
    StorageFailed,
    AuditUnavailable,
    ProviderUnavailable,
    Offline,
    Cancelled,
    ClockRegression,
    IntentionallyPurged,
}

impl MemoryErrorCode {
    pub const ALL: [Self; 23] = [
        Self::Unauthenticated,
        Self::PermissionDenied,
        Self::NotFound,
        Self::VaultLocked,
        Self::VaultRecovering,
        Self::Busy,
        Self::RevisionConflict,
        Self::IdempotencyConflict,
        Self::InvalidRequest,
        Self::InvalidSource,
        Self::BrokenProvenance,
        Self::UnsupportedSchema,
        Self::IndexNotReady,
        Self::BudgetExceeded,
        Self::UnsupportedBudget,
        Self::StorageFull,
        Self::StorageFailed,
        Self::AuditUnavailable,
        Self::ProviderUnavailable,
        Self::Offline,
        Self::Cancelled,
        Self::ClockRegression,
        Self::IntentionallyPurged,
    ];

    /// Default retry guidance. Retrying a write is only safe with the same
    /// idempotency key and payload; a conflict code is never fixed by retrying.
    pub const fn default_retryable(self) -> bool {
        matches!(
            self,
            Self::VaultLocked
                | Self::VaultRecovering
                | Self::Busy
                | Self::IndexNotReady
                | Self::StorageFull
                | Self::StorageFailed
                | Self::AuditUnavailable
                | Self::ProviderUnavailable
                | Self::Offline
                | Self::ClockRegression
        )
    }

    /// Codes that must be returned instead of any record content: callers learn
    /// nothing about hidden records from them.
    pub const fn is_non_disclosing(self) -> bool {
        matches!(
            self,
            Self::Unauthenticated | Self::PermissionDenied | Self::NotFound
        )
    }
}

/// Structured Memory error. Never carries raw exception text, paths, tokens, or
/// record content. A host maps `component` onto its own health DTOs.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct MemoryError {
    pub code: MemoryErrorCode,
    pub component: ComponentId,
    pub retryable: bool,
}

impl MemoryError {
    pub const fn new(code: MemoryErrorCode, component: ComponentId) -> Self {
        Self {
            code,
            component,
            retryable: code.default_retryable(),
        }
    }
}

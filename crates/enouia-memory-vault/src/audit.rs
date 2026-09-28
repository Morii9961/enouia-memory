//! Restricted access audit (MV-1.3): append-only JSONL segments under
//! `vault/audit/`, rotated by size. Each line is one validated `AuditEvent`,
//! whose shape admits only IDs, enums, codes, and times: no record text,
//! query text, file names, secrets, or raw error messages can be written.
//! A failed append is `audit_unavailable`, and callers must fail closed for
//! external reads and Provider dispatch.

use crate::error::{Fault, Result, VaultError};
use crate::fs::ManagedRoot;
use enouia_memory_contract::MemoryErrorCode;
use enouia_memory_contract::commit::AuditEvent;
use enouia_memory_contract::foundation::ComponentId;
use enouia_memory_contract::ports::AuditSink;
use std::sync::Mutex;

pub const AUDIT_DIR: &str = "vault/audit";
pub const DEFAULT_SEGMENT_BYTES: u64 = 1024 * 1024;

pub struct AuditLog {
    root: ManagedRoot,
    segment_bytes: u64,
    lock: Mutex<()>,
}

fn segment_name(index: u32) -> String {
    format!("segment-{index:06}.jsonl")
}

fn unavailable() -> VaultError {
    let mut error = VaultError::new(
        MemoryErrorCode::AuditUnavailable,
        Fault::Io(std::io::ErrorKind::Other),
    );
    error.error.component = ComponentId::Audit;
    error
}

impl AuditLog {
    pub fn new(root: ManagedRoot, segment_bytes: u64) -> Self {
        Self {
            root,
            segment_bytes: segment_bytes.max(1),
            lock: Mutex::new(()),
        }
    }

    /// Segment names in order (`segment-000001.jsonl`, ...).
    pub fn segments(&self) -> Result<Vec<String>> {
        Ok(self
            .root
            .list(AUDIT_DIR)?
            .into_iter()
            .filter(|n| n.starts_with("segment-") && n.ends_with(".jsonl") && n.len() == 20)
            .collect())
    }

    fn current_segment(&self) -> Result<String> {
        let segments = self.segments()?;
        let Some(last) = segments.last() else {
            return Ok(segment_name(1));
        };
        let index: u32 = last[8..14]
            .parse()
            .map_err(|_| VaultError::corrupt("audit segment"))?;
        let size = self
            .root
            .read(&format!("{AUDIT_DIR}/{last}"))?
            .map_or(0, |b| b.len() as u64);
        Ok(if size >= self.segment_bytes {
            segment_name(index + 1)
        } else {
            last.clone()
        })
    }

    pub fn append(&self, event: &AuditEvent) -> Result<()> {
        let violations = event.validate();
        if !violations.is_empty() {
            return Err(VaultError::invalid(
                violations.iter().map(|v| v.rule).collect(),
            ));
        }
        let _guard = self.lock.lock().map_err(|_| unavailable())?;
        let line = serde_json::to_vec(event).map_err(|_| unavailable())?;
        let segment = self.current_segment().map_err(|_| unavailable())?;
        self.root
            .append_line(&format!("{AUDIT_DIR}/{segment}"), &line)
            .map_err(|_| unavailable())
    }

    /// Every event, oldest first. A torn final line (a crash mid-append) is
    /// skipped and counted.
    pub fn read_all(&self) -> Result<(Vec<AuditEvent>, usize)> {
        let mut events = Vec::new();
        let mut torn = 0;
        for segment in self.segments()? {
            let bytes = self
                .root
                .read(&format!("{AUDIT_DIR}/{segment}"))?
                .unwrap_or_default();
            for line in bytes.split(|&b| b == b'\n').filter(|l| !l.is_empty()) {
                match serde_json::from_slice::<AuditEvent>(line) {
                    Ok(event) => events.push(event),
                    Err(_) => torn += 1,
                }
            }
        }
        Ok((events, torn))
    }
}

impl AuditSink for AuditLog {
    fn record(
        &self,
        event: &AuditEvent,
    ) -> std::result::Result<(), enouia_memory_contract::MemoryError> {
        self.append(event).map_err(Into::into)
    }
}

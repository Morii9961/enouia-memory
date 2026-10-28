//! Explicit Runtime-only protected local source transport. Not an MCP tool.
use super::application::ApplicationFault;
use super::runtime_capture::RuntimeCaptureHost;
use crate::{
    framing::Tag,
    transport::{Result, TransportFault, read, write},
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_contract::{common::*, hash::sha256, ids::*, json::canonical_bytes, next::*};
use enouia_memory_vault::automation::AutomationFault;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::time::Duration;

pub const PROTOCOL: &str = "memory-runtime-capture-pipe-1";
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Write,
    Preflight,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Call {
    pub mode: Mode,
    pub request: RuntimeCaptureRequest,
}
#[derive(Deserialize)]
enum ReceiptKind {
    #[serde(rename = "native_runtime_capture")]
    Capture,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Receipt {
    schema_version: NextVersion,
    kind: ReceiptKind,
    capability: RuntimeCaptureCapability,
    request_id: RequestId,
    request_hash: enouia_memory_contract::hash::Sha256Hex,
    producer: ActorRef,
    scope_hash: enouia_memory_contract::hash::Sha256Hex,
    observed_in: SnapshotBinding,
    sources: Vec<SourceRevisionRef>,
    captured: bool,
    preflight: bool,
    replayed: bool,
    formation_scheduled: bool,
}
impl Receipt {
    fn matches(&self, call: &Call, vault: &VaultId) -> bool {
        let _ = (
            &self.schema_version,
            &self.kind,
            &self.capability,
            &self.scope_hash,
        );
        self.request_id == call.request.request_id
            && self.request_hash == digest(&call.request)
            && self.producer.actor_type == ActorType::Client
            && self.observed_in.vault_id == *vault
            && self.sources.len() == call.request.turns.len()
            && self
                .sources
                .iter()
                .map(|s| &s.source_id)
                .collect::<std::collections::BTreeSet<_>>()
                .len()
                == self.sources.len()
            && self.preflight == (call.mode == Mode::Preflight)
            && !self.formation_scheduled
            && self.captured == (call.mode == Mode::Write || self.replayed)
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum ErrorCode {
    OutcomeUnknown,
    PermissionDenied,
    InvalidRequest,
    Capacity,
    Unavailable,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Reply {
    request_id: RequestId,
    mode: Mode,
    #[serde(deserialize_with = "enouia_memory_contract::json::nullable")]
    capture: Option<Receipt>,
    #[serde(deserialize_with = "enouia_memory_contract::json::nullable")]
    error: Option<ErrorCode>,
}

pub fn hello(vault: &VaultId) -> Value {
    let descriptor = json!({"protocol":PROTOCOL,"recordSchema":2,"layout":"automation-segmented-host-v1","capability":RUNTIME_CAPTURE_CAPABILITY});
    let schemas = std::collections::BTreeMap::from([
        (
            "capture",
            include_str!("../../../../contracts/next/runtime-capture-v2.schema.json"),
        ),
        (
            "pipe",
            include_str!("../../../../contracts/next/runtime-capture-pipe-v2.schema.json"),
        ),
        (
            "common",
            include_str!("../../../../contracts/memory/common-v1.schema.json"),
        ),
        (
            "automation",
            include_str!("../../../../contracts/next/automation-v2.schema.json"),
        ),
    ])
    .into_iter()
    .map(|(k, v)| (k, v.replace("\r\n", "\n")))
    .collect::<std::collections::BTreeMap<_, _>>();
    let hash=sha256(&canonical_bytes(&json!({"descriptor":descriptor,"schemas":schemas,"framing":crate::framing::DESCRIPTOR})).expect("constant compatibility metadata"));
    let mut value = descriptor;
    value["vaultId"] = json!(vault);
    value["compatibilityHash"] = json!(hash);
    value
}
fn error(e: &ApplicationFault) -> &'static str {
    match e {
        ApplicationFault::Vault(AutomationFault::OutcomeUnknown) => "outcome_unknown",
        ApplicationFault::Vault(AutomationFault::Capacity) => "capacity",
        ApplicationFault::Vault(AutomationFault::Conflict) | ApplicationFault::Invalid => {
            "invalid_request"
        }
        ApplicationFault::Locked | ApplicationFault::Vault(AutomationFault::Denied) => {
            "permission_denied"
        }
        _ => "unavailable",
    }
}
fn parse(bytes: &[u8]) -> Result<Call> {
    let value = crate::mcp::json::parse(bytes).map_err(|_| TransportFault::InvalidFrame)?;
    // Refuse unsupported record/capability before decoding any turn text.
    if value["request"]["schema_version"] != 2
        || value["request"]["capability"] != RUNTIME_CAPTURE_CAPABILITY
    {
        return Err(TransportFault::Incompatible);
    }
    let call: Call = serde_json::from_value(value).map_err(|_| TransportFault::InvalidFrame)?;
    if !call.request.valid() {
        return Err(TransportFault::InvalidFrame);
    }
    Ok(call)
}
/// Binds a single enrollment, invalidated on any exit. Native control can retain
/// the Host in an Arc and revoke it while this service borrows it.
/// Caller has accepted the
/// kernel pipe, but no peer-supplied process/identity is trusted.
pub fn serve_connection(
    host: &RuntimeCaptureHost,
    pipe: &mut Pipe,
    timeout: Duration,
) -> Result<()> {
    serve_with_reply(host, pipe, timeout, || Ok(()))
}
pub(crate) fn serve_with_reply(
    host: &RuntimeCaptureHost,
    pipe: &mut Pipe,
    timeout: Duration,
    mut before_reply: impl FnMut() -> Result<()>,
) -> Result<()> {
    host.bind_connection()
        .map_err(|_| TransportFault::Incompatible)?;
    struct Revoke<'a>(&'a RuntimeCaptureHost);
    impl Drop for Revoke<'_> {
        fn drop(&mut self) {
            self.0.revoke();
        }
    }
    let _revoke = Revoke(host);
    let check = |pipe: &Pipe| {
        host.verify_peer(pipe)
            .map_err(|_| TransportFault::Incompatible)
    };
    check(pipe)?;
    let (tag, bytes) = read(pipe, true, timeout)?;
    check(pipe)?;
    let expected = hello(host.vault_id());
    if tag != Tag::Hello || crate::mcp::json::parse(&bytes).ok().as_ref() != Some(&expected) {
        return Err(TransportFault::Incompatible);
    }
    write(pipe, Tag::Welcome, &expected, timeout)?;
    for _ in 0..256 {
        check(pipe)?;
        let (tag, bytes) = read(pipe, true, timeout)?;
        check(pipe)?;
        if tag != Tag::Call {
            return Err(TransportFault::InvalidFrame);
        }
        let call = parse(&bytes)?;
        let result = match call.mode {
            Mode::Preflight => host.preflight(&call.request),
            Mode::Write => host.capture(&call.request),
        };
        let (capture, fault) = match result {
            Ok(value) => (
                Some(serde_json::to_value(value).map_err(|_| TransportFault::InvalidFrame)?),
                None,
            ),
            Err(e) => (None, Some(error(&e))),
        };
        let response = json!({"request_id":call.request.request_id,"mode":call.mode,"capture":capture,"error":fault});
        if before_reply()
            .and_then(|_| check(pipe))
            .and_then(|_| write(pipe, Tag::Reply, &response, timeout))
            .is_err()
        {
            if call.mode == Mode::Write {
                let _ = host.response_lost(&call.request);
            }
            return Err(TransportFault::InvalidFrame);
        }
        if fault == Some("outcome_unknown") {
            return Ok(());
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "runtime_capture_transport_tests.rs"]
mod tests;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExchangeFault {
    NotSent,
    OutcomeUnknown,
    Closed,
}
#[derive(Debug)]
pub struct ExchangeFailure {
    pub fault: ExchangeFault,
    pub original: Call,
}
pub struct Client {
    pipe: Pipe,
    host: ProcessPeer,
    vault: VaultId,
    timeout: Duration,
    closed: bool,
    unknown: Option<Call>,
}
impl Client {
    pub fn connect(
        mut pipe: Pipe,
        host: ProcessPeer,
        vault: &VaultId,
        timeout: Duration,
    ) -> Result<Self> {
        host.verify_server(&pipe)?;
        let expected = hello(vault);
        write(&mut pipe, Tag::Hello, &expected, timeout)?;
        let (tag, bytes) = read(&mut pipe, false, timeout)?;
        host.verify_server(&pipe)?;
        if tag != Tag::Welcome || crate::mcp::json::parse(&bytes).ok().as_ref() != Some(&expected) {
            return Err(TransportFault::Incompatible);
        }
        Ok(Self {
            pipe,
            host,
            vault: vault.clone(),
            timeout,
            closed: false,
            unknown: None,
        })
    }
    pub fn original_unknown(&self) -> Option<&Call> {
        self.unknown.as_ref()
    }
    pub fn exchange(&mut self, call: &Call) -> std::result::Result<Value, ExchangeFailure> {
        let fail = |fault| ExchangeFailure {
            fault,
            original: call.clone(),
        };
        if self.closed {
            return Err(fail(ExchangeFault::Closed));
        }
        if !call.request.valid() {
            return Err(fail(ExchangeFault::NotSent));
        }
        let message = serde_json::to_value(call).map_err(|_| fail(ExchangeFault::NotSent))?;
        if self.host.verify_server(&self.pipe).is_err() {
            self.closed = true;
            return Err(fail(ExchangeFault::NotSent));
        }
        if crate::framing::encode(&message, 256 * 1024).is_err() {
            return Err(fail(ExchangeFault::NotSent));
        }
        self.closed = true;
        let run = (|| -> Result<Value> {
            write(&mut self.pipe, Tag::Call, &message, self.timeout)?;
            let (tag, bytes) = read(&mut self.pipe, false, self.timeout)?;
            self.host.verify_server(&self.pipe)?;
            let v = crate::mcp::json::parse(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
            let reply: Reply =
                serde_json::from_value(v.clone()).map_err(|_| TransportFault::InvalidFrame)?;
            if tag != Tag::Reply
                || reply.request_id != call.request.request_id
                || reply.mode != call.mode
                || reply.capture.is_some() == reply.error.is_some()
                || reply
                    .capture
                    .as_ref()
                    .is_some_and(|r| !r.matches(call, &self.vault))
            {
                return Err(TransportFault::InvalidFrame);
            }
            Ok(v)
        })();
        match run {
            Ok(value) => {
                self.closed = value["error"] == "outcome_unknown";
                if self.closed {
                    self.unknown = Some(call.clone());
                }
                Ok(value)
            }
            Err(_) => {
                self.unknown = Some(call.clone());
                Err(fail(ExchangeFault::OutcomeUnknown))
            }
        }
    }
}

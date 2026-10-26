//! Native operator enrollment and an Agent-only protected pipe (ADR-MEM-92).
use super::{Grant, MAX_INPUT_BYTES, MAX_OUTPUT_BYTES, PROTOCOL_VERSION, json as unique};
use crate::framing::Tag;
use crate::service::HostService;
use crate::transport::{Result, TransportFault, read, write};
use crate::windows::{Pipe, ProcessPeer};
use enouia_memory_contract::common::ActorRef;
use enouia_memory_contract::hash::sha256;
use enouia_memory_contract::ids::VaultId;
use enouia_memory_contract::json::canonical_bytes;
use enouia_memory_contract::policy::Scope;
use enouia_memory_contract::workspace::HostSurface;
use serde_json::{Value, json};
use std::io::{self, BufRead, Read, Write};
use std::time::{Duration, Instant};

const TTL: Duration = Duration::from_millis(crate::SESSION_TTL_MS as u64);

/// Exact compatibility assertion; carries no principal, PID, scope or token.
pub fn hello(vault: &VaultId) -> Value {
    let descriptor = json!({
        "protocol":"memory-agent-pipe-1", "recordSchema":1,
        "layout":"host-current-v2", "capability":"memory-agent-six-tools-1",
        "mcpVersion":PROTOCOL_VERSION, "memoryVersion":1,
    });
    let hash = sha256(&canonical_bytes(&json!({
        "descriptor":descriptor, "host":crate::compatibility_hash(),
        "memory":include_str!("../../../../contracts/ipc/memory-v1.schema.json").replace("\r\n", "\n"),
        "common":include_str!("../../../../contracts/memory/common-v1.schema.json").replace("\r\n", "\n"),
    })).expect("constant compatibility metadata"));
    let mut hello = descriptor;
    hello["vaultId"] = json!(vault);
    hello["compatibilityHash"] = json!(hash);
    hello
}

/// Single-use native operator enrollment. Never construct from received bytes.
/// The retained identity is used only for kernel peer checks, never for Owner
/// admission; its legacy ProcessPeer surface is deliberately unused.
pub struct Enrollment {
    peer: ProcessPeer,
    grant: Grant,
    until: Instant,
}
impl Enrollment {
    pub fn enroll(pid: u32, principal: ActorRef, scopes: Vec<Scope>) -> Result<Self> {
        if scopes.is_empty()
            || scopes
                .iter()
                .enumerate()
                .any(|(i, s)| scopes[..i].contains(s))
        {
            return Err(TransportFault::Incompatible);
        }
        let grant = Grant::from_authenticated_agent(principal, scopes)
            .ok_or(TransportFault::Incompatible)?;
        Ok(Self {
            peer: ProcessPeer::enroll(pid, HostSurface::QuickSearch)?,
            grant,
            until: Instant::now() + TTL,
        })
    }
    pub(super) fn allows_v2_reader(&self, principal: &ActorRef, source_read: bool) -> bool {
        self.grant.principal == *principal
            && self.grant.scopes.contains(&Scope::MemoryRead)
            && self
                .grant
                .scopes
                .iter()
                .all(|s| matches!(s, Scope::MemoryRead | Scope::SourceRead))
            && (!source_read || self.grant.scopes.contains(&Scope::SourceRead))
    }

    /// Native revocation handle; shared with the server, not serialized.
    pub fn revocation(&self) -> Grant {
        self.grant.clone()
    }
    pub fn live(&self) -> Result<()> {
        self.peer.live()?;
        if Instant::now() >= self.until
            || !self.grant.active.load(std::sync::atomic::Ordering::SeqCst)
        {
            return Err(TransportFault::Incompatible);
        }
        Ok(())
    }
    pub(super) fn check(&self, pipe: &Pipe) -> Result<()> {
        self.live()?;
        self.peer.verify_client(pipe)?;
        Ok(())
    }
    pub(super) fn timeout(&self, timeout: Duration) -> Duration {
        timeout.min(self.until.saturating_duration_since(Instant::now()))
    }
}
impl Drop for Enrollment {
    fn drop(&mut self) {
        self.grant.revoke();
    }
}

/// Consumes admission, borrowing the already-bound sole Host. Only MCP frames
/// are accepted; owner/control frames and client-supplied enrollment are absent.
pub fn serve_connection(
    host: &HostService,
    pipe: &mut Pipe,
    enrollment: Enrollment,
    timeout: Duration,
) -> Result<()> {
    serve_with_reply(host, pipe, enrollment, timeout, |_| Ok(()))
}

pub(crate) fn serve_with_reply(
    host: &HostService,
    pipe: &mut Pipe,
    enrollment: Enrollment,
    timeout: Duration,
    mut before_reply: impl FnMut(&Option<Value>) -> Result<()>,
) -> Result<()> {
    enrollment.check(pipe)?;
    let (tag, bytes) = read(pipe, true, enrollment.timeout(timeout))?;
    enrollment.check(pipe)?;
    let expected = hello(host.agent_vault_id());
    if tag != Tag::Hello || unique::parse(&bytes).ok().as_ref() != Some(&expected) {
        return Err(TransportFault::Incompatible);
    }
    let mut server = host.mcp_agent_server(enrollment.grant.clone())?;
    write(pipe, Tag::Welcome, &expected, enrollment.timeout(timeout))?;
    loop {
        enrollment.check(pipe)?;
        server.port.check_live()?;
        let (tag, bytes) = read(pipe, true, enrollment.timeout(timeout))?;
        enrollment.check(pipe)?;
        server.port.check_live()?;
        if tag != Tag::Call {
            return Err(TransportFault::InvalidFrame);
        }
        let message = unique::parse(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
        let response = server.handle(&message);
        before_reply(&response)?;
        write(
            pipe,
            Tag::Reply,
            &json!({"response":response}),
            enrollment.timeout(timeout),
        )?;
        if server.phase == super::Phase::Closed {
            return Ok(());
        }
    }
}

/// Retains the exact packet on any ambiguous exchange and poisons the channel.
#[derive(Debug)]
pub struct ExchangeFailure {
    pub original_message: Value,
    pub fault: ExchangeFault,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ExchangeFault {
    Closed,
    NotSent,
    OutcomeUnknown,
}

/// No Vault access or automatic connect/reconnect/retry. Native launcher fixes
/// the expected live Host object and selected Vault; wire data cannot choose it.
pub struct Client {
    pipe: Pipe,
    host: ProcessPeer,
    timeout: Duration,
    poisoned: bool,
    original_unknown: Option<Value>,
}
impl Client {
    pub fn connect(
        pipe: Pipe,
        host: ProcessPeer,
        vault: &VaultId,
        timeout: Duration,
    ) -> Result<Self> {
        Self::connect_expected(pipe, host, hello(vault), timeout)
    }
    pub(super) fn connect_expected(
        mut pipe: Pipe,
        host: ProcessPeer,
        expected: Value,
        timeout: Duration,
    ) -> Result<Self> {
        host.verify_server(&pipe)?;
        write(&mut pipe, Tag::Hello, &expected, timeout)?;
        let (tag, bytes) = read(&mut pipe, false, timeout)?;
        host.verify_server(&pipe)?;
        if tag != Tag::Welcome || unique::parse(&bytes).ok().as_ref() != Some(&expected) {
            return Err(TransportFault::Incompatible);
        }
        Ok(Self {
            pipe,
            host,
            timeout,
            poisoned: false,
            original_unknown: None,
        })
    }
    pub fn exchange(
        &mut self,
        message: &Value,
    ) -> std::result::Result<Option<Value>, ExchangeFailure> {
        let fail = |fault| ExchangeFailure {
            original_message: message.clone(),
            fault,
        };
        if self.poisoned {
            return Err(fail(ExchangeFault::Closed));
        }
        if self.host.verify_server(&self.pipe).is_err() {
            self.poisoned = true;
            return Err(fail(ExchangeFault::NotSent));
        }
        if crate::framing::encode(message, MAX_INPUT_BYTES).is_err() {
            return Err(fail(ExchangeFault::NotSent));
        }
        self.poisoned = true;
        let run = (|| -> Result<Option<Value>> {
            self.host.verify_server(&self.pipe)?;
            write(&mut self.pipe, Tag::Call, message, self.timeout)?;
            let (tag, bytes) = read(&mut self.pipe, false, self.timeout)?;
            self.host.verify_server(&self.pipe)?;
            let reply = unique::parse(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
            if tag != Tag::Reply
                || !reply
                    .as_object()
                    .is_some_and(|o| o.len() == 1 && o.contains_key("response"))
            {
                return Err(TransportFault::InvalidFrame);
            }
            let response = &reply["response"];
            if message.get("id").is_none() {
                if !response.is_null() {
                    return Err(TransportFault::InvalidFrame);
                }
                return Ok(None);
            }
            if response["jsonrpc"] != "2.0"
                || response["id"] != message["id"]
                || (response.get("result").is_some() == response.get("error").is_some())
            {
                return Err(TransportFault::InvalidFrame);
            }
            Ok(Some(response.clone()))
        })();
        match run {
            Ok(value) => {
                let unknown = value.as_ref().is_some_and(|v| {
                    v["result"]["structuredContent"]["error"]["code"] == "outcome_unknown"
                });
                self.poisoned = unknown;
                if unknown {
                    self.original_unknown = Some(message.clone());
                }
                Ok(value)
            }
            Err(_) => {
                self.original_unknown = Some(message.clone());
                Err(fail(ExchangeFault::OutcomeUnknown))
            }
        }
    }
    pub(super) fn close(&mut self) {
        self.poisoned = true;
    }
    pub(super) fn retain_output_unknown(&mut self, message: &Value) {
        self.poisoned = true;
        self.original_unknown = Some(message.clone());
    }
    pub fn original_unknown(&self) -> Option<&Value> {
        self.original_unknown.as_ref()
    }

    /// Runnable stdio bridge to the enrolled Host. EOF/output failure ends this
    /// channel. A failed call emits the exact original tool packet then exits.
    pub fn serve_stdio(
        &mut self,
        input: &mut impl BufRead,
        output: &mut impl Write,
    ) -> io::Result<()> {
        let result = self.stdio_inner(input, output);
        self.poisoned = true;
        result
    }
    fn stdio_inner(&mut self, input: &mut impl BufRead, output: &mut impl Write) -> io::Result<()> {
        loop {
            let mut line = Vec::new();
            let n = (&mut *input)
                .take((MAX_INPUT_BYTES + 2) as u64)
                .read_until(b'\n', &mut line)?;
            if n == 0 {
                return Ok(());
            }
            if line.last() != Some(&b'\n') || line.len() > MAX_INPUT_BYTES + 1 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            line.pop();
            let message =
                unique::parse(&line).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
            let (response, close) = match self.exchange(&message) {
                Ok(response) => (response, false),
                Err(failure) => {
                    let original = failure
                        .original_message
                        .get("params")
                        .and_then(|p| p.get("arguments"))
                        .unwrap_or(&failure.original_message);
                    (
                        message.get("id").map(|id| {
                            super::reply(
                                id.clone(),
                                super::tool_error(
                                    if failure.fault == ExchangeFault::OutcomeUnknown {
                                        "outcome_unknown"
                                    } else {
                                        "connection_closed"
                                    },
                                    Some(original),
                                ),
                            )
                        }),
                        true,
                    )
                }
            };
            if let Some(response) = response {
                // Retain the original if stdout fails after Host completion.
                self.original_unknown = Some(message.clone());
                let bytes = response.to_string();
                if bytes.len() > MAX_OUTPUT_BYTES {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                output.write_all(bytes.as_bytes())?;
                output.write_all(b"\n")?;
                output.flush()?;
                if !self.poisoned {
                    self.original_unknown = None;
                }
            }
            if self.poisoned {
                return Ok(());
            }
            if close {
                return Ok(());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use enouia_memory_contract::common::ActorType;
    use enouia_memory_contract::ids::PrincipalId;
    #[test]
    fn native_enrollment_monotonic_expiry_and_drop_revoke_its_shared_grant() {
        let principal = ActorRef {
            actor_type: ActorType::Agent,
            actor_id: PrincipalId::parse("prn_00000002-0000-4000-8000-000000000002").unwrap(),
        };
        let mut enrollment =
            Enrollment::enroll(std::process::id(), principal, vec![Scope::MemoryRead]).unwrap();
        let revocation = enrollment.revocation();
        assert!(enrollment.live().is_ok());
        enrollment.until = Instant::now();
        assert!(enrollment.live().is_err());
        drop(enrollment);
        assert!(!revocation.allows(enouia_memory_contract::ipc::Operation::MemoryRead));
    }
}

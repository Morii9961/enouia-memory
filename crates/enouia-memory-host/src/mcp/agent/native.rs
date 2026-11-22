//! ADR-MEM-166: combined Agent protected pipe. Idle waits are not timeouts;
//! a caller-owned stop flag ends an idle connection within a quarter second.
use super::{AgentWriters, Server};
use crate::{
    automation::qualified_agent::QualifiedAgentReader,
    framing::Tag,
    mcp::{
        PROTOCOL_VERSION, corrections, forget_requests, initialized, json as unique,
        native::{Enrollment, ExchangeFailure},
        notes,
    },
    transport::{Result, TransportFault, read, read_idle, write},
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_contract::{
    hash::sha256,
    ids::VaultId,
    json::canonical_bytes,
    next::{
        AGENT_CORRECTION_CAPABILITY, AGENT_FORGET_CAPABILITY, AGENT_NOTE_CAPABILITY,
        AttentionProfile,
    },
};
use serde_json::{Value, json};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// Fixed for every grant combination; `tools/list` shows the allowed tools.
pub fn hello(vault: &VaultId) -> Value {
    let descriptor = json!({"protocol":"memory-agent-pipe-11","recordSchema":2,"layout":"automation-segmented-owner-v1","capability":"memory-agent-combined-1","readCapability":initialized::CAPABILITY,"noteCapability":notes::CAPABILITY,"correctionCapability":corrections::CAPABILITY,"forgetCapability":forget_requests::CAPABILITY,"nativeReadCapability":"memory-agent-initialized-native-read-1","nativeNoteCapability":AGENT_NOTE_CAPABILITY,"nativeCorrectionCapability":AGENT_CORRECTION_CAPABILITY,"nativeForgetCapability":AGENT_FORGET_CAPABILITY,"sourceCarrier":"captured-source-import-1","mcpVersion":PROTOCOL_VERSION,"memoryVersion":2});
    let read_hello = initialized::native::hello(vault);
    let hash = sha256(
        &canonical_bytes(&json!({"descriptor":descriptor,
        "readCompatibility":read_hello["compatibilityHash"],
        "noteInput":notes::schema(false),"noteOutput":notes::schema(true),
        "correctionInput":corrections::schema(false),"correctionOutput":corrections::schema(true),
        "forgetInput":forget_requests::schema(false),"forgetOutput":forget_requests::schema(true)}))
        .expect("closed compatibility metadata"),
    );
    let mut expected = descriptor;
    expected["vaultId"] = json!(vault);
    expected["compatibilityHash"] = json!(hash);
    expected
}

/// Serves one connection until the Agent closes it, `stop` becomes true, or
/// enrollment, reader or any writer stops being live. Between frames it waits
/// without a time limit and rechecks all of these every quarter second; once a
/// frame starts, the rest of it and the reply must finish within `timeout`.
pub fn serve_connection(
    reader: &QualifiedAgentReader<'_>,
    writers: AgentWriters<'_, '_>,
    profile: &AttentionProfile,
    pipe: &mut Pipe,
    enrollment: Enrollment,
    timeout: Duration,
    stop: &AtomicBool,
) -> Result<()> {
    struct Revoke<'a, 'host> {
        reader: &'a QualifiedAgentReader<'host>,
        writers: AgentWriters<'a, 'host>,
    }
    impl Drop for Revoke<'_, '_> {
        fn drop(&mut self) {
            self.reader.revoke();
            self.writers.revoke();
        }
    }
    let _revoke = Revoke { reader, writers };
    let result = (|| {
        // Everything but the pipe PID, which cannot change while a read is
        // pending on the connected pipe; `enrollment.check` covers it per frame.
        let alive = || {
            let ok = !stop.load(Ordering::SeqCst)
                && enrollment.live().is_ok()
                && reader.check_connection().is_ok()
                && writers.check_connection().is_ok();
            if ok {
                enrollment.follow_grant(reader.grant_backed());
            }
            ok
        };
        let live = |pipe: &Pipe| -> Result<()> {
            enrollment.check(pipe)?;
            if !alive() {
                return Err(TransportFault::Incompatible);
            }
            Ok(())
        };
        enrollment.check(pipe)?;
        let source_read = writers.note.is_some()
            || writers.correction.is_some()
            || writers.forget.is_some()
            || reader.source_read();
        if !enrollment.allows_v2_reader(reader.principal(), source_read) {
            return Err(TransportFault::Incompatible);
        }
        let mut server =
            Server::new(reader, writers, profile).ok_or(TransportFault::Incompatible)?;
        live(pipe)?;
        let (tag, bytes) = read(pipe, true, enrollment.timeout(timeout))?;
        enrollment.check(pipe)?;
        let expected = hello(reader.vault_id());
        if tag != Tag::Hello || unique::parse(&bytes).ok().as_ref() != Some(&expected) {
            return Err(TransportFault::Incompatible);
        }
        write(pipe, Tag::Welcome, &expected, enrollment.timeout(timeout))?;
        loop {
            live(pipe)?;
            let (tag, bytes) = read_idle(pipe, true, enrollment.timeout(timeout), &mut || alive())?;
            live(pipe)?;
            if tag != Tag::Call {
                return Err(TransportFault::InvalidFrame);
            }
            let message = unique::parse(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
            let response = server.handle(&message);
            if server.outcome_unknown() {
                return Err(TransportFault::Incompatible);
            }
            write(
                pipe,
                Tag::Reply,
                &json!({"response":response}),
                enrollment.timeout(timeout),
            )?;
            if server.closed() {
                return Ok(());
            }
        }
    })();
    // Returning Err alone leaves this borrowed pipe alive. Uncertainty must EOF.
    let _ = pipe.disconnect();
    result
}

pub struct Client {
    inner: crate::mcp::v2::native::Client,
}
impl Client {
    pub fn connect(
        pipe: Pipe,
        host: ProcessPeer,
        vault: &VaultId,
        timeout: Duration,
    ) -> Result<Self> {
        Ok(Self {
            inner: crate::mcp::v2::native::Client::connect_expected(
                pipe,
                host,
                hello(vault),
                timeout,
            )?,
        })
    }
    pub fn exchange(
        &mut self,
        message: &Value,
    ) -> std::result::Result<Option<Value>, ExchangeFailure> {
        self.inner.exchange(message)
    }
    pub fn original_unknown(&self) -> Option<&Value> {
        self.inner.original_unknown()
    }
    pub fn serve_stdio(
        &mut self,
        input: &mut impl std::io::BufRead,
        output: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        self.inner.serve_stdio_full(input, output)
    }
}

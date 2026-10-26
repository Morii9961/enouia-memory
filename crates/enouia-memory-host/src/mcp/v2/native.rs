//! Protected pipe v2 over the same borrowed automation Host/Agent reader.
use super::{Server, schemas};
use crate::automation::agent_read::{AgentReader, CAPABILITY};
use crate::framing::Tag;
use crate::mcp::{
    PROTOCOL_VERSION, json as unique,
    native::{Enrollment, ExchangeFailure},
};
use crate::transport::{Result, TransportFault, read, write};
use crate::windows::{Pipe, ProcessPeer};
use enouia_memory_contract::{
    hash::sha256, ids::VaultId, json::canonical_bytes, next::AttentionProfile,
};
use serde_json::{Value, json};
use std::time::Duration;
pub fn hello(vault: &VaultId) -> Value {
    hello_for(vault, false)
}
pub fn record_hello(vault: &VaultId) -> Value {
    hello_for(vault, true)
}
fn hello_for(vault: &VaultId, records: bool) -> Value {
    let descriptor = json!({"protocol":"memory-agent-pipe-2","recordSchema":2,"layout":if records {"automation-host-v3"} else {"automation-host-v2"},"capability":if records {crate::automation::agent_read::RECORD_CAPABILITY} else {CAPABILITY},"mcpVersion":PROTOCOL_VERSION,"memoryVersion":2});
    let texts: std::collections::BTreeMap<_, _> = schemas::documents()
        .into_iter()
        .map(|(k, v)| (k, v.replace("\r\n", "\n")))
        .collect();
    let hash = sha256(
        &canonical_bytes(&json!({"descriptor":descriptor,"schemas":texts}))
            .expect("closed compatibility metadata"),
    );
    let mut expected = descriptor;
    expected["vaultId"] = json!(vault);
    expected["compatibilityHash"] = json!(hash);
    expected
}
pub fn serve_connection(
    reader: &AgentReader<'_>,
    profile: &AttentionProfile,
    pipe: &mut Pipe,
    enrollment: Enrollment,
    timeout: Duration,
) -> Result<()> {
    struct Revoke<'a, 'host>(&'a AgentReader<'host>);
    impl Drop for Revoke<'_, '_> {
        fn drop(&mut self) {
            self.0.revoke();
        }
    }
    let _revoke = Revoke(reader);
    enrollment.check(pipe)?;
    if !enrollment.allows_v2_reader(reader.principal(), reader.source_read()) {
        return Err(TransportFault::Incompatible);
    }
    reader
        .check_connection()
        .map_err(|_| TransportFault::Incompatible)?;
    let (tag, bytes) = read(pipe, true, enrollment.timeout(timeout))?;
    enrollment.check(pipe)?;
    let expected = hello_for(reader.vault_id(), reader.records());
    if tag != Tag::Hello || unique::parse(&bytes).ok().as_ref() != Some(&expected) {
        return Err(TransportFault::Incompatible);
    }
    let mut server = Server::new(reader, profile).ok_or(TransportFault::Incompatible)?;
    write(pipe, Tag::Welcome, &expected, enrollment.timeout(timeout))?;
    loop {
        enrollment.check(pipe)?;
        reader
            .check_connection()
            .map_err(|_| TransportFault::Incompatible)?;
        let (tag, bytes) = read(pipe, true, enrollment.timeout(timeout))?;
        enrollment.check(pipe)?;
        reader
            .check_connection()
            .map_err(|_| TransportFault::Incompatible)?;
        if tag != Tag::Call {
            return Err(TransportFault::InvalidFrame);
        }
        let message = unique::parse(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
        let response = server.handle(&message);
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
}
pub struct Client {
    inner: crate::mcp::native::Client,
}
impl Client {
    pub fn connect_records(
        pipe: Pipe,
        host: ProcessPeer,
        vault: &VaultId,
        timeout: Duration,
    ) -> Result<Self> {
        Ok(Self {
            inner: crate::mcp::native::Client::connect_expected(
                pipe,
                host,
                record_hello(vault),
                timeout,
            )?,
        })
    }
    pub fn connect(
        pipe: Pipe,
        host: ProcessPeer,
        vault: &VaultId,
        timeout: Duration,
    ) -> Result<Self> {
        Ok(Self {
            inner: crate::mcp::native::Client::connect_expected(pipe, host, hello(vault), timeout)?,
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
}

impl Client {
    /// Standard MCP stdio edge for an explicitly enrolled existing V2 Host.
    /// IO uncertainty is a protocol/transport error, not a fabricated tool result.
    pub fn serve_stdio(
        &mut self,
        input: &mut impl std::io::BufRead,
        output: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        let result = self.stdio_inner(input, output);
        self.inner.close();
        result
    }
    fn stdio_inner(
        &mut self,
        input: &mut impl std::io::BufRead,
        output: &mut impl std::io::Write,
    ) -> std::io::Result<()> {
        use std::io::{self, BufRead, Read};
        loop {
            let mut line = Vec::new();
            let n = (&mut *input)
                .take((crate::mcp::MAX_INPUT_BYTES + 2) as u64)
                .read_until(b'\n', &mut line)?;
            if n == 0 {
                return Ok(());
            }
            if line.last() != Some(&b'\n') || line.len() > crate::mcp::MAX_INPUT_BYTES + 1 {
                return Err(io::ErrorKind::InvalidData.into());
            }
            line.pop();
            let message =
                unique::parse(&line).map_err(|_| io::Error::from(io::ErrorKind::InvalidData))?;
            let (response, close) = match self.exchange(&message) {
                Ok(v) => (v, false),
                Err(e) => {
                    let original = e
                        .original_message
                        .get("params")
                        .and_then(|p| p.get("arguments"))
                        .unwrap_or(&e.original_message);
                    (message.get("id").map(|id|json!({"jsonrpc":"2.0","id":id,"error":{"code":-32000,
                    "message":if e.fault==crate::mcp::native::ExchangeFault::OutcomeUnknown{"outcome_unknown"}else{"connection_closed"},
                    "data":{"retryable":false,"originalRequest":original}}})),true)
                }
            };
            if let Some(response) = response {
                let bytes = response.to_string();
                if bytes.len() > crate::mcp::MAX_OUTPUT_BYTES {
                    return Err(io::ErrorKind::InvalidData.into());
                }
                let written = (|| {
                    output.write_all(bytes.as_bytes())?;
                    output.write_all(b"\n")?;
                    output.flush()
                })();
                if written.is_err() {
                    self.inner.retain_output_unknown(&message);
                }
                written?;
            }
            if close {
                return Ok(());
            }
        }
    }
}

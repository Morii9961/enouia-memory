//! Protected local segmented Agent transport; one existing Host/Vault.
use super::{Server, schemas};
use crate::automation::segmented_agent::{CAPABILITY, SegmentedAgentReader};
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
    let descriptor = json!({"protocol":"memory-agent-pipe-3","recordSchema":2,"layout":"automation-segmented-host-v1","capability":CAPABILITY,"mcpVersion":PROTOCOL_VERSION,"memoryVersion":2});
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
    reader: &SegmentedAgentReader<'_>,
    profile: &AttentionProfile,
    pipe: &mut Pipe,
    enrollment: Enrollment,
    timeout: Duration,
) -> Result<()> {
    struct Revoke<'a, 'host>(&'a SegmentedAgentReader<'host>);
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
    let expected = hello(reader.vault_id());
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
        self.inner.serve_stdio(input, output)
    }
}

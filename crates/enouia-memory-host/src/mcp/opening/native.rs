//! Explicit read-only opening/resources profile. No capture or write authority.
use super::Server;
use crate::{
    automation::qualified_agent::QualifiedAgentReader,
    framing::Tag,
    mcp::{
        PROTOCOL_VERSION, json as unique,
        native::{Enrollment, ExchangeFailure},
    },
    transport::{Result, TransportFault, read, write},
    windows::{Pipe, ProcessPeer},
};
use enouia_memory_contract::{
    hash::sha256, ids::VaultId, json::canonical_bytes, next::AttentionProfile,
};
use serde_json::{Value, json};
use std::time::Duration;

pub fn hello(vault: &VaultId) -> Value {
    let descriptor = json!({"protocol":"memory-agent-pipe-10","recordSchema":2,"layout":"automation-segmented-owner-v1","capability":"memory-agent-initialized-context-1","readCapability":super::super::initialized::CAPABILITY,"contextCapability":super::CAPABILITY,"nativeReadCapability":"memory-agent-initialized-native-read-1","sourceCarrier":"captured-source-import-1","mcpVersion":PROTOCOL_VERSION,"memoryVersion":2});
    let read_hello = super::super::initialized::native::hello(vault);
    let mut descriptor = descriptor;
    descriptor["sessionCapability"] = json!(super::SESSION_CAPABILITY);
    let responses: std::collections::BTreeMap<_, _> = [
        "resources/list",
        "resources/templates/list",
        "resources/read",
        "prompts/list",
        "prompts/get",
    ]
    .into_iter()
    .map(|method| {
        (
            method,
            super::response_schema(method).expect("frozen response schema"),
        )
    })
    .collect();
    let hash = sha256(
        &canonical_bytes(&json!({"descriptor":descriptor,
            "readCompatibility":read_hello["compatibilityHash"],
            "startupInput":super::schema(true,false),"startupOutput":super::schema(true,true),
            "resourceInput":super::schema(false,false),"resourceOutput":super::schema(false,true),
            "guide":super::guide(),"resources":super::resources(),"templates":super::templates(),
            "prompts":super::prompts(),"preamble":super::PREAMBLE,
            "responses":responses,"sessionStartTool":super::session_tool()
        }))
        .expect("closed compatibility metadata"),
    );
    let mut expected = descriptor;
    expected["vaultId"] = json!(vault);
    expected["compatibilityHash"] = json!(hash);
    expected
}

pub fn serve_connection(
    reader: &QualifiedAgentReader<'_>,
    profile: &AttentionProfile,
    pipe: &mut Pipe,
    enrollment: Enrollment,
    timeout: Duration,
) -> Result<()> {
    struct Revoke<'a, 'host> {
        reader: &'a QualifiedAgentReader<'host>,
    }
    impl Drop for Revoke<'_, '_> {
        fn drop(&mut self) {
            self.reader.revoke();
        }
    }
    let _revoke = Revoke { reader };
    let result = (|| {
        enrollment.check(pipe)?;
        if !enrollment.allows_v2_reader(reader.principal(), reader.source_read()) {
            return Err(TransportFault::Incompatible);
        }
        let mut server = Server::new(reader, profile).ok_or(TransportFault::Incompatible)?;
        let (tag, bytes) = read(pipe, true, enrollment.timeout(timeout))?;
        enrollment.check(pipe)?;
        let expected = hello(reader.vault_id());
        if tag != Tag::Hello || unique::parse(&bytes).ok().as_ref() != Some(&expected) {
            return Err(TransportFault::Incompatible);
        }
        write(pipe, Tag::Welcome, &expected, enrollment.timeout(timeout))?;
        loop {
            enrollment.check(pipe)?;
            reader
                .check_context_connection()
                .map_err(|_| TransportFault::Incompatible)?;
            let (tag, bytes) = read(pipe, true, enrollment.timeout(timeout))?;
            enrollment.check(pipe)?;
            reader
                .check_context_connection()
                .map_err(|_| TransportFault::Incompatible)?;
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

//! Native authenticated-process pipe adapter. Every native control is framed
//! separately from the unchanged page envelope; peers cannot choose a scope.
use crate::NativeAdmission;
use crate::framing::{Tag, encode};
use crate::service::{HostService, ServiceFault};
use crate::windows::{Pipe, ProcessPeer};
use enouia_memory_contract::host::{
    parse_document,
    session::{ClientHello, ServerHello},
};
use enouia_memory_contract::workspace::{self, Empty, HostSurface};
use serde_json::{Value, json};
use std::io;
use std::time::{Duration, Instant};

#[derive(Debug)]
pub enum TransportFault {
    Io(io::ErrorKind),
    InvalidFrame,
    Incompatible,
    Service(ServiceFault),
}
pub type Result<T> = std::result::Result<T, TransportFault>;
impl From<io::Error> for TransportFault {
    fn from(e: io::Error) -> Self {
        Self::Io(e.kind())
    }
}
impl From<ServiceFault> for TransportFault {
    fn from(e: ServiceFault) -> Self {
        Self::Service(e)
    }
}
pub(crate) fn read(pipe: &mut Pipe, client: bool, timeout: Duration) -> Result<(Tag, Vec<u8>)> {
    let deadline = Instant::now() + timeout;
    let mut header = [0; 5];
    pipe.read_exact(&mut header, deadline)?;
    let tag = Tag::decode(header[0], client)?;
    let len = u32::from_le_bytes(header[1..].try_into().expect("four-byte frame size")) as usize;
    if len == 0 || len > tag.limit() {
        return Err(TransportFault::InvalidFrame);
    }
    let mut body = vec![0; len];
    pipe.read_exact(&mut body, deadline)?;
    Ok((tag, body))
}
pub(crate) fn write(pipe: &mut Pipe, tag: Tag, value: &Value, timeout: Duration) -> Result<()> {
    let body = encode(value, tag.limit())?;
    let mut header = [tag as u8, 0, 0, 0, 0];
    header[1..].copy_from_slice(&(body.len() as u32).to_le_bytes());
    let deadline = Instant::now() + timeout;
    pipe.write_all(&header, deadline)?;
    pipe.write_all(&body, deadline)?;
    Ok(())
}
fn empty(bytes: &[u8]) -> Result<()> {
    serde_json::from_slice::<Empty>(bytes).map_err(|_| TransportFault::InvalidFrame)?;
    Ok(())
}
fn admission(host: &HostService, peer: &ProcessPeer) -> Result<NativeAdmission> {
    NativeAdmission::from_authenticated_owner(host.native_owner(), peer.surface)
        .map_err(|e| TransportFault::Service(ServiceFault::Session(e)))
}

/// Caller accepted the kernel connection. Verify the enrolled live process
/// before reading a handshake, and again before every Core/control dispatch.
/// EOF/error drops the opaque connection. No request, model call or response
/// is retried, and no embedded writer is opened. Reuse the listening pipe only
/// after disconnecting this instance.
pub fn serve_connection(
    host: &HostService,
    pipe: &mut Pipe,
    peer: &ProcessPeer,
    timeout: Duration,
) -> Result<()> {
    peer.verify_client(pipe)?;
    let mut connection = None;
    loop {
        peer.verify_client(pipe)?;
        let (tag, bytes) = read(pipe, true, timeout)?;
        peer.verify_client(pipe)?;
        match tag {
            Tag::Hello => {
                let hello: ClientHello =
                    parse_document(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
                // Drop any old handle before fresh native admission/handshake.
                drop(connection.take());
                let (welcome, fresh) = host.connect(admission(host, peer)?, &hello)?;
                write(
                    pipe,
                    Tag::Welcome,
                    &serde_json::to_value(welcome).map_err(|_| TransportFault::InvalidFrame)?,
                    timeout,
                )?;
                connection = Some(fresh);
            }
            Tag::Call => {
                let current = connection.as_ref().ok_or(TransportFault::InvalidFrame)?;
                let request: Value =
                    serde_json::from_slice(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
                let response = host.call(current, &request)?;
                write(pipe, Tag::Reply, &response, timeout)?;
            }
            Tag::Lock | Tag::Unlock => {
                let hello: ClientHello =
                    parse_document(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
                host.validate_native_hello(&hello)?;
                if peer.surface != HostSurface::Workspace {
                    return Err(TransportFault::InvalidFrame);
                }
                connection = None;
                if tag == Tag::Lock {
                    let reply = host.lock(admission(host, peer)?)?;
                    if !reply["error"].is_null() {
                        return Err(TransportFault::InvalidFrame);
                    }
                } else {
                    host.unlock(admission(host, peer)?)?;
                }
                write(pipe, Tag::Ack, &json!({}), timeout)?;
            }
            _ => return Err(TransportFault::InvalidFrame),
        }
    }
}

/// One native broker connection. The native owner supplies the enrolled live
/// Host process and expected surface, never a page-provided PID or pipe name.
pub struct Client {
    pipe: Pipe,
    host: ProcessPeer,
    hello: ClientHello,
    timeout: Duration,
    connected: bool,
    poisoned: bool,
}
impl Client {
    pub fn connect(
        pipe: Pipe,
        host: ProcessPeer,
        hello: ClientHello,
        timeout: Duration,
    ) -> Result<Self> {
        let mut client = Self::open(pipe, host, hello, timeout)?;
        client.handshake()?;
        Ok(client)
    }
    /// Native owner may open a verified channel while locked, then explicitly
    /// unlock with a versioned control handshake. Ordinary calls stay disabled.
    pub fn open(
        pipe: Pipe,
        host: ProcessPeer,
        hello: ClientHello,
        timeout: Duration,
    ) -> Result<Self> {
        host.verify_server(&pipe)?;
        Ok(Self {
            pipe,
            host,
            hello,
            timeout,
            connected: false,
            poisoned: false,
        })
    }
    pub fn handshake(&mut self) -> Result<ServerHello> {
        if self.poisoned {
            return Err(TransportFault::InvalidFrame);
        }
        self.poisoned = true;
        self.connected = false;
        self.host.verify_server(&self.pipe)?;
        write(
            &mut self.pipe,
            Tag::Hello,
            &serde_json::to_value(&self.hello).map_err(|_| TransportFault::InvalidFrame)?,
            self.timeout,
        )?;
        let (tag, bytes) = read(&mut self.pipe, false, self.timeout)?;
        self.host.verify_server(&self.pipe)?;
        if tag != Tag::Welcome {
            return Err(TransportFault::InvalidFrame);
        }
        let welcome: ServerHello =
            parse_document(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
        if welcome.vault_id != self.hello.vault_id
            || welcome.compatibility_hash != self.hello.compatibility_hash
            || welcome.protocol_version != self.hello.protocol_version
            || welcome.workspace_version != self.hello.workspace_version
            || welcome.surface != self.host.surface
        {
            return Err(TransportFault::Incompatible);
        }
        self.connected = true;
        self.poisoned = false;
        Ok(welcome)
    }
    pub fn call(&mut self, request: &Value) -> Result<Value> {
        if self.poisoned || !self.connected || !self.host.surface.allows(request) {
            return Err(TransportFault::InvalidFrame);
        }
        let (parsed, _) =
            workspace::parse_request(request).map_err(|_| TransportFault::InvalidFrame)?;
        self.connected = false;
        self.poisoned = true;
        self.host.verify_server(&self.pipe)?;
        write(&mut self.pipe, Tag::Call, request, self.timeout)?;
        let (tag, bytes) = read(&mut self.pipe, false, self.timeout)?;
        self.host.verify_server(&self.pipe)?;
        if tag != Tag::Reply {
            self.connected = false;
            return Err(TransportFault::InvalidFrame);
        }
        let value: Value =
            serde_json::from_slice(&bytes).map_err(|_| TransportFault::InvalidFrame)?;
        workspace::validate_response(&parsed.command, &value)
            .map_err(|_| TransportFault::InvalidFrame)?;
        if value["requestId"] != json!(parsed.request_id) {
            self.connected = false;
            return Err(TransportFault::InvalidFrame);
        }
        self.connected = true;
        self.poisoned = false;
        Ok(value)
    }
    fn control(&mut self, tag: Tag) -> Result<()> {
        if self.poisoned || self.host.surface != HostSurface::Workspace {
            return Err(TransportFault::InvalidFrame);
        }
        self.connected = false;
        self.poisoned = true;
        self.host.verify_server(&self.pipe)?;
        write(
            &mut self.pipe,
            tag,
            &serde_json::to_value(&self.hello).map_err(|_| TransportFault::InvalidFrame)?,
            self.timeout,
        )?;
        let (reply, bytes) = read(&mut self.pipe, false, self.timeout)?;
        self.host.verify_server(&self.pipe)?;
        if reply != Tag::Ack {
            return Err(TransportFault::InvalidFrame);
        }
        empty(&bytes)?;
        self.poisoned = false;
        Ok(())
    }
    pub fn lock(&mut self) -> Result<()> {
        self.control(Tag::Lock)
    }
    pub fn unlock(&mut self) -> Result<ServerHello> {
        self.control(Tag::Unlock)?;
        self.handshake()
    }
}

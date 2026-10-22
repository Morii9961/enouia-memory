//! Native shell boundary for queued work and explicit reconnection.
//! Capture a ticket before queuing a worker. The shell serializes access to
//! this broker and injects the real window surface, never a page claim.
use crate::framing::{Tag, encode};
use crate::transport::{Client, TransportFault};
use crate::windows::{Pipe, ProcessPeer};
use enouia_memory_contract::host::session::{ClientHello, ServerHello};
use enouia_memory_contract::workspace::{self, HostSurface};
use serde_json::Value;
use std::sync::Arc;
use std::time::Duration;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum State {
    Disconnected,
    AwaitingHandshake,
    Ready,
    Locked,
}

/// Native-only queue ticket. This is not authentication or a page capability.
#[derive(Clone)]
pub struct Ticket(Arc<()>);

#[derive(Debug)]
pub enum Fault {
    Stale,
    NotReady,
    ScopeDenied,
    InvalidPacket,
    Transport(TransportFault),
    /// No validated reply arrived. The Core outcome may already be durable.
    /// No request is retained or retried; reconnect and inspect/replay the
    /// original key only through a separate, explicit caller action.
    UncertainCall(TransportFault),
}
pub type Result<T> = std::result::Result<T, Fault>;

pub struct Broker {
    surface: HostSurface,
    client: Option<Client>,
    state: State,
    ticket: Ticket,
}
impl Broker {
    pub fn new(surface: HostSurface) -> Self {
        Self {
            surface,
            client: None,
            state: State::Disconnected,
            ticket: Ticket(Arc::new(())),
        }
    }
    /// Local lifecycle observation only; it does not probe health or expiry.
    pub fn state(&self) -> State {
        self.state
    }
    pub fn ticket(&self) -> Ticket {
        self.ticket.clone()
    }
    fn revoke(&mut self) {
        self.ticket = Ticket(Arc::new(()));
    }
    fn check(&self, ticket: &Ticket) -> Result<()> {
        if !Arc::ptr_eq(&ticket.0, &self.ticket.0) {
            return Err(Fault::Stale);
        }
        Ok(())
    }
    pub fn disconnect(&mut self) {
        self.revoke();
        self.client = None;
        self.state = State::Disconnected;
    }
    /// Explicit native endpoint selection/enrollment. No discovery, spawn,
    /// connection retry, handshake, unlock or application send occurs here.
    /// Replacing even a failed connection invalidates all earlier queue work.
    pub fn open(
        &mut self,
        pipe: Pipe,
        host: ProcessPeer,
        hello: ClientHello,
        timeout: Duration,
    ) -> Result<()> {
        self.disconnect();
        if host.surface != self.surface {
            return Err(Fault::ScopeDenied);
        }
        self.client = Some(Client::open(pipe, host, hello, timeout).map_err(Fault::Transport)?);
        self.state = State::AwaitingHandshake;
        Ok(())
    }
    pub fn handshake(&mut self, ticket: &Ticket) -> Result<ServerHello> {
        self.check(ticket)?;
        if !matches!(self.state, State::AwaitingHandshake | State::Ready) {
            return Err(Fault::NotReady);
        }
        self.revoke();
        match self.client.as_mut().ok_or(Fault::NotReady)?.handshake() {
            Ok(welcome) => {
                self.state = State::Ready;
                Ok(welcome)
            }
            Err(e) => {
                self.disconnect();
                Err(Fault::Transport(e))
            }
        }
    }
    /// All page packets use only the fixed Call frame. `window` is injected
    /// by the native shell when it accepts work, independently of the packet.
    pub fn call(&mut self, ticket: &Ticket, window: HostSurface, request: &Value) -> Result<Value> {
        self.check(ticket)?;
        if !self.surface.allows(request) || !window.allows(request) {
            return Err(Fault::ScopeDenied);
        }
        let (_, command) = workspace::parse_request(request).map_err(|_| Fault::InvalidPacket)?;
        if matches!(
            command,
            workspace::Command::VaultOpen(_)
                | workspace::Command::VaultCreate(_)
                | workspace::Command::VaultUnlock(_)
        ) {
            return Err(Fault::ScopeDenied);
        }
        // Refuse an over-budget envelope before any I/O, including JSON escape
        // expansion. Transport failures after this point remain conservative.
        encode(request, Tag::Call.limit()).map_err(|_| Fault::InvalidPacket)?;
        if self.state != State::Ready {
            return Err(Fault::NotReady);
        }
        let reply = self.client.as_mut().ok_or(Fault::NotReady)?.call(request);
        match reply {
            Ok(reply) => {
                if matches!(command, workspace::Command::VaultLock(_)) {
                    // The Host revokes before Core lock, even if Core returns
                    // an error envelope. Never revive a queued old request.
                    self.revoke();
                    self.state = State::Locked;
                }
                Ok(reply)
            }
            Err(e) => {
                self.disconnect();
                Err(Fault::UncertainCall(e))
            }
        }
    }
    fn owner_control(&self, ticket: &Ticket, window: HostSurface) -> Result<()> {
        self.check(ticket)?;
        if self.surface != HostSurface::Workspace || window != HostSurface::Workspace {
            return Err(Fault::ScopeDenied);
        }
        if self.state == State::Disconnected {
            return Err(Fault::NotReady);
        }
        Ok(())
    }
    /// Explicit native action; never select this operation from a page tag.
    pub fn lock(&mut self, ticket: &Ticket, window: HostSurface) -> Result<()> {
        self.owner_control(ticket, window)?;
        self.revoke();
        match self.client.as_mut().ok_or(Fault::NotReady)?.lock() {
            Ok(()) => {
                self.state = State::Locked;
                Ok(())
            }
            Err(e) => {
                self.disconnect();
                Err(Fault::Transport(e))
            }
        }
    }
    /// Explicit native unlock then fresh handshake; old tickets stay revoked.
    pub fn unlock(&mut self, ticket: &Ticket, window: HostSurface) -> Result<ServerHello> {
        self.owner_control(ticket, window)?;
        self.revoke();
        match self.client.as_mut().ok_or(Fault::NotReady)?.unlock() {
            Ok(welcome) => {
                self.state = State::Ready;
                Ok(welcome)
            }
            Err(e) => {
                self.disconnect();
                Err(Fault::Transport(e))
            }
        }
    }
}

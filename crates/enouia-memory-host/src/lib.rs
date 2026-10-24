//! Native single-Host service, session gate and opt-in Windows pipe adapters.
//! No automatic spawn, adoption or embedded fallback.

#[cfg(windows)]
pub mod broker;
mod compatibility;
mod framing;
pub use compatibility::compatibility_hash;
pub mod mcp;
pub mod service;
#[cfg(windows)]
pub mod transport;
#[cfg(windows)]
pub mod windows;

use enouia_memory_contract::common::{ActorRef, ActorType};
use enouia_memory_contract::foundation::Clock;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::host::PrimaryBinding;
use enouia_memory_contract::host::session::{
    ClientHello, HOST_PROTOCOL_VERSION, ServerHello, WORKSPACE_PROTOCOL_VERSION,
};
use enouia_memory_contract::ids::{PrincipalId, VaultId};
use enouia_memory_contract::json::SchemaVersion;
use enouia_memory_contract::ports::IdSource;
use enouia_memory_contract::store::StoreDocument;
use enouia_memory_contract::time::Timestamp;
use enouia_memory_contract::workspace::{self, HostSurface};
use serde_json::Value;
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex, MutexGuard, Weak};

pub const SESSION_TTL_MS: i64 = 5 * 60 * 1000;
pub const MAX_CONNECTIONS: usize = 64;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SessionFault {
    Unavailable,
    Locked,
    Unauthenticated,
    Incompatible,
    WrongVault,
    Revoked,
    Expired,
    ScopeDenied,
    InvalidPacket,
    Capacity,
    ClockRegression,
    InvalidTime,
}
pub type Result<T> = std::result::Result<T, SessionFault>;

/// Native admission supplied only AFTER the transport authenticates and
/// authorizes the owner's process/window. This constructor performs no OS
/// authentication. Never derive its principal/surface from handshake or page
/// packets, executable names alone, or untrusted imported text.
pub struct NativeAdmission {
    owner: PrincipalId,
    surface: HostSurface,
}
impl NativeAdmission {
    pub fn from_authenticated_owner(owner: &ActorRef, surface: HostSurface) -> Result<Self> {
        if owner.actor_type != ActorType::Owner {
            return Err(SessionFault::Unauthenticated);
        }
        Ok(Self {
            owner: owner.actor_id.clone(),
            surface,
        })
    }
}

struct Entry {
    surface: Option<HostSurface>,
    expires_ms: i64,
    lifetime: Weak<()>,
}
struct State {
    entries: BTreeMap<u64, Entry>,
    next: u64,
    locked: bool,
    closed: bool,
    last_ms: Option<i64>,
}
impl State {
    fn time(&mut self, now: i64) -> Result<()> {
        if self.last_ms.is_some_and(|last| now < last) {
            self.entries.clear();
            self.locked = true;
            return Err(SessionFault::ClockRegression);
        }
        if Timestamp::from_unix_ms(now).is_none() {
            self.entries.clear();
            self.locked = true;
            return Err(SessionFault::InvalidTime);
        }
        self.last_ms = Some(now);
        Ok(())
    }
    fn available(&self) -> Result<()> {
        if self.closed {
            Err(SessionFault::Unavailable)
        } else if self.locked {
            Err(SessionFault::Locked)
        } else {
            Ok(())
        }
    }
}
struct Inner {
    state: Mutex<State>,
}
impl Inner {
    fn lock(&self) -> Result<MutexGuard<'_, State>> {
        self.state.lock().map_err(|_| SessionFault::Unavailable)
    }
}

/// Connection-bound native handle. No serialization, raw token or Clone.
/// A transport owns one handle per live authenticated connection. Dropping it
/// disconnects; reconnect always requires fresh native admission/handshake.
pub struct Connection {
    inner: Weak<Inner>,
    serial: u64,
    _lifetime: Arc<()>,
}

/// Serializes admission, synchronous dispatch and lifecycle revocation.
/// The process still owns the Vault Host lease and authenticates native peers.
pub struct SessionGate {
    inner: Arc<Inner>,
    vault: VaultId,
    owner: PrincipalId,
    primary: PrimaryBinding,
    instance: Sha256Hex,
    compatibility: Sha256Hex,
    clock: Arc<dyn Clock + Send + Sync>,
}
impl SessionGate {
    pub fn new(
        vault: VaultId,
        owner: PrincipalId,
        primary: PrimaryBinding,
        clock: Arc<dyn Clock + Send + Sync>,
        ids: &dyn IdSource,
    ) -> Self {
        let mut nonce = b"enouia-host-instance-v1".to_vec();
        nonce.extend_from_slice(&ids.random_16());
        nonce.extend_from_slice(&ids.random_16());
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(State {
                    entries: BTreeMap::new(),
                    next: 1,
                    locked: false,
                    closed: false,
                    last_ms: None,
                }),
            }),
            vault,
            owner,
            primary,
            instance: sha256(&nonce),
            compatibility: compatibility_hash(),
            clock,
        }
    }

    pub fn connect(
        &self,
        native: NativeAdmission,
        hello: &ClientHello,
    ) -> Result<(ServerHello, Connection)> {
        let mut state = self.inner.lock()?;
        state.available()?;
        let now = self.clock.now_unix_ms();
        state.time(now)?;
        if native.owner != self.owner {
            return Err(SessionFault::Unauthenticated);
        }
        self.validate_hello(hello)?;
        let expires_ms = now
            .checked_add(SESSION_TTL_MS)
            .ok_or(SessionFault::InvalidTime)?;
        let expires_at = Timestamp::from_unix_ms(expires_ms).ok_or(SessionFault::InvalidTime)?;
        state
            .entries
            .retain(|_, e| e.expires_ms > now && e.lifetime.strong_count() > 0);
        if state.entries.len() >= MAX_CONNECTIONS {
            return Err(SessionFault::Capacity);
        }
        let serial = state.next;
        state.next = serial.checked_add(1).ok_or(SessionFault::Capacity)?;
        let lifetime = Arc::new(());
        state.entries.insert(
            serial,
            Entry {
                surface: Some(native.surface),
                expires_ms,
                lifetime: Arc::downgrade(&lifetime),
            },
        );
        Ok((
            ServerHello {
                schema_version: SchemaVersion,
                protocol_version: HOST_PROTOCOL_VERSION,
                workspace_version: WORKSPACE_PROTOCOL_VERSION,
                vault_id: self.vault.clone(),
                compatibility_hash: self.compatibility.clone(),
                instance_id: self.instance.clone(),
                primary: self.primary.clone(),
                surface: native.surface,
                expires_at,
            },
            Connection {
                inner: Arc::downgrade(&self.inner),
                serial,
                _lifetime: lifetime,
            },
        ))
    }

    /// Trusted in-process Agent enrollment only; never reachable from a window
    /// hello or a page packet. The embedding layer authenticates separately.
    pub(crate) fn connect_agent(&self) -> Result<Connection> {
        let mut state = self.inner.lock()?;
        state.available()?;
        let now = self.clock.now_unix_ms();
        state.time(now)?;
        let expires_ms = now
            .checked_add(SESSION_TTL_MS)
            .ok_or(SessionFault::InvalidTime)?;
        Timestamp::from_unix_ms(expires_ms).ok_or(SessionFault::InvalidTime)?;
        state
            .entries
            .retain(|_, e| e.expires_ms > now && e.lifetime.strong_count() > 0);
        if state.entries.len() >= MAX_CONNECTIONS {
            return Err(SessionFault::Capacity);
        }
        let serial = state.next;
        state.next = serial.checked_add(1).ok_or(SessionFault::Capacity)?;
        let lifetime = Arc::new(());
        state.entries.insert(
            serial,
            Entry {
                surface: None,
                expires_ms,
                lifetime: Arc::downgrade(&lifetime),
            },
        );
        Ok(Connection {
            inner: Arc::downgrade(&self.inner),
            serial,
            _lifetime: lifetime,
        })
    }

    pub(crate) fn dispatch_agent<T>(
        &self,
        connection: &Connection,
        run: impl FnOnce() -> T,
    ) -> Result<T> {
        let mut state = self.inner.lock()?;
        state.available()?;
        let now = self.clock.now_unix_ms();
        state.time(now)?;
        if !Weak::ptr_eq(&connection.inner, &Arc::downgrade(&self.inner)) {
            return Err(SessionFault::Revoked);
        }
        let entry = state
            .entries
            .get(&connection.serial)
            .ok_or(SessionFault::Revoked)?;
        if now >= entry.expires_ms {
            state.entries.remove(&connection.serial);
            return Err(SessionFault::Expired);
        }
        if entry.surface.is_some() {
            return Err(SessionFault::ScopeDenied);
        }
        Ok(run())
    }

    pub(crate) fn validate_hello(&self, hello: &ClientHello) -> Result<()> {
        if !hello.validate().is_empty() || hello.compatibility_hash != self.compatibility {
            return Err(SessionFault::Incompatible);
        }
        if hello.vault_id != self.vault {
            return Err(SessionFault::WrongVault);
        }
        Ok(())
    }

    /// Closure MUST perform fresh native Host authority preflight and then
    /// scoped Core dispatch while this lifecycle gate is held. Never call back
    /// into SessionGate from the closure. Asynchronous
    /// work still needs Core's cancellation/lock checks; this is not a grant
    /// for later work. Errors and panics never fall back to an embedded writer.
    pub fn dispatch<T>(
        &self,
        connection: &Connection,
        request: &Value,
        run: impl FnOnce(HostSurface) -> T,
    ) -> Result<T> {
        let mut state = self.inner.lock()?;
        state.available()?;
        let now = self.clock.now_unix_ms();
        state.time(now)?;
        if !Weak::ptr_eq(&connection.inner, &Arc::downgrade(&self.inner)) {
            return Err(SessionFault::Revoked);
        }
        let entry = state
            .entries
            .get(&connection.serial)
            .ok_or(SessionFault::Revoked)?;
        if now >= entry.expires_ms {
            state.entries.remove(&connection.serial);
            return Err(SessionFault::Expired);
        }
        let surface = entry.surface.ok_or(SessionFault::ScopeDenied)?;
        if !surface.allows(request) {
            return Err(SessionFault::ScopeDenied);
        }
        let (_, command) =
            workspace::parse_request(request).map_err(|_| SessionFault::InvalidPacket)?;
        if matches!(command, workspace::Command::VaultLock(_)) {
            // Fail closed even if the Core's lock action subsequently fails.
            state.entries.clear();
            state.locked = true;
        }
        Ok(run(surface))
    }

    /// Returns only after admitted synchronous dispatch has left the gate.
    pub fn revoke_all(&self) -> Result<()> {
        self.inner.lock()?.entries.clear();
        Ok(())
    }
    pub fn lock(&self) -> Result<()> {
        let mut state = self.inner.lock()?;
        state.entries.clear();
        state.locked = true;
        Ok(())
    }
    /// Trusted native unlock only; old connections never revive. This does not
    /// unlock the Workspace or authenticate the owner by itself.
    pub fn unlock(&self) -> Result<()> {
        let mut state = self.inner.lock()?;
        if state.closed {
            return Err(SessionFault::Unavailable);
        }
        state.entries.clear();
        state.locked = false;
        state.last_ms = None;
        Ok(())
    }
}
impl Drop for SessionGate {
    fn drop(&mut self) {
        if let Ok(mut state) = self.inner.state.lock() {
            state.closed = true;
            state.entries.clear();
        }
    }
}

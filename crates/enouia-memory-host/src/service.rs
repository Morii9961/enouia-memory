//! Native Host-bound Workspace service; a listener authenticates peers separately.
use crate::{Connection, NativeAdmission, SessionFault, SessionGate};
use enouia_memory_contract::MemoryError;
use enouia_memory_contract::host::session::{ClientHello, ServerHello};
use enouia_memory_contract::ids::{DeviceId, RequestId};
use enouia_memory_contract::workspace::{HostSurface, WorkspaceError};
use enouia_memory_vault::{Vault, VaultError, VaultOptions, VerifiedRoot};
use enouia_memory_workspace::{Config, Workspace};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

#[derive(Debug)]
pub enum ServiceFault {
    Session(SessionFault),
    Core(WorkspaceError),
    Vault(MemoryError),
    NativeControlRequired,
}
type Result<T> = std::result::Result<T, ServiceFault>;
impl From<SessionFault> for ServiceFault {
    fn from(e: SessionFault) -> Self {
        Self::Session(e)
    }
}
impl From<WorkspaceError> for ServiceFault {
    fn from(e: WorkspaceError) -> Self {
        Self::Core(e)
    }
}
impl From<VaultError> for ServiceFault {
    fn from(e: VaultError) -> Self {
        Self::Vault(e.error)
    }
}

/// Owns the native Vault lease across client disconnects and UI locks.
/// Opening requires an existing v2 root; no bootstrap, provider activation,
/// automatic process launch, peer authentication or embedded fallback.
pub struct HostService {
    core: Workspace,
    vault: Arc<Vault>,
    gate: SessionGate,
    root: PathBuf,
    control: Mutex<()>,
}
impl HostService {
    #[cfg(windows)]
    pub(crate) fn native_owner(&self) -> &enouia_memory_contract::common::ActorRef {
        &self.vault.descriptor().created_by
    }
    #[cfg(windows)]
    pub(crate) fn validate_native_hello(&self, hello: &ClientHello) -> Result<()> {
        self.vault.host_writer_state()?;
        self.gate.validate_hello(hello)?;
        Ok(())
    }
    pub fn open(root: &VerifiedRoot, identity: DeviceId, config: Config) -> Result<Self> {
        let vault = Arc::new(
            Vault::open(
                root,
                Some(identity.clone()),
                config.clock.clone(),
                config.ids.clone(),
                VaultOptions::default(),
            )?
            .into_host_writer(root, identity)?,
        );
        let state = vault.host_writer_state()?;
        let gate = SessionGate::new(
            state.vault_id,
            vault.descriptor().created_by.actor_id.clone(),
            state.primary,
            config.clock.clone(),
            config.ids.as_ref(),
        );
        let core = Workspace::from_host_vault(config, vault.clone())?;
        Ok(Self {
            core,
            vault,
            gate,
            root: root.path().to_path_buf(),
            control: Mutex::new(()),
        })
    }

    pub fn connect(
        &self,
        native: NativeAdmission,
        hello: &ClientHello,
    ) -> Result<(ServerHello, Connection)> {
        self.vault.host_writer_state()?;
        Ok(self.gate.connect(native, hello)?)
    }

    pub fn call(&self, connection: &Connection, request: &Value) -> Result<Value> {
        self.gate.dispatch(connection, request, |_| {
            self.vault.host_writer_state()?;
            if matches!(
                request.get("command").and_then(Value::as_str),
                Some("vault_open" | "vault_create" | "vault_unlock")
            ) {
                return Err(ServiceFault::NativeControlRequired);
            }
            Ok(self.core.call(request))
        })?
    }

    pub(crate) fn admit_agent(&self) -> Result<Connection> {
        self.vault.host_writer_state()?;
        Ok(self.gate.connect_agent()?)
    }

    pub(crate) fn dispatch_agent<T>(
        &self,
        connection: &Connection,
        run: impl FnOnce(&Vault) -> T,
    ) -> Result<T> {
        self.gate.dispatch_agent(connection, || {
            self.vault.host_writer_state()?;
            Ok(run(&self.vault))
        })?
    }

    fn owner_control(&self, native: &NativeAdmission) -> Result<()> {
        if native.owner != self.vault.descriptor().created_by.actor_id
            || native.surface != HostSurface::Workspace
        {
            return Err(SessionFault::Unauthenticated.into());
        }
        Ok(())
    }

    /// Trusted native owner action. Revocation precedes Core cancellation/join.
    pub fn lock(&self, native: NativeAdmission) -> Result<Value> {
        self.owner_control(&native)?;
        let _control = self.control.lock().map_err(|_| SessionFault::Unavailable)?;
        self.gate.lock()?;
        Ok(self.core.call(&json!({
            "schemaVersion": 1, "requestId": RequestId::from_random(self.vault.random_id_bytes()),
            "command": "vault_lock", "idempotencyKey": null, "arguments": {},
        })))
    }

    /// Trusted native owner action; reopen only the retained root/Host handle.
    /// Any failure leaves the session gate locked and old connections revoked.
    pub fn unlock(&self, native: NativeAdmission) -> Result<()> {
        self.owner_control(&native)?;
        let _control = self.control.lock().map_err(|_| SessionFault::Unavailable)?;
        self.gate.lock()?;
        self.core.open_root(&self.root)?;
        self.vault.host_writer_state()?;
        self.gate.unlock()?;
        Ok(())
    }
}
impl Drop for HostService {
    fn drop(&mut self) {
        let _ = self.gate.lock();
        self.core.shutdown();
    }
}

#[cfg(all(test, windows))]
#[path = "../tests/support/scoped_reads.rs"]
mod scoped_reads;

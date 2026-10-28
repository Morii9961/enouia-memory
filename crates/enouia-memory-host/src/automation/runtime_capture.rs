//! Runtime producer binding on the already leased Vault. No wire enrollment.
use super::application::{ApplicationFault, ApplicationResult};
use crate::windows::{Pipe, ProcessPeer};
use enouia_memory_contract::{common::*, next::*};
use enouia_memory_vault::automation::{
    AutomationFault,
    segmented::{
        NativeRuntimeCapture, RuntimeCapturePermit, RuntimeCaptureScope, SegmentedAutomationVault,
    },
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub struct RuntimeCaptureHost {
    vault: Arc<SegmentedAutomationVault>,
    peer: ProcessPeer,
    permit: RuntimeCapturePermit,
    unknown: Mutex<Option<RuntimeCaptureRequest>>,
    connection_bound: AtomicBool,
}
impl RuntimeCaptureHost {
    /// Native Owner setup supplies the actual Runtime process and selected
    /// policy scope. Neither PID nor principal is obtained from a packet.
    pub fn enroll(
        vault: Arc<SegmentedAutomationVault>,
        native_owner: &ActorRef,
        native_runtime_pid: u32,
        principal: ActorRef,
        scope: RuntimeCaptureScope,
    ) -> ApplicationResult<Self> {
        if principal.actor_type != ActorType::Client {
            return Err(ApplicationFault::Invalid);
        }
        // ProcessPeer's legacy surface marker grants no capability here. This
        // object never enters Workspace transport or creates NativeAdmission.
        let peer = ProcessPeer::enroll(
            native_runtime_pid,
            enouia_memory_contract::workspace::HostSurface::QuickSearch,
        )
        .map_err(|_| ApplicationFault::Locked)?;
        let permit = vault.authorize_runtime_capture(native_owner, principal, scope)?;
        Ok(Self {
            vault,
            peer,
            permit,
            unknown: Mutex::new(None),
            connection_bound: AtomicBool::new(false),
        })
    }
    /// Lock/withdrawal is permanent for this enrollment. Fresh native setup is
    /// required; it never reconnects or replays retained work automatically.
    pub fn revoke(&self) {
        self.permit.revoke();
    }
    pub fn principal(&self) -> &ActorRef {
        self.permit.principal()
    }
    pub(crate) fn vault_id(&self) -> &enouia_memory_contract::ids::VaultId {
        &self.vault.descriptor().vault_id
    }
    pub(crate) fn bind_connection(&self) -> ApplicationResult<()> {
        self.live()?;
        self.connection_bound
            .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
            .map_err(|_| ApplicationFault::Locked)?;
        Ok(())
    }
    fn live(&self) -> ApplicationResult<()> {
        if self
            .unknown
            .lock()
            .map_err(|_| ApplicationFault::Locked)?
            .is_some()
        {
            return Err(AutomationFault::OutcomeUnknown.into());
        }
        if self.peer.live().is_err() {
            self.revoke();
            return Err(ApplicationFault::Locked);
        }
        self.vault.runtime_capture_live(&self.permit)?;
        Ok(())
    }
    /// A future Runtime transport calls this before decoding and dispatching.
    /// The actual kernel pipe peer must be the retained enrolled process.
    pub fn verify_peer(&self, pipe: &Pipe) -> ApplicationResult<()> {
        self.live()?;
        self.peer
            .verify_client(pipe)
            .map_err(|_| ApplicationFault::Locked)
    }
    pub fn preflight(
        &self,
        request: &RuntimeCaptureRequest,
    ) -> ApplicationResult<NativeRuntimeCapture> {
        self.live()?;
        let result = self.vault.capture_runtime(&self.permit, request, true)?;
        self.live()?;
        Ok(result)
    }
    pub fn capture(
        &self,
        request: &RuntimeCaptureRequest,
    ) -> ApplicationResult<NativeRuntimeCapture> {
        self.live()?;
        match self.vault.capture_runtime(&self.permit, request, false) {
            Ok(result) if self.live().is_ok() => Ok(result),
            Ok(_) | Err(AutomationFault::OutcomeUnknown) => {
                self.response_lost(request)?;
                Err(AutomationFault::OutcomeUnknown.into())
            }
            Err(error) => Err(error.into()),
        }
    }
    /// Called when a submitted packet has no trustworthy response. Keep the
    /// exact original and close admission; this is never a resend operation.
    pub fn response_lost(&self, request: &RuntimeCaptureRequest) -> ApplicationResult<()> {
        let mut unknown = self.unknown.lock().map_err(|_| ApplicationFault::Locked)?;
        if unknown.is_none() {
            *unknown = Some(request.clone());
        }
        self.revoke();
        Ok(())
    }
    pub fn original_unknown(&self) -> ApplicationResult<Option<RuntimeCaptureRequest>> {
        Ok(self
            .unknown
            .lock()
            .map_err(|_| ApplicationFault::Locked)?
            .clone())
    }
}

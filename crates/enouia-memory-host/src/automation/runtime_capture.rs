//! Runtime producer binding on the already leased Vault. No wire enrollment.
use super::application::{ApplicationFault, ApplicationResult};
use crate::windows::{Pipe, ProcessPeer};
use enouia_memory_contract::{common::*, next::*};
use enouia_memory_vault::automation::{
    AutomationFault,
    segmented::{
        CapturedQueueReceipt, NativeRuntimeCapture, ProducerJobStatus, RuntimeCapturePermit,
        RuntimeCaptureScope, SegmentedAutomationVault,
    },
};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, Ordering},
};

pub struct RuntimeCaptureHost {
    pub(super) vault: Arc<SegmentedAutomationVault>,
    peer: ProcessPeer,
    permit: RuntimeCapturePermit,
    unknown: Mutex<Option<RuntimeCaptureRequest>>,
    connection_bound: AtomicBool,
    pub(super) worker: Arc<super::captured_worker::Control>,
    formation_unknown: AtomicBool,
}
impl Drop for RuntimeCaptureHost {
    fn drop(&mut self) {
        self.revoke();
    }
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
        Self::enroll_profile(
            vault,
            native_owner,
            native_runtime_pid,
            principal,
            scope,
            false,
        )
    }
    /// Explicit Owner-root Runtime producer. Not an Agent or System reader.
    pub fn enroll_owner(
        vault: Arc<SegmentedAutomationVault>,
        native_owner: &ActorRef,
        native_runtime_pid: u32,
        principal: ActorRef,
        scope: RuntimeCaptureScope,
    ) -> ApplicationResult<Self> {
        Self::enroll_profile(
            vault,
            native_owner,
            native_runtime_pid,
            principal,
            scope,
            true,
        )
    }
    fn enroll_profile(
        vault: Arc<SegmentedAutomationVault>,
        native_owner: &ActorRef,
        native_runtime_pid: u32,
        principal: ActorRef,
        scope: RuntimeCaptureScope,
        owner_root: bool,
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
        let permit = if owner_root {
            vault.authorize_owner_runtime_capture(native_owner, principal, scope)?
        } else {
            vault.authorize_runtime_capture(native_owner, principal, scope)?
        };
        Ok(Self {
            vault,
            peer,
            permit,
            unknown: Mutex::new(None),
            connection_bound: AtomicBool::new(false),
            worker: Arc::new(super::captured_worker::Control::default()),
            formation_unknown: AtomicBool::new(false),
        })
    }
    /// Lock/withdrawal is permanent for this enrollment. Fresh native setup is
    /// required; it never reconnects or replays retained work automatically.
    pub fn revoke(&self) {
        self.worker.stop();
        self.permit.revoke();
    }
    pub(super) fn poison_formation(&self) {
        self.formation_unknown.store(true, Ordering::SeqCst);
        self.permit.revoke();
    }
    pub(super) fn formation_live(&self) -> ApplicationResult<()> {
        self.live()?;
        self.vault.runtime_formation_live(&self.permit)?;
        Ok(())
    }
    pub fn capability(&self) -> &'static str {
        self.permit.capability()
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
        if self.formation_unknown.load(Ordering::SeqCst)
            || self
                .unknown
                .lock()
                .map_err(|_| ApplicationFault::Locked)?
                .is_some()
        {
            return Err(AutomationFault::OutcomeUnknown.into());
        }
        if self.peer.live().is_err() {
            self.worker.cancel();
            self.permit.revoke();
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
        let _foreground = self.worker.foreground()?;
        let result = self.vault.capture_runtime(&self.permit, request, true)?;
        self.live()?;
        Ok(result)
    }
    pub fn capture(
        &self,
        request: &RuntimeCaptureRequest,
    ) -> ApplicationResult<NativeRuntimeCapture> {
        self.live()?;
        let _foreground = self.worker.foreground()?;
        match self.vault.capture_runtime(&self.permit, request, false) {
            Ok(result) if self.live().is_ok() => Ok(result),
            Ok(_) | Err(AutomationFault::OutcomeUnknown) => {
                self.response_lost(request)?;
                Err(AutomationFault::OutcomeUnknown.into())
            }
            Err(error) => Err(error.into()),
        }
    }
    /// Queue-only: durable waiting jobs for this producer's own captured
    /// revisions. Repeats return the original receipt and send nothing.
    pub fn enqueue(
        &self,
        capture: &NativeRuntimeCapture,
    ) -> ApplicationResult<CapturedQueueReceipt> {
        self.live()?;
        let _foreground = self.worker.foreground()?;
        if capture.producer != *self.permit.principal()
            || capture.scope_hash != *self.permit.scope_hash()
        {
            return Err(AutomationFault::Denied.into());
        }
        Ok(self.vault.enqueue_captured(
            &self.vault.descriptor().system_actor,
            capture,
            self.permit.policy(),
        )?)
    }
    pub fn status(
        &self,
        request: &RuntimeCaptureRequest,
    ) -> ApplicationResult<Vec<ProducerJobStatus>> {
        self.live()?;
        let _foreground = self.worker.foreground()?;
        let result = self.vault.runtime_capture_status(&self.permit, request)?;
        self.live()?;
        Ok(result)
    }
    /// Called when a submitted packet has no trustworthy response. Keep the
    /// exact original and close admission; this is never a resend operation.
    pub fn response_lost(&self, request: &RuntimeCaptureRequest) -> ApplicationResult<()> {
        let mut unknown = self.unknown.lock().map_err(|_| ApplicationFault::Locked)?;
        if unknown.is_none() {
            *unknown = Some(request.clone());
        }
        // Worker lifecycle checks take the control gate before this mutex.
        // Release the packet before waiting for that gate during revocation.
        drop(unknown);
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

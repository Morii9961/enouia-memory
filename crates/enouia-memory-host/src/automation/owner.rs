//! Explicit native Owner entry. Never wired to MCP or a model/JSON identity.
use super::application::{ApplicationFault, ApplicationResult};
use enouia_memory_contract::{common::*, hash::Sha256Hex, ids::OperationId, next::*};
use enouia_memory_vault::automation::segmented::{
    NativeOwnerConsent, NativeOwnerPreview, SegmentedAutomationVault,
};
use std::sync::{Arc, RwLock, RwLockReadGuard};
pub struct NativeMemoryOwner {
    vault: Arc<SegmentedAutomationVault>,
    owner: ActorRef,
    unlocked: RwLock<bool>,
}
impl NativeMemoryOwner {
    /// The embedding native Host authenticates this principal. Never fill it from wire data.
    pub fn from_authenticated_owner(
        vault: Arc<SegmentedAutomationVault>,
        owner: ActorRef,
    ) -> ApplicationResult<Self> {
        if owner.actor_type != ActorType::Owner
            || owner != vault.descriptor().created_by
            || vault.descriptor().layout != AutomationLayout::SegmentedOwnerV1
            || !vault.descriptor().validate().is_empty()
        {
            return Err(ApplicationFault::Invalid);
        }
        vault.snapshot()?;
        Ok(Self {
            vault,
            owner,
            unlocked: RwLock::new(true),
        })
    }
    fn gate(&self) -> ApplicationResult<RwLockReadGuard<'_, bool>> {
        let g = self.unlocked.read().map_err(|_| ApplicationFault::Locked)?;
        if !*g {
            return Err(ApplicationFault::Locked);
        }
        Ok(g)
    }
    pub fn lock(&self) -> ApplicationResult<()> {
        let mut g = self
            .unlocked
            .write()
            .map_err(|_| ApplicationFault::Locked)?;
        *g = false;
        self.vault.discard_owner_plans(&self.owner)?;
        Ok(())
    }
    pub fn unlock(&self, owner: &ActorRef) -> ApplicationResult<()> {
        if owner != &self.owner {
            return Err(ApplicationFault::Invalid);
        }
        self.vault.snapshot()?;
        *self
            .unlocked
            .write()
            .map_err(|_| ApplicationFault::Locked)? = true;
        Ok(())
    }
    pub fn plan(
        &self,
        action: OwnerAction,
        max: Sensitivity,
        correction: Option<OwnerCorrectionMetadata>,
    ) -> ApplicationResult<NativeOwnerPreview> {
        let _g = self.gate()?;
        Ok(self
            .vault
            .plan_owner_action(&self.owner, action, max, correction)?)
    }
    /// The native adapter calls this only after the human confirms the displayed plan.
    /// Do not forward an Agent/System message or parse a confirmation DTO as a gesture.
    pub fn confirm_from_owner_gesture(
        &self,
        operation: &OperationId,
        plan_hash: &Sha256Hex,
        effects_hash: &Sha256Hex,
        surface: TrustedSurface,
    ) -> ApplicationResult<NativeOwnerConsent> {
        let _g = self.gate()?;
        Ok(self.vault.confirm_owner_action(
            &self.owner,
            operation,
            plan_hash,
            effects_hash,
            surface,
        )?)
    }
    pub fn commit(&self, consent: NativeOwnerConsent) -> ApplicationResult<NativeOwnerOutcome> {
        let _g = self.gate()?;
        Ok(self.vault.commit_owner_action(&self.owner, consent)?)
    }
    pub fn outcome(
        &self,
        operation: &OperationId,
        max: Sensitivity,
    ) -> ApplicationResult<Option<NativeOwnerOutcome>> {
        let _g = self.gate()?;
        Ok(self
            .vault
            .owner_action_outcome(&self.owner, operation, max)?)
    }
}

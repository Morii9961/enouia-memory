//! MV-8.1a authority model and native platform ports (ADR-MEM-66).
//!
//! These are pure rules, not authentication, a durable store or an OS fence.
//! Every production write must check the latest authority inside the SAME
//! exclusive transaction as its data publication. Never authorize from a copy.

use crate::error::{ContractError, Violation};
use crate::hash::{Sha256Hex, sha256};
use crate::ids::{CommitId, DeviceId, OperationId, VaultId};
use crate::json::{
    MAX_SAFE_INTEGER, Revision, SchemaVersion, canonical_bytes, check_safe_integers,
    ensure_writable,
};
use crate::store::StoreDocument;
use serde::{Deserialize, Serialize};

/// Retain every migration receipt. Exhaustion refuses another migration;
/// receipts are never silently pruned or made reusable.
pub const MAX_PROMOTIONS: usize = 128;
pub const MAX_HOST_DOCUMENT_BYTES: usize = 256 * 1024;

/// Bounded raw host-document parsing. Direct typed deserialization also rejects
/// duplicate decoded field names, which a Value-only parser would overwrite.
/// All Host documents are closed structs/enums, without arbitrary JSON maps.
pub fn parse_document<T: HostDocument>(bytes: &[u8]) -> Result<T, ContractError> {
    if bytes.len() > MAX_HOST_DOCUMENT_BYTES {
        return Err(ContractError::Invalid(vec![Violation::new(
            "host.document_budget",
            "",
        )]));
    }
    let value: serde_json::Value =
        serde_json::from_slice(bytes).map_err(|_| ContractError::Malformed)?;
    if !value.is_object() {
        return Err(ContractError::Malformed);
    }
    ensure_writable(&value)?;
    let mut errors = Vec::new();
    check_safe_integers(&value, "", &mut errors);
    if !errors.is_empty() {
        return Err(ContractError::Invalid(errors));
    }
    let document: T = serde_json::from_slice(bytes)
        .map_err(|_| ContractError::Shape("invalid host document".into()))?;
    errors.extend(document.validate());
    if errors.is_empty() {
        Ok(document)
    } else {
        Err(ContractError::Invalid(errors))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PrimaryBinding {
    pub host_id: DeviceId,
    pub primary_epoch: Revision,
}

/// Must be independently verified against complete stored data by the native
/// adapter. Parsing a claimed digest is not snapshot or recovery verification.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Checkpoint {
    pub commit_id: CommitId,
    pub commit_sequence: Revision,
    pub snapshot_hash: Sha256Hex,
    pub policy_epoch: u64,
    pub deletion_epoch: u64,
}

impl Checkpoint {
    fn valid(&self) -> bool {
        self.policy_epoch <= MAX_SAFE_INTEGER && self.deletion_epoch <= MAX_SAFE_INTEGER
    }

    fn follows(&self, previous: &Self) -> bool {
        self.valid()
            && self.policy_epoch >= previous.policy_epoch
            && self.deletion_epoch >= previous.deletion_epoch
            && (self == previous
                || (self.commit_sequence > previous.commit_sequence
                    && self.commit_id != previous.commit_id
                    && self.snapshot_hash != previous.snapshot_hash))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferIntent {
    pub vault_id: VaultId,
    pub operation_id: OperationId,
    pub source: PrimaryBinding,
    pub target_host: DeviceId,
    pub base_revision: Revision,
    pub checkpoint: Checkpoint,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TransferPlan {
    pub schema_version: SchemaVersion,
    pub intent: TransferIntent,
    pub plan_hash: Sha256Hex,
}

fn intent_hash(intent: &TransferIntent) -> Result<Sha256Hex, HostFault> {
    canonical_bytes(intent)
        .map(|bytes| sha256(&bytes))
        .map_err(|_| HostFault::InvalidState)
}

impl StoreDocument for TransferPlan {
    fn validate(&self) -> Vec<Violation> {
        let mut errors = Vec::new();
        if self.intent.source.host_id == self.intent.target_host {
            errors.push(Violation::new(
                "host.distinct_target",
                "/intent/target_host",
            ));
        }
        if !self.intent.checkpoint.valid() {
            errors.push(Violation::new("host.watermark_range", "/intent/checkpoint"));
        }
        if intent_hash(&self.intent).ok().as_ref() != Some(&self.plan_hash) {
            errors.push(Violation::new("host.plan_hash", "/plan_hash"));
        }
        errors
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "state", rename_all = "snake_case", deny_unknown_fields)]
pub enum AuthorityPhase {
    Active,
    Quiesced { plan: Box<TransferPlan> },
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AuthorityState {
    pub schema_version: SchemaVersion,
    pub vault_id: VaultId,
    pub revision: Revision,
    pub primary: PrimaryBinding,
    pub checkpoint: Checkpoint,
    pub phase: AuthorityPhase,
    pub promotions: Vec<TransferPlan>,
}

mod sealed {
    pub trait Sealed {}
    impl Sealed for super::AuthorityState {}
    impl Sealed for super::TransferPlan {}
}

/// Only closed Host documents may use the raw parser's duplicate-field guarantee.
pub trait HostDocument: StoreDocument + sealed::Sealed {}
impl HostDocument for AuthorityState {}
impl HostDocument for TransferPlan {}

impl StoreDocument for AuthorityState {
    fn validate(&self) -> Vec<Violation> {
        let mut errors = Vec::new();
        if !self.checkpoint.valid() {
            errors.push(Violation::new("host.watermark_range", "/checkpoint"));
        }
        if self.promotions.len() > MAX_PROMOTIONS {
            errors.push(Violation::new("host.receipt_budget", "/promotions"));
        }
        let mut previous: Option<&TransferPlan> = None;
        let mut operations = std::collections::BTreeSet::new();
        for (index, plan) in self.promotions.iter().enumerate() {
            errors.extend(plan.validate());
            let intent = &plan.intent;
            let chain_ok = intent.vault_id == self.vault_id
                && intent.source.primary_epoch.get() == index as u64 + 1
                && operations.insert(&intent.operation_id)
                && previous.is_none_or(|old| {
                    intent.source.host_id == old.intent.target_host
                        && intent.base_revision.get() >= old.intent.base_revision.get() + 2
                        && intent.checkpoint.follows(&old.intent.checkpoint)
                });
            if !chain_ok {
                errors.push(Violation::new("host.receipt_chain", "/promotions"));
            }
            previous = Some(plan);
        }
        if self.primary.primary_epoch.get() != self.promotions.len() as u64 + 1
            || previous.is_some_and(|last| {
                self.primary.host_id != last.intent.target_host
                    || self.revision.get() < last.intent.base_revision.get() + 2
                    || !self.checkpoint.follows(&last.intent.checkpoint)
            })
        {
            errors.push(Violation::new("host.primary_chain", "/primary"));
        }
        if let AuthorityPhase::Quiesced { plan } = &self.phase {
            errors.extend(plan.validate());
            if !self.matches_plan(plan, true)
                || self
                    .promotions
                    .iter()
                    .any(|p| p.intent.operation_id == plan.intent.operation_id)
            {
                errors.push(Violation::new("host.quiesced_binding", "/phase"));
            }
        }
        errors
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HostFault {
    InvalidState,
    Fenced,
    Quiesced,
    Conflict,
    PlanMismatch,
    CheckpointMismatch,
    CounterExhausted,
    ReceiptBudget,
    Busy,
    PermissionDenied,
    Unavailable,
    StorageFailed,
    /// Publication may have committed. Reload; never infer safe rollback.
    OutcomeUnknown,
}

pub type HostResult<T> = Result<T, HostFault>;

fn increment(value: Revision) -> HostResult<Revision> {
    Revision::new(value.get() + 1).ok_or(HostFault::CounterExhausted)
}

impl AuthorityState {
    pub fn initial(
        vault_id: VaultId,
        host_id: DeviceId,
        checkpoint: Checkpoint,
    ) -> HostResult<Self> {
        let state = Self {
            schema_version: SchemaVersion,
            vault_id,
            revision: Revision::new(1).expect("one"),
            primary: PrimaryBinding {
                host_id,
                primary_epoch: Revision::new(1).expect("one"),
            },
            checkpoint,
            phase: AuthorityPhase::Active,
            promotions: Vec::new(),
        };
        state.checked()?;
        Ok(state)
    }

    fn checked(&self) -> HostResult<()> {
        if self.validate().is_empty() {
            Ok(())
        } else {
            Err(HostFault::InvalidState)
        }
    }

    fn matches_plan(&self, plan: &TransferPlan, quiesced: bool) -> bool {
        self.vault_id == plan.intent.vault_id
            && self.primary == plan.intent.source
            && self.checkpoint == plan.intent.checkpoint
            && self.revision.get() == plan.intent.base_revision.get() + u64::from(quiesced)
    }

    fn confirmation(plan: &TransferPlan, confirmed_hash: &Sha256Hex) -> HostResult<()> {
        if !plan.validate().is_empty() || confirmed_hash != &plan.plan_hash {
            Err(HostFault::PlanMismatch)
        } else {
            Ok(())
        }
    }

    /// Pure comparison only; invoke against authoritative state inside a native
    /// transaction. Caller identity must NOT come from a client packet.
    pub fn check_writer(&self, vault: &VaultId, writer: &PrimaryBinding) -> HostResult<()> {
        self.checked()?;
        if &self.vault_id != vault || &self.primary != writer {
            return Err(HostFault::Fenced);
        }
        if !matches!(self.phase, AuthorityPhase::Active) {
            return Err(HostFault::Quiesced);
        }
        Ok(())
    }

    /// Native planning only; does not authenticate or approve a migration.
    pub fn prepare_transfer(
        &self,
        operation_id: OperationId,
        target_host: DeviceId,
    ) -> HostResult<TransferPlan> {
        self.check_writer(&self.vault_id, &self.primary)?;
        if self.promotions.len() == MAX_PROMOTIONS {
            return Err(HostFault::ReceiptBudget);
        }
        if self
            .promotions
            .iter()
            .any(|p| p.intent.operation_id == operation_id)
        {
            return Err(HostFault::PlanMismatch);
        }
        let intent = TransferIntent {
            vault_id: self.vault_id.clone(),
            operation_id,
            source: self.primary.clone(),
            target_host,
            base_revision: self.revision,
            checkpoint: self.checkpoint.clone(),
        };
        let plan = TransferPlan {
            schema_version: SchemaVersion,
            plan_hash: intent_hash(&intent)?,
            intent,
        };
        Self::confirmation(&plan, &plan.plan_hash)?;
        Ok(plan)
    }

    /// The trusted control plane must obtain owner confirmation before calling.
    pub fn quiesce(
        &self,
        writer: &PrimaryBinding,
        plan: &TransferPlan,
        confirmed_hash: &Sha256Hex,
    ) -> HostResult<Self> {
        self.checked()?;
        Self::confirmation(plan, confirmed_hash)?;
        if writer != &self.primary {
            return Err(HostFault::Fenced);
        }
        if let AuthorityPhase::Quiesced { plan: old } = &self.phase {
            return if old.as_ref() == plan {
                Ok(self.clone())
            } else {
                Err(HostFault::Conflict)
            };
        }
        if !self.matches_plan(plan, false) {
            return Err(HostFault::Conflict);
        }
        let mut next = self.clone();
        next.revision = increment(self.revision)?;
        next.phase = AuthorityPhase::Quiesced {
            plan: Box::new(plan.clone()),
        };
        next.checked()?;
        Ok(next)
    }

    /// Target identity and restored checkpoint must come from a trusted adapter,
    /// not a client's claimed proof. Owner approval is separate from identity.
    pub fn promote(
        &self,
        target: &DeviceId,
        plan: &TransferPlan,
        confirmed_hash: &Sha256Hex,
        verified_target: &Checkpoint,
    ) -> HostResult<Self> {
        self.checked()?;
        Self::confirmation(plan, confirmed_hash)?;
        if target != &plan.intent.target_host {
            return Err(HostFault::Fenced);
        }
        if verified_target != &plan.intent.checkpoint {
            return Err(HostFault::CheckpointMismatch);
        }
        if let Some(receipt) = self
            .promotions
            .iter()
            .find(|p| p.intent.operation_id == plan.intent.operation_id)
        {
            return if receipt == plan {
                Ok(self.clone())
            } else {
                Err(HostFault::PlanMismatch)
            };
        }
        if !matches!(&self.phase, AuthorityPhase::Quiesced { plan: saved } if saved.as_ref() == plan)
            || !self.matches_plan(plan, true)
        {
            return Err(HostFault::Conflict);
        }
        if self.promotions.len() == MAX_PROMOTIONS {
            return Err(HostFault::ReceiptBudget);
        }
        let mut next = self.clone();
        next.revision = increment(self.revision)?;
        next.primary = PrimaryBinding {
            host_id: target.clone(),
            primary_epoch: increment(self.primary.primary_epoch)?,
        };
        next.phase = AuthorityPhase::Active;
        next.promotions.push(plan.clone());
        next.checked()?;
        Ok(next)
    }

    pub fn abort(
        &self,
        writer: &PrimaryBinding,
        plan: &TransferPlan,
        confirmed_hash: &Sha256Hex,
    ) -> HostResult<Self> {
        self.checked()?;
        Self::confirmation(plan, confirmed_hash)?;
        if writer != &self.primary {
            return Err(HostFault::Fenced);
        }
        if !matches!(&self.phase, AuthorityPhase::Quiesced { plan: saved } if saved.as_ref() == plan)
        {
            return Err(HostFault::Conflict);
        }
        let mut next = self.clone();
        next.revision = increment(self.revision)?;
        next.phase = AuthorityPhase::Active;
        next.checked()?;
        Ok(next)
    }

    /// Data publication and this transition MUST be one platform transaction.
    pub fn advance(
        &self,
        writer: &PrimaryBinding,
        expected: &Checkpoint,
        published: Checkpoint,
    ) -> HostResult<Self> {
        self.check_writer(&self.vault_id, writer)?;
        if expected != &self.checkpoint {
            return Err(HostFault::Conflict);
        }
        if !published.follows(expected)
            || published.commit_sequence != increment(expected.commit_sequence)?
        {
            return Err(HostFault::CheckpointMismatch);
        }
        let mut next = self.clone();
        next.revision = increment(self.revision)?;
        next.checkpoint = published;
        next.checked()?;
        Ok(next)
    }
}

/// Native platform transaction. No production adapter exists in MV-8.1a.
/// Must own process exclusion and the shared authority/data publication boundary.
pub trait AuthorityTransaction {
    fn load(&mut self) -> HostResult<AuthorityState>;
    fn verified_checkpoint(&mut self) -> HostResult<Checkpoint>;
    /// Compare the complete expected state, publish next with staged data, flush,
    /// and only then succeed. A post-publication failure is OutcomeUnknown.
    fn publish(&mut self, expected: &AuthorityState, next: &AuthorityState) -> HostResult<()>;
}

/// Never serve a transaction from a cached/local authority when the authoritative
/// store is unavailable. Acquire exclusion before loading; hold it through flush.
pub trait AuthorityStore {
    fn transact<T>(
        &mut self,
        vault: &VaultId,
        action: impl FnOnce(&mut dyn AuthorityTransaction) -> HostResult<T>,
    ) -> HostResult<T>;
}

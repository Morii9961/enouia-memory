//! Native owner policy plans. Inspection precedes grant/revocation; no HTTP.
use crate::{Result, codec::Api, error};
use enouia_memory_contract::{
    MemoryErrorCode,
    approval::{ApprovalBinding, ApprovalRecord},
    commit::OperationKind,
    common::{ActorRef, ActorType, TrustedSurface},
    context::{DestinationKind, Purpose},
    hash::{Sha256Hex, sha256},
    ids::{ApprovalId, CommitId, PolicyId, VaultId},
    json::{Revision, SchemaVersion, canonical_bytes},
    policy::*,
    ports::{CommitOutcome, CommitPin, CommitRequest, IdempotencyScope, StagedRecord},
    record::{RecordKind, RecordRef, parse_record},
    time::Timestamp,
};
use enouia_memory_vault::Vault;

/// Opaque plan: callers inspect the exact persisted policy and confirm its hash.
pub struct PolicyPlan {
    owner: ActorRef,
    surface: TrustedSurface,
    vault_id: VaultId,
    root: std::path::PathBuf,
    pin: CommitPin,
    policy: PolicyRecord,
    bytes: Vec<u8>,
    expected: Option<Revision>,
    expires: Timestamp,
    nonce: String,
}
impl PolicyPlan {
    pub fn inspect(&self) -> &[u8] {
        &self.bytes
    }
    pub fn hash(&self) -> Sha256Hex {
        sha256(&self.bytes)
    }
    pub fn policy_id(&self) -> &PolicyId {
        &self.policy.policy_id
    }
}
fn owner(vault: &Vault, actor: &ActorRef) -> Result<()> {
    if actor.actor_type != ActorType::Owner || actor != &vault.descriptor().created_by {
        return Err(error(MemoryErrorCode::PermissionDenied));
    }
    Ok(())
}
/// A standing grant is explicitly scoped to project/kind/sensitivity, purpose,
/// and exact API/model pairs. It cannot declassify private content.
pub fn plan_grant(
    vault: &Vault,
    actor: ActorRef,
    surface: TrustedSurface,
    resources: ResourceSelector,
    purposes: Vec<Purpose>,
    bindings: &[(Api, String)],
) -> Result<PolicyPlan> {
    owner(vault, &actor)?;
    if bindings.is_empty() {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let now = vault.now().map_err(|e| e.error)?;
    let mut destinations = vec![];
    for (api, model) in bindings {
        let binding = api.binding(model)?;
        destinations.push(DestinationSelector {
            kind: DestinationKind::ExternalProvider,
            provider: Some(binding.provider),
            model: Some(binding.model),
        });
    }
    let policy = PolicyRecord {
        schema_version: SchemaVersion,
        policy_id: PolicyId::from_random(vault.random_id_bytes()),
        revision: Revision::new(1).expect("one"),
        status: PolicyStatus::Active,
        origin: PolicyOrigin::OwnerGrant,
        approval_id: Some(ApprovalId::from_random(vault.random_id_bytes())),
        principals: vec![PrincipalSelector {
            actor_type: ActorType::Owner,
            actor_id: Some(actor.actor_id.clone()),
        }],
        scopes: vec![Scope::ContextRead, Scope::ProviderSend],
        resources,
        purposes,
        destinations,
        valid_from: now.clone(),
        valid_until: None,
        revoked_at: None,
        created_at: now.clone(),
        updated_at: now,
    };
    make_plan(vault, actor, surface, policy, None)
}
pub fn plan_revoke(
    vault: &Vault,
    actor: ActorRef,
    surface: TrustedSurface,
    id: &PolicyId,
) -> Result<PolicyPlan> {
    owner(vault, &actor)?;
    let pin = vault.pin_current().map_err(|e| e.error)?;
    let entry = vault
        .record_entry(&pin, RecordKind::Policy, id.as_str())
        .map_err(|e| e.error)?
        .ok_or_else(|| error(MemoryErrorCode::NotFound))?;
    let data = vault
        .read_record(
            &pin,
            &RecordRef::new(RecordKind::Policy, id.as_str(), entry.revision),
        )
        .map_err(|e| e.error)?;
    let mut policy: PolicyRecord =
        parse_record(&data).map_err(|_| error(MemoryErrorCode::InvalidRequest))?;
    if policy.origin != PolicyOrigin::OwnerGrant || policy.status != PolicyStatus::Active {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let expected = policy.revision;
    policy.revision =
        Revision::new(expected.get() + 1).ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?;
    policy.status = PolicyStatus::Revoked;
    policy.revoked_at = Some(vault.now().map_err(|e| e.error)?);
    policy.updated_at = policy.revoked_at.clone().expect("set");
    // Revocation requires its own approval for this exact policy revision.
    policy.approval_id = Some(ApprovalId::from_random(vault.random_id_bytes()));
    make_plan(vault, actor, surface, policy, Some(expected))
}
fn make_plan(
    vault: &Vault,
    owner: ActorRef,
    surface: TrustedSurface,
    policy: PolicyRecord,
    expected: Option<Revision>,
) -> Result<PolicyPlan> {
    if !policy.validate().is_empty() {
        return Err(error(MemoryErrorCode::InvalidRequest));
    }
    let bytes = canonical_bytes(&policy).map_err(|_| error(MemoryErrorCode::InvalidRequest))?;
    let now = vault.now().map_err(|e| e.error)?;
    Ok(PolicyPlan {
        owner,
        surface,
        vault_id: vault.descriptor().vault_id.clone(),
        root: vault.managed_root().root().to_path_buf(),
        pin: vault.pin_current().map_err(|e| e.error)?,
        policy,
        bytes,
        expected,
        expires: Timestamp::from_unix_ms(now.unix_ms() + 600_000)
            .ok_or_else(|| error(MemoryErrorCode::InvalidRequest))?,
        nonce: CommitId::from_random(vault.random_id_bytes()).to_string(),
    })
}
pub fn confirm(
    vault: &Vault,
    actor: &ActorRef,
    surface: TrustedSurface,
    plan: &PolicyPlan,
    inspected_hash: &Sha256Hex,
) -> Result<CommitOutcome> {
    owner(vault, actor)?;
    // A restored/copied history may retain owner, Vault ID and CURRENT.
    // Native inspection still authorizes only the local Vault it came from.
    // This physical binding is private, never part of persisted policy bytes.
    if actor != &plan.owner
        || surface != plan.surface
        || plan.vault_id != vault.descriptor().vault_id
        || plan.root.as_path() != vault.managed_root().root()
        || inspected_hash != &plan.hash()
    {
        return Err(error(MemoryErrorCode::PermissionDenied));
    }
    let now = vault.now().map_err(|e| e.error)?;
    if now > plan.expires {
        return Err(error(MemoryErrorCode::PermissionDenied));
    }
    let approval_id = plan.policy.approval_id.clone().expect("owner grant");
    let approval = ApprovalRecord {
        schema_version: SchemaVersion,
        approval_id: approval_id.clone(),
        approved_by: actor.clone(),
        trusted_surface: surface,
        approval_nonce: plan.nonce.clone(),
        approved_diff_hash: plan.hash(),
        issued_at: now,
        expires_at: None,
        binding: ApprovalBinding::PolicyGrant {
            policy_id: plan.policy.policy_id.clone(),
            policy_revision: plan.policy.revision,
            grant_hash: plan.hash(),
        },
    };
    vault
        .commit(CommitRequest {
            commit_id: CommitId::from_random(vault.random_id_bytes()),
            expected_commit_id: Some(plan.pin.commit_id.clone()),
            principal: actor.clone(),
            operation_kind: OperationKind::PolicyChange,
            idempotency: IdempotencyScope {
                principal_id: actor.actor_id.clone(),
                operation_kind: OperationKind::PolicyChange,
                key_hash: sha256(plan.nonce.as_bytes()),
            },
            request_payload_hash: plan.hash(),
            expected_revisions: vec![
                (
                    RecordKind::Policy,
                    plan.policy.policy_id.to_string(),
                    plan.expected,
                ),
                (RecordKind::Approval, approval_id.to_string(), None),
            ],
            records: vec![
                StagedRecord {
                    record_kind: RecordKind::Policy,
                    record_id: plan.policy.policy_id.to_string(),
                    revision: plan.policy.revision,
                    bytes: plan.bytes.clone(),
                },
                StagedRecord {
                    record_kind: RecordKind::Approval,
                    record_id: approval_id.to_string(),
                    revision: Revision::new(1).expect("one"),
                    bytes: canonical_bytes(&approval)
                        .map_err(|_| error(MemoryErrorCode::InvalidRequest))?,
                },
            ],
            objects: vec![],
        })
        .map_err(|e| e.error)
}

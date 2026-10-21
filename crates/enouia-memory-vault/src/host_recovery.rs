//! Native witnessed publication and exact, owner-confirmed forward recovery.
//! No directory guessing, legacy fallback, data rewrite or independent restore.
use crate::error::Result;
use crate::fault::FaultPoint;
use crate::host_transaction::{HostCoordinator, MODE_FILE};
use crate::{Vault, VerifiedRoot};
use enouia_memory_contract::catalog::StoredCommit;
use enouia_memory_contract::hash::{Sha256Hex, sha256};
use enouia_memory_contract::host::publication::{
    MAX_INTENT_BYTES, MAX_WITNESS_BYTES, PublicationIntent, PublicationWitness,
};
use enouia_memory_contract::host::{
    AuthorityState, HostFault, MAX_HOST_DOCUMENT_BYTES, parse_document,
};
use enouia_memory_contract::ids::DeviceId;
use enouia_memory_contract::json::{SchemaVersion, canonical_bytes};
use enouia_memory_contract::layout;
use enouia_memory_contract::store::{CurrentPointer, parse_store};
use serde::Serialize;

pub const INTENT_FILE: &str = "config/host-publication-intent.json";
pub const WITNESS_FILE: &str = "config/host-publication-witness.json";

fn pointer(bytes: &[u8]) -> Result<CurrentPointer> {
    // Direct decode also rejects duplicate decoded fields.
    let p = serde_json::from_slice(bytes).map_err(|_| HostFault::InvalidState)?;
    parse_store::<CurrentPointer>(bytes).map_err(|_| HostFault::InvalidState)?;
    Ok(p)
}
fn authority_path(hash: &Sha256Hex) -> String {
    format!("vault/host-authority/{hash}.json")
}
fn hash(bytes: &Option<Vec<u8>>) -> Option<Sha256Hex> {
    bytes.as_ref().map(|b| sha256(b))
}

impl Vault {
    fn publication_intent(&self) -> Result<(PublicationIntent, Vec<u8>)> {
        let bytes = self
            .managed_root()
            .read_bounded(INTENT_FILE, MAX_INTENT_BYTES)?
            .ok_or(HostFault::Unavailable)?;
        let intent = parse_document(&bytes).map_err(|_| HostFault::InvalidState)?;
        Ok((intent, bytes))
    }

    fn check_intent_transition(&self, intent: &PublicationIntent) -> Result<()> {
        let before = intent
            .before
            .host_authority_hash
            .as_ref()
            .map(|hash| -> Result<AuthorityState> {
                let bytes = self
                    .managed_root()
                    .read_bounded(&authority_path(hash), MAX_HOST_DOCUMENT_BYTES)?
                    .ok_or(HostFault::Unavailable)?;
                if &sha256(&bytes) != hash {
                    return Err(HostFault::InvalidState.into());
                }
                parse_document::<AuthorityState>(&bytes).map_err(|_| HostFault::InvalidState.into())
            })
            .transpose()?;
        intent.check_transition(before.as_ref())?;
        Ok(())
    }

    fn check_intent_data(&self, intent: &PublicationIntent) -> Result<()> {
        self.check_intent_transition(intent)?;
        let pin = self.pin_pointer(&intent.after)?;
        if self.checkpoint_for_locked(&pin)? != intent.authority.checkpoint {
            return Err(HostFault::CheckpointMismatch.into());
        }
        Ok(())
    }

    /// Called after CURRENT/authority and actual data have already been checked.
    pub(crate) fn check_publication_locked(
        &self,
        current: &CurrentPointer,
        state: &AuthorityState,
    ) -> Result<()> {
        let (intent, _) = self.publication_intent()?;
        let bytes = self
            .managed_root()
            .read_bounded(WITNESS_FILE, MAX_WITNESS_BYTES)?
            .ok_or(HostFault::Unavailable)?;
        let witness: PublicationWitness =
            parse_document(&bytes).map_err(|_| HostFault::InvalidState)?;
        if &intent.after != current || &intent.authority != state || &witness.current != current {
            return Err(HostFault::Unavailable.into());
        }
        self.check_intent_transition(&intent)
    }

    fn ensure_host_mode(&self) -> Result<()> {
        match self.managed_root().read_bounded(MODE_FILE, 1024)? {
            None => self
                .managed_root()
                .write_new(MODE_FILE, &self.mode_bytes())?,
            Some(bytes) if bytes == self.mode_bytes() => {}
            Some(_) => return Err(HostFault::InvalidState.into()),
        }
        Ok(())
    }

    fn witness(&self, current: &CurrentPointer) -> Result<()> {
        let witness = PublicationWitness {
            schema_version: SchemaVersion,
            current: current.clone(),
        };
        self.managed_root().write_atomic(
            WITNESS_FILE,
            &canonical_bytes(&witness).map_err(|_| HostFault::InvalidState)?,
        )?;
        self.managed_root()
            .faults()
            .io(FaultPoint::HostWitnessPublished)?;
        Ok(())
    }

    /// Caller owns Host/writer exclusion and validated the previous state.
    pub(crate) fn publish_host(
        &self,
        commit: &StoredCommit,
        state: &AuthorityState,
        bootstrap: bool,
    ) -> Result<()> {
        let before = pointer(
            &self
                .managed_root()
                .read_bounded(&layout::current_pointer(), 4096)?
                .ok_or(HostFault::Unavailable)?,
        )?;
        if bootstrap != before.host_authority_hash.is_none() {
            return Err(HostFault::InvalidState.into());
        }
        let authority = canonical_bytes(state).map_err(|_| HostFault::InvalidState)?;
        let after = self.pointer_for(commit, Some(sha256(&authority)))?;
        let intent = PublicationIntent {
            schema_version: SchemaVersion,
            before,
            after,
            authority: state.clone(),
        };
        let bytes = canonical_bytes(&intent).map_err(|_| HostFault::InvalidState)?;
        parse_document::<PublicationIntent>(&bytes).map_err(|_| HostFault::InvalidState)?;
        self.check_intent_data(&intent)?;
        // This is the first Host mutation in bootstrap. Its presence alone
        // fences all legacy operations, including a crash before MODE exists.
        self.managed_root().write_atomic(INTENT_FILE, &bytes)?;
        self.managed_root()
            .faults()
            .io(FaultPoint::HostIntentPublished)?;
        self.ensure_host_mode()?;
        let authority_hash = self.write_authority(state)?;
        self.publish_with_authority(commit, Some(authority_hash))?;
        self.witness(&intent.after)
    }
}

struct Observation {
    intent: PublicationIntent,
    intent_hash: Sha256Hex,
    current: Option<Vec<u8>>,
    mode: Option<Vec<u8>>,
    witness: Option<Vec<u8>>,
    authority: Option<Vec<u8>>,
    complete: bool,
}

/// Native-produced metadata only. It cannot be deserialized from a page packet.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct RecoveryPlan {
    plan_hash: Sha256Hex,
    intent_hash: Sha256Hex,
    current_hash: Option<Sha256Hex>,
    mode_hash: Option<Sha256Hex>,
    witness_hash: Option<Sha256Hex>,
    authority_hash: Option<Sha256Hex>,
    target: AuthorityState,
    repair_needed: bool,
}
impl RecoveryPlan {
    pub fn hash(&self) -> &Sha256Hex {
        &self.plan_hash
    }
    pub fn target(&self) -> &AuthorityState {
        &self.target
    }
    pub fn repair_needed(&self) -> bool {
        self.repair_needed
    }
}

/// Holds the same OS Host lease as the normal coordinator. Identity and owner
/// confirmation belong to the trusted native control plane, never a client.
pub struct HostRecovery<'a> {
    host: HostCoordinator<'a>,
}
impl<'a> HostRecovery<'a> {
    pub fn open(vault: &'a Vault, root: &VerifiedRoot, identity: DeviceId) -> Result<Self> {
        Ok(Self {
            host: HostCoordinator::acquire(vault, root, identity)?,
        })
    }

    fn observe(&self) -> Result<Observation> {
        let vault = self.host.vault;
        let (intent, bytes) = vault.publication_intent()?;
        if intent.authority.primary.host_id != self.host.identity {
            return Err(HostFault::Fenced.into());
        }
        vault.check_intent_data(&intent)?;
        let authority_hash = intent
            .after
            .host_authority_hash
            .as_ref()
            .ok_or(HostFault::InvalidState)?;
        let authority_bytes = vault
            .managed_root()
            .read_bounded(&authority_path(authority_hash), MAX_HOST_DOCUMENT_BYTES)?;
        let mode = vault.managed_root().read_bounded(MODE_FILE, 1024)?;
        // A torn new mode file is repairable from the verified v2 intent.
        // An explicitly different format is never silently upgraded.
        if mode.as_ref().is_some_and(|b| {
            serde_json::from_slice::<serde_json::Value>(b).is_ok_and(|v| {
                v.get("mode")
                    .and_then(|x| x.as_str())
                    .is_some_and(|m| m != "host-current-v2")
                    || v.get("schema_version")
                        .is_some_and(|s| s != &serde_json::json!(1))
            })
        }) {
            return Err(HostFault::InvalidState.into());
        }
        let witness = vault
            .managed_root()
            .read_bounded(WITNESS_FILE, MAX_WITNESS_BYTES)?;
        let w: Option<PublicationWitness> = witness
            .as_ref()
            .map(|b| parse_document(b).map_err(|_| HostFault::InvalidState))
            .transpose()?;
        match &w {
            Some(w) if w.current == intent.before || w.current == intent.after => {}
            None if intent.before.host_authority_hash.is_none() => {}
            _ => return Err(HostFault::Unavailable.into()),
        }
        let current = vault
            .managed_root()
            .read_bounded(&layout::current_pointer(), 4096)?;
        let parsed = current.as_ref().and_then(|b| pointer(b).ok());
        if parsed
            .as_ref()
            .is_some_and(|p| p != &intent.before && p != &intent.after)
        {
            return Err(HostFault::Conflict.into());
        }
        let complete = authority_bytes
            .as_ref()
            .is_some_and(|b| &sha256(b) == authority_hash)
            && mode.as_ref() == Some(&vault.mode_bytes())
            && parsed.as_ref() == Some(&intent.after)
            && w.as_ref().is_some_and(|w| w.current == intent.after);
        Ok(Observation {
            intent,
            intent_hash: sha256(&bytes),
            current,
            mode,
            witness,
            authority: authority_bytes,
            complete,
        })
    }

    fn plan_at(&self, o: &Observation) -> Result<RecoveryPlan> {
        let current_hash = hash(&o.current);
        let mode_hash = hash(&o.mode);
        let witness_hash = hash(&o.witness);
        let authority_hash = hash(&o.authority);
        let plan_hash = sha256(
            &canonical_bytes(&serde_json::json!({
                "version":"host-recovery-plan-1", "identity":self.host.identity,
                "intentHash":o.intent_hash, "currentHash":current_hash,
                "modeHash":mode_hash, "witnessHash":witness_hash, "authorityHash":authority_hash, "target":o.intent.authority,
            }))
            .map_err(|_| HostFault::InvalidState)?,
        );
        Ok(RecoveryPlan {
            plan_hash,
            intent_hash: o.intent_hash.clone(),
            current_hash,
            mode_hash,
            witness_hash,
            authority_hash,
            target: o.intent.authority.clone(),
            repair_needed: !o.complete,
        })
    }

    /// Bounded, read-only verification; never initializes or repairs.
    pub fn plan(&self) -> Result<RecoveryPlan> {
        let _guard = self.host.vault.lock()?;
        self.plan_at(&self.observe()?)
    }

    /// Trusted native caller has obtained explicit owner confirmation of hash.
    /// Exact target bytes from this repair may be recognized after a lost reply.
    pub fn confirm(
        &self,
        plan: &RecoveryPlan,
        confirmed_hash: &Sha256Hex,
    ) -> Result<AuthorityState> {
        let vault = self.host.vault;
        let _guard = vault.lock()?;
        if plan.hash() != confirmed_hash {
            return Err(HostFault::PlanMismatch.into());
        }
        let o = self.observe()?;
        if o.intent_hash != plan.intent_hash || o.intent.authority != plan.target {
            return Err(HostFault::Conflict.into());
        }
        let target_current =
            canonical_bytes(&o.intent.after).map_err(|_| HostFault::InvalidState)?;
        let target_witness = canonical_bytes(&PublicationWitness {
            schema_version: SchemaVersion,
            current: o.intent.after.clone(),
        })
        .map_err(|_| HostFault::InvalidState)?;
        let target_authority =
            canonical_bytes(&o.intent.authority).map_err(|_| HostFault::InvalidState)?;
        for (now, before, after) in [
            (
                hash(&o.current),
                &plan.current_hash,
                sha256(&target_current),
            ),
            (hash(&o.mode), &plan.mode_hash, sha256(&vault.mode_bytes())),
            (
                hash(&o.witness),
                &plan.witness_hash,
                sha256(&target_witness),
            ),
            (
                hash(&o.authority),
                &plan.authority_hash,
                sha256(&target_authority),
            ),
        ] {
            if &now != before && now.as_ref() != Some(&after) {
                return Err(HostFault::Conflict.into());
            }
        }
        if !o.complete {
            let write = || -> Result<()> {
                if o.mode.as_ref() != Some(&vault.mode_bytes()) {
                    vault
                        .managed_root()
                        .write_atomic(MODE_FILE, &vault.mode_bytes())?;
                }
                if o.authority.as_ref() != Some(&target_authority) {
                    let selected = o
                        .intent
                        .after
                        .host_authority_hash
                        .as_ref()
                        .ok_or(HostFault::InvalidState)?;
                    vault
                        .managed_root()
                        .write_atomic(&authority_path(selected), &target_authority)?;
                }
                if o.current.as_ref() != Some(&target_current) {
                    let commit = vault.stored_commit(&vault.pin_pointer(&o.intent.after)?)?;
                    vault.publish_with_authority(
                        &commit,
                        o.intent.after.host_authority_hash.clone(),
                    )?;
                }
                if o.witness.as_ref() != Some(&target_witness) {
                    vault.witness(&o.intent.after)?;
                }
                Ok(())
            };
            write().map_err(|_| HostFault::OutcomeUnknown)?;
        }
        Ok(o.intent.authority)
    }
}

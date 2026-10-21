//! Exact local publication evidence; never a distributed authority or grant.
use super::*;
use crate::store::CurrentPointer;

pub const MAX_INTENT_BYTES: usize = MAX_HOST_DOCUMENT_BYTES + 8192;
pub const MAX_WITNESS_BYTES: usize = 4096;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationIntent {
    pub schema_version: SchemaVersion,
    pub before: CurrentPointer,
    pub after: CurrentPointer,
    pub authority: AuthorityState,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PublicationWitness {
    pub schema_version: SchemaVersion,
    pub current: CurrentPointer,
}

impl sealed::Sealed for PublicationIntent {}
impl sealed::Sealed for PublicationWitness {}
impl HostDocument for PublicationIntent {
    const MAX_BYTES: usize = MAX_INTENT_BYTES;
}
impl HostDocument for PublicationWitness {
    const MAX_BYTES: usize = MAX_WITNESS_BYTES;
}

impl StoreDocument for PublicationWitness {
    fn validate(&self) -> Vec<Violation> {
        let mut out = self.current.validate();
        if self.current.host_authority_hash.is_none() {
            out.push(Violation::new("host.witness_binding", "/current"));
        }
        out
    }
}

impl StoreDocument for PublicationIntent {
    fn validate(&self) -> Vec<Violation> {
        let mut out = self.before.validate();
        out.extend(self.after.validate());
        out.extend(self.authority.validate());
        let authority = canonical_bytes(&self.authority);
        let same_commit = self.before.commit_id == self.after.commit_id
            && self.before.sequence == self.after.sequence
            && self.before.manifest_sha256 == self.after.manifest_sha256;
        let advances = self.after.sequence == self.before.sequence.saturating_add(1)
            && self.after.commit_id != self.before.commit_id;
        if self.before.vault_id != self.after.vault_id
            || self.after.vault_id != self.authority.vault_id
            || self.after.commit_id != self.authority.checkpoint.commit_id
            || self.after.sequence != self.authority.checkpoint.commit_sequence.get()
            || authority
                .as_ref()
                .is_ok_and(|b| b.len() > MAX_HOST_DOCUMENT_BYTES)
            || authority.as_ref().ok().map(|b| sha256(b)) != self.after.host_authority_hash
            || !(same_commit || advances)
            || (self.before.host_authority_hash.is_none() && !same_commit)
        {
            out.push(Violation::new("host.publication_binding", ""));
        }
        out
    }
}

impl PublicationIntent {
    /// Validate the one legal transition selected by this intent. Actual
    /// before-authority bytes and the target data must be verified by the port.
    pub fn check_transition(&self, before: Option<&AuthorityState>) -> HostResult<()> {
        if !self.validate().is_empty() {
            return Err(HostFault::InvalidState);
        }
        let expected = match before {
            None if self.before.host_authority_hash.is_none() => AuthorityState::initial(
                self.authority.vault_id.clone(),
                self.authority.primary.host_id.clone(),
                self.authority.checkpoint.clone(),
            )?,
            Some(old) => {
                if self.before.host_authority_hash.as_ref()
                    != Some(&sha256(
                        &canonical_bytes(old).map_err(|_| HostFault::InvalidState)?,
                    ))
                    || old.vault_id != self.before.vault_id
                    || old.checkpoint.commit_id != self.before.commit_id
                    || old.checkpoint.commit_sequence.get() != self.before.sequence
                {
                    return Err(HostFault::CheckpointMismatch);
                }
                if self.after.sequence > self.before.sequence {
                    old.advance(
                        &old.primary,
                        &old.checkpoint,
                        self.authority.checkpoint.clone(),
                    )?
                } else {
                    match (&old.phase, &self.authority.phase) {
                        (AuthorityPhase::Active, AuthorityPhase::Quiesced { plan }) => {
                            old.quiesce(&old.primary, plan, &plan.plan_hash)?
                        }
                        (AuthorityPhase::Quiesced { plan }, AuthorityPhase::Active)
                            if old.primary == self.authority.primary =>
                        {
                            old.abort(&old.primary, plan, &plan.plan_hash)?
                        }
                        (AuthorityPhase::Quiesced { plan }, AuthorityPhase::Active) => old
                            .promote(
                                &plan.intent.target_host,
                                plan,
                                &plan.plan_hash,
                                &old.checkpoint,
                            )?,
                        _ => return Err(HostFault::InvalidState),
                    }
                }
            }
            _ => return Err(HostFault::InvalidState),
        };
        if expected != self.authority {
            return Err(HostFault::InvalidState);
        }
        Ok(())
    }
}

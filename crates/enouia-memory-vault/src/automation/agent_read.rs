//! Sealed native Owner delegation to Agent reads, never a System substitute.
use super::*;
use enouia_memory_contract::hash::sha256;
use serde::Serialize;
use std::collections::BTreeSet;
use std::sync::Mutex;
use std::time::Instant;
const MAX_TTL_MS: i64 = 5 * 60 * 1000;

/// Native configuration, not a deserializable credential or per-memory review.
#[derive(Clone, Debug, Serialize)]
pub struct AgentReadScope {
    pub subject_id: SubjectId,
    pub project_id: Option<ProjectId>,
    pub source_ids: Vec<SourceId>,
    pub max_sensitivity: Sensitivity,
    pub source_read: bool,
    pub valid_until: Timestamp,
}
/// Issued only by the live Vault after trusted native Owner registration.
/// No serialization/Clone/field constructor; a new Vault cannot adopt it.
pub struct AgentReadPermit {
    instance: Arc<()>,
    principal: ActorRef,
    scope: AgentReadScope,
    policy: PolicyRef,
    last_seen: Mutex<Timestamp>,
    scope_hash: Sha256Hex,
    until: Instant,
    active: AtomicBool,
}
impl AgentReadPermit {
    pub fn principal(&self) -> &ActorRef {
        &self.principal
    }
    pub fn scope_hash(&self) -> &Sha256Hex {
        &self.scope_hash
    }
    pub fn max_sensitivity(&self) -> Sensitivity {
        self.scope.max_sensitivity
    }
    pub fn source_read(&self) -> bool {
        self.scope.source_read
    }
    pub fn revoke(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
    fn check_live(&self, clock: impl FnOnce() -> Result<Timestamp>) -> Result<()> {
        let mut last_seen = self.last_seen.lock().map_err(|_| AutomationFault::Denied)?;
        // Sample the clock while holding this watermark lock. A timestamp
        // sampled before another reader's check must not look like rollback.
        let now = match clock() {
            Ok(now) => now,
            Err(error) => {
                self.revoke();
                return Err(error);
            }
        };
        if !self.active.load(Ordering::SeqCst)
            || Instant::now() >= self.until
            || now < *last_seen
            || now >= self.scope.valid_until
        {
            self.revoke();
            return Err(AutomationFault::Denied);
        }
        *last_seen = now;
        Ok(())
    }
    fn allows(
        &self,
        vault: &AutomationVault,
        commit: &AutomationCommit,
        memory: &MachineMemory,
        stack: &mut BTreeSet<(MemoryId, Revision)>,
    ) -> bool {
        let input = &memory.decision.input;
        let key = (memory.memory.memory_id.clone(), memory.memory.revision);
        if stack.len() >= 128 || !stack.insert(key.clone()) {
            return false;
        }
        let permitted = input.subject_id == self.scope.subject_id
            && input.project_id == self.scope.project_id
            && input.sensitivity <= self.scope.max_sensitivity
            && input
                .input_refs
                .iter()
                .all(|r| self.scope.source_ids.contains(&r.source_id))
            && input.dimensions.as_ref().is_some_and(|d| {
                d.derived_from
                    .iter()
                    .chain(d.reverts_to.iter())
                    .all(|reference| {
                        commit
                            .data
                            .memories
                            .iter()
                            .find(|m| m.target() == *reference)
                            .is_some_and(|parent| {
                                vault
                                    .eligible(commit, parent, self.scope.max_sensitivity, true)
                                    .is_ok()
                                    && self.allows(vault, commit, parent, stack)
                            })
                    })
            });
        stack.remove(&key);
        permitted
    }
}
impl AutomationVault {
    /// Native Owner assertion must be authenticated independently, never from JSON.
    /// Current automation policy is a ceiling, not an Agent/System identity grant.
    pub fn authorize_agent_read(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        mut scope: AgentReadScope,
    ) -> Result<AgentReadPermit> {
        if owner != &self.descriptor.created_by
            || principal.actor_type != ActorType::Agent
            || self.descriptor.layout != AutomationLayout::HostV2
        {
            return Err(AutomationFault::Denied);
        }
        let state = self.inspect()?;
        let now = self.now()?;
        let p = &state.data.policy;
        let duration = scope.valid_until.unix_ms() - now.unix_ms();
        if !p.active
            || now < state.created_at
            || now < p.valid_from
            || now >= p.valid_until
            || !(1..=MAX_TTL_MS).contains(&duration)
            || scope.valid_until > p.valid_until
            || scope.subject_id != p.subject_id
            || scope.project_id != p.project_id
            || scope.max_sensitivity > p.max_sensitivity
            || scope.source_ids.is_empty()
            || scope.source_ids.len() > 128
            || scope.source_ids.iter().collect::<BTreeSet<_>>().len() != scope.source_ids.len()
            || scope
                .source_ids
                .iter()
                .any(|id| !p.source_ids.contains(id) || state.data.deleted_sources.contains(id))
        {
            return Err(AutomationFault::Denied);
        }
        scope.source_ids.sort();
        let scope_hash = sha256(&canonical_bytes(&serde_json::json!({
            "capability":"memory-agent-read-1","vault":self.descriptor.vault_id,
            "principal":principal,"scope":scope,"policy":p.reference(),"nonce":self.ids.random_16()
        }))?);
        Ok(AgentReadPermit {
            instance: self.agent_instance.clone(),
            principal,
            scope,
            policy: p.reference(),
            last_seen: Mutex::new(now),
            scope_hash,
            until: Instant::now() + Duration::from_millis(duration as u64),
            active: AtomicBool::new(true),
        })
    }
    pub fn check_agent_read_live(&self, permit: &AgentReadPermit) -> Result<()> {
        if !Arc::ptr_eq(&permit.instance, &self.agent_instance) {
            return Err(AutomationFault::Denied);
        }
        permit.check_live(|| self.now())
    }

    /// Native projection carrier, not a wire/Agent DTO. Only the Host projector
    /// may export its qualified fields; it must honor permit.source_read().
    /// This type intentionally has no Serialize implementation.
    pub fn authorized_agent_read_data(
        &self,
        permit: &AgentReadPermit,
        selected: Option<&MemoryId>,
    ) -> Result<AuthorizedReadData> {
        if !Arc::ptr_eq(&permit.instance, &self.agent_instance) {
            return Err(AutomationFault::Denied);
        }
        self.read_data_with(
            permit.scope.max_sensitivity,
            selected,
            |commit, _| {
                self.check_agent_read_live(permit)?;
                if commit.data.policy.reference() != permit.policy {
                    permit.revoke();
                    return Err(AutomationFault::Denied);
                }
                Ok(())
            },
            |commit, memory| permit.allows(self, commit, memory, &mut BTreeSet::new()),
        )
    }
}

impl AutomationVault {
    /// Selected source only, with independent native source-read permission.
    pub fn agent_source(
        &self,
        permit: &AgentReadPermit,
        id: &SourceId,
    ) -> Result<Option<(SnapshotBinding, Timestamp, CapturedSource)>> {
        self.check_agent_read_live(permit)?;
        if !permit.source_read() {
            return Err(AutomationFault::Denied);
        }
        self.available()?;
        let _writer = lock::acquire(&self.root, Duration::ZERO)?;
        let (commit, _) = self.current_locked()?;
        let now = self.now()?;
        if self.descriptor.layout != AutomationLayout::HostV2
            || commit.data.policy.reference() != permit.policy
            || !commit.data.policy.active
            || now < commit.created_at
            || now < commit.data.policy.valid_from
            || now >= commit.data.policy.valid_until
        {
            permit.revoke();
            return Err(AutomationFault::Denied);
        }
        if !permit.scope.source_ids.contains(id) || commit.data.deleted_sources.contains(id) {
            return Ok(None);
        }
        let source = commit
            .data
            .sources
            .iter()
            .rev()
            .find(|s| &s.record.source_id == id)
            .filter(|s| {
                s.record.access_policy_id == commit.data.policy.access_policy_id
                    && s.record.sensitivity <= permit.scope.max_sensitivity
            });
        Ok(source.cloned().map(|s| (commit.snapshot(), now, s)))
    }
}

//! Separate native Agent delegation. Never call a System read port as an Agent.
use super::*;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub const INITIALIZED_AGENT_READ_CAPABILITY: &str = "memory-agent-initialized-native-read-1";

pub const QUALIFIED_AGENT_READ_CAPABILITY: &str = "memory-agent-qualified-native-read-1";

// These published profiles predate native imports and typed attachments. Check
// all retained revisions, including deleted/unselected sources: changing a
// selection or deletion barrier cannot downgrade a root's carrier format.
fn original_carrier(state: &State) -> Result<()> {
    if state.sources.values().flatten().any(|s| {
        !matches!(
            s.record.source_kind,
            enouia_memory_contract::source::SourceKind::RuntimeEvent
                | enouia_memory_contract::source::SourceKind::ManualAssertion
        ) || s.record.completeness != enouia_memory_contract::source::Completeness::Complete
            || s.record.evidence_class
                == enouia_memory_contract::common::EvidenceClass::UserConfirmation
            || !s.attachments.is_empty()
    }) {
        return Err(AutomationFault::Denied);
    }
    Ok(())
}

#[derive(Clone, serde::Serialize)]
pub struct SegmentedAgentScope {
    pub subject_id: SubjectId,
    pub project_id: Option<ProjectId>,
    pub policy_ids: Vec<PolicyId>,
    pub source_ids: Vec<SourceId>,
    pub max_sensitivity: Sensitivity,
    pub source_read: bool,
    pub valid_until: Timestamp,
}
pub struct SegmentedAgentPermit {
    layout: AutomationLayout,
    capability: &'static str,
    instance: Arc<()>,
    principal: ActorRef,
    scope: SegmentedAgentScope,
    sources: BTreeSet<SourceId>,
    source_policies: BTreeMap<SourceId, Vec<PolicyId>>,
    policies: BTreeMap<PolicyId, PolicyRef>,
    hash: Sha256Hex,
    active: AtomicBool,
    until: Instant,
    last_seen: Mutex<Timestamp>,
}
impl SegmentedAgentPermit {
    pub fn capability(&self) -> &'static str {
        self.capability
    }
    pub fn principal(&self) -> &ActorRef {
        &self.principal
    }
    pub fn scope_hash(&self) -> &Sha256Hex {
        &self.hash
    }
    pub fn source_read(&self) -> bool {
        self.scope.source_read
    }
    pub fn max_sensitivity(&self) -> Sensitivity {
        self.scope.max_sensitivity
    }
    pub fn revoke(&self) {
        self.active.store(false, Ordering::SeqCst);
    }
    fn live(&self, vault: &SegmentedAutomationVault) -> Result<Timestamp> {
        if !Arc::ptr_eq(&self.instance, &vault.native.agent_instance)
            || self.layout != vault.native.descriptor.layout
        {
            return Err(AutomationFault::Denied);
        }
        let mut last = self.last_seen.lock().map_err(|_| AutomationFault::Denied)?;
        let now = match vault.native.now() {
            Ok(t) => t,
            Err(e) => {
                self.revoke();
                return Err(e);
            }
        };
        if !self.active.load(Ordering::SeqCst)
            || Instant::now() >= self.until
            || now < *last
            || now >= self.scope.valid_until
        {
            self.revoke();
            return Err(AutomationFault::Denied);
        }
        *last = now.clone();
        Ok(now)
    }
    fn policies_current(&self, state: &State) -> bool {
        self.policies.iter().all(|(id, reference)| {
            state
                .policies
                .get(id)
                .is_some_and(|p| p.reference() == *reference)
        })
    }
    fn source_allowed(&self, state: &State, source: &CapturedSource) -> bool {
        self.source_policies
            .get(&source.record.source_id)
            .is_some_and(|policies| {
                policies.iter().any(|id| {
                    state.policies.get(id).is_some_and(|p| {
                        state.source_in_policy(&source.reference(), p, self.scope.max_sensitivity)
                    })
                })
            })
    }
    fn allows(
        &self,
        state: &State,
        descriptor: &AutomationDescriptor,
        entry: &SegmentedMemory,
        memo: &mut BTreeMap<(MemoryId, Revision), bool>,
        depth: usize,
    ) -> bool {
        let m = &entry.memory;
        let i = &m.decision.input;
        let key = (m.memory.memory_id.clone(), m.memory.revision);
        if let Some(allowed) = memo.get(&key) {
            return *allowed;
        }
        memo.insert(key.clone(), false);
        let Some(p) = state.policy(i) else {
            return false;
        };
        let valid = depth < 128
            && !state.deleted_memories.contains(&m.memory.memory_id)
            && self.policies.get(&p.policy_id) == Some(&i.policy)
            && i.actor == descriptor.system_actor
            && i.subject_id == self.scope.subject_id
            && i.project_id == self.scope.project_id
            && i.access_policy_id == p.access_policy_id
            && i.egress_policy_id == p.egress_policy_id
            && i.sensitivity <= self.scope.max_sensitivity.min(p.max_sensitivity)
            && i.input_refs.iter().all(|r| {
                self.sources.contains(&r.source_id)
                    && state.source_in_policy(r, p, i.sensitivity.min(self.scope.max_sensitivity))
            })
            && i.record_fields.as_ref().is_some_and(|f| {
                f.entities.iter().all(|mention| {
                    state.entity_in_policy(
                        &mention.entity_id,
                        p,
                        i.sensitivity.min(self.scope.max_sensitivity),
                    ) && state
                        .entities
                        .get(&mention.entity_id)
                        .and_then(|v| v.last())
                        .is_some_and(|e| {
                            e.evidence
                                .iter()
                                .chain(e.aliases.iter().flat_map(|a| &a.evidence))
                                .all(|q| self.sources.contains(&q.reference.source_id))
                        })
                })
            })
            && i.dimensions.as_ref().is_some_and(|d| {
                d.derived_from.iter().all(|r| {
                    state
                        .memories
                        .get(&r.memory.memory_id)
                        .and_then(|v| v.last())
                        .is_some_and(|v| {
                            v.target == *r && self.allows(state, descriptor, v, memo, depth + 1)
                        })
                }) && d.reverts_to.as_ref().is_none_or(|r| {
                    state
                        .memories
                        .get(&r.memory.memory_id)
                        .and_then(|v| v.iter().find(|m| m.target == *r))
                        .is_some_and(|v| self.allows(state, descriptor, v, memo, depth + 1))
                })
            });
        memo.insert(key, valid);
        valid
    }
}
pub struct AuthorizedSegmentedAgentRead<'a> {
    pub current: AuthorizedSegmentedRead<'a>,
    pub history: Vec<&'a SegmentedMemory>,
}
impl SegmentedAutomationVault {
    /// Native format check on the already leased Vault, before any Agent
    /// projection/count. Does not expose or decode a business request.
    pub fn check_original_agent_carrier(&self) -> Result<()> {
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        original_carrier(&state)
    }
    pub fn authorize_agent_read(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> Result<SegmentedAgentPermit> {
        self.authorize_agent_profile(
            owner,
            principal,
            scope,
            AutomationLayout::SegmentedV1,
            "memory-agent-catalog-read-1",
        )
    }
    /// Trusted native setup only; not an OS authentication/enrollment method.
    pub fn authorize_qualified_agent_read(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> Result<SegmentedAgentPermit> {
        self.authorize_agent_profile(
            owner,
            principal,
            scope,
            AutomationLayout::SegmentedOwnerV1,
            QUALIFIED_AGENT_READ_CAPABILITY,
        )
    }
    /// Explicit native adoption of ADR104 sources; never selected by a packet.
    pub fn authorize_initialized_agent_read(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> Result<SegmentedAgentPermit> {
        self.authorize_agent_profile(
            owner,
            principal,
            scope,
            AutomationLayout::SegmentedOwnerV1,
            INITIALIZED_AGENT_READ_CAPABILITY,
        )
    }
    fn authorize_agent_profile(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        mut scope: SegmentedAgentScope,
        layout: AutomationLayout,
        capability: &'static str,
    ) -> Result<SegmentedAgentPermit> {
        self.native.owner(owner)?;
        if principal.actor_type != ActorType::Agent || self.native.descriptor.layout != layout {
            return Err(AutomationFault::Denied);
        }
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if capability != INITIALIZED_AGENT_READ_CAPABILITY {
            original_carrier(&state)?;
        }
        let now = self.native.now()?;
        let ttl = scope.valid_until.unix_ms() - now.unix_ms();
        if !(1..=300_000).contains(&ttl)
            || scope.policy_ids.is_empty()
            || scope.policy_ids.len() > MAX_SEGMENTED_POLICIES
            || scope.source_ids.is_empty()
            || scope.source_ids.len() > MAX_SEGMENTED_REVISIONS
            || scope.policy_ids.iter().collect::<BTreeSet<_>>().len() != scope.policy_ids.len()
            || scope.source_ids.iter().collect::<BTreeSet<_>>().len() != scope.source_ids.len()
        {
            return Err(AutomationFault::Denied);
        }
        let mut policies = BTreeMap::new();
        let mut covered = BTreeSet::new();
        for id in &scope.policy_ids {
            let p = state.policies.get(id).ok_or(AutomationFault::Denied)?;
            if !p.active
                || now < p.valid_from
                || now >= p.valid_until
                || scope.valid_until > p.valid_until
                || p.subject_id != scope.subject_id
                || p.project_id != scope.project_id
                || scope.max_sensitivity > p.max_sensitivity
            {
                return Err(AutomationFault::Denied);
            }
            policies.insert(id.clone(), p.reference());
            covered.extend(p.source_ids.iter().cloned());
        }
        if scope
            .source_ids
            .iter()
            .any(|id| !covered.contains(id) || state.deleted_sources.contains(id))
        {
            return Err(AutomationFault::Denied);
        }
        scope.policy_ids.sort();
        scope.source_ids.sort();
        let selected: BTreeSet<_> = scope.source_ids.iter().cloned().collect();
        let mut source_policies: BTreeMap<SourceId, Vec<PolicyId>> = BTreeMap::new();
        for id in policies.keys() {
            for source in &state.policies[id].source_ids {
                if selected.contains(source) {
                    source_policies
                        .entry(source.clone())
                        .or_default()
                        .push(id.clone());
                }
            }
        }
        let hash = digest(
            &serde_json::json!({"capability":capability,"layout":layout,"vault":self.native.descriptor.vault_id,"principal":principal,"scope":scope,"policies":policies,"nonce":self.native.ids.random_16()}),
        );
        Ok(SegmentedAgentPermit {
            layout,
            capability,
            instance: self.native.agent_instance.clone(),
            principal,
            sources: scope.source_ids.iter().cloned().collect(),
            source_policies,
            scope,
            policies,
            hash,
            active: AtomicBool::new(true),
            until: Instant::now() + Duration::from_millis(ttl as u64),
            last_seen: Mutex::new(now),
        })
    }
    pub fn check_agent_read_live(&self, permit: &SegmentedAgentPermit) -> Result<()> {
        permit.live(self)?;
        // Connection polling only inspects the already loaded native metadata.
        // Taking the filesystem writer lock here would race legitimate writes
        // between calls. read_agent still checks CURRENT under that lock before
        // projection; this mutex observes native imports atomically.
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        if let Err(e) = if permit.capability == INITIALIZED_AGENT_READ_CAPABILITY {
            Ok(())
        } else {
            original_carrier(&state)
        } {
            permit.revoke();
            return Err(e);
        }
        Ok(())
    }
    /// Agent authority is checked before returning any rows, counts or labels.
    /// The borrowed native carrier has no Serialize implementation.
    pub fn read_agent<T>(
        &self,
        permit: &SegmentedAgentPermit,
        selected: Option<&MemoryId>,
        f: impl FnOnce(AuthorizedSegmentedAgentRead<'_>) -> T,
    ) -> Result<T> {
        permit.live(self)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if let Err(e) = if permit.capability == INITIALIZED_AGENT_READ_CAPABILITY {
            Ok(())
        } else {
            original_carrier(&state)
        } {
            permit.revoke();
            return Err(e);
        }
        let now = permit.live(self)?;
        if !permit.policies_current(&state) {
            permit.revoke();
            return Err(AutomationFault::Denied);
        }
        let mut data =
            state.authorized(&self.native.descriptor, permit.scope.max_sensitivity, now)?;
        let mut memo = BTreeMap::new();
        data.memories
            .retain(|m| permit.allows(&state, &self.native.descriptor, m, &mut memo, 0));
        let history = selected
            .filter(|id| {
                data.memories
                    .iter()
                    .any(|m| &m.memory.memory.memory_id == *id)
            })
            .and_then(|id| state.memories.get(id))
            .map_or_else(Vec::new, |v| {
                v.iter()
                    .filter(|m| permit.allows(&state, &self.native.descriptor, m, &mut memo, 0))
                    .collect()
            });
        data.sources.retain(|_, s| permit.source_allowed(&state, s));
        // Rebuild overlays over the Agent's current view, never filter already
        // computed System aliases/redirects after they have borrowed names.
        data.entities.retain(|_, e| {
            e.evidence
                .iter()
                .chain(e.aliases.iter().flat_map(|a| &a.evidence))
                .all(|q| {
                    data.sources
                        .get(&q.reference.source_id)
                        .is_some_and(|s| s.record.revision == q.reference.source_revision)
                })
        });
        if permit.layout == AutomationLayout::SegmentedOwnerV1 {
            state.refresh_owner_overlays(&mut data, permit.scope.max_sensitivity)?;
        }
        let mut entities: BTreeSet<_> = data
            .memories
            .iter()
            .chain(&history)
            .flat_map(|m| {
                m.memory
                    .decision
                    .input
                    .record_fields
                    .as_ref()
                    .into_iter()
                    .flat_map(|f| f.entities.iter().map(|e| e.entity_id.clone()))
            })
            .collect();
        entities.extend(data.entity_redirects.values().cloned());
        data.entities.retain(|id, _| entities.contains(id));
        // Conversation surfaces are not part of this Agent view. Do not carry
        // unselected native metadata/turn ordinals into a future adapter.
        data.conversations.clear();
        data.turns.clear();
        permit.live(self)?;
        Ok(f(AuthorizedSegmentedAgentRead {
            current: data,
            history,
        }))
    }
}

//! Separate native Agent delegation. Never call a System read port as an Agent.
use super::*;
use std::collections::BTreeSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

pub const INITIALIZED_AGENT_READ_CAPABILITY: &str = "memory-agent-initialized-native-read-1";
pub use enouia_memory_contract::next::{
    AGENT_CORRECTION_CAPABILITY, AGENT_FORGET_CAPABILITY, AGENT_NOTE_CAPABILITY,
    AGENT_SUPERSEDE_CAPABILITY,
};

/// Independent native delegation for reporting a correction, never Owner action.
pub struct AgentCorrectionPermit {
    read: SegmentedAgentPermit,
    hash: Sha256Hex,
}
impl AgentCorrectionPermit {
    pub fn revoke(&self) {
        self.read.revoke();
    }
    pub(super) fn live(&self, vault: &SegmentedAutomationVault) -> Result<Timestamp> {
        self.read.live(vault)
    }
    pub fn principal(&self) -> &ActorRef {
        self.read.principal()
    }
    /// A delegated policy that currently covers the exact evidence revision.
    pub(super) fn source_policy(
        &self,
        state: &State,
        source: &enouia_memory_contract::common::SourceRevisionRef,
    ) -> Option<PolicyRef> {
        self.read
            .source_policies
            .get(&source.source_id)?
            .iter()
            .filter_map(|id| state.policies.get(id))
            .find(|p| state.source_in_policy(source, p, self.read.scope.max_sensitivity))
            .map(|p| p.reference())
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentCorrectionRequest {
    pub request_id: RequestId,
    pub target: TargetRef,
    pub source: enouia_memory_contract::common::SourceRevisionRef,
    pub quote_hint: String,
    pub correction: String,
}
/// Untrusted report data, not a saved receipt or permission to revise a memory.
#[derive(serde::Serialize)]
pub struct AgentCorrectionDraft {
    pub schema_version: NextVersion,
    pub capability: &'static str,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub report_key: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub target: TargetRef,
    pub evidence: QuotedEvidence,
    pub correction: String,
}
/// Result of the shared authorization gate for preparation and submission.
pub(super) enum CorrectionAdmission {
    Current(Box<AgentCorrectionDraft>),
    /// The exact requested revision is in the currently authorized history of
    /// a currently qualified target; only an already saved key may replay.
    Original(SnapshotBinding),
}

/// Independent native delegation for reporting a supersede, never Owner action.
pub struct AgentSupersedePermit {
    read: SegmentedAgentPermit,
    hash: Sha256Hex,
}
impl AgentSupersedePermit {
    pub fn revoke(&self) {
        self.read.revoke();
    }
    pub(super) fn live(&self, vault: &SegmentedAutomationVault) -> Result<Timestamp> {
        self.read.live(vault)
    }
    pub fn principal(&self) -> &ActorRef {
        self.read.principal()
    }
    /// A delegated policy that currently covers the exact evidence revision.
    pub(super) fn source_policy(
        &self,
        state: &State,
        source: &enouia_memory_contract::common::SourceRevisionRef,
    ) -> Option<PolicyRef> {
        self.read
            .source_policies
            .get(&source.source_id)?
            .iter()
            .filter_map(|id| state.policies.get(id))
            .find(|p| state.source_in_policy(source, p, self.read.scope.max_sensitivity))
            .map(|p| p.reference())
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentSupersedeRequest {
    pub request_id: RequestId,
    pub target: TargetRef,
    pub source: enouia_memory_contract::common::SourceRevisionRef,
    pub quote_hint: String,
    pub supersede: String,
}
/// Untrusted report data, not a saved receipt or permission to revise a memory.
#[derive(serde::Serialize)]
pub struct AgentSupersedeDraft {
    pub schema_version: NextVersion,
    pub capability: &'static str,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub report_key: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub target: TargetRef,
    pub evidence: QuotedEvidence,
    pub supersede: String,
}
/// Result of the shared authorization gate for preparation and submission.
pub(super) enum SupersedeAdmission {
    Current(Box<AgentSupersedeDraft>),
    /// The exact requested revision is in the currently authorized history of
    /// a currently qualified target; only an already saved key may replay.
    Original(SnapshotBinding),
}

pub struct AgentForgetPermit {
    read: SegmentedAgentPermit,
    hash: Sha256Hex,
}
impl AgentForgetPermit {
    pub fn revoke(&self) {
        self.read.revoke();
    }
    pub(super) fn live(&self, vault: &SegmentedAutomationVault) -> Result<Timestamp> {
        self.read.live(vault)
    }
    pub fn principal(&self) -> &ActorRef {
        self.read.principal()
    }
    /// A delegated policy that currently covers the exact evidence revision.
    pub(super) fn source_policy(
        &self,
        state: &State,
        source: &enouia_memory_contract::common::SourceRevisionRef,
    ) -> Option<PolicyRef> {
        self.read
            .source_policies
            .get(&source.source_id)?
            .iter()
            .filter_map(|id| state.policies.get(id))
            .find(|p| state.source_in_policy(source, p, self.read.scope.max_sensitivity))
            .map(|p| p.reference())
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentForgetRequest {
    pub request_id: RequestId,
    pub target: TargetRef,
    pub source: enouia_memory_contract::common::SourceRevisionRef,
    pub quote_hint: String,
    pub forget: String,
}
/// Untrusted report data, not a saved receipt or permission to revise a memory.
pub(super) struct AuthorizedForget {
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub request_key: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub snapshot: SnapshotBinding,
    pub target: TargetRef,
    pub evidence: QuotedEvidence,
    pub forget: String,
}
/// Result of the shared authorization gate for preparation and submission.
pub(super) enum ForgetAdmission {
    Current(Box<AuthorizedForget>),
    /// The exact requested revision is in the currently authorized history of
    /// a currently qualified target; only an already saved key may replay.
    Original(SnapshotBinding),
}

/// Separate native delegation, deliberately not accepted by Agent read APIs.
pub struct AgentNotePermit {
    read: SegmentedAgentPermit,
    hash: Sha256Hex,
}
impl AgentNotePermit {
    pub fn principal(&self) -> &ActorRef {
        self.read.principal()
    }
    pub fn revoke(&self) {
        self.read.revoke();
    }
    pub(super) fn live(&self, vault: &SegmentedAutomationVault) -> Result<Timestamp> {
        self.read.live(vault)
    }
}
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentNoteRequest {
    pub request_id: RequestId,
    pub source: enouia_memory_contract::common::SourceRevisionRef,
    pub quote_hint: String,
    pub note: String,
}
/// Data only: this draft is neither a write capability nor a saved receipt.
#[derive(serde::Serialize)]
pub struct AgentNoteDraft {
    pub schema_version: NextVersion,
    pub capability: &'static str,
    pub request_id: RequestId,
    pub request_hash: Sha256Hex,
    pub note_key: Sha256Hex,
    pub principal: ActorRef,
    pub scope_hash: Sha256Hex,
    pub policy: PolicyRef,
    pub snapshot: SnapshotBinding,
    pub authorized_at: Timestamp,
    pub evidence: QuotedEvidence,
    pub note: String,
}

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
    grant: Option<OwnerReadGrant>,
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
        if let Some(g) = &self.grant {
            return state.read_grants.get(&g.grant_id) == Some(g);
        }
        self.policies.iter().all(|(id, reference)| {
            state
                .policies
                .get(id)
                .is_some_and(|p| p.reference() == *reference)
        })
    }
    fn source_allowed(&self, state: &State, source: &CapturedSource) -> bool {
        if let Some(g) = &self.grant {
            return self.sources.contains(&source.record.source_id)
                && g.policies
                    .iter()
                    .filter(|r| self.scope.policy_ids.contains(&r.policy_id))
                    .filter_map(|r| state.policy_history.get(&r.hash))
                    .any(|p| {
                        state.source_in_policy(&source.reference(), p, self.scope.max_sensitivity)
                    });
        }
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
        let Some(p) = state.read_policy(i, self.grant.is_some()) else {
            return false;
        };
        let valid = depth < 128
            && !state.deleted_memories.contains(&m.memory.memory_id)
            && (if let Some(g) = &self.grant {
                self.scope.policy_ids.contains(&p.policy_id) && g.policies.contains(&i.policy)
            } else {
                self.policies.get(&p.policy_id) == Some(&i.policy)
            })
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
    pub fn authorize_agent_correction(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> Result<AgentCorrectionPermit> {
        if !scope.source_read {
            return Err(AutomationFault::Denied);
        }
        let read = self.authorize_initialized_agent_read(owner, principal, scope)?;
        let hash = digest(&(AGENT_CORRECTION_CAPABILITY, read.scope_hash()));
        Ok(AgentCorrectionPermit { read, hash })
    }
    /// Both target closure and correction source must be currently authorized.
    /// A quoted user statement remains evidence; the Agent interpretation does
    /// not inherit its authority. No state is published by this preparation.
    pub fn prepare_agent_correction(
        &self,
        permit: &AgentCorrectionPermit,
        request: &AgentCorrectionRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<AgentCorrectionDraft> {
        match self.admit_agent_correction(permit, request, locate)? {
            CorrectionAdmission::Current(draft) => Ok(*draft),
            CorrectionAdmission::Original(_) => Err(AutomationFault::Stale),
        }
    }
    pub(super) fn admit_agent_correction(
        &self,
        permit: &AgentCorrectionPermit,
        request: &AgentCorrectionRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<CorrectionAdmission> {
        if request.quote_hint.trim().is_empty()
            || request.quote_hint.len() > 2000
            || request.quote_hint.chars().count() > 500
            || request.correction.trim().is_empty()
            || request.correction.len() > 8192
            || request.correction.chars().count() > 2000
        {
            return Err(AutomationFault::Denied);
        }
        self.read_agent(
            &permit.read,
            Some(&request.target.memory.memory_id),
            |data| {
                let target = data
                    .current
                    .memories
                    .iter()
                    .find(|m| m.target.memory.memory_id == request.target.memory.memory_id)
                    .ok_or(AutomationFault::Denied)?;
                let original = target.target != request.target;
                if original && !data.history.iter().any(|m| m.target == request.target) {
                    return Err(AutomationFault::Stale);
                }
                let source = data
                    .current
                    .sources
                    .get(&request.source.source_id)
                    .ok_or(AutomationFault::Denied)?;
                if source.reference() != request.source
                    || source.record.source_kind
                        != enouia_memory_contract::source::SourceKind::RuntimeEvent
                    || super::runtime_capture::provenance(source).is_none()
                    || source.record.speaker_role
                        != enouia_memory_contract::common::SpeakerRole::User
                    || source.record.evidence_class
                        != enouia_memory_contract::common::EvidenceClass::UserStatement
                    || source.text.len() > 16384
                {
                    return Err(AutomationFault::Denied);
                }
                if original {
                    permit.read.live(self)?;
                    return Ok(CorrectionAdmission::Original(data.current.snapshot));
                }
                let evidence =
                    locate(source, &request.quote_hint).ok_or(AutomationFault::Denied)?;
                if !evidence.resolves(std::slice::from_ref(*source)) {
                    return Err(AutomationFault::Denied);
                }
                let authorized_at = permit.read.live(self)?;
                Ok(CorrectionAdmission::Current(Box::new(
                    AgentCorrectionDraft {
                        schema_version: NextVersion,
                        capability: AGENT_CORRECTION_CAPABILITY,
                        request_id: request.request_id.clone(),
                        request_hash: digest(request),
                        report_key: agent_correction_key(
                            &self.descriptor().vault_id,
                            permit.read.principal(),
                            &request.request_id,
                        ),
                        principal: permit.read.principal().clone(),
                        scope_hash: permit.hash.clone(),
                        snapshot: data.current.snapshot,
                        authorized_at,
                        target: request.target.clone(),
                        evidence,
                        correction: request.correction.clone(),
                    },
                )))
            },
        )?
    }
    /// Metadata-only correction connection poll; submission owns target/source checks.
    pub fn check_agent_correction_live(&self, permit: &AgentCorrectionPermit) -> Result<()> {
        self.native.available()?;
        permit.live(self)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        if !permit.read.policies_current(&state) {
            permit.revoke();
            return Err(AutomationFault::Denied);
        }
        Ok(())
    }
    /// Metadata-only connection poll; submission owns CURRENT/source checks.
    pub fn check_agent_note_live(&self, permit: &AgentNotePermit) -> Result<()> {
        self.native.available()?;
        permit.live(self)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        if !permit.read.policies_current(&state) {
            permit.revoke();
            return Err(AutomationFault::Denied);
        }
        Ok(())
    }
    /// Explicit trusted native Owner delegation; existing read permits do not
    /// authorize note submission, and this preparation route writes nothing.
    pub fn authorize_agent_note(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> Result<AgentNotePermit> {
        if !scope.source_read {
            return Err(AutomationFault::Denied);
        }
        let read = self.authorize_initialized_agent_read(owner, principal, scope)?;
        let hash = digest(&(AGENT_NOTE_CAPABILITY, read.scope_hash()));
        Ok(AgentNotePermit { read, hash })
    }
    /// The locator runs only on the exact authorized source. Its output is
    /// untrusted and must independently resolve before a draft is returned.
    pub fn prepare_agent_note(
        &self,
        permit: &AgentNotePermit,
        request: &AgentNoteRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<AgentNoteDraft> {
        if request.quote_hint.trim().is_empty()
            || request.quote_hint.len() > 2000
            || request.quote_hint.chars().count() > 500
            || request.note.trim().is_empty()
            || request.note.len() > 8192
            || request.note.chars().count() > 2000
        {
            return Err(AutomationFault::Denied);
        }
        let read = &permit.read;
        permit.live(self)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        if !read.policies_current(&state) {
            read.revoke();
            return Err(AutomationFault::Denied);
        }
        let source = state
            .source(&request.source)
            .ok_or(AutomationFault::Denied)?;
        if !read.source_allowed(&state, source)
            || source.record.source_kind != enouia_memory_contract::source::SourceKind::RuntimeEvent
            || super::runtime_capture::provenance(source).is_none()
            || source.text.len() > 16384
            || state.sources[&request.source.source_id]
                .last()
                .unwrap()
                .reference()
                != request.source
        {
            return Err(AutomationFault::Denied);
        }
        let evidence = locate(source, &request.quote_hint).ok_or(AutomationFault::Denied)?;
        if !evidence.resolves(std::slice::from_ref(source)) {
            return Err(AutomationFault::Denied);
        }
        let policy = read.source_policies[&request.source.source_id]
            .iter()
            .map(|id| &state.policies[id])
            .find(|p| state.source_in_policy(&request.source, p, read.scope.max_sensitivity))
            .unwrap()
            .reference();
        let authorized_at = permit.live(self)?;
        Ok(AgentNoteDraft {
            schema_version: NextVersion,
            capability: AGENT_NOTE_CAPABILITY,
            request_id: request.request_id.clone(),
            request_hash: digest(request),
            note_key: agent_note_key(
                &self.descriptor().vault_id,
                read.principal(),
                &request.request_id,
            ),
            principal: read.principal().clone(),
            scope_hash: permit.hash.clone(),
            policy,
            snapshot: state.snapshot()?,
            authorized_at,
            evidence,
            note: request.note.clone(),
        })
    }
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
            None,
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
            None,
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
            None,
        )
    }
    /// Explicit opt-in authority; retains the initialized read carrier only.
    pub fn authorize_owner_grant_read(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        grant_id: &PolicyId,
        scope: SegmentedAgentScope,
    ) -> Result<SegmentedAgentPermit> {
        self.authorize_agent_profile(
            owner,
            principal,
            scope,
            AutomationLayout::SegmentedOwnerV1,
            INITIALIZED_AGENT_READ_CAPABILITY,
            Some(grant_id),
        )
    }
    fn authorize_agent_profile(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        mut scope: SegmentedAgentScope,
        layout: AutomationLayout,
        capability: &'static str,
        grant_id: Option<&PolicyId>,
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
        let grant = grant_id
            .map(|id| {
                state
                    .read_grants
                    .get(id)
                    .cloned()
                    .ok_or(AutomationFault::Denied)
            })
            .transpose()?;
        if let Some(g) = &grant
            && (!g.active
                || now < g.valid_from
                || now >= g.valid_until
                || scope.valid_until > g.valid_until
                || g.principal != principal
                || g.subject_id != scope.subject_id
                || g.project_id != scope.project_id
                || scope.max_sensitivity > g.max_sensitivity
                || (scope.source_read && !g.source_read)
                || scope.source_ids.iter().any(|id| !g.source_ids.contains(id)))
        {
            return Err(AutomationFault::Denied);
        }
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
        let candidates: Vec<_> = if let Some(g) = &grant {
            g.policies
                .iter()
                .filter_map(|r| state.policy_history.get(&r.hash))
                .collect()
        } else {
            state.policies.values().collect()
        };
        for id in &scope.policy_ids {
            let matching: Vec<_> = candidates
                .iter()
                .copied()
                .filter(|p| &p.policy_id == id)
                .collect();
            if matching.is_empty() {
                return Err(AutomationFault::Denied);
            }
            for p in matching {
                if (grant.is_none()
                    && (!p.active
                        || now < p.valid_from
                        || now >= p.valid_until
                        || scope.valid_until > p.valid_until))
                    || p.subject_id != scope.subject_id
                    || p.project_id != scope.project_id
                    || scope.max_sensitivity > p.max_sensitivity
                {
                    return Err(AutomationFault::Denied);
                }
                policies.insert(id.clone(), p.reference());
                covered.extend(p.source_ids.iter().cloned());
            }
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
        for p in &candidates {
            if !policies.contains_key(&p.policy_id) {
                continue;
            }
            for source in &p.source_ids {
                if selected.contains(source) {
                    source_policies
                        .entry(source.clone())
                        .or_default()
                        .push(p.policy_id.clone());
                }
            }
        }
        let hash = digest(
            &serde_json::json!({"capability":capability,"layout":layout,"vault":self.native.descriptor.vault_id,"principal":principal,"scope":scope,"policies":policies,"read_grant":grant,"nonce":self.native.ids.random_16()}),
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
            grant,
            hash,
            active: AtomicBool::new(true),
            until: Instant::now() + Duration::from_millis(ttl as u64),
            last_seen: Mutex::new(now),
        })
    }
    pub fn check_agent_read_live(&self, permit: &SegmentedAgentPermit) -> Result<()> {
        self.check_agent_connection(permit, false)
    }
    /// Opening catalogs also require the delegated policy revision to be current.
    pub fn check_agent_context_live(&self, permit: &SegmentedAgentPermit) -> Result<()> {
        self.check_agent_connection(permit, true)
    }
    fn check_agent_connection(
        &self,
        permit: &SegmentedAgentPermit,
        current_policies: bool,
    ) -> Result<()> {
        self.native.available()?;
        permit.live(self)?;
        // Connection polling only inspects the already loaded native metadata.
        // Taking the filesystem writer lock here would race legitimate writes
        // between calls. read_agent still checks CURRENT under that lock before
        // projection; this mutex observes native imports atomically.
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        if (current_policies || permit.grant.is_some()) && !permit.policies_current(&state) {
            permit.revoke();
            return Err(AutomationFault::Denied);
        }
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
        let mut data = state.authorized_for(
            &self.native.descriptor,
            permit.scope.max_sensitivity,
            now,
            permit.grant.as_ref(),
        )?;
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

impl SegmentedAutomationVault {
    pub fn authorize_agent_supersede(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> Result<AgentSupersedePermit> {
        if !scope.source_read {
            return Err(AutomationFault::Denied);
        }
        let read = self.authorize_initialized_agent_read(owner, principal, scope)?;
        let hash = digest(&(AGENT_SUPERSEDE_CAPABILITY, read.scope_hash()));
        Ok(AgentSupersedePermit { read, hash })
    }
    /// Both target closure and supersede source must be currently authorized.
    /// A quoted user statement remains evidence; the Agent interpretation does
    /// not inherit its authority. No state is published by this preparation.
    pub fn prepare_agent_supersede(
        &self,
        permit: &AgentSupersedePermit,
        request: &AgentSupersedeRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<AgentSupersedeDraft> {
        match self.admit_agent_supersede(permit, request, locate)? {
            SupersedeAdmission::Current(draft) => Ok(*draft),
            SupersedeAdmission::Original(_) => Err(AutomationFault::Stale),
        }
    }
    pub(super) fn admit_agent_supersede(
        &self,
        permit: &AgentSupersedePermit,
        request: &AgentSupersedeRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<SupersedeAdmission> {
        if request.quote_hint.trim().is_empty()
            || request.quote_hint.len() > 2000
            || request.quote_hint.chars().count() > 500
            || request.supersede.trim().is_empty()
            || request.supersede.len() > 8192
            || request.supersede.chars().count() > 2000
        {
            return Err(AutomationFault::Denied);
        }
        self.read_agent(
            &permit.read,
            Some(&request.target.memory.memory_id),
            |data| {
                let target = data
                    .current
                    .memories
                    .iter()
                    .find(|m| m.target.memory.memory_id == request.target.memory.memory_id)
                    .ok_or(AutomationFault::Denied)?;
                let original = target.target != request.target;
                if original && !data.history.iter().any(|m| m.target == request.target) {
                    return Err(AutomationFault::Stale);
                }
                let source = data
                    .current
                    .sources
                    .get(&request.source.source_id)
                    .ok_or(AutomationFault::Denied)?;
                if source.reference() != request.source
                    || source.record.source_kind
                        != enouia_memory_contract::source::SourceKind::RuntimeEvent
                    || super::runtime_capture::provenance(source).is_none()
                    || source.record.speaker_role
                        != enouia_memory_contract::common::SpeakerRole::User
                    || source.record.evidence_class
                        != enouia_memory_contract::common::EvidenceClass::UserStatement
                    || source.text.len() > 16384
                {
                    return Err(AutomationFault::Denied);
                }
                if original {
                    permit.read.live(self)?;
                    return Ok(SupersedeAdmission::Original(data.current.snapshot));
                }
                let evidence =
                    locate(source, &request.quote_hint).ok_or(AutomationFault::Denied)?;
                if !evidence.resolves(std::slice::from_ref(*source)) {
                    return Err(AutomationFault::Denied);
                }
                let authorized_at = permit.read.live(self)?;
                Ok(SupersedeAdmission::Current(Box::new(AgentSupersedeDraft {
                    schema_version: NextVersion,
                    capability: AGENT_SUPERSEDE_CAPABILITY,
                    request_id: request.request_id.clone(),
                    request_hash: digest(request),
                    report_key: agent_supersede_key(
                        &self.descriptor().vault_id,
                        permit.read.principal(),
                        &request.request_id,
                    ),
                    principal: permit.read.principal().clone(),
                    scope_hash: permit.hash.clone(),
                    snapshot: data.current.snapshot,
                    authorized_at,
                    target: request.target.clone(),
                    evidence,
                    supersede: request.supersede.clone(),
                })))
            },
        )?
    }
    /// Metadata-only supersede connection poll; submission owns target/source checks.
    pub fn check_agent_supersede_live(&self, permit: &AgentSupersedePermit) -> Result<()> {
        self.native.available()?;
        permit.live(self)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        if !permit.read.policies_current(&state) {
            permit.revoke();
            return Err(AutomationFault::Denied);
        }
        Ok(())
    }
}

impl SegmentedAutomationVault {
    pub fn authorize_agent_forget(
        &self,
        owner: &ActorRef,
        principal: ActorRef,
        scope: SegmentedAgentScope,
    ) -> Result<AgentForgetPermit> {
        if !scope.source_read {
            return Err(AutomationFault::Denied);
        }
        let read = self.authorize_initialized_agent_read(owner, principal, scope)?;
        let hash = digest(&(AGENT_FORGET_CAPABILITY, read.scope_hash()));
        Ok(AgentForgetPermit { read, hash })
    }
    pub(super) fn admit_agent_forget(
        &self,
        permit: &AgentForgetPermit,
        request: &AgentForgetRequest,
        locate: impl FnOnce(&CapturedSource, &str) -> Option<QuotedEvidence>,
    ) -> Result<ForgetAdmission> {
        if request.quote_hint.trim().is_empty()
            || request.quote_hint.len() > 2000
            || request.quote_hint.chars().count() > 500
            || request.forget.trim().is_empty()
            || request.forget.len() > 8192
            || request.forget.chars().count() > 2000
        {
            return Err(AutomationFault::Denied);
        }
        self.read_agent(
            &permit.read,
            Some(&request.target.memory.memory_id),
            |data| {
                let target = data
                    .current
                    .memories
                    .iter()
                    .find(|m| m.target.memory.memory_id == request.target.memory.memory_id)
                    .ok_or(AutomationFault::Denied)?;
                let original = target.target != request.target;
                if original && !data.history.iter().any(|m| m.target == request.target) {
                    return Err(AutomationFault::Stale);
                }
                let source = data
                    .current
                    .sources
                    .get(&request.source.source_id)
                    .ok_or(AutomationFault::Denied)?;
                if source.reference() != request.source
                    || source.record.source_kind
                        != enouia_memory_contract::source::SourceKind::RuntimeEvent
                    || super::runtime_capture::provenance(source).is_none()
                    || source.record.speaker_role
                        != enouia_memory_contract::common::SpeakerRole::User
                    || source.record.evidence_class
                        != enouia_memory_contract::common::EvidenceClass::UserStatement
                    || source.text.len() > 16384
                {
                    return Err(AutomationFault::Denied);
                }
                if original {
                    permit.read.live(self)?;
                    return Ok(ForgetAdmission::Original(data.current.snapshot));
                }
                let evidence =
                    locate(source, &request.quote_hint).ok_or(AutomationFault::Denied)?;
                if !evidence.resolves(std::slice::from_ref(*source)) {
                    return Err(AutomationFault::Denied);
                }
                permit.read.live(self)?;
                Ok(ForgetAdmission::Current(Box::new(AuthorizedForget {
                    request_id: request.request_id.clone(),
                    request_hash: digest(request),
                    request_key: agent_forget_key(
                        &self.descriptor().vault_id,
                        permit.read.principal(),
                        &request.request_id,
                    ),
                    principal: permit.read.principal().clone(),
                    scope_hash: permit.hash.clone(),
                    snapshot: data.current.snapshot,

                    target: request.target.clone(),
                    evidence,
                    forget: request.forget.clone(),
                })))
            },
        )?
    }
    /// Metadata-only forget connection poll; submission owns target/source checks.
    pub fn check_agent_forget_live(&self, permit: &AgentForgetPermit) -> Result<()> {
        self.native.available()?;
        permit.live(self)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        if !permit.read.policies_current(&state) {
            permit.revoke();
            return Err(AutomationFault::Denied);
        }
        Ok(())
    }
}

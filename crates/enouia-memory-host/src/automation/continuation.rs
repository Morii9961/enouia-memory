//! Opt-in synthetic continuous egress. No HTTP, secrets, ApprovalRecord or V1 bypass.
use super::*;
use enouia_memory_contract::{common::ActorType, hash::sha256, ids::VaultId};
use enouia_memory_vault::RootPolicy;
use std::sync::RwLock;

pub const CONTINUATION_PATH: &str = "sync-state/core4-continuation.json";
const CONTINUATION_LOCK: &str = "sync-state/core4-continuation.lock";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PauseReason {
    Locked,
    NoPolicy,
    Scope,
    Expired,
    Budget,
    InFlight,
    AlreadyAttempted,
    Unavailable,
}
struct ControlState {
    journal: ContinuationJournal,
    permit: Option<Permit>,
    epoch: u64,
    faulted: bool,
}
struct ControlInner {
    root: ManagedRoot,
    _lease: File,
    clock: Arc<dyn Clock + Send + Sync>,
    state: Mutex<ControlState>,
    jobs: SyntheticJobs,
    policy_gate: RwLock<()>,
}
impl ControlInner {
    fn persist(&self, state: &mut ControlState, journal: ContinuationJournal) -> Result<()> {
        if state.faulted {
            return Err(Fault::StorageUnknown);
        }
        if !journal.validate().is_empty() {
            return Err(Fault::Invalid);
        }
        let bytes = canonical_bytes(&journal).map_err(|_| Fault::Invalid)?;
        if bytes.len() > MAX_CONTINUATION_BYTES {
            return Err(Fault::Capacity);
        }
        if self.root.write_atomic(CONTINUATION_PATH, &bytes).is_err()
            || self
                .root
                .faults()
                .io(enouia_memory_vault::fault::FaultPoint::HostAuthorityPublished)
                .is_err()
        {
            state.faulted = true;
            state.permit = None;
            return Err(Fault::StorageUnknown);
        }
        state.journal = journal;
        Ok(())
    }
    fn reason(
        &self,
        state: &ControlState,
        input: &AdmissionInput,
        destination: &GenerationDestination,
        limits: &Limits,
    ) -> std::result::Result<(), PauseReason> {
        if state.faulted {
            return Err(PauseReason::Unavailable);
        }
        let permit = state.permit.as_ref().ok_or(PauseReason::Locked)?;
        let policy = state.journal.policy.as_ref().ok_or(PauseReason::NoPolicy)?;
        let now_ms = self.clock.now_unix_ms();
        let now = Timestamp::from_unix_ms(now_ms).ok_or(PauseReason::Expired)?;
        if now_ms < state.journal.last_ms
            || now >= policy.valid_until
            || now_ms < permit.from.unix_ms()
            || now_ms >= permit.until.unix_ms()
        {
            return Err(PauseReason::Expired);
        }
        if !policy.covers(input, destination, &now) || !permit.matches(input) {
            return Err(PauseReason::Scope);
        }
        if state.journal.attempts.iter().any(|a| {
            a.work_key == admission_batch_key(std::slice::from_ref(input))
                || a.operation_id == input.operation_id
        }) {
            return Err(PauseReason::AlreadyAttempted);
        }
        if state
            .journal
            .attempts
            .iter()
            .any(|a| a.state == ContinuationState::Running)
        {
            return Err(PauseReason::InFlight);
        }
        let (tokens, cost) = state.journal.charges().ok_or(PauseReason::Budget)?;
        if limits.call_tokens > policy.call_tokens
            || limits.call_cost_microusd > policy.call_cost_microusd
            || tokens
                .checked_add(limits.call_tokens)
                .is_none_or(|t| t > policy.total_tokens)
            || cost
                .checked_add(limits.call_cost_microusd)
                .is_none_or(|c| c > policy.total_cost_microusd)
            || state.journal.attempts.len() >= MAX_CONTINUATION_ATTEMPTS
        {
            return Err(PauseReason::Budget);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ContinuousPolicyHost {
    inner: Arc<ControlInner>,
}
impl ContinuousPolicyHost {
    /// Native caller authenticates binding and root selection. Reopen is locked.
    pub fn open(
        jobs: SyntheticJobs,
        vault_id: VaultId,
        owner: ActorRef,
        system: ActorRef,
        clock: Arc<dyn Clock + Send + Sync>,
        faults: Faults,
    ) -> Result<Self> {
        let verified =
            enouia_memory_vault::verify_data_root(jobs.inner.root.root(), &RootPolicy::default())
                .map_err(|_| Fault::Unavailable)?;
        let managed = ManagedRoot::new(verified.path(), faults);
        if managed.exists("vault").map_err(|_| Fault::Unavailable)? {
            return Err(Fault::Unavailable);
        }
        let lease = managed
            .open_lock_file(CONTINUATION_LOCK)
            .map_err(|_| Fault::Unavailable)?;
        lease.try_lock().map_err(|_| Fault::Busy)?;
        let now = clock.now_unix_ms();
        let mut journal = match managed
            .read_bounded(CONTINUATION_PATH, MAX_CONTINUATION_BYTES)
            .map_err(|_| Fault::Unavailable)?
        {
            Some(bytes) => {
                parse_document::<ContinuationJournal>(&bytes).map_err(|_| Fault::Invalid)?
            }
            None => ContinuationJournal {
                schema_version: NextVersion,
                kind: ContinuationJournalKind::ContinuousGenerationJournal,
                vault_id: vault_id.clone(),
                owner: owner.clone(),
                system_actor: system.clone(),
                policy: None,
                policy_history: vec![],
                last_ms: now,
                attempts: vec![],
            },
        };
        if journal.vault_id != vault_id
            || journal.owner != owner
            || journal.system_actor != system
            || now < journal.last_ms
        {
            return Err(Fault::Conflict);
        }
        for attempt in &mut journal.attempts {
            if attempt.state == ContinuationState::Running {
                attempt.state = ContinuationState::OutcomeUnknown;
            }
        }
        journal.last_ms = now;
        if !journal.validate().is_empty() {
            return Err(Fault::Invalid);
        }
        managed
            .write_atomic(
                CONTINUATION_PATH,
                &canonical_bytes(&journal).map_err(|_| Fault::Invalid)?,
            )
            .map_err(|_| Fault::StorageUnknown)?;
        Ok(Self {
            inner: Arc::new(ControlInner {
                root: managed,
                _lease: lease,
                clock,
                state: Mutex::new(ControlState {
                    journal,
                    permit: None,
                    epoch: 0,
                    faulted: false,
                }),
                jobs,
                policy_gate: RwLock::new(()),
            }),
        })
    }
    pub fn set_policy(&self, owner: &ActorRef, policy: ContinuationPolicy) -> Result<()> {
        let _gate = self
            .inner
            .policy_gate
            .write()
            .map_err(|_| Fault::Unavailable)?;
        {
            let state = self.inner.state.lock().map_err(|_| Fault::Unavailable)?;
            if owner != &state.journal.owner
                || !policy.validate().is_empty()
                || policy.granted_by != state.journal.owner
                || policy.system_actor != state.journal.system_actor
                || policy.vault_id != state.journal.vault_id
            {
                return Err(Fault::Invalid);
            }
            if state.journal.policy.as_ref() == Some(&policy) {
                return Ok(());
            }
        }
        self.inner.jobs.lock()?;
        let mut state = self.inner.state.lock().map_err(|_| Fault::Unavailable)?;
        let journal = &state.journal;
        if owner != &journal.owner
            || owner.actor_type != ActorType::Owner
            || !policy.validate().is_empty()
            || policy.granted_by != journal.owner
            || policy.system_actor != journal.system_actor
            || policy.vault_id != journal.vault_id
        {
            return Err(Fault::Invalid);
        }
        if journal.policy.as_ref() == Some(&policy) {
            return Ok(());
        }
        if journal
            .policy
            .as_ref()
            .map_or(policy.revision.get() != 1, |old| {
                old.policy_id != policy.policy_id || old.revision.get() + 1 != policy.revision.get()
            })
        {
            return Err(Fault::Conflict);
        }
        let now = self.inner.clock.now_unix_ms();
        if now < journal.last_ms {
            return Err(Fault::Expired);
        }
        let mut next = journal.clone();
        next.policy_history.push(policy.clone());
        next.policy = Some(policy);
        next.last_ms = now;
        for a in &mut next.attempts {
            if a.state == ContinuationState::Running {
                a.state = ContinuationState::Revoked;
            }
        }
        self.inner.persist(&mut state, next)?;
        state.epoch += 1;
        state.permit = None;
        Ok(())
    }
    pub fn activate(&self, context: &EvaluationContext<'_>) -> Result<()> {
        let permit = Permit::from_context(context)?;
        let _gate = self
            .inner
            .policy_gate
            .write()
            .map_err(|_| Fault::Unavailable)?;
        {
            let state = self.inner.state.lock().map_err(|_| Fault::Unavailable)?;
            if permit.actor != state.journal.system_actor
                || permit.snapshot.vault_id != state.journal.vault_id
            {
                return Err(Fault::Invalid);
            }
            if state.permit.as_ref() == Some(&permit) && !state.faulted {
                return Ok(());
            }
        }
        self.inner.jobs.lock()?;
        let mut state = self.inner.state.lock().map_err(|_| Fault::Unavailable)?;
        if state.faulted
            || permit.actor != state.journal.system_actor
            || permit.snapshot.vault_id != state.journal.vault_id
        {
            return Err(Fault::Unavailable);
        }
        if state.permit.as_ref() != Some(&permit) {
            self.invalidate(&mut state)?;
            state.permit = Some(permit);
        }
        Ok(())
    }
    fn invalidate(&self, state: &mut ControlState) -> Result<()> {
        state.permit = None;
        state.epoch += 1;
        let mut journal = state.journal.clone();
        let now = self.inner.clock.now_unix_ms();
        if now < journal.last_ms {
            return Err(Fault::Expired);
        }
        journal.last_ms = now;
        for a in &mut journal.attempts {
            if a.state == ContinuationState::Running {
                a.state = ContinuationState::Revoked;
            }
        }
        self.inner.persist(state, journal)
    }
    pub fn lock(&self) -> Result<()> {
        let _gate = self
            .inner
            .policy_gate
            .write()
            .map_err(|_| Fault::Unavailable)?;
        self.inner.jobs.lock()?;
        let mut state = self.inner.state.lock().map_err(|_| Fault::Unavailable)?;
        self.invalidate(&mut state)
    }
    pub(super) fn owns_jobs(&self, jobs: &SyntheticJobs) -> bool {
        Arc::ptr_eq(&self.inner.jobs.inner, &jobs.inner)
    }
    pub(super) fn with_application<T>(
        &self,
        bindings: &[GenerationBinding],
        action: impl FnOnce() -> T,
    ) -> Result<T> {
        let _gate = self
            .inner
            .policy_gate
            .read()
            .map_err(|_| Fault::Unavailable)?;
        {
            let state = self.inner.state.lock().map_err(|_| Fault::Unavailable)?;
            let now = self.inner.clock.now_unix_ms();
            let policy = state.journal.policy.as_ref().ok_or(Fault::Unavailable)?;
            let permit = state.permit.as_ref().ok_or(Fault::Unavailable)?;
            if state.faulted
                || !policy.active
                || now < state.journal.last_ms
                || now < policy.valid_from.unix_ms()
                || now >= policy.valid_until.unix_ms()
                || now < permit.from.unix_ms()
                || now >= permit.until.unix_ms()
                || bindings.is_empty()
                || bindings.iter().any(|binding| {
                    !state.journal.attempts.iter().any(|a| {
                        a.input_hash == binding.input_hash
                            && a.operation_id == binding.operation_id
                            && a.response_hash.as_ref() == Some(&binding.response_hash)
                            && a.state == ContinuationState::Complete
                            && a.policy == policy.reference()
                            && a.snapshot == permit.snapshot
                            && a.tokens_used.is_some_and(|t| t <= a.reserved_tokens)
                            && a.cost_microusd
                                .is_some_and(|c| c <= a.reserved_cost_microusd)
                    })
                })
            {
                return Err(Fault::Unavailable);
            }
        }
        Ok(action())
    }
    pub fn inspect_native(&self) -> Result<ContinuationJournal> {
        let state = self.inner.state.lock().map_err(|_| Fault::Unavailable)?;
        if state.faulted {
            return Err(Fault::StorageUnknown);
        }
        Ok(state.journal.clone())
    }
    pub fn paused(
        &self,
        input: &AdmissionInput,
        destination: &GenerationDestination,
        limits: &Limits,
    ) -> Option<PauseReason> {
        let Ok(state) = self.inner.state.lock() else {
            return Some(PauseReason::Unavailable);
        };
        self.inner.reason(&state, input, destination, limits).err()
    }
    /// Only synthetic transport is executable in Core-4. No account/HTTP adapter.
    pub fn fake_generator(
        &self,
        destination: GenerationDestination,
        generator: impl Generator,
    ) -> Result<ContinuousGenerator> {
        if destination.api != GenerationApi::Synthetic {
            return Err(Fault::Invalid);
        }
        Ok(ContinuousGenerator {
            controller: self.clone(),
            destination,
            transport: Box::new(generator),
        })
    }
}

pub struct ContinuousGenerator {
    controller: ContinuousPolicyHost,
    destination: GenerationDestination,
    transport: Box<dyn Generator>,
}
impl Generator for ContinuousGenerator {
    fn ready(&mut self, input: &AdmissionInput, limits: &Limits) -> bool {
        self.controller
            .paused(input, &self.destination, limits)
            .is_none()
    }
    fn start(
        &mut self,
        input: &AdmissionInput,
        limits: &Limits,
        stop: &StopToken,
    ) -> std::result::Result<Box<dyn PendingGeneration>, Generation> {
        let inner = &self.controller.inner;
        let mut state = inner.state.lock().map_err(|_| Generation::Paused)?;
        if stop.is_cancelled()
            || inner
                .reason(&state, input, &self.destination, limits)
                .is_err()
        {
            return Err(Generation::Paused);
        }
        let mut journal = state.journal.clone();
        let index = journal.attempts.len();
        journal.attempts.push(ContinuationAttempt {
            work_key: admission_batch_key(std::slice::from_ref(input)),
            operation_id: input.operation_id.clone(),
            input_hash: digest(input),
            policy: journal.policy.as_ref().unwrap().reference(),
            snapshot: input.snapshot.clone(),
            state: ContinuationState::Running,
            reserved_tokens: limits.call_tokens,
            reserved_cost_microusd: limits.call_cost_microusd,
            tokens_used: None,
            cost_microusd: None,
            response_hash: None,
        });
        journal.last_ms = inner.clock.now_unix_ms();
        // A failed write has no handoff. Reopen conservatively retains any intent.
        inner
            .persist(&mut state, journal)
            .map_err(|_| Generation::Paused)?;
        let epoch = state.epoch;
        // Native revocation shares this gate. Implementations must be nonblocking.
        let pending = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            self.transport.start(input, limits, stop)
        }))
        .unwrap_or(Err(Generation::Unknown));
        drop(state);
        Ok(Box::new(ContinuousPending {
            controller: self.controller.clone(),
            index,
            epoch,
            pending: Some(pending),
            finished: false,
        }))
    }
}
struct ContinuousPending {
    controller: ContinuousPolicyHost,
    index: usize,
    epoch: u64,
    pending: Option<std::result::Result<Box<dyn PendingGeneration>, Generation>>,
    finished: bool,
}
impl ContinuousPending {
    fn settle(&mut self, state: &mut ControlState, generation: &Generation) -> Result<()> {
        let mut journal = state.journal.clone();
        let attempt = &mut journal.attempts[self.index];
        if attempt.state != ContinuationState::Running {
            self.finished = true;
            return Ok(());
        }
        attempt.state = match generation {
            Generation::Complete(result) => {
                if result.input_hash != attempt.input_hash {
                    ContinuationState::OutcomeUnknown
                } else {
                    attempt.tokens_used = Some(result.tokens_used);
                    attempt.cost_microusd = Some(result.cost_microusd);
                    attempt.response_hash = Some(sha256(result.response_json.as_bytes()));
                    ContinuationState::Complete
                }
            }
            Generation::Refused => ContinuationState::Failed,
            _ => ContinuationState::OutcomeUnknown,
        };
        let now = self.controller.inner.clock.now_unix_ms();
        if now < journal.last_ms {
            return Err(Fault::Expired);
        }
        journal.last_ms = now;
        self.controller.inner.persist(state, journal)?;
        self.finished = true;
        Ok(())
    }
}
impl PendingGeneration for ContinuousPending {
    fn poll(&mut self, stop: &StopToken) -> Option<Generation> {
        let controller = self.controller.clone();
        let mut state = match controller.inner.state.lock() {
            Ok(s) => s,
            Err(_) => return Some(Generation::Unknown),
        };
        let now = controller.inner.clock.now_unix_ms();
        let allowed = !state.faulted
            && state.epoch == self.epoch
            && state.permit.is_some()
            && !stop.is_cancelled()
            && now >= state.journal.last_ms
            && state.journal.policy.as_ref().is_some_and(|p| {
                p.active && now >= p.valid_from.unix_ms() && now < p.valid_until.unix_ms()
            })
            && state
                .permit
                .as_ref()
                .is_some_and(|p| now >= p.from.unix_ms() && now < p.until.unix_ms());
        let result = if !allowed {
            Some(Generation::Unknown)
        } else {
            match self.pending.take().unwrap() {
                Err(result) => Some(result),
                Ok(mut pending) => {
                    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        pending.poll(stop)
                    }))
                    .unwrap_or(Some(Generation::Unknown));
                    self.pending = Some(Ok(pending));
                    result
                }
            }
        };
        result.map(|result| {
            // A transport cannot claim Paused after handoff and obtain a refund.
            let result = if matches!(result,Generation::Paused) { Generation::Unknown } else { result };
            if self.settle(&mut state,&result).is_err() {
                state.faulted = true; state.permit = None; Generation::Unknown
            } else if matches!(&result,Generation::Complete(r) if r.input_hash != state.journal.attempts[self.index].input_hash) {
                Generation::Unknown
            } else { result }
        })
    }
}
impl Drop for ContinuousPending {
    fn drop(&mut self) {
        if !self.finished {
            let controller = self.controller.clone();
            if let Ok(mut state) = controller.inner.state.lock()
                && self.settle(&mut state, &Generation::Unknown).is_err()
            {
                state.faulted = true;
                state.permit = None;
            }
        }
    }
}

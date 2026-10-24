//! Native-only Core-2 -> V2 transaction bridge. Not a page/MCP route.
use super::*;
use enouia_memory_contract::{common::Sensitivity, ids::SourceId};
use enouia_memory_vault::automation::{AutomationFault, AutomationVault};
use std::sync::{RwLock, RwLockReadGuard, RwLockWriteGuard};

#[derive(Debug)]
pub enum ApplicationFault {
    Job(Fault),
    Vault(AutomationFault),
    Invalid,
    Locked,
}
impl From<Fault> for ApplicationFault {
    fn from(e: Fault) -> Self {
        Self::Job(e)
    }
}
impl From<AutomationFault> for ApplicationFault {
    fn from(e: AutomationFault) -> Self {
        Self::Vault(e)
    }
}
pub type ApplicationResult<T> = std::result::Result<T, ApplicationFault>;

pub struct AutomationHost {
    vault: AutomationVault,
    system: ActorRef,
    jobs: SyntheticJobs,
    unlocked: RwLock<bool>,
    continuation: Option<super::continuation::ContinuousPolicyHost>,
}
impl AutomationHost {
    /// Caller authenticates the native System identity separately. Never fill
    /// this from a model, page or MCP packet. No auth transport is added here.
    pub fn from_authenticated_system(
        vault: AutomationVault,
        system: ActorRef,
        jobs: SyntheticJobs,
    ) -> ApplicationResult<Self> {
        if system.actor_type != ActorType::System || system != vault.descriptor().system_actor {
            return Err(ApplicationFault::Invalid);
        }
        vault.inspect()?;
        jobs.lock()?;
        Ok(Self {
            vault,
            system,
            jobs,
            unlocked: RwLock::new(true),
            continuation: None,
        })
    }
    /// Bind before workers start; application shares the policy revoke barrier.
    pub fn with_continuation(
        mut self,
        policy: super::continuation::ContinuousPolicyHost,
    ) -> ApplicationResult<Self> {
        if !policy.owns_jobs(&self.jobs) {
            return Err(ApplicationFault::Invalid);
        }
        self.continuation = Some(policy);
        Ok(self)
    }
    fn gate(&self) -> ApplicationResult<RwLockReadGuard<'_, bool>> {
        let guard = self.unlocked.read().map_err(|_| ApplicationFault::Locked)?;
        if !*guard {
            return Err(ApplicationFault::Locked);
        }
        Ok(guard)
    }
    fn control(&self) -> ApplicationResult<RwLockWriteGuard<'_, bool>> {
        let guard = self
            .unlocked
            .write()
            .map_err(|_| ApplicationFault::Locked)?;
        if !*guard {
            return Err(ApplicationFault::Locked);
        }
        Ok(guard)
    }
    pub fn lock(&self) -> ApplicationResult<()> {
        *self
            .unlocked
            .write()
            .map_err(|_| ApplicationFault::Locked)? = false;
        self.jobs.lock()?;
        Ok(())
    }
    pub fn unlock(&self, system: &ActorRef) -> ApplicationResult<()> {
        if system != &self.system {
            return Err(ApplicationFault::Invalid);
        }
        self.vault.inspect()?;
        *self
            .unlocked
            .write()
            .map_err(|_| ApplicationFault::Locked)? = true;
        Ok(())
    }
    pub fn inspect_native(&self) -> ApplicationResult<AutomationCommit> {
        let _gate = self.gate()?;
        Ok(self.vault.inspect()?)
    }
    pub fn capture_native(&self, source: CapturedSource) -> ApplicationResult<AutomationCurrent> {
        let _gate = self.control()?;
        self.jobs.lock()?;
        Ok(self.vault.capture(&self.system, source)?)
    }
    pub fn set_policy_native(
        &self,
        owner: &ActorRef,
        policy: AutomationPolicy,
    ) -> ApplicationResult<AutomationCurrent> {
        let _gate = self.control()?;
        self.jobs.lock()?;
        Ok(self.vault.set_policy(owner, policy)?)
    }
    pub fn delete_source_native(
        &self,
        owner: &ActorRef,
        id: SourceId,
    ) -> ApplicationResult<AutomationCurrent> {
        let _gate = self.control()?;
        self.jobs.lock()?;
        Ok(self.vault.delete_source(owner, id)?)
    }
    pub fn prepare_submit(&self, inputs: Vec<AdmissionInput>) -> ApplicationResult<Sha256Hex> {
        let _gate = self.gate()?;
        let inputs: Vec<_> = inputs
            .into_iter()
            .map(|input| self.vault.prepare(&self.system, input))
            .collect::<std::result::Result<_, _>>()?;
        let commit = self.vault.inspect()?;
        let now = self.vault.current_time()?;
        with_stored_context(
            &commit.data,
            &commit.snapshot(),
            &self.system,
            &now,
            |context| {
                self.jobs.activate(Permit::from_context(context)?)?;
                let contexts: Vec<_> = (0..inputs.len())
                    .map(|_| EvaluationContext {
                        system_actor: context.system_actor,
                        policy: context.policy,
                        snapshot: context.snapshot,
                        now: context.now,
                        sources: context.sources,
                        targets: context.targets,
                    })
                    .collect();
                let key = self.jobs.submit(inputs, &contexts)?;
                let state = {
                    let state = self.jobs.inner.lock()?;
                    state
                        .journal
                        .jobs
                        .iter()
                        .find(|job| job.key == key)
                        .ok_or(Fault::NotFound)?
                        .state
                };
                if state == JobState::Invalidated {
                    self.jobs.resume(&key, &contexts)?;
                }
                Ok::<_, Fault>(key)
            },
        )
        .map_err(ApplicationFault::from)
    }
    pub fn apply_job(&self, key: &Sha256Hex) -> ApplicationResult<Vec<MemoryReceipt>> {
        let _gate = self.control()?;
        let bindings = self.jobs.generated_bindings(key)?;
        if let Some(receipts) = self.vault.replay_job(&self.system, key, &bindings)? {
            return Ok(receipts);
        }
        if let Some(policy) = &self.continuation {
            return policy
                .with_application(&bindings, || self.apply_generated(key, bindings.clone()))?;
        }
        self.apply_generated(key, bindings)
    }
    fn apply_generated(
        &self,
        key: &Sha256Hex,
        bindings: Vec<GenerationBinding>,
    ) -> ApplicationResult<Vec<MemoryReceipt>> {
        let (inputs, results) = self.jobs.generated_batch(key)?;
        let mut decisions = Vec::new();
        let mut responses = Vec::new();
        for (input, result) in inputs.iter().zip(results) {
            if result.input_hash != digest(input)
                || result.response_hash
                    != enouia_memory_contract::hash::sha256(result.response_json.as_bytes())
            {
                return Err(ApplicationFault::Invalid);
            }
            let decision = parse_document::<DecisionDraft>(result.response_json.as_bytes())
                .map_err(|_| ApplicationFault::Invalid)?;
            check_generation_binding(input, &decision.input)
                .map_err(|_| ApplicationFault::Invalid)?;
            decisions.push(decision);
            responses.push(result.response_json);
        }
        self.jobs.lock()?;
        Ok(self.vault.apply_batch(
            &self.system,
            key.clone(),
            inputs,
            decisions,
            bindings,
            responses,
        )?)
    }
    /// A weak completion callback avoids a Host/lease reference cycle. One
    /// private worker generates and then applies, never waiting inside Vault.
    pub fn start_worker(
        self: &Arc<Self>,
        generator: impl Generator,
    ) -> ApplicationResult<JoinHandle<()>> {
        let weak = Arc::downgrade(self);
        Ok(self.jobs.spawn_with_completion(generator, move |key| {
            if let Some(host) = weak.upgrade() {
                let _ = host.apply_job(&key);
            }
        })?)
    }
    pub fn receipts(&self, key: &Sha256Hex) -> ApplicationResult<Option<Vec<MemoryReceipt>>> {
        let _gate = self.gate()?;
        let bindings = self.jobs.generated_bindings(key)?;
        Ok(self.vault.replay_job(&self.system, key, &bindings)?)
    }
    pub fn cancel(&self, key: &Sha256Hex) -> ApplicationResult<()> {
        let _gate = self.control()?;
        self.jobs.cancel(key)?;
        Ok(())
    }
    pub fn context(
        &self,
        query: &str,
        max: Sensitivity,
        limit: usize,
    ) -> ApplicationResult<String> {
        let _gate = self.gate()?;
        let memories = self.vault.recall(&self.system, query, max, limit)?;
        Ok(memories
            .iter()
            .map(|memory| {
                format!(
                    "Subject stated (not independently verified): {}",
                    memory.decision.input.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n"))
    }
}
impl Drop for AutomationHost {
    fn drop(&mut self) {
        let _ = self.jobs.shutdown();
    }
}

//! ADR-MEM-112: one bounded System formation step over captured jobs. Analysis
//! runs outside every Vault lock; dispatch intent is durable first, so a lost
//! result is recorded and never resent. Application is the ordinary System path.
use super::{formation_queue::QueuedCapture, *};
use enouia_memory_contract::{
    common::EvidenceRef,
    next::{
        EntityMention, FormationAnnotations, FormationMode, FormationOrigin, MemoryRecordFields,
        QuotedEvidence, RecordTime,
    },
};
use serde::{Deserialize, Serialize};
use std::{
    fs::File,
    panic::{AssertUnwindSafe, catch_unwind},
    sync::Arc,
};

const JOURNAL: &str = "sync-state/captured-formation-1.json";
const LEASE: &str = "sync-state/captured-formation-1.lock";
const MAX_JOURNAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_STATEMENTS: usize = 32;

/// Untrusted analysis output. The evaluator decides; this approves nothing.
#[derive(Clone)]
pub struct CaptureStatement {
    pub quote: String,
    pub title: String,
    pub interpretation: Interpretation,
    pub dimensions: FormationDimensions,
    pub event_time: RecordTime,
    pub valid_time: RecordTime,
    pub note: Option<String>,
    pub prompt_version: String,
    pub model_version: Option<String>,
}
pub enum CaptureAnalysis {
    Statements(Vec<CaptureStatement>),
    NoMemory,
    /// Proven no handoff happened; the job stays queued.
    Paused,
    /// A handoff may have happened. Never resent.
    Unknown,
}
/// Native analysis port. Called outside Vault locks; no Agent or wire reaches it.
pub trait CaptureAnalyzer {
    fn analyze(&mut self, job: &QueuedCapture, source: &CapturedSource) -> CaptureAnalysis;
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationJobState {
    Dispatched,
    Forming,
    Formed,
    NoMemory,
    Rejected,
    AnalysisUnknown,
}
impl FormationJobState {
    fn terminal(self) -> bool {
        !matches!(self, Self::Dispatched | Self::Forming)
    }
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkState {
    Queued,
    Prepared,
    Applying,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Work {
    seed: AdmissionInput,
    request: Option<AdmissionInput>,
    response: Option<String>,
    binding: Option<GenerationBinding>,
    state: WorkState,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Job {
    job_key: Sha256Hex,
    state: FormationJobState,
    work: Vec<Work>,
    receipts: Vec<MemoryReceipt>,
    refused: usize,
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Journal {
    version: u32,
    vault_id: VaultId,
    descriptor_hash: Sha256Hex,
    jobs: Vec<Job>,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FormationJobStatus {
    pub job_key: Sha256Hex,
    pub state: FormationJobState,
    pub receipts: Vec<MemoryReceipt>,
    pub refused: usize,
}
pub struct PreparedFormation {
    pub request: AdmissionInput,
    pub response: String,
    pub binding: GenerationBinding,
}

/// Fields shared by every formation producer, so live and backfill records match.
pub struct FormationSeed<'a> {
    pub policy: &'a AutomationPolicy,
    pub source: &'a CapturedSource,
    pub evidence: QuotedEvidence,
    pub snapshot_commit: CommitId,
    pub operation_id: OperationId,
    pub result: MemoryRevisionRef,
    pub title: &'a str,
    pub interpretation: Interpretation,
    pub dimensions: FormationDimensions,
    pub event_time: RecordTime,
    pub valid_time: RecordTime,
    pub entities: Vec<EntityMention>,
    pub prompt_version: &'a str,
    pub model_version: Option<String>,
    pub import_batch_id: Option<ImportId>,
    pub mode: FormationMode,
    pub annotations: Option<FormationAnnotations>,
}
/// Locate an exact quote; unresolvable analysis is refused, never repaired.
pub fn quoted_evidence(source: &CapturedSource, quote: &str) -> Option<QuotedEvidence> {
    let byte_start = source.text.find(quote)?;
    let start = source.text[..byte_start].chars().count() as u32;
    let evidence = QuotedEvidence {
        reference: EvidenceRef {
            source_id: source.record.source_id.clone(),
            source_revision: source.record.revision,
            locator: source.record.locator.clone(),
            object_hash: source.record.anchor_hash().clone(),
            evidence_class: source.record.evidence_class,
            supports: "content".into(),
        },
        quote: quote.into(),
        speaker: source.record.speaker_role,
        start,
        end: start + quote.chars().count() as u32,
    };
    evidence
        .resolves(std::slice::from_ref(source))
        .then_some(evidence)
}
pub fn formation_seed(seed: FormationSeed<'_>) -> Option<AdmissionInput> {
    let (policy, source, quote) = (seed.policy, seed.source, seed.evidence.quote.clone());
    let revision = Revision::new(1)?;
    let input = AdmissionInput {
        schema_version: NextVersion,
        kind: InputKind::AdmissionInput,
        snapshot: SnapshotBinding {
            vault_id: policy.vault_id.clone(),
            base_commit_id: seed.snapshot_commit,
            primary_epoch: revision,
            policy_watermark: revision,
            deletion_watermark: revision,
        },
        operation_id: seed.operation_id,
        actor: policy.system_actor.clone(),
        policy: policy.reference(),
        producer_version: seed.prompt_version.into(),
        model_version: seed.model_version,
        subject_id: policy.subject_id.clone(),
        project_id: policy.project_id.clone(),
        content: quote.clone(),
        interpretation: seed.interpretation,
        input_refs: vec![source.reference()],
        evidence: vec![seed.evidence.reference.clone()],
        context_source_refs: vec![],
        target: None,
        result: seed.result,
        sensitivity: source.record.sensitivity,
        access_policy_id: policy.access_policy_id.clone(),
        egress_policy_id: policy.egress_policy_id.clone(),
        dimensions: Some(seed.dimensions),
        record_fields: Some(MemoryRecordFields {
            title: if quote.to_lowercase().contains(&seed.title.to_lowercase()) {
                seed.title.into()
            } else {
                quote.trim().chars().take(60).collect()
            },
            content_lang: source.record_fields.as_ref()?.content_lang.clone(),
            event_time: seed.event_time,
            valid_time: seed.valid_time,
            entities: seed.entities,
            evidence: vec![seed.evidence],
            formation: FormationOrigin {
                prompt_version: seed.prompt_version.into(),
                import_batch_id: seed.import_batch_id,
                mode: seed.mode,
            },
            annotations: seed.annotations,
        }),
    };
    input.validate().is_empty().then_some(input)
}
impl SegmentedAutomationVault {
    /// The ordinary System route: evaluation happens here, not in the analyzer.
    pub fn prepare_formation(
        &self,
        system: &ActorRef,
        seed: AdmissionInput,
    ) -> Result<PreparedFormation> {
        let request = self.prepare(system, seed)?;
        let response = serde_json::to_string(&self.evaluate(system, &request)?)
            .map_err(|_| AutomationFault::Unavailable)?;
        Ok(PreparedFormation {
            binding: GenerationBinding {
                operation_id: request.operation_id.clone(),
                input_hash: digest(&request),
                response_hash: sha256(response.as_bytes()),
            },
            request,
            response,
        })
    }
    /// Chain truth for one operation, even if later revoked or deleted. Under
    /// the worker lease after recovery, None proves it was not admitted.
    fn admitted_receipt(
        &self,
        system: &ActorRef,
        binding: &GenerationBinding,
    ) -> Result<Option<MemoryReceipt>> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let Some((job_key, input_hash)) = state.operations.get(&binding.operation_id) else {
            return Ok(None);
        };
        let receipt = state
            .jobs
            .get(job_key)
            .and_then(|(_, receipts)| {
                receipts
                    .iter()
                    .find(|r| r.operation_id == binding.operation_id)
            })
            .ok_or(AutomationFault::Unavailable)?;
        if input_hash != &binding.input_hash || receipt.response_hash != binding.response_hash {
            return Err(AutomationFault::Conflict);
        }
        Ok(Some(receipt.clone()))
    }
    fn queued_input(
        &self,
        system: &ActorRef,
        job: &QueuedCapture,
    ) -> Result<(CapturedSource, AutomationPolicy)> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let source = state.source(&job.source).ok_or(AutomationFault::Denied)?;
        let policy = state
            .policies
            .get(&job.policy.policy_id)
            .ok_or(AutomationFault::Denied)?;
        Ok((source.clone(), policy.clone()))
    }
}

/// A moved head or a policy gap says nothing about the analysis: re-evaluate.
fn retryable(error: &AutomationFault) -> bool {
    match error {
        AutomationFault::Stale => true,
        AutomationFault::Contract(e) => e.rules().iter().any(|rule| {
            matches!(
                *rule,
                "next.snapshot_stale" | "next.policy_inactive" | "next.policy_binding"
            )
        }),
        _ => false,
    }
}
/// The evaluator's verdict on untrusted analysis. Recorded, never a queue stop.
fn refused(error: &AutomationFault) -> bool {
    matches!(
        error,
        AutomationFault::Denied | AutomationFault::Conflict | AutomationFault::Contract(_)
    )
}

pub struct CapturedFormation {
    vault: Arc<SegmentedAutomationVault>,
    system: ActorRef,
    journal: Journal,
    _lease: File,
    poisoned: bool,
}
impl CapturedFormation {
    /// Uses the caller's leased Vault instance; never opens a second writer.
    pub fn open(vault: Arc<SegmentedAutomationVault>, system: &ActorRef) -> Result<Self> {
        if vault.descriptor().layout != AutomationLayout::SegmentedOwnerV1 {
            return Err(AutomationFault::Denied);
        }
        vault.native.system(system)?;
        let lease = vault.native.root.open_lock_file(LEASE)?;
        lease.try_lock().map_err(|_| AutomationFault::Busy)?;
        let descriptor_hash = digest(vault.descriptor());
        let mut journal = match vault.native.root.read_bounded(JOURNAL, MAX_JOURNAL_BYTES)? {
            Some(raw) => {
                let journal: Journal =
                    serde_json::from_slice(&raw).map_err(|_| AutomationFault::Unavailable)?;
                if journal.version != 1
                    || journal.vault_id != vault.descriptor().vault_id
                    || journal.descriptor_hash != descriptor_hash
                {
                    return Err(AutomationFault::Denied);
                }
                journal
            }
            None => Journal {
                version: 1,
                vault_id: vault.descriptor().vault_id.clone(),
                descriptor_hash,
                jobs: vec![],
            },
        };
        // A dispatch with no durable result may have reached a model.
        for job in &mut journal.jobs {
            if job.state == FormationJobState::Dispatched {
                job.state = FormationJobState::AnalysisUnknown;
            }
        }
        let mut this = Self {
            vault,
            system: system.clone(),
            journal,
            _lease: lease,
            poisoned: false,
        };
        this.save()?;
        Ok(this)
    }
    fn save(&mut self) -> Result<()> {
        let written = canonical_bytes(&self.journal)
            .map_err(AutomationFault::from)
            .and_then(|bytes| {
                if bytes.len() > MAX_JOURNAL_BYTES {
                    return Err(AutomationFault::Capacity);
                }
                self.vault
                    .native
                    .root
                    .write_atomic(JOURNAL, &bytes)
                    .map_err(AutomationFault::from)
            });
        if written.is_err() {
            self.poisoned = true;
            return Err(AutomationFault::OutcomeUnknown);
        }
        Ok(())
    }
    pub fn status(&self) -> Vec<FormationJobStatus> {
        self.journal
            .jobs
            .iter()
            .map(|job| FormationJobStatus {
                job_key: job.job_key.clone(),
                state: job.state,
                receipts: job.receipts.clone(),
                refused: job.refused,
            })
            .collect()
    }
    /// One bounded action; false means nothing is dispatchable now.
    pub fn step(&mut self, analyzer: &mut impl CaptureAnalyzer) -> Result<bool> {
        if self.poisoned {
            return Err(AutomationFault::OutcomeUnknown);
        }
        let jobs = self.vault.queued_captures(&self.system)?;
        let next = jobs.into_iter().find_map(|job| {
            let known = self
                .journal
                .jobs
                .iter()
                .position(|j| j.job_key == job.job_key);
            match known {
                Some(index) if self.journal.jobs[index].state.terminal() => None,
                known => Some((job, known)),
            }
        });
        match next {
            None => Ok(false),
            Some((job, None)) => self.dispatch(&job, analyzer),
            Some((_, Some(index))) => self.form(index),
        }
    }
    fn dispatch(
        &mut self,
        job: &QueuedCapture,
        analyzer: &mut impl CaptureAnalyzer,
    ) -> Result<bool> {
        let (source, policy) = self.vault.queued_input(&self.system, job)?;
        self.journal.jobs.push(Job {
            job_key: job.job_key.clone(),
            state: FormationJobState::Dispatched,
            work: vec![],
            receipts: vec![],
            refused: 0,
        });
        self.save()?;
        let analysis = catch_unwind(AssertUnwindSafe(|| analyzer.analyze(job, &source)))
            .unwrap_or(CaptureAnalysis::Unknown);
        let index = self.journal.jobs.len() - 1;
        match analysis {
            CaptureAnalysis::Paused => {
                self.journal.jobs.pop();
                self.save()?;
                return Ok(false);
            }
            CaptureAnalysis::Unknown => {
                self.journal.jobs[index].state = FormationJobState::AnalysisUnknown;
            }
            CaptureAnalysis::NoMemory => {
                self.journal.jobs[index].state = FormationJobState::NoMemory;
            }
            CaptureAnalysis::Statements(statements) if statements.is_empty() => {
                self.journal.jobs[index].state = FormationJobState::NoMemory;
            }
            CaptureAnalysis::Statements(statements) if statements.len() > MAX_STATEMENTS => {
                self.journal.jobs[index].refused = statements.len();
                self.journal.jobs[index].state = FormationJobState::Rejected;
            }
            CaptureAnalysis::Statements(statements) => {
                let seeds: Vec<_> = statements
                    .iter()
                    .map(|s| self.seed(s, &source, &policy))
                    .collect();
                let job = &mut self.journal.jobs[index];
                job.state = FormationJobState::Forming;
                job.refused = seeds.iter().filter(|s| s.is_none()).count();
                job.work = seeds
                    .into_iter()
                    .flatten()
                    .map(|seed| Work {
                        seed,
                        request: None,
                        response: None,
                        binding: None,
                        state: WorkState::Queued,
                    })
                    .collect();
            }
        }
        self.save()?;
        Ok(true)
    }
    fn seed(
        &self,
        statement: &CaptureStatement,
        source: &CapturedSource,
        policy: &AutomationPolicy,
    ) -> Option<AdmissionInput> {
        let ids = &self.vault.native.ids;
        let result = MemoryRevisionRef {
            memory_id: MemoryId::from_random(ids.random_16()),
            revision: Revision::new(1)?,
        };
        formation_seed(FormationSeed {
            policy,
            source,
            evidence: quoted_evidence(source, &statement.quote)?,
            snapshot_commit: CommitId::from_random(ids.random_16()),
            operation_id: OperationId::from_random(ids.random_16()),
            title: &statement.title,
            interpretation: statement.interpretation,
            dimensions: statement.dimensions.clone(),
            event_time: statement.event_time.clone(),
            valid_time: statement.valid_time.clone(),
            entities: vec![],
            prompt_version: &statement.prompt_version,
            model_version: statement.model_version.clone(),
            import_batch_id: None,
            mode: FormationMode::Live,
            annotations: Some(FormationAnnotations {
                memory: result.clone(),
                model_version: statement.model_version.clone(),
                prompt_version: statement.prompt_version.clone(),
                formation_note: statement.note.clone(),
                suggested_tags: vec![],
                tone: None,
                importance_guess_milli: None,
                current_state_note: None,
                suggested_relations: vec![],
            }),
            result,
        })
    }
    fn settle_work(&mut self, index: usize, receipt: Option<MemoryReceipt>) -> Result<bool> {
        let job = &mut self.journal.jobs[index];
        job.work.remove(0);
        match receipt {
            Some(receipt) => job.receipts.push(receipt),
            None => job.refused += 1,
        }
        self.save()?;
        Ok(true)
    }
    fn form(&mut self, index: usize) -> Result<bool> {
        let job = &mut self.journal.jobs[index];
        let Some(work) = job.work.first_mut() else {
            job.state = if job.receipts.is_empty() {
                FormationJobState::Rejected
            } else {
                FormationJobState::Formed
            };
            self.save()?;
            return Ok(true);
        };
        match work.state {
            WorkState::Queued => self.prepare(index),
            WorkState::Prepared => self.apply(index),
            WorkState::Applying => {
                let binding = work.binding.clone().ok_or(AutomationFault::Unavailable)?;
                match self.vault.admitted_receipt(&self.system, &binding)? {
                    Some(receipt) => self.settle_work(index, Some(receipt)),
                    None => self.requeue_work(index),
                }
            }
        }
    }
    fn prepare(&mut self, index: usize) -> Result<bool> {
        let seed = self.journal.jobs[index].work[0].seed.clone();
        match self.vault.prepare_formation(&self.system, seed) {
            Ok(prepared) => {
                let work = &mut self.journal.jobs[index].work[0];
                work.request = Some(prepared.request);
                work.response = Some(prepared.response);
                work.binding = Some(prepared.binding);
                work.state = WorkState::Prepared;
                self.save()?;
                Ok(true)
            }
            Err(error) if retryable(&error) => Ok(true),
            Err(error) if refused(&error) => self.settle_work(index, None),
            Err(error) => Err(error),
        }
    }
    fn apply(&mut self, index: usize) -> Result<bool> {
        let work = &mut self.journal.jobs[index].work[0];
        let (Some(request), Some(response), Some(binding)) = (
            work.request.clone(),
            work.response.clone(),
            work.binding.clone(),
        ) else {
            return Err(AutomationFault::Unavailable);
        };
        work.state = WorkState::Applying;
        self.save()?;
        match self
            .vault
            .apply_batch(&self.system, vec![request], vec![response], vec![binding])
        {
            Ok(mut receipts) => self.settle_work(index, receipts.pop()),
            Err(AutomationFault::Busy) => {
                self.requeue_work(index)?;
                Err(AutomationFault::Busy)
            }
            // A prepared request can name a superseded policy revision. Prepare
            // judges it against the current one, so Denied is not final here.
            Err(AutomationFault::Denied) => self.requeue_work(index),
            Err(error) if retryable(&error) => self.requeue_work(index),
            Err(error) if refused(&error) => self.settle_work(index, None),
            Err(error) => {
                self.poisoned = true;
                Err(error)
            }
        }
    }
    /// Only for proven non-admission: re-evaluates the durable analysis against
    /// the current head with no new model call.
    fn requeue_work(&mut self, index: usize) -> Result<bool> {
        let work = &mut self.journal.jobs[index].work[0];
        work.request = None;
        work.response = None;
        work.binding = None;
        work.state = WorkState::Queued;
        self.save()?;
        Ok(true)
    }
}

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
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Instant,
};

const JOURNAL: &str = "sync-state/captured-formation-1.json";
const LEASE: &str = "sync-state/captured-formation-1.lock";
const MAX_JOURNAL_BYTES: usize = 16 * 1024 * 1024;
const MAX_STATEMENTS: usize = 32;

/// Untrusted analysis output. The evaluator decides; this approves nothing.
#[derive(Clone, Serialize)]
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
    /// Bounded nonblocking handoff only. Never wait for a model or network here.
    fn start(
        &mut self,
        job: &QueuedCapture,
        source: &CapturedSource,
        notes: &[AgentNoteRecord],
        stop: &CaptureStopToken,
    ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis>;
}
/// One bounded nonblocking poll; no Vault locks are held by this interface.
pub trait PendingCaptureAnalysis: Send {
    fn poll(&mut self, stop: &CaptureStopToken) -> Option<CaptureAnalysis>;
}
/// The target's current revision as a correction analyzer sees it.
#[derive(Clone, Serialize)]
pub struct CorrectionTarget {
    pub target: TargetRef,
    pub content: String,
    pub title: String,
    pub interpretation: Interpretation,
    pub dimensions: FormationDimensions,
    pub sensitivity: Sensitivity,
}
/// ADR-MEM-122 native port for saved Agent correction reports. Separate from
/// CaptureAnalyzer so v3 adapters are unchanged. The report text is untrusted.
pub trait CorrectionAnalyzer {
    /// Bounded nonblocking handoff only, as for captured analysis.
    fn start(
        &mut self,
        report: &AgentCorrectionRecord,
        target: &CorrectionTarget,
        source: &CapturedSource,
        stop: &CaptureStopToken,
    ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis>;
}
pub trait SupersedeAnalyzer {
    /// Bounded nonblocking handoff only, as for captured analysis.
    fn start(
        &mut self,
        report: &AgentSupersedeRecord,
        target: &CorrectionTarget,
        source: &CapturedSource,
        stop: &CaptureStopToken,
    ) -> std::result::Result<Box<dyn PendingCaptureAnalysis>, CaptureAnalysis>;
}
#[derive(Default)]
struct StopState {
    cancelled: AtomicBool,
    parent: Option<CaptureStopToken>,
}
#[derive(Clone, Default)]
pub struct CaptureStopToken(Arc<StopState>);
impl CaptureStopToken {
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::SeqCst);
    }
    pub fn is_cancelled(&self) -> bool {
        self.0.cancelled.load(Ordering::SeqCst)
            || self.0.parent.as_ref().is_some_and(Self::is_cancelled)
    }
    fn child(&self) -> Self {
        Self(Arc::new(StopState {
            cancelled: AtomicBool::new(false),
            parent: Some(self.clone()),
        }))
    }
}
struct PendingAnalysis {
    job: QueuedCapture,
    source: CapturedSource,
    policy: AutomationPolicy,
    pending: Box<dyn PendingCaptureAnalysis>,
    stop: CaptureStopToken,
    started: Instant,
}
struct PendingReport {
    job_key: Sha256Hex,
    source: CapturedSource,
    policy: AutomationPolicy,
    target: CorrectionTarget,
    pending: Box<dyn PendingCaptureAnalysis>,
    stop: CaptureStopToken,
    started: Instant,
}
enum CorrectionInput {
    Ready(
        Box<(
            AgentCorrectionRecord,
            CapturedSource,
            AutomationPolicy,
            CorrectionTarget,
        )>,
    ),
    /// Deleted target/source or an inactive/non-covering policy: no analysis.
    Closed,
}

enum SupersedeInput {
    Ready(
        Box<(
            AgentSupersedeRecord,
            CapturedSource,
            AutomationPolicy,
            CorrectionTarget,
        )>,
    ),
    /// Deleted target/source or an inactive/non-covering policy: no analysis.
    Closed,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FormationJobState {
    Queued,
    Dispatched,
    Forming,
    Formed,
    NoMemory,
    Rejected,
    AnalysisUnknown,
}
impl FormationJobState {
    fn terminal(self) -> bool {
        !matches!(self, Self::Queued | Self::Dispatched | Self::Forming)
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
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    note_keys: Vec<Sha256Hex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    report_key: Option<Sha256Hex>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    supersede_key: Option<Sha256Hex>,
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
/// Content-free projection for an authorized Runtime producer.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProducerJobStatus {
    pub job_key: Sha256Hex,
    pub state: FormationJobState,
    pub receipts: usize,
    pub refused: usize,
}
impl SegmentedAutomationVault {
    pub(super) fn captured_job_status(&self, keys: &[Sha256Hex]) -> Result<Vec<ProducerJobStatus>> {
        let journal = self.read_captured_journal()?;
        Ok(keys
            .iter()
            .map(|key| {
                let job = journal
                    .as_ref()
                    .and_then(|j| j.jobs.iter().find(|j| j.job_key == *key));
                ProducerJobStatus {
                    job_key: key.clone(),
                    state: job.map_or(FormationJobState::Queued, |j| j.state),
                    receipts: job.map_or(0, |j| j.receipts.len()),
                    refused: job.map_or(0, |j| j.refused),
                }
            })
            .collect())
    }
    fn read_captured_journal(&self) -> Result<Option<Journal>> {
        self.native
            .root
            .read_bounded(JOURNAL, MAX_JOURNAL_BYTES)?
            .map(|raw| {
                let journal: Journal =
                    serde_json::from_slice(&raw).map_err(|_| AutomationFault::Unavailable)?;
                if journal.version != 1
                    || journal.vault_id != self.descriptor().vault_id
                    || journal.descriptor_hash != digest(self.descriptor())
                {
                    return Err(AutomationFault::Denied);
                }
                Ok(journal)
            })
            .transpose()
    }
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
    ) -> Result<(CapturedSource, AutomationPolicy, Vec<AgentNoteRecord>)> {
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
        let now = self.native.now()?;
        if state.deleted_sources.contains(&job.source.source_id)
            || !policy.active
            || now < policy.valid_from
            || now >= policy.valid_until
            || !state.source_in_policy(&job.source, policy, policy.max_sensitivity)
        {
            return Err(AutomationFault::Denied);
        }
        let mut notes: Vec<_> = state
            .agent_notes
            .values()
            .filter(|(note, _)| {
                note.policy == policy.reference()
                    && note.evidence.reference.source_id == job.source.source_id
                    && note.evidence.reference.source_revision == job.source.source_revision
            })
            .take(8)
            .map(|(note, _)| note.clone())
            .collect();
        while canonical_bytes(&notes)?.len() > 65536 {
            notes.pop();
        }
        Ok((source.clone(), policy.clone(), notes))
    }
    /// The first saved report, in key order, without a journal entry.
    fn next_correction(
        &self,
        system: &ActorRef,
        known: impl Fn(&Sha256Hex) -> bool,
    ) -> Result<Option<(Sha256Hex, Sha256Hex, CorrectionInput)>> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = self.native.now()?;
        let Some((job_key, report)) = state
            .agent_corrections
            .iter()
            .map(|(key, (report, _))| (digest(&("agent-correction-job-1", key)), report))
            .find(|(job_key, _)| !known(job_key))
        else {
            return Ok(None);
        };
        let reference = enouia_memory_contract::common::SourceRevisionRef {
            source_id: report.evidence.reference.source_id.clone(),
            source_revision: report.evidence.reference.source_revision,
        };
        let memory = &report.target.memory.memory_id;
        let current = state
            .memories
            .get(memory)
            .and_then(|v| v.last())
            .filter(|_| !state.deleted_memories.contains(memory));
        let source = state
            .source(&reference)
            .filter(|_| !state.deleted_sources.contains(&reference.source_id));
        let policy = state.policies.get(&report.policy.policy_id).filter(|p| {
            p.active
                && now >= p.valid_from
                && now < p.valid_until
                && state.source_in_policy(&reference, p, p.max_sensitivity)
        });
        let input = current.map(|m| &m.memory.decision.input);
        let (Some(current), Some(source), Some(policy), Some(fields), Some(dimensions)) = (
            current,
            source,
            policy,
            input.and_then(|i| i.record_fields.as_ref()),
            input.and_then(|i| i.dimensions.as_ref()),
        ) else {
            return Ok(Some((
                job_key,
                report.report_key.clone(),
                CorrectionInput::Closed,
            )));
        };
        let input = &current.memory.decision.input;
        let target = CorrectionTarget {
            target: current.target.clone(),
            content: input.content.clone(),
            title: fields.title.clone(),
            interpretation: input.interpretation,
            dimensions: dimensions.clone(),
            sensitivity: input.sensitivity,
        };
        Ok(Some((
            job_key,
            report.report_key.clone(),
            CorrectionInput::Ready(Box::new((
                report.clone(),
                source.clone(),
                policy.clone(),
                target,
            ))),
        )))
    }
    /// The first saved report, in key order, without a journal entry.
    fn next_supersede(
        &self,
        system: &ActorRef,
        known: impl Fn(&Sha256Hex) -> bool,
    ) -> Result<Option<(Sha256Hex, Sha256Hex, SupersedeInput)>> {
        self.native.system(system)?;
        let _writer = lock::acquire(&self.native.root, Duration::ZERO)?;
        let state = self
            .state
            .lock()
            .map_err(|_| AutomationFault::Unavailable)?;
        self.current(&state)?;
        let now = self.native.now()?;
        let Some((job_key, report)) = state
            .agent_supersedes
            .iter()
            .map(|(key, (report, _))| (digest(&("agent-supersede-job-1", key)), report))
            .find(|(job_key, _)| !known(job_key))
        else {
            return Ok(None);
        };
        let reference = enouia_memory_contract::common::SourceRevisionRef {
            source_id: report.evidence.reference.source_id.clone(),
            source_revision: report.evidence.reference.source_revision,
        };
        let memory = &report.target.memory.memory_id;
        let current = state
            .memories
            .get(memory)
            .and_then(|v| v.last())
            .filter(|_| !state.deleted_memories.contains(memory));
        let source = state
            .source(&reference)
            .filter(|_| !state.deleted_sources.contains(&reference.source_id));
        let policy = state.policies.get(&report.policy.policy_id).filter(|p| {
            p.active
                && now >= p.valid_from
                && now < p.valid_until
                && state.source_in_policy(&reference, p, p.max_sensitivity)
        });
        let input = current.map(|m| &m.memory.decision.input);
        let (Some(current), Some(source), Some(policy), Some(fields), Some(dimensions)) = (
            current,
            source,
            policy,
            input.and_then(|i| i.record_fields.as_ref()),
            input.and_then(|i| i.dimensions.as_ref()),
        ) else {
            return Ok(Some((
                job_key,
                report.report_key.clone(),
                SupersedeInput::Closed,
            )));
        };
        let input = &current.memory.decision.input;
        let target = CorrectionTarget {
            target: current.target.clone(),
            content: input.content.clone(),
            title: fields.title.clone(),
            interpretation: input.interpretation,
            dimensions: dimensions.clone(),
            sensitivity: input.sensitivity,
        };
        Ok(Some((
            job_key,
            report.report_key.clone(),
            SupersedeInput::Ready(Box::new((
                report.clone(),
                source.clone(),
                policy.clone(),
                target,
            ))),
        )))
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
    pending: Option<PendingAnalysis>,
    pending_correction: Option<PendingReport>,
    pending_supersede: Option<PendingReport>,
    analysis_timeout: Duration,
    stop: CaptureStopToken,
}
impl Drop for CapturedFormation {
    fn drop(&mut self) {
        self.stop.cancel();
        if let Some(pending) = &self.pending {
            pending.stop.cancel();
        }
        if let Some(pending) = &self.pending_correction {
            pending.stop.cancel();
        }
        if let Some(pending) = &self.pending_supersede {
            pending.stop.cancel();
        }
    }
}
impl CapturedFormation {
    /// Uses the caller's leased Vault instance; never opens a second writer.
    pub fn open(vault: Arc<SegmentedAutomationVault>, system: &ActorRef) -> Result<Self> {
        Self::open_with_timeout(vault, system, Duration::from_secs(30))
    }
    pub fn open_with_timeout(
        vault: Arc<SegmentedAutomationVault>,
        system: &ActorRef,
        timeout: Duration,
    ) -> Result<Self> {
        if timeout.is_zero() || timeout > Duration::from_secs(300) {
            return Err(AutomationFault::Denied);
        }
        if vault.descriptor().layout != AutomationLayout::SegmentedOwnerV1 {
            return Err(AutomationFault::Denied);
        }
        vault.native.system(system)?;
        let lease = vault.native.root.open_lock_file(LEASE)?;
        lease.try_lock().map_err(|_| AutomationFault::Busy)?;
        let descriptor_hash = digest(vault.descriptor());
        let mut journal = vault.read_captured_journal()?.unwrap_or(Journal {
            version: 1,
            vault_id: vault.descriptor().vault_id.clone(),
            descriptor_hash,
            jobs: vec![],
        });
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
            pending: None,
            pending_correction: None,
            pending_supersede: None,
            analysis_timeout: timeout,
            stop: CaptureStopToken::default(),
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
        self.step_with_stop(analyzer, &self.stop.clone())
    }
    pub fn has_pending(&self) -> bool {
        self.pending.is_some()
            || self.pending_correction.is_some()
            || self.pending_supersede.is_some()
    }
    pub fn step_with_stop(
        &mut self,
        analyzer: &mut impl CaptureAnalyzer,
        stop: &CaptureStopToken,
    ) -> Result<bool> {
        if self.poisoned {
            return Err(AutomationFault::OutcomeUnknown);
        }
        let deleted = if stop.is_cancelled() {
            BTreeSet::new()
        } else {
            let _writer = lock::acquire(&self.vault.native.root, Duration::ZERO)?;
            let state = self
                .vault
                .state
                .lock()
                .map_err(|_| AutomationFault::Unavailable)?;
            self.vault.current(&state)?;
            state
                .capture_queue
                .iter()
                .flat_map(|entry| entry.sources.iter().zip(&entry.job_keys))
                .filter(|(source, _)| state.deleted_sources.contains(&source.source_id))
                .map(|(_, key)| key.clone())
                .collect::<BTreeSet<_>>()
        };
        if let Some(mut pending) = self.pending.take() {
            if deleted.contains(&pending.job.job_key) {
                pending.stop.cancel();
                self.journal.jobs.last_mut().unwrap().state = FormationJobState::Rejected;
                self.save()?;
                return Ok(true);
            }
            let expired = pending.started.elapsed() >= self.analysis_timeout;
            let result = if stop.is_cancelled() || pending.stop.is_cancelled() || expired {
                Some(CaptureAnalysis::Unknown)
            } else {
                catch_unwind(AssertUnwindSafe(|| pending.pending.poll(&pending.stop)))
                    .unwrap_or(Some(CaptureAnalysis::Unknown))
            };
            if stop.is_cancelled()
                || pending.stop.is_cancelled()
                || pending.started.elapsed() >= self.analysis_timeout
            {
                pending.stop.cancel();
                return self.finish_analysis(
                    &pending.job,
                    &pending.source,
                    &pending.policy,
                    CaptureAnalysis::Unknown,
                );
            }
            if let Some(result) = result {
                pending.stop.cancel();
                return self.finish_analysis(
                    &pending.job,
                    &pending.source,
                    &pending.policy,
                    result,
                );
            }
            self.pending = Some(pending);
            return Ok(false);
        }
        if stop.is_cancelled() {
            return Ok(false);
        }
        // Deleted sources no longer appear in queued_captures. Finish their
        // durable work without analysis or publication; an interrupted apply
        // still resolves against chain truth so genuine receipts are retained.
        if let Some(index) = self.journal.jobs.iter().position(|job| {
            job.state == FormationJobState::Forming && deleted.contains(&job.job_key)
        }) {
            return match self.journal.jobs[index].work.first() {
                Some(work) if work.state == WorkState::Applying => {
                    let binding = work.binding.clone().ok_or(AutomationFault::Unavailable)?;
                    let receipt = self.vault.admitted_receipt(&self.system, &binding)?;
                    self.settle_work(index, receipt)
                }
                Some(_) => self.settle_work(index, None),
                None => self.form(index),
            };
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
            // Dispatches never overlap; finish_analysis owns the last entry.
            Some((_, None))
                if self.pending_correction.is_some() || self.pending_supersede.is_some() =>
            {
                Ok(false)
            }
            Some((job, None)) => self.dispatch(&job, analyzer, stop),
            Some((_, Some(index))) => self.form(index),
        }
    }
    fn dispatch(
        &mut self,
        job: &QueuedCapture,
        analyzer: &mut impl CaptureAnalyzer,
        stop: &CaptureStopToken,
    ) -> Result<bool> {
        let (source, policy, notes) = match self.vault.queued_input(&self.system, job) {
            Ok(value) => value,
            Err(AutomationFault::Denied) => return Ok(false),
            Err(error) => return Err(error),
        };
        self.journal.jobs.push(Job {
            job_key: job.job_key.clone(),
            note_keys: notes.iter().map(|note| note.note_key.clone()).collect(),
            report_key: None,
            supersede_key: None,
            state: FormationJobState::Dispatched,
            work: vec![],
            receipts: vec![],
            refused: 0,
        });
        self.save()?;
        if stop.is_cancelled() {
            return self.finish_analysis(job, &source, &policy, CaptureAnalysis::Paused);
        }
        let started = Instant::now();
        let pending_stop = stop.child();
        let analysis = catch_unwind(AssertUnwindSafe(|| {
            analyzer.start(job, &source, &notes, &pending_stop)
        }))
        .unwrap_or(Err(CaptureAnalysis::Unknown));
        if stop.is_cancelled() || started.elapsed() >= self.analysis_timeout {
            pending_stop.cancel();
            return self.finish_analysis(job, &source, &policy, CaptureAnalysis::Unknown);
        }
        match analysis {
            Ok(pending) => {
                self.pending = Some(PendingAnalysis {
                    job: job.clone(),
                    source,
                    policy,
                    pending,
                    stop: pending_stop,
                    started,
                });
                Ok(true)
            }
            Err(analysis) => self.finish_analysis(job, &source, &policy, analysis),
        }
    }
    fn finish_analysis(
        &mut self,
        _job: &QueuedCapture,
        source: &CapturedSource,
        policy: &AutomationPolicy,
        analysis: CaptureAnalysis,
    ) -> Result<bool> {
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
            CaptureAnalysis::Statements(statements)
                if statements.len() > MAX_STATEMENTS
                    || serde_json::to_vec(&statements).map_or(true, |b| b.len() > 256 * 1024) =>
            {
                self.journal.jobs[index].refused = statements.len();
                self.journal.jobs[index].state = FormationJobState::Rejected;
            }
            CaptureAnalysis::Statements(statements) => {
                let ids = &self.vault.native.ids;
                let seeds: Vec<_> = statements
                    .iter()
                    .map(|s| {
                        let result = MemoryRevisionRef {
                            memory_id: MemoryId::from_random(ids.random_16()),
                            revision: Revision::new(1)?,
                        };
                        self.seed(s, source, policy, result)
                    })
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
        result: MemoryRevisionRef,
    ) -> Option<AdmissionInput> {
        let ids = &self.vault.native.ids;
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
        let bound = seed.target.is_some();
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
            // prepare rebinds the head itself; Stale there means the bound
            // target moved or vanished, which re-preparing cannot change.
            Err(AutomationFault::Stale) if bound => self.settle_work(index, None),
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
    /// One bounded action over saved correction reports (ADR-MEM-122). The
    /// same journal, lease, timeout and unknown rules as captured analysis;
    /// the ordinary evaluator decides any target-bound revision.
    pub fn step_corrections(
        &mut self,
        analyzer: &mut impl CorrectionAnalyzer,
        stop: &CaptureStopToken,
    ) -> Result<bool> {
        if self.poisoned {
            return Err(AutomationFault::OutcomeUnknown);
        }
        if let Some(mut pending) = self.pending_correction.take() {
            let result = if stop.is_cancelled() || pending.stop.is_cancelled() {
                Some(CaptureAnalysis::Unknown)
            } else {
                catch_unwind(AssertUnwindSafe(|| pending.pending.poll(&pending.stop)))
                    .unwrap_or(Some(CaptureAnalysis::Unknown))
            };
            let result = if stop.is_cancelled()
                || pending.stop.is_cancelled()
                || pending.started.elapsed() >= self.analysis_timeout
            {
                Some(CaptureAnalysis::Unknown)
            } else {
                result
            };
            let Some(result) = result else {
                self.pending_correction = Some(pending);
                return Ok(false);
            };
            pending.stop.cancel();
            return self.finish_report(
                FormationChange::Correction,
                &pending.job_key,
                &pending.source,
                &pending.policy,
                &pending.target,
                result,
            );
        }
        if stop.is_cancelled() || self.pending.is_some() || self.pending_supersede.is_some() {
            return Ok(false);
        }
        if let Some(index) = self
            .journal
            .jobs
            .iter()
            .position(|job| job.report_key.is_some() && job.state == FormationJobState::Forming)
        {
            return self.form(index);
        }
        let journal = &self.journal;
        let Some((job_key, report_key, input)) =
            self.vault.next_correction(&self.system, |key| {
                journal.jobs.iter().any(|job| &job.job_key == key)
            })?
        else {
            return Ok(false);
        };
        let ready = match input {
            CorrectionInput::Ready(ready) => Some(ready),
            CorrectionInput::Closed => None,
        };
        self.journal.jobs.push(Job {
            job_key: job_key.clone(),
            note_keys: vec![],
            report_key: Some(report_key),
            supersede_key: None,
            state: if ready.is_some() {
                FormationJobState::Dispatched
            } else {
                FormationJobState::Rejected
            },
            work: vec![],
            receipts: vec![],
            refused: 0,
        });
        self.save()?;
        let Some(ready) = ready else {
            return Ok(true);
        };
        let (report, source, policy, target) = *ready;
        if stop.is_cancelled() {
            return self.finish_report(
                FormationChange::Correction,
                &job_key,
                &source,
                &policy,
                &target,
                CaptureAnalysis::Paused,
            );
        }
        let started = Instant::now();
        let pending_stop = stop.child();
        let analysis = catch_unwind(AssertUnwindSafe(|| {
            analyzer.start(&report, &target, &source, &pending_stop)
        }))
        .unwrap_or(Err(CaptureAnalysis::Unknown));
        if stop.is_cancelled() || started.elapsed() >= self.analysis_timeout {
            pending_stop.cancel();
            return self.finish_report(
                FormationChange::Correction,
                &job_key,
                &source,
                &policy,
                &target,
                CaptureAnalysis::Unknown,
            );
        }
        match analysis {
            Ok(pending) => {
                self.pending_correction = Some(PendingReport {
                    job_key,
                    source,
                    policy,
                    target,
                    pending,
                    stop: pending_stop,
                    started,
                });
                Ok(true)
            }
            Err(analysis) => self.finish_report(
                FormationChange::Correction,
                &job_key,
                &source,
                &policy,
                &target,
                analysis,
            ),
        }
    }
    pub fn step_supersedes(
        &mut self,
        analyzer: &mut impl SupersedeAnalyzer,
        stop: &CaptureStopToken,
    ) -> Result<bool> {
        if self.poisoned {
            return Err(AutomationFault::OutcomeUnknown);
        }
        if let Some(mut pending) = self.pending_supersede.take() {
            let result = if stop.is_cancelled() || pending.stop.is_cancelled() {
                Some(CaptureAnalysis::Unknown)
            } else {
                catch_unwind(AssertUnwindSafe(|| pending.pending.poll(&pending.stop)))
                    .unwrap_or(Some(CaptureAnalysis::Unknown))
            };
            let result = if stop.is_cancelled()
                || pending.stop.is_cancelled()
                || pending.started.elapsed() >= self.analysis_timeout
            {
                Some(CaptureAnalysis::Unknown)
            } else {
                result
            };
            let Some(result) = result else {
                self.pending_supersede = Some(pending);
                return Ok(false);
            };
            pending.stop.cancel();
            return self.finish_report(
                FormationChange::Update,
                &pending.job_key,
                &pending.source,
                &pending.policy,
                &pending.target,
                result,
            );
        }
        if stop.is_cancelled() || self.pending.is_some() || self.pending_correction.is_some() {
            return Ok(false);
        }
        if let Some(index) =
            self.journal.jobs.iter().position(|job| {
                job.supersede_key.is_some() && job.state == FormationJobState::Forming
            })
        {
            return self.form(index);
        }
        let journal = &self.journal;
        let Some((job_key, supersede_key, input)) =
            self.vault.next_supersede(&self.system, |key| {
                journal.jobs.iter().any(|job| &job.job_key == key)
            })?
        else {
            return Ok(false);
        };
        let ready = match input {
            SupersedeInput::Ready(ready) => Some(ready),
            SupersedeInput::Closed => None,
        };
        self.journal.jobs.push(Job {
            job_key: job_key.clone(),
            note_keys: vec![],
            report_key: None,
            supersede_key: Some(supersede_key),
            state: if ready.is_some() {
                FormationJobState::Dispatched
            } else {
                FormationJobState::Rejected
            },
            work: vec![],
            receipts: vec![],
            refused: 0,
        });
        self.save()?;
        let Some(ready) = ready else {
            return Ok(true);
        };
        let (report, source, policy, target) = *ready;
        if stop.is_cancelled() {
            return self.finish_report(
                FormationChange::Update,
                &job_key,
                &source,
                &policy,
                &target,
                CaptureAnalysis::Paused,
            );
        }
        let started = Instant::now();
        let pending_stop = stop.child();
        let analysis = catch_unwind(AssertUnwindSafe(|| {
            analyzer.start(&report, &target, &source, &pending_stop)
        }))
        .unwrap_or(Err(CaptureAnalysis::Unknown));
        if stop.is_cancelled() || started.elapsed() >= self.analysis_timeout {
            pending_stop.cancel();
            return self.finish_report(
                FormationChange::Update,
                &job_key,
                &source,
                &policy,
                &target,
                CaptureAnalysis::Unknown,
            );
        }
        match analysis {
            Ok(pending) => {
                self.pending_supersede = Some(PendingReport {
                    job_key,
                    source,
                    policy,
                    target,
                    pending,
                    stop: pending_stop,
                    started,
                });
                Ok(true)
            }
            Err(analysis) => self.finish_report(
                FormationChange::Update,
                &job_key,
                &source,
                &policy,
                &target,
                analysis,
            ),
        }
    }
    fn finish_report(
        &mut self,
        change: FormationChange,
        job_key: &Sha256Hex,
        source: &CapturedSource,
        policy: &AutomationPolicy,
        target: &CorrectionTarget,
        analysis: CaptureAnalysis,
    ) -> Result<bool> {
        let index = self
            .journal
            .jobs
            .iter()
            .position(|job| &job.job_key == job_key)
            .ok_or(AutomationFault::Unavailable)?;
        match analysis {
            CaptureAnalysis::Paused => {
                self.journal.jobs.remove(index);
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
            CaptureAnalysis::Statements(statements)
                if statements.len() > 1
                    || serde_json::to_vec(&statements).map_or(true, |b| b.len() > 256 * 1024) =>
            {
                self.journal.jobs[index].refused = statements.len();
                self.journal.jobs[index].state = FormationJobState::Rejected;
            }
            CaptureAnalysis::Statements(statements) => {
                // Bound to the dispatched current revision; never retargeted.
                let seed = (statements[0].dimensions.change == change)
                    .then(|| {
                        let result = MemoryRevisionRef {
                            memory_id: target.target.memory.memory_id.clone(),
                            revision: Revision::new(target.target.memory.revision.get() + 1)?,
                        };
                        let mut input = self.seed(&statements[0], source, policy, result)?;
                        input.target = Some(target.target.clone());
                        input.sensitivity = input.sensitivity.max(target.sensitivity);
                        input.validate().is_empty().then_some(input)
                    })
                    .flatten();
                let job = &mut self.journal.jobs[index];
                match seed {
                    None => {
                        job.refused = 1;
                        job.state = FormationJobState::Rejected;
                    }
                    Some(seed) => {
                        job.state = FormationJobState::Forming;
                        job.work = vec![Work {
                            seed,
                            request: None,
                            response: None,
                            binding: None,
                            state: WorkState::Queued,
                        }];
                    }
                }
            }
        }
        self.save()?;
        Ok(true)
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

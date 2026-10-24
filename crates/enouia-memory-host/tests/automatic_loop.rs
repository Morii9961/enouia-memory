//! End-to-end Core-3 with actual V2 files/leases and fake generation. No owner
//! review, real Provider, private data, Runtime or default Host route.
#![cfg(windows)]
#[path = "automatic_loop/attention_cases.rs"]
mod attention_cases;
#[path = "automatic_loop/continuation_cases.rs"]
mod continuation_cases;
#[path = "automatic_loop/formation_cases.rs"]
mod formation_cases;
#[path = "../../enouia-memory-contract/tests/support/mod.rs"]
mod support;
use enouia_memory_contract::{
    common::*,
    foundation::FakeClock,
    hash::{Sha256Hex, sha256},
    ids::*,
    json::{Revision, canonical_bytes},
    next::*,
    source::{SourceKind, SourceRecord},
    time::Timestamp,
};
use enouia_memory_host::automation::{
    application::{ApplicationFault, AutomationHost},
    *,
};
use enouia_memory_vault::{
    OsIdSource, RootPolicy, VerifiedRoot,
    automation::{AUTOMATION_WITNESS, AutomationFault, AutomationVault},
    fault::{FaultAction, FaultPoint, Faults},
    verify_data_root,
};
use serde_json::{Value, json};
use std::{
    path::PathBuf,
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
static ROOTS: AtomicU64 = AtomicU64::new(0);
const NOW: i64 = 1_792_843_200_000;
fn id(pre: &str, n: u8) -> String {
    format!("{pre}_{n:08x}-0000-4000-8000-000000000001")
}
fn root(label: &str) -> VerifiedRoot {
    let base = std::env::temp_dir().join("enouia-memory-core3-tests");
    std::fs::create_dir_all(&base).unwrap();
    let base = std::fs::canonicalize(base).unwrap();
    let base = PathBuf::from(base.to_string_lossy().trim_start_matches(r"\\?\"));
    let path = base.join(format!(
        "{label}-{}-{}",
        std::process::id(),
        ROOTS.fetch_add(1, Ordering::SeqCst)
    ));
    std::fs::create_dir(&path).unwrap();
    verify_data_root(&path, &RootPolicy::default()).unwrap()
}
fn limits() -> Limits {
    let mut limits: Limits =
        serde_json::from_value(support::fixture("../automation/queued.json")["limits"].clone())
            .unwrap();
    limits.max_jobs = 16;
    limits.total_tokens = 10_000;
    limits.total_cost_microusd = 10_000;
    limits.call_timeout_ms = 2_000;
    limits
}
struct Env {
    root: VerifiedRoot,
    jobs_root: VerifiedRoot,
    clock: Arc<FakeClock>,
    descriptor: AutomationDescriptor,
    policy: AutomationPolicy,
}
impl Env {
    fn new() -> Self {
        let descriptor: AutomationDescriptor =
            serde_json::from_value(support::fixture("../automation-vault/descriptor.json"))
                .unwrap();
        let mut policy: AutomationPolicy =
            serde_json::from_value(support::fixture("../next/policy.json")).unwrap();
        policy.allow_revise_created = true;
        policy.rule_version = RuleVersion::V2;
        policy.source_ids = (1..=12)
            .map(|n| SourceId::parse(&id("src", n)).unwrap())
            .collect();
        Self {
            root: root("data"),
            jobs_root: root("jobs"),
            clock: Arc::new(FakeClock::new(NOW)),
            descriptor,
            policy,
        }
    }
    fn vault(&self, faults: Faults) -> AutomationVault {
        AutomationVault::create(
            &self.root,
            self.descriptor.clone(),
            self.policy.clone(),
            self.clock.clone(),
            Arc::new(OsIdSource),
            faults,
        )
        .unwrap()
    }
    fn open(&self) -> AutomationVault {
        AutomationVault::open(
            &self.root,
            self.descriptor.host_id.clone(),
            self.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap()
    }
    fn jobs(&self) -> SyntheticJobs {
        SyntheticJobs::open(
            &self.jobs_root,
            limits(),
            self.clock.clone(),
            Faults::none(),
        )
        .unwrap()
    }
    fn host(&self, vault: AutomationVault, jobs: SyntheticJobs) -> AutomationHost {
        AutomationHost::from_authenticated_system(vault, self.descriptor.system_actor.clone(), jobs)
            .unwrap()
    }
    fn source(&self, n: u8, text: &str, role: SpeakerRole) -> CapturedSource {
        let mut record: SourceRecord =
            serde_json::from_value(support::fixture("records/source-runtime-event.json")).unwrap();
        record.source_id = SourceId::parse(&id("src", n)).unwrap();
        record.revision = Revision::new(1).unwrap();
        record.source_kind = SourceKind::RuntimeEvent;
        record.raw_object_hash = None;
        record.import_id = None;
        record.provider = None;
        record.account_scope = None;
        record.parser_version = None;
        record.manual_assertion = None;
        record.agent_submission = None;
        record.content_hash = sha256(text.as_bytes());
        record.locator = Locator::RuntimeEvent {
            event_id: EventId::from_random([n; 16]),
        };
        record.speaker_role = role;
        record.evidence_class = if role == SpeakerRole::User {
            EvidenceClass::UserStatement
        } else {
            EvidenceClass::ModelClaim
        };
        record.sensitivity = Sensitivity::Private;
        record.access_policy_id = self.policy.access_policy_id.clone();
        record.captured_at = Timestamp::from_unix_ms(NOW).unwrap();
        record.created_at = record.captured_at.clone();
        CapturedSource {
            record,
            text: text.into(),
            attributed_subject: (role == SpeakerRole::User).then(|| self.policy.subject_id.clone()),
        }
    }
    fn input(&self, n: u8, target: Option<TargetRef>) -> AdmissionInput {
        let mut input: AdmissionInput =
            serde_json::from_value(support::fixture("../next/input.json")).unwrap();
        input.input_refs = vec![SourceRevisionRef {
            source_id: SourceId::parse(&id("src", n)).unwrap(),
            source_revision: Revision::new(1).unwrap(),
        }];
        input.target = target;
        input
    }
}
#[derive(Clone, Copy)]
enum ModelMode {
    Faithful,
    Inference,
    Conflict,
    Owner,
    MissingContext,
    WrongAnchor,
    BadSecond,
    WrongTarget,
    Pending,
}
struct Model {
    mode: ModelMode,
}
struct Pending {
    input: AdmissionInput,
    mode: ModelMode,
}
impl Generator for Model {
    fn start(
        &mut self,
        input: &AdmissionInput,
        _limits: &Limits,
        _stop: &StopToken,
    ) -> std::result::Result<Box<dyn PendingGeneration>, Generation> {
        Ok(Box::new(Pending {
            input: input.clone(),
            mode: self.mode,
        }))
    }
}
impl PendingGeneration for Pending {
    fn poll(&mut self, _stop: &StopToken) -> Option<Generation> {
        if matches!(self.mode, ModelMode::Pending) {
            return None;
        }
        let sources: Vec<CapturedSource> = serde_json::from_str(&self.input.content).unwrap();
        let mut input = self.input.clone();
        input.content = sources[0].text.clone();
        input.interpretation = match self.mode {
            ModelMode::Inference => Interpretation::Inference,
            ModelMode::Conflict => Interpretation::Conflicting,
            _ if sources[0].record.speaker_role == SpeakerRole::User => {
                Interpretation::ClearSelfReport
            }
            _ => Interpretation::Inference,
        };
        let knowledge = match input.interpretation {
            Interpretation::ClearSelfReport => KnowledgeState::AttributedStatement,
            Interpretation::Inference => KnowledgeState::Hypothesis,
            Interpretation::Ambiguous => KnowledgeState::Uncertain,
            Interpretation::Conflicting => KnowledgeState::Disputed,
        };
        let adoption = if knowledge == KnowledgeState::AttributedStatement {
            AdoptionState::Adopted
        } else {
            AdoptionState::Staged
        };
        if matches!(self.mode, ModelMode::Owner) {
            input.actor.actor_type = ActorType::Owner;
        }
        if matches!(self.mode, ModelMode::MissingContext) {
            input.context_source_refs.clear();
        }
        if matches!(self.mode, ModelMode::WrongAnchor)
            || (matches!(self.mode, ModelMode::BadSecond)
                && sources[0].record.source_id == SourceId::parse(&id("src", 2)).unwrap())
        {
            input.evidence[0].object_hash = sha256(b"invented support");
        }
        if matches!(self.mode, ModelMode::WrongTarget) {
            input.result.memory_id = MemoryId::from_random([99; 16]);
        }
        let decision = DecisionDraft {
            schema_version: NextVersion,
            kind: DecisionKind::AdmissionDraft,
            rule_version: RuleVersion::V2,
            input,
            outcome: Outcome {
                knowledge,
                adoption,
                reassessment: (adoption == AdoptionState::Staged)
                    .then_some(Reassessment::OnEvidenceOrPolicyChange),
            },
        };
        Some(Generation::Complete(Box::new(Generated {
            input_hash: digest(&self.input),
            response_json: serde_json::to_string(&decision).unwrap(),
            tokens_used: 5,
            cost_microusd: 1,
        })))
    }
}
fn wait(jobs: &SyntheticJobs, key: &Sha256Hex, state: JobState) {
    let until = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(s) = jobs.status(key)
            && s.state == state
        {
            return;
        }
        assert!(Instant::now() < until, "{:?}", jobs.status(key));
        std::thread::sleep(Duration::from_millis(5));
    }
}
fn worker(jobs: &SyntheticJobs, mode: ModelMode) -> std::thread::JoinHandle<()> {
    jobs.spawn(Model { mode }).unwrap()
}
fn stop(jobs: &SyntheticJobs, thread: std::thread::JoinHandle<()>) {
    jobs.shutdown().unwrap();
    thread.join().unwrap();
}

#[test]
fn sources_form_correct_and_cancel_memories_then_next_session_recalls_after_reopen() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = worker(&jobs, ModelMode::Faithful);
    let first = e.source(1, "I prefer green tea.", SpeakerRole::User);
    host.capture_native(first.clone()).unwrap();
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    let receipt = host.apply_job(&key).unwrap();
    assert_eq!(receipt.len(), 1);
    let context = host.context("tea", Sensitivity::Private, 8).unwrap();
    assert!(context.contains("Subject stated") && context.contains("green tea"));
    assert!(
        !host
            .context("tea", Sensitivity::Public, 8)
            .unwrap()
            .contains("tea")
    );
    assert_eq!(host.apply_job(&key).unwrap(), receipt);
    let target = host
        .inspect_native()
        .unwrap()
        .data
        .memories
        .last()
        .unwrap()
        .target();
    host.capture_native(e.source(
        2,
        "I cancelled the tea tasting plan; it did not happen.",
        SpeakerRole::User,
    ))
    .unwrap();
    let corrected = host.prepare_submit(vec![e.input(2, Some(target))]).unwrap();
    wait(&jobs, &corrected, JobState::Generated);
    let revision = host.apply_job(&corrected).unwrap();
    assert_eq!(revision[0].result.revision.get(), 2);
    let context = host.context("tea", Sensitivity::Private, 8).unwrap();
    assert!(context.contains("cancelled") && !context.contains("green tea"));
    let state = host.inspect_native().unwrap();
    assert_eq!(state.data.memories.len(), 2);
    assert_eq!(state.data.sources[0], first);
    assert!(
        state
            .data
            .memories
            .iter()
            .all(|m| m.decision.input.actor.actor_type == ActorType::System)
    );
    drop(host);
    thread.join().unwrap();
    drop(jobs);
    let jobs = e.jobs();
    let host = e.host(e.open(), jobs.clone());
    assert_eq!(host.apply_job(&corrected).unwrap(), revision);
    assert!(
        host.context("tea", Sensitivity::Private, 8)
            .unwrap()
            .contains("did not happen")
    );
    assert_eq!(host.inspect_native().unwrap().data.memories.len(), 2);
}

#[test]
fn native_replay_rejects_changed_raw_response_before_returning_an_old_receipt() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = worker(&jobs, ModelMode::Faithful);
    host.capture_native(e.source(1, "I prefer green tea.", SpeakerRole::User))
        .unwrap();
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    let receipt = host.apply_job(&key).unwrap();
    let state = host.inspect_native().unwrap();
    let memory = state.data.memories[0].clone();
    let bindings = jobs.generated_bindings(&key).unwrap();
    drop(host);
    thread.join().unwrap();
    drop(jobs);
    let vault = e.open();
    let apply = |response| {
        vault.apply_batch(
            &e.descriptor.system_actor,
            key.clone(),
            vec![memory.request.clone()],
            vec![memory.decision.clone()],
            bindings.clone(),
            vec![response],
        )
    };
    assert!(matches!(
        apply(format!("{} ", memory.response_json)),
        Err(AutomationFault::Conflict)
    ));
    assert_eq!(apply(memory.response_json.clone()).unwrap(), receipt);
    assert_eq!(vault.inspect().unwrap().current(), state.current());
}

#[test]
fn uncertainty_is_staged_then_new_evidence_is_automatically_adopted_without_owner_review() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    host.capture_native(e.source(
        1,
        "The subject might prefer quiet rooms.",
        SpeakerRole::Assistant,
    ))
    .unwrap();
    let thread = worker(&jobs, ModelMode::Inference);
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    assert!(
        host.context("quiet", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    let state = host.inspect_native().unwrap();
    assert_eq!(
        state.data.memories[0].decision.outcome.adoption,
        AdoptionState::Staged
    );
    let target = state.data.memories[0].target();
    stop(&jobs, thread);
    drop(host);
    drop(jobs);
    let jobs = e.jobs();
    let host = e.host(e.open(), jobs.clone());
    let thread = worker(&jobs, ModelMode::Faithful);
    host.capture_native(e.source(2, "I explicitly prefer quiet rooms.", SpeakerRole::User))
        .unwrap();
    let key = host.prepare_submit(vec![e.input(2, Some(target))]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    assert!(
        host.context("quiet", Sensitivity::Private, 8)
            .unwrap()
            .contains("explicitly prefer")
    );
    assert_eq!(host.inspect_native().unwrap().data.memories.len(), 2);
    stop(&jobs, thread);
}

#[test]
fn a_conflicted_new_revision_withholds_the_old_assertion_but_preserves_history() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = worker(&jobs, ModelMode::Faithful);
    host.capture_native(e.source(1, "I plan to travel on Friday.", SpeakerRole::User))
        .unwrap();
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    let target = host.inspect_native().unwrap().data.memories[0].target();
    stop(&jobs, thread);
    drop(host);
    drop(jobs);
    let jobs = e.jobs();
    let host = e.host(e.open(), jobs.clone());
    let thread = worker(&jobs, ModelMode::Conflict);
    host.capture_native(e.source(
        2,
        "The Friday plan is unclear and conflicts with another plan.",
        SpeakerRole::User,
    ))
    .unwrap();
    let key = host.prepare_submit(vec![e.input(2, Some(target))]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    assert!(
        host.context("Friday", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    let state = host.inspect_native().unwrap();
    assert_eq!(
        state.data.memories[0].decision.outcome.adoption,
        AdoptionState::Adopted
    );
    assert_eq!(
        state.data.memories[1].decision.outcome.knowledge,
        KnowledgeState::Disputed
    );
    stop(&jobs, thread);
}

#[test]
fn atomic_batch_has_one_commit_and_bad_second_item_writes_nothing() {
    for bad in [false, true] {
        let e = Env::new();
        let jobs = e.jobs();
        let host = e.host(e.vault(Faults::none()), jobs.clone());
        host.capture_native(e.source(1, "I like blue notebooks.", SpeakerRole::User))
            .unwrap();
        host.capture_native(e.source(2, "I like green pens.", SpeakerRole::User))
            .unwrap();
        let thread = worker(
            &jobs,
            if bad {
                ModelMode::BadSecond
            } else {
                ModelMode::Faithful
            },
        );
        let key = host
            .prepare_submit(vec![e.input(1, None), e.input(2, None)])
            .unwrap();
        wait(&jobs, &key, JobState::Generated);
        let before = host.inspect_native().unwrap().current();
        let result = host.apply_job(&key);
        if bad {
            assert!(result.is_err());
            assert_eq!(host.inspect_native().unwrap().current(), before);
        } else {
            let receipts = result.unwrap();
            assert_eq!(receipts.len(), 2);
            assert_eq!(receipts[0].committed_in, receipts[1].committed_in);
            assert_eq!(host.apply_job(&key).unwrap(), receipts);
        }
        stop(&jobs, thread);
    }
}

#[test]
fn model_cannot_impersonate_owner_omit_context_change_target_or_invent_support() {
    for mode in [
        ModelMode::Owner,
        ModelMode::MissingContext,
        ModelMode::WrongAnchor,
        ModelMode::WrongTarget,
    ] {
        let e = Env::new();
        let jobs = e.jobs();
        let host = e.host(e.vault(Faults::none()), jobs.clone());
        host.capture_native(e.source(1, "A synthetic personal statement.", SpeakerRole::User))
            .unwrap();
        host.capture_native(e.source(
            2,
            "Required context, not supporting evidence.",
            SpeakerRole::Assistant,
        ))
        .unwrap();
        let mut input = e.input(1, None);
        input
            .input_refs
            .push(e.input(2, None).input_refs[0].clone());
        let thread = worker(&jobs, mode);
        let key = host.prepare_submit(vec![input]).unwrap();
        wait(&jobs, &key, JobState::Generated);
        let before = host.inspect_native().unwrap().current();
        assert!(host.apply_job(&key).is_err());
        assert_eq!(host.inspect_native().unwrap().current(), before);
        assert!(host.inspect_native().unwrap().data.receipts.is_empty());
        stop(&jobs, thread);
    }
}

#[test]
fn current_deletion_policy_lock_and_native_control_invalidate_old_jobs() {
    for action in ["source", "policy", "lock", "revision"] {
        let e = Env::new();
        let jobs = e.jobs();
        let host = e.host(e.vault(Faults::none()), jobs.clone());
        let source = e.source(
            1,
            "Current source must remain authorized.",
            SpeakerRole::User,
        );
        host.capture_native(source.clone()).unwrap();
        let thread = worker(&jobs, ModelMode::Faithful);
        let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
        wait(&jobs, &key, JobState::Generated);
        match action {
            "source" => {
                host.delete_source_native(&e.descriptor.created_by, source.record.source_id)
                    .unwrap();
            }
            "policy" => {
                let mut p = e.policy.clone();
                p.revision = Revision::new(2).unwrap();
                p.active = false;
                host.set_policy_native(&e.descriptor.created_by, p).unwrap();
            }
            "lock" => host.lock().unwrap(),
            _ => {
                let mut changed = source;
                changed.record.revision = Revision::new(2).unwrap();
                changed.text.push_str(" edited");
                changed.record.content_hash = sha256(changed.text.as_bytes());
                host.capture_native(changed).unwrap();
            }
        }
        assert!(host.apply_job(&key).is_err());
        assert_ne!(jobs.status(&key).unwrap().state, JobState::Generated);
        if action == "lock" {
            host.unlock(&e.descriptor.system_actor).unwrap();
        }
        assert!(host.inspect_native().unwrap().data.memories.is_empty());
        stop(&jobs, thread);
    }
}

#[test]
fn source_deletion_also_revokes_saved_receipts_and_context() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = worker(&jobs, ModelMode::Faithful);
    let source = e.source(1, "Remember the synthetic project.", SpeakerRole::User);
    host.capture_native(source.clone()).unwrap();
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    host.apply_job(&key).unwrap();
    host.delete_source_native(&e.descriptor.created_by, source.record.source_id)
        .unwrap();
    assert!(
        host.context("project", Sensitivity::Private, 8)
            .unwrap()
            .is_empty()
    );
    assert!(host.apply_job(&key).is_err());
    let state = host.inspect_native().unwrap();
    assert_eq!(state.data.memories.len(), 1);
    assert_eq!(state.data.deletion_watermark.get(), 2);
    stop(&jobs, thread);
}

#[test]
fn pending_generation_holds_no_vault_writer_and_lock_stops_it() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = e.host(e.vault(Faults::none()), jobs.clone());
    let thread = worker(&jobs, ModelMode::Pending);
    host.capture_native(e.source(1, "A pending synthetic generation.", SpeakerRole::User))
        .unwrap();
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Running);
    let managed = enouia_memory_vault::fs::ManagedRoot::new(e.root.path(), Faults::none());
    let guard = enouia_memory_vault::lock::acquire(&managed, Duration::ZERO).unwrap();
    drop(guard);
    host.lock().unwrap();
    wait(&jobs, &key, JobState::Invalidated);
    assert!(host.apply_job(&key).is_err());
    host.unlock(&e.descriptor.system_actor).unwrap();
    assert!(host.inspect_native().unwrap().data.memories.is_empty());
    stop(&jobs, thread);
}

#[test]
fn native_worker_automatically_publishes_without_an_apply_or_owner_review_step() {
    let e = Env::new();
    let jobs = e.jobs();
    let host = Arc::new(e.host(e.vault(Faults::none()), jobs.clone()));
    let thread = host
        .start_worker(Model {
            mode: ModelMode::Faithful,
        })
        .unwrap();
    host.capture_native(e.source(1, "I prefer concise project notes.", SpeakerRole::User))
        .unwrap();
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    let until = Instant::now() + Duration::from_secs(15);
    let receipt = loop {
        if let Ok(Some(receipts)) = host.receipts(&key) {
            break receipts;
        }
        assert!(
            Instant::now() < until,
            "automatic application: {:?}",
            host.receipts(&key)
        );
        std::thread::sleep(Duration::from_millis(5));
    };
    assert_eq!(receipt.len(), 1);
    assert_eq!(host.inspect_native().unwrap().data.memories.len(), 1);
    assert!(
        host.context("notes", Sensitivity::Private, 8)
            .unwrap()
            .contains("Subject stated")
    );
    drop(host);
    thread.join().unwrap();
    drop(jobs);
    let jobs = e.jobs();
    let host = e.host(e.open(), jobs);
    assert_eq!(host.apply_job(&key).unwrap(), receipt);
}
#[test]
fn v1_readers_refuse_the_new_root_and_legacy_owner_bytes_are_never_rewritten() {
    let e = Env::new();
    let vault = e.vault(Faults::none());
    let head = vault.inspect().unwrap().current();
    drop(vault);
    let descriptor_before = std::fs::read(e.root.path().join("vault/vault.json")).unwrap();
    assert!(
        enouia_memory_vault::Vault::open(
            &e.root,
            None,
            e.clock.clone(),
            Arc::new(OsIdSource),
            enouia_memory_vault::VaultOptions::default()
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(e.root.path().join("vault/vault.json")).unwrap(),
        descriptor_before
    );
    assert_eq!(e.open().inspect().unwrap().current(), head);
    let legacy = root("legacy");
    std::fs::create_dir(legacy.path().join("vault")).unwrap();
    let descriptor =
        std::fs::read(support::fixtures_root().join("../store/descriptor.json")).unwrap();
    let owner_record =
        std::fs::read(support::fixtures_root().join("records/memory-fact.json")).unwrap();
    let review =
        std::fs::read(support::fixtures_root().join("records/review-accept.json")).unwrap();
    std::fs::write(legacy.path().join("vault/vault.json"), &descriptor).unwrap();
    std::fs::write(legacy.path().join("vault/owner-memory.json"), &owner_record).unwrap();
    std::fs::write(legacy.path().join("vault/owner-review.json"), &review).unwrap();
    assert!(
        AutomationVault::open(
            &legacy,
            e.descriptor.host_id.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    assert!(!legacy.path().join("indexes").exists());
    assert_eq!(
        std::fs::read(legacy.path().join("vault/owner-memory.json")).unwrap(),
        owner_record
    );
    assert_eq!(
        std::fs::read(legacy.path().join("vault/owner-review.json")).unwrap(),
        review
    );
    assert!(
        AutomationVault::create(
            &legacy,
            e.descriptor.clone(),
            e.policy.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
}
#[test]
fn host_fences_wrong_principals_and_duplicate_native_event_identities() {
    let e = Env::new();
    let vault = e.vault(Faults::none());
    assert!(
        AutomationVault::open(
            &e.root,
            e.descriptor.host_id.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    assert!(
        vault
            .capture(
                &e.descriptor.created_by,
                e.source(1, "Cannot borrow owner authority.", SpeakerRole::User)
            )
            .is_err()
    );
    let first = e.source(1, "One event.", SpeakerRole::User);
    vault
        .capture(&e.descriptor.system_actor, first.clone())
        .unwrap();
    let mut alias = e.source(2, "A rewrapped version of one event.", SpeakerRole::User);
    alias.record.locator = first.record.locator;
    assert!(vault.capture(&e.descriptor.system_actor, alias).is_err());
    let mut confirmation = e.source(
        3,
        "A statement is not an owner confirmation.",
        SpeakerRole::User,
    );
    confirmation.record.evidence_class = EvidenceClass::UserConfirmation;
    assert!(
        vault
            .capture(&e.descriptor.system_actor, confirmation)
            .is_err()
    );
    assert!(
        vault
            .set_policy(&e.descriptor.system_actor, e.policy.clone())
            .is_err()
    );
    drop(vault);
    assert!(
        AutomationVault::open(
            &e.root,
            DeviceId::from_random([88; 16]),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
}
#[test]
fn exact_forward_recovery_retains_original_results_without_regenerating() {
    for point in [
        FaultPoint::StagingWritten,
        FaultPoint::HostIntentPublished,
        FaultPoint::BeforeCurrent,
        FaultPoint::AfterCurrent,
        FaultPoint::HostWitnessPublished,
    ] {
        let e = Env::new();
        let faults = Faults::armed();
        let jobs = e.jobs();
        let host = e.host(e.vault(faults.clone()), jobs.clone());
        host.capture_native(e.source(1, "Recover the exact synthetic result.", SpeakerRole::User))
            .unwrap();
        let thread = worker(&jobs, ModelMode::Faithful);
        let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
        wait(&jobs, &key, JobState::Generated);
        faults.arm(point, 1, FaultAction::Fail(5));
        assert!(matches!(
            host.apply_job(&key),
            Err(ApplicationFault::Vault(AutomationFault::OutcomeUnknown))
        ));
        drop(host);
        thread.join().unwrap();
        drop(jobs);
        let opened = AutomationVault::open(
            &e.root,
            e.descriptor.host_id.clone(),
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        );
        let vault = match opened {
            Ok(v) => v,
            Err(AutomationFault::RecoveryRequired) => AutomationVault::recover(
                &e.root,
                e.descriptor.host_id.clone(),
                &e.descriptor.system_actor,
                e.clock.clone(),
                Arc::new(OsIdSource),
                Faults::none(),
            )
            .unwrap(),
            Err(e) => panic!("{e:?}"),
        };
        let already = vault.inspect().unwrap().data.memories.len() == 1;
        let jobs = e.jobs();
        let host = e.host(vault, jobs.clone());
        if !already {
            let resumed = host.prepare_submit(vec![e.input(1, None)]).unwrap();
            assert_eq!(resumed, key);
        }
        let receipt = host.apply_job(&key).unwrap();
        assert_eq!(host.apply_job(&key).unwrap(), receipt);
        assert_eq!(host.inspect_native().unwrap().data.memories.len(), 1);
        assert_eq!(receipt[0].sequence.get(), 3);
    }
}
#[test]
fn conflicting_witness_is_never_guessed_or_repaired() {
    let e = Env::new();
    let faults = Faults::armed();
    let jobs = e.jobs();
    let host = e.host(e.vault(faults.clone()), jobs.clone());
    host.capture_native(e.source(
        1,
        "No rollback from a conflicting witness.",
        SpeakerRole::User,
    ))
    .unwrap();
    let thread = worker(&jobs, ModelMode::Faithful);
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    faults.arm(FaultPoint::AfterCurrent, 1, FaultAction::Fail(5));
    assert!(host.apply_job(&key).is_err());
    drop(host);
    thread.join().unwrap();
    drop(jobs);
    let current_bytes = std::fs::read(e.root.path().join("vault/CURRENT")).unwrap();
    let mut foreign: Value = serde_json::from_slice(&current_bytes).unwrap();
    foreign["commit_id"] = json!(CommitId::from_random([9; 16]));
    std::fs::write(
        e.root.path().join(AUTOMATION_WITNESS),
        canonical_bytes(&foreign).unwrap(),
    )
    .unwrap();
    assert!(
        AutomationVault::recover(
            &e.root,
            e.descriptor.host_id.clone(),
            &e.descriptor.system_actor,
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none()
        )
        .is_err()
    );
    assert_eq!(
        std::fs::read(e.root.path().join("vault/CURRENT")).unwrap(),
        current_bytes
    );
}
#[test]
fn core3_publication_child() {
    let Some(path) = std::env::var_os("ENOU_CORE3_DATA_ROOT") else {
        return;
    };
    let mut e = Env::new();
    e.root = verify_data_root(&PathBuf::from(path), &RootPolicy::default()).unwrap();
    e.jobs_root = verify_data_root(
        &PathBuf::from(std::env::var_os("ENOU_CORE3_JOB_ROOT").unwrap()),
        &RootPolicy::default(),
    )
    .unwrap();
    let point = match std::env::var("ENOU_CORE3_POINT").unwrap().as_str() {
        "intent" => FaultPoint::HostIntentPublished,
        "current" => FaultPoint::AfterCurrent,
        _ => FaultPoint::HostWitnessPublished,
    };
    let faults = Faults::armed();
    let jobs = e.jobs();
    let host = e.host(e.vault(faults.clone()), jobs.clone());
    host.capture_native(e.source(1, "A process-crash formation result.", SpeakerRole::User))
        .unwrap();
    let _thread = worker(&jobs, ModelMode::Faithful);
    let key = host.prepare_submit(vec![e.input(1, None)]).unwrap();
    wait(&jobs, &key, JobState::Generated);
    faults.arm(point, 1, FaultAction::Abort);
    host.apply_job(&key).unwrap();
    panic!("fault did not terminate the owned child");
}
#[test]
fn actual_process_crash_preserves_one_result_and_the_same_receipt() {
    for point in ["intent", "current", "witness"] {
        let e = Env::new();
        let status = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "core3_publication_child", "--nocapture"])
            .env("ENOU_CORE3_DATA_ROOT", e.root.path())
            .env("ENOU_CORE3_JOB_ROOT", e.jobs_root.path())
            .env("ENOU_CORE3_POINT", point)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .unwrap();
        assert_eq!(status.code(), Some(3));
        let vault = AutomationVault::recover(
            &e.root,
            e.descriptor.host_id.clone(),
            &e.descriptor.system_actor,
            e.clock.clone(),
            Arc::new(OsIdSource),
            Faults::none(),
        )
        .unwrap();
        let state = vault.inspect().unwrap();
        assert_eq!(state.data.memories.len(), 1);
        let receipt = state.data.receipts[0].clone();
        let jobs = e.jobs();
        let host = e.host(vault, jobs);
        assert_eq!(host.apply_job(&receipt.job_key).unwrap(), vec![receipt]);
        assert!(
            host.context("crash", Sensitivity::Private, 8)
                .unwrap()
                .contains("Subject stated")
        );
    }
}

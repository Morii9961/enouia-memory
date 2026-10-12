//! Only synthetic archived exports and isolated Vaults. No HTTP/secrets.
#[path = "../../enouia-memory-context/tests/support/mod.rs"]
mod support;
use enouia_memory_contract::{
    MemoryErrorCode,
    common::{ActorType, Sensitivity},
    extraction::ExtractionState,
    foundation::Cancellation,
    hash::sha256,
    ids::SubjectId,
    record::{AnyRecord, RecordKind, RecordRef},
};
use enouia_memory_import::history::HistoryLimits;
use enouia_memory_import::{ImportOptions, import_file, pipeline::NeverCancel};
use enouia_memory_provider::{
    codec::Api,
    extraction,
    history::{HistoryOptions, materialize},
};
use enouia_memory_vault::{RootPolicy, Vault, VaultOptions, verify_data_root};
use std::sync::Arc;
use support::{Env, owner};

const EXPORT: &[u8] =
    include_bytes!("../../../tests/fixtures/history/initialization-synthetic.json");

fn options(env: &Env, content: &[u8], selected: Vec<usize>) -> HistoryOptions {
    let path = env.root.join("synthetic-export.json");
    std::fs::write(&path, content).unwrap();
    let mut import = ImportOptions::new(owner(), "synthetic", env.policy());
    import.sensitivity = Sensitivity::Normal;
    let report = import_file(&env.vault, &path, &import, &NeverCancel).unwrap();
    HistoryOptions {
        import_id: report.import_id,
        local_policy: env.policy(),
        history_limits: HistoryLimits {
            messages_per_batch: 2,
            ..HistoryLimits::default()
        },
        batch_indices: selected,
        api: Api::OpenAiResponses,
        model: "synthetic-model".into(),
        subject_id: SubjectId::parse("sub_00000001-0000-4000-8000-000000000001").unwrap(),
        max_candidates: 2,
        max_pending: 500,
        max_reserved_tokens: 100_000,
        max_reserved_cost_microusd: None,
    }
}

fn reopen(env: &Env, faults: enouia_memory_vault::fault::Faults) -> Vault {
    let root = verify_data_root(env.vault.managed_root().root(), &RootPolicy::default()).unwrap();
    Vault::open(
        &root,
        None,
        env.clock.clone(),
        Arc::new(enouia_memory_vault::OsIdSource),
        VaultOptions {
            faults,
            ..VaultOptions::default()
        },
    )
    .unwrap()
}

#[test]
fn archived_bindings_replay_after_reopen_without_jobs_or_head_multiplication() {
    let env = Env::new("history-bound-reopen");
    let options = options(&env, EXPORT, vec![0, 3, 4]);
    let first = materialize(
        &env.vault,
        &owner(),
        &options,
        b"history-task",
        &NeverCancel,
    )
    .unwrap();
    assert_eq!(first.input_object_hash, sha256(EXPORT));
    assert_eq!(first.next_batch, Some(0));
    assert!(
        first
            .batches
            .iter()
            .all(|b| b.state == ExtractionState::Ready
                && b.invocation_state.is_none()
                && b.applied == 0)
    );
    let pin = env.vault.pin_current().unwrap();
    assert_eq!(
        env.vault
            .record_entries(&pin, RecordKind::Session)
            .unwrap()
            .len(),
        4
    );
    assert!(
        env.vault
            .record_entries(&pin, RecordKind::Candidate)
            .unwrap()
            .is_empty()
    );
    let reopened = reopen(&env, enouia_memory_vault::fault::Faults::none());
    let replay = materialize(&reopened, &owner(), &options, b"history-task", &NeverCancel).unwrap();
    assert_eq!(replay, first);
    assert_eq!(reopened.pin_current().unwrap(), pin);
    assert!(reopened.verify(&pin).unwrap().is_clean());
}

#[test]
fn saved_coordinator_lists_source_dependencies_and_matches_its_hash() {
    let env = Env::new("history-coordinator");
    let options = options(&env, EXPORT, vec![3, 4]);
    let result = materialize(&env.vault, &owner(), &options, b"coordinator", &NeverCancel).unwrap();
    let pin = env.vault.pin_current().unwrap();
    let AnyRecord::SessionEvent(event) = env
        .vault
        .read_parsed(
            &pin,
            &RecordRef::new(
                RecordKind::SessionEvent,
                result.coordinator_input_id.as_str(),
                enouia_memory_contract::json::Revision::new(1).unwrap(),
            ),
        )
        .unwrap()
    else {
        panic!("coordinator event")
    };
    let text = enouia_memory_context::session::text(&env.vault, &pin, &event).unwrap();
    assert_eq!(sha256(text.as_bytes()), result.plan_hash);
    let metadata: serde_json::Value = serde_json::from_str(&text).unwrap();
    assert_eq!(metadata["task"], "history-extraction-plan-1");
    assert_eq!(metadata["adapter"]["name"], "chatgpt-conversations");
    assert_eq!(metadata["batches"][1]["index"], 4);
    assert_eq!(
        event.source_refs.len(),
        3,
        "u1/a1/u2, with ancestor deduplication"
    );
    let last = result.batches.last().unwrap();
    let child = extraction::job(&env.vault, &owner(), &last.session_id, &last.run_id).unwrap();
    assert_eq!(child.sources.len(), 2, "preceding a1 and cancellation u2");
    for s in child.sources {
        let AnyRecord::Source(record) = env
            .vault
            .read_parsed(
                &pin,
                &RecordRef::new(
                    RecordKind::Source,
                    s.source.source_id.as_str(),
                    s.source.source_revision,
                ),
            )
            .unwrap()
        else {
            panic!("source")
        };
        assert_eq!(record.raw_object_hash, Some(sha256(EXPORT)));
    }
}

#[test]
fn changed_selection_order_model_or_bounds_under_same_key_is_a_conflict() {
    for change in 0..4 {
        let env = Env::new("history-plan-conflict");
        let mut options = options(&env, EXPORT, vec![0, 3, 4]);
        materialize(&env.vault, &owner(), &options, b"same-key", &NeverCancel).unwrap();
        let pin = env.vault.pin_current().unwrap();
        match change {
            0 => options.batch_indices.swap(0, 1),
            1 => options.batch_indices.pop().map(|_| ()).unwrap(),
            2 => options.model = "synthetic-other-model".into(),
            _ => options.max_reserved_tokens += 1,
        }
        assert_eq!(
            materialize(&env.vault, &owner(), &options, b"same-key", &NeverCancel)
                .unwrap_err()
                .code,
            MemoryErrorCode::IdempotencyConflict
        );
        assert_eq!(env.vault.pin_current().unwrap(), pin);
    }
}

#[test]
fn malformed_or_unauthorized_selections_write_nothing() {
    for change in 0..6 {
        let env = Env::new("history-invalid-selection");
        let mut options = options(&env, EXPORT, vec![0]);
        let mut actor = owner();
        match change {
            0 => options.batch_indices = vec![],
            1 => options.batch_indices = vec![0, 0],
            2 => options.batch_indices = vec![999],
            3 => options.history_limits.messages_per_batch = 16,
            4 => options.max_pending = 501,
            _ => actor.actor_type = ActorType::Agent,
        }
        let pin = env.vault.pin_current().unwrap();
        assert!(materialize(&env.vault, &actor, &options, b"invalid", &NeverCancel).is_err());
        assert_eq!(env.vault.pin_current().unwrap(), pin);
    }
}

struct CancelAfterFirstJob<'a>(&'a Vault);
impl Cancellation for CancelAfterFirstJob<'_> {
    fn is_cancelled(&self) -> bool {
        let pin = self.0.pin_current().unwrap();
        self.0
            .record_entries(&pin, RecordKind::Session)
            .unwrap()
            .len()
            >= 2
    }
}

#[test]
fn cancelled_materialization_resumes_the_persisted_child_and_coordinator() {
    let env = Env::new("history-cancel-resume");
    let options = options(&env, EXPORT, vec![0, 3, 4]);
    assert_eq!(
        materialize(
            &env.vault,
            &owner(),
            &options,
            b"cancel",
            &CancelAfterFirstJob(&env.vault)
        )
        .unwrap_err()
        .code,
        MemoryErrorCode::Cancelled
    );
    let pin = env.vault.pin_current().unwrap();
    assert_eq!(
        env.vault
            .record_entries(&pin, RecordKind::Session)
            .unwrap()
            .len(),
        2
    );
    let done = materialize(&env.vault, &owner(), &options, b"cancel", &NeverCancel).unwrap();
    assert_eq!(done.batches.len(), 3);
    assert_eq!(
        env.vault
            .record_entries(&env.vault.pin_current().unwrap(), RecordKind::Session)
            .unwrap()
            .len(),
        4
    );
}

#[test]
fn lost_child_publication_response_and_unpublished_job_both_recover_once() {
    use enouia_memory_vault::fault::{FaultAction, FaultPoint, Faults};
    for point in [FaultPoint::BeforeCurrent, FaultPoint::AfterCurrent] {
        let env = Env::new("history-lost-publication");
        let options = options(&env, EXPORT, vec![0, 3]);
        let faults = Faults::armed();
        let interrupted = reopen(&env, faults.clone());
        // coordinator header/input, first child header/input/job.
        faults.arm(point, 5, FaultAction::Fail(112));
        assert!(
            materialize(
                &interrupted,
                &owner(),
                &options,
                b"lost-response",
                &NeverCancel
            )
            .is_err()
        );
        drop(interrupted);
        let restored = reopen(&env, Faults::none());
        let recovered = materialize(
            &restored,
            &owner(),
            &options,
            b"lost-response",
            &NeverCancel,
        )
        .unwrap();
        assert_eq!(recovered.batches.len(), 2);
        let pin = restored.pin_current().unwrap();
        assert_eq!(
            restored
                .record_entries(&pin, RecordKind::Session)
                .unwrap()
                .len(),
            3
        );
        assert_eq!(
            materialize(
                &restored,
                &owner(),
                &options,
                b"lost-response",
                &NeverCancel
            )
            .unwrap(),
            recovered
        );
        assert_eq!(restored.pin_current().unwrap(), pin);
        assert!(restored.verify(&pin).unwrap().is_clean());
    }
}

#[test]
fn pause_state_and_application_cursor_are_recovered_from_child_jobs() {
    let env = Env::new("history-paused-child");
    let options = options(&env, EXPORT, vec![0, 3]);
    let first = materialize(&env.vault, &owner(), &options, b"paused", &NeverCancel).unwrap();
    let child = &first.batches[0];
    extraction::set_paused(&env.vault, &owner(), &child.session_id, &child.run_id, true).unwrap();
    let pin = env.vault.pin_current().unwrap();
    let resumed = materialize(&env.vault, &owner(), &options, b"paused", &NeverCancel).unwrap();
    assert_eq!(resumed.batches[0].state, ExtractionState::Paused);
    assert_eq!(resumed.batches[0].applied, 0);
    assert_eq!(env.vault.pin_current().unwrap(), pin);
}

#[test]
fn oversize_text_is_refused_before_any_coordinator_is_written() {
    let env = Env::new("history-oversize");
    let mut input: serde_json::Value = serde_json::from_slice(EXPORT).unwrap();
    input[1]["mapping"]["p1"]["message"]["content"]["parts"][0] =
        serde_json::json!("界".repeat(11_000));
    let options = options(&env, &serde_json::to_vec(&input).unwrap(), vec![0]);
    let pin = env.vault.pin_current().unwrap();
    assert_eq!(
        materialize(&env.vault, &owner(), &options, b"large", &NeverCancel)
            .unwrap_err()
            .code,
        MemoryErrorCode::BudgetExceeded
    );
    assert_eq!(env.vault.pin_current().unwrap(), pin);
}

fn purge_memory(env: &Env, id: &enouia_memory_contract::ids::MemoryId) {
    use enouia_memory_contract::{
        commit::{DeleteMode, DeleteScope},
        common::TrustedSurface,
        json::Revision,
    };
    use enouia_memory_govern::{Canonical, Decision, Origin, OwnerConfirmation, Proposed};
    let current = enouia_memory_govern::canonical_memories(
        &env.vault,
        &env.vault.pin_current().unwrap(),
        Canonical::Active,
    )
    .unwrap()
    .into_iter()
    .find(|m| &m.memory_id == id)
    .unwrap();
    let proposal = enouia_memory_govern::delete::delete_proposal(
        &current,
        DeleteMode::Purge,
        DeleteScope::WithDependents,
    );
    let Proposed::Stored(candidate) = enouia_memory_govern::propose(
        &env.vault,
        &proposal,
        &Origin::owner(owner()),
        b"purge-unrelated",
    )
    .unwrap() else {
        panic!("candidate")
    };
    let plan = enouia_memory_govern::plan(
        &env.vault,
        &[Decision::Accept {
            candidate_id: candidate.id,
            revision: Revision::new(1).unwrap(),
        }],
        &owner(),
        TrustedSurface::TrustedLocalCli,
    )
    .unwrap();
    enouia_memory_govern::confirm(
        &env.vault,
        &plan,
        &OwnerConfirmation {
            owner: owner(),
            surface: TrustedSurface::TrustedLocalCli,
        },
    )
    .unwrap();
    enouia_memory_govern::delete::complete_purge(
        &env.vault,
        &plan.ids[0].delete_id,
        &owner(),
        b"purge-unrelated-files",
    )
    .unwrap();
}

#[test]
fn unrelated_purged_sources_do_not_block_new_archived_history() {
    let mut env = Env::new("history-unrelated-purge");
    let unrelated = env.remember("obsolete", "synthetic_obsolete", "合成旧资料", None);
    let options = options(&env, EXPORT, vec![0]);
    purge_memory(&env, &unrelated);
    assert!(
        materialize(
            &env.vault,
            &owner(),
            &options,
            b"after-unrelated-purge",
            &NeverCancel
        )
        .is_ok()
    );
}

#[test]
fn revised_imported_source_blocks_old_plan_even_with_a_new_key() {
    let env = Env::new("history-revised-source");
    let original = options(&env, EXPORT, vec![0]);
    materialize(
        &env.vault,
        &owner(),
        &original,
        b"before-revision",
        &NeverCancel,
    )
    .unwrap();
    let mut changed: serde_json::Value = serde_json::from_slice(EXPORT).unwrap();
    changed[1]["mapping"]["p2"]["message"]["content"]["parts"][0] =
        serde_json::json!("合成修订：偏好再次改变。");
    let _new_import = options(&env, &serde_json::to_vec(&changed).unwrap(), vec![0]);
    let pin = env.vault.pin_current().unwrap();
    for key in [b"before-revision".as_slice(), b"new-key".as_slice()] {
        assert_eq!(
            materialize(&env.vault, &owner(), &original, key, &NeverCancel)
                .unwrap_err()
                .code,
            MemoryErrorCode::BrokenProvenance
        );
        assert_eq!(env.vault.pin_current().unwrap(), pin);
    }
}

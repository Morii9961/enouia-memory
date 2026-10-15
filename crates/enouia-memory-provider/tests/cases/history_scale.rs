//! Generated synthetic boundaries, not real-history scale or model quality.
use super::*;
use enouia_memory_contract::{json::Revision, ports::CommitPin, session::SessionEvent};
use serde_json::{Value, json};
use std::{collections::BTreeSet, time::Instant};

fn synthetic_export(prefix: usize, leaves: usize) -> Vec<u8> {
    assert!(prefix > 0);
    let mut mapping = serde_json::Map::new();
    let mut insert =
        |id: String, parent: Option<String>, ordinal: usize| {
            mapping.insert(id.clone(), json!({"parent":parent,"message":{
            "id":id,"author":{"role":if ordinal.is_multiple_of(2) {"user"} else {"assistant"}},
            "create_time":1_704_067_200 + ordinal,
            "content":{"parts":[format!("合成批次边界消息 {id}，尚待人工审核。")]}},
        }));
        };
    for index in 0..prefix {
        insert(
            format!("m{index}"),
            index.checked_sub(1).map(|n| format!("m{n}")),
            index,
        );
    }
    for index in 0..leaves {
        insert(
            format!("leaf{index:02}"),
            Some(format!("m{}", prefix - 1)),
            prefix + index,
        );
    }
    let current = if leaves == 0 {
        format!("m{}", prefix - 1)
    } else {
        format!("leaf{:02}", leaves - 1)
    };
    serde_json::to_vec(
        &json!([{"id":"synthetic-batch-boundary", "current_node":current,
        "mapping":mapping}]),
    )
    .unwrap()
}

fn input_event(
    vault: &Vault,
    pin: &CommitPin,
    id: &enouia_memory_contract::ids::EventId,
) -> SessionEvent {
    let AnyRecord::SessionEvent(event) = vault
        .read_parsed(
            pin,
            &RecordRef::new(
                RecordKind::SessionEvent,
                id.as_str(),
                Revision::new(1).unwrap(),
            ),
        )
        .unwrap()
    else {
        panic!("input event")
    };
    event
}

fn source_ids(
    vault: &Vault,
    pin: &CommitPin,
    job: &enouia_memory_contract::extraction::ExtractionJob,
) -> Vec<String> {
    job.sources
        .iter()
        .map(|slice| {
            let AnyRecord::Source(source) = vault
                .read_parsed(
                    pin,
                    &RecordRef::new(
                        RecordKind::Source,
                        slice.source.source_id.as_str(),
                        slice.source.source_revision,
                    ),
                )
                .unwrap()
            else {
                panic!("source")
            };
            source.original_message_id.unwrap()
        })
        .collect()
}

fn sessions(vault: &Vault, pin: &CommitPin) -> Vec<enouia_memory_contract::session::SessionRecord> {
    vault
        .record_entries(pin, RecordKind::Session)
        .unwrap()
        .into_iter()
        .map(|entry| {
            let AnyRecord::Session(session) = vault
                .read_parsed(
                    pin,
                    &RecordRef::new(RecordKind::Session, &entry.record_id, entry.revision),
                )
                .unwrap()
            else {
                panic!("session")
            };
            session
        })
        .collect()
}

struct CancelAtSessions<'a> {
    vault: &'a Vault,
    count: usize,
}
impl Cancellation for CancelAtSessions<'_> {
    fn is_cancelled(&self) -> bool {
        let pin = self.vault.pin_current().unwrap();
        self.vault
            .record_entries(&pin, RecordKind::Session)
            .unwrap()
            .len()
            >= self.count
    }
}

#[test]
fn maximum_batch_selection_resumes_in_order_without_duplicate_jobs() {
    let started = Instant::now();
    let env = Env::new("history-32-batch-boundary");
    let raw = synthetic_export(33, 0);
    let mut options = options(&env, &raw, (0..32).rev().collect());
    options.history_limits.messages_per_batch = 1;
    let imported_pin = env.vault.pin_current().unwrap();
    options.batch_indices = (0..33).collect();
    assert_eq!(
        materialize(
            &env.vault,
            &owner(),
            &options,
            b"batch-boundary",
            &NeverCancel
        )
        .unwrap_err()
        .code,
        MemoryErrorCode::InvalidRequest
    );
    assert_eq!(env.vault.pin_current().unwrap(), imported_pin);
    options.batch_indices = (0..32).rev().collect();
    assert_eq!(
        materialize(
            &env.vault,
            &owner(),
            &options,
            b"batch-boundary",
            &CancelAtSessions {
                vault: &env.vault,
                count: 17
            }
        )
        .unwrap_err()
        .code,
        MemoryErrorCode::Cancelled
    );
    let partial_pin = env.vault.pin_current().unwrap();
    let partial = env
        .vault
        .record_entries(&partial_pin, RecordKind::Session)
        .unwrap();
    assert_eq!(
        partial.len(),
        17,
        "coordinator plus sixteen persisted children"
    );
    let partial_jobs = sessions(&env.vault, &partial_pin)
        .into_iter()
        .flat_map(|session| session.extraction_jobs)
        .collect::<Vec<_>>();
    assert_eq!(partial_jobs.len(), 16);
    let reopened = reopen(&env, enouia_memory_vault::fault::Faults::none());
    let resumed = materialize(
        &reopened,
        &owner(),
        &options,
        b"batch-boundary",
        &NeverCancel,
    )
    .unwrap();
    let pin = reopened.pin_current().unwrap();
    assert_eq!(resumed.batches.len(), 32);
    assert_eq!(resumed.next_batch, Some(31));
    assert_eq!(resumed.input_object_hash, sha256(&raw));
    assert_eq!(
        reopened
            .record_entries(&pin, RecordKind::Session)
            .unwrap()
            .len(),
        33
    );
    let jobs = sessions(&reopened, &pin)
        .into_iter()
        .flat_map(|session| session.extraction_jobs)
        .collect::<Vec<_>>();
    assert_eq!(jobs.len(), 32);
    for job in partial_jobs {
        assert!(jobs.contains(&job), "persisted child and state retained");
    }
    let mut run_ids = BTreeSet::new();
    let mut event_ids = BTreeSet::new();
    for (batch, &index) in resumed.batches.iter().zip(&options.batch_indices) {
        assert_eq!(batch.batch_index, index);
        assert_eq!(batch.state, ExtractionState::Ready);
        assert_eq!(batch.applied, 0);
        assert!(batch.invocation_state.is_none());
        assert!(run_ids.insert(batch.run_id.clone()));
        assert!(event_ids.insert(batch.input_event_id.clone()));
        let job = extraction::job(&reopened, &owner(), &batch.session_id, &batch.run_id).unwrap();
        let expected = index
            .checked_sub(1)
            .into_iter()
            .chain([index])
            .map(|n| format!("m{n}"))
            .collect::<Vec<_>>();
        assert_eq!(source_ids(&reopened, &pin, &job), expected);
    }
    let coordinator = input_event(&reopened, &pin, &resumed.coordinator_input_id);
    assert_eq!(
        coordinator.source_refs.len(),
        32,
        "unselected m32 is not silently included"
    );
    for kind in [RecordKind::Candidate, RecordKind::Memory] {
        assert!(reopened.record_entries(&pin, kind).unwrap().is_empty());
    }
    for session in sessions(&reopened, &pin) {
        assert!(session.provider_invocations.is_empty());
    }
    assert_eq!(
        materialize(
            &reopened,
            &owner(),
            &options,
            b"batch-boundary",
            &NeverCancel
        )
        .unwrap(),
        resumed
    );
    assert_eq!(reopened.pin_current().unwrap(), pin);
    assert!(reopened.verify(&pin).unwrap().is_clean());
    eprintln!(
        "synthetic_batch_boundary: 33 imported messages, 32 jobs, cancelled after 16, replay unchanged; elapsed_ms={}",
        started.elapsed().as_millis()
    );
}

#[test]
fn fifteen_message_chunks_retain_the_full_preceding_context() {
    let env = Env::new("history-15-message-boundary");
    let raw = synthetic_export(31, 0);
    let mut options = options(&env, &raw, vec![0, 1, 2]);
    options.history_limits.messages_per_batch = 15;
    let progress = materialize(
        &env.vault,
        &owner(),
        &options,
        b"fifteen-boundary",
        &NeverCancel,
    )
    .unwrap();
    let pin = env.vault.pin_current().unwrap();
    assert_eq!(progress.batches.len(), 3);
    for (batch, indices) in progress.batches.iter().zip([0..15, 14..30, 29..31]) {
        let job = extraction::job(&env.vault, &owner(), &batch.session_id, &batch.run_id).unwrap();
        assert_eq!(
            source_ids(&env.vault, &pin, &job),
            indices.map(|n| format!("m{n}")).collect::<Vec<_>>()
        );
        let event = input_event(&env.vault, &pin, &job.input_event_id);
        let text = enouia_memory_context::session::text(&env.vault, &pin, &event).unwrap();
        let body: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(body["sources"].as_array().unwrap().len(), job.sources.len());
        for (context, id) in body["sources"]
            .as_array()
            .unwrap()
            .iter()
            .zip(source_ids(&env.vault, &pin, &job))
        {
            assert_eq!(
                context["text"],
                format!("合成批次边界消息 {id}，尚待人工审核。")
            );
            let ordinal: usize = id.strip_prefix('m').unwrap().parse().unwrap();
            assert_eq!(
                context["speaker_role"],
                if ordinal.is_multiple_of(2) {
                    "user"
                } else {
                    "assistant"
                }
            );
        }
    }
    assert_eq!(
        input_event(&env.vault, &pin, &progress.coordinator_input_id)
            .source_refs
            .len(),
        31
    );
    assert!(env.vault.verify(&pin).unwrap().is_clean());
}

#[test]
fn shared_branch_metadata_budget_refuses_all_batches_before_writing() {
    let started = Instant::now();
    let env = Env::new("history-plan-metadata-boundary");
    // Thirty-message paths have two 15-message chunks. The second retains
    // one preceding message: 32 selected branches repeat sixteen refs each.
    let raw = synthetic_export(29, 32);
    let mut options = options(&env, &raw, (0..32).map(|n| n * 2 + 1).collect());
    options.history_limits.messages_per_batch = 15;
    let before = env.vault.pin_current().unwrap();
    assert_eq!(
        materialize(
            &env.vault,
            &owner(),
            &options,
            b"metadata-boundary",
            &NeverCancel
        )
        .unwrap_err()
        .code,
        MemoryErrorCode::BudgetExceeded
    );
    assert_eq!(env.vault.pin_current().unwrap(), before);
    assert!(
        env.vault
            .record_entries(&before, RecordKind::Session)
            .unwrap()
            .is_empty()
    );
    // Each selected batch is independently valid. Narrowing the explicit
    // choice under the unused key succeeds; the failed plan left no receipt.
    options.batch_indices.truncate(1);
    let one = materialize(
        &env.vault,
        &owner(),
        &options,
        b"metadata-boundary",
        &NeverCancel,
    )
    .unwrap();
    let job = extraction::job(
        &env.vault,
        &owner(),
        &one.batches[0].session_id,
        &one.batches[0].run_id,
    )
    .unwrap();
    assert_eq!(job.sources.len(), 16);
    assert!(
        env.vault
            .verify(&env.vault.pin_current().unwrap())
            .unwrap()
            .is_clean()
    );
    eprintln!(
        "synthetic_metadata_boundary: 61 imported messages, 32 selected batches, 512 refs refused without writes; one 16-ref batch accepted; elapsed_ms={}",
        started.elapsed().as_millis()
    );
}

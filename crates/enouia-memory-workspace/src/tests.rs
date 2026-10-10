//! The workspace Core through its page channel: every request is the JSON
//! the frontend sends, every response is checked against the workspace IPC
//! contract. Synthetic data in a temporary root only.

use super::*;
use enouia_memory_contract::foundation::FakeClock;
use enouia_memory_contract::ports::SequentialIdSource;
use enouia_memory_contract::workspace::validate_response;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

static COUNTER: AtomicU64 = AtomicU64::new(0);
const T0: i64 = 1_790_000_000_000;

#[test]
fn full_remember_replays_a_source_published_after_its_initial_lookup() {
    check_full_remember_replay(0, true, false);
}

#[test]
fn full_remember_replays_a_concurrent_source_commit() {
    check_full_remember_replay(1, false, false);
}

#[test]
fn full_remember_refuses_a_changed_claim_after_source_replay() {
    check_full_remember_replay(0, true, true);
}

fn check_full_remember_replay(skip: usize, ordered: bool, conflict: bool) {
    let env = Env::new("full-remember-replay");
    let text = "Synthetic full remember replay";
    let envelope = json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000201",
        "command": "remember", "idempotencyKey": "synthetic-full-remember-replay",
        "arguments": {"text": text, "claimKey": "synthetic.full_remember"}});
    let callers = AtomicU64::new(0);
    with_concurrent_allocations_at(
        &env,
        skip,
        ordered,
        |_| {},
        |ws| {
            let n = callers.fetch_add(1, Ordering::SeqCst);
            let mut own = envelope.clone();
            own["requestId"] = json!(format!("req_00000000-0000-4000-8000-{:012x}", n + 201));
            if conflict {
                own["arguments"]["claimKey"] = json!(format!("synthetic.full_remember_{n}"));
            }
            call_with_contention_retry(ws, &own)
        },
        |ws, first, second| {
            for response in [&first, &second] {
                validate_response("remember", response).unwrap();
            }
            assert_ne!(first["requestId"], second["requestId"]);
            let responses = [&first, &second];
            let successful: Vec<_> = responses
                .iter()
                .filter(|r| r["error"] == Value::Null)
                .collect();
            assert_eq!(successful.len(), if conflict { 1 } else { 2 });
            let published = &successful[0]["result"];
            let open = ws.open().unwrap();
            let pin = open.vault.pin_current().unwrap();
            let sources = open.vault.record_entries(&pin, RecordKind::Source).unwrap();
            assert_eq!(sources.len(), 1);
            assert_eq!(json!(sources[0].record_id), published["sourceId"]);
            let pending = pending_candidates(&open.vault, &pin).unwrap();
            assert_eq!(pending.len(), 1);
            assert_eq!(json!(pending[0].candidate_id), published["candidateId"]);
            assert_eq!(json!(pending[0].source_id), published["sourceId"]);
            assert_eq!(
                json!(pending[0].evidence[0].source_id),
                published["sourceId"]
            );
            let excerpt = ws.call(&json!({"schemaVersion": 1,
                "requestId": "req_00000000-0000-4000-8000-000000000203",
                "command": "source_excerpt", "idempotencyKey": null,
                "arguments": {"sourceId": published["sourceId"], "sourceRevision": 1,
                    "startByte": null, "maxBytes": 4096}}));
            validate_response("source_excerpt", &excerpt).unwrap();
            assert_eq!(excerpt["error"], Value::Null, "{excerpt}");
            assert_eq!(excerpt["result"]["excerpt"], text);
            assert_eq!(excerpt["result"]["untrusted"], true);
            if conflict {
                let refused = responses
                    .iter()
                    .find(|r| r["error"] != Value::Null)
                    .unwrap();
                assert_eq!(refused["error"]["code"], "idempotency_conflict");
                assert_eq!(refused["error"]["rules"][0], "workspace.key_reuse");
                assert_eq!(refused["result"], Value::Null);
            } else {
                assert_eq!(first["result"], second["result"]);
                let mut repeated = envelope.clone();
                repeated["requestId"] = json!("req_00000000-0000-4000-8000-000000000204");
                let replay = ws.call(&repeated);
                validate_response("remember", &replay).unwrap();
                assert_eq!(replay["error"], Value::Null, "{replay}");
                assert_eq!(replay["result"], *published);
            }
            assert_eq!(open.vault.pin_current().unwrap().commit_id, pin.commit_id);
        },
    );
}

#[test]
fn correction_head_retry_rechecks_the_target_revision() {
    let env = Env::new("correction-head-retry-target");
    let memory = env.remember("Synthetic head retry target", "synthetic.head_retry");
    let keys = ["synthetic-target-retry-one", "synthetic-target-retry-two"];
    let texts = [
        "Synthetic first target revision",
        "Synthetic second target revision",
    ];
    let open = env.ws.open().unwrap();
    for (key, text) in keys.iter().zip(texts.iter()) {
        env.ws
            .assertion(&open, text, format!("correction-source\n{key}").as_bytes())
            .unwrap();
    }
    let callers = AtomicU64::new(0);
    with_concurrent_allocations_at(
        &env,
        0,
        true,
        |ws| {
            let open = ws.open().unwrap();
            let pending =
                pending_candidates(&open.vault, &open.vault.pin_current().unwrap()).unwrap();
            assert_eq!(pending.len(), 1);
            let plan = ws.call(&json!({"schemaVersion": 1,
                "requestId": "req_00000000-0000-4000-8000-000000000211",
                "command": "review_plan", "idempotencyKey": null,
                "arguments": {"decisions": [{"candidateId": pending[0].candidate_id,
                    "revision": pending[0].revision, "action": "accept",
                    "editedContent": null, "mergeTarget": null}]}}));
            validate_response("review_plan", &plan).unwrap();
            assert_eq!(plan["error"], Value::Null, "{plan}");
            let confirmed = ws.call(&json!({"schemaVersion": 1,
                "requestId": "req_00000000-0000-4000-8000-000000000212",
                "command": "review_confirm", "idempotencyKey": "synthetic-target-retry-confirm",
                "arguments": {"planId": plan["result"]["planId"], "diffHash": plan["result"]["diffHash"]}}));
            validate_response("review_confirm", &confirmed).unwrap();
            assert_eq!(confirmed["error"], Value::Null, "{confirmed}");
        },
        |ws| {
            let n = callers.fetch_add(1, Ordering::SeqCst) as usize;
            ws.call(&json!({"schemaVersion": 1,
                "requestId": format!("req_00000000-0000-4000-8000-{:012x}", n + 213),
                "command": "correction_propose", "idempotencyKey": keys[n],
                "arguments": {"memoryId": memory, "revision": 1, "text": texts[n]}}))
        },
        |ws, first, second| {
            for response in [&first, &second] {
                validate_response("correction_propose", response).unwrap();
            }
            let responses = [&first, &second];
            assert_eq!(
                responses
                    .iter()
                    .filter(|r| r["error"] == Value::Null)
                    .count(),
                1
            );
            let refused = responses
                .iter()
                .find(|r| r["error"] != Value::Null)
                .unwrap();
            assert_eq!(refused["error"]["code"], "revision_conflict");
            assert_eq!(refused["error"]["rules"][0], "fault.revision_mismatch");
            assert_eq!(refused["result"], Value::Null);
            let open = ws.open().unwrap();
            let pin = open.vault.pin_current().unwrap();
            assert!(pending_candidates(&open.vault, &pin).unwrap().is_empty());
            assert_eq!(
                open.vault
                    .record_entry(&pin, RecordKind::Memory, &memory)
                    .unwrap()
                    .unwrap()
                    .revision,
                Revision::new(2).unwrap()
            );
        },
    );
}

#[test]
fn different_keys_do_not_publish_duplicate_pending_candidates() {
    check_different_key_proposals(true, false);
}

#[test]
fn different_keys_preserve_distinct_proposals_after_head_movement() {
    check_different_key_proposals(false, false);
}

#[test]
fn governance_refuses_proposal_publication_from_a_stale_dedupe_snapshot() {
    check_different_key_proposals(true, true);
}

fn check_different_key_proposals(duplicate: bool, domain_only: bool) {
    let env = Env::new("different-key-proposal-dedupe");
    let texts = [
        "Synthetic simultaneous duplicate",
        if duplicate {
            "Synthetic simultaneous duplicate"
        } else {
            "Synthetic independent proposal"
        },
    ];
    let keys = ["synthetic-dedupe-key-one", "synthetic-dedupe-key-two"];
    let open = env.ws.open().unwrap();
    let sources: Vec<_> = keys
        .iter()
        .enumerate()
        .map(|(n, key)| {
            env.ws
                .assertion(
                    &open,
                    texts[n],
                    format!("remember-source\n{key}").as_bytes(),
                )
                .unwrap()
        })
        .collect();
    let callers = AtomicU64::new(0);
    with_concurrent_allocations_at(
        &env,
        0,
        true,
        |_| {},
        |ws| {
            let n = callers.fetch_add(1, Ordering::SeqCst) as usize;
            if domain_only {
                let open = ws.open().unwrap();
                let subject = open.owner.actor_id.as_str().replacen("prn_", "sub_", 1);
                let details = json!({"claim_key": "synthetic.simultaneous_duplicate", "subject_ids": [subject]});
                let proposal = Proposal::create(
                    ProposedType::Fact,
                    texts[n],
                    details.as_object().unwrap().clone(),
                    vec![EvidenceSpec::content(sources[n].clone(), one())],
                );
                return match propose(
                    &open.vault,
                    &proposal,
                    &Origin::owner(open.owner.clone()),
                    keys[n].as_bytes(),
                ) {
                    Ok(Proposed::Stored(written)) => {
                        json!({"candidate": written.id, "error": null})
                    }
                    Ok(Proposed::DuplicateOf(id)) => json!({"candidate": id, "error": null}),
                    Err(error) => json!({"candidate": null, "error": format!("{:?}", error.fault)}),
                };
            }
            ws.call(&json!({"schemaVersion": 1,
            "requestId": format!("req_00000000-0000-4000-8000-{n:012x}"),
            "command": "remember", "idempotencyKey": keys[n],
            "arguments": {"text": texts[n], "claimKey": "synthetic.simultaneous_duplicate"}}))
        },
        |ws, first, second| {
            if domain_only {
                let responses = [&first, &second];
                assert_eq!(
                    responses
                        .iter()
                        .filter(|r| r["error"] == Value::Null)
                        .count(),
                    1
                );
                let refused = responses
                    .iter()
                    .find(|r| r["error"] != Value::Null)
                    .unwrap();
                assert_eq!(refused["error"], "HeadMoved");
                assert_eq!(refused["candidate"], Value::Null);
                let open = ws.open().unwrap();
                assert_eq!(
                    pending_candidates(&open.vault, &open.vault.pin_current().unwrap())
                        .unwrap()
                        .len(),
                    1
                );
                return;
            }
            for response in [&first, &second] {
                validate_response("remember", response).unwrap();
                assert_eq!(response["error"], Value::Null, "{response}");
            }
            let open = ws.open().unwrap();
            let pin = open.vault.pin_current().unwrap();
            assert_eq!(
                pending_candidates(&open.vault, &pin).unwrap().len(),
                if duplicate { 1 } else { 2 }
            );
            assert_eq!(
                first["result"]["candidateId"] == second["result"]["candidateId"],
                duplicate
            );
        },
    );
}

#[test]
fn correction_and_forget_recover_late_proposal_receipts() {
    for command in ["correction_propose", "forget_plan"] {
        let env = Env::new(command);
        let memory = env.remember("Synthetic admission target", "synthetic.admission_target");
        let key = "synthetic-other-proposal-race";
        let args = if command == "correction_propose" {
            let open = env.ws.open().unwrap();
            env.ws
                .assertion(
                    &open,
                    "Synthetic admission correction",
                    format!("correction-source\n{key}").as_bytes(),
                )
                .unwrap();
            json!({"memoryId": memory, "revision": 1, "text": "Synthetic admission correction"})
        } else {
            json!({"memoryId": memory, "mode": "forget", "withDependents": false})
        };
        let request = json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000102",
            "command": command, "idempotencyKey": key, "arguments": args});
        with_concurrent_allocations_at(
            &env,
            0,
            true,
            |_| {},
            |ws| ws.call(&request),
            |ws, first, second| {
                for response in [&first, &second] {
                    validate_response(command, response).unwrap();
                    assert_eq!(response["error"], Value::Null, "{command}: {response}");
                    if command == "forget_plan" {
                        let tombstone = response["result"]["records"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .find(|r| r["record_kind"] == "tombstone")
                            .unwrap();
                        assert_eq!(tombstone["value"]["targets"][0]["record_id"], memory);
                        assert_eq!(tombstone["value"]["mode"], "logical_delete");
                        assert_eq!(tombstone["value"]["scope"], "all_revisions");
                    }
                }
                if command == "correction_propose" {
                    assert_eq!(first["result"], second["result"]);
                }
                let open = ws.open().unwrap();
                let pin = open.vault.pin_current().unwrap();
                assert_eq!(pending_candidates(&open.vault, &pin).unwrap().len(), 1);
                let retry = ws.call(&request);
                assert_eq!(retry["error"], Value::Null);
                let mut changed = request.clone();
                if command == "correction_propose" {
                    changed["arguments"]["text"] = json!("Synthetic different correction");
                } else {
                    changed["arguments"]["mode"] = json!("purge");
                }
                let refused = ws.call(&changed);
                assert_eq!(refused["error"]["code"], "idempotency_conflict");
                assert_eq!(refused["error"]["rules"][0], "workspace.key_reuse");
                assert_eq!(refused["result"], Value::Null);
                assert_eq!(open.vault.pin_current().unwrap().commit_id, pin.commit_id);
            },
        );
    }
}

#[test]
fn remember_replays_a_proposal_published_after_its_initial_lookup() {
    check_remember_proposal_admission(false);
}

#[test]
fn remember_refuses_a_changed_claim_published_after_its_initial_lookup() {
    check_remember_proposal_admission(true);
}

fn check_remember_proposal_admission(conflict: bool) {
    let env = Env::new("remember-proposal-admission");
    let key = "synthetic-remember-proposal-race";
    let args = json!({"text": "Synthetic proposal replay", "claimKey": "synthetic.proposal_race"});
    let open = env.ws.open().unwrap();
    env.ws
        .assertion(
            &open,
            args["text"].as_str().unwrap(),
            format!("remember-source\n{key}").as_bytes(),
        )
        .unwrap();
    let request = json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000101",
        "command": "remember", "idempotencyKey": key, "arguments": args});
    let callers = AtomicU64::new(0);
    with_concurrent_allocations_at(
        &env,
        0,
        true,
        |_| {},
        |ws| {
            let n = callers.fetch_add(1, Ordering::SeqCst);
            let mut own = request.clone();
            if conflict {
                own["arguments"]["claimKey"] = json!(format!("synthetic.race_claim_{n}"));
            }
            ws.call(&own)
        },
        |ws, first, second| {
            for response in [&first, &second] {
                validate_response("remember", response).unwrap();
            }
            if conflict {
                let responses = [&first, &second];
                assert_eq!(
                    responses
                        .iter()
                        .filter(|r| r["error"] == Value::Null)
                        .count(),
                    1
                );
                let refused = responses
                    .iter()
                    .find(|r| r["error"] != Value::Null)
                    .unwrap();
                assert_eq!(refused["error"]["code"], "idempotency_conflict");
                assert_eq!(refused["error"]["rules"][0], "workspace.key_reuse");
                assert_eq!(refused["result"], Value::Null);
                let open = ws.open().unwrap();
                assert_eq!(
                    pending_candidates(&open.vault, &open.vault.pin_current().unwrap())
                        .unwrap()
                        .len(),
                    1
                );
                return;
            }
            for response in [&first, &second] {
                assert_eq!(response["error"], Value::Null, "{response}");
            }
            assert_eq!(first["result"], second["result"]);
            let open = ws.open().unwrap();
            let pin = open.vault.pin_current().unwrap();
            assert_eq!(pending_candidates(&open.vault, &pin).unwrap().len(), 1);
            let repeated = ws.call(&request);
            assert_eq!(repeated["result"], first["result"]);
            assert_eq!(open.vault.pin_current().unwrap().commit_id, pin.commit_id);
            let mut changed = request.clone();
            changed["arguments"]["claimKey"] = json!("synthetic.changed_claim");
            let changed = ws.call(&changed);
            assert_eq!(changed["error"]["code"], "idempotency_conflict");
            assert_eq!(changed["result"], Value::Null);
            assert_eq!(open.vault.pin_current().unwrap().commit_id, pin.commit_id);
        },
    );
}

#[test]
fn correction_replay_binds_target_revision_and_text() {
    let env = Env::new("correction-key-binding");
    let target = env.remember(
        "Synthetic first correction target",
        "synthetic.correct_first",
    );
    let other = env.remember(
        "Synthetic other correction target",
        "synthetic.correct_other",
    );
    let args = json!({"memoryId": target, "revision": 1, "text": "Synthetic corrected text"});
    let first = env.send_keyed("correction_propose", args.clone(), "correction-binding-key");
    assert_eq!(first["error"], Value::Null, "{first}");
    for reopen in [false, true] {
        if reopen {
            env.ws.shutdown();
            env.ws.open_root(&env.base.join("vault")).unwrap();
        }
        let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
        let same = env.send_keyed("correction_propose", args.clone(), "correction-binding-key");
        assert_eq!(same["result"], first["result"], "{same}");
        for changed in [
            json!({"memoryId": other, "revision": 1, "text": "Synthetic corrected text"}),
            json!({"memoryId": target, "revision": 1, "text": "Synthetic different text"}),
        ] {
            let response = env.send_keyed("correction_propose", changed, "correction-binding-key");
            assert_eq!(
                response["error"]["code"], "idempotency_conflict",
                "{response}"
            );
            assert_eq!(response["error"]["rules"][0], "workspace.key_reuse");
            assert_eq!(response["result"], Value::Null);
        }
        assert_eq!(
            env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
            before
        );
    }
    let update = env.ok(
        "correction_propose",
        json!({
            "memoryId": target, "revision": 1, "text": "Synthetic independently accepted update",
        }),
    );
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{
            "candidateId": update["candidateId"], "revision": update["revision"],
            "action": "accept", "editedContent": null, "mergeTarget": null,
        }]}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
    let changed = env.send_keyed(
        "correction_propose",
        json!({
            "memoryId": target, "revision": 2, "text": "Synthetic corrected text",
        }),
        "correction-binding-key",
    );
    assert_eq!(
        changed["error"]["code"], "idempotency_conflict",
        "{changed}"
    );
    assert_eq!(changed["error"]["rules"][0], "workspace.key_reuse");
    let stale = env.send_keyed("correction_propose", args, "correction-binding-key");
    assert_eq!(stale["error"]["code"], "revision_conflict", "{stale}");
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
        before
    );
}

#[test]
fn forget_replay_binds_target_mode_and_dependent_scope() {
    let env = Env::new("forget-key-binding");
    let target = env.remember("Synthetic first deletion target", "synthetic.delete_first");
    let other = env.remember("Synthetic other deletion target", "synthetic.delete_other");
    let args = json!({"memoryId": target, "mode": "forget", "withDependents": false});
    let first = env.send_keyed("forget_plan", args.clone(), "forget-binding-key");
    assert_eq!(first["error"], Value::Null, "{first}");
    for reopen in [false, true] {
        if reopen {
            env.ws.shutdown();
            env.ws.open_root(&env.base.join("vault")).unwrap();
        }
        let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
        let same = env.send_keyed("forget_plan", args.clone(), "forget-binding-key");
        assert_eq!(same["error"], Value::Null, "{same}");
        let tombstone = same["result"]["records"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["record_kind"] == "tombstone")
            .unwrap();
        assert_eq!(tombstone["value"]["targets"][0]["record_id"], target);
        assert_eq!(tombstone["value"]["mode"], "logical_delete");
        assert_eq!(tombstone["value"]["scope"], "all_revisions");
        assert_eq!(same["result"]["purge"], false);
        for changed in [
            json!({"memoryId": other, "mode": "forget", "withDependents": false}),
            json!({"memoryId": target, "mode": "purge", "withDependents": false}),
            json!({"memoryId": target, "mode": "forget", "withDependents": true}),
        ] {
            let response = env.send_keyed("forget_plan", changed, "forget-binding-key");
            assert_eq!(
                response["error"]["code"], "idempotency_conflict",
                "{response}"
            );
            assert_eq!(response["error"]["rules"][0], "workspace.key_reuse");
            assert_eq!(response["result"], Value::Null);
        }
        assert_eq!(
            env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
            before
        );
    }
}

#[test]
fn correction_replay_rechecks_a_receipt_published_after_admission() {
    let env = Env::new("correction-concurrent-binding");
    let targets = [
        env.remember("Synthetic racing target one", "synthetic.race_one"),
        env.remember("Synthetic racing target two", "synthetic.race_two"),
    ];
    let callers = AtomicU64::new(0);
    with_concurrent_allocations_at(
        &env,
        0,
        true,
        |_| {},
        |ws| {
            let n = callers.fetch_add(1, Ordering::SeqCst) as usize;
            ws.call(&json!({
            "schemaVersion": 1,
            "requestId": format!("req_00000000-0000-4000-8000-{n:012x}"),
            "command": "correction_propose", "idempotencyKey": "correction-racing-key",
            "arguments": {"memoryId": targets[n], "revision": 1, "text": "Synthetic same correction"},
        }))
        },
        |ws, first, second| {
            let responses = [first, second];
            for response in &responses {
                validate_response("correction_propose", response).unwrap();
            }
            assert_eq!(
                responses
                    .iter()
                    .filter(|v| v["error"] == Value::Null)
                    .count(),
                1,
                "{responses:?}"
            );
            let refused = responses
                .iter()
                .find(|v| v["kind"] == "memory_error")
                .unwrap();
            assert_eq!(
                refused["error"]["code"], "idempotency_conflict",
                "{refused}"
            );
            assert_eq!(refused["error"]["rules"][0], "workspace.key_reuse");
            let open = ws.open().unwrap();
            let pin = open.vault.pin_current().unwrap();
            let candidates: Vec<CandidateRecord> =
                latest_records(&open.vault, &pin, RecordKind::Candidate).unwrap();
            assert_eq!(
                candidates
                    .iter()
                    .filter(
                        |c| c.status == enouia_memory_contract::candidate::CandidateStatus::Pending
                    )
                    .count(),
                1
            );
        },
    );
}

#[test]
fn remember_replay_binds_the_claim_key_after_reopen_and_review() {
    let env = Env::new("remember-claim-replay");
    let args = json!({"text": "Synthetic stable text", "claimKey": "synthetic.first_claim"});
    let first = env.send_keyed("remember", args.clone(), "claim-replay-key");
    assert_eq!(first["error"], Value::Null);
    let candidate = &first["result"];
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{
            "candidateId": candidate["candidateId"], "revision": candidate["revision"],
            "action": "accept", "editedContent": null, "mergeTarget": null,
        }]}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    for reopen in [false, true] {
        if reopen {
            env.ws.shutdown();
            env.ws.open_root(&env.base.join("vault")).unwrap();
        }
        let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
        let same = env.send_keyed("remember", args.clone(), "claim-replay-key");
        assert_eq!(same["error"], Value::Null);
        assert_eq!(same["result"]["candidateId"], candidate["candidateId"]);
        let changed = env.send_keyed(
            "remember",
            json!({
                "text": "Synthetic stable text", "claimKey": "synthetic.other_claim",
            }),
            "claim-replay-key",
        );
        assert_eq!(
            changed["error"]["code"], "idempotency_conflict",
            "{changed}"
        );
        assert_eq!(changed["error"]["rules"][0], "workspace.key_reuse");
        assert_eq!(
            env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
            before
        );
    }
}

#[test]
fn source_excerpt_advances_or_rejects_an_insufficient_utf8_budget() {
    let env = Env::new("excerpt-budget");
    let text = "Aé中😀Z";
    let memory = env.remember(text, "synthetic.excerpt_budget");
    let detail = env.ok("memory_read", json!({"memoryId": memory}));
    let source = &detail["evidence"][0];
    for start in (0..=text.len() as u64 + 1).chain([enouia_memory_contract::json::MAX_SAFE_INTEGER])
    {
        let mut aligned = start.min(text.len() as u64) as usize;
        while !text.is_char_boundary(aligned) {
            aligned -= 1;
        }
        for budget in 1..=4 {
            let response = env.send(
                "source_excerpt",
                json!({
                    "sourceId": source["sourceId"], "sourceRevision": source["sourceRevision"],
                    "startByte": start, "maxBytes": budget,
                }),
            );
            let next = text[aligned..].chars().next();
            if next.is_some_and(|c| c.len_utf8() as u64 > budget) {
                assert_eq!(
                    response["error"]["code"], "invalid_request",
                    "start={start}, budget={budget}: {response}"
                );
                assert_eq!(response["error"]["rules"][0], "workspace.excerpt_budget");
                assert_eq!(response["error"]["retryable"], false);
                continue;
            }
            assert_eq!(response["error"], Value::Null, "{response}");
            let result = &response["result"];
            let end = result["byteEnd"].as_u64().unwrap() as usize;
            assert_eq!(result["byteStart"], aligned);
            assert!(text.is_char_boundary(end));
            assert!(end - aligned <= budget as usize);
            assert_eq!(result["excerpt"], &text[aligned..end]);
            assert_eq!(result["totalBytes"], text.len());
            if next.is_some() {
                assert!(end as u64 > start, "non-advancing page: {result}");
            } else {
                assert_eq!(result["excerpt"], "");
                assert_eq!(end, text.len());
            }
        }
    }
}

#[test]
fn picker_admission_excludes_concurrent_page_uses_of_one_token() {
    let env = Env::new("picker-admission");
    std::fs::write(env.base.join("notes.md"), "Synthetic picker input").unwrap();
    std::fs::create_dir(env.base.join("backup")).unwrap();
    std::fs::create_dir(env.base.join("export")).unwrap();
    for (kind, relative, command, field) in [
        (
            PickKind::BackupDestination,
            "backup",
            "backup_export",
            "destinationToken",
        ),
        (
            PickKind::ImportFile,
            "notes.md",
            "import_preview",
            "importToken",
        ),
        (
            PickKind::ImportFile,
            "notes.md",
            "import_start",
            "importToken",
        ),
        (
            PickKind::ExportFolder,
            "export",
            "restore_preview",
            "exportToken",
        ),
    ] {
        let token = env.pick(kind, relative);
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        std::thread::scope(|scope| {
            let ws = &env.ws;
            let held_token = &token;
            let held = scope.spawn(move || {
                ws.with_lifecycle(false, || {
                    ws.with_pick(held_token, kind, true, |_| {
                        ready_tx.send(()).unwrap();
                        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                        Ok(())
                    })
                })
            });
            ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            let mut args = json!({field: token});
            if command == "import_start" {
                args["accountAlias"] = json!("synthetic-picker");
            }
            let response = env.send(command, args);
            release_tx.send(()).unwrap();
            held.join().unwrap().unwrap();
            assert_eq!(response["error"]["code"], "busy", "{command}: {response}");
            assert_eq!(response["error"]["rules"][0], "workspace.token_busy");
            assert_eq!(response["error"]["retryable"], true);
            assert!(
                !response
                    .to_string()
                    .contains(&env.base.to_string_lossy().to_string())
            );
        });
    }
}

#[test]
fn picker_admission_releases_on_error_and_unwind_and_consumes_on_success() {
    let env = Env::new("picker-release");
    for kind in [
        PickKind::VaultRoot,
        PickKind::BackupDestination,
        PickKind::ExportFolder,
    ] {
        let token = env.pick(kind, "vault");
        let denied: R<()> = env.ws.with_pick(&token, kind, true, |_| {
            Err(fail(
                MemoryErrorCode::StorageFailed,
                "synthetic.pick_failed",
            ))
        });
        assert!(denied.is_err());
        let panic = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            env.ws.with_lifecycle(false, || {
                env.ws
                    .with_pick::<()>(&token, kind, true, |_| panic!("synthetic picker unwind"))
            })
        }));
        assert!(panic.is_err());
        assert!(!env.ws.picks.is_poisoned());
        env.ws
            .with_pick(&token, kind, false, |path| {
                assert_eq!(path, env.base.join("vault"));
                Ok(())
            })
            .unwrap();
        env.ws.with_pick(&token, kind, true, |_| Ok(())).unwrap();
        let missing = env
            .ws
            .with_pick(&token, kind, true, |_| Ok(()))
            .unwrap_err();
        assert_eq!(missing.0.rules[0], "workspace.token_unknown");
    }
}

#[test]
fn a_reserved_picker_does_not_block_other_choices_or_extend_its_ttl() {
    let env = Env::new("picker-independent");
    std::fs::create_dir(env.base.join("backup")).unwrap();
    let chosen_at = env.clock.now_unix_ms();
    let token = env.pick(PickKind::BackupDestination, "backup");
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    std::thread::scope(|scope| {
        let ws = &env.ws;
        let held_token = &token;
        let held = scope.spawn(move || {
            ws.with_lifecycle(false, || {
                ws.with_pick::<()>(held_token, PickKind::BackupDestination, true, |_| {
                    ready_tx.send(()).unwrap();
                    release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
                    Err(fail(
                        MemoryErrorCode::StorageFailed,
                        "synthetic.pick_failed",
                    ))
                })
            })
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        env.clock.set(chosen_at + PICK_TTL_MS);
        let start = std::time::Instant::now();
        let other = env.pick(PickKind::BackupDestination, "backup");
        let unblocked = env
            .ws
            .with_pick(&other, PickKind::BackupDestination, true, |_| Ok(()));
        let elapsed = start.elapsed();
        let retained_while_active = env.ws.picks.lock().unwrap().contains_key(&token);
        let expired = env
            .ws
            .with_pick(&token, PickKind::BackupDestination, true, |_| Ok(()));
        release_tx.send(()).unwrap();
        assert!(held.join().unwrap().is_err());
        unblocked.unwrap();
        assert!(
            elapsed < Duration::from_millis(500),
            "unrelated picker blocked: {elapsed:?}"
        );
        assert!(retained_while_active);
        assert_eq!(expired.unwrap_err().0.rules[0], "workspace.token_unknown");
        let after = env
            .ws
            .with_pick(&token, PickKind::BackupDestination, true, |_| Ok(()));
        assert_eq!(after.unwrap_err().0.rules[0], "workspace.token_unknown");
        env.pick(PickKind::BackupDestination, "backup");
        assert!(!env.ws.picks.lock().unwrap().contains_key(&token));
    });
}

#[test]
fn picker_preview_retries_and_async_failure_preserve_consumption_rules() {
    let env = Env::new("picker-outcomes");
    std::fs::write(env.base.join("notes.md"), "Synthetic reusable preview").unwrap();
    let token = env.pick(PickKind::ImportFile, "notes.md");
    std::fs::remove_file(env.base.join("notes.md")).unwrap();
    assert_eq!(
        env.err("import_preview", json!({"importToken": token}))["rules"][0],
        "workspace.pick_missing"
    );
    std::fs::write(env.base.join("notes.md"), "Synthetic reusable preview").unwrap();
    for _ in 0..2 {
        assert_eq!(
            env.ok("import_preview", json!({"importToken": token}))["recognized"],
            true
        );
    }
    // Scheduling succeeds even though the selected input disappeared. The
    // worker fails, but the single-use token must not become available again.
    std::fs::remove_file(env.base.join("notes.md")).unwrap();
    let started = env.ok(
        "import_start",
        json!({"importToken": token, "accountAlias": "synthetic-picker"}),
    );
    assert_eq!(env.wait(&started)["state"], "failed");
    assert_eq!(
        env.err("import_preview", json!({"importToken": token}))["rules"][0],
        "workspace.token_unknown"
    );

    std::fs::create_dir(env.base.join("repo-backup")).unwrap();
    std::fs::create_dir(env.base.join("repo-backup/.git")).unwrap();
    let destination = env.pick(PickKind::BackupDestination, "repo-backup");
    assert_eq!(
        env.err("backup_export", json!({"destinationToken": destination}))["code"],
        "invalid_request"
    );
    // The root verification failure occurs before scheduling and keeps choice.
    std::fs::remove_dir(env.base.join("repo-backup/.git")).unwrap();
    let started = env.ok("backup_export", json!({"destinationToken": destination}));
    assert_eq!(env.wait(&started)["state"], "succeeded");
    assert_eq!(
        env.err("backup_export", json!({"destinationToken": destination}))["rules"][0],
        "workspace.token_unknown"
    );

    let valid_export = env.pick(PickKind::ExportFolder, "repo-backup");
    assert_eq!(
        env.ok("restore_preview", json!({"exportToken": valid_export}))["valid"],
        true
    );
    assert_eq!(
        env.err("restore_preview", json!({"exportToken": valid_export}))["rules"][0],
        "workspace.token_unknown"
    );

    std::fs::create_dir(env.base.join("invalid-export")).unwrap();
    let export = env.pick(PickKind::ExportFolder, "invalid-export");
    env.err("restore_preview", json!({"exportToken": export}));
    env.err("restore_preview", json!({"exportToken": export}));
    assert!(env.ws.picks.lock().unwrap().contains_key(&export));
}

#[test]
fn import_preview_revalidates_a_changed_picker_file_kind() {
    let env = Env::new("preview-changed-kind");
    let selected = env.base.join("selected.md");
    std::fs::write(&selected, "Synthetic selected preview").unwrap();
    let token = env.pick(PickKind::ImportFile, "selected.md");
    let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
    std::fs::remove_file(&selected).unwrap();
    std::fs::create_dir(&selected).unwrap();
    let refused = env.send("import_preview", json!({"importToken": token}));
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(
        refused["error"]["rules"][0],
        "import.input_not_regular_file"
    );
    assert_eq!(refused["result"], Value::Null);
    std::fs::remove_dir(&selected).unwrap();
    std::fs::write(&selected, "Synthetic restored preview").unwrap();
    for _ in 0..2 {
        let preview = env.ok("import_preview", json!({"importToken": token}));
        assert_eq!(preview["recognized"], true);
        assert_eq!(preview["bytes"], 26);
    }
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
        before
    );
}

#[test]
fn import_worker_refuses_changed_kind_without_a_partial_archive() {
    let env = Env::new("import-worker-changed-kind");
    let selected = env.base.join("selected.md");
    std::fs::write(&selected, "Synthetic selected import").unwrap();
    let token = env.pick(PickKind::ImportFile, "selected.md");
    let before = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
    std::fs::remove_file(&selected).unwrap();
    std::fs::create_dir(&selected).unwrap();
    let started = env.ok(
        "import_start",
        json!({"importToken": token, "accountAlias": "synthetic-bounded-import"}),
    );
    let refused = env.wait(&started);
    assert_eq!(refused["state"], "failed");
    assert_eq!(refused["error"]["code"], "invalid_request");
    assert_eq!(
        refused["error"]["rules"][0],
        "import.input_not_regular_file"
    );
    assert_eq!(refused["result"], Value::Null);
    assert_eq!(env.ok("import_list", json!({}))["items"], json!([]));
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
        before
    );
    assert_eq!(
        env.err("import_preview", json!({"importToken": token}))["rules"][0],
        "workspace.token_unknown"
    );
    std::fs::remove_dir(&selected).unwrap();
    std::fs::write(&selected, "Synthetic restored import").unwrap();
    let fresh = env.pick(PickKind::ImportFile, "selected.md");
    let started = env.ok(
        "import_start",
        json!({"importToken": fresh, "accountAlias": "synthetic-bounded-import"}),
    );
    assert_eq!(env.wait(&started)["state"], "succeeded");
    assert_eq!(
        env.ok("import_list", json!({}))["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
}

#[test]
fn import_preview_bounds_actual_reads_past_the_metadata_budget() {
    let exact = b"synthetic";
    let mut same = std::io::Cursor::new(exact);
    assert_eq!(read_preview_bytes(&mut same, 9, 9).unwrap(), exact);
    let mut grown = std::io::Cursor::new(vec![b'x'; 8192]);
    let refused = read_preview_bytes(&mut grown, 9, 9).unwrap_err();
    assert_eq!(refused.0.code, MemoryErrorCode::InvalidRequest);
    assert_eq!(refused.0.rules[0], "import.input_too_large");
    assert_eq!(
        grown.position(),
        10,
        "only the budget and one sentinel byte may be read"
    );
}

#[test]
fn import_preview_refuses_size_drift_within_the_global_limit() {
    let mut grown = std::io::Cursor::new(vec![b'x'; 8192]);
    let refused = read_preview_bytes(&mut grown, 9, 8192).unwrap_err();
    assert_eq!(refused.0.rules[0], "import.input_changed");
    assert_eq!(
        grown.position(),
        10,
        "the metadata size also bounds reading"
    );
    let refused = read_preview_bytes(std::io::Cursor::new(b"short"), 9, 8192).unwrap_err();
    assert_eq!(refused.0.rules[0], "import.input_changed");
}

struct Env {
    base: PathBuf,
    clock: Arc<FakeClock>,
    ws: Workspace,
    n: AtomicU64,
}

impl Drop for Env {
    fn drop(&mut self) {
        self.ws.shutdown();
        let _ = std::fs::remove_dir_all(&self.base);
    }
}

impl Env {
    fn new(name: &str) -> Self {
        Self::with_clock(name, None)
    }

    fn with_clock(name: &str, custom_clock: Option<Arc<dyn Clock + Send + Sync>>) -> Self {
        let tmp = std::env::temp_dir().join("enouia-memory-workspace-tests");
        std::fs::create_dir_all(&tmp).unwrap();
        let tmp = std::fs::canonicalize(&tmp).unwrap();
        let tmp = PathBuf::from(tmp.to_string_lossy().trim_start_matches(r"\\?\"));
        let base = tmp.join(format!(
            "{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join("vault")).unwrap();
        let clock = Arc::new(FakeClock::new(T0));
        let ws = Workspace::new(Config {
            clock: custom_clock.unwrap_or_else(|| clock.clone()),
            ids: Arc::new(SequentialIdSource::new(0xb000)),
        });
        let env = Self {
            base,
            clock,
            ws,
            n: AtomicU64::new(0),
        };
        let token = env.pick(PickKind::VaultRoot, "vault");
        let status = env.ok(
            "vault_create",
            json!({"rootToken": token, "confirmPhrase": "create new vault"}),
        );
        assert_eq!(status["vault"]["state"], "open");
        env
    }

    fn pick(&self, kind: PickKind, relative: &str) -> String {
        self.ws
            .register_pick(kind, &self.base.join(relative))
            .unwrap()
            .token
    }

    fn tick(&self) {
        self.clock.set(self.clock.now_unix_ms() + 1_000);
    }

    /// Send one request as the page would and check the envelope.
    fn send(&self, command: &str, arguments: Value) -> Value {
        self.tick();
        let n = self.n.fetch_add(1, Ordering::SeqCst);
        let key = wire::is_write(command).then(|| format!("test-key-{n:016}"));
        let request = json!({
            "schemaVersion": 1,
            "requestId": format!("req_00000000-0000-4000-8000-{n:012x}"),
            "command": command, "idempotencyKey": key, "arguments": arguments,
        });
        let response = self.ws.call(&request);
        validate_response(command, &response)
            .unwrap_or_else(|e| panic!("{command}: {e}: {response}"));
        let text = response.to_string();
        let root = self.base.to_string_lossy().replace('\\', "\\\\");
        assert!(!text.contains(&root), "{command} leaked a path: {text}");
        response
    }

    /// Send with an explicit idempotency key, as a page retry does.
    fn send_keyed(&self, command: &str, arguments: Value, key: &str) -> Value {
        self.tick();
        let n = self.n.fetch_add(1, Ordering::SeqCst);
        let request = json!({
            "schemaVersion": 1,
            "requestId": format!("req_00000000-0000-4000-8000-{n:012x}"),
            "command": command, "idempotencyKey": key, "arguments": arguments,
        });
        let response = self.ws.call(&request);
        validate_response(command, &response)
            .unwrap_or_else(|e| panic!("{command}: {e}: {response}"));
        response
    }

    fn ok(&self, command: &str, arguments: Value) -> Value {
        let response = self.send(command, arguments);
        assert_eq!(response["error"], Value::Null, "{command}: {response}");
        response["result"].clone()
    }

    fn err(&self, command: &str, arguments: Value) -> Value {
        let response = self.send(command, arguments);
        assert_eq!(response["kind"], "memory_error", "{command}: {response}");
        response["error"].clone()
    }

    fn wait(&self, started: &Value) -> Value {
        let id = OperationId::parse(started["operationId"].as_str().unwrap()).unwrap();
        self.ws
            .operations()
            .wait(&id, Duration::from_secs(60))
            .unwrap()
    }

    /// Owner statement -> pending candidate -> plan -> confirmed memory.
    fn remember(&self, text: &str, claim: &str) -> String {
        let proposed = self.ok("remember", json!({"text": text, "claimKey": claim}));
        assert_eq!(proposed["state"], "pending");
        let plan = self.ok(
            "review_plan",
            json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
        );
        self.ok(
            "review_confirm",
            json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
        );
        let list = self.ok(
            "memory_list",
            json!({"includeInactive": false, "cursor": null, "limit": 100}),
        );
        list["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|m| m["snippet"] == text)
            .unwrap()["memoryId"]
            .as_str()
            .unwrap()
            .to_owned()
    }
}

#[test]
fn w01_import_review_ask_inspect_correct_and_resume() {
    let env = Env::new("w01");
    // Import: the page names a token, never a path.
    std::fs::write(
        env.base.join("notes.md"),
        "# MoriMeta\n\n设计决定：采用 Professional Darkroom 风格。\n",
    )
    .unwrap();
    let token = env.pick(PickKind::ImportFile, "notes.md");
    let preview = env.ok("import_preview", json!({"importToken": token}));
    assert_eq!(preview["displayName"], "notes.md");
    assert_eq!(preview["recognized"], true);
    let started = env.ok(
        "import_start",
        json!({"importToken": token, "accountAlias": "acct-main"}),
    );
    let done = env.wait(&started);
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["result"]["status"], "completed", "{done}");
    // The token was single-use.
    let again = env.err(
        "import_start",
        json!({"importToken": token, "accountAlias": "acct-main"}),
    );
    assert_eq!(again["rules"][0], "workspace.token_unknown");
    let imports = env.ok("import_list", json!({}));
    assert_eq!(imports["items"].as_array().unwrap().len(), 1);

    // Review: a pending candidate becomes a memory only through a plan
    // confirmed with the exact diff hash.
    let proposed = env.ok(
        "remember",
        json!({"text": "MoriMeta 的设计决定是 Professional Darkroom。", "claimKey": "project.morimeta.design"}),
    );
    let page = env.ok("candidate_list", json!({"cursor": null, "limit": null}));
    assert_eq!(page["total"], 1);
    assert_eq!(page["items"][0]["candidateId"], proposed["candidateId"]);
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(listed["total"], 0, "a candidate is not a memory");
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    assert_eq!(plan["confirmCode"].as_str().unwrap().len(), 8);
    let wrong = env.err(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": "0".repeat(64)}),
    );
    assert_eq!(wrong["code"], "revision_conflict");
    assert_eq!(
        env.ok(
            "memory_list",
            json!({"includeInactive": false, "cursor": null, "limit": null})
        )["total"],
        0,
        "a wrong hash writes nothing"
    );
    let confirmed = env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    let replay = env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    assert_eq!(confirmed, replay, "a retried confirm answers the same");
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    let memory_id = listed["items"][0]["memoryId"].as_str().unwrap().to_owned();
    assert_eq!(listed["items"][0]["status"], "active");

    // Look up and open the source.
    let found = env.ok(
        "memory_search",
        json!({"query": "MoriMeta", "includeHistorical": false, "cursor": null, "limit": null}),
    );
    assert!(
        found["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|h| h["memoryId"] == json!(memory_id))
    );
    let detail = env.ok("memory_read", json!({"memoryId": memory_id}));
    let evidence = &detail["evidence"][0];
    assert_eq!(evidence["available"], true);
    let excerpt = env.ok(
        "source_excerpt",
        json!({"sourceId": evidence["sourceId"], "sourceRevision": evidence["sourceRevision"], "startByte": null, "maxBytes": 8192}),
    );
    assert_eq!(
        excerpt["excerpt"],
        "MoriMeta 的设计决定是 Professional Darkroom。"
    );
    assert_eq!(excerpt["untrusted"], true);

    // Ask in a session: input saved, context compiled, local Mock answers
    // with sources; the inspector shows the saved capsule and the request.
    let created = env.ok("session_new", json!({}));
    let (sid, bid) = (created["sessionId"].clone(), created["branchId"].clone());
    let turn = env.ok(
        "session_ask",
        json!({"sessionId": sid, "branchId": bid, "text": "MoriMeta 设计决定"}),
    );
    assert_eq!(turn["destination"], "local_mock");
    assert!(!turn["sources"].as_array().unwrap().is_empty(), "{turn}");
    let inspected = env.ok("context_inspect", json!({"capsuleId": turn["capsuleId"]}));
    assert_eq!(inspected["delivery"], "dispatched");
    assert!(
        inspected["inspection"]["decisions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["record_id"] == json!(memory_id) && d["decision"] == "included")
    );
    let request = env.ok(
        "dispatch_inspect",
        json!({"dispatchId": turn["dispatchId"]}),
    );
    assert_eq!(request["verified"], true);
    assert_eq!(request["tools"], 0);
    let preview = env.ok(
        "context_preview",
        json!({"query": "MoriMeta", "sessionId": null, "branchId": null}),
    );
    let previewed = env.ok(
        "context_inspect",
        json!({"capsuleId": preview["capsuleId"]}),
    );
    assert_eq!(previewed["delivery"], "preview_not_sent");

    // Correct: a revision is proposed, reviewed, and then current.
    let fix = env.ok(
        "correction_propose",
        json!({"memoryId": memory_id, "revision": 1, "text": "MoriMeta 的设计决定是 Darkroom 2。"}),
    );
    let candidates = env.ok("candidate_list", json!({"cursor": null, "limit": null}));
    let row = &candidates["items"][0];
    assert_eq!(row["proposalKind"], "revise");
    assert_eq!(
        row["target"]["content"],
        "MoriMeta 的设计决定是 Professional Darkroom。"
    );
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": fix["candidateId"], "revision": fix["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    env.ok(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    let detail = env.ok("memory_read", json!({"memoryId": memory_id}));
    assert_eq!(detail["record"]["revision"], 2);
    assert_eq!(
        detail["record"]["content"],
        "MoriMeta 的设计决定是 Darkroom 2。"
    );

    // Resume: a checkpoint, then a restart of the Core keeps everything.
    env.ok(
        "session_checkpoint",
        json!({"sessionId": sid, "branchId": bid, "summary": "讨论了 MoriMeta 的设计决定。"}),
    );
    env.ws.shutdown();
    let token = env.pick(PickKind::VaultRoot, "vault");
    env.ok("vault_open", json!({"rootToken": token}));
    let detail = env.ok("session_detail", json!({"sessionId": sid, "branchId": bid}));
    assert_eq!(detail["turns"][0]["state"], "completed");
    assert_eq!(detail["checkpoints"][0]["status"], "provisional");
    let sessions = env.ok("session_list", json!({}));
    assert_eq!(sessions["items"][0]["sessionId"], sid);
}

#[test]
fn w02_long_work_cancel_paging_and_index_busy() {
    let env = Env::new("w02");
    for i in 0..5 {
        env.remember(&format!("合成事实 {i}"), &format!("synthetic.fact_{i}"));
    }
    // Paging: two pages of two and one; a later commit makes a cursor stale.
    let first = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": 2}),
    );
    assert_eq!(first["items"].as_array().unwrap().len(), 2);
    let second = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": first["nextCursor"], "limit": 2}),
    );
    assert_eq!(second["items"].as_array().unwrap().len(), 2);
    assert_ne!(first["items"][0], second["items"][0]);
    env.remember("合成事实 5", "synthetic.fact_5");
    let stale = env.err(
        "memory_list",
        json!({"includeInactive": false, "cursor": second["nextCursor"], "limit": 2}),
    );
    assert_eq!(stale["rules"][0], "workspace.cursor_stale");

    // While the index is held (a rebuild), search answers at once.
    let open = env.ws.open().unwrap();
    {
        let _held = open.index.lock().unwrap();
        let busy = env.err(
            "memory_search",
            json!({"query": "合成", "includeHistorical": false, "cursor": null, "limit": null}),
        );
        assert_eq!(busy["code"], "index_not_ready");
        assert_eq!(busy["rules"][0], "index.busy");
        let status = env.ok("workspace_status", json!({}));
        let index = status["components"]
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["component"] == "memory_index")
            .unwrap();
        assert_eq!(index["state"], "recovering");
        assert_eq!(status["vault"]["health"], "healthy");
    }
    // A rebuild runs off the calling thread and reaches the head.
    let started = env.ok("index_rebuild", json!({}));
    let done = env.wait(&started);
    assert_eq!(done["state"], "succeeded", "{done}");
    assert_eq!(done["result"]["reachedHead"], true);
    assert!(done["progress"]["done"].as_u64().unwrap() > 0);
    let found = env.ok(
        "memory_search",
        json!({"query": "合成", "includeHistorical": false, "cursor": null, "limit": null}),
    );
    assert_eq!(found["items"].as_array().unwrap().len(), 6);

    // Cancellation is visible and never reported as success.
    let id = OperationId::from_random([7; 16]);
    env.ws
        .operations()
        .spawn(id.clone(), "test_wait", |ticket| {
            while !ticket.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            Err(WorkspaceError::new(
                MemoryErrorCode::Cancelled,
                &["test.cancelled"],
            ))
        });
    let cancelled = env.ok("operation_cancel", json!({"operationId": id}));
    assert_eq!(cancelled["cancelRequested"], true);
    let finished = env
        .ws
        .operations()
        .wait(&id, Duration::from_secs(10))
        .unwrap();
    assert_eq!(finished["state"], "cancelled");
    let unknown = env.err(
        "operation_get",
        json!({"operationId": OperationId::from_random([9; 16])}),
    );
    assert_eq!(unknown["code"], "not_found");

    // Vault verification and backup export run as operations; the export
    // previews as the same Vault.
    let verified = env.wait(&env.ok("vault_verify", json!({})));
    assert_eq!(verified["result"]["clean"], true, "{verified}");
    std::fs::create_dir_all(env.base.join("backup")).unwrap();
    let token = env.pick(PickKind::BackupDestination, "backup");
    let exported = env.wait(&env.ok("backup_export", json!({"destinationToken": token})));
    assert_eq!(exported["state"], "succeeded", "{exported}");
    let status = env.ok("workspace_status", json!({}));
    assert_eq!(status["lastBackup"]["state"], "succeeded");
    let token = env.pick(PickKind::ExportFolder, "backup");
    let restore = env.ok("restore_preview", json!({"exportToken": token}));
    assert_eq!(restore["sameVaultAsOpen"], true);
    assert_eq!(restore["commitId"], exported["result"]["commitId"]);
}

#[test]
fn w03_lock_unlock_and_shutdown() {
    let env = Env::new("w03");
    env.remember("锁定前保存的事实", "synthetic.before_lock");
    let locked = env.ok("vault_lock", json!({}));
    assert_eq!(locked["vault"]["state"], "locked");
    let refused = env.err(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(refused["code"], "vault_locked");
    let refused = env.err(
        "remember",
        json!({"text": "不会保存", "claimKey": "synthetic.locked"}),
    );
    assert_eq!(refused["code"], "vault_locked");
    let status = env.ok("workspace_status", json!({}));
    let activity = status["components"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["component"] == "activity")
        .unwrap();
    assert_eq!(activity["mode"], "independent_not_managed");
    let unlocked = env.ok("vault_unlock", json!({}));
    assert_eq!(unlocked["vault"]["state"], "open");
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(listed["total"], 1);
    // A plan made before a lock cannot be confirmed after it.
    let proposed = env.ok(
        "remember",
        json!({"text": "计划中的事实", "claimKey": "synthetic.planned"}),
    );
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    env.ok("vault_lock", json!({}));
    env.ok("vault_unlock", json!({}));
    let gone = env.err(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
    );
    assert_eq!(gone["rules"][0], "workspace.plan_unknown");
    env.ws.shutdown();
    let none = env.err(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(none["rules"][0], "workspace.no_vault");
}

#[test]
fn w04_page_cannot_name_paths_commands_or_identities() {
    let env = Env::new("w04");
    let root = env.base.join("vault").to_string_lossy().into_owned();
    let cases = [
        json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "vault_open", "idempotencyKey": null, "arguments": {"rootToken": root}}),
        json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "run_shell", "idempotencyKey": null, "arguments": {"cmd": "dir"}}),
        json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "memory_list", "idempotencyKey": null, "arguments": {"includeInactive": false, "cursor": null, "limit": null, "principal": "prn_x"}}),
        json!({"schemaVersion": 2, "requestId": "req_00000000-0000-4000-8000-00000000ffff", "command": "workspace_status", "idempotencyKey": null, "arguments": {}}),
        json!("not an object"),
    ];
    for case in cases {
        let response = env.ws.call(&case);
        assert_eq!(response["kind"], "memory_error", "{case}");
        assert!(!response.to_string().contains(&root));
    }
    // A token of another kind is refused; an import token cannot open a Vault.
    std::fs::write(env.base.join("a.md"), "# a\n").unwrap();
    let token = env.pick(PickKind::ImportFile, "a.md");
    let wrong = env.err("vault_open", json!({"rootToken": token}));
    assert_eq!(wrong["rules"][0], "workspace.token_kind");
    // Source text is returned as data, marked untrusted; scripts stay text.
    let id = env.remember(
        "<script>alert(1)</script><img src=\"https://example.invalid/x.png\">",
        "synthetic.injection",
    );
    let detail = env.ok("memory_read", json!({"memoryId": id}));
    let excerpt = env.ok(
        "source_excerpt",
        json!({"sourceId": detail["evidence"][0]["sourceId"], "sourceRevision": 1, "startByte": null, "maxBytes": 16}),
    );
    assert_eq!(excerpt["truncated"], true);
    assert_eq!(excerpt["untrusted"], true);
    assert_eq!(excerpt["byteEnd"], 16);
}

#[test]
fn forget_and_purge_go_through_a_plan() {
    let env = Env::new("forget");
    let keep = env.remember("保留的事实", "synthetic.keep");
    let gone = env.remember("要忘记的事实", "synthetic.forget");
    let purged = env.remember("要彻底删除的事实", "synthetic.purge");
    let impact = env.ok(
        "delete_preview",
        json!({"memoryId": purged, "withDependents": false}),
    );
    assert!(!impact["targets"].as_array().unwrap().is_empty());
    for (id, mode) in [(&gone, "forget"), (&purged, "purge")] {
        let plan = env.ok(
            "forget_plan",
            json!({"memoryId": id, "mode": mode, "withDependents": false}),
        );
        assert_eq!(plan["purge"], mode == "purge");
        let done = env.ok(
            "review_confirm",
            json!({"planId": plan["planId"], "diffHash": plan["diffHash"]}),
        );
        assert_eq!(done["purge"].is_object(), mode == "purge", "{done}");
    }
    let listed = env.ok(
        "memory_list",
        json!({"includeInactive": true, "cursor": null, "limit": null}),
    );
    let ids: Vec<&str> = listed["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|m| m["memoryId"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec![keep.as_str()]);
    let missing = env.err("memory_read", json!({"memoryId": gone}));
    assert_eq!(missing["code"], "not_found");
    let verified = env.wait(&env.ok("vault_verify", json!({})));
    assert_eq!(verified["result"]["clean"], true, "{verified}");
}

#[test]
fn concurrent_purge_confirm_replays_one_complete_result() {
    let env = Env::new("concurrent-purge-confirm");
    let memory = env.remember(
        "Synthetic concurrent purge target",
        "synthetic.purge_confirm",
    );
    env.ws.shutdown();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (a_tx, a_rx) = std::sync::mpsc::channel();
    let (b_tx, b_rx) = std::sync::mpsc::channel();
    let ids = Arc::new(HeldIds {
        armed: AtomicU64::new(0),
        skip_per_thread: 0,
        calls: Mutex::new(std::collections::HashMap::new()),
        ready: ready_tx,
        releases: Mutex::new(vec![a_rx, b_rx]),
        seq: SequentialIdSource::new(0xe000),
    });
    let ws = Workspace::new(Config {
        clock: env.clock.clone(),
        ids: ids.clone(),
    });
    ws.open_root(&env.base.join("vault")).unwrap();
    let plan = ws.call(&json!({"schemaVersion": 1,
        "requestId": "req_00000000-0000-4000-8000-000000000301",
        "command": "forget_plan", "idempotencyKey": "synthetic-concurrent-purge-plan",
        "arguments": {"memoryId": memory, "mode": "purge", "withDependents": false}}));
    validate_response("forget_plan", &plan).unwrap();
    assert_eq!(plan["error"], Value::Null, "{plan}");
    let request = json!({"schemaVersion": 1,
        "requestId": "req_00000000-0000-4000-8000-000000000302",
        "command": "review_confirm", "idempotencyKey": "synthetic-concurrent-purge-confirm",
        "arguments": {"planId": plan["result"]["planId"], "diffHash": plan["result"]["diffHash"]}});
    ids.armed.store(2, Ordering::SeqCst);
    let (first, second) = std::thread::scope(|scope| {
        let first = scope.spawn(|| call_with_contention_retry(&ws, &request));
        let paused = ready_rx.recv_timeout(Duration::from_secs(5)).is_ok();
        let observed = ws.call(&json!({"schemaVersion": 1,
            "requestId": "req_00000000-0000-4000-8000-000000000304",
            "command": "workspace_status", "idempotencyKey": null, "arguments": {}}));
        let second = scope.spawn(|| {
            let mut own = request.clone();
            own["requestId"] = json!("req_00000000-0000-4000-8000-000000000303");
            call_with_contention_retry(&ws, &own)
        });
        // The old path reaches a second receipt allocation; a serialized
        // confirm waits before touching the purge again. Always release both.
        let _ = ready_rx.recv_timeout(Duration::from_secs(5));
        let _ = b_tx.send(());
        let _ = a_tx.send(());
        let results = (first.join().unwrap(), second.join().unwrap());
        assert!(paused, "first confirmation must pause after removing files");
        validate_response("workspace_status", &observed).unwrap();
        assert_eq!(observed["error"], Value::Null, "{observed}");
        assert_eq!(observed["result"]["vault"]["state"], "open");
        results
    });
    for response in [&first, &second] {
        validate_response("review_confirm", response).unwrap();
        assert_eq!(response["error"], Value::Null, "{response}");
    }
    assert_ne!(first["requestId"], second["requestId"]);
    assert!(first["result"]["purge"]["filesRemoved"].as_u64().unwrap() > 0);
    assert_eq!(
        first["result"], second["result"],
        "same plan must replay its complete result"
    );
    let open = ws.open().unwrap();
    let pin = open.vault.pin_current().unwrap();
    assert_eq!(
        open.vault
            .record_entries(&pin, RecordKind::PurgeReceipt)
            .unwrap()
            .len(),
        1
    );
    let replay = ws.call(&request);
    assert_eq!(replay["result"], first["result"]);
    assert_eq!(open.vault.pin_current().unwrap().commit_id, pin.commit_id);
    ws.shutdown();
}

#[test]
fn closing_during_purge_waits_for_the_complete_response_and_expires_replay() {
    for action in ["lock", "shutdown"] {
        let env = Env::new(&format!("purge-close-{action}"));
        let memory = env.remember("Synthetic closing purge target", "synthetic.purge_close");
        env.ws.shutdown();
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        let ids = Arc::new(HeldIds {
            armed: AtomicU64::new(0),
            skip_per_thread: 0,
            calls: Mutex::new(std::collections::HashMap::new()),
            ready: ready_tx,
            releases: Mutex::new(vec![release_rx]),
            seq: SequentialIdSource::new(0xf000),
        });
        let ws = Workspace::new(Config {
            clock: env.clock.clone(),
            ids: ids.clone(),
        });
        ws.open_root(&env.base.join("vault")).unwrap();
        let plan = ws.call(&json!({"schemaVersion": 1,
            "requestId": "req_00000000-0000-4000-8000-000000000311",
            "command": "forget_plan", "idempotencyKey": "synthetic-closing-purge-plan",
            "arguments": {"memoryId": memory, "mode": "purge", "withDependents": false}}));
        validate_response("forget_plan", &plan).unwrap();
        assert_eq!(plan["error"], Value::Null, "{plan}");
        let request = json!({"schemaVersion": 1,
            "requestId": "req_00000000-0000-4000-8000-000000000312",
            "command": "review_confirm", "idempotencyKey": "synthetic-closing-purge-confirm",
            "arguments": {"planId": plan["result"]["planId"], "diffHash": plan["result"]["diffHash"]}});
        ids.armed.store(1, Ordering::SeqCst);
        let response = std::thread::scope(|scope| {
            let confirming = scope.spawn(|| ws.call(&request));
            let paused = ready_rx.recv_timeout(Duration::from_secs(5)).is_ok();
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            let ws_ref = &ws;
            let closing = scope.spawn(move || {
                started_tx.send(()).unwrap();
                let response = if action == "lock" {
                    Some(ws_ref.call(&json!({"schemaVersion": 1,
                        "requestId": "req_00000000-0000-4000-8000-000000000313",
                        "command": "vault_lock", "idempotencyKey": null, "arguments": {}})))
                } else {
                    ws_ref.shutdown();
                    None
                };
                done_tx.send(()).unwrap();
                response
            });
            let started = started_rx.recv_timeout(Duration::from_secs(5)).is_ok();
            let closed_early = done_rx.recv_timeout(Duration::from_millis(150)).is_ok();
            let (status_tx, status_rx) = std::sync::mpsc::channel();
            let ws_ref = &ws;
            let observing = scope.spawn(move || {
                status_tx
                    .send(ws_ref.call(&json!({"schemaVersion": 1,
                    "requestId": "req_00000000-0000-4000-8000-000000000314",
                    "command": "workspace_status", "idempotencyKey": null, "arguments": {}})))
                    .unwrap();
            });
            let status = status_rx.recv_timeout(Duration::from_secs(5)).ok();
            // Release the receipt allocator before assertions, even on a
            // broken lifecycle path or an observation timeout.
            let _ = release_tx.send(());
            let response = confirming.join().unwrap();
            let closed = closing.join().unwrap();
            observing.join().unwrap();
            assert!(
                paused,
                "purge must reach its post-removal receipt allocation"
            );
            assert!(started, "the lifecycle action must have started");
            assert!(!closed_early, "{action} closed before the purge completed");
            let status = status.expect("status bypasses the pending lifecycle action");
            validate_response("workspace_status", &status).unwrap();
            assert_eq!(status["error"], Value::Null, "{status}");
            assert_eq!(status["result"]["vault"]["state"], "open");
            if let Some(closed) = closed {
                validate_response("vault_lock", &closed).unwrap();
                assert_eq!(closed["error"], Value::Null, "{closed}");
            }
            response
        });
        validate_response("review_confirm", &response).unwrap();
        assert_eq!(response["error"], Value::Null, "{response}");
        assert!(
            response["result"]["purge"]["filesRemoved"]
                .as_u64()
                .unwrap()
                > 0
        );
        assert!(response["vaultCommitId"].is_string(), "{response}");
        let closed_retry = ws.call(&request);
        validate_response("review_confirm", &closed_retry).unwrap();
        assert_eq!(closed_retry["result"], Value::Null);
        assert_eq!(
            closed_retry["error"]["rules"][0],
            if action == "lock" {
                "workspace.locked"
            } else {
                "workspace.no_vault"
            }
        );
        ws.open_root(&env.base.join("vault")).unwrap();
        let open = ws.open().unwrap();
        let pin = open.vault.pin_current().unwrap();
        let stale = ws.call(&request);
        validate_response("review_confirm", &stale).unwrap();
        assert_eq!(stale["result"], Value::Null);
        assert_eq!(stale["error"]["rules"][0], "workspace.plan_unknown");
        assert_eq!(open.vault.pin_current().unwrap().commit_id, pin.commit_id);
        let receipts = open
            .vault
            .record_entries(&pin, RecordKind::PurgeReceipt)
            .unwrap();
        assert_eq!(receipts.len(), 1);
        assert_eq!(
            receipts[0].record_id,
            response["result"]["purge"]["receiptId"].as_str().unwrap()
        );
        assert!(open.vault.verify(&pin).unwrap().is_clean());
        ws.shutdown();
    }
}

/// ADR-MEM-46: a rejected root names its reason, and a failed command does
/// not consume the picker token, so the same choice can be retried.
#[test]
fn rejected_roots_name_their_reason_and_keep_the_token() {
    let env = Env::new("root-reason");
    std::fs::create_dir_all(env.base.join("repo/.git")).unwrap();
    std::fs::create_dir_all(env.base.join("repo/vault")).unwrap();
    let token = env.pick(PickKind::VaultRoot, "repo/vault");
    for _ in 0..2 {
        let error = env.err("vault_open", json!({"rootToken": token}));
        assert_eq!(error["code"], "invalid_request");
        assert_eq!(
            error["rules"],
            json!(["workspace.root_rejected", "root.inside_repository"])
        );
    }
}

/// ADR-MEM-46: a retryable confirm failure keeps the plan; the same confirm
/// then succeeds.
#[test]
fn a_busy_confirm_keeps_its_plan_for_the_retry() {
    let env = Env::new("confirm-busy");
    let proposed = env.ok(
        "remember",
        json!({"text": "Synthetic retry fact.", "claimKey": "test.retry"}),
    );
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    let confirm = json!({"planId": plan["planId"], "diffHash": plan["diffHash"]});
    // Another writer holds the Vault lock: the confirm waits, then is busy.
    let writer = std::fs::OpenOptions::new()
        .read(true)
        .write(true)
        .open(env.base.join("vault/vault/LOCK"))
        .unwrap();
    writer.try_lock().unwrap();
    let response = env.send_keyed("review_confirm", confirm.clone(), "retry-key-0000000001");
    assert_eq!(response["error"]["code"], "busy", "{response}");
    assert_eq!(response["error"]["retryable"], true);
    drop(writer);
    let response = env.send_keyed("review_confirm", confirm.clone(), "retry-key-0000000001");
    assert_eq!(response["error"], Value::Null, "{response}");
    assert!(response["result"]["commitId"].is_string());
    let list = env.ok(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(list["total"], 1);
}

/// ADR-MEM-46: an import retried with the same key answers the operation
/// already started; the same key with another request is a conflict.
#[test]
fn an_import_retry_answers_the_started_operation() {
    let env = Env::new("import-key");
    std::fs::write(
        env.base.join("notes.md"),
        "# Synthetic

One synthetic note.
",
    )
    .unwrap();
    let token = env.pick(PickKind::ImportFile, "notes.md");
    let args = json!({"importToken": token, "accountAlias": "acct-main"});
    let first = env.send_keyed("import_start", args.clone(), "import-key-000000001");
    let retry = env.send_keyed("import_start", args, "import-key-000000001");
    assert_eq!(first["operationId"], retry["operationId"], "{retry}");
    assert!(first["operationId"].is_string());
    let other = env.send_keyed(
        "import_start",
        json!({"importToken": token, "accountAlias": "acct-other"}),
        "import-key-000000001",
    );
    assert_eq!(other["error"]["code"], "idempotency_conflict", "{other}");
    let done = env.wait(&first["result"]);
    assert_eq!(done["state"], "succeeded", "{done}");
    // A new submission (new key) cannot reuse the consumed token.
    let again = env.send_keyed(
        "import_start",
        json!({"importToken": token, "accountAlias": "acct-main"}),
        "import-key-000000002",
    );
    assert_eq!(again["error"]["rules"][0], "workspace.token_unknown");
}

/// ADR-MEM-46: one embedded Core per Vault. A second Core is refused while
/// the first has the Vault open, and admitted once it locks.
#[test]
fn a_second_core_cannot_open_an_open_vault() {
    let env = Env::new("host-lock");
    let other = Workspace::new(Config {
        clock: env.clock.clone(),
        ids: Arc::new(SequentialIdSource::new(0xc000)),
    });
    let root = env.base.join("vault");
    let refused = other.open_root(&root).unwrap_err();
    assert_eq!(refused.code, MemoryErrorCode::Busy);
    assert_eq!(refused.rules, ["workspace.vault_in_use"]);
    env.ok("vault_lock", json!({}));
    other.open_root(&root).unwrap();
    let unlock = env.err("vault_unlock", json!({}));
    assert_eq!(unlock["rules"][0], "workspace.vault_in_use");
    let status = env.ok("workspace_status", json!({}));
    assert_eq!(
        status["vault"]["state"], "locked",
        "a refused unlock stays locked"
    );
    other.shutdown();
    let status = env.ok("vault_unlock", json!({}));
    assert_eq!(status["vault"]["state"], "open");
}

/// ADR-MEM-46: a panicking operation ends `failed`, never `running`.
#[test]
fn a_panicking_operation_ends_failed() {
    let env = Env::new("panic");
    let id = OperationId::from_random([5; 16]);
    env.ws
        .operations()
        .spawn(id.clone(), "test_panic", |_| panic!("synthetic panic"));
    let done = env
        .ws
        .operations()
        .wait(&id, Duration::from_secs(10))
        .unwrap();
    assert_eq!(done["state"], "failed", "{done}");
    assert_eq!(done["error"]["rules"], json!(["operation.panicked"]));
    assert_eq!(done["error"]["retryable"], false);
}

/// ADR-MEM-46: unlocking an open Vault answers its status (it once hung on
/// the slot mutex).
#[test]
fn unlock_of_an_open_vault_answers_at_once() {
    let env = Env::new("unlock-open");
    let status = env.ok("vault_unlock", json!({}));
    assert_eq!(status["vault"]["state"], "open");
}

/// ADR-MEM-46: a panic after a cancel request is still a failure.
#[test]
fn a_panic_after_cancel_still_ends_failed() {
    let env = Env::new("panic-cancel");
    let id = OperationId::from_random([6; 16]);
    env.ws
        .operations()
        .spawn(id.clone(), "test_panic", |ticket| {
            while !ticket.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            panic!("synthetic panic after cancel")
        });
    env.ok("operation_cancel", json!({"operationId": id}));
    let done = env
        .ws
        .operations()
        .wait(&id, Duration::from_secs(10))
        .unwrap();
    assert_eq!(done["state"], "failed", "{done}");
}

/// ADR-MEM-46: the host lock belongs to the open/close lifecycle. A handle
/// to the closed Vault still held in this process (an in-flight read) does
/// not make this Core's own unlock or reopen report `vault_in_use`, and the
/// refusal for another process is not a blind retry.
#[test]
fn a_held_handle_does_not_block_this_cores_reopen() {
    let env = Env::new("host-lifecycle");
    let held = env.ws.open().unwrap();
    env.ok("vault_lock", json!({}));
    let status = env.ok("vault_unlock", json!({}));
    assert_eq!(status["vault"]["state"], "open");
    drop(held);
    let other = Workspace::new(Config {
        clock: env.clock.clone(),
        ids: Arc::new(SequentialIdSource::new(0xd000)),
    });
    let refused = other.open_root(&env.base.join("vault")).unwrap_err();
    assert_eq!(refused.rules, ["workspace.vault_in_use"]);
    assert!(!refused.retryable);
}

#[test]
fn a_confirmed_retry_still_checks_the_diff() {
    let env = Env::new("confirm-replay-diff");
    let proposed = env.ok(
        "remember",
        json!({"text": "Synthetic reviewed fact", "claimKey": "synthetic.replay"}),
    );
    let plan = env.ok(
        "review_plan",
        json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
    );
    let args = json!({"planId": plan["planId"], "diffHash": plan["diffHash"]});
    let first = env.ok("review_confirm", args.clone());
    assert_eq!(env.ok("review_confirm", args), first);
    let head = env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
    let wrong = env.err(
        "review_confirm",
        json!({"planId": plan["planId"], "diffHash": "0".repeat(64)}),
    );
    assert_eq!(wrong["rules"], json!(["workspace.diff_hash_mismatch"]));
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["headCommitId"],
        head
    );
}

#[test]
fn closing_a_vault_forgets_confirmed_replies() {
    for action in ["lock", "switch", "failed_open", "shutdown"] {
        let env = Env::new(&format!("confirm-close-{action}"));
        let proposed = env.ok(
            "remember",
            json!({"text": "Synthetic old Vault fact", "claimKey": "synthetic.old_vault"}),
        );
        let plan = env.ok(
            "review_plan",
            json!({"decisions": [{"candidateId": proposed["candidateId"], "revision": proposed["revision"], "action": "accept", "editedContent": null, "mergeTarget": null}]}),
        );
        let args = json!({"planId": plan["planId"], "diffHash": plan["diffHash"]});
        env.ok("review_confirm", args.clone());
        close_for_test(&env, action);
        let gone = env.err("review_confirm", args.clone());
        assert_eq!(
            gone["rules"][0],
            if action == "switch" {
                "workspace.plan_unknown"
            } else if action == "lock" {
                "workspace.locked"
            } else {
                "workspace.no_vault"
            }
        );
        if action == "lock" {
            env.ok("vault_unlock", json!({}));
            assert_eq!(
                env.err("review_confirm", args)["rules"][0],
                "workspace.plan_unknown"
            );
            assert_eq!(
                env.ok(
                    "memory_list",
                    json!({"includeInactive": false, "cursor": null, "limit": null})
                )["total"],
                1
            );
        }
    }
}

fn close_for_test(env: &Env, action: &str) {
    match action {
        "lock" => {
            env.ok("vault_lock", json!({}));
        }
        "switch" => {
            std::fs::create_dir(env.base.join("other-vault")).unwrap();
            let token = env.pick(PickKind::VaultRoot, "other-vault");
            env.ok(
                "vault_create",
                json!({"rootToken": token, "confirmPhrase": "create new vault"}),
            );
        }
        "failed_open" => {
            assert!(env.ws.open_root(&env.base.join("missing")).is_err());
        }
        "shutdown" => env.ws.shutdown(),
        _ => unreachable!(),
    }
}

#[test]
fn closing_a_vault_forgets_operations_backups_and_picks() {
    for action in ["lock", "switch", "failed_open", "shutdown"] {
        let env = Env::new(&format!("operation-close-{action}"));
        std::fs::create_dir(env.base.join("backup")).unwrap();
        let backup_token = env.pick(PickKind::BackupDestination, "backup");
        let started = env.ok("backup_export", json!({"destinationToken": backup_token}));
        assert_eq!(env.wait(&started)["state"], "succeeded");
        let id = started["operationId"].clone();
        assert_eq!(
            env.ok("workspace_status", json!({}))["lastBackup"]["state"],
            "succeeded"
        );
        let export_token = env.pick(PickKind::ExportFolder, "backup");
        let unused_backup_token = env.pick(PickKind::BackupDestination, "backup");
        std::fs::write(env.base.join("notes.md"), "# Synthetic old Vault input\n").unwrap();
        let import_token = env.pick(PickKind::ImportFile, "notes.md");
        close_for_test(&env, action);
        assert_eq!(env.ok("operation_list", json!({}))["items"], json!([]));
        for command in ["operation_get", "operation_cancel"] {
            assert_eq!(
                env.err(command, json!({"operationId": id}))["rules"][0],
                "workspace.operation_unknown"
            );
        }
        assert_eq!(
            env.err("restore_preview", json!({"exportToken": export_token}))["rules"][0],
            "workspace.token_unknown"
        );
        if action == "lock" {
            env.ok("vault_unlock", json!({}));
        }
        if matches!(action, "lock" | "switch") {
            assert_eq!(
                env.ok("workspace_status", json!({}))["lastBackup"],
                Value::Null
            );
            assert_eq!(
                env.err(
                    "backup_export",
                    json!({"destinationToken": unused_backup_token})
                )["rules"][0],
                "workspace.token_unknown"
            );
            assert_eq!(
                env.err("import_preview", json!({"importToken": import_token}))["rules"][0],
                "workspace.token_unknown"
            );
            let fresh = env.pick(PickKind::ImportFile, "notes.md");
            assert_eq!(
                env.ok("import_preview", json!({"importToken": fresh}))["recognized"],
                true
            );
        }
    }
}

#[test]
fn closing_keeps_progress_until_the_worker_has_joined() {
    let env = Env::new("close-worker-join");
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let id = OperationId::from_random([0x73; 16]);
    env.ws.operations().spawn(id.clone(), "test", move |_| {
        ready_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        Ok(json!({"joined": true}))
    });
    ready_rx.recv_timeout(Duration::from_secs(10)).unwrap();
    std::thread::scope(|scope| {
        let closing = scope.spawn(|| env.ws.shutdown());
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let status = env
                .ws
                .operations()
                .status(&id)
                .expect("progress while closing");
            if status["cancelRequested"] == true {
                assert_eq!(status["state"], "running");
                assert_eq!(
                    env.ok("operation_get", json!({"operationId": id}))["state"],
                    "running"
                );
                assert_eq!(
                    env.ok("operation_list", json!({}))["items"]
                        .as_array()
                        .unwrap()
                        .len(),
                    1
                );
                assert_eq!(
                    env.ok("workspace_status", json!({}))["operationsRunning"],
                    1
                );
                assert_eq!(
                    env.ok("operation_cancel", json!({"operationId": id}))["cancelRequested"],
                    true
                );
                break;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "close did not request cancellation"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
        release_tx.send(()).unwrap();
        closing.join().unwrap();
    });
    assert!(env.ws.operations().status(&id).is_none());
    assert_eq!(env.ok("operation_list", json!({}))["items"], json!([]));
}

/// Pause an actual memory-list read after it has acquired the open Vault.
/// The injected Clock is a test port; no production blocking hook is added.
struct PausedClock {
    ready: std::sync::mpsc::Sender<()>,
    release: Mutex<Option<std::sync::mpsc::Receiver<()>>>,
}

impl Clock for PausedClock {
    fn now_unix_ms(&self) -> i64 {
        let release = self.release.lock().unwrap().take();
        if let Some(release) = release {
            self.ready.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        T0 + 5_000
    }
}

#[test]
fn lifecycle_waits_for_an_in_flight_page_call() {
    for command in ["memory_list", "remember"] {
        for action in ["lock", "switch", "failed_open", "shutdown"] {
            let (ready_tx, ready_rx) = std::sync::mpsc::channel();
            let (release_tx, release_rx) = std::sync::mpsc::channel();
            let paused = Arc::new(PausedClock {
                ready: ready_tx,
                release: Mutex::new(None),
            });
            let env = Env::with_clock(
                &format!("lifecycle-page-read-{action}"),
                Some(paused.clone()),
            );
            let before_head =
                env.ok("workspace_status", json!({}))["vault"]["headCommitId"].clone();
            *paused.release.lock().unwrap() = Some(release_rx);
            let arguments = if command == "memory_list" {
                json!({"includeInactive": false, "cursor": null, "limit": null})
            } else {
                json!({"text": "Synthetic held write", "claimKey": "synthetic.lifecycle_write"})
            };
            std::thread::scope(|scope| {
                let reading = scope.spawn(|| env.send(command, arguments));
                ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let (closing_tx, closing_rx) = std::sync::mpsc::channel();
                let (done_tx, done_rx) = std::sync::mpsc::channel();
                let env_ref = &env;
                let closing = scope.spawn(move || {
                    closing_tx.send(()).unwrap();
                    close_for_test(env_ref, action);
                    done_tx.send(()).unwrap();
                });
                closing_rx.recv_timeout(Duration::from_secs(5)).unwrap();
                let closed_early = done_rx.recv_timeout(Duration::from_millis(150)).is_ok();
                let (status_tx, status_rx) = std::sync::mpsc::channel();
                let env_ref = &env;
                let observing = scope.spawn(move || {
                    status_tx
                        .send(env_ref.send("workspace_status", json!({})))
                        .unwrap();
                });
                // This proves lock bypass, not a 500 ms filesystem/ACL latency
                // target. Keep the bound below PausedClock's 10 s release wait.
                let status = status_rx.recv_timeout(Duration::from_secs(5)).ok();
                // Always release before asserting, including on the broken base.
                release_tx.send(()).unwrap();
                let read = reading.join().unwrap();
                closing.join().unwrap();
                observing.join().unwrap();
                assert!(!closed_early, "{action} passed an in-flight Vault read");
                assert_eq!(
                    status.expect("status bypasses a waiting lifecycle writer")["result"]["vault"]
                        ["state"],
                    "open"
                );
                assert_eq!(read["error"], Value::Null, "{read}");
                assert!(
                    read["vaultCommitId"].is_string(),
                    "response pinned before close: {read}"
                );
                if command == "memory_list" {
                    assert_eq!(read["result"]["total"], 0);
                    assert_eq!(read["vaultCommitId"], before_head);
                } else {
                    assert_eq!(read["result"]["state"], "pending");
                    env.ws.open_root(&env.base.join("vault")).unwrap();
                    let candidates =
                        env.ok("candidate_list", json!({"cursor": null, "limit": null}));
                    assert_eq!(candidates["total"], 1);
                    assert_eq!(
                        candidates["items"][0]["candidateId"],
                        read["result"]["candidateId"]
                    );
                }
            });
        }
    }
}

#[test]
fn poisoned_lifecycle_refuses_work_but_allows_observation_and_shutdown() {
    let env = Env::new("lifecycle-poison");
    std::thread::scope(|scope| {
        let panicked = scope.spawn(|| {
            let _guard = env.ws.lifecycle.write().unwrap();
            panic!("synthetic lifecycle failure");
        });
        assert!(panicked.join().is_err());
    });
    let refused = env.err(
        "memory_list",
        json!({"includeInactive": false, "cursor": null, "limit": null}),
    );
    assert_eq!(refused["rules"], json!(["workspace.lifecycle_failed"]));
    assert_eq!(refused["retryable"], false);
    assert_eq!(
        env.ws
            .register_pick(PickKind::VaultRoot, &env.base.join("vault"))
            .unwrap_err()
            .rules,
        ["workspace.lifecycle_failed"]
    );
    assert_eq!(
        env.ws.open_root(&env.base.join("vault")).unwrap_err().rules,
        ["workspace.lifecycle_failed"]
    );
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["state"],
        "open"
    );
    assert_eq!(env.ok("operation_list", json!({}))["items"], json!([]));
    env.ws.shutdown();
    assert_eq!(
        env.ok("workspace_status", json!({}))["vault"]["state"],
        "none"
    );
}

#[test]
fn a_cancel_request_does_not_hide_a_worker_failure() {
    let env = Env::new("cancel-worker-error");
    for code in [
        MemoryErrorCode::StorageFailed,
        MemoryErrorCode::PermissionDenied,
        MemoryErrorCode::Busy,
    ] {
        let id = OperationId::from_random(env.ws.config.ids.random_16());
        let (ready_tx, ready_rx) = std::sync::mpsc::channel();
        let (release_tx, release_rx) = std::sync::mpsc::channel();
        env.ws.operations().spawn(id.clone(), "test", move |_| {
            ready_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
            Err(WorkspaceError::new(code, &["test.worker_failure"]))
        });
        ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        env.ok("operation_cancel", json!({"operationId": id}));
        release_tx.send(()).unwrap();
        let done = env
            .ws
            .operations()
            .wait(&id, Duration::from_secs(5))
            .unwrap();
        assert_eq!(done["state"], "failed", "{done}");
        assert_eq!(done["error"]["code"], json!(code));
        assert_eq!(done["cancelRequested"], true);
    }
}

#[test]
fn cancelling_a_terminal_operation_does_not_change_its_outcome() {
    let env = Env::new("cancel-terminal");
    for kind in ["success", "failure", "cancelled"] {
        let id = OperationId::from_random(env.ws.config.ids.random_16());
        env.ws
            .operations()
            .spawn(id.clone(), "test", move |_| match kind {
                "success" => Ok(json!({"saved": true})),
                "cancelled" => Err(WorkspaceError::new(
                    MemoryErrorCode::Cancelled,
                    &["test.cancelled"],
                )),
                _ => Err(WorkspaceError::new(
                    MemoryErrorCode::StorageFailed,
                    &["test.worker_failure"],
                )),
            });
        let done = env
            .ws
            .operations()
            .wait(&id, Duration::from_secs(5))
            .unwrap();
        assert_eq!(done["cancelRequested"], false);
        assert_eq!(env.ok("operation_cancel", json!({"operationId": id})), done);
    }
}

#[test]
fn a_worker_cancellation_is_reported_as_cancelled() {
    let env = Env::new("worker-cancelled");
    let id = OperationId::from_random(env.ws.config.ids.random_16());
    env.ws.operations().spawn(id.clone(), "test", |_| {
        Err(WorkspaceError::new(
            MemoryErrorCode::Cancelled,
            &["test.cancelled"],
        ))
    });
    let done = env
        .ws
        .operations()
        .wait(&id, Duration::from_secs(5))
        .unwrap();
    assert_eq!(done["state"], "cancelled", "{done}");
    assert_eq!(done["error"]["code"], "cancelled");
}

#[test]
fn completed_work_stays_successful_after_a_cancel_request() {
    let env = Env::new("cancel-successful-work");
    let id = OperationId::from_random(env.ws.config.ids.random_16());
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    env.ws.operations().spawn(id.clone(), "test", move |_| {
        ready_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(10)).unwrap();
        Ok(json!({"saved": true}))
    });
    ready_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    env.ok("operation_cancel", json!({"operationId": id}));
    release_tx.send(()).unwrap();
    let done = env
        .ws
        .operations()
        .wait(&id, Duration::from_secs(5))
        .unwrap();
    assert_eq!(done["state"], "succeeded");
    assert_eq!(done["result"]["saved"], true);
    assert_eq!(done["cancelRequested"], true);
}

#[test]
fn diagnostic_concurrent_assertion_returns_the_published_source() {
    let env = Env::new("diagnostic-assertion-replay");
    with_concurrent_allocations(
        &env,
        |ws| {
            let open = ws.open().unwrap();
            serde_json::to_value(
                ws.assertion(&open, "Synthetic concurrent source", b"diagnostic-same-key")
                    .unwrap(),
            )
            .unwrap()
        },
        |_, first, second| {
            assert_eq!(
                first, second,
                "same key/text must return the original published source"
            );
        },
    );
}

struct HeldIds {
    armed: AtomicU64,
    skip_per_thread: usize,
    calls: Mutex<std::collections::HashMap<std::thread::ThreadId, usize>>,
    ready: std::sync::mpsc::Sender<()>,
    releases: Mutex<Vec<std::sync::mpsc::Receiver<()>>>,
    seq: SequentialIdSource,
}

#[test]
fn concurrent_context_session_input_replays_the_published_id() {
    check_concurrent_context_session_write("input");
}

#[test]
fn concurrent_context_session_fork_replays_the_published_id() {
    check_concurrent_context_session_write("fork");
}

#[test]
fn concurrent_context_session_checkpoint_replays_the_published_id() {
    check_concurrent_context_session_write("checkpoint");
}

fn check_concurrent_context_session_write(kind: &str) {
    let env = Env::new(&format!("session-write-replay-{kind}"));
    let created = env.ok("session_new", json!({}));
    let sid = enouia_memory_contract::ids::SessionId::parse(created["sessionId"].as_str().unwrap())
        .unwrap();
    let bid = enouia_memory_contract::ids::BranchId::parse(created["branchId"].as_str().unwrap())
        .unwrap();
    env.ok(
        "session_ask",
        json!({"sessionId": sid, "branchId": bid, "text": "Synthetic setup turn"}),
    );
    let detail = env.ok("session_detail", json!({"sessionId": sid, "branchId": bid}));
    let at =
        enouia_memory_contract::ids::EventId::parse(detail["lastSavedEventId"].as_str().unwrap())
            .unwrap();
    let request = RequestId::parse("req_00000000-0000-4000-8000-000000000099").unwrap();
    let work = |ws: &Workspace| {
        let open = ws.open().unwrap();
        match kind {
            "input" => serde_json::to_value(
                session::save_input(
                    &open.vault,
                    &open.owner,
                    &sid,
                    &bid,
                    "Synthetic concurrent input",
                    &request,
                    b"synthetic-context-race",
                )
                .unwrap()
                .id,
            )
            .unwrap(),
            "fork" => serde_json::to_value(
                session::fork(
                    &open.vault,
                    &open.owner,
                    &sid,
                    &at,
                    b"synthetic-context-race",
                )
                .unwrap()
                .id,
            )
            .unwrap(),
            _ => serde_json::to_value(
                session::checkpoint(
                    &open.vault,
                    &open.owner,
                    &sid,
                    &bid,
                    &session::CheckpointInput {
                        summary: "Synthetic concurrent checkpoint".into(),
                        decisions: vec![],
                        open_loops: vec![],
                    },
                    b"synthetic-context-race",
                )
                .unwrap()
                .id,
            )
            .unwrap(),
        }
    };
    with_concurrent_allocations(&env, work, |ws, first, second| {
        assert_eq!(first, second, "{kind} must return published IDs");
        let open = ws.open().unwrap();
        let pin = open.vault.pin_current().unwrap();
        match kind {
            "input" | "checkpoint" => {
                let record_kind = if kind == "input" {
                    RecordKind::SessionEvent
                } else {
                    RecordKind::Checkpoint
                };
                assert!(
                    open.vault
                        .record_entry(&pin, record_kind, first.as_str().unwrap())
                        .unwrap()
                        .is_some()
                );
            }
            _ => {
                let stored = session::get(&open.vault, &pin, &sid).unwrap();
                assert_eq!(stored.branches.len(), 2);
                assert!(stored.branches.iter().any(|b| json!(b.branch_id) == first));
            }
        }
        assert_eq!(work(ws), first, "{kind} subsequent replay");
        assert_eq!(open.vault.pin_current().unwrap().commit_id, pin.commit_id);
        if kind == "fork" {
            let later = session::fork(&open.vault, &open.owner, &sid, &at, b"synthetic-later-fork")
                .unwrap();
            assert_ne!(json!(later.id), first);
            assert_eq!(
                work(ws),
                first,
                "replay uses the receipt's historical branch, not the latest fork"
            );
            assert_eq!(
                session::get(&open.vault, &open.vault.pin_current().unwrap(), &sid)
                    .unwrap()
                    .branches
                    .len(),
                3
            );
        }
    });
}
impl IdSource for HeldIds {
    fn random_16(&self) -> [u8; 16] {
        let pause = {
            let mut calls = self.calls.lock().unwrap();
            let count = calls.entry(std::thread::current().id()).or_default();
            *count += 1;
            *count > self.skip_per_thread
        };
        if pause
            && self
                .armed
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                .is_ok()
        {
            let release = self.releases.lock().unwrap().pop().unwrap();
            self.ready.send(()).unwrap();
            release.recv_timeout(Duration::from_secs(10)).unwrap();
        }
        self.seq.random_16()
    }
}

fn with_concurrent_allocations(
    env: &Env,
    work: impl Fn(&Workspace) -> Value + Sync,
    check: impl FnOnce(&Workspace, Value, Value),
) {
    with_concurrent_allocations_at(env, 0, false, |_| {}, work, check);
}

fn with_concurrent_allocations_at(
    env: &Env,
    skip_per_thread: usize,
    ordered_release: bool,
    after_first: impl FnOnce(&Workspace),
    work: impl Fn(&Workspace) -> Value + Sync,
    check: impl FnOnce(&Workspace, Value, Value),
) {
    env.ws.shutdown();
    let (ready_tx, ready_rx) = std::sync::mpsc::channel();
    let (a_tx, a_rx) = std::sync::mpsc::channel();
    let (b_tx, b_rx) = std::sync::mpsc::channel();
    let ids = Arc::new(HeldIds {
        armed: AtomicU64::new(0),
        skip_per_thread,
        calls: Mutex::new(std::collections::HashMap::new()),
        ready: ready_tx,
        releases: Mutex::new(vec![a_rx, b_rx]),
        seq: SequentialIdSource::new(0xd000),
    });
    let ws = Workspace::new(Config {
        clock: env.clock.clone(),
        ids: ids.clone(),
    });
    ws.open_root(&env.base.join("vault")).unwrap();
    ids.armed.store(2, Ordering::SeqCst);
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();
    let values = std::thread::scope(|scope| {
        let run = || {
            let value = work(&ws);
            finished_tx.send(()).unwrap();
            value
        };
        let first = scope.spawn(run);
        let second = scope.spawn(run);
        let both_ready = ready_rx.recv_timeout(Duration::from_secs(5)).is_ok()
            && ready_rx.recv_timeout(Duration::from_secs(5)).is_ok();
        let _ = b_tx.send(());
        let first_finished =
            !ordered_release || finished_rx.recv_timeout(Duration::from_secs(5)).is_ok();
        let between = if ordered_release && first_finished {
            Some(std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || after_first(&ws),
            )))
        } else {
            None
        };
        let _ = a_tx.send(());
        let results = (first.join().unwrap(), second.join().unwrap());
        if let Some(Err(panic)) = between {
            std::panic::resume_unwind(panic);
        }
        assert!(
            both_ready,
            "both calls must pass the initial receipt lookup before publication"
        );
        assert!(
            first_finished,
            "the first call must finish before the second resumes"
        );
        results
    });
    check(&ws, values.0, values.1);
    ws.shutdown();
}

#[test]
fn session_ask_replay_reads_a_receipt_published_after_its_initial_pin() {
    check_session_ask_replay(0, true, false, true);
}

#[test]
fn concurrent_session_ask_replay_uses_the_published_dispatch_and_reply() {
    check_session_ask_replay(1, false, false, true);
}

#[test]
fn session_ask_replay_refuses_evidence_deleted_while_the_call_is_paused() {
    check_session_ask_replay(0, true, true, true);
}

#[test]
fn full_session_ask_replays_an_input_published_while_the_call_is_paused() {
    check_session_ask_replay(0, true, false, false);
}

#[test]
fn full_session_ask_replays_a_concurrent_input_commit() {
    check_session_ask_replay(1, false, false, false);
}

fn call_with_contention_retry(ws: &Workspace, envelope: &Value) -> Value {
    for _ in 0..200 {
        let response = ws.call(envelope);
        let error = &response["error"];
        let contention = error["code"] == "index_not_ready"
            || (error["code"] == "busy" && error["rules"][0] == "fault.writer_busy");
        if !contention || error["retryable"] != true {
            return response;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("synthetic index/writer contention did not release");
}

fn check_session_ask_replay(
    skip: usize,
    ordered: bool,
    delete_during_replay: bool,
    prepared: bool,
) {
    let env = Env::new(&format!("mock-page-replay-{skip}-{prepared}"));
    let created = env.ok("session_new", json!({}));
    let sid = enouia_memory_contract::ids::SessionId::parse(created["sessionId"].as_str().unwrap())
        .unwrap();
    let bid = enouia_memory_contract::ids::BranchId::parse(created["branchId"].as_str().unwrap())
        .unwrap();
    let key = "synthetic-ask-dispatch-race";
    let text = "Synthetic replay question";
    let memory = delete_during_replay.then(|| env.remember(text, "synthetic.replay_deleted"));
    let included = (!prepared).then(|| env.remember(text, "synthetic.full_ask"));
    if prepared {
        let request = request_id_for(key, "session_ask");
        let open = env.ws.open().unwrap();
        session::save_input(
            &open.vault,
            &open.owner,
            &sid,
            &bid,
            text,
            &request,
            format!("ask-input\n{key}").as_bytes(),
        )
        .unwrap();
        env.ws
            .compiled(&open, text, request, Some(&sid), Some(&bid))
            .unwrap();
    }
    let callers = AtomicU64::new(0);
    let envelope = json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000011",
        "command": "session_ask", "idempotencyKey": key,
        "arguments": {"sessionId": sid, "branchId": bid, "text": text}});
    with_concurrent_allocations_at(
        &env,
        skip,
        ordered,
        |ws| {
            if let Some(memory) = &memory {
                let plan = ws.call(&json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000021",
                    "command": "forget_plan", "idempotencyKey": "synthetic-replay-forget",
                    "arguments": {"memoryId": memory, "mode": "forget", "withDependents": false}}));
                assert_eq!(plan["error"], Value::Null, "{plan}");
                let confirmed = ws.call(&json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000022",
                    "command": "review_confirm", "idempotencyKey": "synthetic-replay-forget-confirm",
                    "arguments": {"planId": plan["result"]["planId"], "diffHash": plan["result"]["diffHash"]}}));
                assert_eq!(confirmed["error"], Value::Null, "{confirmed}");
            }
        },
        |ws| {
            let n = callers.fetch_add(1, Ordering::SeqCst) + 0x100;
            let mut own = envelope.clone();
            own["requestId"] = json!(format!("req_00000000-0000-4000-8000-{n:012x}"));
            call_with_contention_retry(ws, &own)
        },
        |ws, first, second| {
            if delete_during_replay {
                let responses = [&first, &second];
                for response in responses {
                    validate_response("session_ask", response).unwrap();
                }
                assert_eq!(
                    responses
                        .iter()
                        .filter(|r| r["error"] == Value::Null)
                        .count(),
                    1
                );
                let refused = responses
                    .iter()
                    .find(|r| r["error"] != Value::Null)
                    .unwrap();
                assert_eq!(refused["kind"], "memory_error");
                assert_eq!(refused["result"], Value::Null);
                assert!(
                    matches!(
                        refused["error"]["rules"][0].as_str(),
                        Some("mock.recompile_required" | "fault.not_found")
                    ),
                    "{refused}"
                );
                return;
            }
            for response in [&first, &second] {
                validate_response("session_ask", response).unwrap();
                assert_eq!(response["error"], Value::Null, "{response}");
            }
            assert_eq!(first["result"], second["result"]);
            if let Some(memory) = &included {
                assert_eq!(first["result"]["memories"][0]["memory_id"], *memory);
                assert_ne!(first["requestId"], second["requestId"]);
            }
            let dispatch = ws.call(
                &json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000012",
            "command": "dispatch_inspect", "idempotencyKey": null,
            "arguments": {"dispatchId": first["result"]["dispatchId"]}}),
            );
            validate_response("dispatch_inspect", &dispatch).unwrap();
            assert_eq!(dispatch["result"]["verified"], true, "{dispatch}");
            let detail = ws.call(&json!({"schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000013",
            "command": "session_detail", "idempotencyKey": null, "arguments": {"sessionId": sid, "branchId": bid}}));
            validate_response("session_detail", &detail).unwrap();
            assert_eq!(detail["result"]["turns"].as_array().unwrap().len(), 1);
            assert_eq!(detail["result"]["turns"][0]["state"], "completed");
            assert_eq!(detail["result"]["transcript"].as_array().unwrap().len(), 2);
            let saved = detail["result"]["transcript"][1]["text"].as_str().unwrap();
            let saved: Value = serde_json::from_str(saved).unwrap();
            assert_eq!(saved["dispatch_id"], first["result"]["dispatchId"]);
            let before = ws.open().unwrap().vault.pin_current().unwrap();
            let mut retry = envelope.clone();
            retry["requestId"] = json!("req_00000000-0000-4000-8000-000000000014");
            let repeated = ws.call(&retry);
            assert_eq!(
                repeated["result"], first["result"],
                "fresh transport request ID replays the same action"
            );
            assert_eq!(
                ws.open().unwrap().vault.pin_current().unwrap().commit_id,
                before.commit_id
            );
            if !prepared {
                retry["arguments"]["text"] = json!("Synthetic changed request text");
                let changed = ws.call(&retry);
                validate_response("session_ask", &changed).unwrap();
                assert_eq!(
                    changed["error"]["code"], "idempotency_conflict",
                    "{changed}"
                );
                assert_eq!(changed["result"], Value::Null);
                assert_eq!(
                    ws.open().unwrap().vault.pin_current().unwrap().commit_id,
                    before.commit_id
                );
            }
        },
    );
}

#[test]
fn concurrent_session_new_replays_the_published_session_and_branch() {
    let env = Env::new("session-new-replay");
    with_concurrent_allocations(
        &env,
        |ws| {
            ws.call(&json!({
        "schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000001",
        "command": "session_new", "idempotencyKey": "synthetic-session-new-race", "arguments": {},
    }))
        },
        |ws, first, second| {
            for response in [&first, &second] {
                validate_response("session_new", response).unwrap();
                assert_eq!(response["error"], Value::Null, "{response}");
            }
            assert_eq!(
                first["result"], second["result"],
                "same-key session creation must return published IDs"
            );
            let listed = ws.call(&json!({
                "schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000002",
                "command": "session_list", "idempotencyKey": null, "arguments": {},
            }));
            assert_eq!(listed["result"]["items"].as_array().unwrap().len(), 1);
            let detail = ws.call(&json!({
            "schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000003",
            "command": "session_detail", "idempotencyKey": null,
            "arguments": {"sessionId": first["result"]["sessionId"], "branchId": first["result"]["branchId"]},
        }));
            validate_response("session_detail", &detail).unwrap();
            assert_eq!(detail["error"], Value::Null, "{detail}");
            let replay = ws.call(&json!({
            "schemaVersion": 1, "requestId": "req_00000000-0000-4000-8000-000000000004",
            "command": "session_new", "idempotencyKey": "synthetic-session-new-race", "arguments": {},
        }));
            assert_eq!(replay["result"], first["result"]);
        },
    );
}

/// A synthetic ChatGPT export: `n` two-message conversations.
fn chatgpt_export(n: usize) -> String {
    let conversations: Vec<Value> = (0..n)
        .map(|i| {
            let (a, b) = (format!("m{i}a"), format!("m{i}b"));
            json!({
                "title": "Synthetic", "create_time": 1_719_900_000.0 + i as f64,
                "update_time": 1_719_900_000.0 + i as f64,
                "conversation_id": format!("conv-{i}"), "id": format!("conv-{i}"), "current_node": b,
                "mapping": {
                    "root": {"id": "root", "message": null, "parent": null, "children": [a]},
                    a.clone(): {"id": a, "parent": "root", "children": [b], "message": {"id": a, "author": {"role": "user", "name": null, "metadata": {}}, "create_time": 1_719_900_000.0 + i as f64, "content": {"content_type": "text", "parts": [format!("Synthetic question {i}.")]}, "metadata": {}, "status": "finished_successfully"}},
                    b.clone(): {"id": b, "parent": a, "children": [], "message": {"id": b, "author": {"role": "assistant", "name": null, "metadata": {}}, "create_time": 1_719_900_001.0 + i as f64, "content": {"content_type": "text", "parts": [format!("Synthetic answer {i}.")]}, "metadata": {}, "status": "finished_successfully"}},
                },
            })
        })
        .collect();
    serde_json::to_string(&conversations).unwrap()
}

/// ADR-MEM-46: an import cancelled before its first batch stays `archived`;
/// its operation still reports `cancelled` and resumable, and resume
/// completes it.
#[test]
fn an_import_cancelled_before_its_first_batch_is_resumable() {
    struct Cancelled;
    impl enouia_memory_contract::foundation::Cancellation for Cancelled {
        fn is_cancelled(&self) -> bool {
            true
        }
    }
    let env = Env::new("cancel-early");
    let file = env.base.join("conversations.json");
    std::fs::write(&file, chatgpt_export(120)).unwrap();
    let open = env.ws.open().unwrap();
    let options = env.ws.import_options(&open, "acct-main").unwrap();
    let report =
        enouia_memory_import::import_file(&open.vault, &file, &options, &Cancelled).unwrap();
    assert_eq!(
        report.manifest.status,
        enouia_memory_contract::import::ImportStatus::Archived
    );
    let id = OperationId::from_random([8; 16]);
    let manifest = report.manifest.clone();
    env.ws
        .operations()
        .spawn(id.clone(), "import", move |ticket| {
            while !ticket.cancelled() {
                std::thread::sleep(Duration::from_millis(2));
            }
            Ok(import_outcome(&manifest, ticket))
        });
    env.ok("operation_cancel", json!({"operationId": id}));
    let done = env
        .ws
        .operations()
        .wait(&id, Duration::from_secs(10))
        .unwrap();
    assert_eq!(done["state"], "cancelled", "{done}");
    assert_eq!(done["result"]["resumable"], true);
    let resumed = env.wait(&env.ok(
        "import_resume",
        json!({"importId": report.import_id, "accountAlias": "acct-main"}),
    ));
    assert_eq!(resumed["state"], "succeeded", "{resumed}");
    assert_eq!(resumed["result"]["status"], "completed", "{resumed}");
}

mod native;
